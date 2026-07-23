use super::*;

// Keep scheduling policy independent from GPUI so lifecycle edges remain unit-testable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SftpRemoteLoadState {
    loading: bool,
    pending: bool,
    inflight: bool,
}

impl SftpRemoteLoadState {
    fn request(mut self) -> Self {
        // A newer request queues behind the one shared in-flight list operation.
        self.loading = true;
        self.pending = true;
        self
    }

    fn start(mut self) -> Option<Self> {
        // SFTP views share one list slot, which keeps stale completions unambiguous.
        if self.inflight || !self.pending {
            return None;
        }
        self.loading = true;
        self.pending = false;
        self.inflight = true;
        Some(self)
    }

    fn complete(mut self) -> Self {
        // Keep the loading indicator only when another request is already queued.
        self.inflight = false;
        self.loading = self.pending;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpRemoteListCompletionContext {
    CurrentVisibleView,
    CurrentHiddenView,
    StaleView,
}

impl SftpRemoteListCompletionContext {
    fn should_apply(self) -> bool {
        !matches!(self, Self::StaleView)
    }
}

fn classify_sftp_remote_list_completion(
    tab_still_owns_node: bool,
    view_still_owns_node: bool,
    tab_is_active: bool,
) -> SftpRemoteListCompletionContext {
    // Visibility does not own the result; the remembered SFTP view does.
    if !tab_still_owns_node || !view_still_owns_node {
        SftpRemoteListCompletionContext::StaleView
    } else if tab_is_active {
        SftpRemoteListCompletionContext::CurrentVisibleView
    } else {
        SftpRemoteListCompletionContext::CurrentHiddenView
    }
}

impl WorkspaceApp {
    fn sftp_remote_load_state(&self) -> SftpRemoteLoadState {
        SftpRemoteLoadState {
            loading: self.sftp_view.remote_loading,
            pending: self.sftp_view.remote_load_pending,
            inflight: self.sftp_view.remote_load_inflight,
        }
    }

    fn set_sftp_remote_load_state(&mut self, state: SftpRemoteLoadState) {
        self.sftp_view.remote_loading = state.loading;
        self.sftp_view.remote_load_pending = state.pending;
        self.sftp_view.remote_load_inflight = state.inflight;
    }

    pub(in crate::workspace::sftp) fn request_sftp_remote_load(&mut self) {
        let state = self.sftp_remote_load_state().request();
        self.set_sftp_remote_load_state(state);
        self.signal_sftp_remote_load();
    }

    fn signal_sftp_remote_load(&self) {
        // The wake shares the ordered worker channel so path changes finish
        // mutating UI state before the GPUI consumer snapshots a new request.
        let _ = self.sftp_worker_tx.send(SftpWorkerResult::WakeRemoteLoad);
    }

    pub(in crate::workspace) fn open_sftp_tab(
        &mut self,
        node_id: NodeId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial_remote_path = self.active_ssh_terminal_cwd_path_for_node(&node_id, cx);
        let node_title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.0.clone());
        let title = format!("{} · {}", self.i18n.t("sidebar.panels.sftp"), node_title);
        let tab_id = if let Some((tab_id, _)) = self
            .sftp_tab_nodes
            .iter()
            .find(|(_, existing_node_id)| *existing_node_id == &node_id)
        {
            *tab_id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Sftp,
                title,
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            });
            self.sftp_tab_nodes.insert(tab_id, node_id.clone());
            tab_id
        };

        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.main_window_tabs.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.active_ssh_node_id = Some(node_id.clone());
        self.activate_sftp_view_for_node(&node_id);
        if let Some(path) = initial_remote_path.filter(|path| !path.trim().is_empty()) {
            // SFTP keeps its own remembered path, but an explicit open from an
            // active SSH terminal can use that pane cwd as the initial folder.
            self.set_sftp_path(SftpPane::Remote, path);
        }
        // Opening the SFTP surface mirrors Tauri's createTab path: it does
        // not start SSH. The SFTP worker consumes an already-connected node
        // and reports the router's not-connected error when the node is down.
        self.request_sftp_remote_load();
        cx.notify();
    }

    pub(in crate::workspace) fn open_sftp_tab_at_remote_path(
        &mut self,
        node_id: NodeId,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_sftp_tab(node_id, window, cx);
        if !path.trim().is_empty() {
            // This path comes from an explicit cwd-panel action, so it may be a
            // browsed row rather than the active terminal's confirmed cwd.
            self.set_sftp_path(SftpPane::Remote, path);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn activate_sftp_view_for_node(&mut self, node_id: &NodeId) {
        if self.sftp_view_node.as_ref() == Some(node_id) {
            // Returning to a hidden SFTP tab must restart any pending request
            // without relying on the workspace heartbeat.
            self.signal_sftp_remote_load();
            return;
        }

        if let Some(previous_node_id) = self.sftp_view_node.clone() {
            self.sftp_local_path_memory
                .insert(previous_node_id.clone(), self.sftp_view.local_path.clone());
            if !self.sftp_view.remote_path.is_empty() {
                self.sftp_path_memory
                    .insert(previous_node_id, self.sftp_view.remote_path.clone());
            }
        }

        self.sftp_view_node = Some(node_id.clone());
        let local_path = self
            .sftp_local_path_memory
            .get(node_id)
            .cloned()
            .unwrap_or_else(home_path);
        self.sftp_view.local_path = local_path.clone();
        self.sftp_view.local_path_input = local_path.clone();
        self.sftp_view.local_path_completion.dismiss();
        self.sftp_view.local_files = list_local_files(&local_path).unwrap_or_default();
        self.sftp_view.local_selected.clear();
        self.sftp_view.local_last_selected = None;
        self.sftp_view
            .local_path_scroll
            .set_offset(Point::new(px(0.0), px(0.0)));

        let remembered_remote = self
            .sftp_path_memory
            .get(node_id)
            .cloned()
            .unwrap_or_default();
        self.sftp_view.remote_path = remembered_remote.clone();
        self.sftp_view.remote_path_input = remembered_remote;
        self.sftp_view.remote_path_completion.dismiss();
        self.sftp_view.remote_path_completion_pending_selection = None;
        self.sftp_view.remote_files.clear();
        self.sftp_view.remote_selected.clear();
        self.sftp_view.remote_last_selected = None;
        self.sftp_view
            .remote_path_scroll
            .set_offset(Point::new(px(0.0), px(0.0)));
        // Keep an older node's list request serialized. Its completion will
        // release the shared in-flight slot and leave this node's request pending.
        self.request_sftp_remote_load();
        self.sftp_view.remote_load_retry_count = 0;
        self.sftp_view.init_error = None;
    }

    pub(in crate::workspace::sftp) fn maybe_start_sftp_remote_load(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(started_state) = self.sftp_remote_load_state().start() else {
            return false;
        };
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
            return false;
        };
        if self
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .is_none_or(|tab| tab.kind != TabKind::Sftp)
        {
            return false;
        }
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return false;
        };
        let path = self.sftp_view.remote_path.clone();
        self.set_sftp_remote_load_state(started_state);
        self.start_sftp_remote_load(tab_id, node_id, path, cx);
        true
    }

    fn start_sftp_remote_load(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let session_id = format!("node:{}:sftp", node_id.0);
        self.sftp_view.init_error = None;

        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        let router = self.node_router.clone();
        runtime.spawn(async move {
            // Tauri node_sftp_* calls do not synchronously borrow a terminal
            // session before starting SFTP work. The worker waits on the
            // node-owned connection and then opens the real SFTP subsystem
            // channel from ConnectionEntry.
            let result = load_remote_sftp_listing(router, &node_id, &path).await;
            let _ = tx.send(SftpWorkerResult::RemoteList {
                tab_id,
                node_id,
                session_id,
                path,
                result,
            });
        });
        cx.notify();
    }

    pub(in crate::workspace) fn handle_sftp_worker_result(
        &mut self,
        result: SftpWorkerResult,
        cx: &mut Context<Self>,
    ) {
        let changed = {
            let mut changed = false;
            match result {
                SftpWorkerResult::WakeRemoteLoad => {
                    self.maybe_start_sftp_remote_load(cx);
                }
                SftpWorkerResult::RemoteList {
                    tab_id,
                    node_id,
                    session_id,
                    path,
                    result,
                } => {
                    let completion_context = classify_sftp_remote_list_completion(
                        self.sftp_tab_nodes.get(&tab_id) == Some(&node_id),
                        self.sftp_view_node.as_ref() == Some(&node_id),
                        self.main_window_tabs.active_tab_id == Some(tab_id),
                    );
                    self.set_sftp_remote_load_state(self.sftp_remote_load_state().complete());
                    changed = true;
                    if completion_context.should_apply() {
                        match result {
                            Ok(listing) => {
                                let cwd = listing.cwd;
                                self.sftp_path_memory.insert(node_id.clone(), cwd.clone());
                                self.sftp_remote_home_by_node
                                    .entry(node_id.clone())
                                    .or_insert_with(|| cwd.clone());
                                self.sftp_view.remote_load_retry_count = 0;
                                self.sftp_view.remote_path = cwd.clone();
                                self.sftp_view.remote_path_input = cwd.clone();
                                self.sftp_view.remote_files = listing.files;
                                self.sftp_view.remote_selected.clear();
                                self.sftp_view.remote_last_selected = None;
                                if self
                                    .sftp_view
                                    .remote_path_completion_pending_selection
                                    .as_ref()
                                    .is_some_and(|(parent_path, _)| parent_path == &cwd)
                                    && let Some((_, name)) = self
                                        .sftp_view
                                        .remote_path_completion_pending_selection
                                        .take()
                                    && self
                                        .sftp_view
                                        .remote_files
                                        .iter()
                                        .any(|entry| entry.name == name)
                                {
                                    self.sftp_view.remote_selected.insert(name.clone());
                                    self.sftp_view.remote_last_selected = Some(name);
                                }
                                self.sftp_view.init_error = None;
                                // GPUI still carries a session id for tab/UI compatibility, but
                                // the real SFTP owner lives in ConnectionEntry via NodeRouter.
                                if let Ok(event) = self.node_router.bind_sftp_session(
                                    &node_id,
                                    session_id,
                                    Some(cwd),
                                ) {
                                    self.emit_node_event(event);
                                }
                                self.spawn_sftp_background_transfer_load(node_id.clone());
                                self.spawn_sftp_incomplete_load(node_id);
                            }
                            Err(error) => {
                                if oxideterm_sftp::error_should_retry_initialization(&error)
                                    && self.sftp_view.remote_load_retry_count < 3
                                {
                                    // Tauri's node_sftp_list_dir retry path only
                                    // rebuilds the SFTP channel through
                                    // NodeRouter/ConnectionEntry; it does not
                                    // let SFTP own SSH liveness. Native keeps
                                    // the same boundary here: queue a delayed
                                    // list retry, but leave SSH reconnect/start
                                    // decisions to the node owner.
                                    self.sftp_view.remote_load_retry_count += 1;
                                    let attempt = self.sftp_view.remote_load_retry_count;
                                    self.schedule_sftp_remote_load_retry(
                                        tab_id, node_id, path, attempt, cx,
                                    );
                                    self.sftp_view.remote_loading = true;
                                    self.sftp_view.init_error = None;
                                } else {
                                    self.sftp_view.remote_load_retry_count = 0;
                                    if oxideterm_sftp::error_is_permission_denied(&error) {
                                        if let Some(previous_path) =
                                            self.sftp_path_memory.get(&node_id).cloned()
                                        {
                                            self.sftp_view.remote_path = previous_path.clone();
                                            self.sftp_view.remote_path_input = previous_path;
                                        }
                                    } else if oxideterm_sftp::error_is_not_found(&error) {
                                        self.sftp_view.remote_path = "/".to_string();
                                        self.sftp_view.remote_path_input = "/".to_string();
                                        self.sftp_path_memory.insert(node_id, "/".to_string());
                                        if path != "/" {
                                            self.request_sftp_remote_load();
                                        }
                                    }
                                    self.sftp_view.init_error = Some(format!("{}: {error}", path));
                                }
                            }
                        }
                    }
                }
                SftpWorkerResult::RemotePathCompletion {
                    generation,
                    node_id,
                    parent_path,
                    result,
                } => {
                    if self.sftp_view_node.as_ref() == Some(&node_id) {
                        let entries = result.unwrap_or_default();
                        changed |= self.sftp_view.remote_path_completion.apply_entries(
                            generation,
                            &parent_path,
                            entries,
                        );
                    }
                }
                SftpWorkerResult::TransferProgress {
                    id,
                    transferred,
                    total,
                    speed,
                    state: _state,
                    error: _error,
                } => {
                    if let Some(item) = self
                        .sftp_view
                        .transfers
                        .iter_mut()
                        .find(|item| item.id == id)
                    {
                        changed |= apply_tauri_transfer_progress(item, transferred, total, speed);
                    }
                }
                SftpWorkerResult::TransferProtocolResolved { id, protocol } => {
                    if let Some(item) = self
                        .sftp_view
                        .transfers
                        .iter_mut()
                        .find(|item| item.id == id)
                    {
                        changed |= item.protocol != protocol;
                        item.protocol = protocol;
                    }
                }
                SftpWorkerResult::TransferComplete {
                    node_id,
                    transfer_id,
                    id,
                    result,
                    refresh_remote,
                    refresh_local,
                } => {
                    self.on_sftp_transfer_finished_for_reconnect(
                        &node_id,
                        &transfer_id,
                        result.is_ok(),
                        cx,
                    );
                    let mut batch_update = None;
                    let should_refresh = if let Some(item) = self
                        .sftp_view
                        .transfers
                        .iter_mut()
                        .find(|item| item.id == id)
                    {
                        let should_refresh = apply_tauri_transfer_completion(item, &result);
                        batch_update = item.batch_id.map(|batch_id| (batch_id, item.state));
                        should_refresh
                    } else {
                        result.is_ok()
                    };
                    if let Some((batch_id, state)) = batch_update {
                        self.update_sftp_transfer_batch_toast(batch_id, state);
                    }
                    let active_sftp_node = self
                        .main_window_tabs
                        .active_tab_id
                        .and_then(|tab_id| self.sftp_tab_nodes.get(&tab_id))
                        .cloned();
                    if active_sftp_node.as_ref() == Some(&node_id)
                        && should_refresh
                        && refresh_remote
                    {
                        self.request_sftp_remote_load();
                    }
                    if active_sftp_node.as_ref() == Some(&node_id)
                        && should_refresh
                        && refresh_local
                        && let Ok(files) = list_local_files(&self.sftp_view.local_path)
                    {
                        self.sftp_view.local_files = files;
                    }
                    if let Some(node_id) = self
                        .main_window_tabs
                        .active_tab_id
                        .and_then(|tab_id| self.sftp_tab_nodes.get(&tab_id))
                        .cloned()
                    {
                        self.spawn_sftp_incomplete_load(node_id);
                    }
                    changed = true;
                }
                SftpWorkerResult::ResumeIncompleteTransferLoaded {
                    node_id,
                    transfer_id,
                    result,
                } => {
                    match result {
                        Ok(progress) if progress.is_incomplete() => {
                            if !self.queue_sftp_resume_transfer_for_node(node_id.clone(), progress)
                            {
                                self.on_sftp_transfer_finished_for_reconnect(
                                    &node_id,
                                    &transfer_id,
                                    false,
                                    cx,
                                );
                            }
                        }
                        Ok(_) | Err(_) => {
                            self.on_sftp_transfer_finished_for_reconnect(
                                &node_id,
                                &transfer_id,
                                false,
                                cx,
                            );
                        }
                    }
                    changed = true;
                }
                SftpWorkerResult::RemoteMutationComplete {
                    result,
                    refresh_remote,
                    refresh_local,
                    toast,
                } => {
                    match result {
                        Ok(()) => {
                            if let Some(toast) = toast {
                                self.push_sftp_toast(
                                    toast.success_title,
                                    toast.success_description,
                                    TerminalNoticeVariant::Success,
                                );
                            }
                        }
                        Err(error) => {
                            if let Some(toast) = toast {
                                self.push_sftp_toast(
                                    toast.error_title,
                                    Some(error),
                                    TerminalNoticeVariant::Error,
                                );
                            } else {
                                self.sftp_view.init_error = Some(error);
                            }
                        }
                    }
                    if refresh_remote {
                        self.request_sftp_remote_load();
                    }
                    if refresh_local && let Ok(files) = list_local_files(&self.sftp_view.local_path)
                    {
                        self.sftp_view.local_files = files;
                    }
                    changed = true;
                }
                SftpWorkerResult::IncompleteTransfersLoaded { node_id, result } => {
                    if self
                        .main_window_tabs
                        .active_tab_id
                        .and_then(|tab_id| self.sftp_tab_nodes.get(&tab_id))
                        != Some(&node_id)
                    {
                        return;
                    }
                    self.sftp_view.incomplete_load_inflight = false;
                    match result {
                        Ok(transfers) => {
                            self.sftp_view.incomplete_transfers = transfers
                                .into_iter()
                                .filter(StoredTransferProgress::is_incomplete)
                                .collect();
                            if self.sftp_view.incomplete_transfers.is_empty() {
                                self.sftp_view.show_incomplete = false;
                            }
                        }
                        Err(error) => {
                            if !is_sftp_incomplete_store_compat_error(&error) {
                                self.sftp_view.init_error = Some(error);
                            }
                            self.sftp_view.incomplete_transfers.clear();
                            self.sftp_view.show_incomplete = false;
                        }
                    }
                    changed = true;
                }
                SftpWorkerResult::BackgroundTransfersLoaded { node_id, result } => {
                    if self
                        .main_window_tabs
                        .active_tab_id
                        .and_then(|tab_id| self.sftp_tab_nodes.get(&tab_id))
                        != Some(&node_id)
                    {
                        return;
                    }
                    match result {
                        Ok(snapshots) => {
                            for snapshot in snapshots {
                                self.upsert_sftp_background_transfer_snapshot(snapshot);
                            }
                        }
                        Err(error) => {
                            self.sftp_view.init_error = Some(error);
                        }
                    }
                    changed = true;
                }
                SftpWorkerResult::PreviewLoaded {
                    generation,
                    path,
                    result,
                } => {
                    // Preview loads race with quick file switching and dialog close. Match
                    // Tauri's current-preview ownership by dropping stale worker completions.
                    if generation != self.sftp_view.preview_generation {
                        return;
                    }
                    self.sftp_view.preview_loading = false;
                    self.sftp_view.preview_hex_loading_more = false;
                    self.sftp_view.preview_path = Some(path);
                    match result {
                        Ok(content) => {
                            let asset_owner =
                                PreviewAssetOwner::from_asset_content_owned_temp(&content);
                            if let Some(owner) = asset_owner.as_ref() {
                                match owner.kind() {
                                    AssetFileKind::Audio => {
                                        let _ = self.sftp_view.preview_audio.load(owner.path());
                                    }
                                    AssetFileKind::Font => match std::fs::read(owner.path()) {
                                        Ok(bytes) => {
                                            let family = font_family_name_from_bytes(&bytes)
                                                .or_else(|| {
                                                    owner
                                                        .path()
                                                        .file_stem()
                                                        .and_then(|name| name.to_str())
                                                        .map(str::to_string)
                                                });
                                            match cx
                                                .text_system()
                                                .add_fonts(vec![Cow::Owned(bytes)])
                                            {
                                                Ok(()) => {
                                                    self.sftp_view.preview_font_family = family;
                                                    self.sftp_view.preview_font_error = None;
                                                }
                                                Err(error) => {
                                                    self.sftp_view.preview_font_family = None;
                                                    self.sftp_view.preview_font_error =
                                                        Some(error.to_string());
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            self.sftp_view.preview_font_family = None;
                                            self.sftp_view.preview_font_error =
                                                Some(error.to_string());
                                        }
                                    },
                                    AssetFileKind::Image
                                    | AssetFileKind::Video
                                    | AssetFileKind::Office => {}
                                }
                            }
                            self.sftp_view.preview_session =
                                PreviewSession::ready(content.clone(), asset_owner.clone());
                            self.sftp_view.preview_asset_owner = asset_owner;
                            self.sftp_view.preview_content = Some(content);
                            self.sftp_view.preview_error = None;
                        }
                        Err(error) => {
                            self.sftp_view.preview_content = None;
                            self.sftp_view.preview_asset_owner = None;
                            self.sftp_view.preview_session = PreviewSession::error(error.clone());
                            self.sftp_view.preview_error = Some(error);
                        }
                    }
                    changed = true;
                }
                SftpWorkerResult::PreviewHexLoaded {
                    generation,
                    path,
                    offset: _offset,
                    result,
                } => {
                    if generation != self.sftp_view.preview_generation {
                        return;
                    }
                    self.sftp_view.preview_hex_loading_more = false;
                    match result {
                        Ok(PreviewContent::Hex {
                            data: next_data,
                            total_size: next_total_size,
                            offset: next_offset,
                            chunk_size: next_chunk_size,
                            has_more: next_has_more,
                        }) => {
                            if self.sftp_view.preview_path.as_deref() == Some(path.as_str())
                                && let Some(PreviewContent::Hex {
                                    data,
                                    total_size,
                                    offset,
                                    chunk_size,
                                    has_more,
                                }) = self.sftp_view.preview_content.as_mut()
                            {
                                data.push_str(&next_data);
                                *total_size = next_total_size;
                                *offset = next_offset;
                                *chunk_size = next_chunk_size;
                                *has_more = next_has_more;
                                self.sftp_view.preview_error = None;
                            }
                        }
                        Ok(other) => {
                            self.sftp_view.preview_error = Some(format!(
                                "{}: {}",
                                self.i18n.t("sftp.toast.load_more_failed"),
                                preview_content_text(&other)
                            ));
                        }
                        Err(error) => {
                            self.sftp_view.preview_error = Some(format!(
                                "{}: {}",
                                self.i18n.t("sftp.toast.load_more_failed"),
                                error
                            ));
                        }
                    }
                    changed = true;
                }
                SftpWorkerResult::PreviewSaved {
                    generation,
                    path,
                    content,
                    encoding: _encoding,
                    result,
                } => {
                    if generation != self.sftp_view.preview_generation {
                        return;
                    }
                    self.sftp_view.preview_editor_saving = false;
                    match result {
                        Ok(saved) => {
                            let saved_content = content.clone();
                            self.sftp_view.preview_editor_dirty = false;
                            self.sftp_view.preview_editor_initial_content = saved_content.clone();
                            self.sftp_view.preview_editor_observed_content = saved_content;
                            self.sftp_view.preview_editor_save_error = None;
                            self.sftp_view.preview_editor_network_error = false;
                            self.sftp_view.preview_editor_retry_count = 0;
                            self.sftp_view.preview_editor_last_saved_mtime = saved.mtime;
                            self.sftp_view.preview_editor_last_atomic_write =
                                Some(saved.atomic_write);
                            self.sftp_view.preview_editor_encoding = saved.encoding_used.clone();
                            self.sftp_view.preview_path = Some(path.clone());
                            if let Some(editor) = self.sftp_view.preview_editor.clone() {
                                editor.update(cx, |editor, cx| editor.mark_saved_external(cx));
                            }
                            if let Some(PreviewContent::Text {
                                data,
                                encoding: current_encoding,
                                ..
                            }) = self.sftp_view.preview_content.as_mut()
                            {
                                *data = restore_text_line_endings(
                                    &content,
                                    self.sftp_view.preview_editor_line_ending,
                                );
                                *current_encoding = saved.encoding_used.clone();
                            }
                            if let Some(file) = self
                                .sftp_view
                                .remote_files
                                .iter_mut()
                                .find(|file| file.path == path)
                            {
                                if let Some(size) = saved.size {
                                    file.size = size;
                                }
                                file.modified = saved.mtime.map(|mtime| mtime as i64);
                            }
                            self.request_sftp_remote_load();
                        }
                        Err(error) => {
                            if sftp_preview_editor_is_network_error(&error) {
                                self.sftp_view.preview_editor_network_error = true;
                                self.sftp_view.preview_editor_save_error =
                                    Some(self.i18n.t("sftp.preview.network_error"));
                            } else {
                                self.sftp_view.preview_editor_network_error = false;
                                self.sftp_view.preview_editor_save_error = Some(error);
                            }
                        }
                    }
                    changed = true;
                }
            }
            changed
        };
        if changed {
            cx.notify();
        }
    }

    fn schedule_sftp_remote_load_retry(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        path: String,
        attempt: u8,
        cx: &mut Context<Self>,
    ) {
        let delay_secs = 2_u64.saturating_pow(attempt as u32);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(delay_secs))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .sftp_tab_nodes
                    .get(&tab_id)
                    .is_some_and(|tab_node_id| tab_node_id == &node_id)
                    && this.sftp_view_node.as_ref() == Some(&node_id)
                    && this.sftp_view.remote_path == path
                    && !this.sftp_view.remote_load_inflight
                {
                    // Hidden SFTP views keep the retry pending; tab activation
                    // sends another ordered wake when the view becomes visible.
                    this.request_sftp_remote_load();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn apply_sftp_ready_event(
        &mut self,
        node_id: &NodeId,
        ready: bool,
        cwd: Option<String>,
    ) {
        if self
            .main_window_tabs
            .active_tab_id
            .and_then(|tab_id| self.sftp_tab_nodes.get(&tab_id))
            != Some(node_id)
        {
            return;
        }
        self.sftp_view.remote_loading = !ready;
        if let Some(cwd) = cwd {
            self.sftp_view.remote_path = cwd.clone();
            self.sftp_view.remote_path_input = cwd;
        }
    }
}

#[cfg(test)]
mod remote_load_state_tests {
    use super::*;

    #[test]
    fn hidden_current_view_completion_clears_inflight_before_return() {
        let loading = SftpRemoteLoadState::default().request().start().unwrap();
        let completion = classify_sftp_remote_list_completion(true, true, false);

        let completed = loading.complete();

        assert_eq!(
            completion,
            SftpRemoteListCompletionContext::CurrentHiddenView
        );
        assert!(completion.should_apply());
        assert_eq!(completed, SftpRemoteLoadState::default());

        let returned_view = classify_sftp_remote_list_completion(true, true, true);
        assert_eq!(
            returned_view,
            SftpRemoteListCompletionContext::CurrentVisibleView
        );
        assert!(!completed.inflight);
    }

    #[test]
    fn switching_sftp_views_waits_for_old_request_then_starts_pending_view() {
        let old_request = SftpRemoteLoadState::default().request().start().unwrap();
        let switched_view = old_request.request();
        let completion = classify_sftp_remote_list_completion(true, false, false);

        let old_request_completed = switched_view.complete();

        assert_eq!(completion, SftpRemoteListCompletionContext::StaleView);
        assert!(!completion.should_apply());
        assert_eq!(
            old_request_completed,
            SftpRemoteLoadState {
                loading: true,
                pending: true,
                inflight: false,
            }
        );
        assert!(old_request_completed.start().is_some());
    }

    #[test]
    fn hidden_pending_load_starts_after_activation_wake() {
        let hidden_pending = SftpRemoteLoadState::default().request();

        let reactivated = hidden_pending.start().unwrap();

        assert_eq!(
            reactivated,
            SftpRemoteLoadState {
                loading: true,
                pending: false,
                inflight: true,
            }
        );
    }
}

fn apply_tauri_transfer_progress(
    item: &mut SftpTransferItem,
    transferred: u64,
    total: u64,
    speed: u64,
) -> bool {
    if matches!(
        item.state,
        SftpTransferState::Completed | SftpTransferState::Cancelled | SftpTransferState::Error
    ) {
        return false;
    }

    item.transferred = transferred;
    // Tauri's transferStore.updateProgress preserves the original size for
    // indeterminate tar/streaming progress where total=0; completion arrives
    // through sftp:complete instead of this progress event.
    if total > 0 {
        item.size = total;
    }
    item.speed = speed;
    item.state = if item.state == SftpTransferState::Paused {
        SftpTransferState::Paused
    } else if total > 0 && transferred >= total {
        SftpTransferState::Completed
    } else {
        SftpTransferState::Active
    };
    true
}

fn apply_tauri_transfer_completion(
    item: &mut SftpTransferItem,
    result: &Result<(), String>,
) -> bool {
    match result {
        Ok(()) => {
            item.transferred = item.size;
            item.state = SftpTransferState::Completed;
            item.error = None;
            true
        }
        Err(_error) if item.state == SftpTransferState::Cancelled => {
            // resolveTransferCompletionUpdate() in the Tauri SFTP view drops a
            // late failure for a user-cancelled transfer so the queue does not
            // flicker back to "error" after the cancellation wins.
            false
        }
        Err(error) => {
            item.state = SftpTransferState::Error;
            item.error = Some(error.clone());
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transfer_item(state: SftpTransferState) -> SftpTransferItem {
        SftpTransferItem {
            id: 1,
            transfer_id: "tx-1".to_string(),
            batch_id: None,
            node_id: NodeId::new("node-1"),
            name: "file.txt".to_string(),
            local_path: "/tmp/file.txt".to_string(),
            remote_path: "/home/file.txt".to_string(),
            direction: SftpTransferDirection::Upload,
            protocol: RemoteTransferProtocol::Sftp,
            size: 500,
            transferred: 0,
            speed: 0,
            state,
            error: None,
        }
    }

    #[test]
    fn transfer_progress_preserves_paused_state_like_tauri_store() {
        let mut item = transfer_item(SftpTransferState::Paused);

        assert!(apply_tauri_transfer_progress(&mut item, 250, 500, 42));

        assert_eq!(item.state, SftpTransferState::Paused);
        assert_eq!(item.transferred, 250);
        assert_eq!(item.speed, 42);
    }

    #[test]
    fn transfer_progress_ignores_terminal_state_like_tauri_store() {
        let mut item = transfer_item(SftpTransferState::Completed);
        item.transferred = 500;

        assert!(!apply_tauri_transfer_progress(&mut item, 250, 500, 42));

        assert_eq!(item.state, SftpTransferState::Completed);
        assert_eq!(item.transferred, 500);
        assert_eq!(item.speed, 0);
    }

    #[test]
    fn transfer_progress_keeps_indeterminate_size_until_complete_event() {
        let mut item = transfer_item(SftpTransferState::Pending);
        item.size = 0;

        assert!(apply_tauri_transfer_progress(&mut item, 2048, 0, 512));

        assert_eq!(item.state, SftpTransferState::Active);
        assert_eq!(item.size, 0);
        assert_eq!(item.transferred, 2048);
    }

    #[test]
    fn transfer_completion_preserves_cancelled_late_failure_like_tauri_view() {
        let mut item = transfer_item(SftpTransferState::Cancelled);

        assert!(!apply_tauri_transfer_completion(
            &mut item,
            &Err("late failure".to_string())
        ));

        assert_eq!(item.state, SftpTransferState::Cancelled);
        assert_eq!(item.error, None);
    }

    #[test]
    fn stale_node_sftp_errors_are_connection_unavailable() {
        assert!(oxideterm_sftp::error_is_connection_unavailable(
            "Connection abc is stale: transport is closed"
        ));
        assert!(oxideterm_sftp::error_is_connection_unavailable(
            "SFTP init failed: Channel error: SSH connection is closed and cannot open an SFTP channel"
        ));
        assert!(oxideterm_sftp::error_is_connection_unavailable(
            "Capability unavailable: Session not found: node-1"
        ));
        assert!(oxideterm_sftp::error_is_connection_unavailable(
            "SFTP subsystem not available: failed to open SFTP channel: channel closed"
        ));
        assert!(!oxideterm_sftp::error_is_connection_unavailable(
            "Permission denied: /home/me/secret"
        ));
    }

    #[test]
    fn sftp_retry_classifier_matches_tauri_error_classes() {
        assert!(oxideterm_sftp::error_should_retry_initialization(
            "SFTP subsystem not available: failed to open SFTP channel: channel closed"
        ));
        assert!(oxideterm_sftp::error_should_retry_initialization(
            "Connection timeout while opening SFTP"
        ));

        assert!(!oxideterm_sftp::error_should_retry_initialization(
            "Authentication failed: Permission denied (publickey,password)"
        ));
        assert!(!oxideterm_sftp::error_should_retry_initialization(
            "Permission denied: /home/me/secret"
        ));
        assert!(!oxideterm_sftp::error_should_retry_initialization(
            "Directory not found: /home/me/missing"
        ));
        assert!(!oxideterm_sftp::error_should_retry_initialization(
            "SFTP subsystem not available: server disabled subsystem"
        ));
    }

    #[test]
    fn sftp_path_not_found_classifier_does_not_catch_dead_sessions() {
        assert!(oxideterm_sftp::error_is_not_found(
            "Directory not found: /home/me/missing"
        ));
        assert!(oxideterm_sftp::error_is_not_found(
            "No such file or directory: /home/me/missing"
        ));

        assert!(!oxideterm_sftp::error_is_not_found(
            "Capability unavailable: Session not found: node-1"
        ));
        assert!(!oxideterm_sftp::error_is_not_found(
            "Node not found: node-1"
        ));
    }

    #[test]
    fn sftp_auth_failure_is_not_path_permission_denied() {
        assert!(oxideterm_sftp::error_is_auth_failure(
            "Authentication failed: Permission denied (publickey,password)"
        ));
        assert!(!oxideterm_sftp::error_is_permission_denied(
            "Authentication failed: Permission denied (publickey,password)"
        ));
        assert!(oxideterm_sftp::error_is_permission_denied(
            "Permission denied: /home/me/secret"
        ));
    }
}
