// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oxideterm_environment::{
    ProjectProbeError, ProjectProbeKey, ProjectProbeOutcome, ProjectProbeScope, ProjectSnapshot,
    ProjectTask, current_directory_cd_command, parse_remote_shell_project_probe_output,
    probe_local_project, remote_project_cwd_source_is_trusted, remote_shell_project_probe_command,
};
use oxideterm_ssh::NodeId;

use super::*;

const TERMINAL_PROJECT_PROBE_TTL_MS: u64 = 5_000;
const TERMINAL_PROJECT_REMOTE_TIMEOUT: Duration = Duration::from_secs(3);
const TERMINAL_PROJECT_REMOTE_MAX_OUTPUT: usize = 512 * 1024;

#[derive(Clone, Debug)]
pub(in crate::workspace) enum TerminalProjectDelivery {
    Probe {
        key: ProjectProbeKey,
        generation: u64,
        outcome: ProjectProbeOutcome,
    },
}

#[derive(Default)]
pub(in crate::workspace) struct TerminalProjectPanelState {
    pub open: bool,
    pub query: String,
    pub highlighted_task_id: Option<String>,
}

impl TerminalProjectPanelState {
    fn close(&mut self) {
        *self = Self::default();
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn terminal_project_tasks_enabled(&self) -> bool {
        let command_bar_settings = &self.settings_store.settings().terminal.command_bar;
        command_bar_settings.enabled && command_bar_settings.project_tasks
    }

    pub(in crate::workspace) fn active_terminal_project_snapshot(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<ProjectSnapshot> {
        let key = self.active_terminal_project_key(cx)?;
        self.terminal_project_store.snapshot(&key).cloned()
    }

    pub(in crate::workspace) fn maybe_refresh_active_terminal_project(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.active_terminal_project_key(cx) else {
            return;
        };
        let now_ms = terminal_project_now_ms();
        if !self
            .terminal_project_store
            .should_probe(&key, now_ms, TERMINAL_PROJECT_PROBE_TTL_MS)
        {
            return;
        }

        let generation = self
            .terminal_project_store
            .mark_loading(key.clone(), now_ms);
        match key.scope() {
            ProjectProbeScope::Local => self.spawn_local_terminal_project_probe(key, generation),
            ProjectProbeScope::SshNode(node_id) => {
                let node_id = NodeId::new(node_id.clone());
                self.spawn_remote_terminal_project_probe(key, generation, node_id, cx);
            }
        }
    }

    pub(in crate::workspace) fn poll_terminal_project_results(&mut self, cx: &mut Context<Self>) {
        if !self.terminal_project_tasks_enabled() {
            // Drop stale probe results while the feature is disabled so a
            // completed background probe cannot resurrect the project panel.
            while self.terminal_project_rx.try_recv().is_ok() {}
            if self.close_terminal_project_panel() {
                cx.notify();
            }
            return;
        }

        let mut changed = false;
        while let Ok(delivery) = self.terminal_project_rx.try_recv() {
            match delivery {
                TerminalProjectDelivery::Probe {
                    key,
                    generation,
                    outcome,
                } => {
                    changed |= self.terminal_project_store.finish_probe(
                        &key,
                        generation,
                        outcome,
                        terminal_project_now_ms(),
                    );
                    if changed {
                        self.ensure_terminal_project_task_highlight(cx);
                    }
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn open_terminal_project_panel(&mut self, cx: &mut Context<Self>) {
        if self.active_terminal_project_key(cx).is_none() {
            return;
        }
        self.dismiss_terminal_broadcast_menu();
        self.close_terminal_quick_commands_popover();
        self.close_terminal_cwd_picker();
        self.close_terminal_git_branch_picker();
        self.terminal_command_suggestions_open = false;
        self.terminal_command_suggestion_highlighted = None;
        self.terminal_command_bar_focused = false;
        self.terminal_project_panel.open = true;
        self.ensure_terminal_project_task_highlight(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_project_panel(&mut self) -> bool {
        let was_open = self.terminal_project_panel.open;
        if was_open {
            self.terminal_project_panel.close();
        }
        was_open
    }

    pub(in crate::workspace) fn visible_terminal_project_tasks(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<ProjectTask> {
        let Some(snapshot) = self.active_terminal_project_snapshot(cx) else {
            return Vec::new();
        };
        let query = self
            .terminal_project_panel
            .query
            .trim()
            .to_ascii_lowercase();
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

    pub(in crate::workspace) fn run_terminal_project_task(
        &mut self,
        task: ProjectTask,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = self.active_terminal_project_snapshot(cx) else {
            return;
        };
        let Some(cd_command) = current_directory_cd_command(snapshot.root_path()) else {
            return;
        };
        let command = format!("{cd_command} && {}", task.command());
        let Some(pane) = self.active_pane() else {
            return;
        };
        // Project tasks must be visible terminal actions so failures, prompts,
        // and long-running dev servers stay under the active shell lifecycle.
        pane.update(cx, |pane, cx| pane.send_command_line(&command, cx));
        self.close_terminal_project_panel();
        cx.notify();
    }

    pub(in crate::workspace) fn handle_terminal_project_panel_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.terminal_project_panel.open {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return false;
        }

        match key {
            "escape" => {
                self.close_terminal_project_panel();
                cx.notify();
                true
            }
            "up" | "arrowup" => {
                self.step_terminal_project_task_highlight(false, cx);
                cx.notify();
                true
            }
            "down" | "arrowdown" => {
                self.step_terminal_project_task_highlight(true, cx);
                cx.notify();
                true
            }
            "home" => {
                self.highlight_terminal_project_task_edge(false, cx);
                cx.notify();
                true
            }
            "end" => {
                self.highlight_terminal_project_task_edge(true, cx);
                cx.notify();
                true
            }
            "enter" => {
                let tasks = self.visible_terminal_project_tasks(cx);
                let task = self
                    .terminal_project_panel
                    .highlighted_task_id
                    .as_deref()
                    .and_then(|id| tasks.into_iter().find(|task| task.id() == id));
                if let Some(task) = task {
                    self.run_terminal_project_task(task, cx);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn active_terminal_project_key(&self, cx: &mut Context<Self>) -> Option<ProjectProbeKey> {
        if !self.terminal_project_tasks_enabled() {
            return None;
        }

        let snapshot = self.active_terminal_cwd_snapshot(cx)?;
        let scope = match snapshot.scope() {
            oxideterm_environment::CurrentDirectoryScope::Local => ProjectProbeScope::Local,
            oxideterm_environment::CurrentDirectoryScope::SshNode(node_id) => {
                if !remote_project_cwd_source_is_trusted(snapshot.source()) {
                    return None;
                }
                ProjectProbeScope::ssh_node(node_id.clone())
            }
        };
        ProjectProbeKey::new(scope, snapshot.path().to_string())
    }

    fn spawn_local_terminal_project_probe(&self, key: ProjectProbeKey, generation: u64) {
        let tx = self.terminal_project_tx.clone();
        let cwd = key.cwd().to_string();
        self.forwarding_runtime.spawn(async move {
            let outcome = probe_local_project(&cwd);
            let _ = tx.send(TerminalProjectDelivery::Probe {
                key,
                generation,
                outcome,
            });
        });
    }

    fn spawn_remote_terminal_project_probe(
        &mut self,
        key: ProjectProbeKey,
        generation: u64,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let resolved = self.node_router.resolve_connection_now(&node_id);
        let handle = match resolved {
            Ok(resolved) => resolved.handle,
            Err(_) => {
                let changed = self.terminal_project_store.finish_probe(
                    &key,
                    generation,
                    ProjectProbeOutcome::Error(ProjectProbeError::new(
                        "ssh node is not ready for project probing",
                    )),
                    terminal_project_now_ms(),
                );
                if changed {
                    cx.notify();
                }
                return;
            }
        };

        let tx = self.terminal_project_tx.clone();
        let command = remote_shell_project_probe_command(key.cwd());
        self.forwarding_runtime.spawn(async move {
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
            let _ = tx.send(TerminalProjectDelivery::Probe {
                key,
                generation,
                outcome,
            });
        });
    }

    pub(in crate::workspace) fn ensure_terminal_project_task_highlight(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let tasks = self.visible_terminal_project_tasks(cx);
        if tasks.iter().any(|task| {
            Some(task.id()) == self.terminal_project_panel.highlighted_task_id.as_deref()
        }) {
            return;
        }
        self.terminal_project_panel.highlighted_task_id =
            tasks.first().map(|task| task.id().to_string());
    }

    fn step_terminal_project_task_highlight(&mut self, forward: bool, cx: &mut Context<Self>) {
        let tasks = self.visible_terminal_project_tasks(cx);
        if tasks.is_empty() {
            self.terminal_project_panel.highlighted_task_id = None;
            return;
        }
        let current = self
            .terminal_project_panel
            .highlighted_task_id
            .as_deref()
            .and_then(|id| tasks.iter().position(|task| task.id() == id));
        let next = match (current, forward) {
            (Some(index), true) => (index + 1).min(tasks.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
            (None, true) => 0,
            (None, false) => tasks.len() - 1,
        };
        self.terminal_project_panel.highlighted_task_id = Some(tasks[next].id().to_string());
    }

    fn highlight_terminal_project_task_edge(&mut self, last: bool, cx: &mut Context<Self>) {
        let tasks = self.visible_terminal_project_tasks(cx);
        self.terminal_project_panel.highlighted_task_id =
            if last { tasks.last() } else { tasks.first() }.map(|task| task.id().to_string());
    }
}

fn terminal_project_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}
