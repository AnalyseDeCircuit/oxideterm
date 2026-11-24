mod archive;
mod bookmarks;
mod listing;
mod model;
mod path_completion;
mod paths;
mod preview_format;
mod preview_stream;

pub use archive::{ArchiveEntryOutcome, can_extract_archive};
pub use bookmarks::{
    BOOKMARKS_FILENAME, bookmark_name_for_path, new_file_manager_bookmark_id, now_ms,
};
pub use listing::{local_file_default_cmp, sorted_local_files};
pub use model::{
    LocalArchiveEntry, LocalArchiveInfo, LocalBatchProgress, LocalBookmark, LocalChecksumResult,
    LocalClipboard, LocalClipboardMode, LocalDrive, LocalFileEntry, LocalFileProperties,
    LocalFileType, LocalPathSegment, LocalPreview, LocalPreviewChunk, LocalPreviewMetadata,
    LocalSidebarLocation, LocalSidebarLocationKind, LocalSortDirection, LocalSortField,
};
pub use path_completion::{
    PathCompletionCandidate, PathCompletionRequest, local_path_completion_request,
    split_path_completion_request,
};
pub use paths::{
    join_local_path, local_parent_path, local_path_segments, normalize_local_path,
    validate_local_name,
};
pub use preview_format::{
    MAX_PREVIEW_SIZE, STREAM_PREVIEW_THRESHOLD, local_file_extension, mime_type_for_extension,
};
pub use preview_stream::{append_local_preview_chunk, append_local_preview_text};

#[cfg(not(target_family = "wasm"))]
mod batch;
#[cfg(not(target_family = "wasm"))]
mod checksum;
#[cfg(not(target_family = "wasm"))]
mod drives;
#[cfg(not(target_family = "wasm"))]
mod external;
#[cfg(not(target_family = "wasm"))]
mod operations;
#[cfg(not(target_family = "wasm"))]
mod preview;
#[cfg(not(target_family = "wasm"))]
mod properties;

#[cfg(not(target_family = "wasm"))]
pub use archive::{compress_local_files, extract_local_archive, list_local_archive_contents};
#[cfg(not(target_family = "wasm"))]
pub use batch::{
    compress_local_files_audited, copy_dropped_local_files, duplicate_local_files,
    extract_local_archive_audited, paste_local_files,
};
#[cfg(not(target_family = "wasm"))]
pub use bookmarks::{
    default_file_manager_bookmarks_path, load_local_bookmarks, reorder_local_bookmarks,
    save_local_bookmarks,
};
#[cfg(not(target_family = "wasm"))]
pub use checksum::calculate_local_checksum;
#[cfg(not(target_family = "wasm"))]
pub use drives::{directory_stats, local_drives};
#[cfg(not(target_family = "wasm"))]
pub use external::{
    local_file_audit_context, local_file_operation, open_path_external, reveal_path_external,
};
#[cfg(not(target_family = "wasm"))]
pub use listing::list_local_files;
#[cfg(not(target_family = "wasm"))]
pub use operations::{
    copy_recursively, copy_recursively_with_progress, create_local_file, create_local_folder,
    delete_local_entries, local_operation_unit_count, rename_local_entry,
};
#[cfg(not(target_family = "wasm"))]
pub use paths::{
    default_download_path, home_path, local_sidebar_locations, unique_copy_path,
    would_move_directory_into_itself,
};
#[cfg(not(target_family = "wasm"))]
pub use preview::{
    local_inspection_preview, local_preview_metadata, read_local_preview, read_local_preview_range,
};
#[cfg(not(target_family = "wasm"))]
pub use properties::local_file_properties;
