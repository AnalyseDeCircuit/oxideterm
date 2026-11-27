// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Portable file and transfer models with native SFTP session and transfer workers.
//!
//! The SSH crate owns node connections; this crate owns SFTP protocol state and
//! transfer semantics. Keeping that boundary explicit mirrors the Tauri backend
//! where SFTP is acquired from a node connection rather than from terminal UI.

#[cfg(not(target_family = "wasm"))]
mod archive;
mod background_transfer;
pub mod bookmarks;
mod browser_entries;
pub mod browser_files;
#[cfg(not(target_family = "wasm"))]
mod browser_io;
mod browser_queue;
mod browser_relay;
mod conflict;
mod directory_progress;
mod error;
#[cfg(not(target_family = "wasm"))]
mod ftp_runtime;
#[cfg(not(target_family = "wasm"))]
mod owned_directory_transfer;
mod path_utils;
#[cfg(not(target_family = "wasm"))]
mod progress;
// Use the wire codec for labels without linking the native transport on the web.
#[cfg(target_family = "wasm")]
#[path = "../../russh-sftp/src/path/label.rs"]
mod remote_label;
mod remote_load;
mod retry;
#[cfg(not(target_family = "wasm"))]
mod scp;
#[cfg(not(target_family = "wasm"))]
mod session;
#[cfg(not(target_family = "wasm"))]
mod ssh_transfer;
#[cfg(not(target_family = "wasm"))]
mod tar_transfer;
mod text_diff;
#[cfg(not(target_family = "wasm"))]
mod transfer_manager;
mod transfer_progress;
mod types;

#[cfg(not(target_family = "wasm"))]
pub use archive::{
    ArchiveExtractionError, ArchiveExtractionPlan, ArchiveKind, archive_kind,
    plan_archive_extraction, shell_quote,
};
pub use background_transfer::{
    BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, DEFAULT_SFTP_CONCURRENT_TRANSFERS, DEFAULT_SFTP_DIRECTORY_PARALLELISM,
    MAX_SFTP_CONCURRENT_TRANSFERS, MAX_SFTP_DIRECTORY_PARALLELISM, SftpTransferRuntimeSettings,
    SftpTransferShutdownReport, SftpTransferStats,
};
pub use bookmarks::SftpPathBookmark;
pub use browser_entries::{
    RemoteSftpListing, SftpFileEntry, SftpFileType, SftpPreviewSaveResult, SftpRemoteId,
    encode_preview_text, remote_listing_from_file_infos,
};
#[cfg(not(target_family = "wasm"))]
pub use browser_io::{
    audit_browser_listing, load_sftp_listing, load_sftp_preview, write_sftp_preview,
};
pub use browser_queue::{
    BrowserTransferEvent, ResumedTransferPaths, SftpTransferItem, SftpTransferState,
    apply_tauri_transfer_completion, apply_tauri_transfer_progress, format_file_size,
    format_transfer_speed,
};
pub use browser_relay::SftpRelayRequest;
#[cfg(not(target_family = "wasm"))]
pub use browser_relay::{discard_sftp_relay_progress, run_sftp_relay};
pub use conflict::{
    ConflictTarget, ConflictTransfer, TransferConflict, find_transfer_conflicts,
    source_not_newer_than_target,
};
pub use directory_progress::DirectoryProgressAccumulator;
pub use error::SftpError;
#[cfg(not(target_family = "wasm"))]
pub use ftp_runtime::{FtpRuntime, FtpTransferRequest, MutationSession, run_ftp_transfer};
pub use oxideterm_topology::NodeId;
pub use path_utils::local_download_name;
pub use path_utils::{
    is_absolute_remote_path, join_remote_path, normalize_remote_path, remote_directory_prefixes,
    remote_parent_path, unique_conflict_name, validate_remote_entry_name,
};
#[cfg(not(target_family = "wasm"))]
pub use progress::{
    DummyProgressStore, LazyProgressStore, ProgressStore, RedbProgressStore,
    list_incomplete_browser_transfers, update_stored_transfer_progress,
};
#[cfg(target_family = "wasm")]
pub use remote_label::display as display_remote_path;
pub use remote_load::{SftpRemoteLoadState, remote_list_result_is_superseded};
pub use retry::{
    RetryConfig, calculate_backoff, error_is_auth_failure, error_is_connection_unavailable,
    error_is_not_found, error_is_permission_denied, error_should_retry_initialization,
    is_retryable_error,
};
#[cfg(not(target_family = "wasm"))]
pub use russh_sftp::path::display as display_remote_path;
#[cfg(not(target_family = "wasm"))]
pub use scp::{
    ScpCapabilities, ScpTransferResult, probe_scp_capabilities, probe_scp_support,
    scp_download_directory, scp_download_file, scp_upload_directory, scp_upload_file,
};
#[cfg(not(target_family = "wasm"))]
pub use session::{
    CLIPBOARD_IMAGE_DIR_SEGMENTS, CLIPBOARD_IMAGE_MAX_BYTES, SftpChannelOpener, SftpSession,
    WriteContentResult, content_version, format_clipboard_image_filename, format_posix_shell_path,
    sanitize_clipboard_image_extension,
};
#[cfg(not(target_family = "wasm"))]
pub use ssh_transfer::{
    SshTransferError, SshTransferPreference, SshTransferRequest, run_ssh_transfer,
};
#[cfg(not(target_family = "wasm"))]
pub use tar_transfer::{
    SftpExecChannelOpener, TarCapabilities, TarCompression, TarDirectoryProfile,
    TarTransferOptions, TarTransferResult, probe_tar_capabilities, probe_tar_compression,
    probe_tar_support, profile_local_directory, tar_download_directory, tar_upload_directory,
};
pub use text_diff::{
    TextDiffLine, TextDiffLineKind, TextDiffStats, compute_text_diff, text_diff_stats,
};
#[cfg(not(target_family = "wasm"))]
pub use transfer_manager::{
    SftpTransferControl, SftpTransferGuard, SftpTransferManager, SftpTransferPermit,
};
pub use transfer_progress::{
    RemoteRelayProgressContext, StoredRemoteRelayProgress, StoredTransferProgress,
    TransferProtocol, TransferStatus, TransferStrategy, TransferType,
};
pub use types::{
    AssetFileKind, FileInfo, FileType, ListFilter, LocalDownloadDisposition, PreviewContent,
    RemoteRelayDisposition, SortOrder, TransferDirection, TransferProgress, TransferState,
    encode_to_encoding, is_office_extension, is_text_extension,
};
