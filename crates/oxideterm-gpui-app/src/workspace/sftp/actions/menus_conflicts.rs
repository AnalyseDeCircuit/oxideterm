use super::dialog_lifecycle::sftp_i18n_count;
use super::*;

struct SftpTransferLaunch {
    id: u64,
    transfer_id: String,
    remote_id: SftpRemoteId,
    direction: SftpTransferDirection,
    is_directory: bool,
    local_path: String,
    remote_path: String,
    download_disposition: LocalDownloadDisposition,
    protocol_override: Option<RemoteTransferProtocol>,
}

enum SftpConflictDecision {
    Cancel,
    Continue,
    Execute {
        pending_transfers: Vec<SftpPendingTransfer>,
        resolved_actions: HashMap<String, SftpConflictResolution>,
    },
}

impl SftpWorkspaceEntity {
    pub(in crate::workspace) fn dismiss_context_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.clear_context_menu_immediately();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace::sftp) fn open_context_menu(
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
            if crate::workspace::browser_behavior::preserve_or_move_context_selection(
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
        self.context_menu = Some(SftpContextMenu { pane, file, x, y });
        cx.notify();
    }

    pub(in crate::workspace::sftp) fn open_rename_dialog(
        &mut self,
        pane: SftpPane,
        old_name: String,
        cx: &mut Context<Self>,
    ) {
        self.dialog_value.clone_from(&old_name);
        self.set_dialog(SftpDialog::Rename { pane, old_name });
        self.focused_input = Some(SftpInput::DialogValue);
        cx.notify();
    }

    pub(in crate::workspace::sftp) fn open_new_folder_dialog(
        &mut self,
        pane: SftpPane,
        cx: &mut Context<Self>,
    ) {
        self.dialog_value.clear();
        self.set_dialog(SftpDialog::NewFolder { pane });
        self.focused_input = Some(SftpInput::DialogValue);
        cx.notify();
    }

    fn selected_transfer_names(&self, pane: SftpPane) -> Vec<String> {
        match pane {
            SftpPane::Local => self.local_selected.iter().cloned().collect(),
            SftpPane::Remote => self.remote_selected.iter().cloned().collect(),
        }
    }

    fn pending_named_transfers(
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
                        name,
                        direction,
                        source,
                        protocol_override: None,
                    })
            })
            .collect()
    }

    fn transfer_conflicts(
        &self,
        pending_transfers: &[SftpPendingTransfer],
    ) -> Vec<SftpConflictInfo> {
        let Some(direction) = pending_transfers.first().map(|transfer| transfer.direction) else {
            return Vec::new();
        };
        sftp_transfer_conflicts(pending_transfers, self.target_files(direction))
    }

    fn begin_transfer_conflicts(
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

    fn target_files(&self, direction: SftpTransferDirection) -> &[SftpFileEntry] {
        match direction {
            SftpTransferDirection::Upload => &self.remote_files,
            SftpTransferDirection::Download => &self.local_files,
        }
    }

    fn prepare_transfer_launches(
        &mut self,
        remote_id: SftpRemoteId,
        pending_transfers: Vec<SftpPendingTransfer>,
        resolved_actions: HashMap<String, SftpConflictResolution>,
        configured_protocol: RemoteTransferProtocol,
        cx: &mut Context<Self>,
    ) -> Vec<SftpTransferLaunch> {
        let Some(direction) = pending_transfers.first().map(|transfer| transfer.direction) else {
            return Vec::new();
        };

        // Resolve names against the current target snapshot before mutating queue state.
        let target_files = self.target_files(direction);
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
                SftpTransferDirection::Upload => join_sftp_path(&self.remote_path, &target_name),
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

    pub(in crate::workspace::sftp) fn toggle_conflict_apply_all(&mut self, cx: &mut Context<Self>) {
        if let Some(conflict) = self.conflict_state.as_mut() {
            conflict.apply_to_all = !conflict.apply_to_all;
            cx.notify();
        }
    }

    fn resolve_transfer_conflict(
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

    fn cancel_transfer_conflicts(&mut self, cx: &mut Context<Self>) {
        if self.conflict_state.take().is_some() {
            cx.notify();
        }
    }

    pub(in crate::workspace::sftp) fn complete_transfer_batch_item(
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

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn extract_remote_sftp_archive(
        &mut self,
        file: SftpFileEntry,
        cx: &mut Context<Self>,
    ) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.extract_failed"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        };
        let remote_directory = self.sftp_view.read(cx).remote_path.clone();
        let archive_path = if file.path.is_empty() {
            join_sftp_path(&remote_directory, &file.name)
        } else {
            file.path.clone()
        };
        let command = match oxideterm_sftp::plan_archive_extraction(
            &file.name,
            &archive_path,
            &remote_directory,
        ) {
            Ok(plan) => plan.command,
            Err(oxideterm_sftp::ArchiveExtractionError::UnsupportedArchive { .. }) => {
                self.push_sftp_toast(
                    self.i18n.t("sftp.toast.unsupported_archive"),
                    Some(file.name),
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
        };

        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let tx = self.sftp_view.read(cx).worker_sender();
        let runtime = self.forwarding_runtime.clone();
        let toast = SftpMutationToast {
            success_title: self.i18n.t("sftp.toast.extract_complete"),
            success_description: Some(file.name),
            error_title: self.i18n.t("sftp.toast.extract_failed"),
        };
        runtime.spawn(async move {
            let result = async {
                let handle = backend.resolve_connection().await?;
                let output = handle
                    .run_command_capture(&command, std::time::Duration::from_secs(300), 64 * 1024)
                    .await
                    .map_err(|error| error.to_string())?;
                if output.exit_code == Some(0) {
                    Ok(())
                } else {
                    Err(format_sftp_remote_extract_error(output))
                }
            }
            .await;
            let _ = tx.send(SftpWorkerResult::RemoteMutationComplete {
                result,
                refresh_remote: true,
                refresh_local: false,
                toast: Some(toast),
            });
        });
        self.sftp_view
            .update(cx, |sftp, cx| sftp.dismiss_context_menu(cx));
    }

    pub(in crate::workspace::sftp) fn queue_sftp_transfers(
        &mut self,
        pane: SftpPane,
        direction: SftpTransferDirection,
        cx: &mut Context<Self>,
    ) {
        let selected = self.sftp_view.read(cx).selected_transfer_names(pane);
        self.queue_sftp_named_transfers(pane, direction, selected, cx);
    }

    pub(in crate::workspace::sftp) fn queue_sftp_named_transfers(
        &mut self,
        pane: SftpPane,
        direction: SftpTransferDirection,
        selected_names: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        if selected_names.is_empty() {
            return;
        }
        let pending_transfers =
            self.sftp_view
                .read(cx)
                .pending_named_transfers(pane, direction, selected_names);
        if pending_transfers.is_empty() {
            return;
        }

        let conflict_action = self.settings_store.settings().sftp.conflict_action;
        let conflicts = self
            .sftp_view
            .read(cx)
            .transfer_conflicts(&pending_transfers);
        if !conflicts.is_empty() && conflict_action == oxideterm_settings::ConflictAction::Ask {
            self.sftp_view.update(cx, |sftp, cx| {
                sftp.begin_transfer_conflicts(conflicts, pending_transfers, cx);
            });
            self.clear_sftp_selection(pane, cx);
            return;
        }

        let resolved_actions = conflicts
            .into_iter()
            .map(|conflict| {
                (
                    conflict.file_name,
                    sftp_conflict_resolution_from_settings(conflict_action),
                )
            })
            .collect::<HashMap<_, _>>();
        self.execute_sftp_pending_transfers(remote_id, pending_transfers, resolved_actions, cx);
        self.clear_sftp_selection(pane, cx);
    }

    pub(in crate::workspace::sftp) fn queue_sftp_external_upload_paths(
        &mut self,
        paths: &[std::path::PathBuf],
        cx: &mut Context<Self>,
    ) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        self.queue_sftp_external_upload_paths_for_remote(remote_id, paths, cx);
    }

    pub(in crate::workspace::sftp) fn queue_sftp_external_upload_paths_for_node(
        &mut self,
        node_id: NodeId,
        paths: &[std::path::PathBuf],
        cx: &mut Context<Self>,
    ) {
        self.queue_sftp_external_upload_paths_for_remote(SftpRemoteId::Node(node_id), paths, cx);
    }

    fn queue_sftp_external_upload_paths_for_remote(
        &mut self,
        remote_id: SftpRemoteId,
        paths: &[std::path::PathBuf],
        cx: &mut Context<Self>,
    ) {
        let pending_transfers = paths
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
            .collect::<Vec<_>>();
        if pending_transfers.is_empty() {
            return;
        }
        let conflicts = self
            .sftp_view
            .read(cx)
            .transfer_conflicts(&pending_transfers);
        if !conflicts.is_empty()
            && self.settings_store.settings().sftp.conflict_action
                == oxideterm_settings::ConflictAction::Ask
        {
            self.sftp_view.update(cx, |sftp, cx| {
                sftp.begin_transfer_conflicts(conflicts, pending_transfers, cx);
            });
            return;
        }

        let conflict_action = self.settings_store.settings().sftp.conflict_action;
        let resolved_actions = conflicts
            .into_iter()
            .map(|conflict| {
                (
                    conflict.file_name,
                    sftp_conflict_resolution_from_settings(conflict_action),
                )
            })
            .collect::<HashMap<_, _>>();
        self.execute_sftp_pending_transfers(remote_id, pending_transfers, resolved_actions, cx);
    }

    pub(in crate::workspace::sftp) fn execute_sftp_pending_transfers(
        &mut self,
        remote_id: SftpRemoteId,
        pending_transfers: Vec<SftpPendingTransfer>,
        resolved_actions: HashMap<String, SftpConflictResolution>,
        cx: &mut Context<Self>,
    ) {
        let pair_primary_remote_id = self.sftp_pair_primary_remote_id(cx);
        let configured_protocol = if pair_primary_remote_id.is_some() {
            RemoteTransferProtocol::Sftp
        } else {
            configured_transfer_protocol(self.settings_store.settings().sftp.transfer_protocol)
        };
        let launches = self.sftp_view.update(cx, |sftp, cx| {
            sftp.prepare_transfer_launches(
                remote_id.clone(),
                pending_transfers,
                resolved_actions,
                configured_protocol,
                cx,
            )
        });
        for launch in launches {
            if let Some(primary_remote_id) = pair_primary_remote_id.clone() {
                self.spawn_sftp_pair_transfer_task(
                    launch.id,
                    launch.transfer_id,
                    launch.direction,
                    launch.is_directory,
                    launch.local_path,
                    launch.remote_path,
                    launch.download_disposition,
                    None,
                    primary_remote_id,
                    remote_id.clone(),
                    cx,
                );
                continue;
            }
            // The entity owns queue state; the workspace retains only the runtime launch adapter.
            self.spawn_sftp_transfer_task(
                launch.id,
                launch.transfer_id,
                launch.remote_id,
                launch.direction,
                launch.is_directory,
                launch.local_path,
                launch.remote_path,
                None,
                launch.download_disposition,
                launch.protocol_override,
                cx,
            );
        }
    }

    pub(in crate::workspace::sftp) fn resolve_sftp_transfer_conflict(
        &mut self,
        resolution: SftpConflictResolution,
        cx: &mut Context<Self>,
    ) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            self.cancel_sftp_transfer_conflicts(cx);
            return;
        };
        match self.sftp_view.update(cx, |sftp, cx| {
            sftp.resolve_transfer_conflict(resolution, cx)
        }) {
            SftpConflictDecision::Cancel => self.close_sftp_dialog(cx),
            SftpConflictDecision::Continue => {}
            SftpConflictDecision::Execute {
                pending_transfers,
                resolved_actions,
            } => {
                self.close_sftp_dialog(cx);
                self.execute_sftp_pending_transfers(
                    remote_id,
                    pending_transfers,
                    resolved_actions,
                    cx,
                );
            }
        }
    }

    pub(in crate::workspace::sftp) fn cancel_sftp_transfer_conflicts(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view
            .update(cx, |sftp, cx| sftp.cancel_transfer_conflicts(cx));
        self.close_sftp_dialog(cx);
    }

    pub(in crate::workspace::sftp) fn show_sftp_transfer_batch_toast(
        &self,
        batch: SftpTransferBatch,
        cx: &App,
    ) {
        let is_upload = batch.direction == SftpTransferDirection::Upload;
        let only_queued_directory_transfers =
            batch.queued > 0 && batch.queued == batch.success && batch.failed == 0;
        if only_queued_directory_transfers {
            return;
        }

        if batch.success > 0 && batch.failed == 0 {
            let description = if batch.skipped > 0 {
                sftp_i18n_transferred_skipped(
                    self.i18n.t("sftp.toast.transferred_skipped"),
                    batch.success,
                    batch.skipped,
                )
            } else {
                sftp_i18n_count(self.i18n.t("sftp.toast.transferred_count"), batch.success)
            };
            self.push_sftp_toast(
                if is_upload {
                    self.i18n.t("sftp.toast.upload_complete")
                } else {
                    self.i18n.t("sftp.toast.download_complete")
                },
                Some(description),
                TerminalNoticeVariant::Success,
                cx,
            );
        } else if batch.failed > 0 && batch.success == 0 {
            self.push_sftp_toast(
                if is_upload {
                    self.i18n.t("sftp.toast.upload_failed")
                } else {
                    self.i18n.t("sftp.toast.download_failed")
                },
                Some(sftp_i18n_count(
                    self.i18n.t("sftp.toast.failed_count"),
                    batch.failed,
                )),
                TerminalNoticeVariant::Error,
                cx,
            );
        } else if batch.success > 0 || batch.failed > 0 {
            self.push_sftp_toast(
                if is_upload {
                    self.i18n.t("sftp.toast.upload_partial")
                } else {
                    self.i18n.t("sftp.toast.download_partial")
                },
                Some(sftp_i18n_partial_detail(
                    self.i18n.t("sftp.toast.partial_detail"),
                    batch.success,
                    batch.failed,
                    batch.skipped,
                )),
                TerminalNoticeVariant::Error,
                cx,
            );
        }
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

pub(in crate::workspace) fn sftp_extract_archive_kind(
    file_name: &str,
) -> Option<oxideterm_sftp::ArchiveKind> {
    // Keep menu capability checks on the same domain rule used for command planning.
    oxideterm_sftp::archive_kind(file_name)
}

fn format_sftp_remote_extract_error(output: oxideterm_ssh::SshCommandOutput) -> String {
    let detail = if !output.stderr.trim().is_empty() {
        output.stderr.trim()
    } else if !output.stdout.trim().is_empty() {
        output.stdout.trim()
    } else {
        "remote extractor exited without details"
    };
    let mut message = if let Some(code) = output.exit_code {
        format!("exit {code}: {detail}")
    } else {
        format!("remote extractor exited without status: {detail}")
    };
    if output.truncated {
        message.push_str(" (output truncated)");
    }
    message
}
