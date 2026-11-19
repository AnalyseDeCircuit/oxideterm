// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Real SFTP protocol/session layer for native OxideTerm.
//!
//! The SSH crate owns node connections; this crate owns SFTP protocol state and
//! transfer semantics. Keeping that boundary explicit mirrors the Tauri backend
//! where SFTP is acquired from a node connection rather than from terminal UI.

mod archive;
pub mod bookmarks;
mod browser_entries;
pub mod browser_files;
mod browser_io;
mod browser_queue;
mod browser_relay;
mod conflict;
mod directory_progress;
mod error;
mod ftp_runtime;
mod owned_directory_transfer;
mod path_utils;
mod progress;
mod remote_load;
mod retry;
mod scp;
mod session;
mod ssh_transfer;
mod tar_transfer;
mod text_diff;
mod transfer_manager;
mod types;

pub use archive::{
    ArchiveExtractionError, ArchiveExtractionPlan, ArchiveKind, archive_kind,
    plan_archive_extraction, shell_quote,
};
pub use bookmarks::SftpPathBookmark;
pub use browser_entries::{RemoteSftpListing, SftpFileEntry, SftpFileType, SftpPreviewSaveResult};
pub use browser_io::{
    audit_browser_listing, encode_preview_text, load_sftp_listing, load_sftp_preview,
    write_sftp_preview,
};
pub use browser_queue::{
    ResumedTransferPaths, SftpTransferItem, SftpTransferState, apply_tauri_transfer_completion,
    apply_tauri_transfer_progress, format_file_size, format_transfer_speed,
};
pub use browser_relay::{SftpRelayRequest, discard_sftp_relay_progress, run_sftp_relay};
pub use conflict::{
    ConflictTarget, ConflictTransfer, TransferConflict, find_transfer_conflicts,
    source_not_newer_than_target,
};
pub use directory_progress::DirectoryProgressAccumulator;
pub use error::SftpError;
pub use ftp_runtime::{
    BrowserTransferEvent, FtpRuntime, FtpTransferRequest, MutationSession, run_ftp_transfer,
};
pub use path_utils::local_download_name;
pub use path_utils::{
    join_remote_path, normalize_remote_path, remote_directory_prefixes, remote_parent_path,
    unique_conflict_name,
};
pub use progress::{
    DummyProgressStore, LazyProgressStore, ProgressStore, RedbProgressStore,
    RemoteRelayProgressContext, StoredRemoteRelayProgress, StoredTransferProgress,
    TransferProtocol, TransferStatus, TransferStrategy, TransferType,
    list_incomplete_browser_transfers, update_stored_transfer_progress,
};
pub use remote_load::{SftpRemoteLoadState, remote_list_result_is_superseded};
pub use retry::{
    RetryConfig, calculate_backoff, error_is_auth_failure, error_is_connection_unavailable,
    error_is_not_found, error_is_permission_denied, error_should_retry_initialization,
    is_retryable_error,
};
pub use russh_sftp::path::display as display_remote_path;
pub use scp::{
    ScpCapabilities, ScpTransferResult, probe_scp_capabilities, probe_scp_support,
    scp_download_directory, scp_download_file, scp_upload_directory, scp_upload_file,
};
pub use session::{SftpChannelOpener, SftpSession, WriteContentResult, content_version};
pub use ssh_transfer::{
    SshTransferError, SshTransferPreference, SshTransferRequest, run_ssh_transfer,
};
pub use tar_transfer::{
    SftpExecChannelOpener, TarCapabilities, TarCompression, TarDirectoryProfile,
    TarTransferOptions, TarTransferResult, probe_tar_capabilities, probe_tar_compression,
    probe_tar_support, profile_local_directory, tar_download_directory, tar_upload_directory,
};
pub use text_diff::{
    TextDiffLine, TextDiffLineKind, TextDiffStats, compute_text_diff, text_diff_stats,
};
pub use transfer_manager::{
    BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, DEFAULT_SFTP_CONCURRENT_TRANSFERS, DEFAULT_SFTP_DIRECTORY_PARALLELISM,
    MAX_SFTP_CONCURRENT_TRANSFERS, MAX_SFTP_DIRECTORY_PARALLELISM, SftpTransferControl,
    SftpTransferGuard, SftpTransferManager, SftpTransferPermit, SftpTransferRuntimeSettings,
    SftpTransferShutdownReport, SftpTransferStats,
};
pub use types::{
    AssetFileKind, FileInfo, FileType, ListFilter, LocalDownloadDisposition, PreviewContent,
    RemoteRelayDisposition, SortOrder, TransferDirection, TransferProgress, TransferState,
    encode_to_encoding,
};
