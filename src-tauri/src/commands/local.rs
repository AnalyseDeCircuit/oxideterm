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
use crate::local::shell::{default_shell, scan_shells, ShellInfo};

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

        let found_shell = shells.into_iter().find(|s| {
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
                tracing::info!(
                    "Found matching shell: {} ({})",
                    shell.label,
                    shell.path.display()
                );
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
        tracing::info!(
            "No shell_path provided, using default: {} ({})",
            shell.label,
            shell.path.display()
        );
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
                    if let Err(e) = app_handle.emit(&format!("local-terminal-data:{}", sid), &event)
                    {
                        tracing::error!("Failed to emit terminal data event: {}", e);
                    }
                }
                SessionEvent::Closed(exit_code) => {
                    let event = LocalTerminalClosedEvent {
                        session_id: sid.clone(),
                        exit_code,
                    };
                    if let Err(e) =
                        app_handle.emit(&format!("local-terminal-closed:{}", sid), &event)
                    {
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

/// Drive / volume information returned to the frontend.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    /// Mount point path (e.g. "/", "/Volumes/USB", "C:\\")
    pub path: String,
    /// Human-readable display name (volume label or folder name)
    pub name: String,
    /// Drive classification: "system" | "removable" | "network"
    pub drive_type: String,
    /// Total capacity in bytes
    pub total_space: u64,
    /// Available (free) space in bytes
    pub available_space: u64,
    /// Whether the volume is mounted read-only
    pub is_read_only: bool,
}

/// Get available local drives / mounted volumes.
///
/// Uses `sysinfo::Disks` for cross-platform detection:
/// - **macOS**: system root + external volumes (filters the /Volumes symlink to /)
/// - **Linux**: all mounted partitions from /proc/mounts (including USB, NAS, etc.)
/// - **Windows**: all lettered drives (C:\, D:\, etc.)
///
/// Each entry includes capacity info so the frontend can render usage bars.
#[tauri::command]
pub fn local_get_drives() -> Vec<DriveInfo> {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();
    let mut drives: Vec<DriveInfo> = Vec::new();
    // Track seen volumes by device ID (Unix) or canonical path (Windows).
    // On macOS, APFS firmlinks (e.g. /Volumes/Macintosh HD → /) share the
    // same dev_id but canonicalize() won't resolve them. Comparing dev_id
    // catches both symlinks and firmlinks. We keep the shortest mount path.
    #[cfg(unix)]
    let mut seen_dev_ids: std::collections::HashMap<u64, usize> = std::collections::HashMap::new(); // dev_id → index in `drives`
    #[cfg(not(unix))]
    let mut seen_mount_points: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();

    for disk in disks.list() {
        let mount_point = disk.mount_point().to_path_buf();

        // ── Deduplication (check only; registration happens after filtering) ──
        #[cfg(unix)]
        let unix_dev_id: Option<u64>;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(meta) = std::fs::metadata(&mount_point) {
                let dev = meta.dev();
                if let Some(&existing_idx) = seen_dev_ids.get(&dev) {
                    // Same device — keep the one with the shorter path (prefer "/" over "/Volumes/Macintosh HD")
                    if mount_point.as_os_str().len() < drives[existing_idx].path.len() {
                        // Current path is shorter — replace the existing entry
                        drives[existing_idx].path = mount_point.to_string_lossy().to_string();
                        drives[existing_idx].name = {
                            let raw = disk.name().to_string_lossy().to_string();
                            if raw.is_empty() {
                                mount_point
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| {
                                        if mount_point.to_string_lossy() == "/" { "System".to_string() } else { mount_point.to_string_lossy().to_string() }
                                    })
                            } else {
                                raw
                            }
                        };
                    }
                    continue; // Skip duplicate either way
                }
                unix_dev_id = Some(dev);
            } else {
                unix_dev_id = None;
            }
        }
        #[cfg(not(unix))]
        {
            let canonical = mount_point.canonicalize().unwrap_or_else(|_| mount_point.clone());
            if seen_mount_points.contains(&canonical) {
                continue;
            }
            seen_mount_points.insert(canonical);
        }

        // Skip pseudo/virtual filesystems (common on Linux)
        let mount_str = mount_point.to_string_lossy();
        if mount_str.starts_with("/proc")
            || mount_str.starts_with("/sys")
            || mount_str.starts_with("/dev")
            || mount_str.starts_with("/snap")
            || mount_str == "/boot"
            || mount_str == "/boot/efi"
        {
            continue;
        }
        // /run has many pseudo-paths but some are real mounts:
        //   /run/media/$USER/*  — udisks2 auto-mount (most distros)
        //   /run/mount/*        — some desktop environments
        //   /run/user/*/gvfs/*  — GNOME virtual FS mounts
        // Block /run unless it matches one of these real-mount patterns.
        if mount_str.starts_with("/run")
            && !mount_str.starts_with("/run/media/")
            && !mount_str.starts_with("/run/mount/")
            && !mount_str.starts_with("/run/user/")
        {
            continue;
        }
        // /run/user/* sub-paths: only allow gvfs mounts
        if mount_str.starts_with("/run/user/") {
            if !mount_str.contains("/gvfs/") {
                continue;
            }
        }

        let drive_type = classify_disk(disk);

        // Determine display name
        let raw_name = disk.name().to_string_lossy().to_string();
        let name = if raw_name.is_empty() {
            // Derive name from mount point
            mount_point
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| {
                    if mount_str == "/" { "System".to_string() } else { mount_str.to_string() }
                })
        } else {
            raw_name
        };

        // Register dev_id only now, after all filters have passed
        #[cfg(unix)]
        if let Some(dev) = unix_dev_id {
            seen_dev_ids.insert(dev, drives.len());
        }

        // Determine read-only status.
        // On macOS Catalina+, the root system volume (/) is technically read-only
        // (Signed System Volume), but firmlinks to the Data volume make it
        // functionally writable for the user. Avoid showing a misleading badge.
        let is_read_only = if cfg!(target_os = "macos") && mount_str == "/" {
            // Check if the user-writable firmlink target is actually writable
            !std::fs::metadata("/Users")
                .map(|m| !m.permissions().readonly())
                .unwrap_or(false)
        } else {
            disk.is_read_only()
        };

        drives.push(DriveInfo {
            path: mount_str.to_string(),
            name,
            drive_type,
            total_space: disk.total_space(),
            available_space: disk.available_space(),
            is_read_only,
        });
    }

    // Sort: system first, then alphabetical by path
    drives.sort_by(|a, b| {
        let a_sys = if a.drive_type == "system" { 0 } else { 1 };
        let b_sys = if b.drive_type == "system" { 0 } else { 1 };
        a_sys.cmp(&b_sys).then(a.path.cmp(&b.path))
    });

    // Fallback: ensure at least root is present
    if drives.is_empty() {
        #[cfg(not(windows))]
        drives.push(DriveInfo {
            path: "/".to_string(),
            name: "System".to_string(),
            drive_type: "system".to_string(),
            total_space: 0,
            available_space: 0,
            is_read_only: false,
        });
    }

    drives
}

/// Classify a disk as "system", "removable", or "network".
fn classify_disk(disk: &sysinfo::Disk) -> String {
    use sysinfo::DiskKind;

    // Check if it's the root mount point → system
    let mount = disk.mount_point().to_string_lossy();
    #[cfg(not(windows))]
    if mount == "/" {
        return "system".to_string();
    }
    #[cfg(windows)]
    {
        // On Windows, C:\ is typically the system drive
        let m = mount.to_uppercase();
        if m.starts_with("C:") {
            return "system".to_string();
        }
    }

    // Check if removable by disk kind or filesystem hints
    if disk.is_removable() {
        return "removable".to_string();
    }

    // Network filesystems
    let fs_type = disk.file_system().to_string_lossy().to_lowercase();
    if fs_type == "nfs" || fs_type == "cifs" || fs_type == "smb" || fs_type == "smbfs"
        || fs_type == "afpfs" || fs_type == "9p" || fs_type == "fuse.sshfs"
    {
        return "network".to_string();
    }

    // SSDs / HDDs that aren't root and aren't removable
    match disk.kind() {
        DiskKind::SSD | DiskKind::HDD => "system".to_string(),
        _ => "removable".to_string(),
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
    let metadata = fs::metadata(path).map_err(|e| format!("Failed to get metadata: {}", e))?;

    let symlink_metadata = fs::symlink_metadata(path).ok();
    let is_symlink = symlink_metadata
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);

    // Get timestamps
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let created = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let accessed = metadata
        .accessed()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    // Detect MIME type: try magic bytes first, fall back to extension
    let mime_type = if !metadata.is_dir() {
        detect_mime_type(path)
    } else {
        None
    };

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
pub async fn local_read_file_range(
    path: String,
    offset: u64,
    length: u64,
) -> Result<FileChunk, String> {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    let mut file = File::open(&path).map_err(|e| format!("Failed to open file: {}", e))?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("Failed to get metadata: {}", e))?;
    let file_len = metadata.len();

    if offset >= file_len {
        return Ok(FileChunk {
            data: Vec::new(),
            eof: true,
        });
    }

    let safe_len = length.min(1024 * 1024); // Cap to 1MB per read
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Failed to seek file: {}", e))?;

    let mut buffer = vec![0u8; safe_len as usize];
    let bytes_read = file
        .read(&mut buffer)
        .map_err(|e| format!("Failed to read file: {}", e))?;
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
    }
    .to_string()
}

/// Detect MIME type using magic bytes (infer crate), falling back to extension
fn detect_mime_type(path: &std::path::Path) -> Option<String> {
    // Try magic bytes first (reads first 8192 bytes)
    if let Ok(mut file) = std::fs::File::open(path) {
        use std::io::Read;
        let mut buf = [0u8; 8192];
        if let Ok(n) = file.read(&mut buf) {
            if let Some(kind) = infer::get(&buf[..n]) {
                return Some(kind.mime_type().to_string());
            }
        }
    }

    // Fall back to extension-based guess
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| guess_mime_type(ext))
}

/// Checksum result
#[derive(Debug, Serialize)]
pub struct ChecksumResult {
    pub md5: String,
    pub sha256: String,
}

/// Calculate MD5 and SHA256 checksums for a file
#[tauri::command]
pub async fn local_calculate_checksum(path: String) -> Result<ChecksumResult, String> {
    use md5::Md5;
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file =
        std::fs::File::open(&path).map_err(|e| format!("Failed to open file: {}", e))?;

    let mut md5_hasher = Md5::new();
    let mut sha256_hasher = Sha256::new();
    let mut buf = [0u8; 65536];

    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Failed to read file: {}", e))?;
        if n == 0 {
            break;
        }
        md5_hasher.update(&buf[..n]);
        sha256_hasher.update(&buf[..n]);
    }

    Ok(ChecksumResult {
        md5: format!("{:x}", md5_hasher.finalize()),
        sha256: format!("{:x}", sha256_hasher.finalize()),
    })
}

/// Directory statistics result
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirStatsResult {
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
}

/// Get directory statistics (file count, subdir count, total size)
#[tauri::command]
pub async fn local_dir_stats(path: String) -> Result<DirStatsResult, String> {
    use walkdir::WalkDir;

    let mut file_count: u64 = 0;
    let mut dir_count: u64 = 0;
    let mut total_size: u64 = 0;

    for entry in WalkDir::new(&path).min_depth(1) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue, // skip permission errors
        };
        let ft = entry.file_type();
        if ft.is_file() {
            file_count += 1;
            total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
        } else if ft.is_dir() {
            dir_count += 1;
        }
    }

    Ok(DirStatsResult {
        file_count,
        dir_count,
        total_size,
    })
}

/// Dynamically allow a single file for the asset protocol scope.
/// This avoids a blanket `**` scope by authorizing files one at a time
/// right before the renderer needs to stream them.
/// Symlinks are resolved via canonicalize; only the canonical path is authorized.
/// Returns the canonical path so the frontend uses it for the asset URL.
#[tauri::command]
pub fn allow_asset_file(app: tauri::AppHandle, path: String) -> Result<String, String> {
    use tauri::Manager;
    let file_path = std::path::PathBuf::from(&path);
    // Resolve symlinks to the real path — only the canonical target is authorized
    // to prevent TOCTOU races if the symlink is retargeted.
    let canonical = std::fs::canonicalize(&file_path)
        .map_err(|e| format!("Failed to resolve path '{}': {}", path, e))?;
    app.asset_protocol_scope()
        .allow_file(&canonical)
        .map_err(|e| format!("Failed to allow asset file: {}", e))?;
    canonical
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Canonical path contains invalid UTF-8".to_string())
}

/// "Revoke" a file that was previously allowed on the asset protocol scope.
///
/// **Important:** This is intentionally a no-op. Tauri's `forbid_file` adds
/// the path to `forbidden_patterns`, which takes **permanent precedence** over
/// `allowed_patterns`. There is no Tauri API to remove a forbidden pattern.
/// Calling `forbid_file` then `allow_file` on the same path leaves it in
/// *both* sets, and the forbidden check wins — resulting in a permanent 403
/// for that path for the rest of the session.
///
/// Since the allowed set is ephemeral (cleared on app restart), leaving paths
/// in it is harmless and far preferable to poisoning them.
#[tauri::command]
pub fn revoke_asset_file(_app: tauri::AppHandle, _path: String) -> Result<(), String> {
    // Intentional no-op — see doc comment above.
    Ok(())
}

// ── Audio Metadata ──────────────────────────────────────────────────────────

/// Audio metadata returned to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioMetadata {
    /// Duration in seconds
    pub duration_secs: Option<f64>,
    /// Overall bitrate in kbps (may be average for VBR)
    pub bitrate_kbps: Option<u32>,
    /// Sample rate in Hz (e.g. 44100)
    pub sample_rate: Option<u32>,
    /// Bit depth if available (e.g. 16, 24)
    pub bit_depth: Option<u8>,
    /// Number of channels
    pub channels: Option<u8>,
    /// Codec / format description
    pub codec: Option<String>,
    /// ID3 / Vorbis tags
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub track_number: Option<u32>,
    pub comment: Option<String>,
    /// Embedded lyrics (USLT / Vorbis LYRICS)
    pub lyrics: Option<String>,
    /// true if embedded cover art was detected
    pub has_cover: bool,
}

/// Read audio file metadata (ID3, Vorbis, FLAC, etc.) using the `lofty` crate.
#[tauri::command]
pub fn get_audio_metadata(path: String) -> Result<AudioMetadata, String> {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::{Accessor, ItemKey};
    use lofty::probe::Probe;

    let tagged_file = Probe::open(&path)
        .map_err(|e| format!("Cannot open '{}': {}", path, e))?
        .read()
        .map_err(|e| format!("Cannot read tags from '{}': {}", path, e))?;

    let properties = tagged_file.properties();
    let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());

    let (title, artist, album, year, genre, track_number, comment, lyrics, has_cover) =
        if let Some(t) = tag {
            (
                t.title().map(|s| s.to_string()),
                t.artist().map(|s| s.to_string()),
                t.album().map(|s| s.to_string()),
                t.year(),
                t.genre().map(|s| s.to_string()),
                t.track(),
                t.comment().map(|s| s.to_string()),
                t.get_string(&ItemKey::Lyrics).map(|s| s.to_string()),
                !t.pictures().is_empty(),
            )
        } else {
            (None, None, None, None, None, None, None, None, false)
        };

    Ok(AudioMetadata {
        duration_secs: {
            let d = properties.duration();
            let secs = d.as_secs_f64();
            if secs > 0.0 { Some(secs) } else { None }
        },
        bitrate_kbps: properties.audio_bitrate(),
        sample_rate: properties.sample_rate(),
        bit_depth: properties.bit_depth(),
        channels: properties.channels(),
        codec: None, // lofty doesn't expose codec name directly; we use file extension on frontend
        title,
        artist,
        album,
        year,
        genre,
        track_number,
        comment,
        lyrics,
        has_cover,
    })
}
