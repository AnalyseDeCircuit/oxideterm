// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashSet, VecDeque},
    fmt,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use futures_util::stream::{self, StreamExt, TryStreamExt};
use russh_sftp::{
    client::{SftpSession as RusshSftpSession, error::Error as SftpErrorInner},
    protocol::{FileAttributes, OpenFlags},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tracing::{debug, info, warn};

use super::{
    error::SftpError,
    path_utils::{is_absolute_remote_path, join_local_path, join_remote_path},
    types::{
        AdaptiveChunkSizer, AssetFileKind, FileInfo, FileType, ListFilter, PreviewContent,
        SortOrder, TransferDirection, TransferProgress, TransferState, constants,
        detect_and_decode, extension_to_language, font_mime_type, generate_hex_dump,
        is_font_extension, is_likely_text_content, is_office_extension, is_text_extension,
    },
};
use crate::{
    ProgressStore, SftpTransferGuard, SftpTransferManager, StoredTransferProgress, TransferType,
};

pub trait SftpChannelOpener: Clone + Send + Sync + 'static {
    fn open_sftp_channel(
        &self,
    ) -> impl Future<Output = Result<russh::Channel<russh::client::Msg>, SftpError>> + Send;
}

pub struct WriteContentResult {
    pub atomic_write: bool,
}

pub struct SftpSession {
    sftp: RusshSftpSession,
    session_id: String,
    cwd: String,
}

#[derive(Clone)]
struct DownloadFileJob {
    remote_path: String,
    local_path: String,
    total_bytes: u64,
}

#[derive(Clone)]
struct UploadFileJob {
    local_path: String,
    remote_path: String,
    total_bytes: u64,
}

impl fmt::Debug for SftpSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SftpSession")
            .field("session_id", &self.session_id)
            .field("cwd", &self.cwd)
            .finish_non_exhaustive()
    }
}

include!("session/basic.rs");
include!("session/preview.rs");
include!("session/file_ops.rs");
include!("session/transfers.rs");
include!("session/preview_helpers.rs");
include!("session/helpers.rs");
