// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::Task;
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_environment::{
    GitProbeError, GitProbeKey, GitProbeOutcome, GitProbeScope, GitRepositorySnapshot,
    GitStatusStore, ProjectProbeError, ProjectProbeKey, ProjectProbeOutcome, ProjectProbeScope,
    ProjectSnapshot, ProjectStatusStore, ProjectTask, current_directory_cd_command,
    parse_remote_shell_project_probe_output, parse_shell_probe_output, probe_local_project,
    remote_shell_probe_command, remote_shell_project_probe_command,
};
use std::{
    ops::Range,
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const TERMINAL_PROJECT_PROBE_TTL_MS: u64 = 5_000;
const TERMINAL_PROJECT_REMOTE_TIMEOUT: Duration = Duration::from_secs(3);
const TERMINAL_PROJECT_REMOTE_MAX_OUTPUT: usize = 512 * 1024;

#[derive(Clone)]
pub(in crate::workspace) enum WorkspaceTerminalEvent {
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
    // Named groups resolve to a runtime snapshot so membership never owns connection startup.
    targets: HashSet<PaneId>,
    selected_group_id: Option<uuid::Uuid>,
    group_editor: Option<TerminalBroadcastGroupEditor>,
    menu_open: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TerminalBroadcastGroupEditKind {
    Create,
    Rename(uuid::Uuid),
}

struct TerminalBroadcastGroupEditor {
    kind: TerminalBroadcastGroupEditKind,
    value: String,
}

/// Owns terminal-wide delivery channels and their foreground cancellation lifecycle.
pub(in crate::workspace) struct WorkspaceTerminalEntity {
    git_tx: delivery::ActiveDeliverySender<TerminalGitProbeDelivery>,
    git_rx: std::sync::mpsc::Receiver<TerminalGitProbeDelivery>,
    git_store: GitStatusStore,
    active_git_probe_key: Option<GitProbeKey>,
    git_probe_schedule_generation: u64,
    git_probe_schedule_task: Option<Task<()>>,
    pub(super) git_action_tx: delivery::ActiveDeliverySender<terminal_git::TerminalGitDelivery>,
    pub(super) git_action_rx: std::sync::mpsc::Receiver<terminal_git::TerminalGitDelivery>,
    pub(super) git_panel: terminal_git::TerminalGitBranchPickerState,
    project_tx: delivery::ActiveDeliverySender<terminal_project::TerminalProjectDelivery>,
    project_rx: std::sync::mpsc::Receiver<terminal_project::TerminalProjectDelivery>,
    project_store: ProjectStatusStore,
    active_project_probe_key: Option<ProjectProbeKey>,
    project_probe_schedule_generation: u64,
    project_probe_schedule_task: Option<Task<()>>,
    project_tasks_enabled: bool,
    project_panel: terminal_project::TerminalProjectPanelState,
    pub(super) cwd_tx: delivery::ActiveDeliverySender<terminal_cwd::TerminalCwdDelivery>,
    pub(super) cwd_rx: std::sync::mpsc::Receiver<terminal_cwd::TerminalCwdDelivery>,
    pub(super) cwd_picker: terminal_cwd::TerminalCwdPickerState,
    pub(super) cast_player: Option<terminal_cast::TerminalCastPlayerState>,
    pub(super) cast_seek_dragging: bool,
    pub(super) cast_tick_generation: u64,
    pub(super) cast_tick_scheduled: bool,
    pub(super) cast_tick_task: Option<Task<()>>,
    pub(super) quick_commands: quick_commands::TerminalQuickCommandsState,
    broadcast: TerminalBroadcastState,
    pub(super) node_router: NodeRouter,
    pub(super) runtime: Arc<tokio::runtime::Runtime>,
}

impl WorkspaceTerminalEntity {
    pub(in crate::workspace) fn new(
        runtime: Arc<tokio::runtime::Runtime>,
        node_router: NodeRouter,
        settings_path: &Path,
        cx: &mut Context<Self>,
    ) -> Self {
        let delivery_wake = delivery::ActiveDeliveryWake::default();
        let (git_tx, git_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(delivery_wake.clone());
        let (git_action_tx, git_action_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(delivery_wake.clone());
        let (project_tx, project_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(delivery_wake.clone());
        let (cwd_tx, cwd_rx) =
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
            git_tx,
            git_rx,
            git_store: GitStatusStore::default(),
            active_git_probe_key: None,
            git_probe_schedule_generation: 0,
            git_probe_schedule_task: None,
            git_action_tx,
            git_action_rx,
            git_panel: terminal_git::TerminalGitBranchPickerState::default(),
            project_tx,
            project_rx,
            project_store: ProjectStatusStore::default(),
            active_project_probe_key: None,
            project_probe_schedule_generation: 0,
            project_probe_schedule_task: None,
            project_tasks_enabled: false,
            project_panel: terminal_project::TerminalProjectPanelState::default(),
            cwd_tx,
            cwd_rx,
            cwd_picker: terminal_cwd::TerminalCwdPickerState::default(),
            cast_player: None,
            cast_seek_dragging: false,
            cast_tick_generation: 0,
            cast_tick_scheduled: false,
            cast_tick_task: None,
            quick_commands: quick_commands::TerminalQuickCommandsState::load(settings_path),
            broadcast: TerminalBroadcastState::default(),
            node_router,
            runtime,
        }
    }

    pub(in crate::workspace) fn broadcast_enabled(&self) -> bool {
        self.broadcast.enabled
    }

    pub(in crate::workspace) fn broadcast_menu_open(&self) -> bool {
        self.broadcast.menu_open
    }

    pub(in crate::workspace) fn cast_player_open(&self) -> bool {
        self.cast_player.is_some()
    }

    pub(in crate::workspace) fn broadcast_targets_empty(&self) -> bool {
        self.broadcast.targets.is_empty()
    }

    pub(in crate::workspace) fn broadcast_target_selected(&self, pane_id: PaneId) -> bool {
        self.broadcast.targets.contains(&pane_id)
    }

    pub(in crate::workspace) fn selected_broadcast_group_id(&self) -> Option<uuid::Uuid> {
        self.broadcast.selected_group_id
    }

    pub(in crate::workspace) fn broadcast_group_editor(
        &self,
    ) -> Option<(TerminalBroadcastGroupEditKind, &str)> {
        self.broadcast
            .group_editor
            .as_ref()
            .map(|editor| (editor.kind, editor.value.as_str()))
    }

    pub(in crate::workspace) fn begin_broadcast_group_create(&mut self) {
        self.broadcast.group_editor = Some(TerminalBroadcastGroupEditor {
            kind: TerminalBroadcastGroupEditKind::Create,
            value: String::new(),
        });
        self.broadcast.menu_open = true;
    }

    pub(in crate::workspace) fn begin_broadcast_group_rename(
        &mut self,
        group_id: uuid::Uuid,
        name: String,
    ) {
        self.broadcast.group_editor = Some(TerminalBroadcastGroupEditor {
            kind: TerminalBroadcastGroupEditKind::Rename(group_id),
            value: name,
        });
        self.broadcast.menu_open = true;
    }

    pub(in crate::workspace) fn replace_broadcast_group_editor_text(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
    ) -> bool {
        let Some(editor) = self.broadcast.group_editor.as_mut() else {
            return false;
        };
        replace_utf16(&mut editor.value, replacement_range, text);
        true
    }

    pub(in crate::workspace) fn cancel_broadcast_group_edit(&mut self) -> bool {
        self.broadcast.group_editor.take().is_some()
    }

    pub(in crate::workspace) fn toggle_broadcast(&mut self) {
        self.broadcast.enabled = if self.broadcast.enabled {
            false
        } else {
            self.broadcast.selected_group_id.is_none() || !self.broadcast.targets.is_empty()
        };
        self.broadcast.menu_open = false;
    }

    pub(in crate::workspace) fn select_broadcast_group(
        &mut self,
        group_id: uuid::Uuid,
        targets: &[PaneId],
    ) {
        self.broadcast.selected_group_id = Some(group_id);
        self.broadcast.targets.clear();
        self.broadcast.targets.extend(targets.iter().copied());
        self.broadcast.enabled = !self.broadcast.targets.is_empty();
    }

    pub(in crate::workspace) fn clear_selected_broadcast_group(&mut self) {
        self.broadcast.selected_group_id = None;
        self.broadcast.targets.clear();
        self.broadcast.enabled = false;
    }

    pub(in crate::workspace) fn refresh_selected_broadcast_group(
        &mut self,
        group_id: uuid::Uuid,
        targets: &[PaneId],
    ) {
        if self.broadcast.selected_group_id != Some(group_id) {
            return;
        }
        let was_enabled = self.broadcast.enabled;
        self.broadcast.targets.clear();
        self.broadcast.targets.extend(targets.iter().copied());
        self.broadcast.enabled = was_enabled && !self.broadcast.targets.is_empty();
    }

    pub(in crate::workspace) fn dismiss_broadcast_menu(&mut self) -> bool {
        let was_open = self.broadcast.menu_open;
        self.broadcast.menu_open = false;
        self.broadcast.group_editor = None;
        was_open
    }

    pub(in crate::workspace) fn set_broadcast_menu_open(&mut self, open: bool) {
        self.broadcast.menu_open = open;
    }

    pub(in crate::workspace) fn toggle_broadcast_target(&mut self, pane_id: PaneId) {
        self.broadcast.selected_group_id = None;
        if !self.broadcast.targets.remove(&pane_id) {
            self.broadcast.targets.insert(pane_id);
        }
        self.broadcast.enabled = !self.broadcast.targets.is_empty();
        self.broadcast.menu_open = true;
    }

    pub(in crate::workspace) fn set_broadcast_targets(&mut self, targets: &[PaneId]) {
        self.broadcast.selected_group_id = None;
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
            if self.broadcast.selected_group_id.is_some() {
                Vec::new()
            } else {
                candidates
            }
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

    pub(in crate::workspace) fn project_panel_open(&self) -> bool {
        self.project_panel.open
    }

    pub(in crate::workspace) fn project_query(&self) -> &str {
        &self.project_panel.query
    }

    pub(in crate::workspace) fn project_task_highlighted(&self, task_id: &str) -> bool {
        self.project_panel.highlighted_task_id.as_deref() == Some(task_id)
    }

    pub(in crate::workspace) fn open_project_panel(&mut self, key: &ProjectProbeKey) {
        self.project_panel.open = true;
        self.ensure_project_task_highlight(key);
    }

    pub(in crate::workspace) fn close_project_panel(&mut self) -> bool {
        let was_open = self.project_panel.open;
        if was_open {
            self.project_panel.close();
        }
        was_open
    }

    pub(in crate::workspace) fn visible_project_tasks(
        &self,
        key: &ProjectProbeKey,
    ) -> Vec<ProjectTask> {
        let Some(snapshot) = self.project_store.snapshot(key) else {
            return Vec::new();
        };
        let query = self.project_panel.query.trim().to_ascii_lowercase();
        snapshot
            .tasks()
            .into_iter()
            .filter(|task| {
                query.is_empty()
                    || task.label().to_ascii_lowercase().contains(&query)
                    || task.command().to_ascii_lowercase().contains(&query)
                    || task
                        .source()
                        .display_name()
                        .to_ascii_lowercase()
                        .contains(&query)
            })
            .collect()
    }

    pub(in crate::workspace) fn replace_project_query(
        &mut self,
        key: &ProjectProbeKey,
        replacement_range: Option<Range<usize>>,
        text: &str,
    ) -> bool {
        if !self.project_panel.open {
            return false;
        }
        replace_utf16(&mut self.project_panel.query, replacement_range, text);
        self.project_panel.highlighted_task_id = None;
        self.ensure_project_task_highlight(key);
        true
    }

    pub(in crate::workspace) fn ensure_project_task_highlight(&mut self, key: &ProjectProbeKey) {
        let tasks = self.visible_project_tasks(key);
        if tasks
            .iter()
            .any(|task| Some(task.id()) == self.project_panel.highlighted_task_id.as_deref())
        {
            return;
        }
        self.project_panel.highlighted_task_id = tasks.first().map(|task| task.id().to_string());
    }

    pub(in crate::workspace) fn step_project_task_highlight(
        &mut self,
        key: &ProjectProbeKey,
        forward: bool,
    ) {
        let tasks = self.visible_project_tasks(key);
        if tasks.is_empty() {
            self.project_panel.highlighted_task_id = None;
            return;
        }
        let current = self
            .project_panel
            .highlighted_task_id
            .as_deref()
            .and_then(|id| tasks.iter().position(|task| task.id() == id));
        let next = match (current, forward) {
            (Some(index), true) => (index + 1).min(tasks.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
            (None, true) => 0,
            (None, false) => tasks.len() - 1,
        };
        self.project_panel.highlighted_task_id = Some(tasks[next].id().to_string());
    }

    pub(in crate::workspace) fn highlight_project_task_edge(
        &mut self,
        key: &ProjectProbeKey,
        last: bool,
    ) {
        let tasks = self.visible_project_tasks(key);
        self.project_panel.highlighted_task_id =
            if last { tasks.last() } else { tasks.first() }.map(|task| task.id().to_string());
    }

    pub(in crate::workspace) fn set_project_task_highlight(&mut self, task_id: &str) -> bool {
        if self.project_task_highlighted(task_id) {
            return false;
        }
        self.project_panel.highlighted_task_id = Some(task_id.to_string());
        true
    }

    pub(in crate::workspace) fn selected_project_task(
        &self,
        key: &ProjectProbeKey,
    ) -> Option<ProjectTask> {
        let highlighted_task_id = self.project_panel.highlighted_task_id.as_deref()?;
        self.visible_project_tasks(key)
            .into_iter()
            .find(|task| task.id() == highlighted_task_id)
    }

    pub(in crate::workspace) fn project_task_command(
        &self,
        key: &ProjectProbeKey,
        task: &ProjectTask,
    ) -> Option<String> {
        let snapshot = self.project_store.snapshot(key)?;
        let cd_command = current_directory_cd_command(snapshot.root_path())?;
        Some(format!("{cd_command} && {}", task.command()))
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

    pub(in crate::workspace) fn set_active_git_probe_key(
        &mut self,
        key: Option<GitProbeKey>,
        cx: &mut Context<Self>,
    ) {
        if self.active_git_probe_key == key {
            return;
        }
        self.active_git_probe_key = key.clone();
        self.git_probe_schedule_generation = self.git_probe_schedule_generation.wrapping_add(1);
        self.git_probe_schedule_task = None;
        let Some(key) = key else {
            return;
        };
        self.maybe_refresh_git(key.clone(), cx);
        let generation = self.git_probe_schedule_generation;
        self.git_probe_schedule_task = Some(cx.spawn(async move |terminal, cx| {
            loop {
                Timer::after(Duration::from_millis(
                    terminal_git::TERMINAL_GIT_PROBE_TTL_MS,
                ))
                .await;
                let should_continue = terminal
                    .update(cx, |terminal, cx| {
                        if terminal.git_probe_schedule_generation != generation
                            || terminal.active_git_probe_key.as_ref() != Some(&key)
                        {
                            return false;
                        }
                        terminal.maybe_refresh_git(key.clone(), cx);
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }));
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
            self.set_active_project_probe_key(None, cx);
            // Disabling invalidates in-flight generations so late completions
            // cannot leave a permanently loading cache entry.
            self.project_store.retain_keys(|_| false);
            self.project_panel.close();
            cx.emit(WorkspaceTerminalEvent::ProjectMetadataChanged);
        }
    }

    pub(in crate::workspace) fn set_active_project_probe_key(
        &mut self,
        key: Option<ProjectProbeKey>,
        cx: &mut Context<Self>,
    ) {
        if self.active_project_probe_key == key {
            return;
        }
        self.active_project_probe_key = key.clone();
        self.project_probe_schedule_generation =
            self.project_probe_schedule_generation.wrapping_add(1);
        self.project_probe_schedule_task = None;
        let Some(key) = key else {
            return;
        };
        if !self.project_tasks_enabled {
            return;
        }
        self.maybe_refresh_project(key.clone(), cx);
        let generation = self.project_probe_schedule_generation;
        self.project_probe_schedule_task = Some(cx.spawn(async move |terminal, cx| {
            loop {
                Timer::after(Duration::from_millis(TERMINAL_PROJECT_PROBE_TTL_MS)).await;
                let should_continue = terminal
                    .update(cx, |terminal, cx| {
                        if terminal.project_probe_schedule_generation != generation
                            || terminal.active_project_probe_key.as_ref() != Some(&key)
                            || !terminal.project_tasks_enabled
                        {
                            return false;
                        }
                        terminal.maybe_refresh_project(key.clone(), cx);
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }));
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
        self.drain_git_results(cx)
            | self.drain_git_action_results(cx)
            | self.drain_project_results(cx)
            | self.drain_cwd_results(cx)
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

impl WorkspaceApp {
    pub(in crate::workspace) fn sync_active_terminal_metadata_context(&mut self, cx: &mut App) {
        let project_tasks_enabled = self.terminal_project_tasks_enabled();
        let git_key = self.active_terminal_git_key(cx);
        let project_key = project_tasks_enabled
            .then(|| self.active_terminal_project_key(cx))
            .flatten();
        self.terminal.update(cx, |terminal, cx| {
            terminal.set_project_tasks_enabled(project_tasks_enabled, cx);
            terminal.set_active_git_probe_key(git_key, cx);
            terminal.set_active_project_probe_key(project_key, cx);
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
    use oxideterm_environment::{GitBranchListOutcome, GitBranchReference};
    use std::sync::atomic::{AtomicU64, Ordering};
    use terminal_git::TerminalGitPanelSection;

    static NEXT_TERMINAL_TEST_SETTINGS_ID: AtomicU64 = AtomicU64::new(1);

    struct TerminalEventRecorder {
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
        let settings_path = std::env::temp_dir().join(format!(
            "oxideterm-terminal-entity-tests-{}-{}.json",
            std::process::id(),
            NEXT_TERMINAL_TEST_SETTINGS_ID.fetch_add(1, Ordering::Relaxed)
        ));
        cx.new(|cx| WorkspaceTerminalEntity::new(runtime, node_router, &settings_path, cx))
    }

    #[gpui::test]
    fn project_probe_state_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        let recorder = cx.new(|cx| {
            let subscription = cx.subscribe(
                &terminal,
                |recorder: &mut TerminalEventRecorder, _terminal, event, _cx| match event {
                    WorkspaceTerminalEvent::GitMetadataChanged => {
                        recorder.git_metadata_changes += 1;
                    }
                    WorkspaceTerminalEvent::ProjectMetadataChanged => {
                        recorder.project_metadata_changes += 1;
                    }
                },
            );
            TerminalEventRecorder {
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
    fn active_metadata_keys_replace_only_entity_owned_schedules(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        let git_first =
            GitProbeKey::new(GitProbeScope::ssh_node("node-a"), "/repo-a").expect("git key");
        let git_second =
            GitProbeKey::new(GitProbeScope::ssh_node("node-b"), "/repo-b").expect("git key");
        let project_first =
            ProjectProbeKey::new(ProjectProbeScope::ssh_node("node-a"), "/project-a")
                .expect("project key");
        let project_second =
            ProjectProbeKey::new(ProjectProbeScope::ssh_node("node-b"), "/project-b")
                .expect("project key");

        terminal.update(cx, |terminal, cx| {
            terminal.set_project_tasks_enabled(true, cx);
            terminal.set_active_git_probe_key(Some(git_first.clone()), cx);
            terminal.set_active_project_probe_key(Some(project_first.clone()), cx);
            let git_generation = terminal.git_probe_schedule_generation;
            let project_generation = terminal.project_probe_schedule_generation;
            assert!(terminal.git_probe_schedule_task.is_some());
            assert!(terminal.project_probe_schedule_task.is_some());

            terminal.set_active_git_probe_key(Some(git_second.clone()), cx);
            terminal.set_active_project_probe_key(Some(project_second.clone()), cx);
            assert_ne!(terminal.git_probe_schedule_generation, git_generation);
            assert_ne!(
                terminal.project_probe_schedule_generation,
                project_generation
            );
            // Switching the active context stops only future scheduling. The
            // previous key's cache and any in-flight delivery remain valid.
            assert!(terminal.git_store.get(&git_first).is_some());
            assert!(terminal.project_store.get(&project_first).is_some());

            terminal.set_active_git_probe_key(None, cx);
            terminal.set_active_project_probe_key(None, cx);
            assert!(terminal.git_probe_schedule_task.is_none());
            assert!(terminal.project_probe_schedule_task.is_none());
            assert!(terminal.git_store.get(&git_first).is_some());
            assert!(terminal.project_store.get(&project_first).is_some());
        });
    }

    #[gpui::test]
    fn git_probe_state_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        let recorder = cx.new(|cx| {
            let subscription = cx.subscribe(
                &terminal,
                |recorder: &mut TerminalEventRecorder, _terminal, event, _cx| match event {
                    WorkspaceTerminalEvent::GitMetadataChanged => {
                        recorder.git_metadata_changes += 1;
                    }
                    WorkspaceTerminalEvent::ProjectMetadataChanged => {
                        recorder.project_metadata_changes += 1;
                    }
                },
            );
            TerminalEventRecorder {
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
    fn git_panel_delivery_and_reopen_generation_are_entity_owned(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        let key = GitProbeKey::new(GitProbeScope::ssh_node("missing-node"), "/repo")
            .expect("git probe key");
        let sender = terminal.update(cx, |terminal, cx| {
            terminal.open_git_panel(key.clone(), TerminalGitPanelSection::Branches, cx);
            terminal.git_action_tx.clone()
        });
        sender
            .send(terminal_git::TerminalGitDelivery::BranchList {
                key: key.clone(),
                generation: 1,
                outcome: GitBranchListOutcome::Ready(vec![
                    GitBranchReference::new("main", true).expect("branch"),
                ]),
            })
            .expect("branch delivery");
        cx.run_until_parked();
        terminal.read_with(cx, |terminal, _cx| {
            assert!(terminal.git_panel_open());
            assert_eq!(terminal.visible_git_branches()[0].name(), "main");
        });

        terminal.update(cx, |terminal, cx| {
            assert!(terminal.close_git_panel());
            terminal.open_git_panel(key.clone(), TerminalGitPanelSection::Branches, cx);
        });
        sender
            .send(terminal_git::TerminalGitDelivery::BranchList {
                key,
                generation: 1,
                outcome: GitBranchListOutcome::NotRepository,
            })
            .expect("stale branch delivery");
        cx.run_until_parked();
        terminal.read_with(cx, |terminal, _cx| {
            assert!(terminal.visible_git_branches().is_empty());
            assert_eq!(
                terminal.git_panel_error(),
                Some(&terminal_git::TerminalGitBranchError::NodeUnavailable)
            );
        });
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

    #[gpui::test]
    fn named_broadcast_group_never_widens_after_targets_close(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        let group_id = uuid::Uuid::new_v4();
        let target = PaneId(42);

        terminal.update(cx, |terminal, _cx| {
            terminal.select_broadcast_group(group_id, &[target]);
            assert!(terminal.broadcast_enabled());
            terminal.toggle_broadcast();
            assert!(!terminal.broadcast_enabled());
            assert_eq!(terminal.selected_broadcast_group_id(), Some(group_id));
            terminal.toggle_broadcast();
            assert!(terminal.broadcast_enabled());
            terminal.retain_live_broadcast_targets(&HashSet::new());
            assert!(!terminal.broadcast_enabled());
            assert!(
                terminal
                    .filter_broadcast_targets(vec![PaneId(7)])
                    .is_empty()
            );
            assert_eq!(terminal.selected_broadcast_group_id(), Some(group_id));
        });
    }

    #[gpui::test]
    fn empty_named_broadcast_group_stays_disabled(cx: &mut TestAppContext) {
        let terminal = new_terminal_entity(cx);
        terminal.update(cx, |terminal, _cx| {
            terminal.select_broadcast_group(uuid::Uuid::new_v4(), &[]);
            assert!(!terminal.broadcast_enabled());
            terminal.toggle_broadcast();
            assert!(!terminal.broadcast_enabled());
        });
    }
}
