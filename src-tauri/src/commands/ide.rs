//! IDE Mode Commands
//!
//! Commands for the lightweight IDE mode feature.

use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use crate::sftp::session::SftpRegistry;
use crate::sftp::types::{FileType, PreviewContent};

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub root_path: String,
    pub name: String,
    pub is_git_repo: bool,
    pub git_branch: Option<String>,
    pub file_count: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatInfo {
    pub size: u64,
    pub mtime: u64,
    pub is_dir: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileCheckResult {
    Editable { size: u64, mtime: u64 },
    TooLarge { size: u64, limit: u64 },
    Binary,
    NotEditable { reason: String },
}

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

const MAX_EDITABLE_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB

// ═══════════════════════════════════════════════════════════════════════════
// Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Open a project directory and return basic info
#[tauri::command]
pub async fn ide_open_project(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<ProjectInfo, String> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| format!("SFTP session not found: {}", session_id))?;

    let sftp = sftp.lock().await;

    // Verify directory exists
    let info = sftp
        .stat(&path)
        .await
        .map_err(|e| format!("Path not found: {}", e))?;

    if info.file_type != FileType::Directory {
        return Err("Path is not a directory".to_string());
    }

    // Check if it's a Git repository
    let git_path = format!("{}/.git", path);
    let is_git_repo = sftp.stat(&git_path).await.is_ok();

    // Get Git branch if applicable
    let git_branch = if is_git_repo {
        get_git_branch_inner(&sftp, &path).await.ok()
    } else {
        None
    };

    // Extract project name from path
    let name = path
        .rsplit('/')
        .next()
        .unwrap_or("project")
        .to_string();

    Ok(ProjectInfo {
        root_path: path,
        name,
        is_git_repo,
        git_branch,
        file_count: 0, // Defer counting
    })
}

/// Check if a file is editable
#[tauri::command]
pub async fn ide_check_file(
    session_id: String,
    path: String,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<FileCheckResult, String> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| format!("SFTP session not found: {}", session_id))?;

    let sftp = sftp.lock().await;

    // Get file info
    let info = sftp
        .stat(&path)
        .await
        .map_err(|e| format!("File not found: {}", e))?;

    if info.file_type == FileType::Directory {
        return Ok(FileCheckResult::NotEditable {
            reason: "Is a directory".to_string(),
        });
    }

    if info.size > MAX_EDITABLE_FILE_SIZE {
        return Ok(FileCheckResult::TooLarge {
            size: info.size,
            limit: MAX_EDITABLE_FILE_SIZE,
        });
    }

    // Use preview to detect file type
    let preview = sftp.preview(&path).await.map_err(|e| e.to_string())?;

    match preview {
        PreviewContent::Text { .. } => Ok(FileCheckResult::Editable {
            size: info.size,
            mtime: info.modified as u64,
        }),
        PreviewContent::TooLarge { size, max_size, .. } => Ok(FileCheckResult::TooLarge {
            size,
            limit: max_size,
        }),
        PreviewContent::Hex { .. } => Ok(FileCheckResult::Binary),
        _ => Ok(FileCheckResult::NotEditable {
            reason: "Unsupported file type".to_string(),
        }),
    }
}

/// Batch stat multiple paths
#[tauri::command]
pub async fn ide_batch_stat(
    session_id: String,
    paths: Vec<String>,
    sftp_registry: State<'_, Arc<SftpRegistry>>,
) -> Result<Vec<Option<FileStatInfo>>, String> {
    let sftp = sftp_registry
        .get(&session_id)
        .ok_or_else(|| format!("SFTP session not found: {}", session_id))?;

    let sftp = sftp.lock().await;

    let mut results = Vec::with_capacity(paths.len());
    for path in paths {
        let stat = sftp.stat(&path).await.ok().map(|info| FileStatInfo {
            size: info.size,
            mtime: info.modified as u64,
            is_dir: info.file_type == FileType::Directory,
        });
        results.push(stat);
    }

    Ok(results)
}

// ═══════════════════════════════════════════════════════════════════════════
// Internal Helpers
// ═══════════════════════════════════════════════════════════════════════════

async fn get_git_branch_inner(
    sftp: &tokio::sync::MutexGuard<'_, crate::sftp::session::SftpSession>,
    project_path: &str,
) -> Result<String, String> {
    let head_path = format!("{}/.git/HEAD", project_path);

    // Use preview to read the file
    let preview = sftp.preview(&head_path).await.map_err(|e| e.to_string())?;

    let content = match preview {
        PreviewContent::Text { data, .. } => data,
        _ => return Err("HEAD is not a text file".to_string()),
    };

    // Parse: ref: refs/heads/main
    if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
        Ok(branch.trim().to_string())
    } else {
        // Detached HEAD - return short hash
        Ok(content.chars().take(7).collect())
    }
}
