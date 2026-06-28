impl WorkspaceApp {
    pub(super) fn open_sftp_tab(
        &mut self,
        node_id: NodeId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

        self.main_window_tabs.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.active_ssh_node_id = Some(node_id.clone());
        self.activate_sftp_view_for_node(&node_id);
        // Opening the SFTP surface mirrors Tauri's createTab path: it does
        // not start SSH. The SFTP worker consumes an already-connected node
        // and reports the router's not-connected error when the node is down.
        self.sftp_view.remote_load_pending = true;
        cx.notify();
    }

    pub(super) fn activate_sftp_view_for_node(&mut self, node_id: &NodeId) {
        if self.sftp_view_node.as_ref() == Some(node_id) {
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
        self.sftp_view.local_files = list_local_files(&local_path).unwrap_or_default();
        self.sftp_view.local_selected.clear();
        self.sftp_view.local_last_selected = None;
        self.sftp_view.local_path_scroll_x = 0.0;

        let remembered_remote = self
            .sftp_path_memory
            .get(node_id)
            .cloned()
            .unwrap_or_default();
        self.sftp_view.remote_path = remembered_remote.clone();
        self.sftp_view.remote_path_input = remembered_remote;
        self.sftp_view.remote_files.clear();
        self.sftp_view.remote_selected.clear();
        self.sftp_view.remote_last_selected = None;
        self.sftp_view.remote_path_scroll_x = 0.0;
        self.sftp_view.remote_load_pending = true;
        self.sftp_view.remote_load_inflight = false;
        self.sftp_view.remote_load_retry_count = 0;
        self.sftp_view.init_error = None;
    }

    pub(super) fn maybe_start_sftp_remote_load(&mut self, cx: &mut Context<Self>) {
        if self.sftp_view.remote_load_inflight || !self.sftp_view.remote_load_pending {
            return;
        }
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
            return;
        };
        if self
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .is_none_or(|tab| tab.kind != TabKind::Sftp)
        {
            return;
        }
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let path = self.sftp_view.remote_path.clone();
        self.start_sftp_remote_load(tab_id, node_id, path, cx);
    }

    fn start_sftp_remote_load(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let session_id = format!("node:{}:sftp", node_id.0);
        self.sftp_view.remote_loading = true;
        self.sftp_view.remote_load_pending = false;
        self.sftp_view.remote_load_inflight = true;
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

    pub(super) fn poll_sftp_worker_results(&mut self, cx: &mut Context<Self>) {
        let mut results = Vec::new();
        while let Ok(result) = self.sftp_worker_rx.try_recv() {
            results.push(result);
        }

        let mut changed = false;
        for result in results {
            match result {
                SftpWorkerResult::RemoteList {
                    tab_id,
                    node_id,
                    session_id,
                    path,
                    result,
                } => {
                    if Some(tab_id) == self.main_window_tabs.active_tab_id {
                        self.sftp_view.remote_load_inflight = false;
                        self.sftp_view.remote_loading = false;
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
                                if sftp_error_should_retry_init(&error)
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
                                        tab_id,
                                        node_id.clone(),
                                        path.clone(),
                                        attempt,
                                        cx,
                                    );
                                    self.sftp_view.remote_loading = true;
                                    self.sftp_view.init_error = None;
                                } else {
                                    self.sftp_view.remote_load_retry_count = 0;
                                    if sftp_error_is_permission_denied(&error) {
                                        if let Some(previous_path) =
                                            self.sftp_path_memory.get(&node_id).cloned()
                                        {
                                            self.sftp_view.remote_path = previous_path.clone();
                                            self.sftp_view.remote_path_input = previous_path;
                                        }
                                    } else if sftp_error_is_not_found(&error) {
                                        self.sftp_view.remote_path = "/".to_string();
                                        self.sftp_view.remote_path_input = "/".to_string();
                                        self.sftp_path_memory
                                            .insert(node_id.clone(), "/".to_string());
                                        if path != "/" {
                                            self.sftp_view.remote_load_pending = true;
                                        }
                                    }
                                    self.sftp_view.init_error =
                                        Some(format!("{}: {error}", path));
                                }
                            }
                        }
                        changed = true;
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
                        self.sftp_view.remote_load_pending = true;
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
                        self.sftp_view.remote_load_pending = true;
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
                        continue;
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
                        continue;
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
                        continue;
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
                                    AssetFileKind::Font => {
                                        match std::fs::read(owner.path()) {
                                            Ok(bytes) => {
                                                let family = font_family_name_from_bytes(&bytes)
                                                    .or_else(|| {
                                                        owner
                                                            .path()
                                                            .file_stem()
                                                            .and_then(|name| name.to_str())
                                                            .map(str::to_string)
                                                    });
                                                match cx.text_system().add_fonts(vec![Cow::Owned(bytes)]) {
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
                                        }
                                    }
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
                        continue;
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
                        continue;
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
                                *data = content;
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
                            self.sftp_view.remote_load_pending = true;
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
        }
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
                if this.main_window_tabs.active_tab_id == Some(tab_id)
                    && this
                        .sftp_tab_nodes
                        .get(&tab_id)
                        .is_some_and(|active_node_id| active_node_id == &node_id)
                    && this.sftp_view.remote_path == path
                    && !this.sftp_view.remote_load_inflight
                {
                    this.sftp_view.remote_load_pending = true;
                    this.sftp_view.remote_loading = true;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn apply_sftp_ready_event(
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

/// Classifies capability failures caused by an unavailable SSH connection.
///
/// This deliberately matches transport ownership failures, not ordinary SFTP
/// errors such as permissions or missing files. SFTP may retry its own listing
/// work while the node owner reconnects, but it must not start/revive SSH
/// liveness by itself.
fn sftp_error_is_connection_unavailable(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("stale")
        || lower.contains("link_down")
        || lower.contains("link down")
        || lower.contains("disconnected")
        || lower.contains("transport is closed")
        || lower.contains("transport is missing")
        || lower.contains("ssh connection is closed")
        || lower.contains("connection closed")
        || lower.contains("connection reset")
        || lower.contains("reset by peer")
        || lower.contains("broken pipe")
        || lower.contains("unexpected eof")
        || lower.contains("channel closed")
        || lower.contains("closed channel")
        || lower.contains("no active ssh connection")
        || lower.contains("session not found")
        || lower.contains("not initialized")
}

fn sftp_error_should_retry_init(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    if sftp_error_is_auth_failure(error)
        || sftp_error_is_permission_denied(error)
        || sftp_error_is_not_found(error)
    {
        return false;
    }

    sftp_error_is_connection_unavailable(error)
        || lower.contains("not connected")
        || lower.contains("connection timeout")
        || lower.contains("timeout")
}

fn sftp_error_is_permission_denied(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    !sftp_error_is_auth_failure(error)
        && (lower.contains("permission denied") || lower.contains("permissiondenied"))
}

fn sftp_error_is_not_found(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    if lower.contains("session not found")
        || lower.contains("node not found")
        || lower.contains("connection not found")
    {
        return false;
    }
    lower.contains("file not found")
        || lower.contains("directory not found")
        || lower.contains("path not found")
        || lower.contains("no such file")
        || lower.contains("no such directory")
        || lower.contains("no such path")
        || lower.contains("filenotfound")
        || lower.contains("directorynotfound")
        || lower.contains("pathnotfound")
        || lower.contains("nosuchfile")
        || lower.contains("nosuchdirectory")
}

fn sftp_error_is_auth_failure(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("authentication failed")
        || lower.contains("auth failed")
        || lower.contains("permission denied (publickey")
        || lower.contains("permission denied (password")
        || lower.contains("permission denied (keyboard-interactive")
        || lower.contains("all authentication methods failed")
        || lower.contains("agent authentication failed")
        || lower.contains("keyboard-interactive")
        || lower.contains("password authentication timed out")
        || lower.contains("host key")
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
        assert!(sftp_error_is_connection_unavailable(
            "Connection abc is stale: transport is closed"
        ));
        assert!(sftp_error_is_connection_unavailable(
            "SFTP init failed: Channel error: SSH connection is closed and cannot open an SFTP channel"
        ));
        assert!(sftp_error_is_connection_unavailable(
            "Capability unavailable: Session not found: node-1"
        ));
        assert!(sftp_error_is_connection_unavailable(
            "SFTP subsystem not available: failed to open SFTP channel: channel closed"
        ));
        assert!(!sftp_error_is_connection_unavailable(
            "Permission denied: /home/me/secret"
        ));
    }

    #[test]
    fn sftp_retry_classifier_matches_tauri_error_classes() {
        assert!(sftp_error_should_retry_init(
            "SFTP subsystem not available: failed to open SFTP channel: channel closed"
        ));
        assert!(sftp_error_should_retry_init(
            "Connection timeout while opening SFTP"
        ));

        assert!(!sftp_error_should_retry_init(
            "Authentication failed: Permission denied (publickey,password)"
        ));
        assert!(!sftp_error_should_retry_init(
            "Permission denied: /home/me/secret"
        ));
        assert!(!sftp_error_should_retry_init(
            "Directory not found: /home/me/missing"
        ));
        assert!(!sftp_error_should_retry_init(
            "SFTP subsystem not available: server disabled subsystem"
        ));
    }

    #[test]
    fn sftp_path_not_found_classifier_does_not_catch_dead_sessions() {
        assert!(sftp_error_is_not_found(
            "Directory not found: /home/me/missing"
        ));
        assert!(sftp_error_is_not_found(
            "No such file or directory: /home/me/missing"
        ));

        assert!(!sftp_error_is_not_found(
            "Capability unavailable: Session not found: node-1"
        ));
        assert!(!sftp_error_is_not_found("Node not found: node-1"));
    }

    #[test]
    fn sftp_auth_failure_is_not_path_permission_denied() {
        assert!(sftp_error_is_auth_failure(
            "Authentication failed: Permission denied (publickey,password)"
        ));
        assert!(!sftp_error_is_permission_denied(
            "Authentication failed: Permission denied (publickey,password)"
        ));
        assert!(sftp_error_is_permission_denied(
            "Permission denied: /home/me/secret"
        ));
    }
}
