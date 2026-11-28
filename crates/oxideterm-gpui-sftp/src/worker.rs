// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

use oxideterm_sftp::{
    SftpRemoteLoadState, apply_tauri_transfer_completion, apply_tauri_transfer_progress,
    remote_list_result_is_superseded,
};

struct SftpRemoteListOutcome {
    bind_session: Option<(SftpRemoteId, String, String)>,
    load_transfer_state_for: Option<SftpRemoteId>,
    changed: bool,
}

impl SftpWorkspaceEntity {
    #[cfg(not(target_family = "wasm"))]
    fn take_remote_browse_request(
        &mut self,
        remote_id: &SftpRemoteId,
        path: &str,
    ) -> Option<oxideterm_audit::AuditContext> {
        self.remote_browse_request
            .take()
            .and_then(|(requested_remote, requested_path, context)| {
                (requested_remote == *remote_id && requested_path == path).then_some(context)
            })
    }

    pub fn remote_load_state(&self) -> SftpRemoteLoadState {
        SftpRemoteLoadState {
            loading: self.remote_loading,
            pending: self.remote_load_pending,
            inflight: self.remote_load_inflight,
        }
    }

    pub fn set_remote_load_state(&mut self, state: SftpRemoteLoadState) {
        self.remote_loading = state.loading;
        self.remote_load_pending = state.pending;
        self.remote_load_inflight = state.inflight;
    }

    pub fn request_remote_load(&mut self) {
        let state = self.remote_load_state().request();
        self.set_remote_load_state(state);
    }

    pub fn start_remote_load(
        &mut self,
        surface_id: SftpSurfaceId,
        remote_id: &SftpRemoteId,
    ) -> Option<SftpRemoteLoadRequest> {
        if self.current_surface_id != Some(surface_id)
            || self.current_remote_id.as_ref() != Some(remote_id)
        {
            return None;
        }
        let started = self.remote_load_state().start()?;
        self.set_remote_load_state(started);
        self.init_error = None;
        let path = self.remote_path.clone();
        #[cfg(not(target_family = "wasm"))]
        let audit_request = self.take_remote_browse_request(remote_id, &path);
        Some(SftpRemoteLoadRequest {
            surface_id,
            remote_id: remote_id.clone(),
            view_generation: self.view_generation,
            path,
            #[cfg(not(target_family = "wasm"))]
            audit_request,
        })
    }

    pub fn activate_view(&mut self, surface_id: SftpSurfaceId, remote_id: SftpRemoteId) {
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
        self.clear_sftp_tree_state();
        let local_path = self
            .local_path_by_remote
            .get(&remote_id)
            .cloned()
            .unwrap_or_else(|| self.local_host.download_dir());
        let _ = self.apply_local_path(local_path);

        let remembered_remote = self
            .remote_path_by_remote
            .get(&remote_id)
            .cloned()
            .unwrap_or_default();
        self.remote_path = remembered_remote.clone();
        self.remote_path_input = remembered_remote;
        self.remote_path_completion.dismiss();
        self.completion_selection.clear();
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

    pub fn activate_pair_view(
        &mut self,
        surface_id: SftpSurfaceId,
        primary_remote_id: SftpRemoteId,
        secondary_remote_id: SftpRemoteId,
        primary_initial_path: Option<String>,
    ) {
        self.activate_view(surface_id, secondary_remote_id);
        self.set_primary_endpoint(primary_remote_id, primary_initial_path);
    }

    /// Replacing the left endpoint must not restart or clear the right directory.
    pub fn set_primary_endpoint(&mut self, primary_remote_id: SftpRemoteId, primary_initial_path: Option<String>) {
        self.completion_selection.remove(&SftpPane::Local);
        self.local_path_completion.dismiss();
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

    pub fn deactivate_view(
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
        self.completion_selection.clear();
        self.set_input_focus(None, cx);
        self.editing_local_path = false;
        self.editing_remote_path = false;
        self.folder_picker_task = None;
        self.drag_state = None;
        self.drag_over_pane = None;
        self.drag_autoscroll_position = None;
        self.drag_autoscroll_scheduled = false;
        self.clear_sftp_tree_state();
        self.clear_context_menu_immediately();
        self.bookmark_menu = None;
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
                self.clear_sftp_tree_state();
                self.remote_selected.clear();
                self.remote_last_selected = None;
                self.select_completed_path(SftpPane::Remote);
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
                    self.init_error = Some(SftpViewError::Directory(format!("{path}: {error}")));
                }
                SftpRemoteListOutcome {
                    bind_session: None,
                    load_transfer_state_for: None,
                    changed: true,
                }
            }
        }
    }

    pub(crate) fn schedule_remote_load_retry(
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
            cx.background_executor().timer(delay).await;
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

    pub fn reduce_worker_result(
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
                let Some(request) = self.start_remote_load(surface_id, &remote_id) else {
                    return false;
                };
                effects.push_back(SftpWorkspaceEffect::StartRemoteLoad(request));
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
            SftpWorkerResult::TreeChildren {
                surface_id,
                remote_id,
                view_generation,
                tree_generation,
                path,
                result,
            } => {
                // A root reload or navigation clears the tree state but keeps
                // `view_generation`, so the tree epoch is the authoritative
                // invalidation for in-flight child listings.
                if self.current_surface_id != Some(surface_id)
                    || self.current_remote_id.as_ref() != Some(&remote_id)
                    || self.view_generation != view_generation
                    || self.tree_generation != tree_generation
                {
                    return false;
                }
                self.tree_loading.remove(&path);
                match result {
                    Ok(listing) => {
                        self.tree_children.insert(path, listing.files);
                    }
                    Err(error) => {
                        self.tree_expanded.remove(&path);
                        effects
                            .push_back(SftpWorkspaceEffect::TreeChildrenLoadFailed { path, error });
                    }
                }
                // Failed loads must advance the queue just like successful ones.
                if let Some(next) = self.tree_pending.pop_front() {
                    self.tree_loading.insert(next.clone());
                    effects.push_back(SftpWorkspaceEffect::StartTreeChildrenLoad {
                        surface_id,
                        remote_id,
                        view_generation,
                        tree_generation: self.tree_generation,
                        path: next,
                    });
                }
                true
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
                        self.select_completed_path(SftpPane::Local);
                        self.init_error = None;
                    }
                    Err(error) => {
                        self.init_error =
                            Some(SftpViewError::Directory(format!("{path}: {error}")));
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
                        batch_update = item.batch_id.take().map(|batch_id| (batch_id, item.state));
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
                            self.reload_local_directory();
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
                            self.init_error = Some(SftpViewError::Operation(error));
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
                        self.reload_local_directory();
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
                        self.init_error = Some(SftpViewError::Operation(error));
                    }
                }
                true
            }
            SftpWorkerResult::PreviewLoaded {
                asset_owner,
                generation,
                path,
                result,
            } => self.apply_preview_loaded(generation, path, result, asset_owner, cx),
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

    pub(crate) fn push_remote_load_pending_effect(
        &self,
        effects: &mut VecDeque<SftpWorkspaceEffect>,
    ) {
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

impl SftpWorkspaceEntity {
    pub fn apply_router_sftp_ready(&mut self, ready: bool, cwd: Option<String>) {
        // Channel readiness does not finish a directory load that is pending or still in flight.
        self.remote_loading = !ready || self.remote_load_pending || self.remote_load_inflight;
        if self.remote_load_pending {
            // Readiness can report the shared session's older cwd while explicit navigation waits.
            return;
        }
        if let Some(cwd) = cwd.filter(|_| self.remote_path.is_empty()) {
            self.remote_path.clone_from(&cwd);
            self.remote_path_input = cwd;
        }
    }

    pub(crate) fn apply_remote_path_completion(
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

    pub(crate) fn apply_transfer_progress(
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

    pub(crate) fn apply_transfer_protocol(
        &mut self,
        id: u64,
        protocol: RemoteTransferProtocol,
    ) -> bool {
        let Some(item) = self.transfers.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        let changed = item.protocol != protocol;
        item.protocol = protocol;
        changed
    }

    fn reload_local_directory(&self) {
        if let Ok(files) = self.local_host.list(&self.local_path) {
            let _ = self.worker_tx.send(SftpWorkerResult::LocalFilesLoaded {
                view_generation: self.view_generation,
                path: self.local_path.clone(),
                files,
            });
        }
    }

    pub(crate) fn apply_incomplete_transfers(
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
                        self.init_error = Some(SftpViewError::Operation(error));
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

    pub fn apply_preview_loaded(
        &mut self,
        generation: u64,
        path: String,
        result: Result<PreviewContent, String>,
        asset_owner: Option<PreviewAssetOwner>,
        cx: &mut Context<Self>,
    ) -> bool {
        if generation != self.preview_generation {
            return false;
        }
        self.preview_load_task = None;
        self.preview_loading = false;
        self.preview_hex_loading_more = false;
        self.preview_path = Some(path);
        self.preview_markdown_cache
            .release(&mut self.preview_markdown_scroll);
        match result {
            Ok(content) => {
                if let Some(owner) = asset_owner.as_ref() {
                    match owner.kind() {
                        AssetFileKind::Document => {
                            if let Some(view) = &self.preview_plugin {
                                view.set_source(owner.clone(), cx);
                            }
                        }
                        AssetFileKind::Audio => {
                            let _ = self.preview_audio.load(owner.path());
                        }
                        AssetFileKind::Font => match self.local_host.font_bytes(owner.path()) {
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
                self.preview_image = match &content {
                    PreviewContent::Image { data, mime_type } => {
                        use base64::Engine;
                        gpui::ImageFormat::from_mime_type(mime_type).and_then(|format|
                            base64::engine::general_purpose::STANDARD.decode(data).ok().map(|bytes|Arc::new(gpui::Image::from_bytes(format,bytes))))
                    }
                    _ => None,
                };
                self.preview_asset_owner = asset_owner;
                self.preview_content = Some(Arc::new(content));
                self.preview_error = None;
            }
            Err(error) => {
                self.preview_content = None;
                self.preview_image = None;
                self.preview_asset_owner = None;
                self.preview_error = Some(error);
            }
        }
        true
    }

    pub(crate) fn apply_preview_hex_loaded(
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

    pub fn apply_preview_saved(
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
                        version,
                        encoding: current_encoding,
                        ..
                    } = Arc::make_mut(preview_content)
                {
                    *data = restore_text_line_endings(content.as_ref(), line_ending);
                    *version = saved.version.clone();
                    *current_encoding = saved.encoding_used.clone();
                }
                if let Some(file) = self.remote_files.iter_mut().find(|file| file.path == path) {
                    if let Some(size) = saved.size {
                        file.size = size;
                        file.size_known = true;
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
