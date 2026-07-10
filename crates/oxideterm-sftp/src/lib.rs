// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Real SFTP protocol/session layer for native OxideTerm.
//!
//! The SSH crate owns node connections; this crate owns SFTP protocol state and
//! transfer semantics. Keeping that boundary explicit mirrors the Tauri backend
//! where SFTP is acquired from a node connection rather than from terminal UI.

mod archive;
mod conflict;
mod error;
mod path_utils;
mod progress;
mod retry;
mod session;
mod tar_transfer;
mod text_diff;
mod transfer_manager;
mod types;

pub use archive::{
    ArchiveExtractionError, ArchiveExtractionPlan, ArchiveKind, archive_kind,
    plan_archive_extraction, shell_quote,
};
pub use conflict::{
    ConflictTarget, ConflictTransfer, TransferConflict, find_transfer_conflicts,
    source_not_newer_than_target,
};
pub use error::SftpError;
pub use path_utils::{
    join_remote_path, normalize_remote_path, remote_directory_prefixes, remote_parent_path,
    unique_conflict_name,
};
pub use progress::{
    DummyProgressStore, LazyProgressStore, ProgressStore, RedbProgressStore,
    StoredTransferProgress, TransferStatus, TransferStrategy, TransferType,
};
pub use retry::{
    RetryConfig, calculate_backoff, error_is_auth_failure, error_is_connection_unavailable,
    error_is_not_found, error_is_permission_denied, error_should_retry_initialization,
    is_retryable_error,
};
pub use session::{SftpChannelOpener, SftpSession, WriteContentResult};
pub use tar_transfer::{
    SftpExecChannelOpener, TarCompression, probe_tar_compression, probe_tar_support,
    tar_download_directory, tar_upload_directory,
};
pub use text_diff::{
    TextDiffLine, TextDiffLineKind, TextDiffStats, compute_text_diff, text_diff_stats,
};
pub use transfer_manager::{
    BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, DEFAULT_SFTP_CONCURRENT_TRANSFERS, DEFAULT_SFTP_DIRECTORY_PARALLELISM,
    MAX_SFTP_CONCURRENT_TRANSFERS, MAX_SFTP_DIRECTORY_PARALLELISM, SftpTransferControl,
    SftpTransferGuard, SftpTransferManager, SftpTransferPermit, SftpTransferRuntimeSettings,
    SftpTransferStats,
};
pub use types::{
    AssetFileKind, FileInfo, FileType, ListFilter, PreviewContent, SortOrder, TransferDirection,
    TransferProgress, TransferState, encode_to_encoding,
};
