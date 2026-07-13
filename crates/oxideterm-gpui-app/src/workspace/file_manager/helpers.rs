use super::*;

pub(super) use oxideterm_local_files::{
    bookmark_name_for_path, calculate_local_checksum, can_extract_archive, compress_local_files,
    copy_recursively_with_progress, default_file_manager_bookmarks_path, directory_stats,
    extract_local_archive, home_path, join_local_path, list_local_files, local_drives,
    local_operation_unit_count, local_parent_path, local_preview_metadata, local_sidebar_locations,
    new_file_manager_bookmark_id, normalize_local_path, now_ms, read_local_preview,
    read_local_preview_range, sorted_local_files, unique_copy_path, validate_local_name,
    would_move_directory_into_itself,
};

#[derive(Clone, Debug)]
pub(super) struct FileManagerPathSegment {
    pub(super) name: String,
    pub(super) full_path: String,
    pub(super) root_is_drive: bool,
}

pub(super) fn file_manager_path_segments(path: &str) -> Vec<FileManagerPathSegment> {
    let normalized = normalize_local_path(path);
    if let Some(segments) = windows_path_segments(&normalized) {
        return segments;
    }

    let unix_path = normalized.replace('\\', "/");
    let mut segments = Vec::new();
    segments.push(FileManagerPathSegment {
        name: "/".to_string(),
        full_path: "/".to_string(),
        root_is_drive: false,
    });
    let mut current = String::from("/");
    for part in unix_path
        .trim_start_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
    {
        current = if current == "/" {
            format!("/{part}")
        } else {
            format!("{current}/{part}")
        };
        segments.push(FileManagerPathSegment {
            name: part.to_string(),
            full_path: current.clone(),
            root_is_drive: false,
        });
    }
    segments
}

fn windows_path_segments(path: &str) -> Option<Vec<FileManagerPathSegment>> {
    let normalized = path.replace('/', "\\");
    let bytes = normalized.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' {
        return None;
    }

    let drive = normalized[..2].to_string();
    let mut segments = Vec::new();
    let mut current = format!("{drive}\\");
    segments.push(FileManagerPathSegment {
        name: drive,
        full_path: current.clone(),
        root_is_drive: true,
    });
    for part in normalized[2..]
        .trim_start_matches('\\')
        .split('\\')
        .filter(|part| !part.is_empty())
    {
        if !current.ends_with('\\') {
            current.push('\\');
        }
        current.push_str(part);
        segments.push(FileManagerPathSegment {
            name: part.to_string(),
            full_path: current.clone(),
            root_is_drive: true,
        });
    }
    Some(segments)
}

pub(super) fn file_icon_for_entry(entry: &LocalFileEntry) -> (LucideIcon, u32) {
    if entry.file_type == LocalFileType::Directory {
        return (LucideIcon::Folder, FILE_MANAGER_BLUE);
    }
    if entry.file_type == LocalFileType::Symlink {
        return (LucideIcon::Link2, FILE_MANAGER_GREEN);
    }
    let ext = std::path::Path::new(&entry.name)
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "jar" | "war" | "ear"
        | "apk" | "xpi" | "crx" | "epub" => (LucideIcon::FileArchive, FILE_MANAGER_ORANGE),
        "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" | "wma" | "opus" => {
            (LucideIcon::FileAudio, FILE_MANAGER_PURPLE)
        }
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "ico" | "bmp" => {
            (LucideIcon::FileImage, FILE_MANAGER_GREEN)
        }
        "mp4" | "webm" | "ogv" | "mov" | "mkv" | "avi" | "m4v" => {
            (LucideIcon::FileVideo, FILE_MANAGER_PURPLE)
        }
        "json" => (LucideIcon::FileJson, FILE_MANAGER_ORANGE),
        "md" | "markdown" | "mdx" | "txt" | "log" | "ini" | "conf" | "cfg" | "env" => {
            (LucideIcon::FileText, FILE_MANAGER_BLUE)
        }
        "sh" | "bash" | "zsh" | "fish" | "ps1" => (LucideIcon::FileTerminal, FILE_MANAGER_GREEN),
        "js" | "jsx" | "ts" | "tsx" | "py" | "rs" | "go" | "java" | "c" | "cpp" | "h" | "hpp"
        | "cs" | "rb" | "php" | "swift" | "kt" | "scala" | "sql" | "html" | "htm" | "css"
        | "scss" | "sass" | "less" | "yaml" | "yml" | "toml" | "xml" | "vue" | "svelte" => {
            (LucideIcon::FileCode, FILE_MANAGER_BLUE)
        }
        "xlsx" | "xls" | "ods" | "csv" => (LucideIcon::FileSpreadsheet, FILE_MANAGER_GREEN),
        "lock" => (LucideIcon::FileLock, FILE_MANAGER_ORANGE),
        _ => (LucideIcon::File, 0),
    }
}

pub(super) fn local_file_properties(entry: &LocalFileEntry) -> FileManagerProperties {
    let metadata = local_preview_metadata(&entry.path);
    let accessed = metadata.as_ref().and_then(|metadata| metadata.accessed);
    let created = metadata.as_ref().and_then(|metadata| metadata.created);
    let mode = metadata.as_ref().and_then(|metadata| metadata.mode);
    let mime_type = metadata
        .as_ref()
        .and_then(|metadata| metadata.mime_type.clone());
    let is_symlink = metadata
        .as_ref()
        .is_some_and(|metadata| metadata.is_symlink);
    let (dir_files, dir_dirs, total_size) = if entry.file_type == LocalFileType::Directory {
        let stats = directory_stats(std::path::Path::new(&entry.path));
        (Some(stats.0), Some(stats.1), Some(stats.2))
    } else {
        (None, None, None)
    };
    let location = std::path::Path::new(&entry.path)
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .unwrap_or_default();
    let kind_label = match entry.file_type {
        LocalFileType::Directory => "fileManager.propTypeFolder",
        LocalFileType::Symlink => "fileManager.propTypeSymlink",
        LocalFileType::File => "fileManager.propTypeFile",
    }
    .to_string();
    FileManagerProperties {
        kind_label,
        location,
        size: entry.size,
        modified: entry.modified,
        accessed,
        readonly: entry.readonly,
        dir_files,
        dir_dirs,
        total_size,
        created,
        mode,
        mime_type,
        is_symlink,
    }
}

pub(super) fn format_file_size(bytes: u64) -> String {
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

pub(super) fn format_modified(modified: Option<i64>) -> String {
    let Some(modified) = modified.filter(|modified| *modified > 0) else {
        return "-".to_string();
    };
    let Some(datetime) = chrono::DateTime::from_timestamp(modified, 0) else {
        return "-".to_string();
    };
    datetime
        .with_timezone(&chrono::Local)
        .format("%Y/%-m/%-d")
        .to_string()
}
