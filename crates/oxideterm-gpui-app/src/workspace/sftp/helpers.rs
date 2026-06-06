#[derive(Clone)]
struct PathSegment {
    name: String,
    full_path: String,
}

fn sftp_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, SFTP_BG_ACTIVE_BG_ALPHA)
}

fn sftp_panel_bg(color: u32, has_background: bool, alpha: u32) -> Rgba {
    color_with_background_scaled_alpha(color, has_background, alpha, SFTP_BG_ACTIVE_PANEL_ALPHA)
}

fn sftp_card_surface(surface: gpui::Div, color: u32) -> gpui::Div {
    // SFTP queue subcards mirror Tauri bg-theme-bg-card; the shadow belongs to
    // that card token even when the caller keeps a custom active-background alpha.
    tauri_glass_surface_shadow(surface, color)
}

fn sftp_hover_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, SFTP_BG_ACTIVE_HOVER_ALPHA)
}

fn sftp_border(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, 0x99)
}

fn is_sftp_incomplete_store_compat_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("deserialize")
        || error.contains("invalid type")
        || error.contains("connection_not_found")
        || error.contains("notfound")
        || error.contains("not found")
}

fn home_path() -> String {
    local_home_path_buf()
        .unwrap_or_else(|| std::path::PathBuf::from(home_path_root()))
        .to_string_lossy()
        .to_string()
}

fn local_home_path_buf() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(std::path::PathBuf::from))
}

/// Expands home aliases before local file manager paths reach filesystem APIs.
fn expand_local_path(path: &str) -> std::path::PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" || trimmed == "$HOME" {
        return local_home_path_buf().unwrap_or_else(|| std::path::PathBuf::from(trimmed));
    }

    for prefix in ["~/", "~\\", "$HOME/", "$HOME\\"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            if let Some(home) = local_home_path_buf() {
                return home.join(rest);
            }
        }
    }

    std::path::PathBuf::from(trimmed)
}

fn list_local_files(path: &str) -> std::io::Result<Vec<SftpFileEntry>> {
    let mut entries = Vec::new();
    let expanded_path = expand_local_path(path);
    for entry in std::fs::read_dir(expanded_path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = match std::fs::symlink_metadata(&entry_path) {
            Ok(metadata) => metadata,
            Err(_) => {
                // Windows reserved device names can fail metadata probing even
                // when directory enumeration reports them; keep the listing open.
                continue;
            }
        };
        let name = entry.file_name().to_string_lossy().to_string();
        let full_path = entry_path.to_string_lossy().to_string();
        let file_type = if metadata.is_dir() {
            SftpFileType::Directory
        } else {
            SftpFileType::File
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64);
        entries.push(SftpFileEntry {
            name,
            path: full_path,
            file_type,
            size: metadata.len(),
            modified,
            permissions: None,
            owner: None,
            group: None,
            is_symlink: metadata.file_type().is_symlink(),
            symlink_target: std::fs::read_link(&entry_path)
                .ok()
                .map(|target| target.to_string_lossy().to_string()),
        });
    }
    entries.sort_by(|left, right| match (left.file_type, right.file_type) {
        (SftpFileType::Directory, SftpFileType::File) => std::cmp::Ordering::Less,
        (SftpFileType::File, SftpFileType::Directory) => std::cmp::Ordering::Greater,
        _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
    });
    Ok(entries)
}

fn local_drives() -> Vec<SftpDrive> {
    let mut drives = platform_local_drives();
    drives.sort_by(|left, right| {
        let left_system = if left.drive_type == "system" { 0 } else { 1 };
        let right_system = if right.drive_type == "system" { 0 } else { 1 };
        left_system
            .cmp(&right_system)
            .then_with(|| left.path.cmp(&right.path))
    });
    if drives.is_empty() {
        drives.push(SftpDrive {
            name: "System".to_string(),
            path: home_path_root(),
            drive_type: "system",
            total_space: 0,
            available_space: 0,
            read_only: false,
        });
    }
    drives
}

fn home_path_root() -> String {
    #[cfg(windows)]
    {
        "C:\\".to_string()
    }
    #[cfg(not(windows))]
    {
        "/".to_string()
    }
}

fn platform_local_drives() -> Vec<SftpDrive> {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();
    let mut drives: Vec<SftpDrive> = Vec::new();

    #[cfg(unix)]
    let mut seen_dev_ids: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    #[cfg(not(unix))]
    let mut seen_mount_points: std::collections::HashSet<std::path::PathBuf> =
        std::collections::HashSet::new();

    for disk in disks.list() {
        let mount_point = disk.mount_point().to_path_buf();

        #[cfg(unix)]
        let unix_dev_id = {
            use std::os::unix::fs::MetadataExt;
            match std::fs::metadata(&mount_point) {
                Ok(metadata) => {
                    let dev = metadata.dev();
                    if let Some(&existing_idx) = seen_dev_ids.get(&dev) {
                        if mount_point.as_os_str().len() < drives[existing_idx].path.len() {
                            drives[existing_idx].path = mount_point.to_string_lossy().to_string();
                            drives[existing_idx].name = drive_display_name(disk, &mount_point);
                        }
                        continue;
                    }
                    Some(dev)
                }
                Err(_) => None,
            }
        };
        #[cfg(not(unix))]
        {
            let canonical = mount_point
                .canonicalize()
                .unwrap_or_else(|_| mount_point.clone());
            if !seen_mount_points.insert(canonical) {
                continue;
            }
        }

        let mount = mount_point.to_string_lossy();
        if is_pseudo_mount(&mount) {
            continue;
        }

        #[cfg(unix)]
        if let Some(dev) = unix_dev_id {
            seen_dev_ids.insert(dev, drives.len());
        }

        let read_only = if cfg!(target_os = "macos") && mount == "/" {
            !std::fs::metadata("/Users")
                .map(|metadata| !metadata.permissions().readonly())
                .unwrap_or(false)
        } else {
            disk.is_read_only()
        };

        drives.push(SftpDrive {
            name: drive_display_name(disk, &mount_point),
            path: mount.to_string(),
            drive_type: classify_disk(disk),
            total_space: disk.total_space(),
            available_space: disk.available_space(),
            read_only,
        });
    }
    drives
}

fn is_pseudo_mount(mount: &str) -> bool {
    mount.starts_with("/proc")
        || mount.starts_with("/sys")
        || mount.starts_with("/dev")
        || mount.starts_with("/snap")
        || mount == "/boot"
        || mount == "/boot/efi"
        || is_blocked_run_mount(mount)
}

fn is_blocked_run_mount(mount: &str) -> bool {
    if !mount.starts_with("/run") {
        return false;
    }
    if mount.starts_with("/run/media/") || mount.starts_with("/run/mount/") {
        return false;
    }
    mount.starts_with("/run/user/") && !mount.contains("/gvfs/")
        || (!mount.starts_with("/run/user/"))
}

fn drive_display_name(disk: &sysinfo::Disk, mount_point: &std::path::Path) -> String {
    let raw_name = disk.name().to_string_lossy().to_string();
    if !raw_name.is_empty() {
        return raw_name;
    }
    let mount = mount_point.to_string_lossy();
    mount_point
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            if mount == "/" {
                "System".to_string()
            } else {
                mount.to_string()
            }
        })
}

fn classify_disk(disk: &sysinfo::Disk) -> &'static str {
    use sysinfo::DiskKind;

    let mount = disk.mount_point().to_string_lossy();
    #[cfg(not(windows))]
    if mount == "/" {
        return "system";
    }
    #[cfg(windows)]
    if mount.to_uppercase().starts_with("C:") {
        return "system";
    }

    if disk.is_removable() {
        return "removable";
    }

    let fs_type = disk.file_system().to_string_lossy().to_lowercase();
    if matches!(
        fs_type.as_str(),
        "nfs" | "cifs" | "smb" | "smbfs" | "afpfs" | "9p" | "fuse.sshfs"
    ) {
        return "network";
    }

    match disk.kind() {
        DiskKind::SSD | DiskKind::HDD => "system",
        _ => "removable",
    }
}

fn sftp_file_entry(
    name: String,
    path: String,
    file_type: SftpFileType,
    size: u64,
    modified: Option<i64>,
) -> SftpFileEntry {
    SftpFileEntry {
        name,
        path,
        file_type,
        size,
        modified,
        permissions: None,
        owner: None,
        group: None,
        is_symlink: false,
        symlink_target: None,
    }
}

fn sorted_sftp_files(
    files: &[SftpFileEntry],
    filter: &str,
    sort_field: SftpSortField,
    sort_direction: SftpSortDirection,
) -> Vec<SftpFileEntry> {
    let filter = filter.trim().to_lowercase();
    let mut filtered = files
        .iter()
        .filter(|file| filter.is_empty() || file.name.to_lowercase().contains(&filter))
        .cloned()
        .collect::<Vec<_>>();
    filtered.sort_by(|left, right| {
        if left.file_type == SftpFileType::Directory && right.file_type != SftpFileType::Directory {
            return std::cmp::Ordering::Less;
        }
        if left.file_type != SftpFileType::Directory && right.file_type == SftpFileType::Directory {
            return std::cmp::Ordering::Greater;
        }
        let ordering = match sort_field {
            SftpSortField::Name => left.name.cmp(&right.name),
            SftpSortField::Size => left.size.cmp(&right.size),
            SftpSortField::Modified => left.modified.cmp(&right.modified),
        };
        match sort_direction {
            SftpSortDirection::Asc => ordering,
            SftpSortDirection::Desc => ordering.reverse(),
        }
    });
    filtered
}

fn sftp_path_segments(path: &str, is_remote: bool) -> Vec<PathSegment> {
    let normalized = if is_remote {
        normalize_remote_path(path)
    } else {
        path.replace('\\', "/")
    };
    let mut segments = Vec::new();
    segments.push(PathSegment {
        name: "/".to_string(),
        full_path: "/".to_string(),
    });
    let without_root = normalized.trim_start_matches('/');
    let mut current = String::from("/");
    for part in without_root.split('/').filter(|part| !part.is_empty()) {
        current = if current == "/" {
            format!("/{part}")
        } else {
            format!("{current}/{part}")
        };
        segments.push(PathSegment {
            name: part.to_string(),
            full_path: current.clone(),
        });
    }
    segments
}

fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }
    let normalized = trimmed.replace('\\', "/").replace("//", "/");
    if normalized.starts_with('/') {
        normalized
    } else {
        format!("/{normalized}")
    }
}

fn parent_path(path: &str, remote: bool) -> String {
    let normalized = if remote {
        normalize_remote_path(path)
    } else {
        path.replace('\\', "/")
    };
    if normalized == "/" {
        return "/".to_string();
    }
    let mut parts = normalized
        .trim_end_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    parts.pop();
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn join_sftp_path(base: &str, name: &str) -> String {
    let normalized = base.trim_end_matches('/');
    if normalized.is_empty() {
        format!("/{name}")
    } else if normalized == "/" {
        format!("/{name}")
    } else {
        format!("{normalized}/{name}")
    }
}

fn remote_directory_prefixes(path: &str) -> Vec<String> {
    let mut prefixes = Vec::new();
    let absolute = path.starts_with('/');
    let components: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    for index in 0..components.len() {
        let joined = components[..=index].join("/");
        prefixes.push(if absolute {
            format!("/{joined}")
        } else {
            joined
        });
    }
    prefixes
}

fn join_local_path(base: &str, name: &str) -> String {
    let mut path = std::path::PathBuf::from(base);
    path.push(name);
    path.to_string_lossy().to_string()
}

fn normalize_external_dropped_path(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let raw = path.to_string_lossy();
    if raw.trim().is_empty() {
        return None;
    }
    if raw.len() >= 2
        && raw.as_bytes()[1] == b':'
        && raw.chars().skip(2).all(|ch| ch == '/' || ch == '\\')
    {
        return Some(std::path::PathBuf::from(format!("{}:\\", &raw[..1])));
    }
    if raw.chars().all(|ch| ch == '/' || ch == '\\') {
        return raw.chars().next().map(|root| std::path::PathBuf::from(root.to_string()));
    }
    Some(std::path::PathBuf::from(raw.trim_end_matches(['/', '\\'])))
}

fn new_sftp_transfer_id(node_id: &NodeId, name: &str) -> String {
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let safe_name = name.replace(['/', '\\', ':'], "_");
    format!("{}-{timestamp_ms}-{safe_name}", node_id.0)
}

fn unique_sftp_conflict_name(name: &str, existing_files: &[SftpFileEntry]) -> String {
    let existing_names = existing_files
        .iter()
        .map(|file| file.name.as_str())
        .collect::<HashSet<_>>();
    let last_dot = name.rfind('.');
    let (base_name, extension) = match last_dot {
        Some(index) if index > 0 => (&name[..index], &name[index..]),
        _ => (name, ""),
    };

    let mut counter = 1;
    loop {
        let candidate = format!("{base_name} ({counter}){extension}");
        if !existing_names.contains(candidate.as_str()) {
            return candidate;
        }
        counter += 1;
    }
}

fn sftp_conflict_resolution_from_settings(
    action: oxideterm_settings::ConflictAction,
) -> SftpConflictResolution {
    match action {
        oxideterm_settings::ConflictAction::Ask
        | oxideterm_settings::ConflictAction::Overwrite => SftpConflictResolution::Overwrite,
        oxideterm_settings::ConflictAction::Skip => SftpConflictResolution::Skip,
        oxideterm_settings::ConflictAction::Rename => SftpConflictResolution::Rename,
    }
}

fn sftp_transfer_conflicts(
    pending_transfers: &[SftpPendingTransfer],
    target_files: &[SftpFileEntry],
) -> Vec<SftpConflictInfo> {
    pending_transfers
        .iter()
        .filter(|transfer| transfer.source.file_type != SftpFileType::Directory)
        .filter_map(|transfer| {
            let target = target_files.iter().find(|file| {
                file.name == transfer.name && file.file_type != SftpFileType::Directory
            })?;
            Some(SftpConflictInfo {
                file_name: transfer.name.clone(),
                source_size: transfer.source.size,
                source_modified: transfer.source.modified,
                target_size: target.size,
                target_modified: target.modified,
                direction: transfer.direction,
            })
        })
        .collect()
}

fn sftp_source_not_newer_than_target(
    transfer: &SftpPendingTransfer,
    target_files: &[SftpFileEntry],
) -> bool {
    let Some(target) = target_files.iter().find(|file| {
        file.name == transfer.name && file.file_type != SftpFileType::Directory
    }) else {
        return false;
    };
    match (transfer.source.modified, target.modified) {
        (Some(source_modified), Some(target_modified)) => source_modified <= target_modified,
        _ => false,
    }
}

fn sftp_transfer_state_from_remote(state: RemoteTransferState) -> SftpTransferState {
    match state {
        RemoteTransferState::Pending => SftpTransferState::Pending,
        RemoteTransferState::InProgress => SftpTransferState::Active,
        RemoteTransferState::Paused => SftpTransferState::Paused,
        RemoteTransferState::Completed => SftpTransferState::Completed,
        RemoteTransferState::Failed => SftpTransferState::Error,
        RemoteTransferState::Cancelled => SftpTransferState::Cancelled,
    }
}

fn sftp_transfer_state_from_background(state: BackgroundTransferState) -> SftpTransferState {
    match state {
        BackgroundTransferState::Pending => SftpTransferState::Pending,
        BackgroundTransferState::Active => SftpTransferState::Active,
        BackgroundTransferState::Paused => SftpTransferState::Paused,
        BackgroundTransferState::Completed => SftpTransferState::Completed,
        BackgroundTransferState::Cancelled => SftpTransferState::Cancelled,
        BackgroundTransferState::Error => SftpTransferState::Error,
    }
}

fn preview_content_text(content: &PreviewContent) -> String {
    match content {
        PreviewContent::Text {
            data,
            encoding,
            confidence,
            has_bom,
            ..
        } => {
            let bom = if *has_bom { ", BOM" } else { "" };
            format!(
                "encoding: {encoding} ({:.0}%{bom})\n\n{data}",
                confidence * 100.0
            )
        }
        PreviewContent::Image { mime_type, data } => {
            format!(
                "{mime_type}\nimage preview payload: {} base64 chars",
                data.len()
            )
        }
        PreviewContent::AssetFile {
            path,
            mime_type,
            kind,
        } => {
            format!("{kind:?} asset\n{mime_type}\n{path}")
        }
        PreviewContent::Hex {
            data,
            total_size,
            offset,
            chunk_size,
            has_more,
        } => {
            format!(
                "hex preview: offset {offset}, chunk {chunk_size}, total {total_size}, has_more {has_more}\n\n{data}"
            )
        }
        PreviewContent::TooLarge {
            size,
            max_size,
            recommend_download,
        } => {
            format!(
                "too large to preview: {size} bytes (limit {max_size}), recommend_download={recommend_download}"
            )
        }
        PreviewContent::Unsupported { mime_type, reason } => {
            format!("unsupported preview: {mime_type}\n{reason}")
        }
    }
}

fn sftp_preview_is_markdown(language: Option<&str>, mime_type: Option<&str>) -> bool {
    language.is_some_and(|language| {
        matches!(
            language.to_ascii_lowercase().as_str(),
            "markdown" | "md" | "rmd"
        )
    }) || mime_type.is_some_and(|mime_type| {
        matches!(
            mime_type.to_ascii_lowercase().as_str(),
            "text/markdown" | "text/x-markdown"
        )
    })
}

fn sftp_editor_language(language: Option<&str>, name: &str) -> String {
    let raw = language
        .filter(|language| !language.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::path::Path::new(name)
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "text".to_string());
    match raw.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" => "typescript",
        "md" | "markdown" => "markdown",
        "yml" => "yaml",
        "sh" | "bash" | "zsh" => "bash",
        "makefile" | "mk" => "make",
        "txt" | "text" | "conf" | "cfg" | "ini" | "env" => "text",
        other => other,
    }
    .to_string()
}

async fn load_remote_sftp_listing(
    router: NodeRouter,
    node_id: &NodeId,
    path: &str,
) -> Result<RemoteSftpListing, String> {
    let sftp = router
        .acquire_sftp(node_id)
        .await
        .map_err(|error| error.to_string())?;
    match list_remote_sftp_once(&sftp, path).await {
        Ok(listing) => Ok(listing),
        Err(error) if error.is_channel_recoverable() => {
            let sftp = router
                .invalidate_and_reacquire_sftp(node_id)
                .await
                .map_err(|route_error| route_error.to_string())?;
            list_remote_sftp_once(&sftp, path)
                .await
                .map_err(|retry_error| retry_error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

async fn load_remote_sftp_preview(
    router: NodeRouter,
    node_id: &NodeId,
    path: &str,
) -> Result<PreviewContent, String> {
    let sftp = router
        .acquire_sftp(node_id)
        .await
        .map_err(|error| error.to_string())?;
    match load_remote_sftp_preview_once(&sftp, path).await {
        Ok(preview) => Ok(preview),
        Err(error) if error.is_channel_recoverable() => {
            // Tauri wraps node_sftp_preview in sftp_with_retry!, so stale shared
            // SFTP channels are invalidated and rebuilt once before surfacing an
            // error. Keep preview on that same silent-rebuild path as listing.
            let sftp = router
                .invalidate_and_reacquire_sftp(node_id)
                .await
                .map_err(|route_error| route_error.to_string())?;
            load_remote_sftp_preview_once(&sftp, path)
                .await
                .map_err(|retry_error| retry_error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

async fn load_remote_sftp_preview_once(
    sftp: &std::sync::Arc<tokio::sync::Mutex<SftpSession>>,
    path: &str,
) -> Result<PreviewContent, SftpError> {
    let sftp = sftp.lock().await;
    sftp.preview(path).await
}

async fn load_remote_sftp_preview_hex(
    router: NodeRouter,
    node_id: &NodeId,
    path: &str,
    offset: u64,
) -> Result<PreviewContent, String> {
    let sftp = router
        .acquire_sftp(node_id)
        .await
        .map_err(|error| error.to_string())?;
    match load_remote_sftp_preview_hex_once(&sftp, path, offset).await {
        Ok(preview) => Ok(preview),
        Err(error) if error.is_channel_recoverable() => {
            // Mirrors Tauri node_sftp_preview_hex: retry once after rebuilding
            // the shared SFTP owner when the channel itself is stale.
            let sftp = router
                .invalidate_and_reacquire_sftp(node_id)
                .await
                .map_err(|route_error| route_error.to_string())?;
            load_remote_sftp_preview_hex_once(&sftp, path, offset)
                .await
                .map_err(|retry_error| retry_error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

async fn load_remote_sftp_preview_hex_once(
    sftp: &std::sync::Arc<tokio::sync::Mutex<SftpSession>>,
    path: &str,
    offset: u64,
) -> Result<PreviewContent, SftpError> {
    let sftp = sftp.lock().await;
    sftp.preview_with_offset(path, offset).await
}

async fn save_remote_sftp_preview(
    router: NodeRouter,
    node_id: &NodeId,
    path: &str,
    content: &str,
    encoding: &str,
) -> Result<SftpPreviewSaveResult, String> {
    let target_encoding = if encoding.trim().is_empty() {
        "UTF-8"
    } else {
        encoding
    };
    let encoded = encode_to_encoding(content, target_encoding);
    let sftp = router
        .acquire_sftp(node_id)
        .await
        .map_err(|error| error.to_string())?;
    let sftp = sftp.lock().await;
    let write_result = sftp
        .write_content(path, &encoded)
        .await
        .map_err(|error| error.to_string())?;
    let file_info = sftp.stat(path).await.map_err(|error| error.to_string())?;
    Ok(SftpPreviewSaveResult {
        mtime: (file_info.modified > 0).then_some(file_info.modified as u64),
        size: Some(file_info.size),
        encoding_used: target_encoding.to_string(),
        atomic_write: write_result.atomic_write,
    })
}

fn sftp_preview_editor_is_network_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    [
        "network",
        "connection",
        "timeout",
        "disconnected",
        "eof",
        "broken pipe",
        "reset by peer",
        "channel closed",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

async fn list_remote_sftp_once(
    sftp: &std::sync::Arc<tokio::sync::Mutex<SftpSession>>,
    path: &str,
) -> Result<RemoteSftpListing, SftpError> {
    let sftp = sftp.lock().await;
    // Tauri's node_sftp_list_dir performs one SFTP path resolution inside
    // list_dir. Native used to canonicalize here and then list_dir canonicalized
    // again, adding a visible RTT on every folder change.
    let (cwd, entries) = sftp
        .list_dir_with_cwd(
            path,
            Some(RemoteListFilter {
                show_hidden: true,
                pattern: None,
                sort: RemoteSortOrder::Name,
            }),
        )
        .await?;
    Ok(remote_listing_from_file_infos(cwd, entries))
}

fn remote_listing_from_file_infos(cwd: String, entries: Vec<RemoteFileInfo>) -> RemoteSftpListing {
    let mut files = entries
        .into_iter()
        .map(|entry| SftpFileEntry {
            name: entry.name,
            path: entry.path,
            file_type: match entry.file_type {
                RemoteFileType::Directory => SftpFileType::Directory,
                RemoteFileType::File | RemoteFileType::Symlink | RemoteFileType::Unknown => {
                    SftpFileType::File
                }
            },
            size: entry.size,
            modified: Some(entry.modified),
            permissions: Some(entry.permissions),
            owner: entry.owner,
            group: entry.group,
            is_symlink: entry.is_symlink,
            symlink_target: entry.symlink_target,
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| match (left.file_type, right.file_type) {
        (SftpFileType::Directory, SftpFileType::File) => std::cmp::Ordering::Less,
        (SftpFileType::File, SftpFileType::Directory) => std::cmp::Ordering::Greater,
        _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
    });
    RemoteSftpListing { cwd, files }
}

fn format_file_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1024.0 && index < units.len() - 1 {
        value /= 1024.0;
        index += 1;
    }
    if index == 0 {
        format!("{} {}", value.round() as u64, units[index])
    } else {
        format!("{value:.1} {}", units[index])
    }
}

fn format_transfer_speed(bytes_per_second: u64) -> String {
    if bytes_per_second == 0 {
        return "-".to_string();
    }
    format!("{}/s", format_file_size(bytes_per_second))
}

fn format_modified(modified: Option<i64>) -> String {
    let Some(modified) = modified.filter(|modified| *modified > 0) else {
        return "-".to_string();
    };
    let Some(datetime) = chrono::DateTime::from_timestamp(modified, 0) else {
        return "-".to_string();
    };
    // Tauri renders `new Date(file.modified * 1000).toLocaleDateString()`;
    // native keeps the same Unix-seconds -> local-date contract instead of
    // showing UTC or a placeholder date.
    datetime
        .with_timezone(&chrono::Local)
        .format("%Y/%-m/%-d")
        .to_string()
}

fn format_conflict_modified(modified: Option<i64>) -> String {
    let Some(modified) = modified else {
        return "Unknown".to_string();
    };
    let Some(datetime) = chrono::DateTime::from_timestamp(modified, 0) else {
        return "Unknown".to_string();
    };
    datetime
        .with_timezone(&chrono::Local)
        .format("%Y/%-m/%-d %-H:%M:%S")
        .to_string()
}

fn compute_sftp_diff(left: &str, right: &str) -> Vec<SftpDiffLine> {
    let left_lines = left.split('\n').collect::<Vec<_>>();
    let right_lines = right.split('\n').collect::<Vec<_>>();
    let m = left_lines.len();
    let n = right_lines.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if left_lines[i - 1] == right_lines[j - 1] {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }

    let mut i = m;
    let mut j = n;
    let mut diff = Vec::new();
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && left_lines[i - 1] == right_lines[j - 1] {
            diff.push(SftpDiffLine {
                kind: SftpDiffLineKind::Unchanged,
                content: left_lines[i - 1].to_string(),
                left_line_num: Some(i),
                right_line_num: Some(j),
            });
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            diff.push(SftpDiffLine {
                kind: SftpDiffLineKind::Added,
                content: right_lines[j - 1].to_string(),
                left_line_num: None,
                right_line_num: Some(j),
            });
            j -= 1;
        } else {
            diff.push(SftpDiffLine {
                kind: SftpDiffLineKind::Removed,
                content: left_lines[i - 1].to_string(),
                left_line_num: Some(i),
                right_line_num: None,
            });
            i -= 1;
        }
    }

    diff.reverse();
    diff
}

fn sftp_diff_stats(lines: &[SftpDiffLine]) -> SftpDiffStats {
    let mut stats = SftpDiffStats::default();
    for line in lines {
        match line.kind {
            SftpDiffLineKind::Unchanged => stats.unchanged += 1,
            SftpDiffLineKind::Added => stats.added += 1,
            SftpDiffLineKind::Removed => stats.removed += 1,
        }
    }
    stats
}

#[derive(Clone, Debug)]
struct SftpPreviewVisualLine {
    line_number: Option<usize>,
    content: String,
}

#[derive(Clone, Debug)]
struct SftpDiffVisualLine {
    kind: SftpDiffLineKind,
    left_line_num: String,
    right_line_num: String,
    left_content: String,
    right_content: String,
}

fn sftp_preview_visual_lines(source: &str) -> Vec<SftpPreviewVisualLine> {
    source
        .split('\n')
        .enumerate()
        .flat_map(|(index, line)| {
            wrap_sftp_virtual_text_line(line, SFTP_PREVIEW_CODE_WRAP_COLUMNS)
                .into_iter()
                .enumerate()
                .map(move |(chunk_index, content)| SftpPreviewVisualLine {
                    line_number: (chunk_index == 0).then_some(index + 1),
                    content,
                })
        })
        .collect()
}

fn sftp_diff_visual_lines(lines: &[SftpDiffLine]) -> Vec<SftpDiffVisualLine> {
    let mut visual_lines = Vec::new();
    for line in lines {
        let removed = line.kind == SftpDiffLineKind::Removed;
        let added = line.kind == SftpDiffLineKind::Added;
        let left_content = if added {
            String::new()
        } else if removed {
            format!("- {}", line.content)
        } else {
            line.content.clone()
        };
        let right_content = if removed {
            String::new()
        } else if added {
            format!("+ {}", line.content)
        } else {
            line.content.clone()
        };
        let left_chunks = wrap_sftp_virtual_text_line(&left_content, SFTP_DIFF_WRAP_COLUMNS);
        let right_chunks = wrap_sftp_virtual_text_line(&right_content, SFTP_DIFF_WRAP_COLUMNS);
        let row_count = left_chunks.len().max(right_chunks.len()).max(1);

        for chunk_index in 0..row_count {
            visual_lines.push(SftpDiffVisualLine {
                kind: line.kind,
                left_line_num: if chunk_index == 0 {
                    line.left_line_num
                        .map(|number| number.to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                },
                right_line_num: if chunk_index == 0 {
                    line.right_line_num
                        .map(|number| number.to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                },
                left_content: left_chunks.get(chunk_index).cloned().unwrap_or_default(),
                right_content: right_chunks.get(chunk_index).cloned().unwrap_or_default(),
            });
        }
    }
    visual_lines
}

fn wrap_sftp_virtual_text_line(line: &str, max_columns: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }

    // Tauri uses CSS overflow for long `whitespace-pre` lines. GPUI's virtual
    // lists here have fixed row heights, so we pre-split by character columns
    // to keep long preview/diff lines readable without letting them bleed out
    // of the modal or forcing the UI tree to render every source line at once.
    let max_columns = max_columns.max(1);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut width = 0usize;
    for ch in line.chars() {
        if width >= max_columns {
            chunks.push(std::mem::take(&mut current));
            width = 0;
        }
        current.push(ch);
        width += 1;
    }
    chunks.push(current);
    chunks
}

fn sftp_file_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn sftp_breadcrumb_max_scroll(segments: &[PathSegment], viewport_width: f32, icon_size: f32) -> f32 {
    let content_width = sftp_breadcrumb_content_width(segments, icon_size);
    (content_width - viewport_width.max(0.0)).max(0.0)
}

fn sftp_breadcrumb_content_width(segments: &[PathSegment], icon_size: f32) -> f32 {
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let chevron = if index > 0 { icon_size + 2.0 } else { 0.0 };
            let root_icon = if index == 0 { icon_size + 4.0 } else { 0.0 };
            let label = (segment.name.chars().count() as f32 * 8.0).min(120.0);
            chevron + root_icon + label + 12.0
        })
        .sum()
}

fn sftp_path_bar_viewport_width(window: &Window) -> f32 {
    let viewport = f32::from(window.viewport_size().width);
    let pane_width = ((viewport - SFTP_ROOT_PADDING * 2.0 - SFTP_GAP) / 2.0).max(0.0);
    // Header title, toolbar icon buttons, gaps, path-bar padding and borders.
    // This mirrors the Tauri `PathBreadcrumb className="flex-1"` slot closely
    // enough for scroll clamping while the actual GPUI clipping still happens
    // in the rendered path bar.
    (pane_width - 260.0).max(80.0)
}

fn format_sftp_media_time(duration: std::time::Duration) -> String {
    let total = duration.as_secs();
    let minutes = total / 60;
    let seconds = total % 60;
    format!("{minutes}:{seconds:02}")
}

fn diff_cell(
    number: &str,
    content: &str,
    highlighted: bool,
    border: u32,
    left: bool,
) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .border_r_1()
        .border_color(rgb(border))
        .bg(if highlighted {
            if left {
                rgba((0x7f1d1d << 8) | SFTP_DIFF_LINE_BG_ALPHA)
            } else {
                rgba((0x14532d << 8) | SFTP_DIFF_LINE_BG_ALPHA)
            }
        } else {
            rgba(0x00000000)
        })
        .child(
            div()
                .w(px(SFTP_DIFF_LINE_NUMBER_COL))
                .flex_none()
                .px(px(8.0))
                .py(px(2.0))
                .text_align(gpui::TextAlign::Right)
                .text_color(if highlighted {
                    if left { rgb(SFTP_RED) } else { rgb(SFTP_GREEN) }
                } else {
                    rgb(0xa1a1aa)
                })
                .border_r_1()
                .border_color(rgb(border))
                .child(number.to_string()),
        )
        .child(
            div()
                .flex_1()
                .px(px(8.0))
                .py(px(2.0))
                .child(content.to_string()),
        )
        .into_any_element()
}

#[cfg(test)]
mod sftp_helper_tests {
    use super::*;

    #[test]
    fn modified_date_matches_tauri_seconds_contract() {
        assert_eq!(format_modified(None), "-");
        assert_eq!(format_modified(Some(0)), "-");

        let rendered = format_modified(Some(1_700_000_000));
        assert_ne!(rendered, "-");
        assert_ne!(rendered, "2026/5/7");
        assert!(rendered.contains('/'));
    }
}
