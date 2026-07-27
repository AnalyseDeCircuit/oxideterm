// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_environment::{
    GitProbeError, GitProbeKey, GitProbeOutcome, GitProbeScope, GitRepositorySnapshot,
    GitStatusStore, ProjectProbeError, ProjectProbeKey, ProjectProbeOutcome, ProjectProbeScope,
    ProjectSnapshot, ProjectStatusStore, parse_remote_shell_project_probe_output,
    parse_shell_probe_output, probe_local_project, remote_shell_probe_command,
    remote_shell_project_probe_command,
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const TERMINAL_PROJECT_PROBE_TTL_MS: u64 = 5_000;
const TERMINAL_PROJECT_REMOTE_TIMEOUT: Duration = Duration::from_secs(3);
const TERMINAL_PROJECT_REMOTE_MAX_OUTPUT: usize = 512 * 1024;

/// Moves a bounded notice batch into the window toast adapter exactly once.
#[derive(Clone)]
pub(in crate::workspace) struct TerminalNoticeBatchRequest {
    notices: Arc<Mutex<Option<Vec<TerminalNotice>>>>,
}

impl TerminalNoticeBatchRequest {
    fn new(notices: Vec<TerminalNotice>) -> Self {
        Self {
            notices: Arc::new(Mutex::new(Some(notices))),
        }
    }

    pub(in crate::workspace) fn take(&self) -> Option<Vec<TerminalNotice>> {
        self.notices.lock().ok()?.take()
    }
}

#[derive(Clone)]
pub(in crate::workspace) enum WorkspaceTerminalEvent {
    NoticesReady(TerminalNoticeBatchRequest),
    GitMetadataChanged,
    ProjectMetadataChanged,
}

enum TerminalGitProbeDelivery {
    Probe {
        key: GitProbeKey,
        generation: u64,
        outcome: GitProbeOutcome,
    },
}

#[derive(Default)]
/// Keeps broadcast selection semantics together so stale targets cannot widen a command.
struct TerminalBroadcastState {
    enabled: bool,
    targets: HashSet<PaneId>,
    menu_open: bool,
}

/// Owns terminal-wide delivery channels and their foreground cancellation lifecycle.
pub(in crate::workspace) struct WorkspaceTerminalEntity {
    notice_tx: delivery::ActiveDeliverySender<TerminalNotice>,
    notice_rx: std::sync::mpsc::Receiver<TerminalNotice>,
    git_tx: delivery::ActiveDeliverySender<TerminalGitProbeDelivery>,
    git_rx: std::sync::mpsc::Receiver<TerminalGitProbeDelivery>,
    git_store: GitStatusStore,
    project_tx: delivery::ActiveDeliverySender<terminal_project::TerminalProjectDelivery>,
    project_rx: std::sync::mpsc::Receiver<terminal_project::TerminalProjectDelivery>,
    project_store: ProjectStatusStore,
    project_tasks_enabled: bool,
    broadcast: TerminalBroadcastState,
    node_router: NodeRouter,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl WorkspaceTerminalEntity {
    pub(in crate::workspace) fn new(
        runtime: Arc<tokio::runtime::Runtime>,
        node_router: NodeRouter,
        cx: &mut Context<Self>,
    ) -> Self {
        let delivery_wake = delivery::ActiveDeliveryWake::default();
        let (notice_tx, notice_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(delivery_wake.clone());
        let (git_tx, git_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(delivery_wake.clone());
        let (project_tx, project_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(delivery_wake.clone());
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // External sinks may outlive the UI owner, so release must stop
            // the foreground waiter independently of sender lifetime.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |terminal, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if !should_drain {
                    if stopped {
                        break;
                    }
                    continue;
                }
                let backlog_remaining = terminal
                    .update(cx, |terminal, cx| terminal.drain_deliveries(cx))
                    .unwrap_or(false);
                if backlog_remaining {
                    // Preserve bounded batches while guaranteeing eventual delivery.
                    delivery_wake.mark();
                } else if stopped {
                    break;
                }
            }
        })
        .detach();

        Self {
            notice_tx,
            notice_rx,
            git_tx,
            git_rx,
            git_store: GitStatusStore::default(),
            project_tx,
            project_rx,
            project_store: ProjectStatusStore::default(),
            project_tasks_enabled: false,
            broadcast: TerminalBroadcastState::default(),
            node_router,
            runtime,
        }
    }

    pub(in crate::workspace) fn notice_sender(
        &self,
    ) -> delivery::ActiveDeliverySender<TerminalNotice> {
        // The root keeps one producer capability for legacy surface adapters;
        // receiver state and foreground delivery remain Entity-owned.
        self.notice_tx.clone()
    }

    pub(in crate::workspace) fn broadcast_enabled(&self) -> bool {
        self.broadcast.enabled
    }

    pub(in crate::workspace) fn broadcast_menu_open(&self) -> bool {
        self.broadcast.menu_open
    }

    pub(in crate::workspace) fn broadcast_targets_empty(&self) -> bool {
        self.broadcast.targets.is_empty()
    }

    pub(in crate::workspace) fn broadcast_target_selected(&self, pane_id: PaneId) -> bool {
        self.broadcast.targets.contains(&pane_id)
    }

    pub(in crate::workspace) fn toggle_broadcast(&mut self) {
        self.broadcast.enabled = !self.broadcast.enabled;
        self.broadcast.menu_open = false;
        if !self.broadcast.enabled {
            self.broadcast.targets.clear();
        }
    }

    pub(in crate::workspace) fn dismiss_broadcast_menu(&mut self) -> bool {
        let was_open = self.broadcast.menu_open;
        self.broadcast.menu_open = false;
        was_open
    }

    pub(in crate::workspace) fn set_broadcast_menu_open(&mut self, open: bool) {
        self.broadcast.menu_open = open;
    }

    pub(in crate::workspace) fn toggle_broadcast_target(&mut self, pane_id: PaneId) {
        if !self.broadcast.targets.remove(&pane_id) {
            self.broadcast.targets.insert(pane_id);
        }
        self.broadcast.enabled = !self.broadcast.targets.is_empty();
        self.broadcast.menu_open = true;
    }

    pub(in crate::workspace) fn set_broadcast_targets(&mut self, targets: &[PaneId]) {
        self.broadcast.targets.clear();
        self.broadcast.targets.extend(targets.iter().copied());
        self.broadcast.enabled = !self.broadcast.targets.is_empty();
        self.broadcast.menu_open = true;
    }

    pub(in crate::workspace) fn retain_live_broadcast_targets(
        &mut self,
        live_panes: &HashSet<PaneId>,
    ) {
        self.broadcast
            .targets
            .retain(|pane_id| live_panes.contains(pane_id));
        if self.broadcast.targets.is_empty() {
            // An explicitly empty selection means "all" only while enabled.
            // Disable after pruning so closed targets never widen the command.
            self.broadcast.enabled = false;
        }
    }

    pub(in crate::workspace) fn filter_broadcast_targets(
        &self,
        candidates: Vec<PaneId>,
    ) -> Vec<PaneId> {
        if self.broadcast.targets.is_empty() {
            candidates
        } else {
            candidates
                .into_iter()
                .filter(|pane_id| self.broadcast.targets.contains(pane_id))
                .collect()
        }
    }

    pub(in crate::workspace) fn project_snapshot(
        &self,
        key: &ProjectProbeKey,
    ) -> Option<ProjectSnapshot> {
        self.project_store.snapshot(key).cloned()
    }

    pub(in crate::workspace) fn git_snapshot(
        &self,
        key: &GitProbeKey,
    ) -> Option<GitRepositorySnapshot> {
        self.git_store.snapshot(key).cloned()
    }

    pub(in crate::workspace) fn maybe_refresh_git(
        &mut self,
        key: GitProbeKey,
        cx: &mut Context<Self>,
    ) {
        let now_ms = terminal_git::terminal_git_now_ms();
        if !self
            .git_store
            .should_probe(&key, now_ms, terminal_git::TERMINAL_GIT_PROBE_TTL_MS)
        {
            return;
        }

        let generation = self.git_store.mark_loading(key.clone(), now_ms);
        let remote_node_id = match key.scope() {
            GitProbeScope::Local => None,
            GitProbeScope::SshNode(node_id) => Some(NodeId::new(node_id.clone())),
        };
        if let Some(node_id) = remote_node_id {
            self.spawn_remote_git_probe(key, generation, node_id, cx);
        } else {
            self.spawn_local_git_probe(key, generation);
        }
    }

    pub(in crate::workspace) fn set_project_tasks_enabled(
        &mut self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.project_tasks_enabled == enabled {
            return;
        }
        self.project_tasks_enabled = enabled;
        if !enabled {
            // Disabling invalidates in-flight generations so late completions
            // cannot leave a permanently loading cache entry.
            self.project_store.retain_keys(|_| false);
            cx.emit(WorkspaceTerminalEvent::ProjectMetadataChanged);
        }
    }

    pub(in crate::workspace) fn maybe_refresh_project(
        &mut self,
        key: ProjectProbeKey,
        cx: &mut Context<Self>,
    ) {
        if !self.project_tasks_enabled {
            return;
        }
        let now_ms = terminal_project_now_ms();
        if !self
            .project_store
            .should_probe(&key, now_ms, TERMINAL_PROJECT_PROBE_TTL_MS)
        {
            return;
        }

        let generation = self.project_store.mark_loading(key.clone(), now_ms);
        let remote_node_id = match key.scope() {
            ProjectProbeScope::Local => None,
            ProjectProbeScope::SshNode(node_id) => Some(NodeId::new(node_id.clone())),
        };
        if let Some(node_id) = remote_node_id {
            self.spawn_remote_project_probe(key, generation, node_id, cx);
        } else {
            self.spawn_local_project_probe(key, generation);
        }
    }

    fn drain_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        self.drain_notices(cx) | self.drain_git_results(cx) | self.drain_project_results(cx)
    }

    fn drain_notices(&mut self, cx: &mut Context<Self>) -> bool {
        let delivery_batch =
            delivery::drain_channel(&self.notice_rx, delivery::NOTIFICATION_DELIVERY_BUDGET);
        if !delivery_batch.items.is_empty() {
            cx.emit(WorkspaceTerminalEvent::NoticesReady(
                TerminalNoticeBatchRequest::new(delivery_batch.items),
            ));
        }
        delivery_batch.outcome.backlog_remaining
    }

    fn drain_project_results(&mut self, cx: &mut Context<Self>) -> bool {
        let delivery_batch =
            delivery::drain_channel(&self.project_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        if !self.project_tasks_enabled {
            // Results from a disabled project surface are intentionally discarded.
            return delivery_batch.outcome.backlog_remaining;
        }

        let mut changed = false;
        for delivery in delivery_batch.items {
            match delivery {
                terminal_project::TerminalProjectDelivery::Probe {
                    key,
                    generation,
                    outcome,
                } => {
                    changed |= self.project_store.finish_probe(
                        &key,
                        generation,
                        outcome,
                        terminal_project_now_ms(),
                    );
                }
            }
        }
        if changed {
            cx.emit(WorkspaceTerminalEvent::ProjectMetadataChanged);
        }
        delivery_batch.outcome.backlog_remaining
    }

    fn drain_git_results(&mut self, cx: &mut Context<Self>) -> bool {
        let delivery_batch =
            delivery::drain_channel(&self.git_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        let mut changed = false;
        for delivery in delivery_batch.items {
            match delivery {
                TerminalGitProbeDelivery::Probe {
                    key,
                    generation,
                    outcome,
                } => {
                    changed |= self.git_store.finish_probe(
                        &key,
                        generation,
                        outcome,
                        terminal_git::terminal_git_now_ms(),
                    );
                }
            }
        }
        if changed {
            cx.emit(WorkspaceTerminalEvent::GitMetadataChanged);
        }
        delivery_batch.outcome.backlog_remaining
    }

    fn spawn_local_git_probe(&self, key: GitProbeKey, generation: u64) {
        let git_tx = self.git_tx.clone();
        let cwd = key.cwd().to_string();
        self.runtime.spawn(async move {
            let outcome = terminal_git::run_local_git_probe(&cwd).await;
            let _ = git_tx.send(TerminalGitProbeDelivery::Probe {
                key,
                generation,
                outcome,
            });
        });
    }

    fn spawn_remote_git_probe(
        &mut self,
        key: GitProbeKey,
        generation: u64,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let handle = match self.node_router.resolve_connection_now(&node_id) {
            Ok(resolved) => resolved.handle,
            Err(_) => {
                if self.git_store.finish_probe(
                    &key,
                    generation,
                    GitProbeOutcome::Error(GitProbeError::new(
                        "ssh node is not ready for git probing",
                    )),
                    terminal_git::terminal_git_now_ms(),
                ) {
                    cx.emit(WorkspaceTerminalEvent::GitMetadataChanged);
                }
                return;
            }
        };

        let git_tx = self.git_tx.clone();
        let command = remote_shell_probe_command(key.cwd());
        self.runtime.spawn(async move {
            let outcome = match handle
                .run_command_capture(
                    &command,
                    terminal_git::TERMINAL_GIT_PROBE_TIMEOUT,
                    terminal_git::TERMINAL_GIT_REMOTE_MAX_OUTPUT,
                )
                .await
            {
                Ok(output) => parse_shell_probe_output(&output.stdout),
                Err(_) => GitProbeOutcome::Error(GitProbeError::new("ssh git probe failed")),
            };
            let _ = git_tx.send(TerminalGitProbeDelivery::Probe {
                key,
                generation,
                outcome,
            });
        });
    }

    fn spawn_local_project_probe(&self, key: ProjectProbeKey, generation: u64) {
        let project_tx = self.project_tx.clone();
        let cwd = key.cwd().to_string();
        self.runtime.spawn(async move {
            let outcome = probe_local_project(&cwd);
            let _ = project_tx.send(terminal_project::TerminalProjectDelivery::Probe {
                key,
                generation,
                outcome,
            });
        });
    }

    fn spawn_remote_project_probe(
        &mut self,
        key: ProjectProbeKey,
        generation: u64,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let handle = match self.node_router.resolve_connection_now(&node_id) {
            Ok(resolved) => resolved.handle,
            Err(_) => {
                if self.project_store.finish_probe(
                    &key,
                    generation,
                    ProjectProbeOutcome::Error(ProjectProbeError::new(
                        "ssh node is not ready for project probing",
                    )),
                    terminal_project_now_ms(),
                ) {
                    cx.emit(WorkspaceTerminalEvent::ProjectMetadataChanged);
                }
                return;
            }
        };

        let project_tx = self.project_tx.clone();
        let command = remote_shell_project_probe_command(key.cwd());
        self.runtime.spawn(async move {
            let outcome = match handle
                .run_command_capture(
                    &command,
                    TERMINAL_PROJECT_REMOTE_TIMEOUT,
                    TERMINAL_PROJECT_REMOTE_MAX_OUTPUT,
                )
                .await
            {
                Ok(output) => parse_remote_shell_project_probe_output(&output.stdout),
                Err(_) => {
                    ProjectProbeOutcome::Error(ProjectProbeError::new("ssh project probe failed"))
                }
            };
            let _ = project_tx.send(terminal_project::TerminalProjectDelivery::Probe {
                key,
                generation,
                outcome,
            });
        });
    }
}

impl gpui::EventEmitter<WorkspaceTerminalEvent> for WorkspaceTerminalEntity {}

fn terminal_project_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    struct TerminalEventRecorder {
        notices: Vec<TerminalNoticeBatchRequest>,
        git_metadata_changes: usize,
        project_metadata_changes: usize,
        _subscription: Subscription,
    }

    fn new_terminal_entity(cx: &mut TestAppContext) -> Entity<WorkspaceTerminalEntity> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("create test runtime"),
        );
        let registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(registry);
        cx.new(|cx| WorkspaceTerminalEntity::new(runtime, node_router, cx))
    }

    #[gpui::test]
    fn broadcast_state_transitions_are_entity_owned(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        terminal.update(cx, |terminal, _cx| {
            terminal.set_broadcast_menu_open(true);
            terminal.toggle_broadcast_target(PaneId(1));
        });

        terminal.read_with(cx, |terminal, _cx| {
            assert!(terminal.broadcast_enabled());
            assert!(terminal.broadcast_menu_open());
            assert!(terminal.broadcast_target_selected(PaneId(1)));
        });

        terminal.update(cx, |terminal, _cx| {
            terminal.toggle_broadcast_target(PaneId(1));
        });
        terminal.read_with(cx, |terminal, _cx| {
            assert!(!terminal.broadcast_enabled());
            assert!(terminal.broadcast_menu_open());
            assert!(terminal.broadcast_targets_empty());
        });

        terminal.update(cx, |terminal, _cx| terminal.toggle_broadcast());
        terminal.read_with(cx, |terminal, _cx| {
            assert!(terminal.broadcast_enabled());
            assert!(!terminal.broadcast_menu_open());
        });

        terminal.update(cx, |terminal, _cx| terminal.toggle_broadcast());
        terminal.read_with(cx, |terminal, _cx| {
            assert!(!terminal.broadcast_enabled());
            assert!(terminal.broadcast_targets_empty());
        });
    }

    #[gpui::test]
    fn broadcast_target_filter_and_pruning_are_entity_owned(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        terminal.update(cx, |terminal, _cx| {
            terminal.set_broadcast_targets(&[PaneId(1), PaneId(2)]);
        });

        let filtered = terminal.read_with(cx, |terminal, _cx| {
            terminal.filter_broadcast_targets(vec![PaneId(1), PaneId(2), PaneId(3)])
        });
        assert_eq!(filtered, vec![PaneId(1), PaneId(2)]);

        terminal.update(cx, |terminal, _cx| {
            terminal.retain_live_broadcast_targets(&HashSet::from([PaneId(2), PaneId(3)]));
        });
        terminal.read_with(cx, |terminal, _cx| {
            assert!(terminal.broadcast_enabled());
            assert!(!terminal.broadcast_target_selected(PaneId(1)));
            assert!(terminal.broadcast_target_selected(PaneId(2)));
        });

        terminal.update(cx, |terminal, _cx| {
            terminal.retain_live_broadcast_targets(&HashSet::from([PaneId(3)]));
        });
        terminal.read_with(cx, |terminal, _cx| {
            assert!(!terminal.broadcast_enabled());
            assert!(terminal.broadcast_targets_empty());
        });
    }

    #[gpui::test]
    fn notice_delivery_is_entity_owned_and_payload_is_consumed_once(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        let recorder = cx.new(|cx| {
            let subscription = cx.subscribe(
                &terminal,
                |recorder: &mut TerminalEventRecorder, _terminal, event, _cx| match event {
                    WorkspaceTerminalEvent::NoticesReady(request) => {
                        recorder.notices.push(request.clone());
                    }
                    WorkspaceTerminalEvent::GitMetadataChanged => {
                        recorder.git_metadata_changes += 1;
                    }
                    WorkspaceTerminalEvent::ProjectMetadataChanged => {
                        recorder.project_metadata_changes += 1;
                    }
                },
            );
            TerminalEventRecorder {
                notices: Vec::new(),
                git_metadata_changes: 0,
                project_metadata_changes: 0,
                _subscription: subscription,
            }
        });
        let sender = terminal.read_with(cx, |terminal, _cx| terminal.notice_sender());

        sender
            .send(TerminalNotice {
                title: "ready".to_string(),
                description: Some("description".to_string()),
                status_text: None,
                progress: None,
                variant: TerminalNoticeVariant::Success,
            })
            .expect("notice send");
        cx.run_until_parked();

        let request = recorder.read_with(cx, |recorder, _cx| recorder.notices[0].clone());
        let notices = request.take().expect("notice payload");
        let notice = &notices[0];
        assert_eq!(notice.title, "ready");
        assert_eq!(notice.description.as_deref(), Some("description"));
        assert!(request.take().is_none());
    }

    #[gpui::test]
    fn entity_release_stops_notice_delivery_waiter(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        let notice_wake = terminal.read_with(cx, |terminal, _cx| terminal.notice_sender().wake());

        drop(terminal);
        cx.update(|_cx| {});
        cx.run_until_parked();

        assert!(notice_wake.is_stopped());
    }

    #[gpui::test]
    fn project_probe_state_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        let recorder = cx.new(|cx| {
            let subscription = cx.subscribe(
                &terminal,
                |recorder: &mut TerminalEventRecorder, _terminal, event, _cx| match event {
                    WorkspaceTerminalEvent::NoticesReady(request) => {
                        recorder.notices.push(request.clone());
                    }
                    WorkspaceTerminalEvent::GitMetadataChanged => {
                        recorder.git_metadata_changes += 1;
                    }
                    WorkspaceTerminalEvent::ProjectMetadataChanged => {
                        recorder.project_metadata_changes += 1;
                    }
                },
            );
            TerminalEventRecorder {
                notices: Vec::new(),
                git_metadata_changes: 0,
                project_metadata_changes: 0,
                _subscription: subscription,
            }
        });
        let key = ProjectProbeKey::new(ProjectProbeScope::Local, "/missing-project")
            .expect("project probe key");
        let (generation, sender) = terminal.update(cx, |terminal, cx| {
            terminal.set_project_tasks_enabled(true, cx);
            let generation = terminal
                .project_store
                .mark_loading(key.clone(), terminal_project_now_ms());
            (generation, terminal.project_tx.clone())
        });

        sender
            .send(terminal_project::TerminalProjectDelivery::Probe {
                key: key.clone(),
                generation,
                outcome: ProjectProbeOutcome::NoProject,
            })
            .expect("project delivery");
        cx.run_until_parked();

        let state = terminal.read_with(cx, |terminal, _cx| {
            terminal
                .project_store
                .get(&key)
                .map(|entry| entry.state().clone())
        });
        assert_eq!(
            state,
            Some(oxideterm_environment::ProjectProbeState::NoProject)
        );
        assert_eq!(
            recorder.read_with(cx, |recorder, _cx| recorder.project_metadata_changes),
            1
        );
    }

    #[gpui::test]
    fn git_probe_state_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        let recorder = cx.new(|cx| {
            let subscription = cx.subscribe(
                &terminal,
                |recorder: &mut TerminalEventRecorder, _terminal, event, _cx| match event {
                    WorkspaceTerminalEvent::NoticesReady(request) => {
                        recorder.notices.push(request.clone());
                    }
                    WorkspaceTerminalEvent::GitMetadataChanged => {
                        recorder.git_metadata_changes += 1;
                    }
                    WorkspaceTerminalEvent::ProjectMetadataChanged => {
                        recorder.project_metadata_changes += 1;
                    }
                },
            );
            TerminalEventRecorder {
                notices: Vec::new(),
                git_metadata_changes: 0,
                project_metadata_changes: 0,
                _subscription: subscription,
            }
        });
        let key =
            GitProbeKey::new(GitProbeScope::Local, "/missing-repository").expect("git probe key");
        let (generation, sender) = terminal.update(cx, |terminal, _cx| {
            let generation = terminal
                .git_store
                .mark_loading(key.clone(), terminal_git::terminal_git_now_ms());
            (generation, terminal.git_tx.clone())
        });

        sender
            .send(TerminalGitProbeDelivery::Probe {
                key: key.clone(),
                generation,
                outcome: GitProbeOutcome::GitUnavailable,
            })
            .expect("git delivery");
        cx.run_until_parked();

        let state = terminal.read_with(cx, |terminal, _cx| {
            terminal
                .git_store
                .get(&key)
                .map(|entry| entry.state().clone())
        });
        assert_eq!(
            state,
            Some(oxideterm_environment::GitProbeState::GitUnavailable)
        );
        assert_eq!(
            recorder.read_with(cx, |recorder, _cx| recorder.git_metadata_changes),
            1
        );
    }

    #[gpui::test]
    fn disabling_project_tasks_discards_late_probe_results(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        let key = ProjectProbeKey::new(ProjectProbeScope::Local, "/disabled-project")
            .expect("project probe key");
        let (generation, sender) = terminal.update(cx, |terminal, cx| {
            terminal.set_project_tasks_enabled(true, cx);
            let generation = terminal
                .project_store
                .mark_loading(key.clone(), terminal_project_now_ms());
            let sender = terminal.project_tx.clone();
            terminal.set_project_tasks_enabled(false, cx);
            (generation, sender)
        });

        sender
            .send(terminal_project::TerminalProjectDelivery::Probe {
                key: key.clone(),
                generation,
                outcome: ProjectProbeOutcome::NoProject,
            })
            .expect("project delivery");
        cx.run_until_parked();

        assert!(terminal.read_with(cx, |terminal, _cx| {
            terminal.project_store.get(&key).is_none()
        }));
    }

    #[gpui::test]
    fn local_project_probe_worker_completes_through_entity_delivery(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        let key = ProjectProbeKey::new(ProjectProbeScope::Local, env!("CARGO_MANIFEST_DIR"))
            .expect("project probe key");
        terminal.update(cx, |terminal, cx| {
            terminal.set_project_tasks_enabled(true, cx);
            terminal.maybe_refresh_project(key.clone(), cx);
        });

        // Drive the Entity-owned test runtime on the GPUI test thread so the
        // scheduler observes the same deterministic wake boundary as production.
        let runtime = terminal.read_with(cx, |terminal, _cx| terminal.runtime.clone());
        runtime.block_on(async {
            tokio::task::yield_now().await;
        });
        cx.run_until_parked();

        assert!(terminal.read_with(cx, |terminal, _cx| {
            terminal.project_store.snapshot(&key).is_some()
        }));
    }
}
