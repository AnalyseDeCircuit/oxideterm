//! SFTP Tauri commands
//!
//! Exposes SFTP functionality to the frontend.
//!
//! Note: SFTP opens its own SSH channel on the already-connected session handle.

use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tracing::info;

use crate::session::SessionRegistry;
use crate::sftp::{
    error::SftpError,
    session::{SftpRegistry, SftpSession},
    types::*,
};

/// Initialize SFTP for a session
#[tauri::command]
pub async fn sftp_init(
    session_id: String,
    session_registry: State<'_, Arc<SessionRegistry>>,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<String, SftpError> {
    info!("Initializing SFTP for session {}", session_id);

    // Check if already initialized
    if sftp_registry.has_sftp(&session_id) {
        if let Some(sftp) = sftp_registry.get(&session_id) {
            let sftp = sftp.lock().await;
            return Ok(sftp.cwd().to_string());
        }
    }

    // Get the HandleController to open a new channel
    let handle_controller = session_registry
        .get_handle_controller(&session_id)
        .ok_or_else(|| SftpError::SessionNotFound(session_id.clone()))?;

    // Create SFTP session using HandleController
    let sftp = SftpSession::new(handle_controller, session_id.clone()).await?;
    let cwd = sftp.cwd().to_string();

    // Register SFTP session
    sftp_registry.register(session_id.clone(), sftp);

    info!("SFTP initialized for session {}, cwd: {}", session_id, cwd);
    Ok(cwd)
}

/// List directory contents
#[tauri::command]
pub async fn sftp_list_dir(
    session_id: String,
    path: String,
    filter: Option<ListFilter>,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<Vec<FileInfo>, SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    let sftp = sftp.lock().await;
    sftp.list_dir(&path, filter).await
}

/// Get file/directory info
#[tauri::command]
pub async fn sftp_stat(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<FileInfo, SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    let sftp = sftp.lock().await;
    sftp.stat(&path).await
}

/// Preview file content
#[tauri::command]
pub async fn sftp_preview(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<PreviewContent, SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    let sftp = sftp.lock().await;
    sftp.preview(&path).await
}

/// Preview more hex data (incremental loading)
#[tauri::command]
pub async fn sftp_preview_hex(
    session_id: String,
    path: String,
    offset: u64,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<PreviewContent, SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    let sftp = sftp.lock().await;
    sftp.preview_with_offset(&path, offset).await
}

/// Download file
#[tauri::command]
pub async fn sftp_download(
    session_id: String,
    remote_path: String,
    local_path: String,
    transfer_id: Option<String>,
    app: AppHandle,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<(), SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    // Create progress channel
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);

    // Spawn progress event emitter
    let app_clone = app.clone();
    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", session_id_clone), &progress);
        }
    });

    let sftp = sftp.lock().await;

    // Use download_with_resume for full features (pause/cancel, retry)
    sftp.download_with_resume(
        &remote_path,
        &local_path,
        (*progress_store).clone(),
        Some(tx),
        Some((*transfer_manager).clone()),
        transfer_id,
    ).await?;

    Ok(())
}

/// Upload file
#[tauri::command]
pub async fn sftp_upload(
    session_id: String,
    local_path: String,
    remote_path: String,
    transfer_id: Option<String>,
    app: AppHandle,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<(), SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    // Create progress channel
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);

    // Spawn progress event emitter
    let app_clone = app.clone();
    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", session_id_clone), &progress);
        }
    });

    let sftp = sftp.lock().await;

    // Use upload_with_resume for full features (pause/cancel, retry, .oxide-part)
    sftp.upload_with_resume(
        &local_path,
        &remote_path,
        (*progress_store).clone(),
        Some(tx),
        Some((*transfer_manager).clone()),
        transfer_id,
    ).await?;

    Ok(())
}

/// Delete file or directory
#[tauri::command]
pub async fn sftp_delete(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<(), SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    let sftp = sftp.lock().await;
    sftp.delete(&path).await
}

/// Create directory
#[tauri::command]
pub async fn sftp_mkdir(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<(), SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    let sftp = sftp.lock().await;
    sftp.mkdir(&path).await
}

/// Rename/move file or directory
#[tauri::command]
pub async fn sftp_rename(
    session_id: String,
    old_path: String,
    new_path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<(), SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    let sftp = sftp.lock().await;
    sftp.rename(&old_path, &new_path).await
}

/// Delete file or directory recursively
#[tauri::command]
pub async fn sftp_delete_recursive(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<u64, SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    let sftp = sftp.lock().await;
    sftp.delete_recursive(&path).await
}

/// Download directory recursively
#[tauri::command]
pub async fn sftp_download_dir(
    session_id: String,
    remote_path: String,
    local_path: String,
    app: AppHandle,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<u64, SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    // Create progress channel
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);

    // Spawn progress event emitter
    let app_clone = app.clone();
    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", session_id_clone), &progress);
        }
    });

    let sftp = sftp.lock().await;
    sftp.download_dir(&remote_path, &local_path, Some(tx)).await
}

/// Upload directory recursively
#[tauri::command]
pub async fn sftp_upload_dir(
    session_id: String,
    local_path: String,
    remote_path: String,
    app: AppHandle,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<u64, SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    // Create progress channel
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);

    // Spawn progress event emitter
    let app_clone = app.clone();
    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", session_id_clone), &progress);
        }
    });

    let sftp = sftp.lock().await;
    sftp.upload_dir(&local_path, &remote_path, Some(tx)).await
}

/// Get current working directory
#[tauri::command]
pub async fn sftp_pwd(
    session_id: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<String, SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    let sftp = sftp.lock().await;
    Ok(sftp.cwd().to_string())
}

/// Change working directory
#[tauri::command]
pub async fn sftp_cd(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<String, SftpError> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    let mut sftp = sftp.lock().await;

    // Validate path exists and is a directory
    let info = sftp.stat(&path).await?;
    if info.file_type != FileType::Directory {
        return Err(SftpError::InvalidPath("Not a directory".to_string()));
    }

    sftp.set_cwd(info.path.clone());
    Ok(info.path)
}

/// Close SFTP session
#[tauri::command]
pub async fn sftp_close(
    session_id: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<(), SftpError> {
    info!("Closing SFTP for session {}", session_id);
    sftp_registry.remove(&session_id);
    Ok(())
}

/// Check if SFTP is initialized for a session
#[tauri::command]
pub async fn sftp_is_initialized(
    session_id: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<bool, SftpError> {
    Ok(sftp_registry.has_sftp(&session_id))
}

// ============ Transfer Control Commands ============

/// Cancel a specific transfer
#[tauri::command]
pub async fn sftp_cancel_transfer(
    transfer_id: String,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<bool, SftpError> {
    Ok(transfer_manager.cancel(&transfer_id))
}

/// Pause a specific transfer
#[tauri::command]
pub async fn sftp_pause_transfer(
    transfer_id: String,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<bool, SftpError> {
    Ok(transfer_manager.pause(&transfer_id))
}

/// Resume a specific transfer
#[tauri::command]
pub async fn sftp_resume_transfer(
    transfer_id: String,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<bool, SftpError> {
    Ok(transfer_manager.resume(&transfer_id))
}

/// Get transfer manager stats
#[tauri::command]
pub async fn sftp_transfer_stats(
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<(usize, usize), SftpError> {
    Ok((
        transfer_manager.active_count(),
        transfer_manager.max_concurrent(),
    ))
}

/// Update transfer settings (concurrent limit and speed limit)
#[tauri::command]
pub async fn sftp_update_settings(
    max_concurrent: Option<usize>,
    speed_limit_kbps: Option<usize>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<(), SftpError> {
    if let Some(max) = max_concurrent {
        transfer_manager.set_max_concurrent(max);
    }
    if let Some(kbps) = speed_limit_kbps {
        transfer_manager.set_speed_limit_kbps(kbps);
    }
    Ok(())
}

// ============ Resume Transfer Commands ============

/// List incomplete transfers for a session
#[tauri::command]
pub async fn sftp_list_incomplete_transfers(
    session_id: String,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
) -> Result<Vec<IncompleteTransferInfo>, SftpError> {
    use crate::sftp::progress::TransferType;
    use crate::sftp::progress::TransferStatus;

    let transfers = progress_store.list_incomplete(&session_id).await?;

    // Convert to frontend format
    let result: Vec<IncompleteTransferInfo> = transfers
        .into_iter()
        .map(|t| {
            let progress_percent = if t.total_bytes > 0 {
                (t.transferred_bytes as f64 / t.total_bytes as f64) * 100.0
            } else {
                0.0
            };

            // Can resume if paused or failed
            let can_resume = matches!(t.status, TransferStatus::Paused | TransferStatus::Failed);

            IncompleteTransferInfo {
                transfer_id: t.transfer_id,
                transfer_type: match t.transfer_type {
                    TransferType::Upload => "Upload",
                    TransferType::Download => "Download",
                },
                source_path: t.source_path.to_string_lossy().to_string(),
                destination_path: t.destination_path.to_string_lossy().to_string(),
                transferred_bytes: t.transferred_bytes,
                total_bytes: t.total_bytes,
                status: match t.status {
                    TransferStatus::Active => "Active",
                    TransferStatus::Paused => "Paused",
                    TransferStatus::Failed => "Failed",
                    TransferStatus::Completed => "Completed",
                    TransferStatus::Cancelled => "Cancelled",
                },
                session_id: t.session_id,
                error: t.error,
                progress_percent,
                can_resume,
            }
        })
        .collect();

    Ok(result)
}

/// Resume a specific transfer with retry support
#[tauri::command]
pub async fn sftp_resume_transfer_with_retry(
    session_id: String,
    transfer_id: String,
    app: AppHandle,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<(), SftpError> {
    use crate::sftp::progress::TransferType;

    // Load the stored progress
    let stored_progress = progress_store
        .load(&transfer_id)
        .await?
        .ok_or_else(|| SftpError::TransferError("Transfer not found in progress store".to_string()))?;

    // Get SFTP session
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    // Create progress channel
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);

    // Spawn progress event emitter
    let app_clone = app.clone();
    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", session_id_clone), &progress);
        }
    });

    let sftp = sftp.lock().await;

    // Get Arc clones
    let progress_store_arc = (*progress_store).clone();
    let transfer_manager_arc = (*transfer_manager).clone();

    // Call appropriate resume method based on transfer type
    match stored_progress.transfer_type {
        TransferType::Download => {
            sftp
                .download_with_resume(
                    &stored_progress.source_path.to_string_lossy(),
                    &stored_progress.destination_path.to_string_lossy(),
                    progress_store_arc,
                    Some(tx),
                    Some(transfer_manager_arc),
                    Some(transfer_id.clone()),
                )
                .await?;
        }
        TransferType::Upload => {
            sftp
                .upload_with_resume(
                    &stored_progress.source_path.to_string_lossy(),
                    &stored_progress.destination_path.to_string_lossy(),
                    progress_store_arc,
                    Some(tx),
                    Some(transfer_manager_arc),
                    Some(transfer_id.clone()),
                )
                .await?;
        }
    }

    Ok(())
}

/// Frontend type for incomplete transfer info
#[derive(serde::Serialize)]
pub struct IncompleteTransferInfo {
    transfer_id: String,
    transfer_type: &'static str,
    source_path: String,
    destination_path: String,
    transferred_bytes: u64,
    total_bytes: u64,
    status: &'static str,
    session_id: String,
    error: Option<String>,
    progress_percent: f64,
    can_resume: bool,
}

/// Result of a successful file write operation
#[derive(serde::Serialize)]
pub struct WriteResult {
    /// The new modification time of the file (Unix timestamp)
    pub mtime: Option<u64>,
    /// The new size of the file in bytes
    pub size: Option<u64>,
    /// The encoding used to write the file
    pub encoding_used: String,
}

/// Write text content to a remote file
///
/// This command is designed for the IDE mode editor.
/// It writes text content to a remote file via SFTP, with optional encoding support.
///
/// # Arguments
/// * `session_id` - The SSH session ID
/// * `path` - The remote file path to write to
/// * `content` - The UTF-8 text content to write
/// * `encoding` - Optional target encoding (defaults to "UTF-8")
///
/// # Returns
/// * `WriteResult` containing the new mtime (for sync confirmation) and file size
#[tauri::command]
pub async fn sftp_write_content(
    session_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<WriteResult, SftpError> {
    let target_encoding = encoding.as_deref().unwrap_or("UTF-8");
    info!("Writing content to {} for session {} (encoding: {})", path, session_id, target_encoding);

    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| SftpError::NotInitialized(session_id.clone()))?;

    let sftp = sftp.lock().await;

    // Encode content to target encoding
    let encoded_bytes = crate::sftp::types::encode_to_encoding(&content, target_encoding);

    // Write the encoded content to the file
    sftp.write_content(&path, &encoded_bytes).await?;

    // Get the new file metadata to confirm the write
    let file_info = sftp.stat(&path).await?;

    info!(
        "Successfully wrote {} bytes to {} (encoding: {}), modified: {}",
        file_info.size, path, target_encoding, file_info.modified
    );

    Ok(WriteResult {
        mtime: if file_info.modified > 0 { Some(file_info.modified as u64) } else { None },
        size: Some(file_info.size),
        encoding_used: target_encoding.to_string(),
    })
}
