//! IDE Mode Commands
//!
//! Commands for the lightweight IDE mode feature.

use russh::ChannelMsg;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use crate::sftp::session::SftpRegistry;
use crate::sftp::types::{FileType, PreviewContent};
use crate::ssh::SshConnectionRegistry;

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

/// Result of executing a remote command
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Exit code (None if terminated by signal)
    pub exit_code: Option<u32>,
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

pub(crate) async fn get_git_branch_inner(
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

/// Execute a command on the remote server via SSH exec channel
///
/// This is used for running commands like `grep` for search and `git status` for file status.
#[tauri::command]
pub async fn ide_exec_command(
    connection_id: String,
    command: String,
    cwd: Option<String>,
    timeout_secs: Option<u64>,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
) -> Result<ExecResult, String> {
    let controller = connection_registry
        .get_handle_controller(&connection_id)
        .ok_or_else(|| format!("Connection not found: {}", connection_id))?;

    exec_command_inner(controller, command, cwd, timeout_secs).await
}

/// 执行远程命令的内部实现（供 node_ide_exec_command 复用）
pub(crate) async fn exec_command_inner(
    controller: crate::ssh::HandleController,
    command: String,
    cwd: Option<String>,
    timeout_secs: Option<u64>,
) -> Result<ExecResult, String> {
    use tokio::time::{timeout, Duration};
    use tracing::{debug, warn};

    // Open a new session channel
    let mut channel = controller
        .open_session_channel()
        .await
        .map_err(|e| format!("Failed to open exec channel: {}", e))?;

    // Build command with optional cwd
    let full_command = match cwd {
        Some(dir) => format!("cd {} && {}", shell_escape(&dir), command),
        None => command.clone(),
    };

    debug!("IDE exec: {}", full_command);

    // Execute the command
    channel
        .exec(true, full_command.clone())
        .await
        .map_err(|e| format!("Failed to execute command: {}", e))?;

    // Collect output
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut exit_code: Option<u32> = None;

    // Use timeout to prevent hanging
    let timeout_duration = Duration::from_secs(timeout_secs.unwrap_or(30));

    let result = timeout(timeout_duration, async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    stdout.extend_from_slice(&data);
                }
                Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                    // ext=1 is stderr
                    stderr.extend_from_slice(&data);
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exit_code = Some(exit_status);
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                    break;
                }
                Some(_other) => {
                    // Ignore other messages (WindowAdjusted, Success, etc.)
                }
                None => {
                    // Channel closed
                    break;
                }
            }
        }
    })
    .await;

    // Handle timeout
    if result.is_err() {
        warn!("IDE exec timed out after {:?}: {}", timeout_duration, command);
        // Close the channel to clean up
        let _ = channel.close().await;
        return Err(format!("Command timed out after {} seconds", timeout_duration.as_secs()));
    }

    // Convert output to strings
    let stdout_str = String::from_utf8_lossy(&stdout).to_string();
    let stderr_str = String::from_utf8_lossy(&stderr).to_string();

    debug!(
        "IDE exec completed: exit={:?} stdout_len={} stderr_len={}",
        exit_code,
        stdout_str.len(),
        stderr_str.len()
    );

    Ok(ExecResult {
        stdout: stdout_str,
        stderr: stderr_str,
        exit_code,
    })
}

/// Escape a string for use in shell command
fn shell_escape(s: &str) -> String {
    // Simple escaping - wrap in single quotes and escape single quotes
    format!("'{}'", s.replace('\'', "'\\''"))
}
