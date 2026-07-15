// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::{cell::RefCell, time::Duration};

use oxideterm_environment::{
    CurrentDirectoryEntry, CurrentDirectoryEntryKind, CurrentDirectoryKey, CurrentDirectoryScope,
    CurrentDirectorySnapshot, CurrentDirectorySource, current_directory_cd_command,
    current_directory_parent, current_directory_report_command,
    current_directory_shell_path_argument,
};
use oxideterm_sftp::{FileType as RemotePathFileType, ListFilter, SortOrder};
use oxideterm_ssh::NodeId;

use super::*;

const TERMINAL_CWD_REMOTE_LIST_TIMEOUT: Duration = Duration::from_millis(1_200);
const TERMINAL_CWD_REPORT_POLL_INTERVAL: Duration = Duration::from_millis(40);
const TERMINAL_CWD_REPORT_POLL_ATTEMPTS: usize = 30;
const TERMINAL_CWD_MAX_ENTRIES: usize = 160;
const TERMINAL_CWD_LIST_ESTIMATED_HEIGHT: f32 = 42.0;
const TERMINAL_CWD_LIST_OVERSCAN: usize = 8;

pub(in crate::workspace) fn terminal_cwd_list_spec() -> TauriVirtualListSpec {
    TauriVirtualListSpec::new(
        px(TERMINAL_CWD_LIST_ESTIMATED_HEIGHT),
        TERMINAL_CWD_LIST_OVERSCAN,
    )
}

#[derive(Clone, Debug)]
pub(in crate::workspace) enum TerminalCwdDelivery {
    DirectoryList {
        key: CurrentDirectoryKey,
        generation: u64,
        outcome: TerminalCwdListOutcome,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TerminalCwdListOutcome {
    Ready(Vec<CurrentDirectoryEntry>),
    Unavailable,
    RemoteListFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TerminalCwdVisibleEntryKind {
    Parent,
    Directory,
    File,
    TypedPath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct TerminalCwdVisibleEntry {
    pub kind: TerminalCwdVisibleEntryKind,
    pub name: String,
    pub path: String,
}

pub(in crate::workspace) struct TerminalCwdPickerState {
    pub open: bool,
    pub key: Option<CurrentDirectoryKey>,
    pub snapshot: Option<CurrentDirectorySnapshot>,
    pub query: String,
    pub entries: Vec<CurrentDirectoryEntry>,
    pub highlighted_path: Option<String>,
    pub loading: bool,
    pub error: Option<String>,
    pub list_state: ListState,
    pub list_cache: RefCell<VirtualListSignatureCache>,
    probe_scope: Option<CurrentDirectoryScope>,
    probe_pane_id: Option<PaneId>,
    generation: u64,
}

impl Default for TerminalCwdPickerState {
    fn default() -> Self {
        Self {
            open: false,
            key: None,
            snapshot: None,
            query: String::new(),
            entries: Vec::new(),
            highlighted_path: None,
            loading: false,
            error: None,
            // Keep the picker scroll owned by GPUI ListState so the visual
            // scrollbar and rendered rows stay synchronized for large folders.
            list_state: tauri_virtual_list_state(0, ListAlignment::Top, terminal_cwd_list_spec()),
            list_cache: RefCell::new(VirtualListSignatureCache::default()),
            probe_scope: None,
            probe_pane_id: None,
            generation: 0,
        }
    }
}

impl TerminalCwdPickerState {
    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }

    fn close(&mut self) {
        *self = Self::default();
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn terminal_current_directory_awareness_enabled(&self) -> bool {
        self.settings_store
            .settings()
            .terminal
            .command_bar
            .current_directory_awareness
    }

    pub(in crate::workspace) fn active_terminal_cwd_snapshot(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<CurrentDirectorySnapshot> {
        let (scope, pane_id) = self.active_terminal_cwd_scope_and_pane()?;
        self.terminal_cwd_snapshot_for_pane(scope, pane_id, cx)
    }

    pub(in crate::workspace) fn active_terminal_cwd_host(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let (_, pane_id) = self.active_terminal_cwd_scope_and_pane()?;
        self.panes
            .get(&pane_id)?
            .read(cx)
            .current_working_directory_host()
    }

    pub(in crate::workspace) fn active_terminal_cwd_is_pending(
        &self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((_, pane_id)) = self.active_terminal_cwd_scope_and_pane() else {
            return false;
        };
        self.panes
            .get(&pane_id)
            .is_some_and(|pane| pane.read(cx).current_working_directory_is_pending())
    }

    pub(in crate::workspace) fn active_local_terminal_cwd_path(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if !self.terminal_current_directory_awareness_enabled() {
            return None;
        }
        let (scope, pane_id) = self.active_terminal_cwd_scope_and_pane()?;
        if !matches!(&scope, CurrentDirectoryScope::Local) {
            return None;
        }
        self.terminal_cwd_snapshot_for_pane(scope, pane_id, cx)
            .map(|snapshot| snapshot.path().to_string())
    }

    pub(in crate::workspace) fn active_ssh_terminal_cwd_path_for_node(
        &self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if !self.terminal_current_directory_awareness_enabled() {
            return None;
        }
        let (scope, pane_id) = self.active_terminal_cwd_scope_and_pane()?;
        match &scope {
            CurrentDirectoryScope::SshNode(active_node_id) if active_node_id == &node_id.0 => {}
            _ => return None,
        }
        self.terminal_cwd_snapshot_for_pane(scope, pane_id, cx)
            .map(|snapshot| snapshot.path().to_string())
    }

    pub(in crate::workspace) fn active_terminal_cwd_scope_and_pane(
        &self,
    ) -> Option<(CurrentDirectoryScope, PaneId)> {
        let tab = self.active_tab()?;
        let pane_id = tab.active_pane_id?;
        let scope = match tab.kind {
            TabKind::LocalTerminal => CurrentDirectoryScope::Local,
            TabKind::SshTerminal => {
                let session_id = self.active_terminal_session_id()?;
                let node_id = self.terminal_ssh_nodes.get(&session_id)?;
                CurrentDirectoryScope::ssh_node(node_id.0.clone())
            }
            _ => return None,
        };
        Some((scope, pane_id))
    }

    fn terminal_cwd_snapshot_for_pane(
        &self,
        scope: CurrentDirectoryScope,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) -> Option<CurrentDirectorySnapshot> {
        let pane = self.panes.get(&pane_id)?.read(cx);

        let current_cwd = pane.current_working_directory();
        let current_source = pane.current_working_directory_source();
        // A pending user-selected directory and a shell-owned OSC report are
        // more current than an asynchronous process probe.
        if pane.current_working_directory_is_pending()
            || current_source == Some(TerminalWorkingDirectorySource::ShellIntegration)
        {
            let cwd = current_cwd.clone()?;
            let source = match current_source {
                Some(TerminalWorkingDirectorySource::ShellIntegration) => {
                    CurrentDirectorySource::ShellIntegration
                }
                Some(TerminalWorkingDirectorySource::VisibleCommand) => {
                    CurrentDirectorySource::UserAction
                }
                Some(TerminalWorkingDirectorySource::SessionDefault) => {
                    CurrentDirectorySource::SessionDefault
                }
                None => CurrentDirectorySource::VisibleText,
            };
            return CurrentDirectorySnapshot::new(scope, cwd, source);
        }
        if matches!(&scope, CurrentDirectoryScope::Local) {
            if let Some(snapshot) = pane.process_info().cwd.and_then(|path| {
                CurrentDirectorySnapshot::new(
                    scope.clone(),
                    path.to_string_lossy().to_string(),
                    CurrentDirectorySource::ProcessFallback,
                )
            }) {
                return Some(snapshot);
            }
        }
        if let Some(cwd) = current_cwd {
            let source = match current_source {
                Some(TerminalWorkingDirectorySource::VisibleCommand) => {
                    CurrentDirectorySource::UserAction
                }
                Some(TerminalWorkingDirectorySource::SessionDefault) => {
                    CurrentDirectorySource::SessionDefault
                }
                _ => CurrentDirectorySource::VisibleText,
            };
            return CurrentDirectorySnapshot::new(scope, cwd, source);
        }
        None
    }

    pub(in crate::workspace) fn open_terminal_cwd_picker(&mut self, cx: &mut Context<Self>) {
        if !self.terminal_current_directory_awareness_enabled() {
            return;
        }
        self.prepare_terminal_cwd_picker(cx);

        if let Some(snapshot) = self.active_terminal_cwd_snapshot(cx) {
            let generation = self.terminal_cwd_picker.next_generation();
            self.open_terminal_cwd_picker_for_snapshot(snapshot, generation, cx);
            return;
        };

        let Some((scope, pane_id)) = self.active_terminal_cwd_scope_and_pane() else {
            return;
        };

        let generation = self.terminal_cwd_picker.next_generation();
        self.terminal_cwd_picker.open = true;
        self.terminal_cwd_picker.key = None;
        self.terminal_cwd_picker.snapshot = None;
        self.terminal_cwd_picker.query.clear();
        self.terminal_cwd_picker.entries.clear();
        self.terminal_cwd_picker.highlighted_path = None;
        self.terminal_cwd_picker.loading = true;
        self.terminal_cwd_picker.error = None;
        self.terminal_cwd_picker.probe_scope = Some(scope);
        self.terminal_cwd_picker.probe_pane_id = Some(pane_id);

        if matches!(
            self.terminal_cwd_picker.probe_scope,
            Some(CurrentDirectoryScope::SshNode(_))
        ) {
            // SSH fallback probes used to write a hidden-looking command into the
            // interactive PTY, but remote shells can echo it visibly. Until the
            // prompt-owned hook is installed, unknown remote cwd must degrade
            // instead of mutating the user's terminal input stream.
            self.terminal_cwd_picker.loading = false;
            self.terminal_cwd_picker.error = Some(self.i18n.t("terminal.cwd.unavailable"));
            self.terminal_cwd_picker.probe_scope = None;
            self.terminal_cwd_picker.probe_pane_id = None;
            cx.notify();
            return;
        }

        if self.request_terminal_cwd_report(pane_id, cx) {
            self.spawn_terminal_cwd_report_poll(generation, cx);
        } else {
            self.terminal_cwd_picker.loading = false;
            self.terminal_cwd_picker.error = Some(self.i18n.t("terminal.cwd.unavailable"));
        }
        cx.notify();
    }

    fn prepare_terminal_cwd_picker(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_broadcast_menu();
        self.close_terminal_quick_commands_popover();
        self.close_terminal_git_branch_picker();
        self.close_terminal_project_panel();
        self.terminal_command_suggestions_open = false;
        self.terminal_command_suggestion_highlighted = None;
        self.terminal_command_bar_focused = false;
        self.ime_marked_text = None;
        self.clear_ime_selection();
        cx.notify();
    }

    fn open_terminal_cwd_picker_for_snapshot(
        &mut self,
        snapshot: CurrentDirectorySnapshot,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let key = snapshot.key().clone();
        self.terminal_cwd_picker.open = true;
        self.terminal_cwd_picker.key = Some(key.clone());
        self.terminal_cwd_picker.snapshot = Some(snapshot.clone());
        self.terminal_cwd_picker.query.clear();
        self.terminal_cwd_picker.entries.clear();
        self.terminal_cwd_picker.highlighted_path =
            current_directory_parent(snapshot.path()).or_else(|| Some(snapshot.path().to_string()));
        self.terminal_cwd_picker.error = None;
        self.terminal_cwd_picker.probe_scope = None;
        self.terminal_cwd_picker.probe_pane_id = None;

        match snapshot.scope() {
            CurrentDirectoryScope::Local => {
                self.terminal_cwd_picker.loading = false;
                let outcome = terminal_cwd_local_directory_entries(snapshot.path());
                let changed =
                    self.apply_terminal_cwd_directory_list_result(key, generation, outcome);
                if changed {
                    cx.notify();
                }
            }
            CurrentDirectoryScope::SshNode(node_id) => {
                self.terminal_cwd_picker.loading = true;
                self.spawn_remote_terminal_cwd_directory_list(
                    key,
                    generation,
                    NodeId::new(node_id.clone()),
                );
                cx.notify();
            }
        }
    }

    fn request_terminal_cwd_report(&mut self, pane_id: PaneId, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.panes.get(&pane_id) else {
            return false;
        };
        let command = current_directory_report_command();
        pane.update(cx, |pane, cx| {
            pane.send_internal_control_command_line(command, cx)
        })
    }

    fn spawn_terminal_cwd_report_poll(&mut self, generation: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |weak, cx| {
            for _ in 0..TERMINAL_CWD_REPORT_POLL_ATTEMPTS {
                gpui::Timer::after(TERMINAL_CWD_REPORT_POLL_INTERVAL).await;
                match weak.update(cx, |this, cx| {
                    this.apply_terminal_cwd_report_if_ready(generation, cx)
                }) {
                    Ok(true) | Err(_) => return,
                    Ok(false) => {}
                }
            }
            let _ = weak.update(cx, |this, cx| {
                this.finish_terminal_cwd_report_timeout(generation, cx);
            });
        })
        .detach();
    }

    fn apply_terminal_cwd_report_if_ready(
        &mut self,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.terminal_cwd_picker.open || self.terminal_cwd_picker.generation != generation {
            return true;
        }
        if self.terminal_cwd_picker.snapshot.is_some() {
            return true;
        }
        let Some(scope) = self.terminal_cwd_picker.probe_scope.clone() else {
            return true;
        };
        let Some(pane_id) = self.terminal_cwd_picker.probe_pane_id else {
            return true;
        };
        let Some(snapshot) = self.terminal_cwd_snapshot_for_pane(scope, pane_id, cx) else {
            return false;
        };
        self.open_terminal_cwd_picker_for_snapshot(snapshot, generation, cx);
        true
    }

    fn finish_terminal_cwd_report_timeout(&mut self, generation: u64, cx: &mut Context<Self>) {
        if !self.terminal_cwd_picker.open
            || self.terminal_cwd_picker.generation != generation
            || self.terminal_cwd_picker.snapshot.is_some()
        {
            return;
        }
        self.terminal_cwd_picker.loading = false;
        self.terminal_cwd_picker.error = Some(self.i18n.t("terminal.cwd.unavailable"));
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_cwd_picker(&mut self) -> bool {
        let was_open = self.terminal_cwd_picker.open;
        if was_open {
            self.terminal_cwd_picker.close();
            self.ime_marked_text = None;
            self.clear_ime_selection();
        }
        was_open
    }

    pub(in crate::workspace) fn copy_terminal_cwd_path(
        &mut self,
        path: String,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(path));
    }

    pub(in crate::workspace) fn open_terminal_cwd_path_in_file_manager(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_terminal_cwd_picker();
        self.open_file_manager_tab_at_path(path, window, cx);
    }

    pub(in crate::workspace) fn open_terminal_cwd_path_in_sftp(
        &mut self,
        node_id: NodeId,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_terminal_cwd_picker();
        self.open_sftp_tab_at_remote_path(node_id, path, window, cx);
    }

    pub(in crate::workspace) fn open_terminal_cwd_path_in_ide(
        &mut self,
        node_id: NodeId,
        path: String,
        cx: &mut Context<Self>,
    ) {
        self.close_terminal_cwd_picker();
        self.open_ide_folder_picker_tab_at_path(node_id, path, cx);
    }

    pub(in crate::workspace) fn visible_terminal_cwd_entries(
        &self,
    ) -> Vec<TerminalCwdVisibleEntry> {
        let Some(path) = self.terminal_cwd_browse_path() else {
            return Vec::new();
        };
        let query = self.terminal_cwd_picker.query.trim().to_ascii_lowercase();
        let mut rows = Vec::new();

        if let Some(parent) = current_directory_parent(path) {
            rows.push(TerminalCwdVisibleEntry {
                kind: TerminalCwdVisibleEntryKind::Parent,
                name: "..".to_string(),
                path: parent,
            });
        }

        rows.extend(
            self.terminal_cwd_picker
                .entries
                .iter()
                .filter(|entry| {
                    query.is_empty()
                        || entry.name().to_ascii_lowercase().contains(&query)
                        || entry.path().to_ascii_lowercase().contains(&query)
                })
                .map(|entry| TerminalCwdVisibleEntry {
                    kind: match entry.kind() {
                        CurrentDirectoryEntryKind::Directory => {
                            TerminalCwdVisibleEntryKind::Directory
                        }
                        CurrentDirectoryEntryKind::File => TerminalCwdVisibleEntryKind::File,
                    },
                    name: entry.name().to_string(),
                    path: entry.path().to_string(),
                }),
        );

        if let Some(path) = self.terminal_cwd_query_path_candidate() {
            rows.push(TerminalCwdVisibleEntry {
                kind: TerminalCwdVisibleEntryKind::TypedPath,
                name: path.clone(),
                path,
            });
        }

        rows
    }

    pub(in crate::workspace) fn terminal_cwd_browse_path(&self) -> Option<&str> {
        self.terminal_cwd_picker
            .key
            .as_ref()
            .map(CurrentDirectoryKey::path)
            .or_else(|| {
                self.terminal_cwd_picker
                    .snapshot
                    .as_ref()
                    .map(CurrentDirectorySnapshot::path)
            })
    }

    pub(in crate::workspace) fn enter_terminal_cwd_directory(
        &mut self,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = &self.terminal_cwd_picker.snapshot else {
            return;
        };
        let Some(key) = CurrentDirectoryKey::new(snapshot.scope().clone(), path) else {
            return;
        };
        let generation = self.terminal_cwd_picker.next_generation();
        self.load_terminal_cwd_directory(key, generation, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn select_terminal_cwd_path(
        &mut self,
        path: String,
        verified_directory: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = current_directory_cd_command(&path) else {
            return;
        };
        let Some(pane_id) = self.active_pane_id() else {
            self.terminal_cwd_picker.error = Some(self.i18n.t("terminal.cwd.unavailable"));
            cx.notify();
            return;
        };
        let Some(pane) = self.panes.get(&pane_id).cloned() else {
            self.terminal_cwd_picker.error = Some(self.i18n.t("terminal.cwd.unavailable"));
            cx.notify();
            return;
        };
        if !pane.read(cx).can_switch_working_directory_from_chrome() {
            self.terminal_cwd_picker.error = Some(self.i18n.t("terminal.cwd.unavailable"));
            cx.notify();
            return;
        }

        // Directory changes must be visible shell actions on the active pane;
        // background probes never mutate cwd on a reused SSH node.
        pane.update(cx, |pane, cx| {
            pane.send_command_line(&command, cx);
            if verified_directory {
                // Listed directories were resolved in the active pane scope, so
                // the chrome can follow the visible `cd` while the command mark
                // still gets the final say on success or rollback.
                pane.set_pending_current_working_directory_from_terminal_action(
                    path.clone(),
                    command.clone(),
                    cx,
                );
            }
        });
        self.close_terminal_cwd_picker();
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn insert_terminal_cwd_file_path(
        &mut self,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(argument) = current_directory_shell_path_argument(&path) else {
            return;
        };

        // File rows should help compose the command bar, not run shell input or
        // change cwd. Preserve any draft command and append a shell-safe path.
        if self
            .terminal_command_bar_draft
            .chars()
            .next_back()
            .is_some_and(|last| !last.is_whitespace())
        {
            self.terminal_command_bar_draft.push(' ');
        }
        self.terminal_command_bar_draft.push_str(&argument);
        self.terminal_command_bar_focused = true;
        self.terminal_command_suggestions_open = false;
        self.terminal_command_suggestion_highlighted = None;
        self.close_terminal_cwd_picker();
        cx.notify();
    }

    pub(in crate::workspace) fn handle_terminal_cwd_picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.terminal_cwd_picker.open {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return false;
        }

        match key {
            "escape" => {
                self.close_terminal_cwd_picker();
                cx.notify();
                true
            }
            "up" | "arrowup" => {
                self.step_terminal_cwd_highlight(false);
                cx.notify();
                true
            }
            "down" | "arrowdown" => {
                self.step_terminal_cwd_highlight(true);
                cx.notify();
                true
            }
            "home" => {
                self.highlight_terminal_cwd_edge(false);
                cx.notify();
                true
            }
            "end" => {
                self.highlight_terminal_cwd_edge(true);
                cx.notify();
                true
            }
            "enter" => {
                let visible = self.visible_terminal_cwd_entries();
                let selected = self
                    .terminal_cwd_picker
                    .highlighted_path
                    .as_deref()
                    .and_then(|path| visible.iter().find(|entry| entry.path == path))
                    .or_else(|| visible.first())
                    .cloned();
                if let Some(entry) = selected {
                    match entry.kind {
                        TerminalCwdVisibleEntryKind::File => {
                            self.insert_terminal_cwd_file_path(entry.path, cx);
                        }
                        _ => self.select_terminal_cwd_path(
                            entry.path,
                            terminal_cwd_entry_confirms_directory(entry.kind),
                            window,
                            cx,
                        ),
                    }
                }
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace) fn poll_terminal_cwd_results(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        while let Ok(delivery) = self.terminal_cwd_rx.try_recv() {
            match delivery {
                TerminalCwdDelivery::DirectoryList {
                    key,
                    generation,
                    outcome,
                } => {
                    changed |=
                        self.apply_terminal_cwd_directory_list_result(key, generation, outcome);
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    fn spawn_remote_terminal_cwd_directory_list(
        &self,
        key: CurrentDirectoryKey,
        generation: u64,
        node_id: NodeId,
    ) {
        let node_router = self.node_router.clone();
        let tx = self.terminal_cwd_tx.clone();
        let cwd = key.path().to_string();
        self.forwarding_runtime.spawn(async move {
            let outcome = tokio::time::timeout(TERMINAL_CWD_REMOTE_LIST_TIMEOUT, async {
                let shared = node_router
                    .acquire_sftp(&node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                let entries = {
                    let sftp = shared.lock().await;
                    sftp.list_dir_with_cwd(
                        &cwd,
                        Some(ListFilter {
                            show_hidden: true,
                            pattern: None,
                            sort: SortOrder::Name,
                        }),
                    )
                    .await
                    .map_err(|error| error.to_string())?
                };
                let (_, entries) = entries;
                let mut rows = entries
                    .into_iter()
                    .filter_map(|entry| match entry.file_type {
                        RemotePathFileType::Directory => {
                            CurrentDirectoryEntry::new(entry.name, entry.path)
                        }
                        RemotePathFileType::File
                        | RemotePathFileType::Symlink
                        | RemotePathFileType::Unknown => {
                            CurrentDirectoryEntry::new_file(entry.name, entry.path)
                        }
                    })
                    .collect::<Vec<_>>();
                rows.sort_by(terminal_cwd_entry_order);
                rows.truncate(TERMINAL_CWD_MAX_ENTRIES);
                Ok::<Vec<CurrentDirectoryEntry>, String>(rows)
            })
            .await
            .ok()
            .and_then(|result| result.ok())
            .map(TerminalCwdListOutcome::Ready)
            .unwrap_or(TerminalCwdListOutcome::RemoteListFailed);

            let _ = tx.send(TerminalCwdDelivery::DirectoryList {
                key,
                generation,
                outcome,
            });
        });
    }

    fn load_terminal_cwd_directory(
        &mut self,
        key: CurrentDirectoryKey,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        self.terminal_cwd_picker.key = Some(key.clone());
        self.terminal_cwd_picker.query.clear();
        self.terminal_cwd_picker.entries.clear();
        self.terminal_cwd_picker.highlighted_path =
            current_directory_parent(key.path()).or_else(|| Some(key.path().to_string()));
        self.terminal_cwd_picker.error = None;

        match key.scope().clone() {
            CurrentDirectoryScope::Local => {
                self.terminal_cwd_picker.loading = false;
                let outcome = terminal_cwd_local_directory_entries(key.path());
                let changed =
                    self.apply_terminal_cwd_directory_list_result(key, generation, outcome);
                if changed {
                    cx.notify();
                }
            }
            CurrentDirectoryScope::SshNode(node_id) => {
                self.terminal_cwd_picker.loading = true;
                self.spawn_remote_terminal_cwd_directory_list(
                    key,
                    generation,
                    NodeId::new(node_id),
                );
                cx.notify();
            }
        }
    }

    fn apply_terminal_cwd_directory_list_result(
        &mut self,
        key: CurrentDirectoryKey,
        generation: u64,
        outcome: TerminalCwdListOutcome,
    ) -> bool {
        if !self.terminal_cwd_picker.open
            || self.terminal_cwd_picker.key.as_ref() != Some(&key)
            || self.terminal_cwd_picker.generation != generation
        {
            return false;
        }

        self.terminal_cwd_picker.loading = false;
        match outcome {
            TerminalCwdListOutcome::Ready(entries) => {
                self.terminal_cwd_picker.error = None;
                self.terminal_cwd_picker.entries = entries;
                self.ensure_terminal_cwd_highlight();
            }
            TerminalCwdListOutcome::Unavailable => {
                self.terminal_cwd_picker.entries.clear();
                self.terminal_cwd_picker.highlighted_path = None;
                self.terminal_cwd_picker.error = Some(self.i18n.t("terminal.cwd.unavailable"));
            }
            TerminalCwdListOutcome::RemoteListFailed => {
                self.terminal_cwd_picker.entries.clear();
                self.terminal_cwd_picker.highlighted_path = None;
                self.terminal_cwd_picker.error =
                    Some(self.i18n.t("terminal.cwd.remote_list_failed"));
            }
        }
        true
    }

    fn terminal_cwd_query_path_candidate(&self) -> Option<String> {
        let query = self.terminal_cwd_picker.query.trim();
        if !terminal_cwd_looks_path_like(query) {
            return None;
        }
        if self
            .terminal_cwd_picker
            .entries
            .iter()
            .any(|entry| entry.path() == query)
        {
            return None;
        }
        current_directory_cd_command(query).map(|_| query.to_string())
    }

    fn ensure_terminal_cwd_highlight(&mut self) {
        let visible = self.visible_terminal_cwd_entries();
        if visible.iter().any(|entry| {
            Some(entry.path.as_str()) == self.terminal_cwd_picker.highlighted_path.as_deref()
        }) {
            return;
        }
        self.terminal_cwd_picker.highlighted_path = visible.first().map(|entry| entry.path.clone());
    }

    fn step_terminal_cwd_highlight(&mut self, forward: bool) {
        let visible = self.visible_terminal_cwd_entries();
        if visible.is_empty() {
            self.terminal_cwd_picker.highlighted_path = None;
            return;
        }
        let current = self
            .terminal_cwd_picker
            .highlighted_path
            .as_deref()
            .and_then(|path| visible.iter().position(|entry| entry.path == path));
        let next = match (current, forward) {
            (Some(index), true) => (index + 1).min(visible.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
            (None, true) => 0,
            (None, false) => visible.len() - 1,
        };
        self.terminal_cwd_picker.highlighted_path = Some(visible[next].path.clone());
    }

    fn highlight_terminal_cwd_edge(&mut self, last: bool) {
        let visible = self.visible_terminal_cwd_entries();
        self.terminal_cwd_picker.highlighted_path = if last {
            visible.last()
        } else {
            visible.first()
        }
        .map(|entry| entry.path.clone());
    }
}

fn terminal_cwd_local_directory_entries(cwd: &str) -> TerminalCwdListOutcome {
    let actual_cwd = terminal_cwd_expand_local_home(cwd);
    let Ok(entries) = std::fs::read_dir(&actual_cwd) else {
        return TerminalCwdListOutcome::Unavailable;
    };
    let mut rows = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = std::fs::symlink_metadata(entry.path()).ok()?;
            let kind = if metadata.is_dir() {
                CurrentDirectoryEntryKind::Directory
            } else if metadata.is_file() || metadata.file_type().is_symlink() {
                CurrentDirectoryEntryKind::File
            } else {
                return None;
            };
            let name = entry.file_name().to_string_lossy().to_string();
            let path = terminal_cwd_join_display_child(cwd, &entry.path(), &name);
            CurrentDirectoryEntry::new_with_kind(name, path, kind)
        })
        .collect::<Vec<_>>();
    rows.sort_by(terminal_cwd_entry_order);
    rows.truncate(TERMINAL_CWD_MAX_ENTRIES);
    TerminalCwdListOutcome::Ready(rows)
}

fn terminal_cwd_entry_order(
    left: &CurrentDirectoryEntry,
    right: &CurrentDirectoryEntry,
) -> std::cmp::Ordering {
    match (left.kind(), right.kind()) {
        (CurrentDirectoryEntryKind::Directory, CurrentDirectoryEntryKind::File) => {
            std::cmp::Ordering::Less
        }
        (CurrentDirectoryEntryKind::File, CurrentDirectoryEntryKind::Directory) => {
            std::cmp::Ordering::Greater
        }
        _ => left.name().to_lowercase().cmp(&right.name().to_lowercase()),
    }
}

fn terminal_cwd_expand_local_home(cwd: &str) -> std::path::PathBuf {
    let cwd = cwd.trim();
    if cwd == "~" {
        return terminal_cwd_local_home().unwrap_or_else(|| std::path::PathBuf::from(cwd));
    }
    if let Some(rest) = cwd.strip_prefix("~/")
        && let Some(home) = terminal_cwd_local_home()
    {
        return home.join(rest);
    }
    std::path::PathBuf::from(cwd)
}

fn terminal_cwd_local_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

fn terminal_cwd_join_display_child(
    cwd: &str,
    absolute_path: &std::path::Path,
    name: &str,
) -> String {
    let cwd = cwd.trim_end_matches(['/', '\\']);
    if cwd == "~" {
        format!("~/{name}")
    } else if cwd.starts_with("~/") {
        format!("{cwd}/{name}")
    } else {
        absolute_path.to_string_lossy().to_string()
    }
}

fn terminal_cwd_looks_path_like(value: &str) -> bool {
    value == "~"
        || value.starts_with("~/")
        || value.starts_with('/')
        || value.starts_with("\\\\")
        || (value.len() > 2 && value.as_bytes().get(1) == Some(&b':'))
}

fn terminal_cwd_entry_confirms_directory(kind: TerminalCwdVisibleEntryKind) -> bool {
    // Parent and directory rows come from a resolved listing. Typed rows are
    // user input and may still fail when the shell executes `cd`.
    matches!(
        kind,
        TerminalCwdVisibleEntryKind::Parent | TerminalCwdVisibleEntryKind::Directory
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_candidate_requires_path_shape() {
        assert!(!terminal_cwd_looks_path_like("Documents"));
        assert!(terminal_cwd_looks_path_like("~/Documents"));
        assert!(terminal_cwd_looks_path_like("/Users/dominical"));
        assert!(terminal_cwd_looks_path_like("C:\\Users"));
    }

    #[test]
    fn display_child_preserves_home_relative_paths() {
        assert_eq!(
            terminal_cwd_join_display_child(
                "~",
                std::path::Path::new("/home/a/Documents"),
                "Documents"
            ),
            "~/Documents"
        );
        assert_eq!(
            terminal_cwd_join_display_child(
                "~/Documents",
                std::path::Path::new("/home/a/Documents/OxideTerm"),
                "OxideTerm",
            ),
            "~/Documents/OxideTerm"
        );
    }

    #[test]
    fn only_resolved_rows_update_cwd_optimistically() {
        assert!(terminal_cwd_entry_confirms_directory(
            TerminalCwdVisibleEntryKind::Parent
        ));
        assert!(terminal_cwd_entry_confirms_directory(
            TerminalCwdVisibleEntryKind::Directory
        ));
        assert!(!terminal_cwd_entry_confirms_directory(
            TerminalCwdVisibleEntryKind::File
        ));
        assert!(!terminal_cwd_entry_confirms_directory(
            TerminalCwdVisibleEntryKind::TypedPath
        ));
    }
}
