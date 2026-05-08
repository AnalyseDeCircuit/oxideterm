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

fn home_path_mock() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/Users/lipsc".to_string())
}

fn list_local_files(path: &str) -> std::io::Result<Vec<SftpFileEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = std::fs::symlink_metadata(entry.path())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let full_path = entry.path().to_string_lossy().to_string();
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
            symlink_target: std::fs::read_link(entry.path())
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

fn mock_drives() -> Vec<SftpDrive> {
    vec![
        SftpDrive {
            name: "Macintosh HD".to_string(),
            path: "/".to_string(),
            drive_type: "system",
            total_space: 512 * 1024 * 1024 * 1024,
            available_space: 128 * 1024 * 1024 * 1024,
            read_only: false,
        },
        SftpDrive {
            name: "Network Share".to_string(),
            path: "/Volumes/share".to_string(),
            drive_type: "network",
            total_space: 1024 * 1024 * 1024 * 1024,
            available_space: 620 * 1024 * 1024 * 1024,
            read_only: false,
        },
    ]
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
    let sftp = sftp.lock().await;
    sftp.preview(path).await.map_err(|error| error.to_string())
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
    let sftp = sftp.lock().await;
    sftp.preview_with_offset(path, offset)
        .await
        .map_err(|error| error.to_string())
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

async fn load_local_sftp_preview(path: &str) -> Result<PreviewContent, String> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| error.to_string())?;
    let size = metadata.len();
    const MAX_LOCAL_TEXT_PREVIEW: u64 = 2 * 1024 * 1024;
    if size > MAX_LOCAL_TEXT_PREVIEW {
        return Ok(PreviewContent::TooLarge {
            size,
            max_size: MAX_LOCAL_TEXT_PREVIEW,
            recommend_download: false,
        });
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| error.to_string())?;
    match String::from_utf8(bytes.clone()) {
        Ok(data) => Ok(PreviewContent::Text {
            data,
            mime_type: None,
            language: None,
            encoding: "UTF-8".to_string(),
            confidence: 1.0,
            has_bom: false,
        }),
        Err(_) => Ok(PreviewContent::Hex {
            data: local_hex_dump(&bytes, 0),
            total_size: size,
            offset: 0,
            chunk_size: bytes.len() as u64,
            has_more: false,
        }),
    }
}

fn local_hex_dump(data: &[u8], offset: u64) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for (index, chunk) in data.chunks(16).enumerate() {
        let address = offset + (index * 16) as u64;
        let _ = write!(output, "{address:08X}  ");
        for (column, byte) in chunk.iter().enumerate() {
            if column == 8 {
                output.push(' ');
            }
            let _ = write!(output, "{byte:02X} ");
        }
        output.push('\n');
    }
    output
}

async fn list_remote_sftp_once(
    sftp: &std::sync::Arc<tokio::sync::Mutex<SftpSession>>,
    path: &str,
) -> Result<RemoteSftpListing, SftpError> {
    let sftp = sftp.lock().await;
    let cwd = sftp.canonicalize(path).await?;
    let entries = sftp
        .list_dir(
            &cwd,
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

fn format_modified(modified: Option<i64>) -> String {
    if modified.is_some() {
        "2026/5/7".to_string()
    } else {
        "-".to_string()
    }
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

fn sftp_file_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
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
