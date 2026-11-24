// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{TransferProtocol, TransferStrategy};
use oxideterm_local_files::now_ms;
use serde::{Deserialize, Serialize};

pub const DEFAULT_SFTP_CONCURRENT_TRANSFERS: usize = 3;
pub const DEFAULT_SFTP_DIRECTORY_PARALLELISM: usize = 4;
pub const MAX_SFTP_CONCURRENT_TRANSFERS: usize = 10;
pub const MAX_SFTP_DIRECTORY_PARALLELISM: usize = 16;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTransferDirection {
    Upload,
    Download,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTransferKind {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTransferState {
    Pending,
    Active,
    Paused,
    Completed,
    Cancelled,
    Error,
}

impl BackgroundTransferState {
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTransferSnapshot {
    pub id: String,
    pub node_id: String,
    pub name: String,
    pub local_path: String,
    pub remote_path: String,
    pub direction: BackgroundTransferDirection,
    pub kind: BackgroundTransferKind,
    pub protocol: TransferProtocol,
    pub strategy: TransferStrategy,
    pub state: BackgroundTransferState,
    pub size: u64,
    pub transferred: u64,
    pub backend_speed: Option<u64>,
    pub error: Option<String>,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub item_count: Option<u64>,
}

impl BackgroundTransferSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        node_id: String,
        name: String,
        local_path: String,
        remote_path: String,
        direction: BackgroundTransferDirection,
        kind: BackgroundTransferKind,
        strategy: TransferStrategy,
        size: u64,
        transferred: u64,
    ) -> Self {
        Self {
            id,
            node_id,
            name,
            local_path,
            remote_path,
            direction,
            kind,
            protocol: TransferProtocol::Sftp,
            strategy,
            state: BackgroundTransferState::Pending,
            size,
            transferred,
            backend_speed: None,
            error: None,
            start_time: now_ms(),
            end_time: None,
            item_count: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SftpTransferRuntimeSettings {
    pub max_concurrent_transfers: usize,
    pub speed_limit_kbps: usize,
    pub directory_parallelism: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SftpTransferStats {
    pub active: usize,
    pub queued: usize,
    pub completed: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SftpTransferShutdownReport {
    pub started: bool,
    pub drained: bool,
    pub cancelled_transfers: usize,
    pub remaining_transfers: usize,
}

impl Default for SftpTransferRuntimeSettings {
    fn default() -> Self {
        Self {
            max_concurrent_transfers: DEFAULT_SFTP_CONCURRENT_TRANSFERS,
            speed_limit_kbps: 0,
            directory_parallelism: DEFAULT_SFTP_DIRECTORY_PARALLELISM,
        }
    }
}
