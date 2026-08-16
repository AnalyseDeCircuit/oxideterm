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

fn remote_list_result_is_superseded(
    requested_path: &str,
    current_path: &str,
    load_pending: bool,
) -> bool {
    // A queued navigation owns the visible path, so an older listing must only release the slot.
    load_pending && requested_path != current_path
}

struct SftpRemoteListOutcome {
    bind_session: Option<(NodeId, String, String)>,
    load_transfer_state_for: Option<NodeId>,
    changed: bool,
}

impl SftpWorkspaceEntity {
    fn remote_load_state(&self) -> SftpRemoteLoadState {
        SftpRemoteLoadState {
            loading: self.remote_loading,
            pending: self.remote_load_pending,
            inflight: self.remote_load_inflight,
        }
    }

    fn set_remote_load_state(&mut self, state: SftpRemoteLoadState) {
        self.remote_loading = state.loading;
        self.remote_load_pending = state.pending;
        self.remote_load_inflight = state.inflight;
    }

    pub(in crate::workspace::sftp) fn request_remote_load(&mut self) {
        let state = self.remote_load_state().request();
        self.set_remote_load_state(state);
    }

    fn start_remote_load(
        &mut self,
        surface_id: SftpSurfaceId,
        node_id: &NodeId,
    ) -> Option<(String, u64)> {
        if self.current_surface_id != Some(surface_id)
            || self.current_node_id.as_ref() != Some(node_id)
        {
            return None;
        }
        let started = self.remote_load_state().start()?;
        self.set_remote_load_state(started);
        self.init_error = None;
        Some((self.remote_path.clone(), self.view_generation))
    }

    pub(in crate::workspace) fn activate_view(
        &mut self,
        surface_id: SftpSurfaceId,
        node_id: NodeId,
    ) {
        self.current_surface_id = Some(surface_id);
        if self.current_node_id.as_ref() == Some(&node_id) {
            // Returning to a hidden view consumes any pending load directly;
            // no workspace heartbeat is involved.
            self.request_remote_load();
            return;
        }

        if let Some(previous_node_id) = self.current_node_id.take() {
            self.local_path_by_node
                .insert(previous_node_id.clone(), self.local_path.clone());
            if !self.remote_path.is_empty() {
                self.remote_path_by_node
                    .insert(previous_node_id, self.remote_path.clone());
            }
        }

        self.current_node_id = Some(node_id.clone());
        self.view_generation = self.view_generation.wrapping_add(1);
        let local_path = self
            .local_path_by_node
            .get(&node_id)
            .cloned()
            .unwrap_or_else(default_download_path);
        self.apply_local_path(local_path);

        let remembered_remote = self
            .remote_path_by_node
            .get(&node_id)
            .cloned()
            .unwrap_or_default();
        self.remote_path = remembered_remote.clone();
        self.remote_path_input = remembered_remote;
        self.remote_path_completion.dismiss();
        self.remote_path_completion_pending_selection = None;
        self.remote_files.clear();
        self.remote_selected.clear();
        self.remote_last_selected = None;
        self.remote_path_scroll
            .set_offset(Point::new(px(0.0), px(0.0)));
        // A request already in flight belongs to the previous generation. Its
        // completion releases the shared slot before this pending view starts.
        self.request_remote_load();
        self.remote_load_retry_count = 0;
        self.remote_load_retry_task = None;
        self.init_error = None;
    }

    pub(in crate::workspace) fn deactivate_view(
        &mut self,
        surface_id: SftpSurfaceId,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.current_surface_id != Some(surface_id)
            || self.current_node_id.as_ref() != Some(node_id)
        {
            return false;
        }

        self.local_path_by_node
            .insert(node_id.clone(), self.local_path.clone());
        if !self.remote_path.is_empty() {
            self.remote_path_by_node
                .insert(node_id.clone(), self.remote_path.clone());
        }
        self.current_surface_id = None;
        self.current_node_id = None;
        self.view_generation = self.view_generation.wrapping_add(1);
        self.remote_load_pending = false;
        self.remote_load_retry_count = 0;
        self.remote_load_retry_task = None;
        self.remote_files.clear();
        self.remote_selected.clear();
        self.remote_last_selected = None;
        self.remote_path_completion.dismiss();
        self.remote_path_completion_pending_selection = None;
        self.focused_input = None;
        self.editing_local_path = false;
        self.editing_remote_path = false;
        self.folder_picker_task = None;
        self.drag_state = None;
        self.drag_over_pane = None;
        self.drag_autoscroll_position = None;
        self.drag_autoscroll_scheduled = false;
        self.clear_context_menu_immediately();
        self.begin_dialog_exit(Duration::ZERO, cx);
        // Transfer tasks are node-owned and intentionally survive removal of
        // this UI projection; their existing completion path remains active.
        cx.notify();
        true
    }

    fn apply_remote_list(
        &mut self,
        surface_id: SftpSurfaceId,
        node_id: NodeId,
        view_generation: u64,
        session_id: String,
        path: String,
        result: Result<RemoteSftpListing, String>,
        cx: &mut Context<Self>,
    ) -> SftpRemoteListOutcome {
        self.set_remote_load_state(self.remote_load_state().complete());
        if self.current_surface_id != Some(surface_id)
            || self.current_node_id.as_ref() != Some(&node_id)
            || self.view_generation != view_generation
        {
            return SftpRemoteListOutcome {
                bind_session: None,
                load_transfer_state_for: None,
                changed: true,
            };
        }
        if remote_list_result_is_superseded(&path, &self.remote_path, self.remote_load_pending) {
            return SftpRemoteListOutcome {
                bind_session: None,
                load_transfer_state_for: None,
                changed: true,
            };
        }

        match result {
            Ok(listing) => {
                let cwd = listing.cwd;
                self.remote_path_by_node
                    .insert(node_id.clone(), cwd.clone());
                self.remote_home_by_node
                    .entry(node_id.clone())
                    .or_insert_with(|| cwd.clone());
                self.remote_load_retry_count = 0;
                self.remote_load_retry_task = None;
                self.remote_path.clone_from(&cwd);
                self.remote_path_input.clone_from(&cwd);
                self.remote_files = listing.files;
                self.remote_selected.clear();
                self.remote_last_selected = None;
                if self
                    .remote_path_completion_pending_selection
                    .as_ref()
                    .is_some_and(|(parent_path, _)| parent_path == &cwd)
                    && let Some((_, name)) = self.remote_path_completion_pending_selection.take()
                    && self.remote_files.iter().any(|entry| entry.name == name)
                {
                    self.remote_selected.insert(name.clone());
                    self.remote_last_selected = Some(name);
                }
                self.init_error = None;
                SftpRemoteListOutcome {
                    bind_session: Some((node_id.clone(), session_id, cwd)),
                    load_transfer_state_for: Some(node_id),
                    changed: true,
                }
            }
            Err(error) => {
                if oxideterm_sftp::error_should_retry_initialization(&error)
                    && self.remote_load_retry_count < 3
                {
                    self.remote_load_retry_count += 1;
                    let attempt = self.remote_load_retry_count;
                    self.schedule_remote_load_retry(
                        surface_id,
                        node_id,
                        view_generation,
                        path,
                        attempt,
                        cx,
                    );
                    self.remote_loading = true;
                    self.init_error = None;
                } else {
                    self.remote_load_retry_count = 0;
                    self.remote_load_retry_task = None;
                    if oxideterm_sftp::error_is_permission_denied(&error) {
                        if let Some(previous_path) = self.remote_path_by_node.get(&node_id).cloned()
                        {
                            self.remote_path.clone_from(&previous_path);
                            self.remote_path_input = previous_path;
                        }
                    } else if oxideterm_sftp::error_is_not_found(&error) {
                        self.remote_path = "/".to_string();
                        self.remote_path_input = "/".to_string();
                        self.remote_path_by_node.insert(node_id, "/".to_string());
                        if path != "/" {
                            self.request_remote_load();
                        }
                    }
                    self.init_error = Some(format!("{path}: {error}"));
                }
                SftpRemoteListOutcome {
                    bind_session: None,
                    load_transfer_state_for: None,
                    changed: true,
                }
            }
        }
    }

    fn schedule_remote_load_retry(
        &mut self,
        surface_id: SftpSurfaceId,
        node_id: NodeId,
        view_generation: u64,
        path: String,
        attempt: u8,
        cx: &mut Context<Self>,
    ) {
        let delay = Duration::from_secs(2_u64.saturating_pow(attempt as u32));
        self.remote_load_retry_task = Some(cx.spawn(async move |entity, cx| {
            gpui::Timer::after(delay).await;
            let _ = entity.update(cx, |sftp, cx| {
                sftp.remote_load_retry_task = None;
                if sftp.current_surface_id == Some(surface_id)
                    && sftp.current_node_id.as_ref() == Some(&node_id)
                    && sftp.view_generation == view_generation
                    && sftp.remote_path == path
                    && !sftp.remote_load_inflight
                {
                    // Hidden surfaces retain the pending request; mounting the
                    // owner later calls the same gate without restarting SSH.
                    sftp.request_remote_load();
                    cx.emit(SftpWorkspaceEvent::RemoteLoadReady {
                        surface_id,
                        node_id,
                        delivery: sftp.worker_tx.clone(),
                    });
                    cx.notify();
                }
            });
        }));
    }

    pub(in crate::workspace::sftp) fn reduce_worker_result(
        &mut self,
        result: SftpWorkerResult,
        effects: &mut VecDeque<SftpWorkspaceEffect>,
        cx: &mut Context<Self>,
    ) -> bool {
        match result {
            SftpWorkerResult::StartRemoteLoad {
                surface_id,
                node_id,
            } => {
                let Some((path, view_generation)) = self.start_remote_load(surface_id, &node_id)
                else {
                    return false;
                };
                effects.push_back(SftpWorkspaceEffect::StartRemoteLoad {
                    surface_id,
                    node_id,
                    path,
                    view_generation,
                });
                true
            }
            SftpWorkerResult::RemoteList {
                surface_id,
                node_id,
                view_generation,
                session_id,
                path,
                result,
            } => {
                let outcome = self.apply_remote_list(
                    surface_id,
                    node_id,
                    view_generation,
                    session_id,
                    path,
                    result,
                    cx,
                );
                if let Some((node_id, session_id, cwd)) = outcome.bind_session {
                    effects.push_back(SftpWorkspaceEffect::BindSession {
                        node_id,
                        session_id,
                        cwd,
                    });
                }
                if let Some(node_id) = outcome.load_transfer_state_for {
                    effects.push_back(SftpWorkspaceEffect::LoadBackgroundTransfers {
                        node_id: node_id.clone(),
                    });
                    if self.begin_incomplete_transfer_load(node_id.clone()) {
                        effects.push_back(SftpWorkspaceEffect::LoadIncompleteTransfers { node_id });
                    }
                }
                self.push_remote_load_pending_effect(effects);
                outcome.changed
            }
            SftpWorkerResult::RemotePathCompletion {
                generation,
                node_id,
                parent_path,
                result,
            } => self.apply_remote_path_completion(generation, &node_id, &parent_path, result),
            SftpWorkerResult::TransferProgress {
                id,
                transferred,
                total,
                speed,
            } => self.apply_transfer_progress(id, transferred, total, speed),
            SftpWorkerResult::TransferProtocolResolved { id, protocol } => {
                self.apply_transfer_protocol(id, protocol)
            }
            SftpWorkerResult::TransferComplete {
                node_id,
                transfer_id,
                id,
                result,
                refresh_remote,
                refresh_local,
            } => {
                let success = result.is_ok();
                effects.push_back(SftpWorkspaceEffect::TransferFinishedForReconnect {
                    node_id: node_id.clone(),
                    transfer_id,
                    success,
                });
                let mut batch_update = None;
                let should_refresh =
                    if let Some(item) = self.transfers.iter_mut().find(|item| item.id == id) {
                        let should_refresh = apply_tauri_transfer_completion(item, &result);
                        batch_update = item.batch_id.map(|batch_id| (batch_id, item.state));
                        should_refresh
                    } else {
                        success
                    };
                if let Some((batch_id, state)) = batch_update
                    && let Some(batch) = self.complete_transfer_batch_item(batch_id, state)
                {
                    effects.push_back(SftpWorkspaceEffect::TransferBatchCompleted(batch));
                }
                if self.current_node_id.as_ref() == Some(&node_id) {
                    if should_refresh && refresh_remote {
                        self.request_remote_load();
                        self.push_remote_load_pending_effect(effects);
                    }
                    if should_refresh && refresh_local {
                        effects.push_back(SftpWorkspaceEffect::ReloadLocalDirectory {
                            view_generation: self.view_generation,
                            path: self.local_path.clone(),
                        });
                    }
                    if self.begin_incomplete_transfer_load(node_id.clone()) {
                        effects.push_back(SftpWorkspaceEffect::LoadIncompleteTransfers { node_id });
                    }
                }
                true
            }
            SftpWorkerResult::ResumeIncompleteTransferLoaded {
                node_id,
                transfer_id,
                result,
            } => {
                let launch = match result {
                    Ok(progress) if progress.is_incomplete() => {
                        let show_in_current_view = self.current_node_id.as_ref() == Some(&node_id);
                        self.prepare_reconnect_resume(
                            node_id.clone(),
                            progress,
                            show_in_current_view,
                        )
                    }
                    Ok(_) | Err(_) => None,
                };
                if let Some(launch) = launch {
                    effects.push_back(SftpWorkspaceEffect::StartTransfer(launch));
                } else {
                    effects.push_back(SftpWorkspaceEffect::TransferFinishedForReconnect {
                        node_id,
                        transfer_id,
                        success: false,
                    });
                }
                true
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
                            effects.push_back(SftpWorkspaceEffect::Toast {
                                title: toast.success_title,
                                description: toast.success_description,
                                variant: TerminalNoticeVariant::Success,
                            });
                        }
                    }
                    Err(error) => {
                        if let Some(toast) = toast {
                            effects.push_back(SftpWorkspaceEffect::Toast {
                                title: toast.error_title,
                                description: Some(error),
                                variant: TerminalNoticeVariant::Error,
                            });
                        } else {
                            self.init_error = Some(error);
                        }
                    }
                }
                if refresh_remote {
                    self.request_remote_load();
                    self.push_remote_load_pending_effect(effects);
                }
                if refresh_local {
                    effects.push_back(SftpWorkspaceEffect::ReloadLocalDirectory {
                        view_generation: self.view_generation,
                        path: self.local_path.clone(),
                    });
                }
                true
            }
            SftpWorkerResult::IncompleteTransfersLoaded { node_id, result } => {
                let (changed, next_load) = self.apply_incomplete_transfers(&node_id, result);
                if let Some(node_id) = next_load {
                    effects.push_back(SftpWorkspaceEffect::LoadIncompleteTransfers { node_id });
                }
                changed
            }
            SftpWorkerResult::BackgroundTransfersLoaded { node_id, result } => {
                if self.current_node_id.as_ref() != Some(&node_id) {
                    return false;
                }
                match result {
                    Ok(snapshots) => {
                        for snapshot in snapshots {
                            self.upsert_background_transfer_snapshot(snapshot);
                        }
                    }
                    Err(error) => {
                        self.init_error = Some(error);
                    }
                }
                true
            }
            SftpWorkerResult::PreviewLoaded {
                generation,
                path,
                result,
            } => self.apply_preview_loaded(generation, path, result, cx),
            SftpWorkerResult::PreviewHexLoaded {
                generation,
                path,
                error_prefix,
                result,
            } => self.apply_preview_hex_loaded(generation, &path, result, &error_prefix),
            SftpWorkerResult::PreviewSaved {
                generation,
                path,
                content,
                network_error_message,
                result,
            } => {
                let (changed, refresh_remote) = self.apply_preview_saved(
                    generation,
                    path,
                    content,
                    result,
                    &network_error_message,
                    cx,
                );
                if refresh_remote {
                    self.request_remote_load();
                    self.push_remote_load_pending_effect(effects);
                }
                changed
            }
            SftpWorkerResult::LocalFilesLoaded {
                view_generation,
                path,
                files,
            } => {
                if self.view_generation != view_generation || self.local_path != path {
                    return false;
                }
                self.local_files = files;
                true
            }
        }
    }

    fn push_remote_load_pending_effect(&self, effects: &mut VecDeque<SftpWorkspaceEffect>) {
        if self.remote_load_pending
            && !self.remote_load_inflight
            && let (Some(surface_id), Some(node_id)) =
                (self.current_surface_id, self.current_node_id.clone())
        {
            effects.push_back(SftpWorkspaceEffect::RemoteLoadPending {
                surface_id,
                node_id,
            });
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn request_sftp_remote_load(&mut self, cx: &mut Context<Self>) {
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.request_remote_load();
            cx.notify();
        });
        self.maybe_start_sftp_remote_load(cx);
    }

    pub(in crate::workspace) fn open_sftp_tab(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let remote_path = self.active_ssh_terminal_cwd_path_for_node(&node_id, cx);
        self.open_sftp_with_preference(node_id, remote_path, window, cx);
    }

    fn open_sftp_with_preference(
        &mut self,
        node_id: NodeId,
        remote_path: Option<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.settings_store.settings().sftp.presentation {
            oxideterm_settings::SftpPresentationPreference::Ask => {
                self.sftp_presentation_request = Some(SftpPresentationRequest {
                    node_id,
                    remote_path,
                });
                self.prepare_modal_interaction_boundary(cx);
                cx.notify();
            }
            oxideterm_settings::SftpPresentationPreference::Tab => {
                self.open_sftp_tab_surface(node_id, remote_path, cx);
            }
            oxideterm_settings::SftpPresentationPreference::Sidebar => {
                self.open_sftp_sidebar_surface(node_id, remote_path, cx);
            }
        }
    }

    pub(in crate::workspace) fn open_sftp_tab_surface(
        &mut self,
        node_id: NodeId,
        initial_remote_path: Option<String>,
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
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::Sftp,
                    title,
                    title_source: TabTitleSource::Static,
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            self.sftp_tab_nodes.insert(tab_id, node_id.clone());
            tab_id
        };

        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.active_ssh_node_id = Some(node_id.clone());
        self.activate_sftp_view_for_node(tab_id, &node_id, cx);
        if let Some(path) = initial_remote_path.filter(|path| !path.trim().is_empty()) {
            // SFTP keeps its own remembered path, but an explicit open from an
            // active SSH terminal can use that pane cwd as the initial folder.
            self.set_sftp_path(SftpPane::Remote, path, cx);
        }
        // Opening the SFTP surface mirrors Tauri's createTab path: it does
        // not start SSH. The SFTP worker consumes an already-connected node
        // and reports the router's not-connected error when the node is down.
        self.request_sftp_remote_load(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_sftp_tab_at_remote_path(
        &mut self,
        node_id: NodeId,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let remote_path = (!path.trim().is_empty()).then_some(path);
        self.open_sftp_with_preference(node_id, remote_path, window, cx);
    }

    pub(in crate::workspace) fn choose_sftp_presentation(
        &mut self,
        preference: oxideterm_settings::SftpPresentationPreference,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.sftp_presentation_request.take() else {
            return;
        };
        self.edit_settings(|settings| settings.sftp.presentation = preference, cx);
        match preference {
            oxideterm_settings::SftpPresentationPreference::Ask => {}
            oxideterm_settings::SftpPresentationPreference::Tab => {
                self.open_sftp_tab_surface(request.node_id, request.remote_path, cx);
            }
            oxideterm_settings::SftpPresentationPreference::Sidebar => {
                self.open_sftp_sidebar_surface(request.node_id, request.remote_path, cx);
            }
        }
    }

    pub(in crate::workspace) fn open_sftp_sidebar_surface(
        &mut self,
        node_id: NodeId,
        remote_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.embedded_sftp_node_id = Some(node_id.clone());
        self.active_ssh_node_id = Some(node_id.clone());
        self.expanded_ssh_nodes.insert(node_id.clone());
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.activate_view(SftpSurfaceId::Sidebar, node_id);
            cx.notify();
        });
        if let Some(path) = remote_path.filter(|path| !path.trim().is_empty()) {
            self.set_sftp_path(SftpPane::Remote, path, cx);
        }
        // Showing Active Sessions is the visibility boundary that starts the
        // pending node-backed directory request exactly once.
        self.set_sidebar_section(SidebarSection::Sessions, cx);
        // The sidebar is a consumer of the node-owned SFTP channel. Hiding it
        // never releases or disconnects the physical SSH node.
        cx.notify();
    }

    pub(in crate::workspace) fn close_embedded_sftp_for_node(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.embedded_sftp_node_id.as_ref() != Some(node_id) {
            return false;
        }

        self.embedded_sftp_node_id = None;
        self.embedded_sftp_sidebar_resizing = false;
        if self
            .sftp_presentation_request
            .as_ref()
            .is_some_and(|request| &request.node_id == node_id)
        {
            self.sftp_presentation_request = None;
        }
        let deactivated = self.sftp_view.update(cx, |sftp, cx| {
            sftp.deactivate_view(SftpSurfaceId::Sidebar, node_id, cx)
        });
        if deactivated {
            self.ime_marked_text = None;
        }
        // Removing the embedded view releases only its presentation identity.
        // NodeRouter and any transfers, forwards, or sibling terminals retain
        // their independent ownership until their own lifecycle ends.
        cx.notify();
        true
    }

    pub(in crate::workspace) fn activate_embedded_sftp_sidebar_if_visible(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_collapsed
            || self.effective_sidebar_panel_section() != SidebarSection::Sessions
            || self
                .active_tab(cx)
                .is_some_and(|tab| tab.kind == TabKind::Sftp)
        {
            return;
        }
        let Some(node_id) = self.embedded_sftp_node_id.clone() else {
            return;
        };
        let already_active = {
            let sftp = self.sftp_view.read(cx);
            sftp.current_surface_id == Some(SftpSurfaceId::Sidebar)
                && sftp.current_node_id.as_ref() == Some(&node_id)
        };
        if !already_active {
            self.sftp_view.update(cx, |sftp, cx| {
                sftp.activate_view(SftpSurfaceId::Sidebar, node_id);
                cx.notify();
            });
        }
        // Hidden views may already be active while retaining a queued load;
        // every visibility transition must wake that pending request.
        self.maybe_start_sftp_remote_load(cx);
    }

    pub(in crate::workspace) fn activate_sftp_view_for_node(
        &mut self,
        tab_id: TabId,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.activate_view(SftpSurfaceId::Tab(tab_id), node_id.clone());
            cx.notify();
        });
        self.maybe_start_sftp_remote_load(cx);
    }

    pub(in crate::workspace) fn maybe_start_sftp_remote_load(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let (surface_id, node_id) = {
            let sftp = self.sftp_view.read(cx);
            let Some(surface_id) = sftp.current_surface_id else {
                return false;
            };
            let Some(node_id) = sftp.current_node_id.clone() else {
                return false;
            };
            (surface_id, node_id)
        };
        if !self.sftp_surface_is_visible(surface_id, &node_id, cx) {
            return false;
        }
        let Some((path, view_generation)) = self
            .sftp_view
            .update(cx, |sftp, _cx| sftp.start_remote_load(surface_id, &node_id))
        else {
            return false;
        };
        let delivery = self.sftp_view.read(cx).worker_sender();
        self.spawn_sftp_remote_load(surface_id, node_id, path, view_generation, delivery);
        true
    }

    fn spawn_sftp_remote_load(
        &self,
        surface_id: SftpSurfaceId,
        node_id: NodeId,
        path: String,
        view_generation: u64,
        tx: delivery::ActiveDeliverySender<SftpWorkerResult>,
    ) {
        let session_id = format!("node:{}:sftp", node_id.0);
        let runtime = self.forwarding_runtime.clone();
        let router = self.node_router.clone();
        let owner_router = router.clone();
        let owner_node_id = node_id.clone();
        runtime.spawn(async move {
            // Opening a visible SFTP surface creates the concrete shared channel
            // that owns AI file capabilities. Listing remains on its independent
            // transfer channel, so capability registration never blocks it.
            let _ = owner_router.acquire_sftp(&owner_node_id).await;
        });
        runtime.spawn(async move {
            // Tauri node_sftp_* calls do not synchronously borrow a terminal
            // session before starting SFTP work. The worker waits on the
            // node-owned connection and then opens the real SFTP subsystem
            // channel from ConnectionEntry.
            let result = load_remote_sftp_listing(router, &node_id, &path).await;
            let _ = tx.send(SftpWorkerResult::RemoteList {
                surface_id,
                node_id,
                view_generation,
                session_id,
                path,
                result,
            });
        });
    }

    pub(in crate::workspace) fn handle_sftp_worker_effects(
        &mut self,
        effect_batch: &SftpWorkspaceEffects,
        cx: &mut Context<Self>,
    ) {
        let delivery = effect_batch.delivery();
        for effect in effect_batch.take() {
            match effect {
                SftpWorkspaceEffect::BindSession {
                    node_id,
                    session_id,
                    cwd,
                } => {
                    if let Ok(event) =
                        self.node_router
                            .bind_sftp_session(&node_id, session_id, Some(cwd))
                    {
                        // Binding only reports node-owned readiness. It never
                        // grants SFTP authority to start or stop the SSH link.
                        self.emit_node_event(event);
                    }
                }
                SftpWorkspaceEffect::LoadBackgroundTransfers { node_id } => {
                    self.spawn_sftp_background_transfer_load_with_sender(node_id, delivery.clone());
                }
                SftpWorkspaceEffect::LoadIncompleteTransfers { node_id } => {
                    self.spawn_sftp_incomplete_load_with_sender(node_id, delivery.clone());
                }
                SftpWorkspaceEffect::RemoteLoadPending {
                    surface_id,
                    node_id,
                } => {
                    if self.sftp_surface_is_visible(surface_id, &node_id, cx) {
                        let _ = delivery.send(SftpWorkerResult::StartRemoteLoad {
                            surface_id,
                            node_id,
                        });
                    }
                }
                SftpWorkspaceEffect::StartRemoteLoad {
                    surface_id,
                    node_id,
                    path,
                    view_generation,
                } => {
                    self.spawn_sftp_remote_load(
                        surface_id,
                        node_id,
                        path,
                        view_generation,
                        delivery.clone(),
                    );
                }
                SftpWorkspaceEffect::TransferFinishedForReconnect {
                    node_id,
                    transfer_id,
                    success,
                } => {
                    self.on_sftp_transfer_finished_for_reconnect(
                        &node_id,
                        &transfer_id,
                        success,
                        cx,
                    );
                }
                SftpWorkspaceEffect::TransferBatchCompleted(batch) => {
                    self.show_sftp_transfer_batch_toast(batch, cx);
                }
                SftpWorkspaceEffect::StartTransfer(launch) => {
                    self.spawn_sftp_transfer_launch_with_sender(launch, delivery.clone());
                }
                SftpWorkspaceEffect::Toast {
                    title,
                    description,
                    variant,
                } => {
                    self.push_sftp_toast(title, description, variant, cx);
                }
                SftpWorkspaceEffect::ReloadLocalDirectory {
                    view_generation,
                    path,
                } => {
                    if let Ok(files) = list_local_files(&path) {
                        let _ = delivery.send(SftpWorkerResult::LocalFilesLoaded {
                            view_generation,
                            path,
                            files,
                        });
                    }
                }
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn request_visible_sftp_remote_load(
        &self,
        surface_id: SftpSurfaceId,
        node_id: NodeId,
        delivery: delivery::ActiveDeliverySender<SftpWorkerResult>,
        cx: &App,
    ) {
        if self.sftp_surface_is_visible(surface_id, &node_id, cx) {
            let _ = delivery.send(SftpWorkerResult::StartRemoteLoad {
                surface_id,
                node_id,
            });
        }
    }

    fn sftp_surface_is_visible(
        &self,
        surface_id: SftpSurfaceId,
        node_id: &NodeId,
        cx: &App,
    ) -> bool {
        match surface_id {
            SftpSurfaceId::Tab(tab_id) => {
                self.active_tab_id(cx) == Some(tab_id)
                    && self
                        .tabs(cx)
                        .iter()
                        .any(|tab| tab.id == tab_id && tab.kind == TabKind::Sftp)
                    && self.sftp_tab_nodes.get(&tab_id) == Some(node_id)
            }
            SftpSurfaceId::Sidebar => {
                !self.sidebar_collapsed
                    && self.effective_sidebar_panel_section() == SidebarSection::Sessions
                    && self.embedded_sftp_node_id.as_ref() == Some(node_id)
            }
        }
    }

    pub(in crate::workspace::sftp) fn visible_sftp_node_id(&self, cx: &App) -> Option<NodeId> {
        let sftp = self.sftp_view.read(cx);
        let surface_id = sftp.current_surface_id?;
        let node_id = sftp.current_node_id.clone()?;
        self.sftp_surface_is_visible(surface_id, &node_id, cx)
            .then_some(node_id)
    }

    pub(in crate::workspace) fn apply_sftp_ready_event(
        &mut self,
        node_id: &NodeId,
        ready: bool,
        cwd: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view.update(cx, |sftp, cx| {
            if sftp.current_node_id.as_ref() != Some(node_id) {
                return;
            }
            sftp.apply_router_sftp_ready(ready, cwd);
            cx.notify();
        });
    }
}

impl SftpWorkspaceEntity {
    fn apply_router_sftp_ready(&mut self, ready: bool, cwd: Option<String>) {
        // Channel readiness does not finish a directory load that is pending or still in flight.
        self.remote_loading = !ready || self.remote_load_pending || self.remote_load_inflight;
        if self.remote_load_pending {
            // Readiness can report the shared session's older cwd while explicit navigation waits.
            return;
        }
        if let Some(cwd) = cwd {
            self.remote_path.clone_from(&cwd);
            self.remote_path_input = cwd;
        }
    }

    fn apply_remote_path_completion(
        &mut self,
        generation: u64,
        node_id: &NodeId,
        parent_path: &str,
        result: Result<Vec<PathCompletionCandidate>, String>,
    ) -> bool {
        if self.current_node_id.as_ref() != Some(node_id) {
            return false;
        }
        self.remote_path_completion.apply_entries(
            generation,
            parent_path,
            result.unwrap_or_default(),
        )
    }

    fn apply_transfer_progress(
        &mut self,
        id: u64,
        transferred: u64,
        total: u64,
        speed: u64,
    ) -> bool {
        self.transfers
            .iter_mut()
            .find(|item| item.id == id)
            .is_some_and(|item| apply_tauri_transfer_progress(item, transferred, total, speed))
    }

    fn apply_transfer_protocol(&mut self, id: u64, protocol: RemoteTransferProtocol) -> bool {
        let Some(item) = self.transfers.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        let changed = item.protocol != protocol;
        item.protocol = protocol;
        changed
    }

    fn apply_incomplete_transfers(
        &mut self,
        node_id: &NodeId,
        result: Result<Vec<StoredTransferProgress>, String>,
    ) -> (bool, Option<NodeId>) {
        if self.incomplete_load_node.as_ref() != Some(node_id) {
            return (false, None);
        }
        self.incomplete_load_inflight = false;
        self.incomplete_load_node = None;
        if self.current_node_id.as_ref() == Some(node_id) {
            match result {
                Ok(transfers) => {
                    self.incomplete_transfers = transfers
                        .into_iter()
                        .filter(StoredTransferProgress::is_incomplete)
                        .collect();
                    if self.incomplete_transfers.is_empty() {
                        self.show_incomplete = false;
                    }
                }
                Err(error) => {
                    if !is_sftp_incomplete_store_compat_error(&error) {
                        self.init_error = Some(error);
                    }
                    self.incomplete_transfers.clear();
                    self.show_incomplete = false;
                }
            }
        }
        let next_load = self
            .incomplete_load_pending_node
            .take()
            .filter(|pending| self.current_node_id.as_ref() == Some(pending));
        if let Some(node_id) = next_load.as_ref() {
            self.incomplete_load_inflight = true;
            self.incomplete_load_node = Some(node_id.clone());
        }
        (true, next_load)
    }

    fn apply_preview_loaded(
        &mut self,
        generation: u64,
        path: String,
        result: Result<PreviewContent, String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if generation != self.preview_generation {
            return false;
        }
        self.preview_loading = false;
        self.preview_hex_loading_more = false;
        self.preview_path = Some(path);
        match result {
            Ok(content) => {
                let asset_owner = PreviewAssetOwner::from_asset_content_owned_temp(&content);
                if let Some(owner) = asset_owner.as_ref() {
                    match owner.kind() {
                        AssetFileKind::Audio => {
                            let _ = self.preview_audio.load(owner.path());
                        }
                        AssetFileKind::Font => match std::fs::read(owner.path()) {
                            Ok(bytes) => {
                                let family = font_family_name_from_bytes(&bytes).or_else(|| {
                                    owner
                                        .path()
                                        .file_stem()
                                        .and_then(|name| name.to_str())
                                        .map(str::to_string)
                                });
                                match cx.text_system().add_fonts(vec![Cow::Owned(bytes)]) {
                                    Ok(()) => {
                                        self.preview_font_family = family;
                                        self.preview_font_error = None;
                                    }
                                    Err(error) => {
                                        self.preview_font_family = None;
                                        self.preview_font_error = Some(error.to_string());
                                    }
                                }
                            }
                            Err(error) => {
                                self.preview_font_family = None;
                                self.preview_font_error = Some(error.to_string());
                            }
                        },
                        AssetFileKind::Image | AssetFileKind::Video | AssetFileKind::Office => {}
                    }
                }
                self.preview_asset_owner = asset_owner;
                self.preview_content = Some(Arc::new(content));
                self.preview_error = None;
            }
            Err(error) => {
                self.preview_content = None;
                self.preview_asset_owner = None;
                self.preview_error = Some(error);
            }
        }
        true
    }

    fn apply_preview_hex_loaded(
        &mut self,
        generation: u64,
        path: &str,
        result: Result<PreviewContent, String>,
        error_prefix: &str,
    ) -> bool {
        if generation != self.preview_generation {
            return false;
        }
        self.preview_hex_loading_more = false;
        match result {
            Ok(PreviewContent::Hex {
                data: next_data,
                total_size: next_total_size,
                offset: next_offset,
                chunk_size: next_chunk_size,
                has_more: next_has_more,
            }) => {
                if self.preview_path.as_deref() == Some(path)
                    && let Some(content) = self.preview_content.as_mut()
                    && let PreviewContent::Hex {
                        data,
                        total_size,
                        offset,
                        chunk_size,
                        has_more,
                    } = Arc::make_mut(content)
                {
                    // Render snapshots normally release their Arc before the
                    // next delivery. Arc::make_mut preserves correctness if a
                    // prior frame still owns the old immutable chunk.
                    data.push_str(&next_data);
                    *total_size = next_total_size;
                    *offset = next_offset;
                    *chunk_size = next_chunk_size;
                    *has_more = next_has_more;
                    self.preview_error = None;
                }
            }
            Ok(other) => {
                self.preview_error =
                    Some(format!("{error_prefix}: {}", preview_content_text(&other)));
            }
            Err(error) => {
                self.preview_error = Some(format!("{error_prefix}: {error}"));
            }
        }
        true
    }

    fn apply_preview_saved(
        &mut self,
        generation: u64,
        path: String,
        content: Arc<str>,
        result: Result<SftpPreviewSaveResult, String>,
        network_error_message: &str,
        cx: &mut Context<Self>,
    ) -> (bool, bool) {
        if generation != self.preview_generation {
            return (false, false);
        }
        self.preview_editor_saving = false;
        match result {
            Ok(saved) => {
                self.preview_editor_dirty = false;
                self.preview_editor_initial_content = content.clone();
                self.preview_editor_observed_content = content.clone();
                self.preview_editor_save_error = None;
                self.preview_editor_network_error = false;
                self.preview_editor_retry_count = 0;
                self.preview_editor_last_saved_mtime = saved.mtime;
                self.preview_editor_last_atomic_write = Some(saved.atomic_write);
                self.preview_editor_encoding = saved.encoding_used.clone();
                self.preview_path = Some(path.clone());
                if let Some(editor) = self.preview_editor.clone() {
                    editor.update(cx, |editor, cx| editor.mark_saved_external(cx));
                }
                let line_ending = self.preview_editor_line_ending;
                if let Some(preview_content) = self.preview_content.as_mut()
                    && let PreviewContent::Text {
                        data,
                        encoding: current_encoding,
                        ..
                    } = Arc::make_mut(preview_content)
                {
                    *data = restore_text_line_endings(content.as_ref(), line_ending);
                    *current_encoding = saved.encoding_used.clone();
                }
                if let Some(file) = self.remote_files.iter_mut().find(|file| file.path == path) {
                    if let Some(size) = saved.size {
                        file.size = size;
                    }
                    file.modified = saved.mtime.map(|mtime| mtime as i64);
                }
                (true, true)
            }
            Err(error) => {
                if sftp_preview_editor_is_network_error(&error) {
                    self.preview_editor_network_error = true;
                    self.preview_editor_save_error = Some(network_error_message.to_string());
                } else {
                    self.preview_editor_network_error = false;
                    self.preview_editor_save_error = Some(error);
                }
                (true, false)
            }
        }
    }
}

#[cfg(test)]
mod remote_load_state_tests {
    use super::*;

    #[test]
    fn sftp_ready_event_preserves_pending_terminal_cwd() {
        let mut sftp = SftpWorkspaceEntity::default();
        // The shared session may become ready after the terminal cwd has queued a newer load.
        sftp.remote_path = "/root/.oxideterm".to_string();
        sftp.remote_path_input = sftp.remote_path.clone();
        sftp.remote_load_pending = true;

        sftp.apply_router_sftp_ready(true, Some("/root".to_string()));

        assert_eq!(sftp.remote_path, "/root/.oxideterm");
        assert_eq!(sftp.remote_path_input, "/root/.oxideterm");
        assert!(sftp.remote_loading);
    }

    #[test]
    fn remote_list_completion_clears_inflight_before_return() {
        let loading = SftpRemoteLoadState::default().request().start().unwrap();

        let completed = loading.complete();

        assert_eq!(completed, SftpRemoteLoadState::default());
        assert!(!completed.inflight);
    }

    #[test]
    fn queued_remote_load_starts_after_the_previous_request_completes() {
        let old_request = SftpRemoteLoadState::default().request().start().unwrap();
        let switched_view = old_request.request();

        let old_request_completed = switched_view.complete();

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
