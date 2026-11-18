mod archive;
mod bookmarks;
mod checksum;
mod drives;
mod listing;
mod model;
mod operations;
mod paths;
mod preview;

pub use archive::{
    ArchiveEntryOutcome, can_extract_archive, compress_local_files, extract_local_archive,
    list_local_archive_contents,
};
pub use bookmarks::{
    BOOKMARKS_FILENAME, bookmark_name_for_path, default_file_manager_bookmarks_path,
    new_file_manager_bookmark_id, now_ms,
};
pub use checksum::calculate_local_checksum;
pub use drives::{directory_stats, local_drives};
pub use listing::{list_local_files, local_file_default_cmp, sorted_local_files};
pub use model::{
    LocalArchiveEntry, LocalArchiveInfo, LocalBookmark, LocalChecksumResult, LocalClipboardMode,
    LocalDrive, LocalFileEntry, LocalFileType, LocalPathSegment, LocalPreview, LocalPreviewChunk,
    LocalPreviewMetadata, LocalSidebarLocation, LocalSidebarLocationKind, LocalSortDirection,
    LocalSortField,
};
pub use operations::{
    copy_recursively, copy_recursively_with_progress, local_operation_unit_count,
};
pub use paths::{
    default_download_path, home_path, join_local_path, local_parent_path, local_path_segments,
    local_sidebar_locations, normalize_local_path, unique_copy_path, validate_local_name,
    would_move_directory_into_itself,
};
pub use preview::{
    MAX_PREVIEW_SIZE, STREAM_PREVIEW_THRESHOLD, local_file_extension, local_preview_metadata,
    mime_type_for_extension, read_local_preview, read_local_preview_range,
};
mod external;
mod path_completion;
mod preview_stream;
pub use external::{local_file_audit_context, open_path_external, reveal_path_external};
pub use path_completion::{
    PathCompletionCandidate, PathCompletionRequest, local_path_completion_request,
    split_path_completion_request,
};
pub use preview::local_inspection_preview;
pub use preview_stream::{append_local_preview_chunk, append_local_preview_text};

mod batch;
mod properties;
pub use batch::{
    LocalBatchProgress, compress_local_files_audited, copy_dropped_local_files,
    duplicate_local_files, extract_local_archive_audited, paste_local_files,
};
pub use properties::{LocalFileProperties, local_file_properties};
