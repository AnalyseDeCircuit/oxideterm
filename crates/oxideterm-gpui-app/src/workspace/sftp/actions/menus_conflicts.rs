use super::*;

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
        let remote_directory = {
            let sftp = self.sftp_view().read(cx);
            if file.path.is_empty() {
                sftp.remote_path.clone()
            } else {
                let parent = parent_path(&file.path, true);
                if parent.is_empty() {
                    sftp.remote_path.clone()
                } else {
                    parent
                }
            }
        };
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
        let tx = self.sftp_view().read(cx).worker_sender();
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
        self.sftp_view()
            .update(cx, |sftp, cx| sftp.dismiss_context_menu(cx));
    }

    pub(in crate::workspace::sftp) fn queue_sftp_transfers(
        &mut self,
        pane: SftpPane,
        direction: SftpTransferDirection,
        cx: &mut Context<Self>,
    ) {
        if pane == SftpPane::Remote && self.sftp_tree_mode(cx) {
            if let Some(file) = self.sftp_view().read(cx).selected_tree_file() {
                self.queue_sftp_tree_download(file, cx);
            }
            return;
        }
        let selected = self.sftp_view().read(cx).selected_transfer_names(pane);
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
            self.sftp_view()
                .read(cx)
                .pending_named_transfers(pane, direction, selected_names);
        if pending_transfers.is_empty() {
            return;
        }
        self.queue_interactive_sftp_transfers(remote_id, pending_transfers, Some(pane), cx);
    }

    /// Queue a download for one absolute tree row. The flat selection cannot
    /// represent nested rows, so the entry itself is the transfer source.
    pub(in crate::workspace::sftp) fn queue_sftp_tree_download(
        &mut self,
        file: SftpFileEntry,
        cx: &mut Context<Self>,
    ) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let pending = self.sftp_view().read(cx).pending_tree_download(file);
        self.queue_interactive_sftp_transfers(remote_id, vec![pending], Some(SftpPane::Remote), cx);
    }

    fn queue_interactive_sftp_transfers(
        &mut self,
        remote_id: SftpRemoteId,
        pending: Vec<SftpPendingTransfer>,
        clear_selection: Option<SftpPane>,
        cx: &mut Context<Self>,
    ) {
        if pending.is_empty() {
            return;
        }
        let primary = self.sftp_pair_primary_remote_id(cx);
        let protocol = if primary.is_some() {
            RemoteTransferProtocol::Sftp
        } else {
            self.transfer_protocol_for_remote(&remote_id)
        };
        let conflict_action = self.settings_store.settings().sftp.conflict_action;
        let launches = self.sftp_view().update(cx, |state, cx| {
            state.prepare_interactive_transfers(
                remote_id.clone(),
                pending,
                clear_selection,
                conflict_action,
                protocol,
                cx,
            )
        });
        self.launch_sftp_batch(remote_id, primary, launches, cx);
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
        self.queue_interactive_sftp_transfers(
            remote_id,
            build_sftp_pending_uploads(paths),
            None,
            cx,
        );
    }

    pub(in crate::workspace::sftp) fn execute_sftp_pending_transfers(
        &mut self,
        remote_id: SftpRemoteId,
        pending_transfers: Vec<SftpPendingTransfer>,
        resolved_actions: HashMap<String, SftpConflictResolution>,
        upload_target: Option<(&str, &[SftpFileEntry])>,
        cx: &mut Context<Self>,
    ) {
        // Explicit upload targets (terminal drop) must never inherit a pair
        // view from the currently visible SFTP surface.
        let pair_primary_remote_id = upload_target
            .is_none()
            .then(|| self.sftp_pair_primary_remote_id(cx))
            .flatten();
        let configured_protocol = if pair_primary_remote_id.is_some() {
            RemoteTransferProtocol::Sftp
        } else {
            self.transfer_protocol_for_remote(&remote_id)
        };
        let launches = self.sftp_view().update(cx, |sftp, cx| {
            sftp.prepare_transfer_launches(
                remote_id.clone(),
                pending_transfers,
                resolved_actions,
                upload_target,
                configured_protocol,
                cx,
            )
        });
        self.launch_sftp_batch(remote_id, pair_primary_remote_id, launches, cx);
    }

    fn launch_sftp_batch(
        &mut self,
        remote_id: SftpRemoteId,
        pair_primary_remote_id: Option<SftpRemoteId>,
        launches: Vec<SftpTransferLaunch>,
        cx: &mut Context<Self>,
    ) {
        for launch in launches {
            if let Some(primary) = pair_primary_remote_id.clone() {
                self.spawn_sftp_relay_launch(launch.into_relay(primary, remote_id.clone()), cx);
            } else {
                self.spawn_sftp_transfer_launch(launch, cx);
            }
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
        match self.sftp_view().update(cx, |sftp, cx| {
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
                    None,
                    cx,
                );
            }
        }
    }

    pub(in crate::workspace::sftp) fn cancel_sftp_transfer_conflicts(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view()
            .update(cx, |sftp, cx| sftp.cancel_transfer_conflicts(cx));
        self.close_sftp_dialog(cx);
    }
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
mod tests {
    use super::*;
    use gpui::TestAppContext;

    fn file(name: &str, path: &str, file_type: SftpFileType) -> SftpFileEntry {
        SftpFileEntry {
            name: name.to_string(),
            path: path.to_string(),
            file_type,
            size_known: true,
            size: 1,
            modified: None,
            permissions: None,
            owner: None,
            group: None,
            is_symlink: false,
            symlink_target: None,
        }
    }

    fn pending(name: &str) -> SftpPendingTransfer {
        SftpPendingTransfer {
            name: name.to_string(),
            direction: SftpTransferDirection::Upload,
            source: SftpFileEntry {
                name: name.to_string(),
                path: format!("C:\\local\\{name}"),
                file_type: SftpFileType::File,
                size_known: true,
                size: 1,
                modified: None,
                permissions: None,
                owner: None,
                group: None,
                is_symlink: false,
                symlink_target: None,
            },
            protocol_override: None,
        }
    }

    #[gpui::test]
    fn terminal_drop_uploads_use_explicit_target_directory(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        entity.update(cx, |sftp, _cx| {
            sftp.remote_path = "/visible".to_string();
        });
        let target_files = vec![file(
            "existing.txt",
            "/target/existing.txt",
            SftpFileType::File,
        )];
        let launches = entity.update(cx, |sftp, cx| {
            sftp.prepare_transfer_launches(
                SftpRemoteId::Node(NodeId::new("node")),
                vec![pending("dropped.txt")],
                HashMap::new(),
                Some(("/target", &target_files)),
                RemoteTransferProtocol::Sftp,
                cx,
            )
        });
        assert_eq!(launches.len(), 1);
        assert_eq!(launches[0].remote_path, "/target/dropped.txt");
        assert_ne!(launches[0].remote_path, "/visible/dropped.txt");
    }

    #[gpui::test]
    fn terminal_drop_rename_conflict_uses_target_snapshot(cx: &mut TestAppContext) {
        let entity = cx.new(SftpWorkspaceEntity::new);
        let target_files = vec![file(
            "dropped.txt",
            "/target/dropped.txt",
            SftpFileType::File,
        )];
        let resolved_actions =
            HashMap::from([("dropped.txt".to_string(), SftpConflictResolution::Rename)]);
        let launches = entity.update(cx, |sftp, cx| {
            sftp.prepare_transfer_launches(
                SftpRemoteId::Node(NodeId::new("node")),
                vec![pending("dropped.txt")],
                resolved_actions,
                Some(("/target", &target_files)),
                RemoteTransferProtocol::Sftp,
                cx,
            )
        });
        assert_eq!(launches.len(), 1);
        assert_eq!(launches[0].remote_path, "/target/dropped (1).txt");
    }

    #[test]
    fn terminal_drop_ask_keeps_both_files() {
        assert_eq!(
            sftp_terminal_drop_conflict_resolution(oxideterm_settings::ConflictAction::Ask),
            SftpConflictResolution::Rename
        );
        assert_eq!(
            sftp_terminal_drop_conflict_resolution(oxideterm_settings::ConflictAction::Overwrite),
            SftpConflictResolution::Overwrite
        );
        assert_eq!(
            sftp_terminal_drop_conflict_resolution(oxideterm_settings::ConflictAction::Skip),
            SftpConflictResolution::Skip
        );
        assert_eq!(
            sftp_terminal_drop_conflict_resolution(oxideterm_settings::ConflictAction::Rename),
            SftpConflictResolution::Rename
        );
    }
}
