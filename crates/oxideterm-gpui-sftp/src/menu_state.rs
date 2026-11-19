// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub enum SftpConflictDecision {
    Cancel,
    Continue,
    Execute {
        pending_transfers: Vec<SftpPendingTransfer>,
        resolved_actions: HashMap<String, SftpConflictResolution>,
    },
}

impl SftpWorkspaceEntity {
    pub fn dismiss_context_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.clear_context_menu_immediately();
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn open_context_menu(
        &mut self,
        pane: SftpPane,
        file: Option<SftpFileEntry>,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.active_pane = pane;
        if let Some(file) = file.as_ref() {
            let selected = match pane {
                SftpPane::Local => &mut self.local_selected,
                SftpPane::Remote => &mut self.remote_selected,
            };
            if oxideterm_gpui_ui::context_menu::preserve_or_move_context_selection(
                selected,
                file.name.clone(),
            ) {
                match pane {
                    SftpPane::Local => self.local_last_selected = Some(file.name.clone()),
                    SftpPane::Remote => self.remote_last_selected = Some(file.name.clone()),
                }
            }
        }
        self.context_menu_presence.reopen();
        self.context_menu_exit_generation = None;
        self.context_menu = Some(SftpContextMenu {
            pane,
            file,
            x,
            y,
            tree: false,
        });
        cx.notify();
    }

    /// Open the flat-pane menu for a tree row. Tree children are not part of
    /// `remote_files`, so the row itself becomes the absolute operation target.
    pub fn open_tree_context_menu(
        &mut self,
        file: SftpFileEntry,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.set_input_focus(None, cx);
        self.select_tree_file(file.path.clone());
        self.context_menu_presence.reopen();
        self.context_menu_exit_generation = None;
        self.context_menu = Some(SftpContextMenu {
            pane: SftpPane::Remote,
            file: Some(file),
            x,
            y,
            tree: true,
        });
        cx.notify();
    }

    pub fn open_rename_dialog(
        &mut self,
        pane: SftpPane,
        old_name: String,
        old_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.dialog_value.clone_from(&old_name);
        self.set_dialog(SftpDialog::Rename {
            pane,
            old_name,
            old_path,
        });
        self.set_input_focus(Some(SftpInput::DialogValue), cx);
        cx.notify();
    }

    pub fn open_new_folder_dialog(
        &mut self,
        pane: SftpPane,
        remote_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.dialog_value.clear();
        self.set_dialog(SftpDialog::NewFolder { pane, remote_path });
        self.set_input_focus(Some(SftpInput::DialogValue), cx);
        cx.notify();
    }

    pub fn selected_transfer_names(&self, pane: SftpPane) -> Vec<String> {
        match pane {
            SftpPane::Local => self.local_selected.iter().cloned().collect(),
            SftpPane::Remote => self.remote_selected.iter().cloned().collect(),
        }
    }

    pub fn pending_named_transfers(
        &self,
        pane: SftpPane,
        direction: SftpTransferDirection,
        selected_names: Vec<String>,
    ) -> Vec<SftpPendingTransfer> {
        let source_files = match pane {
            SftpPane::Local => &self.local_files,
            SftpPane::Remote => &self.remote_files,
        };
        selected_names
            .into_iter()
            .filter_map(|name| {
                source_files
                    .iter()
                    .find(|file| file.name == name)
                    .cloned()
                    .map(|source| SftpPendingTransfer {
                        name: if direction == SftpTransferDirection::Download
                            && self.pair_primary_remote_id.is_none()
                        {
                            oxideterm_sftp::local_download_name(&name)
                        } else {
                            name
                        },
                        direction,
                        source,
                        protocol_override: None,
                    })
            })
            .collect()
    }

    pub fn transfer_conflicts(
        &self,
        pending_transfers: &[SftpPendingTransfer],
    ) -> Vec<SftpConflictInfo> {
        let Some(direction) = pending_transfers.first().map(|transfer| transfer.direction) else {
            return Vec::new();
        };
        sftp_transfer_conflicts(pending_transfers, self.target_files(direction))
    }

    pub fn begin_transfer_conflicts(
        &mut self,
        conflicts: Vec<SftpConflictInfo>,
        pending_transfers: Vec<SftpPendingTransfer>,
        cx: &mut Context<Self>,
    ) {
        self.conflict_state = Some(SftpConflictState {
            conflicts,
            current_index: 0,
            pending_transfers,
            resolved_actions: HashMap::new(),
            apply_to_all: false,
        });
        self.set_dialog(SftpDialog::Conflict);
        self.clear_context_menu_immediately();
        cx.notify();
    }

    pub(crate) fn target_files(&self, direction: SftpTransferDirection) -> &[SftpFileEntry] {
        match direction {
            SftpTransferDirection::Upload => &self.remote_files,
            SftpTransferDirection::Download => &self.local_files,
        }
    }

    pub fn prepare_transfer_launches(
        &mut self,
        remote_id: SftpRemoteId,
        pending_transfers: Vec<SftpPendingTransfer>,
        resolved_actions: HashMap<String, SftpConflictResolution>,
        upload_target: Option<(&str, &[SftpFileEntry])>,
        configured_protocol: RemoteTransferProtocol,
        cx: &mut Context<Self>,
    ) -> Vec<SftpTransferLaunch> {
        let Some(direction) = pending_transfers.first().map(|transfer| transfer.direction) else {
            return Vec::new();
        };

        // Resolve names against the target snapshot before mutating queue state.
        // Explicit upload targets (terminal drop) are resolved against the
        // destination directory, not the SFTP pane's visible directory.
        let target_files = match upload_target {
            Some((_, target_files)) => target_files,
            None => self.target_files(direction),
        };
        let mut batch = SftpTransferBatch {
            direction,
            total: 0,
            success: 0,
            failed: 0,
            skipped: 0,
            queued: 0,
        };
        let planned_transfers = pending_transfers
            .into_iter()
            .filter_map(|transfer| {
                let resolution = resolved_actions.get(&transfer.name).copied();
                if resolution == Some(SftpConflictResolution::Skip)
                    || (resolution == Some(SftpConflictResolution::SkipOlder)
                        && sftp_source_not_newer_than_target(&transfer, target_files))
                {
                    batch.skipped += 1;
                    return None;
                }
                let target_name = if resolution == Some(SftpConflictResolution::Rename) {
                    unique_sftp_conflict_name(&transfer.name, target_files)
                } else {
                    transfer.name.clone()
                };
                if transfer.source.file_type == SftpFileType::Directory {
                    batch.queued += 1;
                }
                batch.total += 1;
                Some((transfer, target_name))
            })
            .collect::<Vec<_>>();

        let batch_id = self.next_transfer_batch_id;
        self.next_transfer_batch_id += 1;
        let mut launches = Vec::with_capacity(planned_transfers.len());
        for (transfer, target_name) in planned_transfers {
            let id = self.next_transfer_id;
            self.next_transfer_id += 1;
            let transfer_id = new_sftp_transfer_id(&remote_id, &transfer.name);
            let is_directory = transfer.source.file_type == SftpFileType::Directory;
            let local_path = match direction {
                SftpTransferDirection::Upload => transfer.source.path.clone(),
                SftpTransferDirection::Download => join_local_path(&self.local_path, &target_name),
            };
            let remote_path = match direction {
                SftpTransferDirection::Upload => {
                    let target_dir = upload_target
                        .map(|(target_dir, _)| target_dir)
                        .unwrap_or(&self.remote_path);
                    join_sftp_path(target_dir, &target_name)
                }
                SftpTransferDirection::Download => transfer.source.path,
            };
            let protocol = transfer.protocol_override.unwrap_or(configured_protocol);
            let resolution = resolved_actions.get(&transfer.name).copied();
            let download_disposition = if resolution == Some(SftpConflictResolution::Overwrite) {
                LocalDownloadDisposition::ReplaceExisting
            } else {
                LocalDownloadDisposition::CreateNew
            };
            self.transfers.push(SftpTransferItem {
                id,
                transfer_id: transfer_id.clone(),
                batch_id: Some(batch_id),
                remote_id: remote_id.clone(),
                name: if is_directory {
                    format!("{target_name}/")
                } else {
                    target_name
                },
                local_path: local_path.clone(),
                remote_path: remote_path.clone(),
                direction,
                protocol,
                size: transfer.source.size.max(1),
                transferred: 0,
                speed: 0,
                state: SftpTransferState::Pending,
                error: None,
            });
            launches.push(SftpTransferLaunch {
                id,
                transfer_id,
                remote_id: remote_id.clone(),
                direction,
                is_directory,
                local_path,
                remote_path,
                resume_progress: None,
                download_disposition,
                protocol_override: transfer.protocol_override,
            });
        }
        if batch.total > 0 {
            self.transfer_batches.insert(batch_id, batch);
            cx.notify();
        }
        launches
    }

    pub fn toggle_conflict_apply_all(&mut self, cx: &mut Context<Self>) {
        if let Some(conflict) = self.conflict_state.as_mut() {
            conflict.apply_to_all = !conflict.apply_to_all;
            cx.notify();
        }
    }

    pub fn resolve_transfer_conflict(
        &mut self,
        resolution: SftpConflictResolution,
        cx: &mut Context<Self>,
    ) -> SftpConflictDecision {
        let Some(mut conflict_state) = self.conflict_state.take() else {
            return SftpConflictDecision::Cancel;
        };
        if conflict_state.conflicts.is_empty() {
            cx.notify();
            return SftpConflictDecision::Cancel;
        }

        let current_index = conflict_state.current_index;
        if conflict_state.apply_to_all {
            for conflict in conflict_state.conflicts.iter().skip(current_index) {
                conflict_state
                    .resolved_actions
                    .insert(conflict.file_name.clone(), resolution);
            }
            cx.notify();
            return SftpConflictDecision::Execute {
                pending_transfers: conflict_state.pending_transfers,
                resolved_actions: conflict_state.resolved_actions,
            };
        }

        if let Some(conflict) = conflict_state.conflicts.get(current_index) {
            conflict_state
                .resolved_actions
                .insert(conflict.file_name.clone(), resolution);
        }
        if current_index + 1 < conflict_state.conflicts.len() {
            conflict_state.current_index += 1;
            conflict_state.apply_to_all = false;
            self.conflict_state = Some(conflict_state);
            self.set_dialog(SftpDialog::Conflict);
            cx.notify();
            SftpConflictDecision::Continue
        } else {
            cx.notify();
            SftpConflictDecision::Execute {
                pending_transfers: conflict_state.pending_transfers,
                resolved_actions: conflict_state.resolved_actions,
            }
        }
    }

    pub fn cancel_transfer_conflicts(&mut self, cx: &mut Context<Self>) {
        if self.conflict_state.take().is_some() {
            cx.notify();
        }
    }

    pub fn complete_transfer_batch_item(
        &mut self,
        batch_id: u64,
        state: SftpTransferState,
    ) -> Option<SftpTransferBatch> {
        let batch = self.transfer_batches.get_mut(&batch_id)?;
        match state {
            SftpTransferState::Completed => batch.success += 1,
            SftpTransferState::Error => batch.failed += 1,
            _ => return None,
        }
        if batch.success + batch.failed < batch.total {
            return None;
        }
        self.transfer_batches.remove(&batch_id)
    }
}

pub fn build_sftp_pending_uploads(paths: &[std::path::PathBuf]) -> Vec<SftpPendingTransfer> {
    paths
        .iter()
        .filter_map(|path| {
            let normalized = normalize_external_dropped_path(path)?;
            let metadata = std::fs::symlink_metadata(&normalized).ok()?;
            let name = normalized.file_name()?.to_string_lossy().to_string();
            if name.is_empty() {
                return None;
            }
            let modified = metadata
                .modified()
                .ok()
                .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64);
            let source = SftpFileEntry {
                name: name.clone(),
                path: normalized.to_string_lossy().to_string(),
                file_type: if metadata.is_dir() {
                    SftpFileType::Directory
                } else {
                    SftpFileType::File
                },
                size_known: true,
                size: metadata.len(),
                modified,
                permissions: None,
                owner: None,
                group: None,
                is_symlink: metadata.file_type().is_symlink(),
                symlink_target: std::fs::read_link(&normalized)
                    .ok()
                    .map(|target| target.to_string_lossy().to_string()),
            };
            Some(SftpPendingTransfer {
                name,
                direction: SftpTransferDirection::Upload,
                source,
                protocol_override: None,
            })
        })
        .collect()
}

pub fn sftp_terminal_drop_conflict_resolution(
    action: oxideterm_settings::ConflictAction,
) -> SftpConflictResolution {
    match action {
        // A terminal drop has no visible SFTP dialog to ask through; keep both
        // files instead of silently overwriting the destination.
        oxideterm_settings::ConflictAction::Ask => SftpConflictResolution::Rename,
        other => sftp_conflict_resolution_from_settings(other),
    }
}

impl SftpWorkspaceEntity {
    pub fn prepare_interactive_transfers(
        &mut self,
        remote_id: SftpRemoteId,
        pending: Vec<SftpPendingTransfer>,
        clear_selection: Option<SftpPane>,
        policy: oxideterm_settings::ConflictAction,
        protocol: RemoteTransferProtocol,
        cx: &mut Context<Self>,
    ) -> Vec<SftpTransferLaunch> {
        let conflicts = self.transfer_conflicts(&pending);
        let launches = if !conflicts.is_empty() && policy == oxideterm_settings::ConflictAction::Ask
        {
            self.begin_transfer_conflicts(conflicts, pending, cx);
            Vec::new()
        } else {
            let decision = sftp_conflict_resolution_from_settings(policy);
            let actions = conflicts
                .into_iter()
                .map(|conflict| (conflict.file_name, decision))
                .collect();
            self.prepare_transfer_launches(remote_id, pending, actions, None, protocol, cx)
        };
        if let Some(pane) = clear_selection
            && self.clear_selection(pane)
        {
            cx.notify();
        }
        launches
    }

    pub fn pending_tree_download(&self, file: SftpFileEntry) -> SftpPendingTransfer {
        SftpPendingTransfer {
            name: if self.pair_primary_remote_id.is_some() {
                file.name.clone()
            } else {
                oxideterm_sftp::local_download_name(&file.name)
            },
            direction: SftpTransferDirection::Download,
            source: file,
            protocol_override: None,
        }
    }
}

impl SftpTransferBatch {
    pub fn notice(self, i18n: &oxideterm_i18n::I18n) -> Option<TerminalNotice> {
        let batch = self;
        let is_upload = batch.direction == SftpTransferDirection::Upload;
        let only_queued_directory_transfers =
            batch.queued > 0 && batch.queued == batch.success && batch.failed == 0;
        if only_queued_directory_transfers {
            return None;
        }

        if batch.success > 0 && batch.failed == 0 {
            let description = if batch.skipped > 0 {
                sftp_i18n_transferred_skipped(
                    i18n.t("sftp.toast.transferred_skipped"),
                    batch.success,
                    batch.skipped,
                )
            } else {
                sftp_i18n_count(i18n.t("sftp.toast.transferred_count"), batch.success)
            };
            Some(transfer_notice(
                if is_upload {
                    i18n.t("sftp.toast.upload_complete")
                } else {
                    i18n.t("sftp.toast.download_complete")
                },
                Some(description),
                TerminalNoticeVariant::Success,
            ))
        } else if batch.failed > 0 && batch.success == 0 {
            Some(transfer_notice(
                if is_upload {
                    i18n.t("sftp.toast.upload_failed")
                } else {
                    i18n.t("sftp.toast.download_failed")
                },
                Some(sftp_i18n_count(
                    i18n.t("sftp.toast.failed_count"),
                    batch.failed,
                )),
                TerminalNoticeVariant::Error,
            ))
        } else if batch.success > 0 || batch.failed > 0 {
            Some(transfer_notice(
                if is_upload {
                    i18n.t("sftp.toast.upload_partial")
                } else {
                    i18n.t("sftp.toast.download_partial")
                },
                Some(sftp_i18n_partial_detail(
                    i18n.t("sftp.toast.partial_detail"),
                    batch.success,
                    batch.failed,
                    batch.skipped,
                )),
                TerminalNoticeVariant::Error,
            ))
        } else {
            None
        }
    }
}

fn transfer_notice(
    title: String,
    description: Option<String>,
    variant: TerminalNoticeVariant,
) -> TerminalNotice {
    TerminalNotice {
        title,
        description,
        variant,
        status_text: None,
        progress: None,
    }
}

fn sftp_i18n_transferred_skipped(template: String, count: usize, skipped: usize) -> String {
    template
        .replace("{{count}}", &count.to_string())
        .replace("{{skipped}}", &skipped.to_string())
}

fn sftp_i18n_partial_detail(
    template: String,
    success: usize,
    failed: usize,
    skipped: usize,
) -> String {
    template
        .replace("{{success}}", &success.to_string())
        .replace("{{failed}}", &failed.to_string())
        .replace("{{skipped}}", &skipped.to_string())
}
