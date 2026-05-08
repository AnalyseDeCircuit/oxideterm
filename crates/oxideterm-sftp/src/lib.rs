// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Real SFTP protocol/session layer for native OxideTerm.
//!
//! The SSH crate owns node connections; this crate owns SFTP protocol state and
//! transfer semantics. Keeping that boundary explicit mirrors the Tauri backend
//! where SFTP is acquired from a node connection rather than from terminal UI.

mod error;
mod path_utils;
mod progress;
mod retry;
mod session;
mod tar_transfer;
mod transfer_manager;
mod types;

pub use error::SftpError;
pub use progress::{
    DummyProgressStore, ProgressStore, RedbProgressStore, StoredTransferProgress, TransferStatus,
    TransferStrategy, TransferType,
};
pub use retry::{RetryConfig, calculate_backoff, is_retryable_error};
pub use session::{SftpChannelOpener, SftpSession, WriteContentResult};
pub use tar_transfer::{
    SftpExecChannelOpener, TarCompression, probe_tar_compression, probe_tar_support,
    tar_download_directory, tar_upload_directory,
};
pub use transfer_manager::{
    BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, DEFAULT_SFTP_CONCURRENT_TRANSFERS, DEFAULT_SFTP_DIRECTORY_PARALLELISM,
    MAX_SFTP_CONCURRENT_TRANSFERS, MAX_SFTP_DIRECTORY_PARALLELISM, SftpTransferControl,
    SftpTransferGuard, SftpTransferManager, SftpTransferPermit, SftpTransferRuntimeSettings,
};
pub use types::{
    AssetFileKind, FileInfo, FileType, ListFilter, PreviewContent, SortOrder, TransferDirection,
    TransferProgress, TransferState, encode_to_encoding,
};
