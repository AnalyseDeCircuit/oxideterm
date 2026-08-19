use super::*;

pub(in crate::workspace::sftp) use oxideterm_sftp::{
    join_remote_path as join_sftp_path, normalize_remote_path, remote_directory_prefixes,
};

pub(in crate::workspace::sftp) fn sftp_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, SFTP_BG_ACTIVE_BG_ALPHA)
}

pub(in crate::workspace::sftp) fn sftp_panel_bg(
    color: u32,
    has_background: bool,
    alpha: u32,
) -> Rgba {
    color_with_background_scaled_alpha(color, has_background, alpha, SFTP_BG_ACTIVE_PANEL_ALPHA)
}

pub(in crate::workspace::sftp) fn sftp_card_surface(surface: gpui::Div, color: u32) -> gpui::Div {
    // SFTP queue subcards mirror Tauri bg-theme-bg-card; the shadow belongs to
    // that card token even when the caller keeps a custom active-background alpha.
    tauri_glass_surface_shadow(surface, color)
}

pub(in crate::workspace::sftp) fn sftp_hover_bg(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, SFTP_BG_ACTIVE_HOVER_ALPHA)
}

pub(in crate::workspace::sftp) fn sftp_border(color: u32, has_background: bool) -> Rgba {
    color_for_background(color, has_background, 0x99)
}

pub(in crate::workspace::sftp) fn is_sftp_incomplete_store_compat_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("deserialize")
        || error.contains("invalid type")
        || error.contains("connection_not_found")
        || error.contains("notfound")
        || error.contains("not found")
}

pub(in crate::workspace::sftp) fn home_path() -> String {
    oxideterm_local_files::home_path()
}

pub(in crate::workspace::sftp) fn default_download_path() -> String {
    oxideterm_local_files::default_download_path()
}

pub(in crate::workspace::sftp) fn list_local_files(
    path: &str,
) -> std::io::Result<Vec<SftpFileEntry>> {
    oxideterm_local_files::list_local_files(&oxideterm_local_files::normalize_local_path(path))
        .map(|files| files.into_iter().map(sftp_file_entry_from_local).collect())
}

pub(in crate::workspace::sftp) fn refreshed_local_files(path: &str) -> Vec<SftpFileEntry> {
    // Keep navigation and explicit refresh failures visible in the file pane.
    list_local_files(path).unwrap_or_else(|error| {
        vec![sftp_file_entry(
            format!("Unable to read folder: {error}"),
            path.to_string(),
            SftpFileType::File,
            0,
            None,
        )]
    })
}

pub(in crate::workspace::sftp) fn local_drives() -> Vec<SftpDrive> {
    oxideterm_local_files::local_drives()
        .into_iter()
        .map(sftp_drive_from_local)
        .collect()
}

fn sftp_file_entry_from_local(entry: oxideterm_local_files::LocalFileEntry) -> SftpFileEntry {
    let is_symlink = entry.file_type == oxideterm_local_files::LocalFileType::Symlink;
    SftpFileEntry {
        name: entry.name,
        path: entry.path,
        file_type: match entry.file_type {
            oxideterm_local_files::LocalFileType::Directory => SftpFileType::Directory,
            oxideterm_local_files::LocalFileType::File
            | oxideterm_local_files::LocalFileType::Symlink => SftpFileType::File,
        },
        size: entry.size,
        modified: entry.modified,
        permissions: None,
        owner: None,
        group: None,
        is_symlink,
        symlink_target: entry.symlink_target,
    }
}

fn sftp_drive_from_local(drive: oxideterm_local_files::LocalDrive) -> SftpDrive {
    SftpDrive {
        name: drive.name,
        path: drive.path,
        drive_type: drive.drive_type,
        total_space: drive.total_space,
        available_space: drive.available_space,
        read_only: drive.read_only,
    }
}

pub(in crate::workspace::sftp) fn sftp_file_entry(
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

pub(in crate::workspace::sftp) fn sorted_sftp_files(
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

pub(in crate::workspace::sftp) fn sftp_path_segments(
    path: &str,
    is_remote: bool,
) -> Vec<oxideterm_local_files::LocalPathSegment> {
    if !is_remote {
        return oxideterm_local_files::local_path_segments(path);
    }

    let normalized = normalize_remote_path(path);
    let mut segments = vec![oxideterm_local_files::LocalPathSegment {
        name: "/".to_string(),
        full_path: "/".to_string(),
        root_is_drive: false,
    }];
    let without_root = normalized.trim_start_matches('/');
    let mut current = String::from("/");
    for part in without_root.split('/').filter(|part| !part.is_empty()) {
        current = if current == "/" {
            format!("/{part}")
        } else {
            format!("{current}/{part}")
        };
        segments.push(oxideterm_local_files::LocalPathSegment {
            name: part.to_string(),
            full_path: current.clone(),
            root_is_drive: false,
        });
    }
    segments
}

pub(in crate::workspace::sftp) fn parent_path(path: &str, remote: bool) -> String {
    if remote {
        return oxideterm_sftp::remote_parent_path(path);
    }
    oxideterm_local_files::local_parent_path(path)
        .unwrap_or_else(|| oxideterm_local_files::normalize_local_path(path))
}

pub(in crate::workspace::sftp) fn join_local_path(base: &str, name: &str) -> String {
    oxideterm_local_files::join_local_path(base, name)
}

pub(in crate::workspace::sftp) fn normalize_external_dropped_path(
    path: &std::path::Path,
) -> Option<std::path::PathBuf> {
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
        return raw
            .chars()
            .next()
            .map(|root| std::path::PathBuf::from(root.to_string()));
    }
    Some(std::path::PathBuf::from(raw.trim_end_matches(['/', '\\'])))
}

pub(in crate::workspace::sftp) fn new_sftp_transfer_id(
    remote_id: &SftpRemoteId,
    name: &str,
) -> String {
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let safe_name = name.replace(['/', '\\', ':'], "_");
    format!("{}-{timestamp_ms}-{safe_name}", remote_id.storage_key())
}

pub(in crate::workspace::sftp) fn unique_sftp_conflict_name(
    name: &str,
    existing_files: &[SftpFileEntry],
) -> String {
    oxideterm_sftp::unique_conflict_name(name, existing_files.iter().map(|file| file.name.as_str()))
}

pub(in crate::workspace::sftp) fn sftp_conflict_resolution_from_settings(
    action: oxideterm_settings::ConflictAction,
) -> SftpConflictResolution {
    match action {
        oxideterm_settings::ConflictAction::Ask | oxideterm_settings::ConflictAction::Overwrite => {
            SftpConflictResolution::Overwrite
        }
        oxideterm_settings::ConflictAction::Skip => SftpConflictResolution::Skip,
        oxideterm_settings::ConflictAction::Rename => SftpConflictResolution::Rename,
    }
}

pub(in crate::workspace::sftp) fn sftp_transfer_conflicts(
    pending_transfers: &[SftpPendingTransfer],
    target_files: &[SftpFileEntry],
) -> Vec<SftpConflictInfo> {
    oxideterm_sftp::find_transfer_conflicts(
        pending_transfers
            .iter()
            .map(|transfer| oxideterm_sftp::ConflictTransfer {
                name: &transfer.name,
                source_size: transfer.source.size,
                source_modified: transfer.source.modified,
                source_is_directory: transfer.source.file_type == SftpFileType::Directory,
                direction: transfer.direction,
            }),
        target_files
            .iter()
            .map(|target| oxideterm_sftp::ConflictTarget {
                name: &target.name,
                size: target.size,
                modified: target.modified,
                is_directory: target.file_type == SftpFileType::Directory,
            }),
    )
}

pub(in crate::workspace::sftp) fn sftp_source_not_newer_than_target(
    transfer: &SftpPendingTransfer,
    target_files: &[SftpFileEntry],
) -> bool {
    oxideterm_sftp::source_not_newer_than_target(
        &transfer.name,
        transfer.source.modified,
        target_files
            .iter()
            .map(|target| oxideterm_sftp::ConflictTarget {
                name: &target.name,
                size: target.size,
                modified: target.modified,
                is_directory: target.file_type == SftpFileType::Directory,
            }),
    )
}

pub(in crate::workspace::sftp) fn sftp_transfer_state_from_background(
    state: BackgroundTransferState,
) -> SftpTransferState {
    match state {
        BackgroundTransferState::Pending => SftpTransferState::Pending,
        BackgroundTransferState::Active => SftpTransferState::Active,
        BackgroundTransferState::Paused => SftpTransferState::Paused,
        BackgroundTransferState::Completed => SftpTransferState::Completed,
        BackgroundTransferState::Cancelled => SftpTransferState::Cancelled,
        BackgroundTransferState::Error => SftpTransferState::Error,
    }
}

pub(in crate::workspace::sftp) fn preview_content_text(content: &PreviewContent) -> String {
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

pub(in crate::workspace::sftp) fn sftp_preview_is_markdown(
    language: Option<&str>,
    mime_type: Option<&str>,
) -> bool {
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

pub(in crate::workspace::sftp) fn sftp_editor_language(
    language: Option<&str>,
    name: &str,
) -> String {
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

pub(in crate::workspace::sftp) fn sftp_editor_language_id(
    language: Option<&str>,
    path: Option<&str>,
    name: &str,
    source: &str,
) -> Option<LanguageId> {
    // Prefer the remote path because the dialog title can be shortened while the path keeps the real extension.
    path.and_then(|path| LanguageId::detect(Some(Path::new(path)), source))
        .or_else(|| LanguageId::detect(Some(Path::new(name)), source))
        .or_else(|| language.and_then(sftp_editor_language_name_id))
}

fn sftp_editor_language_name_id(language: &str) -> Option<LanguageId> {
    match language.trim().to_ascii_lowercase().as_str() {
        "bash" | "sh" | "shell" => Some(LanguageId::Bash),
        "c" => Some(LanguageId::C),
        "cmake" => Some(LanguageId::CMake),
        "csharp" | "c#" | "cs" => Some(LanguageId::CSharp),
        "cpp" | "c++" | "cc" | "cxx" => Some(LanguageId::Cpp),
        "css" => Some(LanguageId::Css),
        "diff" | "patch" => Some(LanguageId::Diff),
        "dockerfile" | "containerfile" => Some(LanguageId::Dockerfile),
        "elixir" | "ex" | "exs" => Some(LanguageId::Elixir),
        "fish" => Some(LanguageId::Fish),
        "go" => Some(LanguageId::Go),
        "html" => Some(LanguageId::Html),
        "java" => Some(LanguageId::Java),
        "javascript" | "js" | "jsx" => Some(LanguageId::Javascript),
        "json" | "jsonc" => Some(LanguageId::Json),
        "lisp" | "commonlisp" => Some(LanguageId::Lisp),
        "lua" => Some(LanguageId::Lua),
        "make" | "makefile" => Some(LanguageId::Make),
        "markdown" | "md" | "mdx" => Some(LanguageId::Markdown),
        "objective-c" | "objectivec" | "objc" => Some(LanguageId::ObjectiveC),
        "perl" | "pl" => Some(LanguageId::Perl),
        "php" => Some(LanguageId::Php),
        "powershell" | "pwsh" | "ps1" => Some(LanguageId::Powershell),
        "python" | "py" => Some(LanguageId::Python),
        "r" => Some(LanguageId::R),
        "ruby" | "rb" => Some(LanguageId::Ruby),
        "rust" | "rs" => Some(LanguageId::Rust),
        "scala" => Some(LanguageId::Scala),
        "sql" => Some(LanguageId::Sql),
        "swift" => Some(LanguageId::Swift),
        "toml" => Some(LanguageId::Toml),
        "tsx" => Some(LanguageId::Tsx),
        "typescript" | "ts" => Some(LanguageId::TypeScript),
        "yaml" | "yml" => Some(LanguageId::Yaml),
        "zsh" => Some(LanguageId::Zsh),
        "zig" => Some(LanguageId::Zig),
        _ => None,
    }
}

pub(in crate::workspace::sftp) async fn load_remote_sftp_listing(
    backend: SftpRemoteBackend,
    path: &str,
) -> Result<RemoteSftpListing, String> {
    load_remote_sftp_listing_inner(backend, path, true).await
}

pub(in crate::workspace::sftp) async fn load_remote_sftp_completion_listing(
    backend: SftpRemoteBackend,
    path: &str,
) -> Result<RemoteSftpListing, String> {
    // Typeahead is observational and must not replace the visible SFTP cwd.
    load_remote_sftp_listing_inner(backend, path, false).await
}

async fn load_remote_sftp_listing_inner(
    backend: SftpRemoteBackend,
    path: &str,
    update_ready_path: bool,
) -> Result<RemoteSftpListing, String> {
    let transfer = backend
        .acquire_transfer_sftp()
        .await
        .map_err(|error| error.to_string())?;
    match list_remote_sftp_once(&transfer, path).await {
        Ok(listing) => {
            if update_ready_path && let SftpRemoteBackend::Node { router, node_id } = &backend {
                let connection = router
                    .resolve_connection(node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                router
                    .mark_sftp_ready_from_listing(
                        node_id,
                        connection.handle.connection_id(),
                        Some(listing.cwd.clone()),
                    )
                    .map_err(|error| error.to_string())?;
            }
            Ok(listing)
        }
        Err(error) if error.is_channel_recoverable() => {
            // Retry directory listing on a new transfer channel. The shared
            // SFTP owner is not part of this path, so a slow list cannot block
            // preview/save operations that already use their own channels.
            let transfer = backend
                .acquire_transfer_sftp()
                .await
                .map_err(|route_error| route_error.to_string())?;
            let listing = list_remote_sftp_once(&transfer, path)
                .await
                .map_err(|retry_error| retry_error.to_string())?;
            if update_ready_path && let SftpRemoteBackend::Node { router, node_id } = &backend {
                let connection = router
                    .resolve_connection(node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                router
                    .mark_sftp_ready_from_listing(
                        node_id,
                        connection.handle.connection_id(),
                        Some(listing.cwd.clone()),
                    )
                    .map_err(|error| error.to_string())?;
            }
            Ok(listing)
        }
        Err(error) => Err(error.to_string()),
    }
}

pub(in crate::workspace::sftp) async fn load_remote_sftp_preview(
    backend: SftpRemoteBackend,
    path: &str,
) -> Result<PreviewContent, String> {
    let sftp = backend
        .acquire_transfer_sftp()
        .await
        .map_err(|error| error.to_string())?;
    match load_remote_sftp_preview_once(&sftp, path).await {
        Ok(preview) => Ok(preview),
        Err(error) if error.is_channel_recoverable() => {
            // Preview can be slow and must not hold the shared directory-owner
            // SFTP mutex; retry once with a fresh short-lived SFTP channel.
            let sftp = backend
                .acquire_transfer_sftp()
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
    sftp: &SftpSession,
    path: &str,
) -> Result<PreviewContent, SftpError> {
    sftp.preview(path).await
}

pub(in crate::workspace::sftp) async fn load_remote_sftp_preview_hex(
    backend: SftpRemoteBackend,
    path: &str,
    offset: u64,
) -> Result<PreviewContent, String> {
    let sftp = backend
        .acquire_transfer_sftp()
        .await
        .map_err(|error| error.to_string())?;
    match load_remote_sftp_preview_hex_once(&sftp, path, offset).await {
        Ok(preview) => Ok(preview),
        Err(error) if error.is_channel_recoverable() => {
            // Hex preview uses its own channel for the same reason as text
            // preview: large reads should not block directory navigation.
            let sftp = backend
                .acquire_transfer_sftp()
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
    sftp: &SftpSession,
    path: &str,
    offset: u64,
) -> Result<PreviewContent, SftpError> {
    sftp.preview_with_offset(path, offset).await
}

pub(in crate::workspace::sftp) async fn save_remote_sftp_preview(
    backend: SftpRemoteBackend,
    path: &str,
    content: &str,
    encoding: &str,
    line_ending: TextLineEnding,
) -> Result<SftpPreviewSaveResult, String> {
    let target_encoding = if encoding.trim().is_empty() {
        "UTF-8"
    } else {
        encoding
    };
    let remote_content = restore_text_line_endings(content, line_ending);
    let encoded = encode_to_encoding(&remote_content, target_encoding);
    let sftp = backend
        .acquire_transfer_sftp()
        .await
        .map_err(|error| error.to_string())?;
    // Saving uses a short-lived SFTP channel so a large write/stat round trip
    // cannot stall the shared directory listing owner.
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

pub(in crate::workspace::sftp) fn sftp_preview_editor_is_network_error(error: &str) -> bool {
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
    sftp: &SftpSession,
    path: &str,
) -> Result<RemoteSftpListing, SftpError> {
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

pub(in crate::workspace::sftp) fn format_file_size(bytes: u64) -> String {
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

pub(in crate::workspace::sftp) fn format_transfer_speed(bytes_per_second: u64) -> String {
    if bytes_per_second == 0 {
        return "-".to_string();
    }
    format!("{}/s", format_file_size(bytes_per_second))
}

pub(in crate::workspace::sftp) fn format_modified(modified: Option<i64>) -> String {
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

pub(in crate::workspace::sftp) fn format_conflict_modified(modified: Option<i64>) -> String {
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

#[derive(Clone, Debug)]
pub(in crate::workspace::sftp) struct SftpDiffVisualLine {
    pub(super) kind: SftpDiffLineKind,
    pub(super) left_line_num: String,
    pub(super) right_line_num: String,
    pub(super) left_content: String,
    pub(super) right_content: String,
}

pub(in crate::workspace::sftp) fn sftp_diff_visual_lines(
    lines: &[SftpDiffLine],
) -> Vec<SftpDiffVisualLine> {
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

pub(in crate::workspace::sftp) fn sftp_file_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

pub(in crate::workspace::sftp) fn format_sftp_media_time(duration: std::time::Duration) -> String {
    let total = duration.as_secs();
    let minutes = total / 60;
    let seconds = total % 60;
    format!("{minutes}:{seconds:02}")
}

pub(in crate::workspace::sftp) fn diff_cell(
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
    fn refreshed_local_files_reads_the_directory_again() {
        let directory =
            std::env::temp_dir().join(format!("oxideterm-sftp-refresh-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("temporary directory should be created");
        let path = directory.to_string_lossy();

        let initial_files = refreshed_local_files(&path);
        std::fs::write(directory.join("country.mmdb"), b"test")
            .expect("fixture file should be created");
        let refreshed_files = refreshed_local_files(&path);

        assert!(!initial_files.iter().any(|file| file.name == "country.mmdb"));
        assert!(
            refreshed_files
                .iter()
                .any(|file| file.name == "country.mmdb")
        );

        // The generated UUID keeps cleanup scoped to this test's directory.
        std::fs::remove_dir_all(&directory).expect("temporary directory should be removed");
    }

    #[test]
    fn modified_date_matches_tauri_seconds_contract() {
        assert_eq!(format_modified(None), "-");
        assert_eq!(format_modified(Some(0)), "-");

        let rendered = format_modified(Some(1_700_000_000));
        assert_ne!(rendered, "-");
        assert_ne!(rendered, "2026/5/7");
        assert!(rendered.contains('/'));
    }

    #[test]
    fn local_navigation_preserves_windows_drive_roots() {
        let segments = sftp_path_segments(r"D:\Projects\OxideTerm", false);

        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.full_path.as_str())
                .collect::<Vec<_>>(),
            [r"D:\", r"D:\Projects", r"D:\Projects\OxideTerm"]
        );
        assert_eq!(parent_path(r"D:\Projects\OxideTerm", false), r"D:\Projects");
        assert_eq!(parent_path(r"D:\", false), r"D:\");
    }
}
