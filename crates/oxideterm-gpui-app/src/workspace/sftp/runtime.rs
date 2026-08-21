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
    bind_session: Option<(SftpRemoteId, String, String)>,
    load_transfer_state_for: Option<SftpRemoteId>,
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
        remote_id: &SftpRemoteId,
    ) -> Option<(String, u64)> {
        if self.current_surface_id != Some(surface_id)
            || self.current_remote_id.as_ref() != Some(remote_id)
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
        remote_id: SftpRemoteId,
    ) {
        self.pair_primary_remote_id = None;
        self.pair_primary_loading = false;
        self.current_surface_id = Some(surface_id);
        if self.current_remote_id.as_ref() == Some(&remote_id) {
            // Returning to a hidden view consumes any pending load directly;
            // no workspace heartbeat is involved.
            self.request_remote_load();
            return;
        }

        if let Some(previous_remote_id) = self.current_remote_id.take() {
            self.local_path_by_remote
                .insert(previous_remote_id.clone(), self.local_path.clone());
            if !self.remote_path.is_empty() {
                self.remote_path_by_remote
                    .insert(previous_remote_id, self.remote_path.clone());
            }
        }

        self.current_remote_id = Some(remote_id.clone());
        self.view_generation = self.view_generation.wrapping_add(1);
        let local_path = self
            .local_path_by_remote
            .get(&remote_id)
            .cloned()
            .unwrap_or_else(default_download_path);
        self.apply_local_path(local_path);

        let remembered_remote = self
            .remote_path_by_remote
            .get(&remote_id)
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

    pub(in crate::workspace) fn activate_pair_view(
        &mut self,
        surface_id: SftpSurfaceId,
        primary_remote_id: SftpRemoteId,
        secondary_remote_id: SftpRemoteId,
        primary_initial_path: Option<String>,
    ) {
        self.activate_view(surface_id, secondary_remote_id);
        self.pair_primary_remote_id = Some(primary_remote_id.clone());
        let path = primary_initial_path
            .filter(|path| !path.trim().is_empty())
            .or_else(|| self.local_path_by_remote.get(&primary_remote_id).cloned())
            .unwrap_or_default();
        self.local_path = path.clone();
        self.local_path_input = path;
        self.local_files.clear();
        self.local_selected.clear();
        self.local_last_selected = None;
        self.pair_primary_loading = true;
    }

    pub(in crate::workspace) fn deactivate_view(
        &mut self,
        surface_id: SftpSurfaceId,
        remote_id: &SftpRemoteId,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.current_surface_id != Some(surface_id)
            || self.current_remote_id.as_ref() != Some(remote_id)
        {
            return false;
        }

        self.local_path_by_remote
            .insert(remote_id.clone(), self.local_path.clone());
        if !self.remote_path.is_empty() {
            self.remote_path_by_remote
                .insert(remote_id.clone(), self.remote_path.clone());
        }
        self.current_surface_id = None;
        self.current_remote_id = None;
        self.pair_primary_remote_id = None;
        self.pair_primary_loading = false;
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
        // Transfer tasks own their remote consumer and survive removal of this UI projection.
        cx.notify();
        true
    }

    fn apply_remote_list(
        &mut self,
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
        view_generation: u64,
        session_id: String,
        path: String,
        result: Result<RemoteSftpListing, String>,
        cx: &mut Context<Self>,
    ) -> SftpRemoteListOutcome {
        self.set_remote_load_state(self.remote_load_state().complete());
        if self.current_surface_id != Some(surface_id)
            || self.current_remote_id.as_ref() != Some(&remote_id)
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
                self.remote_path_by_remote
                    .insert(remote_id.clone(), cwd.clone());
                self.remote_home_by_remote
                    .entry(remote_id.clone())
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
                    bind_session: Some((remote_id.clone(), session_id, cwd)),
                    load_transfer_state_for: Some(remote_id),
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
                        remote_id,
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
                        if let Some(previous_path) =
                            self.remote_path_by_remote.get(&remote_id).cloned()
                        {
                            self.remote_path.clone_from(&previous_path);
                            self.remote_path_input = previous_path;
                        }
                    } else if oxideterm_sftp::error_is_not_found(&error) {
                        self.remote_path = "/".to_string();
                        self.remote_path_input = "/".to_string();
                        self.remote_path_by_remote
                            .insert(remote_id, "/".to_string());
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
        remote_id: SftpRemoteId,
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
                    && sftp.current_remote_id.as_ref() == Some(&remote_id)
                    && sftp.view_generation == view_generation
                    && sftp.remote_path == path
                    && !sftp.remote_load_inflight
                {
                    // Hidden surfaces retain the pending request; mounting the
                    // owner later calls the same gate without restarting SSH.
                    sftp.request_remote_load();
                    cx.emit(SftpWorkspaceEvent::RemoteLoadReady {
                        surface_id,
                        remote_id,
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
                remote_id,
            } => {
                let Some((path, view_generation)) = self.start_remote_load(surface_id, &remote_id)
                else {
                    return false;
                };
                effects.push_back(SftpWorkspaceEffect::StartRemoteLoad {
                    surface_id,
                    remote_id,
                    path,
                    view_generation,
                });
                true
            }
            SftpWorkerResult::RemoteList {
                surface_id,
                remote_id,
                view_generation,
                session_id,
                path,
                result,
            } => {
                let outcome = self.apply_remote_list(
                    surface_id,
                    remote_id,
                    view_generation,
                    session_id,
                    path,
                    result,
                    cx,
                );
                if let Some((remote_id, session_id, cwd)) = outcome.bind_session {
                    effects.push_back(SftpWorkspaceEffect::BindSession {
                        remote_id,
                        session_id,
                        cwd,
                    });
                }
                if let Some(remote_id) = outcome.load_transfer_state_for {
                    effects.push_back(SftpWorkspaceEffect::LoadBackgroundTransfers {
                        remote_id: remote_id.clone(),
                    });
                    if self.begin_incomplete_transfer_load(remote_id.clone()) {
                        effects
                            .push_back(SftpWorkspaceEffect::LoadIncompleteTransfers { remote_id });
                    }
                }
                self.push_remote_load_pending_effect(effects);
                outcome.changed
            }
            SftpWorkerResult::PairPrimaryList {
                surface_id,
                remote_id,
                view_generation,
                path,
                result,
            } => {
                if self.current_surface_id != Some(surface_id)
                    || self.pair_primary_remote_id.as_ref() != Some(&remote_id)
                    || self.view_generation != view_generation
                    || self.local_path != path
                {
                    return false;
                }
                self.pair_primary_loading = false;
                match result {
                    Ok(listing) => {
                        let cwd = listing.cwd;
                        self.local_path_by_remote
                            .insert(remote_id.clone(), cwd.clone());
                        self.remote_home_by_remote
                            .entry(remote_id)
                            .or_insert_with(|| cwd.clone());
                        self.local_path.clone_from(&cwd);
                        self.local_path_input = cwd;
                        self.local_files = listing.files;
                        self.local_selected.clear();
                        self.local_last_selected = None;
                        self.init_error = None;
                    }
                    Err(error) => {
                        self.init_error = Some(format!("{path}: {error}"));
                    }
                }
                true
            }
            SftpWorkerResult::RemotePathCompletion {
                generation,
                remote_id,
                parent_path,
                result,
            } => self.apply_remote_path_completion(generation, &remote_id, &parent_path, result),
            SftpWorkerResult::PairPrimaryPathCompletion {
                generation,
                remote_id,
                parent_path,
                result,
            } => {
                if self.pair_primary_remote_id.as_ref() != Some(&remote_id) {
                    return false;
                }
                self.local_path_completion.apply_entries(
                    generation,
                    &parent_path,
                    result.unwrap_or_default(),
                )
            }
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
                remote_id,
                transfer_id,
                id,
                result,
                refresh_remote,
                refresh_local,
            } => {
                let success = result.is_ok();
                effects.push_back(SftpWorkspaceEffect::TransferFinishedForReconnect {
                    remote_id: remote_id.clone(),
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
                if self.current_remote_id.as_ref() == Some(&remote_id) {
                    if should_refresh && refresh_remote {
                        self.request_remote_load();
                        self.push_remote_load_pending_effect(effects);
                    }
                    if should_refresh && refresh_local {
                        if self.pair_primary_remote_id.is_some() {
                            effects.push_back(SftpWorkspaceEffect::ReloadPairPrimaryDirectory);
                        } else {
                            effects.push_back(SftpWorkspaceEffect::ReloadLocalDirectory {
                                view_generation: self.view_generation,
                                path: self.local_path.clone(),
                            });
                        }
                    }
                    if self.begin_incomplete_transfer_load(remote_id.clone()) {
                        effects
                            .push_back(SftpWorkspaceEffect::LoadIncompleteTransfers { remote_id });
                    }
                }
                true
            }
            SftpWorkerResult::ResumeIncompleteTransferLoaded {
                remote_id,
                transfer_id,
                result,
            } => {
                let launch = match result {
                    Ok(progress) if progress.is_incomplete() => {
                        let show_in_current_view =
                            self.current_remote_id.as_ref() == Some(&remote_id);
                        self.prepare_reconnect_resume(
                            remote_id.clone(),
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
                        remote_id,
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
                    if self.pair_primary_remote_id.is_some() {
                        effects.push_back(SftpWorkspaceEffect::ReloadPairPrimaryDirectory);
                    } else {
                        effects.push_back(SftpWorkspaceEffect::ReloadLocalDirectory {
                            view_generation: self.view_generation,
                            path: self.local_path.clone(),
                        });
                    }
                }
                true
            }
            SftpWorkerResult::IncompleteTransfersLoaded { remote_id, result } => {
                let (changed, next_load) = self.apply_incomplete_transfers(&remote_id, result);
                if let Some(remote_id) = next_load {
                    effects.push_back(SftpWorkspaceEffect::LoadIncompleteTransfers { remote_id });
                }
                changed
            }
            SftpWorkerResult::IncompleteTransferDiscarded {
                transfer_id,
                result,
            } => {
                match result {
                    Ok(()) => {
                        self.incomplete_transfers
                            .retain(|progress| progress.transfer_id != transfer_id);
                        if self.incomplete_transfers.is_empty() {
                            self.show_incomplete = false;
                        }
                    }
                    Err(error) => {
                        if let Some(progress) = self
                            .incomplete_transfers
                            .iter_mut()
                            .find(|progress| progress.transfer_id == transfer_id)
                        {
                            progress.error = Some(error);
                        }
                    }
                }
                true
            }
            SftpWorkerResult::BackgroundTransfersLoaded { remote_id, result } => {
                if self.current_remote_id.as_ref() != Some(&remote_id) {
                    return false;
                }
                match result {
                    Ok(snapshots) => {
                        for snapshot in snapshots {
                            self.upsert_background_transfer_snapshot(remote_id.clone(), snapshot);
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
            && let (Some(surface_id), Some(remote_id)) =
                (self.current_surface_id, self.current_remote_id.clone())
        {
            effects.push_back(SftpWorkspaceEffect::RemoteLoadPending {
                surface_id,
                remote_id,
            });
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn sftp_remote_backend(
        &self,
        remote_id: &SftpRemoteId,
    ) -> Option<SftpRemoteBackend> {
        match remote_id {
            SftpRemoteId::Node(node_id) => Some(SftpRemoteBackend::Node {
                router: self.node_router.clone(),
                node_id: node_id.clone(),
            }),
            SftpRemoteId::Standalone(profile_id) => self
                .standalone_sftp_sessions
                .get(profile_id)
                .map(|runtime| SftpRemoteBackend::Standalone {
                    handle: runtime.handle.clone(),
                }),
        }
    }

    pub(in crate::workspace::sftp) fn acquire_sftp_transfer_backend(
        &self,
        remote_id: &SftpRemoteId,
        transfer_id: &str,
    ) -> Option<(SftpRemoteBackend, Option<StandaloneSftpConsumerLease>)> {
        match remote_id {
            SftpRemoteId::Node(node_id) => Some((
                SftpRemoteBackend::Node {
                    router: self.node_router.clone(),
                    node_id: node_id.clone(),
                },
                None,
            )),
            SftpRemoteId::Standalone(profile_id) => {
                let runtime = self.standalone_sftp_sessions.get(profile_id)?;
                let consumer = ConnectionConsumer::Sftp(format!(
                    "standalone-transfer:{profile_id}:{transfer_id}"
                ));
                let handle = self
                    .ssh_registry
                    .acquire_consumer_for_connection(&runtime.connection_id, consumer.clone())?;
                let lease = StandaloneSftpConsumerLease {
                    registry: self.ssh_registry.clone(),
                    connection_id: runtime.connection_id.clone(),
                    consumer,
                };
                Some((SftpRemoteBackend::Standalone { handle }, Some(lease)))
            }
        }
    }

    pub(in crate::workspace::sftp) fn sftp_remote_id_for_tab(
        &self,
        tab_id: TabId,
    ) -> Option<SftpRemoteId> {
        self.sftp_tab_nodes
            .get(&tab_id)
            .cloned()
            .map(SftpRemoteId::Node)
            .or_else(|| {
                self.standalone_sftp_tabs.get(&tab_id).map(|binding| {
                    SftpRemoteId::Standalone(
                        binding
                            .secondary_endpoint_id
                            .as_ref()
                            .unwrap_or(&binding.primary_endpoint_id)
                            .clone(),
                    )
                })
            })
    }

    pub(in crate::workspace::sftp) fn sftp_pair_primary_remote_id(
        &self,
        cx: &App,
    ) -> Option<SftpRemoteId> {
        self.sftp_view.read(cx).pair_primary_remote_id.clone()
    }

    pub(in crate::workspace::sftp) fn request_sftp_remote_load(&mut self, cx: &mut Context<Self>) {
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.request_remote_load();
            cx.notify();
        });
        self.maybe_start_sftp_remote_load(cx);
    }

    pub(in crate::workspace) fn request_sftp_pair_primary_load(&mut self, cx: &mut Context<Self>) {
        let Some((surface_id, remote_id, path, view_generation, delivery)) = ({
            let sftp = self.sftp_view.read(cx);
            match (sftp.current_surface_id, sftp.pair_primary_remote_id.clone()) {
                (Some(surface_id), Some(remote_id)) => Some((
                    surface_id,
                    remote_id,
                    sftp.local_path.clone(),
                    sftp.view_generation,
                    sftp.worker_sender(),
                )),
                _ => None,
            }
        }) else {
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.pair_primary_loading = true;
            cx.notify();
        });
        self.forwarding_runtime.spawn(async move {
            let result = load_remote_sftp_listing(backend, &path).await;
            let _ = delivery.send(SftpWorkerResult::PairPrimaryList {
                surface_id,
                remote_id,
                view_generation,
                path,
                result,
            });
        });
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

    /// Opens the dedicated independent-SFTP surface without consulting sidebar preferences.
    pub(in crate::workspace) fn open_standalone_sftp_tab_surface(
        &mut self,
        endpoint_id: String,
        endpoint_title: String,
        initial_remote_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let title = format!(
            "{} · {}",
            self.i18n.t("sidebar.panels.sftp"),
            endpoint_title
        );
        let tab_id = if let Some((tab_id, _)) = self
            .standalone_sftp_tabs
            .iter()
            .find(|(_, binding)| binding.primary_endpoint_id == endpoint_id)
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
            self.standalone_sftp_tabs.insert(
                tab_id,
                StandaloneSftpTabBinding {
                    primary_endpoint_id: endpoint_id.clone(),
                    secondary_endpoint_id: None,
                    secondary_initial_remote_path: None,
                },
            );
            tab_id
        };

        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.active_ssh_node_id = None;
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.activate_view(
                SftpSurfaceId::Tab(tab_id),
                SftpRemoteId::Standalone(endpoint_id),
            );
            cx.notify();
        });
        if let Some(path) = initial_remote_path.filter(|path| !path.trim().is_empty()) {
            self.set_sftp_path(SftpPane::Remote, path, cx);
        }
        self.request_sftp_remote_load(cx);
        cx.notify();
    }

    /// Opens one tab that owns two independent standalone SFTP endpoint consumers.
    pub(in crate::workspace) fn open_standalone_sftp_pair_tab_surface(
        &mut self,
        primary_endpoint_id: String,
        secondary_endpoint_id: String,
        endpoint_title: String,
        primary_initial_remote_path: Option<String>,
        secondary_initial_remote_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let title = format!(
            "{} · {}",
            self.i18n.t("sidebar.panels.sftp"),
            endpoint_title
        );
        let tab_id = if let Some((tab_id, _)) =
            self.standalone_sftp_tabs.iter().find(|(_, binding)| {
                binding.primary_endpoint_id == primary_endpoint_id
                    && binding.secondary_endpoint_id.as_deref()
                        == Some(secondary_endpoint_id.as_str())
            }) {
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
            self.standalone_sftp_tabs.insert(
                tab_id,
                StandaloneSftpTabBinding {
                    primary_endpoint_id: primary_endpoint_id.clone(),
                    secondary_endpoint_id: Some(secondary_endpoint_id.clone()),
                    secondary_initial_remote_path: secondary_initial_remote_path.clone(),
                },
            );
            tab_id
        };

        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.active_ssh_node_id = None;
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.activate_pair_view(
                SftpSurfaceId::Tab(tab_id),
                SftpRemoteId::Standalone(primary_endpoint_id),
                SftpRemoteId::Standalone(secondary_endpoint_id),
                primary_initial_remote_path,
            );
            cx.notify();
        });
        if let Some(path) = secondary_initial_remote_path.filter(|path| !path.trim().is_empty()) {
            self.set_sftp_path(SftpPane::Remote, path, cx);
        }
        self.request_sftp_pair_primary_load(cx);
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
            sftp.activate_view(SftpSurfaceId::Sidebar, SftpRemoteId::Node(node_id));
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
            sftp.deactivate_view(
                SftpSurfaceId::Sidebar,
                &SftpRemoteId::Node(node_id.clone()),
                cx,
            )
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
                && sftp.current_remote_id.as_ref() == Some(&SftpRemoteId::Node(node_id.clone()))
        };
        if !already_active {
            self.sftp_view.update(cx, |sftp, cx| {
                sftp.activate_view(SftpSurfaceId::Sidebar, SftpRemoteId::Node(node_id));
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
            sftp.activate_view(
                SftpSurfaceId::Tab(tab_id),
                SftpRemoteId::Node(node_id.clone()),
            );
            cx.notify();
        });
        self.maybe_start_sftp_remote_load(cx);
    }

    pub(in crate::workspace) fn maybe_start_sftp_remote_load(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let (surface_id, remote_id) = {
            let sftp = self.sftp_view.read(cx);
            let Some(surface_id) = sftp.current_surface_id else {
                return false;
            };
            let Some(remote_id) = sftp.current_remote_id.clone() else {
                return false;
            };
            (surface_id, remote_id)
        };
        if !self.sftp_surface_is_visible(surface_id, &remote_id, cx) {
            return false;
        }
        let Some((path, view_generation)) = self.sftp_view.update(cx, |sftp, _cx| {
            sftp.start_remote_load(surface_id, &remote_id)
        }) else {
            return false;
        };
        let delivery = self.sftp_view.read(cx).worker_sender();
        self.spawn_sftp_remote_load(surface_id, remote_id, path, view_generation, delivery);
        true
    }

    fn spawn_sftp_remote_load(
        &self,
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
        path: String,
        view_generation: u64,
        tx: delivery::ActiveDeliverySender<SftpWorkerResult>,
    ) {
        let session_id = format!("{}:sftp", remote_id.storage_key());
        let runtime = self.forwarding_runtime.clone();
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            let _ = tx.send(SftpWorkerResult::RemoteList {
                surface_id,
                remote_id,
                view_generation,
                session_id,
                path,
                result: Err("SFTP endpoint is no longer available".to_string()),
            });
            return;
        };
        let owner_backend = backend.clone();
        runtime.spawn(async move {
            // The visible surface creates one shared SFTP channel through its explicit owner.
            let _ = owner_backend.acquire_sftp().await;
        });
        runtime.spawn(async move {
            let result = load_remote_sftp_listing(backend, &path).await;
            let _ = tx.send(SftpWorkerResult::RemoteList {
                surface_id,
                remote_id,
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
                    remote_id,
                    session_id,
                    cwd,
                } => {
                    if let Some(node_id) = remote_id.node_id()
                        && let Ok(event) =
                            self.node_router
                                .bind_sftp_session(node_id, session_id, Some(cwd))
                    {
                        // Binding only reports node-owned readiness. It never
                        // grants SFTP authority to start or stop the SSH link.
                        self.emit_node_event(event);
                    }
                }
                SftpWorkspaceEffect::LoadBackgroundTransfers { remote_id } => {
                    self.spawn_sftp_background_transfer_load_with_sender(
                        remote_id,
                        delivery.clone(),
                    );
                }
                SftpWorkspaceEffect::LoadIncompleteTransfers { remote_id } => {
                    self.spawn_sftp_incomplete_load_with_sender(remote_id, delivery.clone());
                }
                SftpWorkspaceEffect::RemoteLoadPending {
                    surface_id,
                    remote_id,
                } => {
                    if self.sftp_surface_is_visible(surface_id, &remote_id, cx) {
                        let _ = delivery.send(SftpWorkerResult::StartRemoteLoad {
                            surface_id,
                            remote_id,
                        });
                    }
                }
                SftpWorkspaceEffect::StartRemoteLoad {
                    surface_id,
                    remote_id,
                    path,
                    view_generation,
                } => {
                    self.spawn_sftp_remote_load(
                        surface_id,
                        remote_id,
                        path,
                        view_generation,
                        delivery.clone(),
                    );
                }
                SftpWorkspaceEffect::TransferFinishedForReconnect {
                    remote_id,
                    transfer_id,
                    success,
                } => {
                    if let Some(node_id) = remote_id.node_id() {
                        self.on_sftp_transfer_finished_for_reconnect(
                            node_id,
                            &transfer_id,
                            success,
                            cx,
                        );
                    }
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
                SftpWorkspaceEffect::ReloadPairPrimaryDirectory => {
                    self.request_sftp_pair_primary_load(cx);
                }
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn request_visible_sftp_remote_load(
        &self,
        surface_id: SftpSurfaceId,
        remote_id: SftpRemoteId,
        delivery: delivery::ActiveDeliverySender<SftpWorkerResult>,
        cx: &App,
    ) {
        if self.sftp_surface_is_visible(surface_id, &remote_id, cx) {
            let _ = delivery.send(SftpWorkerResult::StartRemoteLoad {
                surface_id,
                remote_id,
            });
        }
    }

    fn sftp_surface_is_visible(
        &self,
        surface_id: SftpSurfaceId,
        remote_id: &SftpRemoteId,
        cx: &App,
    ) -> bool {
        match surface_id {
            SftpSurfaceId::Tab(tab_id) => {
                self.active_tab_id(cx) == Some(tab_id)
                    && self
                        .tabs(cx)
                        .iter()
                        .any(|tab| tab.id == tab_id && tab.kind == TabKind::Sftp)
                    && self.sftp_remote_id_for_tab(tab_id).as_ref() == Some(remote_id)
            }
            SftpSurfaceId::Sidebar => {
                let Some(node_id) = remote_id.node_id() else {
                    return false;
                };
                !self.sidebar_collapsed
                    && self.effective_sidebar_panel_section() == SidebarSection::Sessions
                    && self.embedded_sftp_node_id.as_ref() == Some(node_id)
            }
        }
    }

    pub(in crate::workspace) fn visible_sftp_remote_id(&self, cx: &App) -> Option<SftpRemoteId> {
        let sftp = self.sftp_view.read(cx);
        let surface_id = sftp.current_surface_id?;
        let remote_id = sftp.current_remote_id.clone()?;
        self.sftp_surface_is_visible(surface_id, &remote_id, cx)
            .then_some(remote_id)
    }

    pub(in crate::workspace::sftp) fn visible_sftp_node_id(&self, cx: &App) -> Option<NodeId> {
        self.visible_sftp_remote_id(cx)?.node_id().cloned()
    }

    pub(in crate::workspace) fn apply_sftp_ready_event(
        &mut self,
        node_id: &NodeId,
        ready: bool,
        cwd: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view.update(cx, |sftp, cx| {
            if sftp.current_remote_id.as_ref() != Some(&SftpRemoteId::Node(node_id.clone())) {
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
        remote_id: &SftpRemoteId,
        parent_path: &str,
        result: Result<Vec<PathCompletionCandidate>, String>,
    ) -> bool {
        if self.current_remote_id.as_ref() != Some(remote_id) {
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
        remote_id: &SftpRemoteId,
        result: Result<Vec<StoredTransferProgress>, String>,
    ) -> (bool, Option<SftpRemoteId>) {
        if self.incomplete_load_remote.as_ref() != Some(remote_id) {
            return (false, None);
        }
        self.incomplete_load_inflight = false;
        self.incomplete_load_remote = None;
        if self.current_remote_id.as_ref() == Some(remote_id) {
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
            .incomplete_load_pending_remote
            .take()
            .filter(|pending| self.current_remote_id.as_ref() == Some(pending));
        if let Some(remote_id) = next_load.as_ref() {
            self.incomplete_load_inflight = true;
            self.incomplete_load_remote = Some(remote_id.clone());
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
            remote_id: SftpRemoteId::Node(NodeId::new("node-1")),
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
