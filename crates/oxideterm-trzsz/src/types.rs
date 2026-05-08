// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::Serialize;

use crate::MAX_TRANSFER_CHUNK_SIZE;

pub const TRZSZ_PROTOCOL_VERSION: &str = "1.1.6";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TrzszHandshakeMode {
    Send,
    Receive,
    Directory,
}

impl TrzszHandshakeMode {
    pub fn from_protocol_char(value: char) -> Option<Self> {
        match value {
            'S' => Some(Self::Send),
            'R' => Some(Self::Receive),
            'D' => Some(Self::Directory),
            _ => None,
        }
    }

    pub fn as_protocol_char(self) -> char {
        match self {
            Self::Send => 'S',
            Self::Receive => 'R',
            Self::Directory => 'D',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TrzszTransferDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TrzszTransferSelection {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrzszDetectedHandshake {
    pub mode: TrzszHandshakeMode,
    pub version: String,
    pub unique_id: String,
    pub remote_is_windows: bool,
    pub direction: TrzszTransferDirection,
    pub selection: TrzszTransferSelection,
}

impl TrzszDetectedHandshake {
    pub fn from_parts(
        mode: TrzszHandshakeMode,
        version: impl Into<String>,
        unique_id: impl Into<String>,
        is_windows_shell: bool,
    ) -> Self {
        let unique_id = unique_id.into();
        let remote_is_windows =
            unique_id == ":1" || (unique_id.len() == 14 && unique_id.ends_with("10"));
        let direction = if mode == TrzszHandshakeMode::Send {
            TrzszTransferDirection::Download
        } else {
            TrzszTransferDirection::Upload
        };
        let selection = if mode == TrzszHandshakeMode::Directory {
            TrzszTransferSelection::Directory
        } else {
            TrzszTransferSelection::File
        };

        Self {
            mode,
            version: version.into(),
            unique_id,
            remote_is_windows: remote_is_windows || is_windows_shell,
            direction,
            selection,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrzszTransferPolicy {
    pub allow_directory: bool,
    pub max_chunk_bytes: usize,
    pub max_file_count: usize,
    pub max_total_bytes: u64,
}

impl Default for TrzszTransferPolicy {
    fn default() -> Self {
        Self {
            allow_directory: true,
            max_chunk_bytes: MAX_TRANSFER_CHUNK_SIZE,
            max_file_count: 1024,
            max_total_bytes: 10 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TrzszTransferEvent {
    Prompt,
    Cancelled,
    Completed,
    Failed { message: String },
    ConnectionLost,
    PartialCleanup,
}
