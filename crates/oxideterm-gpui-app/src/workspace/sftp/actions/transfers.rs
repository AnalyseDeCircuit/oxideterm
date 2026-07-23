use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn queue_quick_scp_download(&mut self) {
        if !self.sftp_view.editing_remote_path {
            // SCP cannot browse, so first place keyboard focus in the existing
            // remote path field and let the user provide one exact file path.
            self.start_sftp_path_edit(SftpPane::Remote);
            return;
        }
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let remote_path = self.sftp_view.remote_path_input.trim();
        let Some(name) = remote_path
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        else {
            self.sftp_view.init_error = Some(self.i18n.t("sftp.scp.enter_remote_file_path_error"));
            return;
        };
        let pending_transfers = vec![SftpPendingTransfer {
            name: name.to_string(),
            direction: SftpTransferDirection::Download,
            source: SftpFileEntry {
                name: name.to_string(),
                path: remote_path.to_string(),
                file_type: SftpFileType::File,
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
        let target_files = self.sftp_view.local_files.clone();
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
            self.sftp_view.set_dialog(SftpDialog::Conflict);
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
            .collect();
        self.execute_sftp_pending_transfers(node_id, pending_transfers, resolved_actions);
    }

    pub(in crate::workspace::sftp) fn spawn_sftp_incomplete_load(&mut self, node_id: NodeId) {
        if self.sftp_view.incomplete_load_inflight {
            return;
        }
        self.sftp_view.incomplete_load_inflight = true;
        let router = self.node_router.clone();
        let progress_store = self.sftp_progress_store.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = async {
                let resolved = router
                    .resolve_connection(&node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                progress_store
                    .list_incomplete(&resolved.connection_id)
                    .await
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = tx.send(SftpWorkerResult::IncompleteTransfersLoaded { node_id, result });
        });
    }

    pub(in crate::workspace::sftp) fn spawn_sftp_background_transfer_load(
        &mut self,
        node_id: NodeId,
    ) {
        let manager = self.sftp_transfer_manager.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let snapshots = manager.list_background_transfers(Some(&node_id.0));
            let _ = tx.send(SftpWorkerResult::BackgroundTransfersLoaded {
                node_id,
                result: Ok(snapshots),
            });
        });
    }

    pub(in crate::workspace::sftp) fn resume_sftp_incomplete_transfer(
        &mut self,
        transfer_id: String,
    ) {
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let Some(progress) = self
            .sftp_view
            .incomplete_transfers
            .iter()
            .find(|progress| progress.transfer_id == transfer_id)
            .cloned()
        else {
            return;
        };
        if !progress.is_incomplete() {
            return;
        }

        self.sftp_view
            .incomplete_transfers
            .retain(|progress| progress.transfer_id != transfer_id);
        if self.sftp_view.incomplete_transfers.is_empty() {
            self.sftp_view.show_incomplete = false;
        }

        let direction = match progress.transfer_type {
            RemoteTransferType::Upload => SftpTransferDirection::Upload,
            RemoteTransferType::Download => SftpTransferDirection::Download,
        };
        let (local_path, remote_path) = match direction {
            SftpTransferDirection::Upload => (
                progress.source_path.to_string_lossy().to_string(),
                progress.destination_path.to_string_lossy().to_string(),
            ),
            SftpTransferDirection::Download => (
                progress.destination_path.to_string_lossy().to_string(),
                progress.source_path.to_string_lossy().to_string(),
            ),
        };
        let name = progress
            .source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| progress.source_path.to_str().unwrap_or(""))
            .to_string();
        let is_directory = progress.is_directory();
        let id = self.sftp_view.next_transfer_id;
        self.sftp_view.next_transfer_id += 1;
        self.sftp_view.transfers.push(SftpTransferItem {
            id,
            transfer_id: transfer_id.clone(),
            batch_id: None,
            node_id: node_id.clone(),
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
        self.spawn_sftp_transfer_task(
            id,
            transfer_id,
            node_id,
            direction,
            is_directory,
            local_path,
            remote_path,
            Some(progress),
            None,
        );
    }

    pub(in crate::workspace) fn request_sftp_transfer_resume_for_node(
        &self,
        node_id: NodeId,
        transfer_id: String,
    ) {
        let router = self.node_router.clone();
        let progress_store = self.sftp_progress_store.clone();
        let tx = self.sftp_worker_tx.clone();
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
                node_id,
                transfer_id,
                result,
            });
        });
    }

    pub(in crate::workspace::sftp) fn queue_sftp_resume_transfer_for_node(
        &mut self,
        node_id: NodeId,
        progress: StoredTransferProgress,
    ) -> bool {
        if !progress.is_incomplete() || !progress.protocol.supports_restart_resume() {
            return false;
        }
        let direction = match progress.transfer_type {
            RemoteTransferType::Upload => SftpTransferDirection::Upload,
            RemoteTransferType::Download => SftpTransferDirection::Download,
        };
        let (local_path, remote_path) = match direction {
            SftpTransferDirection::Upload => (
                progress.source_path.to_string_lossy().to_string(),
                progress.destination_path.to_string_lossy().to_string(),
            ),
            SftpTransferDirection::Download => (
                progress.destination_path.to_string_lossy().to_string(),
                progress.source_path.to_string_lossy().to_string(),
            ),
        };
        let name = progress
            .source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| progress.source_path.to_str().unwrap_or(""))
            .to_string();
        let is_directory = progress.is_directory();
        let id = self.sftp_view.next_transfer_id;
        self.sftp_view.next_transfer_id += 1;

        if self
            .main_window_tabs
            .active_tab_id
            .and_then(|tab_id| self.sftp_tab_nodes.get(&tab_id))
            == Some(&node_id)
        {
            self.sftp_view
                .incomplete_transfers
                .retain(|item| item.transfer_id != progress.transfer_id);
            if self.sftp_view.incomplete_transfers.is_empty() {
                self.sftp_view.show_incomplete = false;
            }
            self.sftp_view.transfers.push(SftpTransferItem {
                id,
                transfer_id: progress.transfer_id.clone(),
                batch_id: None,
                node_id: node_id.clone(),
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

        // This is the native equivalent of Tauri's node_sftp_resume_transfer:
        // the transfer owner is the node/router-backed manager, not the SFTP
        // tab. The UI row is optional; reconnect must still resume in the
        // background when no SFTP tab is focused.
        self.spawn_sftp_transfer_task(
            id,
            progress.transfer_id.clone(),
            node_id,
            direction,
            is_directory,
            local_path,
            remote_path,
            Some(progress),
            None,
        );
        true
    }

    pub(in crate::workspace::sftp) fn spawn_sftp_transfer_task(
        &self,
        id: u64,
        transfer_id: String,
        node_id: NodeId,
        direction: SftpTransferDirection,
        is_directory: bool,
        local_path: String,
        remote_path: String,
        resume_progress: Option<StoredTransferProgress>,
        protocol_override: Option<RemoteTransferProtocol>,
    ) {
        let protocol_preference = self.settings_store.settings().sftp.transfer_protocol;
        let scp_unavailable_error = self.i18n.t("sftp.errors.scp_unavailable");
        let transfer_protocol_unavailable_error =
            self.i18n.t("sftp.errors.transfer_protocol_unavailable");
        let router = self.node_router.clone();
        let manager = self.sftp_transfer_manager.clone();
        let progress_store = self.sftp_progress_store.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        // The runtime owns cancellation from enqueue through completion, even
        // while no SFTP tab is visible or a jump-chain reconnect is in flight.
        let _control = manager.register_for_node(&transfer_id, node_id.0.clone());
        runtime.spawn(async move {
            let _control_guard =
                SftpTransferGuard::new(Some(&manager), transfer_id.clone());
            let _permit = manager.acquire_permit().await;
            if let Err(error) = manager.check_control(&transfer_id).await {
                if matches!(error, SftpError::TransferCancelled) {
                    let _ = progress_store.delete(&transfer_id).await;
                }
                let _ = tx.send(SftpWorkerResult::TransferComplete {
                    node_id,
                    transfer_id,
                    id,
                    result: Err(error.to_string()),
                    refresh_remote: false,
                    refresh_local: false,
                });
                return;
            }
            let resolved = match router.resolve_connection(&node_id).await {
                Ok(resolved) => resolved,
                Err(error) => {
                    let error = error.to_string();
                    let _ = tx.send(SftpWorkerResult::TransferComplete {
                        node_id,
                        transfer_id,
                        id,
                        result: Err(error),
                        refresh_remote: false,
                        refresh_local: false,
                    });
                    return;
                }
            };
            let resolved_connection_id = resolved.connection_id.clone();
            let protocol = match resume_progress
                .as_ref()
                .map(|progress| progress.protocol)
                .or(protocol_override)
            {
                Some(protocol) => protocol,
                None => match protocol_preference {
                    oxideterm_settings::FileTransferProtocolPreference::Sftp => {
                        RemoteTransferProtocol::Sftp
                    }
                    oxideterm_settings::FileTransferProtocolPreference::Scp => {
                        let capabilities = manager
                            .scp_capabilities(&resolved.connection_id, &resolved.handle)
                            .await;
                        if !capabilities.supports_scp {
                            let _ = tx.send(SftpWorkerResult::TransferComplete {
                                node_id,
                                transfer_id,
                                id,
                                result: Err(scp_unavailable_error),
                                refresh_remote: false,
                                refresh_local: false,
                            });
                            return;
                        }
                        RemoteTransferProtocol::Scp
                    }
                    oxideterm_settings::FileTransferProtocolPreference::Auto => {
                        if router.acquire_sftp(&node_id).await.is_ok() {
                            RemoteTransferProtocol::Sftp
                        } else {
                            let capabilities = manager
                                .scp_capabilities(&resolved.connection_id, &resolved.handle)
                                .await;
                            if !capabilities.supports_scp {
                                let _ = tx.send(SftpWorkerResult::TransferComplete {
                                    node_id,
                                    transfer_id,
                                    id,
                                    result: Err(transfer_protocol_unavailable_error),
                                    refresh_remote: false,
                                    refresh_local: false,
                                });
                                return;
                            }
                            RemoteTransferProtocol::Scp
                        }
                    }
                },
            };
            let _ = tx.send(SftpWorkerResult::TransferProtocolResolved { id, protocol });
            let resume_directory_strategy = resume_progress
                .as_ref()
                .filter(|_| is_directory)
                .map(|progress| progress.strategy.clone());
            let mut directory_progress =
                (is_directory || protocol == RemoteTransferProtocol::Scp).then(|| {
                if let Some(mut progress) = resume_progress.clone() {
                    progress.mark_active();
                    if protocol == RemoteTransferProtocol::Scp {
                        // Legacy SCP retries from byte zero after a channel or app restart.
                        progress.transferred_bytes = 0;
                    }
                    // Reconnect creates a new connection generation. Move the
                    // resumable record to the transport that will execute it.
                    progress.session_id = resolved_connection_id.clone();
                    return progress;
                }
                let transfer_type = match direction {
                    SftpTransferDirection::Upload => RemoteTransferType::Upload,
                    SftpTransferDirection::Download => RemoteTransferType::Download,
                };
                let mut progress = StoredTransferProgress::new(
                    transfer_id.clone(),
                    transfer_type,
                    match direction {
                        SftpTransferDirection::Upload => local_path.clone().into(),
                        SftpTransferDirection::Download => remote_path.clone().into(),
                    },
                    match direction {
                        SftpTransferDirection::Upload => remote_path.clone().into(),
                        SftpTransferDirection::Download => local_path.clone().into(),
                    },
                    0,
                    resolved_connection_id.clone(),
                );
                progress.protocol = protocol;
                progress.strategy = if is_directory {
                    RemoteTransferStrategy::DirectoryRecursive
                } else {
                    RemoteTransferStrategy::File
                };
                progress
            });
            if let Some(progress) = directory_progress.as_ref() {
                let _ = progress_store.save(progress).await;
            }
            if is_directory || protocol == RemoteTransferProtocol::Scp {
                let name_path = match direction {
                    SftpTransferDirection::Upload => &local_path,
                    SftpTransferDirection::Download => &remote_path,
                };
                let name = std::path::Path::new(name_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or(name_path)
                    .to_string();
                let name = if !is_directory || name.ends_with('/') {
                    name
                } else {
                    format!("{name}/")
                };
                let (background_direction, strategy, transferred, total) =
                    if let Some(progress) = directory_progress.as_ref() {
                        (
                            match progress.transfer_type {
                                RemoteTransferType::Upload => BackgroundTransferDirection::Upload,
                                RemoteTransferType::Download => {
                                    BackgroundTransferDirection::Download
                                }
                            },
                            progress.strategy.clone(),
                            progress.transferred_bytes,
                            progress.total_bytes,
                        )
                    } else {
                        (
                            match direction {
                                SftpTransferDirection::Upload => BackgroundTransferDirection::Upload,
                                SftpTransferDirection::Download => {
                                    BackgroundTransferDirection::Download
                                }
                            },
                            RemoteTransferStrategy::DirectoryRecursive,
                            0,
                            0,
                        )
                    };
                let mut snapshot = BackgroundTransferSnapshot::new(
                    transfer_id.clone(),
                    node_id.0.clone(),
                    name,
                    local_path.clone(),
                    remote_path.clone(),
                    background_direction,
                    if is_directory {
                        BackgroundTransferKind::Directory
                    } else {
                        BackgroundTransferKind::File
                    },
                    strategy,
                    total,
                    transferred,
                );
                snapshot.protocol = protocol;
                manager.register_background_transfer(snapshot);
            }
            let _ = tx.send(SftpWorkerResult::TransferProgress {
                id,
                transferred: 0,
                total: 0,
                speed: 0,
                state: SftpTransferState::Active,
                error: None,
            });
            let (progress_tx, mut progress_rx) =
                tokio::sync::mpsc::channel::<TransferProgress>(100);
            let progress_ui_tx = tx.clone();
            let progress_store_for_task = progress_store.clone();
            let progress_manager = manager.clone();
            let progress_transfer_id = transfer_id.clone();
            tokio::spawn(async move {
                let mut accumulator = DirectoryProgressAccumulator::default();
                let mut last_directory_progress_save = std::time::Instant::now();
                while let Some(progress) = progress_rx.recv().await {
                    let progress = if is_directory {
                        accumulator.update(progress)
                    } else {
                        progress
                    };
                    if let Some(stored) = directory_progress.as_mut() {
                        stored.total_bytes = stored.total_bytes.max(progress.total_bytes);
                        stored.update_progress(progress.transferred_bytes);
                        if last_directory_progress_save.elapsed()
                            >= std::time::Duration::from_millis(
                                SFTP_DIRECTORY_PROGRESS_SAVE_INTERVAL_MS,
                            )
                        {
                            // The transfer task records terminal directory states; this task only
                            // needs periodic snapshots for resume after process interruption.
                            let _ = progress_store_for_task.save(stored).await;
                            last_directory_progress_save = std::time::Instant::now();
                        }
                    }
                    if is_directory || protocol == RemoteTransferProtocol::Scp {
                        progress_manager.update_background_transfer_progress(
                            &progress_transfer_id,
                            progress.transferred_bytes,
                            progress.total_bytes,
                            progress.speed,
                        );
                    }
                    let _ = progress_ui_tx.send(SftpWorkerResult::TransferProgress {
                        id,
                        transferred: progress.transferred_bytes,
                        total: progress.total_bytes,
                        speed: progress.speed,
                        state: sftp_transfer_state_from_remote(progress.state),
                        error: progress.error,
                    });
                }
            });

            let result = async {
                if is_directory || protocol == RemoteTransferProtocol::Scp {
                    manager.mark_background_transfer_active(&transfer_id);
                }
                let item_count = if protocol == RemoteTransferProtocol::Scp {
                    let result = match (direction, is_directory) {
                        (SftpTransferDirection::Upload, false) => scp_upload_file(
                            &resolved.handle,
                            &local_path,
                            &remote_path,
                            &transfer_id,
                            Some(progress_tx),
                            Some(manager.clone()),
                        )
                        .await,
                        (SftpTransferDirection::Download, false) => scp_download_file(
                            &resolved.handle,
                            &remote_path,
                            &local_path,
                            &transfer_id,
                            Some(progress_tx),
                            Some(manager.clone()),
                        )
                        .await,
                        (SftpTransferDirection::Upload, true) => scp_upload_directory(
                            &resolved.handle,
                            &local_path,
                            &remote_path,
                            &transfer_id,
                            Some(progress_tx),
                            Some(manager.clone()),
                        )
                        .await,
                        (SftpTransferDirection::Download, true) => scp_download_directory(
                            &resolved.handle,
                            &remote_path,
                            &local_path,
                            &transfer_id,
                            Some(progress_tx),
                            Some(manager.clone()),
                        )
                        .await,
                    }
                    .map_err(|error| error.to_string())?;
                    result.items
                } else {
                    match (direction, is_directory, resume_directory_strategy.clone()) {
                    (
                        SftpTransferDirection::Upload,
                        true,
                        Some(RemoteTransferStrategy::DirectoryTar),
                    ) => {
                        // Tauri node_sftp_resume_transfer honors the stored
                        // directory strategy. Do not re-probe auto mode during
                        // resume, otherwise a failed tar task can unexpectedly
                        // restart as tar again instead of its persisted strategy.
                        {
                            let shared = router
                                .acquire_sftp(&node_id)
                                .await
                                .map_err(|error| error.to_string())?;
                            let shared = shared.lock().await;
                            for prefix in remote_directory_prefixes(&remote_path) {
                                let _ = shared.mkdir(&prefix).await;
                            }
                        }
                        let (resolved, capabilities) = sftp_tar_capabilities_for_node(
                            &router, &manager, &node_id,
                        )
                        .await?;
                        tar_upload_directory(
                            &resolved.handle,
                            &local_path,
                            &remote_path,
                            &transfer_id,
                            Some(progress_tx),
                            Some(manager.clone()),
                            Some(capabilities.compression),
                        )
                        .await
                        .map_err(|error| error.to_string())?
                    }
                    (
                        SftpTransferDirection::Upload,
                        true,
                        Some(RemoteTransferStrategy::DirectoryRecursive),
                    ) => {
                        let sftp = router
                            .acquire_transfer_sftp(&node_id)
                            .await
                            .map_err(|error| error.to_string())?;
                        sftp.upload_dir(
                            &local_path,
                            &remote_path,
                            &transfer_id,
                            Some(progress_tx),
                            Some(manager.clone()),
                        )
                        .await
                        .map_err(|error| error.to_string())?
                    }
                    (SftpTransferDirection::Upload, true, _) => {
                        let (resolved, capabilities) = sftp_tar_capabilities_for_node(
                            &router, &manager, &node_id,
                        )
                        .await?;
                        if capabilities.supports_tar {
                            {
                                let shared = router
                                    .acquire_sftp(&node_id)
                                    .await
                                    .map_err(|error| error.to_string())?;
                                let shared = shared.lock().await;
                                for prefix in remote_directory_prefixes(&remote_path) {
                                    let _ = shared.mkdir(&prefix).await;
                                }
                            }
                            manager.update_background_transfer_strategy(
                                &transfer_id,
                                RemoteTransferStrategy::DirectoryTar,
                            );
                            let tar_result = tar_upload_directory(
                                &resolved.handle,
                                &local_path,
                                &remote_path,
                                &transfer_id,
                                Some(progress_tx.clone()),
                                Some(manager.clone()),
                                Some(capabilities.compression),
                            )
                            .await;
                            match tar_result {
                                Ok(count) => count,
                                Err(error)
                                    if !manager
                                        .get_control(&transfer_id)
                                        .is_some_and(|control| control.is_cancelled()) =>
                                {
                                    manager.update_background_transfer_strategy(
                                        &transfer_id,
                                        RemoteTransferStrategy::DirectoryRecursive,
                                    );
                                    let sftp = router
                                        .acquire_transfer_sftp(&node_id)
                                        .await
                                        .map_err(|error| error.to_string())?;
                                    sftp.upload_dir(
                                        &local_path,
                                        &remote_path,
                                        &transfer_id,
                                        Some(progress_tx),
                                        Some(manager.clone()),
                                    )
                                    .await
                                    .map_err(|fallback_error| {
                                        format!(
                                            "tar upload failed ({error}); recursive fallback failed ({fallback_error})"
                                        )
                                    })?
                                }
                                Err(error) => return Err(error.to_string()),
                            }
                        } else {
                            manager.update_background_transfer_strategy(
                                &transfer_id,
                                RemoteTransferStrategy::DirectoryRecursive,
                            );
                            let sftp = router
                                .acquire_transfer_sftp(&node_id)
                                .await
                                .map_err(|error| error.to_string())?;
                            sftp.upload_dir(
                                &local_path,
                                &remote_path,
                                &transfer_id,
                                Some(progress_tx),
                                Some(manager.clone()),
                            )
                            .await
                            .map_err(|error| error.to_string())?
                        }
                    }
                    (SftpTransferDirection::Upload, false, _) => {
                        let sftp = router
                            .acquire_transfer_sftp(&node_id)
                            .await
                            .map_err(|error| error.to_string())?;
                        sftp.upload_with_resume(
                            &local_path,
                            &remote_path,
                            progress_store.clone(),
                            Some(progress_tx),
                            Some(manager.clone()),
                            Some(transfer_id.clone()),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                        0
                    }
                    (
                        SftpTransferDirection::Download,
                        true,
                        Some(RemoteTransferStrategy::DirectoryTar),
                    ) => {
                        let (resolved, capabilities) = sftp_tar_capabilities_for_node(
                            &router, &manager, &node_id,
                        )
                        .await?;
                        tar_download_directory(
                            &resolved.handle,
                            &remote_path,
                            &local_path,
                            &transfer_id,
                            Some(progress_tx),
                            Some(manager.clone()),
                            Some(capabilities.compression),
                        )
                        .await
                        .map_err(|error| error.to_string())?
                    }
                    (
                        SftpTransferDirection::Download,
                        true,
                        Some(RemoteTransferStrategy::DirectoryRecursive),
                    ) => {
                        let sftp = router
                            .acquire_transfer_sftp(&node_id)
                            .await
                            .map_err(|error| error.to_string())?;
                        sftp.download_dir(
                            &remote_path,
                            &local_path,
                            &transfer_id,
                            Some(progress_tx),
                            Some(manager.clone()),
                        )
                        .await
                        .map_err(|error| error.to_string())?
                    }
                    (SftpTransferDirection::Download, true, _) => {
                        let (resolved, capabilities) = sftp_tar_capabilities_for_node(
                            &router, &manager, &node_id,
                        )
                        .await?;
                        if capabilities.supports_tar {
                            manager.update_background_transfer_strategy(
                                &transfer_id,
                                RemoteTransferStrategy::DirectoryTar,
                            );
                            let tar_result = tar_download_directory(
                                &resolved.handle,
                                &remote_path,
                                &local_path,
                                &transfer_id,
                                Some(progress_tx.clone()),
                                Some(manager.clone()),
                                Some(capabilities.compression),
                            )
                            .await;
                            match tar_result {
                                Ok(count) => count,
                                Err(error)
                                    if !manager
                                        .get_control(&transfer_id)
                                        .is_some_and(|control| control.is_cancelled()) =>
                                {
                                    manager.update_background_transfer_strategy(
                                        &transfer_id,
                                        RemoteTransferStrategy::DirectoryRecursive,
                                    );
                                    let sftp = router
                                        .acquire_transfer_sftp(&node_id)
                                        .await
                                        .map_err(|error| error.to_string())?;
                                    sftp.download_dir(
                                        &remote_path,
                                        &local_path,
                                        &transfer_id,
                                        Some(progress_tx),
                                        Some(manager.clone()),
                                    )
                                    .await
                                    .map_err(|fallback_error| {
                                        format!(
                                            "tar download failed ({error}); recursive fallback failed ({fallback_error})"
                                        )
                                    })?
                                }
                                Err(error) => return Err(error.to_string()),
                            }
                        } else {
                            manager.update_background_transfer_strategy(
                                &transfer_id,
                                RemoteTransferStrategy::DirectoryRecursive,
                            );
                            let sftp = router
                                .acquire_transfer_sftp(&node_id)
                                .await
                                .map_err(|error| error.to_string())?;
                            sftp.download_dir(
                                &remote_path,
                                &local_path,
                                &transfer_id,
                                Some(progress_tx),
                                Some(manager.clone()),
                            )
                            .await
                            .map_err(|error| error.to_string())?
                        }
                    }
                    (SftpTransferDirection::Download, false, _) => {
                        let sftp = router
                            .acquire_transfer_sftp(&node_id)
                            .await
                            .map_err(|error| error.to_string())?;
                        sftp.download_with_resume(
                            &remote_path,
                            &local_path,
                            progress_store.clone(),
                            Some(progress_tx),
                            Some(manager.clone()),
                            Some(transfer_id.clone()),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                        0
                    }
                    }
                };
                Ok::<u64, String>(item_count)
            }
            .await
            .map_err(|error| error);

            if is_directory || protocol == RemoteTransferProtocol::Scp {
                match &result {
                    Ok(item_count) => {
                        let _ = progress_store.delete(&transfer_id).await;
                        let _ = manager.finish_background_transfer(
                            &transfer_id,
                            BackgroundTransferState::Completed,
                            None,
                            Some(*item_count),
                        );
                    }
                    Err(error) if error.to_ascii_lowercase().contains("cancel") => {
                        let _ = progress_store.delete(&transfer_id).await;
                        let _ = manager.finish_background_transfer(
                            &transfer_id,
                            BackgroundTransferState::Cancelled,
                            None,
                            None,
                        );
                    }
                    Err(error) => {
                        if let Ok(Some(mut progress)) = progress_store.load(&transfer_id).await {
                            progress.mark_failed(error.clone());
                            let _ = progress_store.save(&progress).await;
                        }
                        let _ = manager.finish_background_transfer(
                            &transfer_id,
                            BackgroundTransferState::Error,
                            Some(error.clone()),
                            None,
                        );
                    }
                }
            }

            let _ = tx.send(SftpWorkerResult::TransferComplete {
                node_id: node_id.clone(),
                transfer_id,
                id,
                result: result.map(|_| ()),
                refresh_remote: matches!(direction, SftpTransferDirection::Upload),
                refresh_local: matches!(direction, SftpTransferDirection::Download),
            });
        });
    }

    pub(in crate::workspace::sftp) fn set_sftp_transfer_state(
        &mut self,
        id: u64,
        state: SftpTransferState,
    ) {
        let transfer_id = self
            .sftp_view
            .transfers
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.transfer_id.clone())
            .unwrap_or_else(|| id.to_string());
        match state {
            SftpTransferState::Paused => {
                self.sftp_transfer_manager.pause(&transfer_id);
                let progress_store = self.sftp_progress_store.clone();
                let transfer_id = transfer_id;
                self.forwarding_runtime.spawn(async move {
                    if let Ok(Some(mut progress)) = progress_store.load(&transfer_id).await {
                        progress.mark_paused();
                        let _ = progress_store.save(&progress).await;
                    }
                });
            }
            SftpTransferState::Pending | SftpTransferState::Active => {
                self.sftp_transfer_manager.resume(&transfer_id);
                let progress_store = self.sftp_progress_store.clone();
                let transfer_id = transfer_id;
                self.forwarding_runtime.spawn(async move {
                    if let Ok(Some(mut progress)) = progress_store.load(&transfer_id).await {
                        progress.mark_active();
                        let _ = progress_store.save(&progress).await;
                    }
                });
            }
            SftpTransferState::Cancelled => {
                self.sftp_transfer_manager.cancel(&transfer_id);
            }
            SftpTransferState::Completed | SftpTransferState::Error => {}
        }
        if let Some(item) = self
            .sftp_view
            .transfers
            .iter_mut()
            .find(|item| item.id == id)
        {
            item.state = state;
        }
    }

    pub(in crate::workspace::sftp) fn cancel_or_remove_sftp_transfer(&mut self, id: u64) {
        if let Some(index) = self
            .sftp_view
            .transfers
            .iter()
            .position(|item| item.id == id)
        {
            let active = matches!(
                self.sftp_view.transfers[index].state,
                SftpTransferState::Active | SftpTransferState::Pending | SftpTransferState::Paused
            );
            if active {
                let transfer_id = self.sftp_view.transfers[index].transfer_id.clone();
                self.sftp_transfer_manager.cancel(&transfer_id);
                self.sftp_view.transfers[index].state = SftpTransferState::Cancelled;
            } else {
                self.sftp_view.transfers.remove(index);
            }
        }
    }

    pub(in crate::workspace::sftp) fn upsert_sftp_background_transfer_snapshot(
        &mut self,
        snapshot: BackgroundTransferSnapshot,
    ) {
        let node_id = NodeId::new(snapshot.node_id.clone());
        let direction = match snapshot.direction {
            BackgroundTransferDirection::Upload => SftpTransferDirection::Upload,
            BackgroundTransferDirection::Download => SftpTransferDirection::Download,
        };
        let state = sftp_transfer_state_from_background(snapshot.state);
        let size = snapshot.size.max(1);
        if let Some(item) = self
            .sftp_view
            .transfers
            .iter_mut()
            .find(|item| item.transfer_id == snapshot.id)
        {
            item.node_id = node_id;
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

        let id = self.sftp_view.next_transfer_id;
        self.sftp_view.next_transfer_id += 1;
        self.sftp_view.transfers.push(SftpTransferItem {
            id,
            transfer_id: snapshot.id,
            batch_id: None,
            node_id,
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

    pub(in crate::workspace) fn interrupt_sftp_transfers_by_node(
        &mut self,
        node_id: &NodeId,
        error: String,
    ) -> bool {
        // Runtime ownership is authoritative because reconnect can resume a
        // transfer without materializing a row in the currently visible view.
        let transfer_ids_to_interrupt = self
            .sftp_transfer_manager
            .interrupt_node(&node_id.0, error.clone());
        let mut changed = !transfer_ids_to_interrupt.is_empty();
        for transfer in &mut self.sftp_view.transfers {
            if &transfer.node_id == node_id
                && matches!(
                    transfer.state,
                    SftpTransferState::Active
                        | SftpTransferState::Pending
                        | SftpTransferState::Paused
                )
            {
                transfer.state = SftpTransferState::Error;
                transfer.error = Some(error.clone());
                changed = true;
            }
        }
        for transfer_id in transfer_ids_to_interrupt {
            let progress_store = self.sftp_progress_store.clone();
            let transfer_id = transfer_id.clone();
            let error = error.clone();
            self.forwarding_runtime.spawn(async move {
                if let Ok(Some(mut progress)) = progress_store.load(&transfer_id).await {
                    progress.mark_failed(error);
                    let _ = progress_store.save(&progress).await;
                }
            });
        }
        changed
    }
}

async fn sftp_tar_capabilities_for_node(
    router: &NodeRouter,
    manager: &SftpTransferManager,
    node_id: &NodeId,
) -> Result<(oxideterm_ssh::ResolvedConnection, TarCapabilities), String> {
    let resolved = router
        .resolve_connection(node_id)
        .await
        .map_err(|error| error.to_string())?;
    let capabilities = manager
        .tar_capabilities(&resolved.connection_id, &resolved.handle)
        .await;
    Ok((resolved, capabilities))
}
