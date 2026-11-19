use super::*;
use oxideterm_sftp::{
    discard_sftp_relay_progress, list_incomplete_browser_transfers, update_stored_transfer_progress,
};

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn queue_quick_scp_download(&mut self, cx: &mut Context<Self>) {
        if !self.sftp_view().read(cx).editing_remote_path {
            // SCP cannot browse, so first place keyboard focus in the existing
            // remote path field and let the user provide one exact file path.
            self.start_sftp_path_edit(SftpPane::Remote, cx);
            return;
        }
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let conflict_action = self.settings_store.settings().sftp.conflict_action;
        let missing_path_error = self.i18n.t("sftp.scp.enter_remote_file_path_error");
        let Some((pending_transfers, resolved_actions)) =
            self.sftp_view().update(cx, |sftp, cx| {
                sftp.prepare_quick_scp_download(conflict_action, missing_path_error, cx)
            })
        else {
            return;
        };
        self.execute_sftp_pending_transfers(
            remote_id,
            pending_transfers,
            resolved_actions,
            None,
            cx,
        );
    }

    pub(in crate::workspace::sftp) fn spawn_sftp_incomplete_load_with_sender(
        &self,
        remote_id: SftpRemoteId,
        tx: delivery::ActiveDeliverySender<SftpWorkerResult>,
    ) {
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let progress_store = self.sftp_progress_store.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = async {
                let connection_id = match &backend {
                    SftpRemoteBackend::Ftp { .. } => remote_id.storage_key(),
                    SftpRemoteBackend::Node {
                        router, node_id, ..
                    } => {
                        router
                            .resolve_connection(node_id)
                            .await
                            .map_err(|error| error.to_string())?
                            .connection_id
                    }
                    SftpRemoteBackend::Standalone { handle } => handle.connection_id().to_string(),
                };
                list_incomplete_browser_transfers(
                    progress_store.as_ref(),
                    &connection_id,
                    remote_id.standalone_endpoint_id(),
                )
                .await
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = tx.send(SftpWorkerResult::IncompleteTransfersLoaded { remote_id, result });
        });
    }

    pub(in crate::workspace::sftp) fn spawn_sftp_background_transfer_load_with_sender(
        &self,
        remote_id: SftpRemoteId,
        tx: delivery::ActiveDeliverySender<SftpWorkerResult>,
    ) {
        let manager = self.sftp_transfer_manager.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let snapshots = manager.list_background_transfers(Some(&remote_id.storage_key()));
            let _ = tx.send(SftpWorkerResult::BackgroundTransfersLoaded {
                remote_id,
                result: Ok(snapshots),
            });
        });
    }

    pub(in crate::workspace) fn resume_sftp_incomplete_transfer(
        &mut self,
        transfer_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let launch = self.sftp_view().update(cx, |state, cx| {
            state.prepare_manual_resume(remote_id, &transfer_id, cx)
        });
        match launch {
            Some(SftpResumeLaunch::Transfer(launch)) => self.spawn_sftp_transfer_launch(launch, cx),
            Some(SftpResumeLaunch::Relay(launch)) => self.spawn_sftp_relay_launch(launch, cx),
            None => {}
        }
    }

    pub(in crate::workspace) fn discard_sftp_incomplete_transfer(
        &mut self,
        transfer_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(progress) = self
            .sftp_view()
            .read(cx)
            .incomplete_transfers
            .iter()
            .find(|progress| progress.transfer_id == transfer_id)
            .cloned()
        else {
            return;
        };
        let Some(relay) = progress.remote_relay.as_ref() else {
            return;
        };
        let profile_revision_matches = self
            .connection_store
            .get_standalone_sftp_profile(&relay.profile_id)
            .is_some_and(|profile| profile.updated_at.to_rfc3339() == relay.profile_revision);
        if !profile_revision_matches {
            let tx = self.sftp_view().read(cx).worker_sender();
            let _ = tx.send(SftpWorkerResult::IncompleteTransferDiscarded {
                transfer_id,
                result: Err(self
                    .i18n
                    .t("sftp.errors.relay_config_changed_cleanup_skipped")),
            });
            return;
        }
        let destination_remote_id = SftpRemoteId::Standalone(relay.destination_endpoint_id.clone());
        let cleanup_owner = format!("discard-{}", progress.transfer_id);
        let tx = self.sftp_view().read(cx).worker_sender();
        let Some((backend, lease)) =
            self.acquire_sftp_transfer_backend(&destination_remote_id, &cleanup_owner)
        else {
            let _ = tx.send(SftpWorkerResult::IncompleteTransferDiscarded {
                transfer_id,
                result: Err("SFTP destination endpoint is no longer available".to_string()),
            });
            return;
        };
        let progress_store = self.sftp_progress_store.clone();
        self.forwarding_runtime.spawn(async move {
            // The cleanup lease owns the destination connection until the exact
            // transfer-owned staging sibling has been removed or rejected.
            let _lease = lease;
            let result = async {
                let destination = backend.acquire_transfer_sftp().await?;
                discard_sftp_relay_progress(&destination, progress_store.as_ref(), &progress)
                    .await
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = tx.send(SftpWorkerResult::IncompleteTransferDiscarded {
                transfer_id,
                result,
            });
        });
    }

    pub(in crate::workspace) fn request_sftp_transfer_resume_for_node(
        &self,
        node_id: NodeId,
        transfer_id: String,
        cx: &App,
    ) {
        let router = self.node_router.clone();
        let progress_store = self.sftp_progress_store.clone();
        let tx = self.sftp_view().read(cx).worker_sender();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            // Tauri's reconnect resume phase first best-effort opens SFTP for
            // each affected node, then resumes transfers even if that init
            // fails. Preserve that ordering so node runtime SFTP state is
            // restored before file-only resumes take the transfer-only path.
            let _ = router.acquire_sftp(&node_id).await;
            let result = progress_store
                .load(&transfer_id)
                .await
                .map_err(|error| error.to_string())
                .and_then(|progress| {
                    progress.ok_or_else(|| "Transfer not found in progress store".to_string())
                });
            let _ = tx.send(SftpWorkerResult::ResumeIncompleteTransferLoaded {
                remote_id: SftpRemoteId::Node(node_id),
                transfer_id,
                result,
            });
        });
    }

    pub(in crate::workspace::sftp) fn spawn_sftp_transfer_launch(
        &self,
        launch: SftpTransferLaunch,
        cx: &App,
    ) {
        let tx = self.sftp_view().read(cx).worker_sender();
        self.spawn_sftp_transfer_launch_with_sender(launch, tx);
    }

    pub(in crate::workspace::sftp) fn spawn_sftp_relay_launch(
        &self,
        mut launch: SftpRelayLaunch,
        cx: &App,
    ) {
        let tx = self.sftp_view().read(cx).worker_sender();
        let transfer_id = launch.request.transfer_id.clone();
        let id = launch.id;
        let secondary_remote_id = launch.remote_id.clone();
        let Some((source_backend, source_lease)) =
            self.acquire_sftp_transfer_backend(&launch.source, &transfer_id)
        else {
            let _ = tx.send(SftpWorkerResult::TransferComplete {
                remote_id: secondary_remote_id,
                transfer_id,
                id,
                result: Err("SFTP source endpoint is no longer available".to_string()),
                refresh_remote: false,
                refresh_local: false,
            });
            return;
        };
        let Some((destination_backend, destination_lease)) =
            self.acquire_sftp_transfer_backend(&launch.destination, &transfer_id)
        else {
            let _ = tx.send(SftpWorkerResult::TransferComplete {
                remote_id: secondary_remote_id,
                transfer_id,
                id,
                result: Err("SFTP destination endpoint is no longer available".to_string()),
                refresh_remote: false,
                refresh_local: false,
            });
            return;
        };
        let manager = self.sftp_transfer_manager.clone();
        let progress_store = self.sftp_progress_store.clone();
        let runtime = self.forwarding_runtime.clone();
        let _control = manager.register_for_node(&transfer_id, launch.storage_key());
        let profile_revision = launch.profile_id.as_deref().and_then(|profile_id| {
            self.connection_store
                .get_standalone_sftp_profile(profile_id)
                .map(|profile| profile.updated_at.to_rfc3339())
        });
        launch.bind_profile_revision(profile_revision);
        let request = launch.request;
        runtime.spawn(async move {
            // Both leases outlive the tab and are released together when the relay finishes.
            let _source_lease = source_lease;
            let _destination_lease = destination_lease;
            let _control_guard = SftpTransferGuard::new(Some(&manager), transfer_id.clone());
            let _permit = manager.acquire_permit().await;
            let source = match source_backend.acquire_transfer_sftp().await {
                Ok(source) => source,
                Err(error) => {
                    let _ = tx.send(SftpWorkerResult::TransferComplete {
                        remote_id: secondary_remote_id,
                        transfer_id,
                        id,
                        result: Err(error),
                        refresh_remote: false,
                        refresh_local: false,
                    });
                    return;
                }
            };
            let destination = match destination_backend.acquire_transfer_sftp().await {
                Ok(destination) => destination,
                Err(error) => {
                    let _ = tx.send(SftpWorkerResult::TransferComplete {
                        remote_id: secondary_remote_id,
                        transfer_id,
                        id,
                        result: Err(error),
                        refresh_remote: false,
                        refresh_local: false,
                    });
                    return;
                }
            };
            let progress_delivery = tx.clone();
            let can_refresh = request.is_directory || request.context.is_some();
            let result = oxideterm_sftp::run_sftp_relay(
                source,
                destination,
                manager.clone(),
                progress_store,
                request,
                move |event| {
                    let _ =
                        progress_delivery.send(SftpWorkerResult::from_transfer_event(id, event));
                },
            )
            .await;
            let _ = tx.send(SftpWorkerResult::TransferComplete {
                remote_id: secondary_remote_id,
                transfer_id,
                id,
                result,
                refresh_remote: can_refresh,
                refresh_local: can_refresh,
            });
        });
    }

    pub(in crate::workspace::sftp) fn spawn_sftp_transfer_launch_with_sender(
        &self,
        launch: SftpTransferLaunch,
        tx: delivery::ActiveDeliverySender<SftpWorkerResult>,
    ) {
        let SftpTransferLaunch {
            id,
            transfer_id,
            remote_id,
            direction,
            is_directory,
            local_path,
            remote_path,
            resume_progress,
            download_disposition,
            protocol_override,
        } = launch;
        use oxideterm_sftp::{
            FtpTransferRequest, SshTransferError, SshTransferPreference, SshTransferRequest,
            run_ftp_transfer, run_ssh_transfer,
        };
        let preference = match self.settings_store.settings().sftp.transfer_protocol {
            oxideterm_settings::FileTransferProtocolPreference::Auto => SshTransferPreference::Auto,
            oxideterm_settings::FileTransferProtocolPreference::Sftp => SshTransferPreference::Sftp,
            oxideterm_settings::FileTransferProtocolPreference::Scp => SshTransferPreference::Scp,
        };
        let scp_unavailable_error = self.i18n.t("sftp.errors.scp_unavailable");
        let transfer_protocol_unavailable_error =
            self.i18n.t("sftp.errors.transfer_protocol_unavailable");
        let Some((backend, standalone_lease)) =
            self.acquire_sftp_transfer_backend(&remote_id, &transfer_id)
        else {
            let _ = tx.send(SftpWorkerResult::TransferComplete {
                remote_id,
                transfer_id,
                id,
                result: Err("SFTP endpoint is no longer available".to_string()),
                refresh_remote: false,
                refresh_local: false,
            });
            return;
        };
        let manager = self.sftp_transfer_manager.clone();
        let progress_store = self.sftp_progress_store.clone();
        let runtime = self.forwarding_runtime.clone();
        let request_audit = oxideterm_audit::AuditContext::current_request().or_else(|| {
            oxideterm_audit::AuditContext::current().map(|mut context| {
                context.source = oxideterm_audit::AuditSource::User;
                context
            })
        });
        // The runtime owns cancellation from enqueue through completion, even
        // while no SFTP tab is visible or a jump-chain reconnect is in flight.
        let remote_storage_key = remote_id.storage_key();
        let _control = manager.register_for_node(&transfer_id, remote_storage_key.clone());
        let scoped_request = request_audit.clone();
        let task = async move {
            // The transfer lease keeps an independent SFTP endpoint alive after tab closure.
            let _standalone_lease = standalone_lease;
            let _control_guard = SftpTransferGuard::new(Some(&manager), transfer_id.clone());
            let _permit = manager.acquire_permit().await;
            if let Err(error) = manager
                .check_browser_transfer_start(&transfer_id, progress_store.as_ref())
                .await
            {
                let _ = tx.send(SftpWorkerResult::TransferComplete {
                    remote_id,
                    transfer_id,
                    id,
                    result: Err(error.to_string()),
                    refresh_remote: false,
                    refresh_local: false,
                });
                return;
            }
            if let SftpRemoteBackend::Ftp { runtime } = &backend {
                let result = run_ftp_transfer(
                    runtime,
                    &manager,
                    FtpTransferRequest {
                        transfer_id: &transfer_id,
                        direction,
                        directory: is_directory,
                        local: &local_path,
                        remote: &remote_path,
                        disposition: download_disposition,
                        owner: &remote_storage_key,
                        audit_context: request_audit.as_ref(),
                    },
                    |event| {
                        let _ = tx.send(SftpWorkerResult::from_transfer_event(id, event));
                    },
                )
                .await;
                let _ = tx.send(SftpWorkerResult::TransferComplete {
                    remote_id,
                    transfer_id,
                    id,
                    result,
                    refresh_remote: direction == SftpTransferDirection::Upload,
                    refresh_local: direction == SftpTransferDirection::Download,
                });
                return;
            }
            let resolved_handle = match backend.resolve_connection().await {
                Ok(handle) => handle,
                Err(error) => {
                    let _ = tx.send(SftpWorkerResult::TransferComplete {
                        remote_id,
                        transfer_id,
                        id,
                        result: Err(error),
                        refresh_remote: false,
                        refresh_local: false,
                    });
                    return;
                }
            };
            let resolved_connection_id = resolved_handle.connection_id().to_string();
            let transfer_audit = resolved_handle.audit_context().map(|context| {
                let request = match &backend {
                    SftpRemoteBackend::Node {
                        router, node_id, ..
                    } => router.audit_context(node_id).map(|node| {
                        request_audit
                            .as_ref()
                            .map_or(node.clone(), |request| node.with_request(request))
                    }),
                    _ => request_audit.clone(),
                };
                request
                    .as_ref()
                    .map_or(context.clone(), |request| context.with_request(request))
            });
            let progress_delivery = tx.clone();
            let transfer = run_ssh_transfer(
                manager.clone(),
                progress_store,
                &resolved_handle,
                resolved_connection_id,
                || backend.acquire_transfer_sftp(),
                SshTransferRequest {
                    transfer_id: transfer_id.clone(),
                    owner: remote_storage_key,
                    direction,
                    is_directory,
                    local_path,
                    remote_path,
                    resume_progress,
                    download_disposition,
                    protocol_override,
                    preference,
                    audit_context: transfer_audit,
                },
                move |event| {
                    let _ =
                        progress_delivery.send(SftpWorkerResult::from_transfer_event(id, event));
                },
            )
            .await;
            let started = matches!(transfer, Ok(()) | Err(SshTransferError::Transfer(_)));
            let result = transfer.map_err(|error| match error {
                SshTransferError::ScpUnavailable => scp_unavailable_error,
                SshTransferError::ProtocolUnavailable => transfer_protocol_unavailable_error,
                SshTransferError::OpenChannel(error) | SshTransferError::Transfer(error) => error,
            });
            let _ = tx.send(SftpWorkerResult::TransferComplete {
                remote_id,
                transfer_id,
                id,
                result,
                refresh_remote: started && direction == SftpTransferDirection::Upload,
                refresh_local: started && direction == SftpTransferDirection::Download,
            });
        };
        runtime.spawn(async move {
            if let Some(context) = scoped_request {
                context.scope(task).await;
            } else {
                task.await;
            }
        });
    }

    pub(in crate::workspace) fn set_sftp_transfer_state(
        &mut self,
        id: u64,
        state: SftpTransferState,
        cx: &mut Context<Self>,
    ) {
        let Some(control) = self
            .sftp_view()
            .update(cx, |sftp, cx| sftp.set_transfer_state(id, state, cx))
        else {
            return;
        };
        match control {
            SftpTransferControl::Pause(transfer_id) => {
                self.sftp_transfer_manager.pause(&transfer_id);
                let progress_store = self.sftp_progress_store.clone();
                self.forwarding_runtime.spawn(async move {
                    let _ = update_stored_transfer_progress(
                        progress_store.as_ref(),
                        &transfer_id,
                        StoredTransferProgress::mark_paused,
                    )
                    .await;
                });
            }
            SftpTransferControl::Resume(transfer_id) => {
                self.sftp_transfer_manager.resume(&transfer_id);
                let progress_store = self.sftp_progress_store.clone();
                self.forwarding_runtime.spawn(async move {
                    let _ = update_stored_transfer_progress(
                        progress_store.as_ref(),
                        &transfer_id,
                        StoredTransferProgress::mark_active,
                    )
                    .await;
                });
            }
            SftpTransferControl::Cancel(transfer_id) => {
                self.sftp_transfer_manager.cancel(&transfer_id);
            }
        }
    }

    pub(in crate::workspace) fn cancel_or_remove_sftp_transfer(
        &mut self,
        id: u64,
        cx: &mut Context<Self>,
    ) {
        if let Some(SftpTransferControl::Cancel(transfer_id)) = self
            .sftp_view()
            .update(cx, |sftp, cx| sftp.cancel_or_remove_transfer(id, cx))
        {
            self.sftp_transfer_manager.cancel(&transfer_id);
        }
    }

    pub(in crate::workspace) fn interrupt_sftp_transfers_by_node(
        &mut self,
        node_id: &NodeId,
        error: String,
        cx: &mut Context<Self>,
    ) -> bool {
        // Runtime ownership is authoritative because reconnect can resume a
        // transfer without materializing a row in the currently visible view.
        let transfer_ids_to_interrupt = self
            .sftp_transfer_manager
            .interrupt_node(&node_id.0, error.clone());
        let mut changed = !transfer_ids_to_interrupt.is_empty();
        changed |= self.sftp_view().update(cx, |sftp, cx| {
            sftp.interrupt_transfers_by_remote(&SftpRemoteId::Node(node_id.clone()), &error, cx)
        });
        for transfer_id in transfer_ids_to_interrupt {
            let progress_store = self.sftp_progress_store.clone();
            let error = error.clone();
            self.forwarding_runtime.spawn(async move {
                let _ = update_stored_transfer_progress(
                    progress_store.as_ref(),
                    &transfer_id,
                    |progress| progress.mark_failed(error),
                )
                .await;
            });
        }
        changed
    }
}
