// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SFTP file management module
//!
//! Provides remote file browsing, upload, download, and preview functionality.

pub mod error;
pub mod path_utils;
pub mod progress;
pub mod retry;
pub mod session;
pub mod tar_transfer;
pub mod transfer;
pub mod types;

pub use error::SftpError;
pub use progress::{
    DummyProgressStore, LazyProgressStore, ProgressStore, RedbProgressStore,
    StoredTransferProgress, TransferStatus, TransferType,
};
pub use retry::{RetryConfig, calculate_backoff, is_retryable_error, transfer_with_retry};
pub use session::{ResumeContext, SftpSession};
pub use transfer::{TransferControl, TransferGuard, TransferManager, check_transfer_control};
pub use types::*;
