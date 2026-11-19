// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub struct SftpTransferLaunch {
    pub id: u64,
    pub transfer_id: String,
    pub remote_id: SftpRemoteId,
    pub direction: SftpTransferDirection,
    pub is_directory: bool,
    pub local_path: String,
    pub remote_path: String,
    pub resume_progress: Option<StoredTransferProgress>,
    pub download_disposition: LocalDownloadDisposition,
    pub protocol_override: Option<RemoteTransferProtocol>,
}

pub struct SftpRelayLaunch {
    pub id: u64,
    pub remote_id: SftpRemoteId,
    pub source: SftpRemoteId,
    pub destination: SftpRemoteId,
    pub profile_id: Option<String>,
    pub request: oxideterm_sftp::SftpRelayRequest,
}

impl SftpTransferLaunch {
    pub fn into_relay(self, primary: SftpRemoteId, secondary: SftpRemoteId) -> SftpRelayLaunch {
        use oxideterm_sftp::{RemoteRelayDisposition, SftpRelayRequest};

        let profile_id = primary.standalone_endpoint_id().map(ToOwned::to_owned);
        let (source, destination, source_path, destination_path) = match self.direction {
            SftpTransferDirection::Upload => (
                primary,
                secondary.clone(),
                self.local_path,
                self.remote_path,
            ),
            SftpTransferDirection::Download => (
                secondary.clone(),
                primary,
                self.remote_path,
                self.local_path,
            ),
        };
        let disposition = match self.download_disposition {
            LocalDownloadDisposition::ReplaceExisting => RemoteRelayDisposition::ReplaceExisting,
            LocalDownloadDisposition::CreateNew | LocalDownloadDisposition::ResumeVerified => {
                RemoteRelayDisposition::CreateNew
            }
        };
        SftpRelayLaunch {
            id: self.id,
            remote_id: secondary,
            source,
            destination,
            profile_id,
            request: SftpRelayRequest {
                transfer_id: self.transfer_id,
                direction: self.direction,
                is_directory: self.is_directory,
                source_path,
                destination_path,
                disposition,
                context: None,
                resume: self.resume_progress,
            },
        }
    }
}

impl SftpRelayLaunch {
    pub fn storage_key(&self) -> String {
        format!(
            "relay:{}:{}",
            self.source.storage_key(),
            self.destination.storage_key()
        )
    }

    pub fn bind_profile_revision(&mut self, revision: Option<String>) {
        self.request.context =
            self.profile_id
                .clone()
                .zip(revision)
                .and_then(|(profile_id, profile_revision)| {
                    Some(oxideterm_sftp::RemoteRelayProgressContext {
                        profile_id,
                        profile_revision,
                        source_endpoint_id: self.source.standalone_endpoint_id()?.to_owned(),
                        destination_endpoint_id: self
                            .destination
                            .standalone_endpoint_id()?
                            .to_owned(),
                    })
                });
    }
}

pub enum SftpResumeLaunch {
    Transfer(SftpTransferLaunch),
    Relay(SftpRelayLaunch),
}

pub enum SftpTransferControl {
    Pause(String),
    Resume(String),
    Cancel(String),
}

impl SftpWorkspaceEntity {
    pub fn prepare_preview_download(
        &mut self,
        remote_id: SftpRemoteId,
        name: &str,
        protocol: RemoteTransferProtocol,
        cx: &mut Context<Self>,
    ) -> Option<SftpTransferLaunch> {
        let remote_path = self.preview_path.clone()?;
        let local_path = join_local_path(&self.local_path, name);
        let size = self
            .remote_files
            .iter()
            .find(|file| file.path == remote_path)
            .map_or(1, |file| file.size.max(1));
        let transfer_id = new_sftp_transfer_id(&remote_id, name);
        let id = self.next_transfer_id;
        self.next_transfer_id += 1;
        self.transfers.push(SftpTransferItem {
            id,
            transfer_id: transfer_id.clone(),
            batch_id: None,
            remote_id: remote_id.clone(),
            name: name.to_owned(),
            local_path: local_path.clone(),
            remote_path: remote_path.clone(),
            direction: SftpTransferDirection::Download,
            protocol,
            size,
            transferred: 0,
            speed: 0,
            state: SftpTransferState::Pending,
            error: None,
        });
        cx.notify();
        Some(SftpTransferLaunch {
            id,
            transfer_id,
            remote_id,
            direction: SftpTransferDirection::Download,
            is_directory: false,
            local_path,
            remote_path,
            resume_progress: None,
            download_disposition: LocalDownloadDisposition::CreateNew,
            protocol_override: None,
        })
    }

    pub fn prepare_quick_scp_download(
        &mut self,
        conflict_action: oxideterm_settings::ConflictAction,
        missing_path_error: String,
        cx: &mut Context<Self>,
    ) -> Option<(
        Vec<SftpPendingTransfer>,
        HashMap<String, SftpConflictResolution>,
    )> {
        let remote_path = self.remote_path_input.trim();
        let Some(name) = remote_path
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        else {
            self.init_error = Some(SftpViewError::Operation(missing_path_error));
            cx.notify();
            return None;
        };
        let pending_transfers = vec![SftpPendingTransfer {
            name: name.to_string(),
            direction: SftpTransferDirection::Download,
            source: SftpFileEntry {
                name: name.to_string(),
                path: remote_path.to_string(),
                file_type: SftpFileType::File,
                size_known: true,
                size: 0,
                modified: None,
                permissions: None,
                owner: None,
                group: None,
                is_symlink: false,
                symlink_target: None,
            },
            protocol_override: Some(RemoteTransferProtocol::Scp),
        }];
        // Conflict detection borrows the entity-owned file list instead of
        // copying every local row into a temporary render-layer snapshot.
        let conflicts = sftp_transfer_conflicts(&pending_transfers, &self.local_files);
        if !conflicts.is_empty() && conflict_action == oxideterm_settings::ConflictAction::Ask {
            self.conflict_state = Some(SftpConflictState {
                conflicts,
                current_index: 0,
                pending_transfers,
                resolved_actions: HashMap::new(),
                apply_to_all: false,
            });
            self.set_dialog(SftpDialog::Conflict);
            cx.notify();
            return None;
        }
        let resolved_actions = conflicts
            .into_iter()
            .map(|conflict| {
                (
                    conflict.file_name,
                    sftp_conflict_resolution_from_settings(conflict_action),
                )
            })
            .collect();
        Some((pending_transfers, resolved_actions))
    }

    pub fn begin_incomplete_transfer_load(&mut self, remote_id: SftpRemoteId) -> bool {
        if self.incomplete_load_inflight {
            if self.incomplete_load_remote.as_ref() != Some(&remote_id) {
                self.incomplete_load_pending_remote = Some(remote_id);
            }
            return false;
        }
        self.incomplete_load_inflight = true;
        self.incomplete_load_remote = Some(remote_id);
        true
    }

    pub fn prepare_manual_resume(
        &mut self,
        remote_id: SftpRemoteId,
        transfer_id: &str,
        cx: &mut Context<Self>,
    ) -> Option<SftpResumeLaunch> {
        let progress = self.take_incomplete_progress_for_resume(transfer_id, cx)?;
        let relay = progress.remote_relay.as_ref().map(|relay| {
            let source = SftpRemoteId::Standalone(relay.source_endpoint_id.clone());
            let destination = SftpRemoteId::Standalone(relay.destination_endpoint_id.clone());
            (source, destination)
        });
        let launch = self.prepare_reconnect_resume(remote_id, progress, true)?;
        let Some((source, destination)) = relay else {
            return Some(SftpResumeLaunch::Transfer(launch));
        };
        if launch.is_directory {
            return None;
        }
        let (primary, secondary) = match launch.direction {
            SftpTransferDirection::Upload => (source, destination),
            SftpTransferDirection::Download => (destination, source),
        };
        Some(SftpResumeLaunch::Relay(
            launch.into_relay(primary, secondary),
        ))
    }

    fn take_incomplete_progress_for_resume(
        &mut self,
        transfer_id: &str,
        cx: &mut Context<Self>,
    ) -> Option<StoredTransferProgress> {
        let index = self
            .incomplete_transfers
            .iter()
            .position(|progress| progress.transfer_id == transfer_id)?;
        if !self.incomplete_transfers[index].is_incomplete() {
            return None;
        }
        let progress = self.incomplete_transfers.remove(index);
        if self.incomplete_transfers.is_empty() {
            self.show_incomplete = false;
        }
        cx.notify();
        Some(progress)
    }

    pub(crate) fn prepare_reconnect_resume(
        &mut self,
        remote_id: SftpRemoteId,
        progress: StoredTransferProgress,
        show_in_current_view: bool,
    ) -> Option<SftpTransferLaunch> {
        if !progress.is_incomplete() || !progress.protocol.supports_restart_resume() {
            return None;
        }
        let launch = self.prepare_resumed_transfer(remote_id, progress, show_in_current_view);
        Some(launch)
    }

    pub(crate) fn prepare_resumed_transfer(
        &mut self,
        remote_id: SftpRemoteId,
        progress: StoredTransferProgress,
        show_in_current_view: bool,
    ) -> SftpTransferLaunch {
        let oxideterm_sftp::ResumedTransferPaths {
            direction,
            local_path,
            remote_path,
            name,
            is_directory,
        } = oxideterm_sftp::ResumedTransferPaths::from_progress(&progress);
        let id = self.next_transfer_id;
        self.next_transfer_id += 1;
        let transfer_id = progress.transfer_id.clone();

        if show_in_current_view {
            self.incomplete_transfers
                .retain(|item| item.transfer_id != transfer_id);
            if self.incomplete_transfers.is_empty() {
                self.show_incomplete = false;
            }
            self.transfers.push(SftpTransferItem {
                id,
                transfer_id: transfer_id.clone(),
                batch_id: None,
                remote_id: remote_id.clone(),
                name: if is_directory {
                    format!("{name}/")
                } else {
                    name
                },
                local_path: local_path.clone(),
                remote_path: remote_path.clone(),
                direction,
                protocol: progress.protocol,
                size: progress.total_bytes.max(1),
                transferred: progress.transferred_bytes,
                speed: 0,
                state: SftpTransferState::Pending,
                error: None,
            });
        }

        SftpTransferLaunch {
            id,
            transfer_id,
            remote_id,
            direction,
            is_directory,
            local_path,
            remote_path,
            resume_progress: Some(progress),
            download_disposition: LocalDownloadDisposition::ResumeVerified,
            protocol_override: None,
        }
    }

    pub fn set_transfer_state(
        &mut self,
        id: u64,
        state: SftpTransferState,
        cx: &mut Context<Self>,
    ) -> Option<SftpTransferControl> {
        let item = self.transfers.iter_mut().find(|item| item.id == id)?;
        let transfer_id = item.transfer_id.clone();
        item.state = state;
        cx.notify();
        match state {
            SftpTransferState::Paused => Some(SftpTransferControl::Pause(transfer_id)),
            SftpTransferState::Pending | SftpTransferState::Active => {
                Some(SftpTransferControl::Resume(transfer_id))
            }
            SftpTransferState::Cancelled => Some(SftpTransferControl::Cancel(transfer_id)),
            SftpTransferState::Completed | SftpTransferState::Error => None,
        }
    }

    pub fn cancel_or_remove_transfer(
        &mut self,
        id: u64,
        cx: &mut Context<Self>,
    ) -> Option<SftpTransferControl> {
        let index = self.transfers.iter().position(|item| item.id == id)?;
        let active = matches!(
            self.transfers[index].state,
            SftpTransferState::Active | SftpTransferState::Pending | SftpTransferState::Paused
        );
        let control = if active {
            let transfer_id = self.transfers[index].transfer_id.clone();
            self.transfers[index].state = SftpTransferState::Cancelled;
            Some(SftpTransferControl::Cancel(transfer_id))
        } else {
            self.transfers.remove(index);
            None
        };
        cx.notify();
        control
    }

    pub fn upsert_background_transfer_snapshot(
        &mut self,
        remote_id: SftpRemoteId,
        snapshot: BackgroundTransferSnapshot,
    ) {
        let direction = match snapshot.direction {
            BackgroundTransferDirection::Upload => SftpTransferDirection::Upload,
            BackgroundTransferDirection::Download => SftpTransferDirection::Download,
        };
        let state = sftp_transfer_state_from_background(snapshot.state);
        let size = snapshot.size.max(1);
        if let Some(item) = self
            .transfers
            .iter_mut()
            .find(|item| item.transfer_id == snapshot.id)
        {
            item.remote_id = remote_id;
            item.name = snapshot.name;
            item.local_path = snapshot.local_path;
            item.remote_path = snapshot.remote_path;
            item.direction = direction;
            if snapshot.size > 0 {
                item.size = snapshot.size;
            } else if item.size == 0 {
                item.size = size;
            }
            item.transferred = snapshot.transferred;
            item.speed = snapshot.backend_speed.unwrap_or(item.speed);
            item.state = state;
            item.error = snapshot.error;
            return;
        }

        let id = self.next_transfer_id;
        self.next_transfer_id += 1;
        self.transfers.push(SftpTransferItem {
            id,
            transfer_id: snapshot.id,
            batch_id: None,
            remote_id,
            name: snapshot.name,
            local_path: snapshot.local_path,
            remote_path: snapshot.remote_path,
            direction,
            protocol: snapshot.protocol,
            size,
            transferred: snapshot.transferred,
            speed: snapshot.backend_speed.unwrap_or_default(),
            state,
            error: snapshot.error,
        });
    }

    pub fn interrupt_transfers_by_remote(
        &mut self,
        remote_id: &SftpRemoteId,
        error: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut changed = false;
        for transfer in &mut self.transfers {
            if &transfer.remote_id == remote_id
                && matches!(
                    transfer.state,
                    SftpTransferState::Active
                        | SftpTransferState::Pending
                        | SftpTransferState::Paused
                )
            {
                transfer.state = SftpTransferState::Error;
                transfer.error = Some(error.to_string());
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
        changed
    }
}

impl SftpWorkspaceEntity {
    pub fn notify_transfer_queue(&self, cx: &mut Context<Self>) {
        if let Some(view) = &self.feature_views.queue {
            view.update(cx, |_, cx| cx.notify());
        }
    }
}
