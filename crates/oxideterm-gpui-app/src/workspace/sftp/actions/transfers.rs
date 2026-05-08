impl WorkspaceApp {
    fn spawn_sftp_incomplete_load(&mut self, node_id: NodeId) {
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

    fn resume_sftp_incomplete_transfer(&mut self, transfer_id: String) {
        let Some(tab_id) = self.active_tab_id else {
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
            name: if is_directory { format!("{name}/") } else { name },
            local_path: local_path.clone(),
            remote_path: remote_path.clone(),
            direction,
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
        );
    }

    pub(super) fn request_sftp_transfer_resume_for_node(
        &self,
        node_id: NodeId,
        transfer_id: String,
    ) {
        let progress_store = self.sftp_progress_store.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = progress_store
                .load(&transfer_id)
                .await
                .map_err(|error| error.to_string())
                .and_then(|progress| {
                    progress.ok_or_else(|| {
                        "Transfer not found in progress store".to_string()
                    })
                });
            let _ = tx.send(SftpWorkerResult::ResumeIncompleteTransferLoaded {
                node_id,
                transfer_id,
                result,
            });
        });
    }

    fn queue_sftp_resume_transfer_for_node(
        &mut self,
        node_id: NodeId,
        progress: StoredTransferProgress,
    ) -> bool {
        if !progress.is_incomplete() {
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
                name: if is_directory { format!("{name}/") } else { name },
                local_path: local_path.clone(),
                remote_path: remote_path.clone(),
                direction,
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
        );
        true
    }

    fn spawn_sftp_transfer_task(
        &self,
        id: u64,
        transfer_id: String,
        node_id: NodeId,
        direction: SftpTransferDirection,
        is_directory: bool,
        local_path: String,
        remote_path: String,
        resume_progress: Option<StoredTransferProgress>,
    ) {
        let router = self.node_router.clone();
        let manager = self.sftp_transfer_manager.clone();
        let progress_store = self.sftp_progress_store.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let resume_directory_strategy = resume_progress
                .as_ref()
                .filter(|_| is_directory)
                .map(|progress| progress.strategy.clone());
            let resolved_connection_id = router
                .resolve_connection(&node_id)
                .map(|resolved| resolved.connection_id)
                .unwrap_or_else(|_| format!("node:{}", node_id.0));
            let mut directory_progress = is_directory.then(|| {
                if let Some(mut progress) = resume_progress.clone() {
                    progress.mark_active();
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
                progress.strategy = RemoteTransferStrategy::DirectoryRecursive;
                progress
            });
            if let Some(progress) = directory_progress.as_ref() {
                let _ = progress_store.save(progress).await;
            }
            if is_directory {
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
                let name = if name.ends_with('/') {
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
                manager.register_background_transfer(BackgroundTransferSnapshot::new(
                    transfer_id.clone(),
                    node_id.0.clone(),
                    name,
                    local_path.clone(),
                    remote_path.clone(),
                    background_direction,
                    BackgroundTransferKind::Directory,
                    strategy,
                    total,
                    transferred,
                ));
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
                while let Some(progress) = progress_rx.recv().await {
                    let progress = if is_directory {
                        accumulator.update(progress)
                    } else {
                        progress
                    };
                    if let Some(stored) = directory_progress.as_mut() {
                        stored.total_bytes = stored.total_bytes.max(progress.total_bytes);
                        stored.update_progress(progress.transferred_bytes);
                        let _ = progress_store_for_task.save(stored).await;
                    }
                    if is_directory {
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
                let _permit = manager.acquire_permit().await;
                if is_directory {
                    manager.mark_background_transfer_active(&transfer_id);
                }
                let item_count = match (direction, is_directory, resume_directory_strategy.clone())
                {
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
                        let compression = sftp_tar_compression_probe_for_node(&router, &node_id)
                            .await?;
                        let resolved = router
                            .resolve_connection(&node_id)
                            .map_err(|error| error.to_string())?;
                        tar_upload_directory(
                            &resolved.handle,
                            &local_path,
                            &remote_path,
                            &transfer_id,
                            Some(progress_tx),
                            Some(manager.clone()),
                            Some(compression),
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
                        if sftp_tar_probe_for_node(&router, &node_id).await? {
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
                            let compression =
                                sftp_tar_compression_probe_for_node(&router, &node_id).await?;
                            manager.update_background_transfer_strategy(
                                &transfer_id,
                                RemoteTransferStrategy::DirectoryTar,
                            );
                            let resolved = router
                                .resolve_connection(&node_id)
                                .map_err(|error| error.to_string())?;
                            let tar_result = tar_upload_directory(
                                &resolved.handle,
                                &local_path,
                                &remote_path,
                                &transfer_id,
                                Some(progress_tx.clone()),
                                Some(manager.clone()),
                                Some(compression),
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
                        let compression = sftp_tar_compression_probe_for_node(&router, &node_id)
                            .await?;
                        let resolved = router
                            .resolve_connection(&node_id)
                            .map_err(|error| error.to_string())?;
                        tar_download_directory(
                            &resolved.handle,
                            &remote_path,
                            &local_path,
                            &transfer_id,
                            Some(progress_tx),
                            Some(manager.clone()),
                            Some(compression),
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
                        if sftp_tar_probe_for_node(&router, &node_id).await? {
                            let compression =
                                sftp_tar_compression_probe_for_node(&router, &node_id).await?;
                            manager.update_background_transfer_strategy(
                                &transfer_id,
                                RemoteTransferStrategy::DirectoryTar,
                            );
                            let resolved = router
                                .resolve_connection(&node_id)
                                .map_err(|error| error.to_string())?;
                            let tar_result = tar_download_directory(
                                &resolved.handle,
                                &remote_path,
                                &local_path,
                                &transfer_id,
                                Some(progress_tx.clone()),
                                Some(manager.clone()),
                                Some(compression),
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
                };
                Ok::<u64, String>(item_count)
            }
            .await
            .map_err(|error| error.to_string());

            if is_directory {
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

    fn set_sftp_transfer_state(&mut self, id: u64, state: SftpTransferState) {
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
                let transfer_id = transfer_id.clone();
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
                let transfer_id = transfer_id.clone();
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

    fn cancel_or_remove_sftp_transfer(&mut self, id: u64) {
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
}

async fn sftp_tar_probe_for_node(router: &NodeRouter, node_id: &NodeId) -> Result<bool, String> {
    let resolved = router
        .resolve_connection(node_id)
        .map_err(|error| error.to_string())?;
    Ok(probe_tar_support(&resolved.handle).await)
}

async fn sftp_tar_compression_probe_for_node(
    router: &NodeRouter,
    node_id: &NodeId,
) -> Result<TarCompression, String> {
    let resolved = router
        .resolve_connection(node_id)
        .map_err(|error| error.to_string())?;
    Ok(probe_tar_compression(&resolved.handle).await)
}
