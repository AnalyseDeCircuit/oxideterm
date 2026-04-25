// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Node-first SFTP commands — Phase 0 of Oxide-Next
//!
//! 所有命令接受 nodeId 而非 sessionId。
//! 内部通过 NodeRouter 解析到具体资源。
//!
//! 参考: docs/reference/OXIDE_NEXT_ARCHITECTURE.md §3.2

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tracing::info;

use crate::router::{NodeRouter, NodeStateSnapshot, RouteError, TerminalEndpoint};
use crate::sftp::error::SftpError;
use crate::sftp::progress::{
    ProgressStore, StoredTransferProgress, TransferStatus, TransferStrategy, TransferType,
};
use crate::sftp::types::*;
use crate::ssh::SshConnectionRegistry;

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
struct TransferCompleteEvent {
    transfer_id: String,
    node_id: String,
    success: bool,
    error: Option<String>,
}

fn transfer_complete_event_name(node_id: &str) -> String {
    format!("sftp:complete:{}", node_id)
}

fn build_transfer_complete_event(
    node_id: &str,
    transfer_id: &str,
    success: bool,
    error: Option<String>,
) -> TransferCompleteEvent {
    TransferCompleteEvent {
        transfer_id: transfer_id.to_string(),
        node_id: node_id.to_string(),
        success,
        error,
    }
}

fn emit_transfer_complete(
    app: &AppHandle,
    node_id: &str,
    transfer_id: &str,
    success: bool,
    error: Option<String>,
) {
    let _ = app.emit(
        &transfer_complete_event_name(node_id),
        &build_transfer_complete_event(node_id, transfer_id, success, error),
    );
}

fn map_incomplete_transfer_info(
    transfer: StoredTransferProgress,
) -> crate::commands::sftp::IncompleteTransferInfo {
    let is_directory = transfer.is_directory();
    let progress_percent = if transfer.total_bytes > 0 {
        (transfer.transferred_bytes as f64 / transfer.total_bytes as f64) * 100.0
    } else {
        0.0
    };
    let can_resume = matches!(
        transfer.status,
        TransferStatus::Paused | TransferStatus::Failed
    );

    crate::commands::sftp::IncompleteTransferInfo {
        transfer_id: transfer.transfer_id,
        transfer_type: match transfer.transfer_type {
            TransferType::Upload => "Upload",
            TransferType::Download => "Download",
        },
        is_directory,
        source_path: transfer.source_path.to_string_lossy().to_string(),
        destination_path: transfer.destination_path.to_string_lossy().to_string(),
        transferred_bytes: transfer.transferred_bytes,
        total_bytes: transfer.total_bytes,
        status: match transfer.status {
            TransferStatus::Active => "Active",
            TransferStatus::Paused => "Paused",
            TransferStatus::Failed => "Failed",
            TransferStatus::Completed => "Completed",
            TransferStatus::Cancelled => "Cancelled",
        },
        session_id: transfer.session_id,
        error: transfer.error,
        progress_percent,
        can_resume,
    }
}

async fn load_incomplete_transfer_infos(
    progress_store: &dyn ProgressStore,
    connection_id: &str,
) -> Result<Vec<crate::commands::sftp::IncompleteTransferInfo>, RouteError> {
    let transfers = progress_store
        .list_incomplete(connection_id)
        .await
        .map_err(RouteError::from)?;

    Ok(transfers
        .into_iter()
        .map(map_incomplete_transfer_info)
        .collect())
}

fn build_directory_transfer_progress(
    transfer_id: &str,
    transfer_type: TransferType,
    source_path: &str,
    destination_path: &str,
    total_bytes: u64,
    session_id: &str,
    strategy: TransferStrategy,
) -> StoredTransferProgress {
    let mut progress = StoredTransferProgress::new(
        transfer_id.to_string(),
        transfer_type,
        source_path.into(),
        destination_path.into(),
        total_bytes,
        session_id.to_string(),
    );
    progress.strategy = strategy;
    progress
}

fn update_directory_transfer_progress(
    progress: &mut StoredTransferProgress,
    event: &TransferProgress,
) {
    if event.total_bytes > 0 {
        progress.total_bytes = progress.total_bytes.max(event.total_bytes);
    }

    if progress.total_bytes > 0 {
        progress.transferred_bytes = event.transferred_bytes.min(progress.total_bytes);
    } else {
        progress.transferred_bytes = event.transferred_bytes;
    }
}

fn create_cancel_flag_bridge(
    control: std::sync::Arc<crate::sftp::TransferControl>,
) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag_clone = cancel_flag.clone();
    let mut cancel_rx = control.subscribe_cancellation();
    tokio::spawn(async move {
        while cancel_rx.changed().await.is_ok() {
            if *cancel_rx.borrow() {
                flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                break;
            }
        }
    });
    cancel_flag
}

fn spawn_directory_progress_forwarder(
    app: AppHandle,
    node_id: String,
    mut rx: tokio::sync::mpsc::Receiver<TransferProgress>,
    progress_store: Arc<dyn ProgressStore>,
    mut stored_progress: StoredTransferProgress,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app.emit(&format!("sftp:progress:{}", node_id), &progress);
            update_directory_transfer_progress(&mut stored_progress, &progress);
            let _ = progress_store.save(&stored_progress).await;
        }
    })
}

async fn finalize_directory_transfer_progress(
    progress_store: &dyn ProgressStore,
    seed: &StoredTransferProgress,
    result: &Result<u64, SftpError>,
) {
    match result {
        Ok(_) | Err(SftpError::TransferCancelled) => {
            let _ = progress_store.delete(&seed.transfer_id).await;
        }
        Err(error) => {
            let mut progress = progress_store
                .load(&seed.transfer_id)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| seed.clone());
            progress.mark_failed(error.to_string());
            let _ = progress_store.save(&progress).await;
        }
    }
}

async fn resume_directory_download(
    router: &NodeRouter,
    node_id: &str,
    transfer_id: &str,
    stored_progress: &StoredTransferProgress,
    app: &AppHandle,
    progress_store: Arc<dyn ProgressStore>,
    transfer_manager: Arc<crate::sftp::TransferManager>,
) -> Result<(), RouteError> {
    let resolved = router.resolve_connection(node_id).await?;
    let seed = build_directory_transfer_progress(
        transfer_id,
        TransferType::Download,
        &stored_progress.source_path.to_string_lossy(),
        &stored_progress.destination_path.to_string_lossy(),
        stored_progress.total_bytes,
        &resolved.connection_id,
        stored_progress.strategy.clone(),
    );
    progress_store.save(&seed).await.map_err(RouteError::from)?;

    let (tx, rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let progress_task = spawn_directory_progress_forwarder(
        app.clone(),
        node_id.to_string(),
        rx,
        progress_store.clone(),
        seed.clone(),
    );

    let control = transfer_manager.register(transfer_id);
    let cancel_flag = create_cancel_flag_bridge(control);

    let result = match stored_progress.strategy {
        TransferStrategy::DirectoryTar => {
            crate::sftp::tar_transfer::tar_download_directory(
                &resolved.handle_controller,
                &stored_progress.source_path.to_string_lossy(),
                &stored_progress.destination_path.to_string_lossy(),
                transfer_id,
                Some(tx),
                Some(cancel_flag),
                Some(
                    crate::sftp::tar_transfer::probe_tar_compression(&resolved.handle_controller)
                        .await,
                ),
                Some(transfer_manager.speed_limit_bps_ref()),
            )
            .await
        }
        TransferStrategy::DirectoryRecursive => {
            let sftp = router.acquire_transfer_sftp(node_id).await?;
            sftp.download_dir(
                &stored_progress.source_path.to_string_lossy(),
                &stored_progress.destination_path.to_string_lossy(),
                transfer_id,
                Some(tx),
                Some(cancel_flag),
                Some(transfer_manager.speed_limit_bps_ref()),
                transfer_manager.directory_parallelism(),
            )
            .await
        }
        TransferStrategy::File => unreachable!(),
    };

    transfer_manager.unregister(transfer_id);
    let _ = progress_task.await;
    finalize_directory_transfer_progress(progress_store.as_ref(), &seed, &result).await;

    result.map(|_| ()).map_err(RouteError::from)
}

async fn resume_directory_upload(
    router: &NodeRouter,
    node_id: &str,
    transfer_id: &str,
    stored_progress: &StoredTransferProgress,
    app: &AppHandle,
    progress_store: Arc<dyn ProgressStore>,
    transfer_manager: Arc<crate::sftp::TransferManager>,
) -> Result<(), RouteError> {
    let resolved = router.resolve_connection(node_id).await?;
    let seed = build_directory_transfer_progress(
        transfer_id,
        TransferType::Upload,
        &stored_progress.source_path.to_string_lossy(),
        &stored_progress.destination_path.to_string_lossy(),
        stored_progress.total_bytes,
        &resolved.connection_id,
        stored_progress.strategy.clone(),
    );
    progress_store.save(&seed).await.map_err(RouteError::from)?;

    let (tx, rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let progress_task = spawn_directory_progress_forwarder(
        app.clone(),
        node_id.to_string(),
        rx,
        progress_store.clone(),
        seed.clone(),
    );

    let control = transfer_manager.register(transfer_id);
    let cancel_flag = create_cancel_flag_bridge(control);

    let result = match stored_progress.strategy {
        TransferStrategy::DirectoryTar => {
            let sftp = router.acquire_sftp(node_id).await?;
            {
                let sftp = sftp.lock().await;
                let destination_path = stored_progress
                    .destination_path
                    .to_string_lossy()
                    .to_string();
                let components: Vec<&str> = destination_path
                    .split('/')
                    .filter(|s| !s.is_empty())
                    .collect();
                for i in 0..components.len() {
                    let prefix = format!("/{}", components[..=i].join("/"));
                    let _ = sftp.mkdir(&prefix).await;
                }
            }
            crate::sftp::tar_transfer::tar_upload_directory(
                &resolved.handle_controller,
                &stored_progress.source_path.to_string_lossy(),
                &stored_progress.destination_path.to_string_lossy(),
                transfer_id,
                Some(tx),
                Some(cancel_flag),
                Some(
                    crate::sftp::tar_transfer::probe_tar_compression(&resolved.handle_controller)
                        .await,
                ),
                Some(transfer_manager.speed_limit_bps_ref()),
            )
            .await
        }
        TransferStrategy::DirectoryRecursive => {
            let sftp = router.acquire_transfer_sftp(node_id).await?;
            sftp.upload_dir(
                &stored_progress.source_path.to_string_lossy(),
                &stored_progress.destination_path.to_string_lossy(),
                transfer_id,
                Some(tx),
                Some(cancel_flag),
                Some(transfer_manager.speed_limit_bps_ref()),
                transfer_manager.directory_parallelism(),
            )
            .await
        }
        TransferStrategy::File => unreachable!(),
    };

    transfer_manager.unregister(transfer_id);
    let _ = progress_task.await;
    finalize_directory_transfer_progress(progress_store.as_ref(), &seed, &result).await;

    result.map(|_| ()).map_err(RouteError::from)
}

// ============================================================================
// SFTP 静默重建辅助宏
// ============================================================================

/// SFTP 操作重试宏（用于只读操作）
///
/// 捕获通道级别可恢复错误，尝试重建 SFTP session 后重试一次。
/// 适用于只读操作（list_dir, stat, preview 等），不建议用于写入操作。
///
/// # 使用方式
/// ```rust,ignore
/// sftp_with_retry!(router, &node_id, sftp, {
///     sftp.list_dir(&path, filter.clone()).await
/// })
/// ```
///
/// # 参数
/// - `$router`: NodeRouter 实例
/// - `$node_id`: 节点 ID 引用
/// - `$sftp`: SFTP session 绑定名
/// - `$op`: 返回 `Result<T, SftpError>` 的异步操作块
macro_rules! sftp_with_retry {
    ($router:expr, $node_id:expr, $sftp:ident, $op:block) => {{
        // 首次尝试
        let sftp_arc = $router.acquire_sftp($node_id).await?;
        let $sftp = sftp_arc.lock().await;
        let first_result: Result<_, SftpError> = $op;
        drop($sftp); // 释放锁以便重建

        match first_result {
            Ok(v) => Ok(v),
            Err(e) if e.is_channel_recoverable() => {
                // 通道错误，尝试重建
                tracing::info!(
                    "SFTP channel error for node {}, attempting rebuild: {}",
                    $node_id,
                    e
                );

                // 重建 SFTP session
                let sftp_arc = $router.invalidate_and_reacquire_sftp($node_id).await?;
                let $sftp = sftp_arc.lock().await;

                // 重试操作
                let retry_result: Result<_, SftpError> = $op;
                retry_result.map_err(RouteError::from)
            }
            Err(e) => {
                // 业务错误，直接返回
                Err(RouteError::from(e))
            }
        }
    }};
}

// ============================================================================
// Node State 查询
// ============================================================================

/// 获取节点状态快照（含 generation，用于前端初始对齐）
#[tauri::command]
pub async fn node_get_state(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<NodeStateSnapshot, RouteError> {
    router.get_node_state(&node_id).await
}

/// 初始化节点的 SFTP，返回 cwd
#[tauri::command]
pub async fn node_sftp_init(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<String, RouteError> {
    info!("node_sftp_init: nodeId={}", node_id);

    let sftp = router.acquire_sftp(&node_id).await?;
    let sftp = sftp.lock().await;
    Ok(sftp.cwd().to_string())
}

/// 列目录（支持静默重建）
#[tauri::command]
pub async fn node_sftp_list_dir(
    node_id: String,
    path: String,
    filter: Option<ListFilter>,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<Vec<FileInfo>, RouteError> {
    let filter = filter.clone();
    sftp_with_retry!(router, &node_id, sftp, {
        sftp.list_dir(&path, filter.clone()).await
    })
}

/// 文件信息（支持静默重建）
#[tauri::command]
pub async fn node_sftp_stat(
    node_id: String,
    path: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<FileInfo, RouteError> {
    sftp_with_retry!(router, &node_id, sftp, { sftp.stat(&path).await })
}

/// 预览文件内容（支持静默重建）
///
/// When the backend decides to stream a file via `asset://` (video, audio,
/// large images, PDF, Office), the result will be `AssetFile { path, … }`.
/// This command automatically allows the temp file on the asset protocol
/// scope so the WebView can stream it directly from disk.
#[tauri::command]
pub async fn node_sftp_preview(
    node_id: String,
    path: String,
    app: AppHandle,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<PreviewContent, RouteError> {
    let result: Result<PreviewContent, RouteError> =
        sftp_with_retry!(router, &node_id, sftp, { sftp.preview(&path).await });

    // If the preview produced a temp file, allow it on the asset scope
    if let Ok(PreviewContent::AssetFile { ref path, .. }) = result {
        use tauri::Manager;
        let _ = app
            .asset_protocol_scope()
            .allow_file(std::path::Path::new(path));
    }
    result
}

/// Clean up a temp file created by SFTP preview.
/// If `path` is provided, deletes that single file (must be inside the
/// `oxideterm-sftp-preview` temp dir). Otherwise, removes the entire temp dir.
#[tauri::command]
pub async fn cleanup_sftp_preview_temp(path: Option<String>) -> Result<(), String> {
    let temp_dir = std::env::temp_dir().join("oxideterm-sftp-preview");

    if let Some(p) = path {
        let target = std::path::Path::new(&p);
        // Safety: only allow deleting files inside our temp dir
        if let (Ok(canonical), Ok(canonical_dir)) = (target.canonicalize(), temp_dir.canonicalize())
        {
            if canonical.starts_with(&canonical_dir) {
                let _ = tokio::fs::remove_file(&canonical).await;
            }
        }
    } else {
        // Wipe the entire preview temp directory
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
    Ok(())
}

/// 写入文件内容（IDE 编辑器用）
#[tauri::command]
pub async fn node_sftp_write(
    node_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<NodeWriteResult, RouteError> {
    let target_encoding = encoding.as_deref().unwrap_or("UTF-8");
    info!(
        "node_sftp_write: nodeId={}, path={}, encoding={}",
        node_id, path, target_encoding
    );

    let sftp = router.acquire_sftp(&node_id).await?;
    let sftp = sftp.lock().await;

    // 编码转换
    let encoded_bytes = crate::sftp::types::encode_to_encoding(&content, target_encoding);

    // 写入
    let write_result = sftp
        .write_content(&path, &encoded_bytes)
        .await
        .map_err(RouteError::from)?;

    // 获取写入后的元数据
    let file_info = sftp.stat(&path).await.map_err(RouteError::from)?;

    info!(
        "node_sftp_write: wrote {} bytes to {} (encoding: {}, atomic: {})",
        file_info.size, path, target_encoding, write_result.atomic_write
    );

    Ok(NodeWriteResult {
        mtime: if file_info.modified > 0 {
            Some(file_info.modified as u64)
        } else {
            None
        },
        size: Some(file_info.size),
        encoding_used: target_encoding.to_string(),
        atomic_write: write_result.atomic_write,
    })
}

/// 下载文件
#[tauri::command]
pub async fn node_sftp_download(
    node_id: String,
    remote_path: String,
    local_path: String,
    transfer_id: Option<String>,
    app: AppHandle,
    router: State<'_, Arc<NodeRouter>>,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<(), RouteError> {
    let tid = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    // Gate concurrency: acquire permit BEFORE opening SSH channel
    // to prevent MaxSessions exhaustion under parallel transfers.
    let _permit = transfer_manager.acquire_permit().await;

    // 使用独立的传输 SFTP session，不阻塞浏览操作
    let sftp = router.acquire_transfer_sftp(&node_id).await?;

    // 进度通道
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);

    // 进度事件推送（使用 node_id 前缀）
    let app_clone = app.clone();
    let node_id_clone = node_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", node_id_clone), &progress);
        }
    });

    let result = sftp
        .download_with_resume(
            &remote_path,
            &local_path,
            (*progress_store).clone(),
            Some(tx),
            Some((*transfer_manager).clone()),
            Some(tid.clone()),
        )
        .await;

    match &result {
        Ok(_) => emit_transfer_complete(&app, &node_id, &tid, true, None),
        Err(err) => emit_transfer_complete(&app, &node_id, &tid, false, Some(err.to_string())),
    }

    result.map(|_| ()).map_err(RouteError::from)
}

/// 上传文件
#[tauri::command]
pub async fn node_sftp_upload(
    node_id: String,
    local_path: String,
    remote_path: String,
    transfer_id: Option<String>,
    app: AppHandle,
    router: State<'_, Arc<NodeRouter>>,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<(), RouteError> {
    let tid = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    // Gate concurrency: acquire permit BEFORE opening SSH channel
    let _permit = transfer_manager.acquire_permit().await;

    // 使用独立的传输 SFTP session，不阻塞浏览操作
    let sftp = router.acquire_transfer_sftp(&node_id).await?;

    // 进度通道
    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);

    let app_clone = app.clone();
    let node_id_clone = node_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", node_id_clone), &progress);
        }
    });

    let result = sftp
        .upload_with_resume(
            &local_path,
            &remote_path,
            (*progress_store).clone(),
            Some(tx),
            Some((*transfer_manager).clone()),
            Some(tid.clone()),
        )
        .await;

    match &result {
        Ok(_) => emit_transfer_complete(&app, &node_id, &tid, true, None),
        Err(err) => emit_transfer_complete(&app, &node_id, &tid, false, Some(err.to_string())),
    }

    result.map(|_| ()).map_err(RouteError::from)
}

/// 删除文件或目录
#[tauri::command]
pub async fn node_sftp_delete(
    node_id: String,
    path: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<(), RouteError> {
    let sftp = router.acquire_sftp(&node_id).await?;
    let sftp = sftp.lock().await;
    sftp.delete(&path).await.map_err(RouteError::from)
}

/// 创建目录
#[tauri::command]
pub async fn node_sftp_mkdir(
    node_id: String,
    path: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<(), RouteError> {
    let sftp = router.acquire_sftp(&node_id).await?;
    let sftp = sftp.lock().await;
    sftp.mkdir(&path).await.map_err(RouteError::from)
}

/// 重命名/移动文件
#[tauri::command]
pub async fn node_sftp_rename(
    node_id: String,
    old_path: String,
    new_path: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<(), RouteError> {
    let sftp = router.acquire_sftp(&node_id).await?;
    let sftp = sftp.lock().await;
    sftp.rename(&old_path, &new_path)
        .await
        .map_err(RouteError::from)
}

/// 获取终端 WebSocket URL
#[tauri::command]
pub async fn node_terminal_url(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<TerminalEndpoint, RouteError> {
    router.terminal_url(&node_id).await
}

// ============================================================================
// Node-specific types (避免与旧命令冲突)
// ============================================================================

/// 写入结果（node_sftp_write 返回值）
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeWriteResult {
    pub mtime: Option<u64>,
    pub size: Option<u64>,
    pub encoding_used: String,
    /// Whether atomic write (swap-file + rename) was used.
    /// `false` means the write fell back to direct overwrite.
    pub atomic_write: bool,
}

// ============================================================================
// Phase 4: 补全缺失的 node_* 命令
// ============================================================================

/// 递归删除目录
#[tauri::command]
pub async fn node_sftp_delete_recursive(
    node_id: String,
    path: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<u64, RouteError> {
    let sftp = router.acquire_sftp(&node_id).await?;
    let sftp = sftp.lock().await;
    sftp.delete_recursive(&path).await.map_err(RouteError::from)
}

/// 递归下载目录
#[tauri::command]
pub async fn node_sftp_download_dir(
    node_id: String,
    remote_path: String,
    local_path: String,
    transfer_id: Option<String>,
    app: AppHandle,
    router: State<'_, Arc<NodeRouter>>,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<u64, RouteError> {
    // Gate concurrency: acquire permit BEFORE opening SSH channel
    let _permit = transfer_manager.acquire_permit().await;
    let resolved = router.resolve_connection(&node_id).await?;
    let sftp = router.acquire_transfer_sftp(&node_id).await?;

    let tid = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let stored_progress = build_directory_transfer_progress(
        &tid,
        TransferType::Download,
        &remote_path,
        &local_path,
        0,
        &resolved.connection_id,
        TransferStrategy::DirectoryRecursive,
    );
    progress_store
        .save(&stored_progress)
        .await
        .map_err(RouteError::from)?;

    let (tx, rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let progress_task = spawn_directory_progress_forwarder(
        app.clone(),
        node_id.clone(),
        rx,
        (*progress_store).clone(),
        stored_progress.clone(),
    );

    // Register with TransferManager for cancel support
    let control = transfer_manager.register(&tid);
    let cancel_flag = create_cancel_flag_bridge(control);

    let result = sftp
        .download_dir(
            &remote_path,
            &local_path,
            &tid,
            Some(tx),
            Some(cancel_flag),
            Some(transfer_manager.speed_limit_bps_ref()),
            transfer_manager.directory_parallelism(),
        )
        .await;
    transfer_manager.unregister(&tid);
    let _ = progress_task.await;
    finalize_directory_transfer_progress(
        progress_store.inner().as_ref(),
        &stored_progress,
        &result,
    )
    .await;

    match &result {
        Ok(_) => emit_transfer_complete(&app, &node_id, &tid, true, None),
        Err(err) => emit_transfer_complete(&app, &node_id, &tid, false, Some(err.to_string())),
    }

    result.map_err(RouteError::from)
}

/// 递归上传目录
#[tauri::command]
pub async fn node_sftp_upload_dir(
    node_id: String,
    local_path: String,
    remote_path: String,
    transfer_id: Option<String>,
    app: AppHandle,
    router: State<'_, Arc<NodeRouter>>,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<u64, RouteError> {
    // Gate concurrency: acquire permit BEFORE opening SSH channel
    let _permit = transfer_manager.acquire_permit().await;
    let resolved = router.resolve_connection(&node_id).await?;
    let sftp = router.acquire_transfer_sftp(&node_id).await?;

    let tid = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let stored_progress = build_directory_transfer_progress(
        &tid,
        TransferType::Upload,
        &local_path,
        &remote_path,
        0,
        &resolved.connection_id,
        TransferStrategy::DirectoryRecursive,
    );
    progress_store
        .save(&stored_progress)
        .await
        .map_err(RouteError::from)?;

    let (tx, rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let progress_task = spawn_directory_progress_forwarder(
        app.clone(),
        node_id.clone(),
        rx,
        (*progress_store).clone(),
        stored_progress.clone(),
    );

    // Register with TransferManager for cancel support
    let control = transfer_manager.register(&tid);
    let cancel_flag = create_cancel_flag_bridge(control);

    let result = sftp
        .upload_dir(
            &local_path,
            &remote_path,
            &tid,
            Some(tx),
            Some(cancel_flag),
            Some(transfer_manager.speed_limit_bps_ref()),
            transfer_manager.directory_parallelism(),
        )
        .await;
    transfer_manager.unregister(&tid);
    let _ = progress_task.await;
    finalize_directory_transfer_progress(
        progress_store.inner().as_ref(),
        &stored_progress,
        &result,
    )
    .await;

    match &result {
        Ok(_) => emit_transfer_complete(&app, &node_id, &tid, true, None),
        Err(err) => emit_transfer_complete(&app, &node_id, &tid, false, Some(err.to_string())),
    }

    result.map_err(RouteError::from)
}

/// 十六进制预览（支持静默重建）
#[tauri::command]
pub async fn node_sftp_preview_hex(
    node_id: String,
    path: String,
    offset: u64,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<PreviewContent, RouteError> {
    sftp_with_retry!(router, &node_id, sftp, {
        sftp.preview_with_offset(&path, offset).await
    })
}

/// 列出未完成的传输
#[tauri::command]
pub async fn node_sftp_list_incomplete_transfers(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
) -> Result<Vec<crate::commands::sftp::IncompleteTransferInfo>, RouteError> {
    let resolved = router.resolve_connection(&node_id).await?;
    load_incomplete_transfer_infos(progress_store.inner().as_ref(), &resolved.connection_id).await
}

/// 恢复传输（带重试）
#[tauri::command]
pub async fn node_sftp_resume_transfer(
    node_id: String,
    transfer_id: String,
    app: AppHandle,
    router: State<'_, Arc<NodeRouter>>,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<(), RouteError> {
    use crate::sftp::progress::TransferType;

    let stored_progress = progress_store
        .load(&transfer_id)
        .await
        .map_err(RouteError::from)?
        .ok_or_else(|| {
            RouteError::SftpOperationError("Transfer not found in progress store".to_string())
        })?;

    // Gate concurrency: acquire permit BEFORE opening SSH channel
    let _permit = transfer_manager.acquire_permit().await;

    let progress_store_arc = (*progress_store).clone();
    let transfer_manager_arc = (*transfer_manager).clone();

    let result = match (
        stored_progress.transfer_type.clone(),
        stored_progress.strategy.clone(),
    ) {
        (TransferType::Download, TransferStrategy::File) => {
            let sftp = router.acquire_transfer_sftp(&node_id).await?;
            let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
            let app_clone = app.clone();
            let node_id_clone = node_id.clone();
            tokio::spawn(async move {
                while let Some(progress) = rx.recv().await {
                    let _ = app_clone.emit(&format!("sftp:progress:{}", node_id_clone), &progress);
                }
            });
            sftp.download_with_resume(
                &stored_progress.source_path.to_string_lossy(),
                &stored_progress.destination_path.to_string_lossy(),
                progress_store_arc.clone(),
                Some(tx),
                Some(transfer_manager_arc.clone()),
                Some(transfer_id.clone()),
            )
            .await
            .map(|_| ())
            .map_err(RouteError::from)
        }
        (TransferType::Upload, TransferStrategy::File) => {
            let sftp = router.acquire_transfer_sftp(&node_id).await?;
            let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
            let app_clone = app.clone();
            let node_id_clone = node_id.clone();
            tokio::spawn(async move {
                while let Some(progress) = rx.recv().await {
                    let _ = app_clone.emit(&format!("sftp:progress:{}", node_id_clone), &progress);
                }
            });
            sftp.upload_with_resume(
                &stored_progress.source_path.to_string_lossy(),
                &stored_progress.destination_path.to_string_lossy(),
                progress_store_arc.clone(),
                Some(tx),
                Some(transfer_manager_arc.clone()),
                Some(transfer_id.clone()),
            )
            .await
            .map(|_| ())
            .map_err(RouteError::from)
        }
        (TransferType::Download, _) => {
            resume_directory_download(
                router.inner().as_ref(),
                &node_id,
                &transfer_id,
                &stored_progress,
                &app,
                progress_store_arc.clone(),
                transfer_manager_arc.clone(),
            )
            .await
        }
        (TransferType::Upload, _) => {
            resume_directory_upload(
                router.inner().as_ref(),
                &node_id,
                &transfer_id,
                &stored_progress,
                &app,
                progress_store_arc.clone(),
                transfer_manager_arc.clone(),
            )
            .await
        }
    };

    match &result {
        Ok(_) => emit_transfer_complete(&app, &node_id, &transfer_id, true, None),
        Err(err) => {
            emit_transfer_complete(&app, &node_id, &transfer_id, false, Some(err.to_string()))
        }
    }

    result
}

/// IDE: 打开项目（通过 nodeId 路由到 SFTP）
#[tauri::command]
pub async fn node_ide_open_project(
    node_id: String,
    path: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<crate::commands::ide::ProjectInfo, RouteError> {
    let sftp = router.acquire_sftp(&node_id).await?;
    let sftp = sftp.lock().await;

    let info = sftp
        .stat(&path)
        .await
        .map_err(|e| RouteError::SftpOperationError(format!("Path not found: {}", e)))?;

    use crate::sftp::types::FileType;
    if info.file_type != FileType::Directory {
        return Err(RouteError::SftpOperationError(
            "Path is not a directory".to_string(),
        ));
    }

    // Use the canonicalized path from stat() — this resolves ~, symlinks,
    // and relative paths to an absolute path that the agent can use directly.
    // Normalize path separators — Windows OpenSSH may return backslashes
    let canonical_path = info.path.replace('\\', "/");

    let git_path = format!("{}/.git", canonical_path.trim_end_matches('/'));
    let is_git_repo = sftp.stat(&git_path).await.is_ok();

    let git_branch = if is_git_repo {
        crate::commands::ide::get_git_branch_inner(&sftp, &canonical_path)
            .await
            .ok()
    } else {
        None
    };

    let name = canonical_path
        .rsplit('/')
        .next()
        .unwrap_or("project")
        .to_string();

    Ok(crate::commands::ide::ProjectInfo {
        root_path: canonical_path,
        name,
        is_git_repo,
        git_branch,
        file_count: 0,
    })
}

/// IDE: 通过 nodeId 执行远程命令
#[tauri::command]
pub async fn node_ide_exec_command(
    node_id: String,
    command: String,
    cwd: Option<String>,
    timeout_secs: Option<u64>,
    router: State<'_, Arc<NodeRouter>>,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<crate::commands::ide::ExecResult, RouteError> {
    // 通过 NodeRouter 解析连接
    let resolved = router.resolve_connection(&node_id).await?;

    // 委托给现有的 ide_exec_command 逻辑
    let controller = connection_registry
        .get_handle_controller(&resolved.connection_id)
        .ok_or_else(|| {
            RouteError::NotConnected(format!("Connection {} not found", resolved.connection_id))
        })?;

    crate::commands::ide::exec_command_inner(controller, command, cwd, timeout_secs)
        .await
        .map_err(|e| RouteError::SftpOperationError(e))
}

/// Check if a file is editable (node-first)
#[tauri::command]
pub async fn node_ide_check_file(
    node_id: String,
    path: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<crate::commands::ide::FileCheckResult, RouteError> {
    let sftp = router.acquire_sftp(&node_id).await?;
    let sftp = sftp.lock().await;

    let info = sftp.stat(&path).await.map_err(RouteError::from)?;

    if info.file_type == FileType::Directory {
        return Ok(crate::commands::ide::FileCheckResult::NotEditable {
            reason: "Is a directory".to_string(),
        });
    }

    const MAX_EDITABLE: u64 = 10 * 1024 * 1024;
    if info.size > MAX_EDITABLE {
        return Ok(crate::commands::ide::FileCheckResult::TooLarge {
            size: info.size,
            limit: MAX_EDITABLE,
        });
    }

    let preview = sftp.preview(&path).await.map_err(RouteError::from)?;

    match preview {
        PreviewContent::Text { .. } => Ok(crate::commands::ide::FileCheckResult::Editable {
            size: info.size,
            mtime: info.modified as u64,
        }),
        PreviewContent::TooLarge { size, max_size, .. } => {
            Ok(crate::commands::ide::FileCheckResult::TooLarge {
                size,
                limit: max_size,
            })
        }
        PreviewContent::Hex { .. } => Ok(crate::commands::ide::FileCheckResult::Binary),
        _ => Ok(crate::commands::ide::FileCheckResult::NotEditable {
            reason: "Unsupported file type".to_string(),
        }),
    }
}

/// Batch stat multiple paths (node-first)
#[tauri::command]
pub async fn node_ide_batch_stat(
    node_id: String,
    paths: Vec<String>,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<Vec<Option<crate::commands::ide::FileStatInfo>>, RouteError> {
    let sftp = router.acquire_sftp(&node_id).await?;
    let sftp = sftp.lock().await;

    let mut results = Vec::with_capacity(paths.len());
    // Parallel stat: launch all requests concurrently
    let futs: Vec<_> = paths.iter().map(|path| sftp.stat(path)).collect();
    let stats = futures_util::future::join_all(futs).await;
    for stat_result in stats {
        let stat = stat_result
            .ok()
            .map(|info| crate::commands::ide::FileStatInfo {
                size: info.size,
                mtime: info.modified as u64,
                is_dir: info.file_type == FileType::Directory,
            });
        results.push(stat);
    }

    Ok(results)
}

// ============================================================================
// Tar streaming transfer commands
// ============================================================================

/// Probe whether the remote host supports `tar` command.
/// Result should be cached per session on the frontend.
#[tauri::command]
pub async fn node_sftp_tar_probe(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<bool, RouteError> {
    let resolved = router.resolve_connection(&node_id).await?;
    Ok(crate::sftp::tar_transfer::probe_tar_support(&resolved.handle_controller).await)
}

/// Probe the best compression method supported by remote `tar`.
/// Returns "zstd", "gzip", or "none". Result should be cached per session.
#[tauri::command]
pub async fn node_sftp_tar_compression_probe(
    node_id: String,
    router: State<'_, Arc<NodeRouter>>,
) -> Result<crate::sftp::tar_transfer::TarCompression, RouteError> {
    let resolved = router.resolve_connection(&node_id).await?;
    Ok(crate::sftp::tar_transfer::probe_tar_compression(&resolved.handle_controller).await)
}

/// Upload a local directory to remote via tar streaming.
/// Falls through to SFTP if tar is not available — caller should check probe first.
#[tauri::command]
pub async fn node_sftp_tar_upload(
    node_id: String,
    local_path: String,
    remote_path: String,
    transfer_id: Option<String>,
    compression: Option<crate::sftp::tar_transfer::TarCompression>,
    app: AppHandle,
    router: State<'_, Arc<NodeRouter>>,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<u64, RouteError> {
    let _permit = transfer_manager.acquire_permit().await;
    let resolved = router.resolve_connection(&node_id).await?;
    let sftp = router.acquire_sftp(&node_id).await?;

    // Register with TransferManager for cancel support
    let tid = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let stored_progress = build_directory_transfer_progress(
        &tid,
        TransferType::Upload,
        &local_path,
        &remote_path,
        0,
        &resolved.connection_id,
        TransferStrategy::DirectoryTar,
    );
    progress_store
        .save(&stored_progress)
        .await
        .map_err(RouteError::from)?;

    let (tx, rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let progress_task = spawn_directory_progress_forwarder(
        app.clone(),
        node_id.clone(),
        rx,
        (*progress_store).clone(),
        stored_progress.clone(),
    );
    let control = transfer_manager.register(&tid);
    let cancel_flag = create_cancel_flag_bridge(control);

    // Recursively create target directory via SFTP (portable `mkdir -p`).
    // Walk path prefixes and create each level; "already exists" errors are ignored.
    // This is O(path_depth), NOT O(file_count) — tar itself creates all internal subdirs.
    {
        let sftp = sftp.lock().await;
        let components: Vec<&str> = remote_path.split('/').filter(|s| !s.is_empty()).collect();
        for i in 0..components.len() {
            let prefix = format!("/{}", components[..=i].join("/"));
            let _ = sftp.mkdir(&prefix).await;
        }
    }

    let result = crate::sftp::tar_transfer::tar_upload_directory(
        &resolved.handle_controller,
        &local_path,
        &remote_path,
        &tid,
        Some(tx),
        Some(cancel_flag),
        compression,
        Some(transfer_manager.speed_limit_bps_ref()),
    )
    .await;

    transfer_manager.unregister(&tid);
    let _ = progress_task.await;
    finalize_directory_transfer_progress(
        progress_store.inner().as_ref(),
        &stored_progress,
        &result,
    )
    .await;

    match &result {
        Ok(_) => emit_transfer_complete(&app, &node_id, &tid, true, None),
        Err(err) => emit_transfer_complete(&app, &node_id, &tid, false, Some(err.to_string())),
    }

    result.map_err(RouteError::from)
}

/// Download a remote directory to local via tar streaming.
#[tauri::command]
pub async fn node_sftp_tar_download(
    node_id: String,
    remote_path: String,
    local_path: String,
    transfer_id: Option<String>,
    compression: Option<crate::sftp::tar_transfer::TarCompression>,
    app: AppHandle,
    router: State<'_, Arc<NodeRouter>>,
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<u64, RouteError> {
    let _permit = transfer_manager.acquire_permit().await;
    let resolved = router.resolve_connection(&node_id).await?;

    let tid = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let stored_progress = build_directory_transfer_progress(
        &tid,
        TransferType::Download,
        &remote_path,
        &local_path,
        0,
        &resolved.connection_id,
        TransferStrategy::DirectoryTar,
    );
    progress_store
        .save(&stored_progress)
        .await
        .map_err(RouteError::from)?;

    let (tx, rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let progress_task = spawn_directory_progress_forwarder(
        app.clone(),
        node_id.clone(),
        rx,
        (*progress_store).clone(),
        stored_progress.clone(),
    );
    let control = transfer_manager.register(&tid);
    let cancel_flag = create_cancel_flag_bridge(control);

    let result = crate::sftp::tar_transfer::tar_download_directory(
        &resolved.handle_controller,
        &remote_path,
        &local_path,
        &tid,
        Some(tx),
        Some(cancel_flag),
        compression,
        Some(transfer_manager.speed_limit_bps_ref()),
    )
    .await;

    transfer_manager.unregister(&tid);
    let _ = progress_task.await;
    finalize_directory_transfer_progress(
        progress_store.inner().as_ref(),
        &stored_progress,
        &result,
    )
    .await;
    match &result {
        Ok(_) => emit_transfer_complete(&app, &node_id, &tid, true, None),
        Err(err) => emit_transfer_complete(&app, &node_id, &tid, false, Some(err.to_string())),
    }

    result.map_err(RouteError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::path::PathBuf;
    use std::sync::Mutex;

    struct RecordingProgressStore {
        last_lookup: Mutex<Option<String>>,
        transfers: Vec<StoredTransferProgress>,
    }

    #[async_trait]
    impl ProgressStore for RecordingProgressStore {
        async fn save(&self, _progress: &StoredTransferProgress) -> Result<(), SftpError> {
            Ok(())
        }

        async fn load(
            &self,
            _transfer_id: &str,
        ) -> Result<Option<StoredTransferProgress>, SftpError> {
            Ok(None)
        }

        async fn list_incomplete(
            &self,
            session_id: &str,
        ) -> Result<Vec<StoredTransferProgress>, SftpError> {
            *self.last_lookup.lock().unwrap() = Some(session_id.to_string());
            Ok(self.transfers.clone())
        }

        async fn list_all_incomplete(&self) -> Result<Vec<StoredTransferProgress>, SftpError> {
            Ok(self.transfers.clone())
        }

        async fn delete(&self, _transfer_id: &str) -> Result<(), SftpError> {
            Ok(())
        }

        async fn delete_for_session(&self, _session_id: &str) -> Result<(), SftpError> {
            Ok(())
        }
    }

    fn make_progress() -> StoredTransferProgress {
        StoredTransferProgress {
            transfer_id: "transfer-1".to_string(),
            transfer_type: TransferType::Download,
            strategy: TransferStrategy::File,
            source_path: PathBuf::from("/remote/file.txt"),
            destination_path: PathBuf::from("/local/file.txt"),
            transferred_bytes: 5,
            total_bytes: 10,
            status: TransferStatus::Failed,
            last_updated: Utc::now(),
            session_id: "conn-1".to_string(),
            error: Some("boom".to_string()),
        }
    }

    #[tokio::test]
    async fn load_incomplete_transfer_infos_uses_connection_id_lookup() {
        let store = RecordingProgressStore {
            last_lookup: Mutex::new(None),
            transfers: vec![make_progress()],
        };

        let infos = load_incomplete_transfer_infos(&store, "conn-1")
            .await
            .unwrap();

        assert_eq!(store.last_lookup.lock().unwrap().as_deref(), Some("conn-1"));
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].session_id, "conn-1");
        assert!(infos[0].can_resume);
        assert!(!infos[0].is_directory);
    }

    #[test]
    fn map_incomplete_transfer_info_marks_only_paused_and_failed_transfers_resumable() {
        let mut paused = make_progress();
        paused.status = TransferStatus::Paused;

        let mut failed = make_progress();
        failed.status = TransferStatus::Failed;

        let mut active = make_progress();
        active.status = TransferStatus::Active;

        let paused_info = map_incomplete_transfer_info(paused);
        let failed_info = map_incomplete_transfer_info(failed);
        let active_info = map_incomplete_transfer_info(active);

        assert!(paused_info.can_resume);
        assert!(failed_info.can_resume);
        assert!(!active_info.can_resume);
    }

    #[test]
    fn map_incomplete_transfer_info_preserves_status_and_progress_shape() {
        let info = map_incomplete_transfer_info(make_progress());

        assert_eq!(info.transfer_type, "Download");
        assert_eq!(info.status, "Failed");
        assert_eq!(info.source_path, "/remote/file.txt");
        assert_eq!(info.destination_path, "/local/file.txt");
        assert_eq!(info.progress_percent, 50.0);
        assert_eq!(info.error.as_deref(), Some("boom"));
    }

    #[test]
    fn build_transfer_complete_event_uses_node_scoped_payload() {
        let event = build_transfer_complete_event(
            "node-1",
            "transfer-1",
            false,
            Some("failed".to_string()),
        );

        assert_eq!(
            transfer_complete_event_name("node-1"),
            "sftp:complete:node-1"
        );
        assert_eq!(
            event,
            TransferCompleteEvent {
                transfer_id: "transfer-1".to_string(),
                node_id: "node-1".to_string(),
                success: false,
                error: Some("failed".to_string()),
            }
        );
    }

    #[test]
    fn map_incomplete_transfer_info_marks_directory_transfers() {
        let mut progress = make_progress();
        progress.strategy = TransferStrategy::DirectoryTar;

        let info = map_incomplete_transfer_info(progress);

        assert!(info.is_directory);
    }

    #[test]
    fn update_directory_transfer_progress_preserves_high_water_total() {
        let mut progress = build_directory_transfer_progress(
            "dir-1",
            TransferType::Upload,
            "/local/dir",
            "/remote/dir",
            0,
            "conn-1",
            TransferStrategy::DirectoryTar,
        );

        update_directory_transfer_progress(
            &mut progress,
            &TransferProgress {
                id: "dir-1".to_string(),
                remote_path: "/remote/dir".to_string(),
                local_path: "/local/dir".to_string(),
                direction: TransferDirection::Upload,
                state: TransferState::InProgress,
                total_bytes: 128,
                transferred_bytes: 64,
                speed: 1,
                eta_seconds: None,
                error: None,
            },
        );

        update_directory_transfer_progress(
            &mut progress,
            &TransferProgress {
                id: "dir-1".to_string(),
                remote_path: "/remote/dir/sub".to_string(),
                local_path: "/local/dir/sub".to_string(),
                direction: TransferDirection::Upload,
                state: TransferState::InProgress,
                total_bytes: 64,
                transferred_bytes: 96,
                speed: 1,
                eta_seconds: None,
                error: None,
            },
        );

        assert_eq!(progress.total_bytes, 128);
        assert_eq!(progress.transferred_bytes, 96);
    }
}
