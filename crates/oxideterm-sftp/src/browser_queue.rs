// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{StoredTransferProgress, TransferDirection, TransferProtocol, TransferType};

pub enum BrowserTransferEvent {
    ProtocolResolved(TransferProtocol),
    Progress {
        transferred: u64,
        total: u64,
        speed: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SftpTransferState {
    Pending,
    Active,
    Paused,
    Completed,
    Cancelled,
    Error,
}

#[derive(Clone, Debug)]
pub struct SftpTransferItem<Remote> {
    pub id: u64,
    pub transfer_id: String,
    pub batch_id: Option<u64>,
    pub remote_id: Remote,
    pub name: String,
    pub local_path: String,
    pub remote_path: String,
    pub direction: TransferDirection,
    pub protocol: TransferProtocol,
    pub size: u64,
    pub transferred: u64,
    pub speed: u64,
    pub state: SftpTransferState,
    pub error: Option<String>,
}

pub fn apply_tauri_transfer_progress<Remote>(
    item: &mut SftpTransferItem<Remote>,
    transferred: u64,
    total: u64,
    speed: u64,
) -> bool {
    if matches!(
        item.state,
        SftpTransferState::Completed | SftpTransferState::Cancelled | SftpTransferState::Error
    ) {
        return false;
    }

    item.transferred = transferred;
    // Tauri's transferStore.updateProgress preserves the original size for
    // indeterminate tar/streaming progress where total=0; completion arrives
    // through sftp:complete instead of this progress event.
    if total > 0 {
        item.size = total;
    }
    item.speed = speed;
    item.state = if item.state == SftpTransferState::Paused {
        SftpTransferState::Paused
    } else if total > 0 && transferred >= total {
        SftpTransferState::Completed
    } else {
        SftpTransferState::Active
    };
    true
}

pub fn apply_tauri_transfer_completion<Remote>(
    item: &mut SftpTransferItem<Remote>,
    result: &Result<(), String>,
) -> bool {
    match result {
        Ok(()) => {
            item.transferred = item.size;
            item.state = SftpTransferState::Completed;
            item.error = None;
            true
        }
        Err(_error) if item.state == SftpTransferState::Cancelled => {
            // resolveTransferCompletionUpdate() in the Tauri SFTP view drops a
            // late failure for a user-cancelled transfer so the queue does not
            // flicker back to "error" after the cancellation wins.
            false
        }
        Err(error) => {
            item.state = SftpTransferState::Error;
            item.error = Some(error.clone());
            false
        }
    }
}

pub struct ResumedTransferPaths {
    pub direction: TransferDirection,
    pub local_path: String,
    pub remote_path: String,
    pub name: String,
    pub is_directory: bool,
}

impl ResumedTransferPaths {
    pub fn from_progress(progress: &StoredTransferProgress) -> Self {
        let direction = match progress.transfer_type {
            TransferType::Upload => TransferDirection::Upload,
            TransferType::Download => TransferDirection::Download,
        };
        let (local_path, remote_path) = match direction {
            TransferDirection::Upload => (
                progress.source_path.to_string_lossy().to_string(),
                progress.destination_path.to_string_lossy().to_string(),
            ),
            TransferDirection::Download => (
                progress.destination_path.to_string_lossy().to_string(),
                progress.source_path.to_string_lossy().to_string(),
            ),
        };
        let name = progress
            .source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| progress.source_path.to_str().unwrap_or(""))
            .to_string();
        let is_directory = progress.is_directory();
        Self {
            direction,
            local_path,
            remote_path,
            name,
            is_directory,
        }
    }
}

pub fn format_file_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1024.0 && index < units.len() - 1 {
        value /= 1024.0;
        index += 1;
    }
    if index == 0 {
        format!("{} {}", value.round() as u64, units[index])
    } else {
        format!("{value:.1} {}", units[index])
    }
}

pub fn format_transfer_speed(bytes_per_second: u64) -> String {
    if bytes_per_second == 0 {
        return "-".to_string();
    }
    format!("{}/s", format_file_size(bytes_per_second))
}
