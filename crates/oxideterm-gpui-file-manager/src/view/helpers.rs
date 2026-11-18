use super::*;

pub(super) use oxideterm_local_files::{
    ArchiveEntryOutcome, bookmark_name_for_path, calculate_local_checksum, can_extract_archive,
    compress_local_files, copy_recursively_with_progress, default_file_manager_bookmarks_path,
    directory_stats, extract_local_archive, home_path, join_local_path, list_local_files,
    local_drives, local_operation_unit_count, local_parent_path,
    local_path_segments as file_manager_path_segments, local_preview_metadata,
    local_sidebar_locations, new_file_manager_bookmark_id, normalize_local_path, now_ms,
    read_local_preview, read_local_preview_range, sorted_local_files, unique_copy_path,
    validate_local_name, would_move_directory_into_itself,
};

pub(super) fn file_icon_for_entry(
    entry: &LocalFileEntry,
) -> oxideterm_gpui_ui::file_icons::FileIcon {
    let icon = if entry.file_type == LocalFileType::Directory {
        oxideterm_gpui_ui::file_icons::folder_icon(&entry.name, false)
    } else {
        oxideterm_gpui_ui::file_icons::file_icon(&entry.name)
    };
    icon.with_symlink(entry.file_type == LocalFileType::Symlink || entry.symlink_target.is_some())
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
