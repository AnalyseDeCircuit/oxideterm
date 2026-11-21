use oxideterm_audit::AuditContext;
use std::sync::Arc;
use tokio::{runtime::Runtime, sync::Mutex};

use crate::{
    BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, SftpExecChannelOpener, SftpSession, SftpTransferGuard,
    SftpTransferManager, TarTransferOptions, TransferStrategy, profile_local_directory,
    remote_directory_prefixes, tar_download_directory, tar_upload_directory,
};

impl SftpTransferManager {
    /// Starts a transfer with an already acquired session. The caller verifies
    /// that the opener and session belong to the same connection generation.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_owned_directory_transfer(
        self: &Arc<Self>,
        runtime: &Runtime,
        owner_session: impl std::ops::Deref<Target = Mutex<SftpSession>> + Send + Sync + 'static,
        connection_handle: impl SftpExecChannelOpener,
        connection_id: &str,
        node_id: &str,
        direction_enum: BackgroundTransferDirection,
        local_path: &str,
        remote_path: &str,
        transfer_id: &str,
        audit: Option<&AuditContext>,
    ) -> BackgroundTransferSnapshot {
        let direction = match direction_enum {
            BackgroundTransferDirection::Upload => "upload",
            BackgroundTransferDirection::Download => "download",
        };
        let tar_capabilities = self
            .tar_capabilities(connection_id, &connection_handle)
            .await;
        let strategy = if tar_capabilities.supports_tar {
            TransferStrategy::DirectoryTar
        } else {
            TransferStrategy::DirectoryRecursive
        };
        let snapshot = BackgroundTransferSnapshot::new(
            transfer_id.to_string(),
            node_id.to_owned(),
            directory_transfer_name(local_path, remote_path),
            local_path.to_string(),
            remote_path.to_string(),
            direction_enum,
            BackgroundTransferKind::Directory,
            strategy.clone(),
            0,
            0,
        );
        self.register_background_transfer(snapshot.clone(), audit);

        let manager = self.clone();
        let transfer_id_for_task = transfer_id.to_string();
        let direction_for_task = direction.to_string();
        let local_path_for_task = local_path.to_string();
        let remote_path_for_task = remote_path.to_string();
        let strategy_for_task = strategy.clone();
        // The manager owns cancellation and progress independently of the requesting tool.
        runtime.spawn(async move {
            let result = async {
                let _permit = manager.acquire_permit().await;
                let control = manager.register(&transfer_id_for_task);
                let _guard = SftpTransferGuard::new(Some(&manager), transfer_id_for_task.clone());
                if control.is_cancelled() {
                    return Err("Transfer cancelled".to_string());
                }
                manager.mark_background_transfer_active(&transfer_id_for_task);
                manager.update_background_transfer_strategy(
                    &transfer_id_for_task,
                    strategy_for_task.clone(),
                );

                if strategy_for_task == TransferStrategy::DirectoryTar
                    && tar_capabilities.supports_tar
                {
                    let profile = match direction_for_task.as_str() {
                        "upload" => profile_local_directory(std::path::Path::new(
                            &local_path_for_task,
                        ))
                        .await
                        .ok(),
                        "download" => {
                            let sftp = owner_session.lock().await;
                            match sftp
                                .profile_remote_directory(
                                    &remote_path_for_task,
                                    &transfer_id_for_task,
                                    &Some(manager.clone()),
                                )
                                .await
                            {
                                Ok(profile) => Some(profile),
                                Err(error) if error.is_transfer_control() => {
                                    return Err(error.to_string());
                                }
                                Err(_) => None,
                            }
                        }
                        _ => unreachable!(),
                    };
                    manager
                        .check_control(&transfer_id_for_task)
                        .await
                        .map_err(|error| error.to_string())?;
                    if let Some(profile) = profile.filter(|profile| profile.prefers_tar()) {
                        if direction_for_task == "upload" {
                            let sftp = owner_session.lock().await;
                            for prefix in remote_directory_prefixes(&remote_path_for_task) {
                                let _ = sftp.mkdir(&prefix).await;
                            }
                        }
                        let compression =
                            profile.recommended_compression(tar_capabilities.compression);
                        let tar_result = match direction_for_task.as_str() {
                            "upload" => tar_upload_directory(
                                &connection_handle,
                                &local_path_for_task,
                                &remote_path_for_task,
                                &transfer_id_for_task,
                                None,
                                Some(manager.clone()),
                                TarTransferOptions {
                                    profile,
                                    compression,
                                },
                            )
                            .await,
                            "download" => tar_download_directory(
                                &connection_handle,
                                &remote_path_for_task,
                                &local_path_for_task,
                                &transfer_id_for_task,
                                None,
                                Some(manager.clone()),
                                TarTransferOptions {
                                    profile,
                                    compression,
                                },
                            )
                            .await,
                            _ => unreachable!(),
                        };
                        match tar_result {
                            Ok(result) => {
                                manager.record_background_transfer_stream_bytes(
                                    &transfer_id_for_task,
                                    result.stream_bytes,
                                );
                                return Ok((
                                    result.item_count,
                                    TransferStrategy::DirectoryTar,
                                    false,
                                ));
                            }
                            Err(error) if !error.is_transfer_control() =>
                            {
                                manager.update_background_transfer_strategy(
                                    &transfer_id_for_task,
                                    TransferStrategy::DirectoryRecursive,
                                );
                                let sftp = owner_session.lock().await;
                                let fallback = match direction_for_task.as_str() {
                                    "upload" => {
                                        sftp.upload_dir(
                                            &local_path_for_task,
                                            &remote_path_for_task,
                                            &transfer_id_for_task,
                                            None,
                                            Some(manager.clone()),
                                        )
                                        .await
                                    }
                                    "download" => {
                                        sftp.download_dir(
                                            &remote_path_for_task,
                                            &local_path_for_task,
                                            &transfer_id_for_task,
                                            None,
                                            Some(manager.clone()),
                                        )
                                        .await
                                    }
                                    _ => unreachable!(),
                                };
                                return fallback
                                    .map(|count| {
                                        (count, TransferStrategy::DirectoryRecursive, true)
                                    })
                                    .map_err(|fallback_error| {
                                        format!(
                                            "tar directory transfer failed ({error}); recursive fallback failed ({fallback_error})"
                                        )
                                    });
                            }
                            Err(error) => return Err(error.to_string()),
                        }
                    }
                }

                manager.update_background_transfer_strategy(
                    &transfer_id_for_task,
                    TransferStrategy::DirectoryRecursive,
                );
                let sftp = owner_session.lock().await;
                match direction_for_task.as_str() {
                    "upload" => {
                        sftp.upload_dir(
                            &local_path_for_task,
                            &remote_path_for_task,
                            &transfer_id_for_task,
                            None,
                            Some(manager.clone()),
                        )
                        .await
                    }
                    "download" => {
                        sftp.download_dir(
                            &remote_path_for_task,
                            &local_path_for_task,
                            &transfer_id_for_task,
                            None,
                            Some(manager.clone()),
                        )
                        .await
                    }
                    _ => unreachable!(),
                }
                .map(|count| (count, TransferStrategy::DirectoryRecursive, false))
                .map_err(|error| error.to_string())
            }
            .await;

            match result {
                Ok((item_count, _, _)) => {
                    manager.finish_background_transfer(
                        &transfer_id_for_task,
                        BackgroundTransferState::Completed,
                        None,
                        Some(item_count),
                    );
                }
                Err(error) => {
                    let state = if error.to_ascii_lowercase().contains("cancel") {
                        BackgroundTransferState::Cancelled
                    } else {
                        BackgroundTransferState::Error
                    };
                    manager.finish_background_transfer(
                        &transfer_id_for_task,
                        state,
                        Some(error),
                        None,
                    );
                }
            }
        });

        snapshot
    }
}

fn directory_transfer_name(local_path: &str, remote_path: &str) -> String {
    std::path::Path::new(local_path)
        .file_name()
        .or_else(|| std::path::Path::new(remote_path).file_name())
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Directory transfer".to_string())
}
