// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    cell::RefCell,
    collections::hash_map::DefaultHasher,
    future::Future,
    hash::{Hash, Hasher},
    ops::Range,
    time::Duration,
};

use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_environment::{
    CurrentDirectoryEntry, CurrentDirectoryEntryKind, CurrentDirectoryKey, CurrentDirectoryScope,
    CurrentDirectorySnapshot, CurrentDirectorySource, current_directory_cd_command,
    current_directory_parent, current_directory_path_is_explicit, current_directory_report_command,
    current_directory_shell_path_argument, list_local_current_directory,
    sort_current_directory_entries,
};
use oxideterm_sftp::{FileType as RemotePathFileType, ListFilter, SortOrder};
use oxideterm_ssh::NodeId;

use super::*;

const TERMINAL_CWD_REMOTE_LIST_TIMEOUT: Duration = Duration::from_secs(15);
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
pub(in crate::workspace) enum TerminalCwdError {
    Unavailable,
    RemoteListFailed,
}

async fn run_terminal_cwd_remote_stages<H, T, AcquireError, ListError>(
    acquire: impl Future<Output = Result<H, AcquireError>>,
    list_timeout: Duration,
    list: impl AsyncFnOnce(H) -> Result<T, ListError>,
) -> Result<T, ()> {
    // Shared SFTP initialization is registry-owned and is not cancellation-safe,
    // so only the directory request receives an outer timeout.
    let handle = acquire.await.map_err(|_| ())?;
    tokio::time::timeout(list_timeout, list(handle))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
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
    open: bool,
    key: Option<CurrentDirectoryKey>,
    snapshot: Option<CurrentDirectorySnapshot>,
    query: String,
    entries: Vec<CurrentDirectoryEntry>,
    highlighted_path: Option<String>,
    loading: bool,
    error: Option<TerminalCwdError>,
    list_state: ListState,
    list_cache: RefCell<VirtualListSignatureCache>,
    probe_scope: Option<CurrentDirectoryScope>,
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
        let generation = self.generation;
        *self = Self::default();
        // A picker close invalidates work but must not reuse its generation
        // when the same directory is opened again before a worker completes.
        self.generation = generation;
    }
}

fn terminal_cwd_entry_signature(entry: &TerminalCwdVisibleEntry) -> u64 {
    // Virtual list state is index-based, so rows need a stable content signature
    // when filtering or changing directories reshuffles the visible entries.
    let mut hasher = DefaultHasher::new();
    terminal_cwd_visible_entry_kind_signature(entry.kind).hash(&mut hasher);
    entry.name.hash(&mut hasher);
    entry.path.hash(&mut hasher);
    hasher.finish()
}

fn terminal_cwd_visible_entry_kind_signature(kind: TerminalCwdVisibleEntryKind) -> u8 {
    match kind {
        TerminalCwdVisibleEntryKind::Parent => 0,
        TerminalCwdVisibleEntryKind::Directory => 1,
        TerminalCwdVisibleEntryKind::File => 2,
        TerminalCwdVisibleEntryKind::TypedPath => 3,
    }
}

impl WorkspaceTerminalEntity {
    pub(in crate::workspace) fn cwd_picker_open(&self) -> bool {
        self.cwd_picker.open
    }

    pub(in crate::workspace) fn cwd_picker_loading(&self) -> bool {
        self.cwd_picker.loading
    }

    pub(in crate::workspace) fn cwd_picker_error(&self) -> Option<TerminalCwdError> {
        self.cwd_picker.error
    }

    pub(in crate::workspace) fn cwd_query(&self) -> &str {
        &self.cwd_picker.query
    }

    pub(in crate::workspace) fn cwd_snapshot_scope(&self) -> Option<CurrentDirectoryScope> {
        self.cwd_picker
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.scope().clone())
    }

    pub(in crate::workspace) fn cwd_browse_path(&self) -> Option<&str> {
        self.cwd_picker
            .key
            .as_ref()
            .map(CurrentDirectoryKey::path)
            .or_else(|| {
                self.cwd_picker
                    .snapshot
                    .as_ref()
                    .map(CurrentDirectorySnapshot::path)
            })
    }

    pub(in crate::workspace) fn cwd_list_state(&self) -> ListState {
        self.cwd_picker.list_state.clone()
    }

    pub(in crate::workspace) fn sync_cwd_list_state(&self, entries: &[TerminalCwdVisibleEntry]) {
        let signatures = entries
            .iter()
            .map(terminal_cwd_entry_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.cwd_picker.list_state,
            &mut self.cwd_picker.list_cache.borrow_mut(),
            "terminal-cwd-picker",
            &signatures,
            terminal_cwd_list_spec(),
        );
    }

    pub(in crate::workspace) fn cwd_path_highlighted(&self, path: &str) -> bool {
        self.cwd_picker.highlighted_path.as_deref() == Some(path)
    }

    pub(in crate::workspace) fn set_cwd_path_highlight(&mut self, path: &str) -> bool {
        if self.cwd_path_highlighted(path) {
            return false;
        }
        self.cwd_picker.highlighted_path = Some(path.to_string());
        true
    }

    pub(in crate::workspace) fn begin_cwd_probe(&mut self, scope: CurrentDirectoryScope) -> u64 {
        let generation = self.cwd_picker.next_generation();
        self.cwd_picker.open = true;
        self.cwd_picker.key = None;
        self.cwd_picker.snapshot = None;
        self.cwd_picker.query.clear();
        self.cwd_picker.entries.clear();
        self.cwd_picker.highlighted_path = None;
        self.cwd_picker.loading = true;
        self.cwd_picker.error = None;
        self.cwd_picker.probe_scope = Some(scope);
        generation
    }

    pub(in crate::workspace) fn finish_cwd_probe_unavailable(&mut self, generation: u64) {
        if !self.cwd_picker.open || self.cwd_picker.generation != generation {
            return;
        }
        self.cwd_picker.loading = false;
        self.cwd_picker.error = Some(TerminalCwdError::Unavailable);
        self.cwd_picker.probe_scope = None;
    }

    pub(in crate::workspace) fn open_cwd_picker_for_snapshot(
        &mut self,
        snapshot: CurrentDirectorySnapshot,
        cx: &mut Context<Self>,
    ) {
        let generation = self.cwd_picker.next_generation();
        self.open_cwd_picker_for_snapshot_generation(snapshot, generation, cx);
    }

    fn open_cwd_picker_for_snapshot_generation(
        &mut self,
        snapshot: CurrentDirectorySnapshot,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let key = snapshot.key().clone();
        let scope = snapshot.scope().clone();
        self.cwd_picker.open = true;
        self.cwd_picker.key = Some(key.clone());
        self.cwd_picker.query.clear();
        self.cwd_picker.entries.clear();
        self.cwd_picker.highlighted_path =
            current_directory_parent(snapshot.path()).or_else(|| Some(snapshot.path().to_string()));
        self.cwd_picker.snapshot = Some(snapshot);
        self.cwd_picker.error = None;
        self.cwd_picker.probe_scope = None;

        match scope {
            CurrentDirectoryScope::Local => {
                self.cwd_picker.loading = false;
                let outcome = list_local_current_directory(key.path(), TERMINAL_CWD_MAX_ENTRIES)
                    .map(TerminalCwdListOutcome::Ready)
                    .unwrap_or(TerminalCwdListOutcome::Unavailable);
                if self.apply_cwd_directory_list_result(key, generation, outcome) {
                    cx.notify();
                }
            }
            CurrentDirectoryScope::SshNode(node_id) => {
                self.cwd_picker.loading = true;
                self.spawn_remote_cwd_directory_list(key, generation, NodeId::new(node_id));
                cx.notify();
            }
        }
    }

    pub(in crate::workspace) fn spawn_cwd_report_poll(
        &mut self,
        generation: u64,
        pane: gpui::WeakEntity<TerminalPane>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |terminal, cx| {
            for _ in 0..TERMINAL_CWD_REPORT_POLL_ATTEMPTS {
                gpui::Timer::after(TERMINAL_CWD_REPORT_POLL_INTERVAL).await;
                match terminal.update(cx, |terminal, cx| {
                    terminal.apply_cwd_report_if_ready(generation, &pane, cx)
                }) {
                    Ok(true) | Err(_) => return,
                    Ok(false) => {}
                }
            }
            let _ = terminal.update(cx, |terminal, cx| {
                terminal.finish_cwd_report_timeout(generation);
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_cwd_report_if_ready(
        &mut self,
        generation: u64,
        pane: &gpui::WeakEntity<TerminalPane>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.cwd_picker.open || self.cwd_picker.generation != generation {
            return true;
        }
        if self.cwd_picker.snapshot.is_some() {
            return true;
        }
        let Some(scope) = self.cwd_picker.probe_scope.clone() else {
            return true;
        };
        let Some(pane) = pane.upgrade() else {
            self.finish_cwd_probe_unavailable(generation);
            return true;
        };
        let Some(snapshot) = terminal_cwd_snapshot_from_pane(scope, pane.read(cx)) else {
            return false;
        };
        self.open_cwd_picker_for_snapshot_generation(snapshot, generation, cx);
        true
    }

    fn finish_cwd_report_timeout(&mut self, generation: u64) {
        if !self.cwd_picker.open
            || self.cwd_picker.generation != generation
            || self.cwd_picker.snapshot.is_some()
        {
            return;
        }
        self.cwd_picker.loading = false;
        self.cwd_picker.error = Some(TerminalCwdError::Unavailable);
    }

    pub(in crate::workspace) fn close_cwd_picker(&mut self) -> bool {
        let was_open = self.cwd_picker.open;
        if was_open {
            self.cwd_picker.close();
        }
        was_open
    }

    pub(in crate::workspace) fn set_cwd_error(&mut self, error: TerminalCwdError) {
        if self.cwd_picker.open {
            self.cwd_picker.loading = false;
            self.cwd_picker.error = Some(error);
        }
    }

    pub(in crate::workspace) fn visible_cwd_entries(&self) -> Vec<TerminalCwdVisibleEntry> {
        let Some(path) = self.cwd_browse_path() else {
            return Vec::new();
        };
        let query = self.cwd_picker.query.trim().to_ascii_lowercase();
        let mut rows = Vec::new();

        if let Some(parent) = current_directory_parent(path) {
            rows.push(TerminalCwdVisibleEntry {
                kind: TerminalCwdVisibleEntryKind::Parent,
                name: "..".to_string(),
                path: parent,
            });
        }
        rows.extend(
            self.cwd_picker
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
        if let Some(path) = self.cwd_query_path_candidate() {
            rows.push(TerminalCwdVisibleEntry {
                kind: TerminalCwdVisibleEntryKind::TypedPath,
                name: path.clone(),
                path,
            });
        }
        rows
    }

    pub(in crate::workspace) fn replace_cwd_query(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
    ) -> bool {
        if !self.cwd_picker.open {
            return false;
        }
        replace_utf16(&mut self.cwd_picker.query, replacement_range, text);
        self.cwd_picker.highlighted_path = None;
        self.ensure_cwd_highlight();
        true
    }

    pub(in crate::workspace) fn enter_cwd_directory(
        &mut self,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = &self.cwd_picker.snapshot else {
            return;
        };
        let Some(key) = CurrentDirectoryKey::new(snapshot.scope().clone(), path) else {
            return;
        };
        let generation = self.cwd_picker.next_generation();
        self.load_cwd_directory(key, generation, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn selected_cwd_entry(&self) -> Option<TerminalCwdVisibleEntry> {
        let visible = self.visible_cwd_entries();
        self.cwd_picker
            .highlighted_path
            .as_deref()
            .and_then(|path| visible.iter().find(|entry| entry.path == path))
            .or_else(|| visible.first())
            .cloned()
    }

    pub(in crate::workspace) fn step_cwd_highlight(&mut self, forward: bool) {
        let visible = self.visible_cwd_entries();
        if visible.is_empty() {
            self.cwd_picker.highlighted_path = None;
            return;
        }
        let current = self
            .cwd_picker
            .highlighted_path
            .as_deref()
            .and_then(|path| visible.iter().position(|entry| entry.path == path));
        let next = match (current, forward) {
            (Some(index), true) => (index + 1).min(visible.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
            (None, true) => 0,
            (None, false) => visible.len() - 1,
        };
        self.cwd_picker.highlighted_path = Some(visible[next].path.clone());
    }

    pub(in crate::workspace) fn highlight_cwd_edge(&mut self, last: bool) {
        let visible = self.visible_cwd_entries();
        self.cwd_picker.highlighted_path = if last {
            visible.last()
        } else {
            visible.first()
        }
        .map(|entry| entry.path.clone());
    }

    pub(super) fn drain_cwd_results(&mut self, cx: &mut Context<Self>) -> bool {
        let delivery_batch =
            delivery::drain_channel(&self.cwd_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        let mut changed = false;
        for delivery in delivery_batch.items {
            match delivery {
                TerminalCwdDelivery::DirectoryList {
                    key,
                    generation,
                    outcome,
                } => {
                    changed |= self.apply_cwd_directory_list_result(key, generation, outcome);
                }
            }
        }
        if changed {
            cx.notify();
        }
        delivery_batch.outcome.backlog_remaining
    }

    fn spawn_remote_cwd_directory_list(
        &self,
        key: CurrentDirectoryKey,
        generation: u64,
        node_id: NodeId,
    ) {
        let node_router = self.node_router.clone();
        let tx = self.cwd_tx.clone();
        let cwd = key.path().to_string();
        self.runtime.spawn(async move {
            let remote_result = async {
                // The picker borrows the registry-owned shared session; it never
                // creates or disconnects a transport tied to picker lifetime.
                let entries = run_terminal_cwd_remote_stages(
                    node_router.acquire_sftp(&node_id),
                    TERMINAL_CWD_REMOTE_LIST_TIMEOUT,
                    async move |shared| {
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
                    },
                )
                .await?;
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
                sort_current_directory_entries(&mut rows);
                rows.truncate(TERMINAL_CWD_MAX_ENTRIES);
                Ok::<_, ()>(rows)
            }
            .await;
            let outcome = remote_result
                .map(TerminalCwdListOutcome::Ready)
                .unwrap_or(TerminalCwdListOutcome::RemoteListFailed);
            let _ = tx.send(TerminalCwdDelivery::DirectoryList {
                key,
                generation,
                outcome,
            });
        });
    }

    fn load_cwd_directory(
        &mut self,
        key: CurrentDirectoryKey,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        self.cwd_picker.key = Some(key.clone());
        self.cwd_picker.query.clear();
        self.cwd_picker.entries.clear();
        self.cwd_picker.highlighted_path =
            current_directory_parent(key.path()).or_else(|| Some(key.path().to_string()));
        self.cwd_picker.error = None;

        match key.scope().clone() {
            CurrentDirectoryScope::Local => {
                self.cwd_picker.loading = false;
                let outcome = list_local_current_directory(key.path(), TERMINAL_CWD_MAX_ENTRIES)
                    .map(TerminalCwdListOutcome::Ready)
                    .unwrap_or(TerminalCwdListOutcome::Unavailable);
                if self.apply_cwd_directory_list_result(key, generation, outcome) {
                    cx.notify();
                }
            }
            CurrentDirectoryScope::SshNode(node_id) => {
                self.cwd_picker.loading = true;
                self.spawn_remote_cwd_directory_list(key, generation, NodeId::new(node_id));
                cx.notify();
            }
        }
    }

    fn apply_cwd_directory_list_result(
        &mut self,
        key: CurrentDirectoryKey,
        generation: u64,
        outcome: TerminalCwdListOutcome,
    ) -> bool {
        if !self.cwd_picker.open
            || self.cwd_picker.key.as_ref() != Some(&key)
            || self.cwd_picker.generation != generation
        {
            return false;
        }
        self.cwd_picker.loading = false;
        match outcome {
            TerminalCwdListOutcome::Ready(entries) => {
                self.cwd_picker.error = None;
                self.cwd_picker.entries = entries;
                self.ensure_cwd_highlight();
            }
            TerminalCwdListOutcome::Unavailable => {
                self.cwd_picker.entries.clear();
                self.cwd_picker.highlighted_path = None;
                self.cwd_picker.error = Some(TerminalCwdError::Unavailable);
            }
            TerminalCwdListOutcome::RemoteListFailed => {
                self.cwd_picker.entries.clear();
                self.cwd_picker.highlighted_path = None;
                self.cwd_picker.error = Some(TerminalCwdError::RemoteListFailed);
            }
        }
        true
    }

    fn cwd_query_path_candidate(&self) -> Option<String> {
        let query = self.cwd_picker.query.trim();
        if !current_directory_path_is_explicit(query)
            || self
                .cwd_picker
                .entries
                .iter()
                .any(|entry| entry.path() == query)
        {
            return None;
        }
        current_directory_cd_command(query).map(|_| query.to_string())
    }

    fn ensure_cwd_highlight(&mut self) {
        let visible = self.visible_cwd_entries();
        if visible
            .iter()
            .any(|entry| Some(entry.path.as_str()) == self.cwd_picker.highlighted_path.as_deref())
        {
            return;
        }
        self.cwd_picker.highlighted_path = visible.first().map(|entry| entry.path.clone());
    }
}

fn terminal_cwd_snapshot_from_pane(
    scope: CurrentDirectoryScope,
    pane: &TerminalPane,
) -> Option<CurrentDirectorySnapshot> {
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
    if matches!(&scope, CurrentDirectoryScope::Local)
        && let Some(snapshot) = pane.process_info().cwd.and_then(|path| {
            CurrentDirectorySnapshot::new(
                scope.clone(),
                path.to_string_lossy().to_string(),
                CurrentDirectorySource::ProcessFallback,
            )
        })
    {
        return Some(snapshot);
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
        cx: &App,
    ) -> Option<CurrentDirectorySnapshot> {
        let (scope, pane_id) = self.active_terminal_cwd_scope_and_pane(cx)?;
        self.terminal_cwd_snapshot_for_pane(scope, pane_id, cx)
    }

    pub(in crate::workspace) fn active_terminal_cwd_host(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let (_, pane_id) = self.active_terminal_cwd_scope_and_pane(cx)?;
        self.tab_host
            .read(cx)
            .panes()
            .get(&pane_id)?
            .read(cx)
            .current_working_directory_host()
    }

    pub(in crate::workspace) fn active_terminal_cwd_is_pending(
        &self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((_, pane_id)) = self.active_terminal_cwd_scope_and_pane(cx) else {
            return false;
        };
        self.tab_host
            .read(cx)
            .panes()
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
        let (scope, pane_id) = self.active_terminal_cwd_scope_and_pane(cx)?;
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
        let (scope, pane_id) = self.active_terminal_cwd_scope_and_pane(cx)?;
        match &scope {
            CurrentDirectoryScope::SshNode(active_node_id) if active_node_id == &node_id.0 => {}
            _ => return None,
        }
        self.terminal_cwd_snapshot_for_pane(scope, pane_id, cx)
            .map(|snapshot| snapshot.path().to_string())
    }

    pub(in crate::workspace) fn active_terminal_cwd_scope_and_pane(
        &self,
        cx: &App,
    ) -> Option<(CurrentDirectoryScope, PaneId)> {
        let tab = self.active_tab(cx)?;
        let pane_id = tab.active_pane_id?;
        let scope = match tab.kind {
            TabKind::LocalTerminal => CurrentDirectoryScope::Local,
            TabKind::SshTerminal => {
                let session_id = self.active_terminal_session_id(cx)?;
                let node_id = self
                    .workspace_runtime
                    .read(cx)
                    .ssh_terminal_node_id(session_id)?;
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
        cx: &App,
    ) -> Option<CurrentDirectorySnapshot> {
        let tab_host = self.tab_host.read(cx);
        let pane = tab_host.panes().get(&pane_id)?.read(cx);
        terminal_cwd_snapshot_from_pane(scope, pane)
    }

    pub(in crate::workspace) fn open_terminal_cwd_picker(&mut self, cx: &mut Context<Self>) {
        if !self.terminal_current_directory_awareness_enabled() {
            return;
        }
        self.prepare_terminal_cwd_picker(cx);

        if let Some(snapshot) = self.active_terminal_cwd_snapshot(cx) {
            self.terminal.update(cx, |terminal, cx| {
                terminal.open_cwd_picker_for_snapshot(snapshot, cx);
            });
            return;
        };

        let Some((scope, pane_id)) = self.active_terminal_cwd_scope_and_pane(cx) else {
            return;
        };
        let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
            return;
        };
        let remote_scope = matches!(&scope, CurrentDirectoryScope::SshNode(_));
        let generation = self
            .terminal
            .update(cx, |terminal, _cx| terminal.begin_cwd_probe(scope));

        if remote_scope {
            // SSH fallback probes used to write a hidden-looking command into the
            // interactive PTY, but remote shells can echo it visibly. Until the
            // prompt-owned hook is installed, unknown remote cwd must degrade
            // instead of mutating the user's terminal input stream.
            self.terminal.update(cx, |terminal, _cx| {
                terminal.finish_cwd_probe_unavailable(generation);
            });
            cx.notify();
            return;
        }

        let command = current_directory_report_command();
        if pane.update(cx, |pane, cx| {
            pane.send_internal_control_command_line(command, cx)
        }) {
            self.terminal.update(cx, |terminal, cx| {
                // The report task observes the pane weakly so closing the pane
                // never delays terminal consumer release or shared-node cleanup.
                terminal.spawn_cwd_report_poll(generation, pane.downgrade(), cx);
            });
        } else {
            self.terminal.update(cx, |terminal, _cx| {
                terminal.finish_cwd_probe_unavailable(generation);
            });
        }
        cx.notify();
    }

    fn prepare_terminal_cwd_picker(&mut self, cx: &mut Context<Self>) {
        self.dismiss_terminal_recording_menu();
        self.dismiss_terminal_broadcast_menu(cx);
        self.dismiss_terminal_highlight_popover();
        self.close_terminal_quick_commands_popover(cx);
        self.close_terminal_git_branch_picker(cx);
        self.close_terminal_project_panel(cx);
        self.ime_marked_text = None;
        self.clear_ime_selection();
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_cwd_picker(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let was_open = self
            .terminal
            .update(cx, |terminal, _cx| terminal.close_cwd_picker());
        if was_open {
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
        self.close_terminal_cwd_picker(cx);
        self.open_file_manager_tab_at_path(path, window, cx);
    }

    pub(in crate::workspace) fn open_terminal_cwd_path_in_sftp(
        &mut self,
        node_id: NodeId,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_terminal_cwd_picker(cx);
        self.open_sftp_tab_at_remote_path(node_id, path, window, cx);
    }

    pub(in crate::workspace) fn open_terminal_cwd_path_in_ide(
        &mut self,
        node_id: NodeId,
        path: String,
        cx: &mut Context<Self>,
    ) {
        self.close_terminal_cwd_picker(cx);
        self.open_ide_folder_picker_tab_at_path(node_id, path, cx);
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
        let Some(pane_id) = self.active_pane_id(cx) else {
            self.terminal.update(cx, |terminal, _cx| {
                terminal.set_cwd_error(TerminalCwdError::Unavailable);
            });
            cx.notify();
            return;
        };
        let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
            self.terminal.update(cx, |terminal, _cx| {
                terminal.set_cwd_error(TerminalCwdError::Unavailable);
            });
            cx.notify();
            return;
        };
        if !pane.read(cx).can_switch_working_directory_from_chrome() {
            self.terminal.update(cx, |terminal, _cx| {
                terminal.set_cwd_error(TerminalCwdError::Unavailable);
            });
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
        self.close_terminal_cwd_picker(cx);
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

        // File rows compose the active sender draft without running shell input
        // or changing the current directory.
        self.append_terminal_command_sender_text(&argument, true, cx);
        self.close_terminal_cwd_picker(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn handle_terminal_cwd_picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.terminal.read(cx).cwd_picker_open() {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return false;
        }

        match key {
            "escape" => {
                self.close_terminal_cwd_picker(cx);
                cx.notify();
                true
            }
            "up" | "arrowup" => {
                self.terminal
                    .update(cx, |terminal, _cx| terminal.step_cwd_highlight(false));
                cx.notify();
                true
            }
            "down" | "arrowdown" => {
                self.terminal
                    .update(cx, |terminal, _cx| terminal.step_cwd_highlight(true));
                cx.notify();
                true
            }
            "home" => {
                self.terminal
                    .update(cx, |terminal, _cx| terminal.highlight_cwd_edge(false));
                cx.notify();
                true
            }
            "end" => {
                self.terminal
                    .update(cx, |terminal, _cx| terminal.highlight_cwd_edge(true));
                cx.notify();
                true
            }
            "enter" => {
                let selected = self.terminal.read(cx).selected_cwd_entry();
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

    #[tokio::test]
    async fn remote_cwd_acquisition_is_not_limited_by_the_list_timeout() {
        let result = run_terminal_cwd_remote_stages(
            async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok::<_, ()>("shared-session")
            },
            Duration::from_millis(1),
            async |session| Ok::<_, ()>(session.len()),
        )
        .await;

        assert_eq!(result, Ok("shared-session".len()));
    }

    #[tokio::test]
    async fn remote_cwd_listing_respects_its_timeout() {
        let result =
            run_terminal_cwd_remote_stages(async { Ok::<_, ()>(()) }, Duration::ZERO, async |_| {
                std::future::pending::<Result<(), ()>>().await
            })
            .await;
        assert_eq!(result, Err(()));
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
