// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_environment::{
    ProjectProbeKey, ProjectProbeOutcome, ProjectProbeScope, ProjectSnapshot, ProjectTask,
    remote_project_cwd_source_is_trusted,
};

use super::*;

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
    pub(super) open: bool,
    pub(super) query: String,
    pub(super) highlighted_task_id: Option<String>,
}

impl TerminalProjectPanelState {
    pub(super) fn close(&mut self) {
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
        self.terminal.read(cx).project_snapshot(&key)
    }

    pub(in crate::workspace) fn open_terminal_project_panel(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.active_terminal_project_key(cx) else {
            return;
        };
        self.dismiss_terminal_recording_menu();
        self.dismiss_terminal_broadcast_menu(cx);
        self.dismiss_terminal_highlight_popover();
        self.close_terminal_quick_commands_popover(cx);
        self.close_terminal_cwd_picker(cx);
        self.close_terminal_git_branch_picker(cx);
        self.terminal
            .update(cx, |terminal, _cx| terminal.open_project_panel(&key));
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_project_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.terminal
            .update(cx, |terminal, _cx| terminal.close_project_panel())
    }

    pub(in crate::workspace) fn run_terminal_project_task(
        &mut self,
        task: ProjectTask,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.active_terminal_project_key(cx) else {
            return;
        };
        let Some(command) = self.terminal.read(cx).project_task_command(&key, &task) else {
            return;
        };
        let Some(pane) = self.active_pane(cx) else {
            return;
        };
        // Project tasks must be visible terminal actions so failures, prompts,
        // and long-running dev servers stay under the active shell lifecycle.
        pane.update(cx, |pane, cx| pane.send_command_line(&command, cx));
        self.close_terminal_project_panel(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn handle_terminal_project_panel_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.terminal.read(cx).project_panel_open() {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return false;
        }

        match key {
            "escape" => {
                self.close_terminal_project_panel(cx);
                cx.notify();
                true
            }
            "up" | "arrowup" => {
                if let Some(key) = self.active_terminal_project_key(cx) {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal.step_project_task_highlight(&key, false);
                    });
                }
                cx.notify();
                true
            }
            "down" | "arrowdown" => {
                if let Some(key) = self.active_terminal_project_key(cx) {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal.step_project_task_highlight(&key, true);
                    });
                }
                cx.notify();
                true
            }
            "home" => {
                if let Some(key) = self.active_terminal_project_key(cx) {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal.highlight_project_task_edge(&key, false);
                    });
                }
                cx.notify();
                true
            }
            "end" => {
                if let Some(key) = self.active_terminal_project_key(cx) {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal.highlight_project_task_edge(&key, true);
                    });
                }
                cx.notify();
                true
            }
            "enter" => {
                let task = self
                    .active_terminal_project_key(cx)
                    .and_then(|key| self.terminal.read(cx).selected_project_task(&key));
                if let Some(task) = task {
                    self.run_terminal_project_task(task, cx);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    pub(in crate::workspace) fn active_terminal_project_key(
        &self,
        cx: &App,
    ) -> Option<ProjectProbeKey> {
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
}
