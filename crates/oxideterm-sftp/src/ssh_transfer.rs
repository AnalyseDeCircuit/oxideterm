// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::directory_progress::next_browser_progress;
use crate::{
    BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, BrowserTransferEvent, DirectoryProgressAccumulator,
    LocalDownloadDisposition, ProgressStore, SftpExecChannelOpener, SftpSession,
    SftpTransferManager, StoredTransferProgress, TarTransferOptions, TransferDirection,
    TransferProgress, TransferProtocol, TransferStrategy, TransferType, profile_local_directory,
    remote_directory_prefixes, scp_download_directory, scp_download_file, scp_upload_directory,
    scp_upload_file, tar_download_directory, tar_upload_directory,
};
use std::{path::Path, sync::Arc, time::Duration};

const DIRECTORY_PROGRESS_SAVE_INTERVAL: Duration = Duration::from_secs(1);

pub enum SshTransferPreference {
    Auto,
    Sftp,
    Scp,
}

pub enum SshTransferError {
    ScpUnavailable,
    ProtocolUnavailable,
    OpenChannel(String),
    Transfer(String),
}

pub struct SshTransferRequest {
    pub transfer_id: String,
    pub owner: String,
    pub direction: TransferDirection,
    pub is_directory: bool,
    pub local_path: String,
    pub remote_path: String,
    pub resume_progress: Option<StoredTransferProgress>,
    pub download_disposition: LocalDownloadDisposition,
    pub protocol_override: Option<TransferProtocol>,
    pub preference: SshTransferPreference,
    pub audit_context: Option<oxideterm_audit::AuditContext>,
}

async fn transfer_sftp<'a, F>(
    open_sftp: &impl Fn() -> F,
    prepared: &'a mut Option<SftpSession>,
) -> Result<&'a SftpSession, String>
where
    F: Future<Output = Result<SftpSession, String>>,
{
    if prepared.is_none() {
        *prepared = Some(open_sftp().await?);
    }
    Ok(prepared.as_ref().expect("transfer session was initialized"))
}

pub async fn run_ssh_transfer<O, F>(
    manager: Arc<SftpTransferManager>,
    progress_store: Arc<dyn ProgressStore>,
    resolved_handle: &O,
    resolved_connection_id: String,
    open_sftp: impl Fn() -> F,
    request: SshTransferRequest,
    emit: impl Fn(BrowserTransferEvent) + Clone + Send + Sync + 'static,
) -> Result<(), SshTransferError>
where
    O: SftpExecChannelOpener,
    F: Future<Output = Result<SftpSession, String>>,
{
    let SshTransferRequest {
        transfer_id,
        owner: remote_storage_key,
        direction,
        is_directory,
        local_path,
        remote_path,
        resume_progress,
        download_disposition,
        protocol_override,
        preference: protocol_preference,
        audit_context: transfer_audit,
    } = request;
    let mut prepared_sftp = None;
    let protocol = match resume_progress
        .as_ref()
        .map(|progress| progress.protocol)
        .or(protocol_override)
    {
        Some(protocol) => protocol,
        None => match protocol_preference {
            SshTransferPreference::Sftp => TransferProtocol::Sftp,
            SshTransferPreference::Scp => {
                let capabilities = manager
                    .scp_capabilities(&resolved_connection_id, resolved_handle)
                    .await;
                if !capabilities.supports_scp {
                    return Err(SshTransferError::ScpUnavailable);
                }
                TransferProtocol::Scp
            }
            SshTransferPreference::Auto => match open_sftp().await {
                Ok(session) => {
                    prepared_sftp = Some(session);
                    TransferProtocol::Sftp
                }
                Err(error) if error.contains("SFTP subsystem not available:") => {
                    let capabilities = manager
                        .scp_capabilities(&resolved_connection_id, resolved_handle)
                        .await;
                    if !capabilities.supports_scp {
                        return Err(SshTransferError::ProtocolUnavailable);
                    }
                    TransferProtocol::Scp
                }
                Err(error) => {
                    return Err(SshTransferError::OpenChannel(error));
                }
            },
        },
    };
    emit(BrowserTransferEvent::ProtocolResolved(protocol));
    let resume_directory_strategy = resume_progress
        .as_ref()
        .filter(|_| is_directory)
        .map(|progress| progress.strategy.clone());
    let mut directory_progress = (is_directory || protocol == TransferProtocol::Scp).then(|| {
        if let Some(mut progress) = resume_progress.clone() {
            progress.mark_active();
            if protocol == TransferProtocol::Scp {
                // Legacy SCP retries from byte zero after a channel or app restart.
                progress.transferred_bytes = 0;
            }
            // Reconnect creates a new connection generation. Move the
            // resumable record to the transport that will execute it.
            progress.session_id = resolved_connection_id.clone();
            return progress;
        }
        let transfer_type = match direction {
            TransferDirection::Upload => TransferType::Upload,
            TransferDirection::Download => TransferType::Download,
        };
        let mut progress = StoredTransferProgress::new(
            transfer_id.clone(),
            transfer_type,
            match direction {
                TransferDirection::Upload => local_path.clone().into(),
                TransferDirection::Download => remote_path.clone().into(),
            },
            match direction {
                TransferDirection::Upload => remote_path.clone().into(),
                TransferDirection::Download => local_path.clone().into(),
            },
            0,
            resolved_connection_id.clone(),
        );
        progress.protocol = protocol;
        progress.strategy = if is_directory {
            TransferStrategy::DirectoryRecursive
        } else {
            TransferStrategy::File
        };
        progress
    });
    if let Some(progress) = directory_progress.as_ref() {
        let _ = progress_store.save(progress).await;
    }
    if is_directory || protocol == TransferProtocol::Scp {
        let name_path = match direction {
            TransferDirection::Upload => &local_path,
            TransferDirection::Download => &remote_path,
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
                        TransferType::Upload => BackgroundTransferDirection::Upload,
                        TransferType::Download => BackgroundTransferDirection::Download,
                    },
                    progress.strategy.clone(),
                    progress.transferred_bytes,
                    progress.total_bytes,
                )
            } else {
                (
                    match direction {
                        TransferDirection::Upload => BackgroundTransferDirection::Upload,
                        TransferDirection::Download => BackgroundTransferDirection::Download,
                    },
                    TransferStrategy::DirectoryRecursive,
                    0,
                    0,
                )
            };
        let mut snapshot = BackgroundTransferSnapshot::new(
            transfer_id.clone(),
            remote_storage_key,
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
        manager.register_background_transfer(snapshot, transfer_audit.as_ref());
    }
    emit(BrowserTransferEvent::Progress {
        transferred: 0,
        total: 0,
        speed: 0,
    });
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let report_progress = emit.clone();
    let progress_store_for_task = progress_store.clone();
    let progress_manager = manager.clone();
    let progress_transfer_id = transfer_id.clone();
    let progress_task = tokio::spawn(async move {
        let mut accumulator = is_directory.then(DirectoryProgressAccumulator::default);
        let mut last_directory_progress_save = std::time::Instant::now();
        while let Some(progress) = next_browser_progress(&mut progress_rx, &mut accumulator).await {
            if let Some(stored) = directory_progress.as_mut() {
                stored.total_bytes = stored.total_bytes.max(progress.total_bytes);
                stored.update_progress(progress.transferred_bytes);
                if last_directory_progress_save.elapsed() >= DIRECTORY_PROGRESS_SAVE_INTERVAL {
                    // The transfer task records terminal directory states; this task only
                    // needs periodic snapshots for resume after process interruption.
                    let _ = progress_store_for_task.save(stored).await;
                    last_directory_progress_save = std::time::Instant::now();
                }
            }
            if is_directory || protocol == TransferProtocol::Scp {
                progress_manager.update_background_transfer_progress(
                    &progress_transfer_id,
                    progress.transferred_bytes,
                    progress.total_bytes,
                    progress.speed,
                );
            }
            report_progress(BrowserTransferEvent::Progress {
                transferred: progress.transferred_bytes,
                total: progress.total_bytes,
                speed: progress.speed,
            });
        }
    });

    let result = async {
        if is_directory || protocol == TransferProtocol::Scp {
            manager.mark_background_transfer_active(&transfer_id);
        }
        let item_count = if protocol == TransferProtocol::Scp {
            let result = match (direction, is_directory) {
                (TransferDirection::Upload, false) => scp_upload_file(
                    resolved_handle,
                    &local_path,
                    &remote_path,
                    &transfer_id,
                    Some(progress_tx),
                    Some(manager.clone()),
                )
                .await,
                (TransferDirection::Download, false) => scp_download_file(
                    resolved_handle,
                    &remote_path,
                    &local_path,
                    download_disposition,
                    &transfer_id,
                    Some(progress_tx),
                    Some(manager.clone()),
                )
                .await,
                (TransferDirection::Upload, true) => scp_upload_directory(
                    resolved_handle,
                    &local_path,
                    &remote_path,
                    &transfer_id,
                    Some(progress_tx),
                    Some(manager.clone()),
                )
                .await,
                (TransferDirection::Download, true) => scp_download_directory(
                    resolved_handle,
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
                TransferDirection::Upload,
                true,
                Some(TransferStrategy::DirectoryTar),
            ) => {
                // Tauri node_sftp_resume_transfer honors the stored
                // directory strategy. Do not re-probe auto mode during
                // resume, otherwise a failed tar task can unexpectedly
                // restart as tar again instead of its persisted strategy.
                {
                    let shared = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
                    for prefix in remote_directory_prefixes(&remote_path) {
                        let _ = shared.mkdir(&prefix).await;
                    }
                }
                let capabilities = manager.tar_capabilities(&resolved_connection_id, resolved_handle)
                .await;
                if capabilities.supports_tar {
                    let profile = profile_local_directory(Path::new(&local_path))
                        .await
                        .map_err(|error| error.to_string())?;
                    let compression =
                        profile.recommended_compression(capabilities.compression);
                    let result = tar_upload_directory(
                        resolved_handle,
                        &local_path,
                        &remote_path,
                        &transfer_id,
                        Some(progress_tx),
                        Some(manager.clone()),
                        TarTransferOptions {
                            profile,
                            compression,
                        },
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    manager.record_background_transfer_stream_bytes(&transfer_id, result.stream_bytes);
                    result.item_count
                } else {
                    manager.update_background_transfer_strategy(
                        &transfer_id,
                        TransferStrategy::DirectoryRecursive,
                    );
                    let sftp = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
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
            (
                TransferDirection::Upload,
                true,
                Some(TransferStrategy::DirectoryRecursive),
            ) => {
                let sftp = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
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
            (TransferDirection::Upload, true, _) => {
                let capabilities = manager.tar_capabilities(&resolved_connection_id, resolved_handle)
                .await;
                let profile = if capabilities.supports_tar {
                    profile_local_directory(Path::new(&local_path)).await.ok()
                } else {
                    None
                };
                manager
                    .check_control(&transfer_id)
                    .await
                    .map_err(|error| error.to_string())?;
                if let Some(profile) = profile.filter(|profile| profile.prefers_tar()) {
                    {
                        let shared = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
                        for prefix in remote_directory_prefixes(&remote_path) {
                            let _ = shared.mkdir(&prefix).await;
                        }
                    }
                    manager.update_background_transfer_strategy(
                        &transfer_id,
                        TransferStrategy::DirectoryTar,
                    );
                    let compression =
                        profile.recommended_compression(capabilities.compression);
                    let tar_result = tar_upload_directory(
                        resolved_handle,
                        &local_path,
                        &remote_path,
                        &transfer_id,
                        Some(progress_tx.clone()),
                        Some(manager.clone()),
                        TarTransferOptions {
                            profile,
                            compression,
                        },
                    )
                    .await;
                    match tar_result {
                        Ok(result) => {
                            manager.record_background_transfer_stream_bytes(&transfer_id, result.stream_bytes);
                            result.item_count
                        }
                        Err(error) if !error.is_transfer_control() =>
                        {
                            manager.update_background_transfer_strategy(
                                &transfer_id,
                                TransferStrategy::DirectoryRecursive,
                            );
                            let sftp = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
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
                        TransferStrategy::DirectoryRecursive,
                    );
                    let sftp = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
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
            (TransferDirection::Upload, false, _) => {
                let sftp = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
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
                TransferDirection::Download,
                true,
                Some(TransferStrategy::DirectoryTar),
            ) => {
                let capabilities = manager.tar_capabilities(&resolved_connection_id, resolved_handle)
                .await;
                if capabilities.supports_tar {
                    let profile = {
                        let shared = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
                        shared
                            .profile_remote_directory(
                                &remote_path,
                                &transfer_id,
                                &Some(manager.clone()),
                            )
                            .await
                            .map_err(|error| error.to_string())?
                    };
                    let compression =
                        profile.recommended_compression(capabilities.compression);
                    let result = tar_download_directory(
                        resolved_handle,
                        &remote_path,
                        &local_path,
                        &transfer_id,
                        Some(progress_tx),
                        Some(manager.clone()),
                        TarTransferOptions {
                            profile,
                            compression,
                        },
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    manager.record_background_transfer_stream_bytes(&transfer_id, result.stream_bytes);
                    result.item_count
                } else {
                    manager.update_background_transfer_strategy(
                        &transfer_id,
                        TransferStrategy::DirectoryRecursive,
                    );
                    let sftp = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
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
            (
                TransferDirection::Download,
                true,
                Some(TransferStrategy::DirectoryRecursive),
            ) => {
                let sftp = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
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
            (TransferDirection::Download, true, _) => {
                let capabilities = manager.tar_capabilities(&resolved_connection_id, resolved_handle)
                .await;
                let profile = if capabilities.supports_tar {
                    let shared = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
                    match shared
                        .profile_remote_directory(
                            &remote_path,
                            &transfer_id,
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
                } else {
                    None
                };
                if let Some(profile) = profile.filter(|profile| profile.prefers_tar()) {
                    manager.update_background_transfer_strategy(
                        &transfer_id,
                        TransferStrategy::DirectoryTar,
                    );
                    let compression =
                        profile.recommended_compression(capabilities.compression);
                    let tar_result = tar_download_directory(
                        resolved_handle,
                        &remote_path,
                        &local_path,
                        &transfer_id,
                        Some(progress_tx.clone()),
                        Some(manager.clone()),
                        TarTransferOptions {
                            profile,
                            compression,
                        },
                    )
                    .await;
                    match tar_result {
                        Ok(result) => {
                            manager.record_background_transfer_stream_bytes(&transfer_id, result.stream_bytes);
                            result.item_count
                        }
                        Err(error) if !error.is_transfer_control() =>
                        {
                            manager.update_background_transfer_strategy(
                                &transfer_id,
                                TransferStrategy::DirectoryRecursive,
                            );
                            let sftp = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
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
                        TransferStrategy::DirectoryRecursive,
                    );
                    let sftp = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
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
            (TransferDirection::Download, false, _) => {
                let sftp = transfer_sftp(&open_sftp, &mut prepared_sftp).await?;
                sftp.download_with_resume(
                    &remote_path,
                    &local_path,
                    download_disposition,
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
    .await;

    drop(prepared_sftp);
    // Drain the producer's final progress before freezing the task result.
    let _ = progress_task.await;

    if is_directory || protocol == TransferProtocol::Scp {
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
    result.map(|_| ()).map_err(SshTransferError::Transfer)
}
