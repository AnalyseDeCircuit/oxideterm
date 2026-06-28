impl WorkspaceApp {
    fn open_sftp_context_menu(
        &mut self,
        pane: SftpPane,
        file: Option<SftpFileEntry>,
        x: f32,
        y: f32,
    ) {
        self.sftp_view.active_pane = pane;
        if let Some(file) = file.as_ref() {
            let selected = match pane {
                SftpPane::Local => &mut self.sftp_view.local_selected,
                SftpPane::Remote => &mut self.sftp_view.remote_selected,
            };
            if crate::workspace::browser_behavior::preserve_or_move_context_selection(
                selected,
                file.name.clone(),
            ) {
                match pane {
                    SftpPane::Local => self.sftp_view.local_last_selected = Some(file.name.clone()),
                    SftpPane::Remote => {
                        self.sftp_view.remote_last_selected = Some(file.name.clone())
                    }
                }
            }
        }
        self.sftp_view.context_menu = Some(SftpContextMenu { pane, file, x, y });
    }

    fn open_sftp_rename_dialog(&mut self, pane: SftpPane, old_name: String) {
        self.sftp_view.dialog_value = old_name.clone();
        self.sftp_view.dialog = Some(SftpDialog::Rename { pane, old_name });
        self.sftp_view.focused_input = Some(SftpInput::DialogValue);
    }

    fn open_sftp_new_folder_dialog(&mut self, pane: SftpPane) {
        self.sftp_view.dialog_value.clear();
        self.sftp_view.dialog = Some(SftpDialog::NewFolder { pane });
        self.sftp_view.focused_input = Some(SftpInput::DialogValue);
    }

    fn extract_remote_sftp_archive(&mut self, file: SftpFileEntry) {
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.extract_failed"),
                None,
                TerminalNoticeVariant::Error,
            );
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.extract_failed"),
                None,
                TerminalNoticeVariant::Error,
            );
            return;
        };
        let remote_directory = self.sftp_view.remote_path.clone();
        let archive_path = if file.path.is_empty() {
            join_sftp_path(&remote_directory, &file.name)
        } else {
            file.path.clone()
        };
        let Some(command) =
            sftp_extract_archive_command(&file.name, &archive_path, &remote_directory)
        else {
            self.push_sftp_toast(
                self.i18n.t("sftp.toast.unsupported_archive"),
                Some(file.name),
                TerminalNoticeVariant::Error,
            );
            return;
        };

        let router = self.node_router.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        let toast = SftpMutationToast {
            success_title: self.i18n.t("sftp.toast.extract_complete"),
            success_description: Some(file.name),
            error_title: self.i18n.t("sftp.toast.extract_failed"),
        };
        runtime.spawn(async move {
            let result = async {
                let resolved = router
                    .resolve_connection(&node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                let output = resolved
                    .handle
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
        self.sftp_view.dismiss_context_menu();
    }

    fn queue_sftp_transfers(&mut self, pane: SftpPane, direction: SftpTransferDirection) {
        let selected = match pane {
            SftpPane::Local => self.sftp_view.local_selected.clone(),
            SftpPane::Remote => self.sftp_view.remote_selected.clone(),
        };
        self.queue_sftp_named_transfers(pane, direction, selected.into_iter().collect());
    }

    fn queue_sftp_named_transfers(
        &mut self,
        pane: SftpPane,
        direction: SftpTransferDirection,
        selected_names: Vec<String>,
    ) {
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        if selected_names.is_empty() {
            return;
        }
        let source_files = match pane {
            SftpPane::Local => self.sftp_view.local_files.clone(),
            SftpPane::Remote => self.sftp_view.remote_files.clone(),
        };
        let pending_transfers = selected_names
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
                    })
            })
            .collect::<Vec<_>>();
        if pending_transfers.is_empty() {
            return;
        }

        let target_files = self.sftp_target_files_for_direction(direction);
        let conflict_action = self.settings_store.settings().sftp.conflict_action;
        let conflicts = sftp_transfer_conflicts(&pending_transfers, &target_files);
        if !conflicts.is_empty() && conflict_action == oxideterm_settings::ConflictAction::Ask {
            self.sftp_view.conflict_state = Some(SftpConflictState {
                conflicts,
                current_index: 0,
                pending_transfers,
                resolved_actions: HashMap::new(),
                apply_to_all: false,
            });
            self.sftp_view.dialog = Some(SftpDialog::Conflict);
            self.sftp_view.dismiss_context_menu();
            self.clear_sftp_selection(pane);
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
        self.execute_sftp_pending_transfers(node_id, pending_transfers, resolved_actions);
        self.clear_sftp_selection(pane);
    }

    fn queue_sftp_external_upload_paths(&mut self, paths: &[std::path::PathBuf]) {
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
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
                })
            })
            .collect::<Vec<_>>();
        if pending_transfers.is_empty() {
            return;
        }
        let conflicts = sftp_transfer_conflicts(&pending_transfers, &self.sftp_view.remote_files);
        if !conflicts.is_empty()
            && self.settings_store.settings().sftp.conflict_action
                == oxideterm_settings::ConflictAction::Ask
        {
            self.sftp_view.conflict_state = Some(SftpConflictState {
                conflicts,
                current_index: 0,
                pending_transfers,
                resolved_actions: HashMap::new(),
                apply_to_all: false,
            });
            self.sftp_view.dialog = Some(SftpDialog::Conflict);
            self.sftp_view.dismiss_context_menu();
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
        self.execute_sftp_pending_transfers(node_id, pending_transfers, resolved_actions);
    }

    fn sftp_target_files_for_direction(&self, direction: SftpTransferDirection) -> Vec<SftpFileEntry> {
        match direction {
            SftpTransferDirection::Upload => self.sftp_view.remote_files.clone(),
            SftpTransferDirection::Download => self.sftp_view.local_files.clone(),
        }
    }

    fn execute_sftp_pending_transfers(
        &mut self,
        node_id: NodeId,
        pending_transfers: Vec<SftpPendingTransfer>,
        resolved_actions: HashMap<String, SftpConflictResolution>,
    ) {
        let Some(direction) = pending_transfers.first().map(|transfer| transfer.direction) else {
            return;
        };
        let target_files = self.sftp_target_files_for_direction(direction);
        let batch_id = self.sftp_view.next_transfer_batch_id;
        self.sftp_view.next_transfer_batch_id += 1;
        let mut batch = SftpTransferBatch {
            direction,
            total: 0,
            success: 0,
            failed: 0,
            skipped: 0,
            queued: 0,
        };
        for transfer in pending_transfers {
            let resolution = resolved_actions.get(&transfer.name).copied();
            if resolution == Some(SftpConflictResolution::Skip) {
                batch.skipped += 1;
                continue;
            }
            if resolution == Some(SftpConflictResolution::SkipOlder)
                && sftp_source_not_newer_than_target(&transfer, &target_files)
            {
                batch.skipped += 1;
                continue;
            }
            let target_name = if resolution == Some(SftpConflictResolution::Rename) {
                unique_sftp_conflict_name(&transfer.name, &target_files)
            } else {
                transfer.name.clone()
            };
            if transfer.source.file_type == SftpFileType::Directory {
                batch.queued += 1;
            }
            batch.total += 1;
            self.queue_sftp_pending_transfer(node_id.clone(), transfer, target_name, Some(batch_id));
        }
        if batch.total > 0 {
            self.sftp_view.transfer_batches.insert(batch_id, batch);
        }
    }

    fn queue_sftp_pending_transfer(
        &mut self,
        node_id: NodeId,
        transfer: SftpPendingTransfer,
        target_name: String,
        batch_id: Option<u64>,
    ) {
        let direction = transfer.direction;
        let is_directory = transfer.source.file_type == SftpFileType::Directory;
        let id = self.sftp_view.next_transfer_id;
        self.sftp_view.next_transfer_id += 1;
        let transfer_id = new_sftp_transfer_id(&node_id, &transfer.name);
        let size = transfer.source.size.max(1);
        let local_path = match direction {
            SftpTransferDirection::Upload => transfer.source.path.clone(),
            SftpTransferDirection::Download => join_local_path(&self.sftp_view.local_path, &target_name),
        };
        let remote_path = match direction {
            SftpTransferDirection::Upload => join_sftp_path(&self.sftp_view.remote_path, &target_name),
            SftpTransferDirection::Download => transfer.source.path.clone(),
        };
        self.sftp_view.transfers.push(SftpTransferItem {
            id,
            transfer_id: transfer_id.clone(),
            batch_id,
            node_id: node_id.clone(),
            name: if is_directory {
                format!("{target_name}/")
            } else {
                target_name
            },
            local_path: local_path.clone(),
            remote_path: remote_path.clone(),
            direction,
            size,
            transferred: 0,
            speed: 0,
            state: SftpTransferState::Pending,
            error: None,
        });
        self.spawn_sftp_transfer_task(
            id,
            transfer_id,
            node_id,
            direction,
            is_directory,
            local_path,
            remote_path,
            None,
        );
    }

    fn toggle_sftp_conflict_apply_all(&mut self) {
        if let Some(conflict) = self.sftp_view.conflict_state.as_mut() {
            conflict.apply_to_all = !conflict.apply_to_all;
        }
    }

    fn resolve_sftp_transfer_conflict(&mut self, resolution: SftpConflictResolution) {
        let Some(mut conflict_state) = self.sftp_view.conflict_state.clone() else {
            return;
        };
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
            self.cancel_sftp_transfer_conflicts();
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            self.cancel_sftp_transfer_conflicts();
            return;
        };
        if conflict_state.conflicts.is_empty() {
            self.cancel_sftp_transfer_conflicts();
            return;
        }

        let current_index = conflict_state.current_index;
        if conflict_state.apply_to_all {
            for conflict in conflict_state.conflicts.iter().skip(current_index) {
                conflict_state
                    .resolved_actions
                    .insert(conflict.file_name.clone(), resolution);
            }
            self.sftp_view.conflict_state = None;
            self.sftp_view.dialog = None;
            self.execute_sftp_pending_transfers(
                node_id,
                conflict_state.pending_transfers,
                conflict_state.resolved_actions,
            );
            return;
        }

        if let Some(conflict) = conflict_state.conflicts.get(current_index) {
            conflict_state
                .resolved_actions
                .insert(conflict.file_name.clone(), resolution);
        }

        if current_index + 1 < conflict_state.conflicts.len() {
            conflict_state.current_index += 1;
            conflict_state.apply_to_all = false;
            self.sftp_view.conflict_state = Some(conflict_state);
            self.sftp_view.dialog = Some(SftpDialog::Conflict);
        } else {
            self.sftp_view.conflict_state = None;
            self.sftp_view.dialog = None;
            self.execute_sftp_pending_transfers(
                node_id,
                conflict_state.pending_transfers,
                conflict_state.resolved_actions,
            );
        }
    }

    fn cancel_sftp_transfer_conflicts(&mut self) {
        self.sftp_view.conflict_state = None;
        self.close_sftp_dialog();
    }

    fn update_sftp_transfer_batch_toast(&mut self, batch_id: u64, state: SftpTransferState) {
        let Some(batch) = self.sftp_view.transfer_batches.get_mut(&batch_id) else {
            return;
        };
        match state {
            SftpTransferState::Completed => batch.success += 1,
            SftpTransferState::Error => batch.failed += 1,
            _ => return,
        }

        if batch.success + batch.failed < batch.total {
            return;
        }

        let Some(batch) = self.sftp_view.transfer_batches.remove(&batch_id) else {
            return;
        };
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SftpExtractArchiveKind {
    Zip,
    Tar,
    TarGzip,
    TarBzip2,
    TarXz,
    TarZstd,
}

fn sftp_extract_archive_command(
    file_name: &str,
    archive_path: &str,
    destination_path: &str,
) -> Option<String> {
    let archive_kind = sftp_extract_archive_kind(file_name)?;
    let archive = sftp_shell_quote(archive_path);
    let destination = sftp_shell_quote(destination_path);
    // Keep extraction non-destructive until SFTP has an archive conflict dialog.
    let command = match archive_kind {
        SftpExtractArchiveKind::Zip => format!("unzip -nq {archive} -d {destination}"),
        SftpExtractArchiveKind::Tar => format!("tar -k -xf {archive} -C {destination}"),
        SftpExtractArchiveKind::TarGzip => format!("tar -k -xzf {archive} -C {destination}"),
        SftpExtractArchiveKind::TarBzip2 => format!("tar -k -xjf {archive} -C {destination}"),
        SftpExtractArchiveKind::TarXz => format!("tar -k -xJf {archive} -C {destination}"),
        SftpExtractArchiveKind::TarZstd => {
            format!("tar -k --zstd -xf {archive} -C {destination}")
        }
    };
    Some(command)
}

fn sftp_extract_archive_kind(file_name: &str) -> Option<SftpExtractArchiveKind> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        Some(SftpExtractArchiveKind::Zip)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        Some(SftpExtractArchiveKind::TarGzip)
    } else if lower.ends_with(".tar.bz2")
        || lower.ends_with(".tbz")
        || lower.ends_with(".tbz2")
    {
        Some(SftpExtractArchiveKind::TarBzip2)
    } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
        Some(SftpExtractArchiveKind::TarXz)
    } else if lower.ends_with(".tar.zst") || lower.ends_with(".tzst") {
        Some(SftpExtractArchiveKind::TarZstd)
    } else if lower.ends_with(".tar") {
        Some(SftpExtractArchiveKind::Tar)
    } else {
        None
    }
}

fn sftp_shell_quote(value: &str) -> String {
    // Remote extraction is dispatched through the user's shell, so every
    // path segment must be quoted as data instead of shell syntax.
    format!("'{}'", value.replace('\'', "'\\''"))
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

#[cfg(test)]
mod sftp_extract_archive_tests {
    use super::*;

    #[test]
    fn sftp_extract_archive_command_quotes_zip_paths() {
        let command = sftp_extract_archive_command("backup.zip", "/srv/a b/backup.zip", "/srv/a b")
            .expect("zip archives should be extractable");

        assert_eq!(command, "unzip -nq '/srv/a b/backup.zip' -d '/srv/a b'");
    }

    #[test]
    fn sftp_extract_archive_command_handles_tar_variants() {
        assert_eq!(
            sftp_extract_archive_command("app.tar", "/tmp/app.tar", "/tmp"),
            Some("tar -k -xf '/tmp/app.tar' -C '/tmp'".to_string())
        );
        assert_eq!(
            sftp_extract_archive_command("app.tgz", "/tmp/app.tgz", "/tmp"),
            Some("tar -k -xzf '/tmp/app.tgz' -C '/tmp'".to_string())
        );
        assert_eq!(
            sftp_extract_archive_command("app.tar.xz", "/tmp/app.tar.xz", "/tmp"),
            Some("tar -k -xJf '/tmp/app.tar.xz' -C '/tmp'".to_string())
        );
    }

    #[test]
    fn sftp_extract_archive_command_escapes_single_quotes() {
        let command =
            sftp_extract_archive_command("it.zip", "/srv/it's/it.zip", "/srv/it's")
                .expect("zip archives should be extractable");

        assert_eq!(command, "unzip -nq '/srv/it'\\''s/it.zip' -d '/srv/it'\\''s'");
    }

    #[test]
    fn sftp_extract_archive_command_rejects_unsupported_files() {
        assert_eq!(
            sftp_extract_archive_command("notes.txt", "/tmp/notes.txt", "/tmp"),
            None
        );
    }
}
