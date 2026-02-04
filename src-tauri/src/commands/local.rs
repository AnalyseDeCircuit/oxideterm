//! Local Terminal Commands
//!
//! Tauri commands for local terminal (PTY) operations.
//!
//! # 命令列表
//!
//! - `local_list_shells` - 列出可用的 shell
//! - `local_get_default_shell` - 获取默认 shell
//! - `local_create_terminal` - 创建本地终端会话
//! - `local_close_terminal` - 关闭本地终端会话
//! - `local_resize_terminal` - 调整终端大小
//! - `local_list_terminals` - 列出所有本地终端
//! - `local_write_terminal` - 向终端写入数据

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::local::registry::LocalTerminalRegistry;
use crate::local::session::{LocalTerminalInfo, SessionEvent};
use crate::local::shell::{scan_shells, default_shell, ShellInfo};

/// Global local terminal registry state
pub struct LocalTerminalState {
    pub registry: LocalTerminalRegistry,
}

impl LocalTerminalState {
    pub fn new() -> Self {
        Self {
            registry: LocalTerminalRegistry::new(),
        }
    }
}

impl Default for LocalTerminalState {
    fn default() -> Self {
        Self::new()
    }
}

/// Request to create a local terminal
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLocalTerminalRequest {
    /// Shell path (optional, uses default if not specified)
    pub shell_path: Option<String>,
    /// Terminal columns
    #[serde(default = "default_cols")]
    pub cols: u16,
    /// Terminal rows  
    #[serde(default = "default_rows")]
    pub rows: u16,
    /// Working directory (optional)
    pub cwd: Option<String>,
    /// Whether to load shell profile (default: true)
    #[serde(default = "default_load_profile")]
    pub load_profile: bool,
    /// Enable Oh My Posh prompt theme (Windows)
    #[serde(default)]
    pub oh_my_posh_enabled: bool,
    /// Path to Oh My Posh theme file
    pub oh_my_posh_theme: Option<String>,
}

fn default_cols() -> u16 {
    80
}

fn default_rows() -> u16 {
    24
}

fn default_load_profile() -> bool {
    true
}

/// Response from creating a local terminal
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLocalTerminalResponse {
    /// Session ID
    pub session_id: String,
    /// Session info
    pub info: LocalTerminalInfo,
}

/// Event emitted when local terminal outputs data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTerminalDataEvent {
    pub session_id: String,
    pub data: Vec<u8>,
}

/// Event emitted when local terminal closes
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTerminalClosedEvent {
    pub session_id: String,
    pub exit_code: Option<i32>,
}

/// List available shells on the system
#[tauri::command]
pub async fn local_list_shells() -> Result<Vec<ShellInfo>, String> {
    Ok(scan_shells())
}

/// Get the default shell for the current platform
#[tauri::command]
pub async fn local_get_default_shell() -> Result<ShellInfo, String> {
    Ok(default_shell())
}

/// Create a new local terminal session
#[tauri::command]
pub async fn local_create_terminal(
    request: CreateLocalTerminalRequest,
    state: State<'_, Arc<LocalTerminalState>>,
    app: AppHandle,
) -> Result<CreateLocalTerminalResponse, String> {
    tracing::info!(
        "local_create_terminal called with shell_path: {:?}, cwd: {:?}",
        request.shell_path,
        request.cwd
    );
    
    // Determine which shell to use
    let shell = if let Some(path) = request.shell_path {
        // Find shell by path
        let shells = scan_shells();
        let path_buf = std::path::PathBuf::from(&path);
        
        let found_shell = shells
            .into_iter()
            .find(|s| {
                // Normalize path for comparison (handles case-insensitivity on Windows)
                #[cfg(target_os = "windows")]
                {
                    s.path.to_string_lossy().to_lowercase() == path.to_lowercase()
                }
                #[cfg(not(target_os = "windows"))]
                {
                    s.path == path_buf
                }
            });
        
        match found_shell {
            Some(shell) => {
                tracing::info!("Found matching shell: {} ({})", shell.label, shell.path.display());
                shell
            }
            None => {
                // Create shell info for custom path
                tracing::warn!(
                    "Shell path '{}' not found in scanned shells, creating custom shell info",
                    path
                );
                let id = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("custom")
                    .to_string();
                ShellInfo::new(id.clone(), id, path_buf)
            }
        }
    } else {
        let shell = default_shell();
        tracing::info!("No shell_path provided, using default: {} ({})", shell.label, shell.path.display());
        shell
    };

    let cwd = request.cwd.map(std::path::PathBuf::from);

    // Create session through registry with options
    let (session_id, mut event_rx) = state
        .registry
        .create_session_with_options(
            shell, 
            request.cols, 
            request.rows, 
            cwd,
            request.load_profile,
            request.oh_my_posh_enabled,
            request.oh_my_posh_theme,
        )
        .await
        .map_err(|e| format!("Failed to create local terminal: {}", e))?;

    // Get session info
    let info = state
        .registry
        .get_session_info(&session_id)
        .await
        .ok_or_else(|| "Session not found after creation".to_string())?;

    // Spawn task to forward events to frontend
    let app_handle = app.clone();
    let sid = session_id.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                SessionEvent::Data(data) => {
                    let event = LocalTerminalDataEvent {
                        session_id: sid.clone(),
                        data,
                    };
                    if let Err(e) = app_handle.emit(&format!("local-terminal-data:{}", sid), &event) {
                        tracing::error!("Failed to emit terminal data event: {}", e);
                    }
                }
                SessionEvent::Closed(exit_code) => {
                    let event = LocalTerminalClosedEvent {
                        session_id: sid.clone(),
                        exit_code,
                    };
                    if let Err(e) = app_handle.emit(&format!("local-terminal-closed:{}", sid), &event) {
                        tracing::error!("Failed to emit terminal closed event: {}", e);
                    }
                    break;
                }
            }
        }
        tracing::debug!("Event forwarder for session {} exited", sid);
    });

    tracing::info!("Created local terminal session: {}", session_id);

    Ok(CreateLocalTerminalResponse { session_id, info })
}

/// Close a local terminal session
#[tauri::command]
pub async fn local_close_terminal(
    session_id: String,
    state: State<'_, Arc<LocalTerminalState>>,
) -> Result<(), String> {
    state
        .registry
        .close_session(&session_id)
        .await
        .map_err(|e| format!("Failed to close session: {}", e))?;

    tracing::info!("Closed local terminal session: {}", session_id);
    Ok(())
}

/// Resize a local terminal
#[tauri::command]
pub async fn local_resize_terminal(
    session_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, Arc<LocalTerminalState>>,
) -> Result<(), String> {
    state
        .registry
        .resize_session(&session_id, cols, rows)
        .await
        .map_err(|e| format!("Failed to resize session: {}", e))?;

    tracing::debug!("Resized local terminal {}: {}x{}", session_id, cols, rows);
    Ok(())
}

/// List all local terminal sessions
#[tauri::command]
pub async fn local_list_terminals(
    state: State<'_, Arc<LocalTerminalState>>,
) -> Result<Vec<LocalTerminalInfo>, String> {
    Ok(state.registry.list_sessions().await)
}

/// Write data to a local terminal (input from frontend)
#[tauri::command]
pub async fn local_write_terminal(
    session_id: String,
    data: Vec<u8>,
    state: State<'_, Arc<LocalTerminalState>>,
) -> Result<(), String> {
    state
        .registry
        .write_to_session(&session_id, &data)
        .await
        .map_err(|e| format!("Failed to write to session: {}", e))
}

/// Get session info for a specific terminal
#[tauri::command]
pub async fn local_get_terminal_info(
    session_id: String,
    state: State<'_, Arc<LocalTerminalState>>,
) -> Result<LocalTerminalInfo, String> {
    state
        .registry
        .get_session_info(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))
}

/// Clean up dead sessions
#[tauri::command]
pub async fn local_cleanup_dead_sessions(
    state: State<'_, Arc<LocalTerminalState>>,
) -> Result<Vec<String>, String> {
    Ok(state.registry.cleanup_dead_sessions().await)
}

/// Get available local drives (Windows: A-Z drives, Unix: root)
/// 
/// Returns a list of available drive paths that can be navigated to.
/// On Windows, this scans A-Z for existing drives.
/// On Unix, returns just "/" as the root.
#[tauri::command]
pub fn local_get_drives() -> Vec<String> {
    #[cfg(windows)]
    {
        let mut drives = Vec::new();
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            if std::path::Path::new(&drive).exists() {
                drives.push(drive);
            }
        }
        drives
    }
    #[cfg(not(windows))]
    {
        vec!["/".to_string()]
    }
}

/// File metadata response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
    /// File size in bytes
    pub size: u64,
    /// Last modified time (Unix timestamp in seconds)
    pub modified: Option<u64>,
    /// Created time (Unix timestamp in seconds) - may not be available on all platforms
    pub created: Option<u64>,
    /// Last accessed time (Unix timestamp in seconds)
    pub accessed: Option<u64>,
    /// Unix permissions mode (e.g., 0o755)
    #[cfg(unix)]
    pub mode: u32,
    /// Is readonly
    pub readonly: bool,
    /// Is directory
    pub is_dir: bool,
    /// Is symlink
    pub is_symlink: bool,
    /// MIME type (guessed from extension)
    pub mime_type: Option<String>,
}

/// File chunk response for streaming preview
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChunk {
    pub data: Vec<u8>,
    pub eof: bool,
}

/// Get detailed file metadata
/// 
/// Returns comprehensive file information including size, timestamps, and permissions.
/// This is called only when entering preview mode, not during directory listing.
#[tauri::command]
pub async fn local_get_file_metadata(path: String) -> Result<FileMetadata, String> {
    use std::fs;
    use std::time::UNIX_EPOCH;
    
    let path = std::path::Path::new(&path);
    let metadata = fs::metadata(path)
        .map_err(|e| format!("Failed to get metadata: {}", e))?;
    
    let symlink_metadata = fs::symlink_metadata(path).ok();
    let is_symlink = symlink_metadata.map(|m| m.file_type().is_symlink()).unwrap_or(false);
    
    // Get timestamps
    let modified = metadata.modified().ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    
    let created = metadata.created().ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    
    let accessed = metadata.accessed().ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    
    // Guess MIME type from extension
    let mime_type = path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| guess_mime_type(ext));
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        
        Ok(FileMetadata {
            size: metadata.len(),
            modified,
            created,
            accessed,
            mode,
            readonly: metadata.permissions().readonly(),
            is_dir: metadata.is_dir(),
            is_symlink,
            mime_type,
        })
    }
    
    #[cfg(not(unix))]
    {
        Ok(FileMetadata {
            size: metadata.len(),
            modified,
            created,
            accessed,
            readonly: metadata.permissions().readonly(),
            is_dir: metadata.is_dir(),
            is_symlink,
            mime_type,
        })
    }
}

/// Read a chunk from a file for streaming preview
#[tauri::command]
pub async fn local_read_file_range(path: String, offset: u64, length: u64) -> Result<FileChunk, String> {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    let mut file = File::open(&path).map_err(|e| format!("Failed to open file: {}", e))?;
    let metadata = file.metadata().map_err(|e| format!("Failed to get metadata: {}", e))?;
    let file_len = metadata.len();

    if offset >= file_len {
        return Ok(FileChunk { data: Vec::new(), eof: true });
    }

    let safe_len = length.min(1024 * 1024); // Cap to 1MB per read
    file.seek(SeekFrom::Start(offset)).map_err(|e| format!("Failed to seek file: {}", e))?;

    let mut buffer = vec![0u8; safe_len as usize];
    let bytes_read = file.read(&mut buffer).map_err(|e| format!("Failed to read file: {}", e))?;
    buffer.truncate(bytes_read);

    let eof = offset + bytes_read as u64 >= file_len || bytes_read == 0;

    Ok(FileChunk { data: buffer, eof })
}

/// Guess MIME type from file extension
fn guess_mime_type(ext: &str) -> String {
    match ext.to_lowercase().as_str() {
        // Images
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        // Videos
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        // Audio
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        // Documents
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        // Archives
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        // Code/Text
        "js" => "text/javascript",
        "ts" => "text/typescript",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "md" => "text/markdown",
        "txt" => "text/plain",
        "py" => "text/x-python",
        "rs" => "text/x-rust",
        "go" => "text/x-go",
        "java" => "text/x-java",
        "c" | "h" => "text/x-c",
        "cpp" | "hpp" | "cc" => "text/x-c++",
        "sh" | "bash" => "text/x-shellscript",
        "yaml" | "yml" => "text/yaml",
        "toml" => "text/x-toml",
        _ => "application/octet-stream",
    }.to_string()
}

