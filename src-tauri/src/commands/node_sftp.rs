//! Node-first SFTP commands — Phase 0 of Oxide-Next
//!
//! 所有命令接受 nodeId 而非 sessionId。
//! 内部通过 NodeRouter 解析到具体资源。
//!
//! 参考: docs/OXIDE_NEXT_ARCHITECTURE.md §3.2

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tracing::info;

use crate::router::{NodeRouter, NodeStateSnapshot, RouteError, TerminalEndpoint};
use crate::sftp::error::SftpError;
use crate::sftp::types::*;
use crate::ssh::SshConnectionRegistry;

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
        let _ = app.asset_protocol_scope().allow_file(std::path::Path::new(path));
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
        if let (Ok(canonical), Ok(canonical_dir)) =
            (target.canonicalize(), temp_dir.canonicalize())
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
    sftp.write_content(&path, &encoded_bytes)
        .await
        .map_err(RouteError::from)?;

    // 获取写入后的元数据
    let file_info = sftp.stat(&path).await.map_err(RouteError::from)?;

    info!(
        "node_sftp_write: wrote {} bytes to {} (encoding: {})",
        file_info.size, path, target_encoding
    );

    Ok(NodeWriteResult {
        mtime: if file_info.modified > 0 {
            Some(file_info.modified as u64)
        } else {
            None
        },
        size: Some(file_info.size),
        encoding_used: target_encoding.to_string(),
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

    sftp.download_with_resume(
        &remote_path,
        &local_path,
        (*progress_store).clone(),
        Some(tx),
        Some((*transfer_manager).clone()),
        transfer_id,
    )
    .await
    .map(|_| ())
    .map_err(RouteError::from)
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

    sftp.upload_with_resume(
        &local_path,
        &remote_path,
        (*progress_store).clone(),
        Some(tx),
        Some((*transfer_manager).clone()),
        transfer_id,
    )
    .await
    .map(|_| ())
    .map_err(RouteError::from)
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
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<u64, RouteError> {
    // Gate concurrency: acquire permit BEFORE opening SSH channel
    let _permit = transfer_manager.acquire_permit().await;
    let sftp = router.acquire_transfer_sftp(&node_id).await?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let app_clone = app.clone();
    let node_id_clone = node_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", node_id_clone), &progress);
        }
    });

    // Register with TransferManager for cancel support
    let tid = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let control = transfer_manager.register(&tid);
    // Build a lightweight AtomicBool cancel flag bridged from TransferControl
    let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag_clone = cancel_flag.clone();
    let mut cancel_rx = control.subscribe_cancellation();
    tokio::spawn(async move {
        // When TransferControl is cancelled, flip the flag
        while cancel_rx.changed().await.is_ok() {
            if *cancel_rx.borrow() {
                flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                break;
            }
        }
    });

    let result = sftp.download_dir(&remote_path, &local_path, Some(tx), Some(cancel_flag), Some(transfer_manager.speed_limit_bps_ref()))
        .await
        .map_err(RouteError::from);
    transfer_manager.unregister(&tid);
    result
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
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<u64, RouteError> {
    // Gate concurrency: acquire permit BEFORE opening SSH channel
    let _permit = transfer_manager.acquire_permit().await;
    let sftp = router.acquire_transfer_sftp(&node_id).await?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let app_clone = app.clone();
    let node_id_clone = node_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", node_id_clone), &progress);
        }
    });

    // Register with TransferManager for cancel support
    let tid = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let control = transfer_manager.register(&tid);
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

    let result = sftp.upload_dir(&local_path, &remote_path, Some(tx), Some(cancel_flag), Some(transfer_manager.speed_limit_bps_ref()))
        .await
        .map_err(RouteError::from);
    transfer_manager.unregister(&tid);
    result
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
    progress_store: State<'_, Arc<dyn crate::sftp::ProgressStore>>,
) -> Result<Vec<crate::commands::sftp::IncompleteTransferInfo>, RouteError> {
    use crate::sftp::progress::{TransferStatus, TransferType};

    // 使用 node_id 作为 key 查询（后端进度存储以 node_id 为前缀）
    // 注意：传输进度可能以旧 session_id 或新 node_id 为 key 存储
    let transfers = progress_store
        .list_incomplete(&node_id)
        .await
        .map_err(RouteError::from)?;

    let result: Vec<crate::commands::sftp::IncompleteTransferInfo> = transfers
        .into_iter()
        .map(|t| {
            let progress_percent = if t.total_bytes > 0 {
                (t.transferred_bytes as f64 / t.total_bytes as f64) * 100.0
            } else {
                0.0
            };
            let can_resume = matches!(t.status, TransferStatus::Paused | TransferStatus::Failed);
            crate::commands::sftp::IncompleteTransferInfo {
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
    let sftp = router.acquire_transfer_sftp(&node_id).await?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let app_clone = app.clone();
    let node_id_clone = node_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", node_id_clone), &progress);
        }
    });

    let progress_store_arc = (*progress_store).clone();
    let transfer_manager_arc = (*transfer_manager).clone();

    match stored_progress.transfer_type {
        TransferType::Download => {
            sftp.download_with_resume(
                &stored_progress.source_path.to_string_lossy(),
                &stored_progress.destination_path.to_string_lossy(),
                progress_store_arc,
                Some(tx),
                Some(transfer_manager_arc),
                Some(transfer_id.clone()),
            )
            .await
            .map(|_| ())
            .map_err(RouteError::from)?;
        }
        TransferType::Upload => {
            sftp.upload_with_resume(
                &stored_progress.source_path.to_string_lossy(),
                &stored_progress.destination_path.to_string_lossy(),
                progress_store_arc,
                Some(tx),
                Some(transfer_manager_arc),
                Some(transfer_id.clone()),
            )
            .await
            .map(|_| ())
            .map_err(RouteError::from)?;
        }
    }

    Ok(())
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

    let git_path = format!("{}/.git", path);
    let is_git_repo = sftp.stat(&git_path).await.is_ok();

    let git_branch = if is_git_repo {
        crate::commands::ide::get_git_branch_inner(&sftp, &path)
            .await
            .ok()
    } else {
        None
    };

    let name = path.rsplit('/').next().unwrap_or("project").to_string();

    Ok(crate::commands::ide::ProjectInfo {
        root_path: path,
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
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<u64, RouteError> {
    let _permit = transfer_manager.acquire_permit().await;
    let resolved = router.resolve_connection(&node_id).await?;
    let sftp = router.acquire_sftp(&node_id).await?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let app_clone = app.clone();
    let node_id_clone = node_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", node_id_clone), &progress);
        }
    });

    // Register with TransferManager for cancel support
    let tid = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let control = transfer_manager.register(&tid);
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
    .await
    .map_err(RouteError::from);

    transfer_manager.unregister(&tid);
    result
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
    transfer_manager: State<'_, Arc<crate::sftp::TransferManager>>,
) -> Result<u64, RouteError> {
    let _permit = transfer_manager.acquire_permit().await;
    let resolved = router.resolve_connection(&node_id).await?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let app_clone = app.clone();
    let node_id_clone = node_id.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = app_clone.emit(&format!("sftp:progress:{}", node_id_clone), &progress);
        }
    });

    let tid = transfer_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let control = transfer_manager.register(&tid);
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
    .await
    .map_err(RouteError::from);

    transfer_manager.unregister(&tid);
    result
}
