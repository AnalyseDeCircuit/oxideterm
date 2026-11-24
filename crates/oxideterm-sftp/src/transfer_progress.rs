// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::RemoteRelayDisposition;

const REMOTE_RELAY_PROGRESS_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRelayProgressContext {
    pub profile_id: String,
    pub profile_revision: String,
    pub source_endpoint_id: String,
    pub destination_endpoint_id: String,
}

impl RemoteRelayProgressContext {
    pub fn storage_key(&self) -> String {
        format!("standalone-sftp-relay:{}", self.profile_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredRemoteRelayProgress {
    #[serde(default = "default_remote_relay_progress_version")]
    pub version: u8,
    pub profile_id: String,
    pub profile_revision: String,
    pub source_endpoint_id: String,
    pub destination_endpoint_id: String,
    pub staging_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<PathBuf>,
    pub disposition: RemoteRelayDisposition,
    pub source_size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_modified: Option<u32>,
    pub source_sample_sha256: String,
}

const fn default_remote_relay_progress_version() -> u8 {
    REMOTE_RELAY_PROGRESS_VERSION
}

impl StoredRemoteRelayProgress {
    pub fn new(
        context: &RemoteRelayProgressContext,
        staging_path: PathBuf,
        backup_path: Option<PathBuf>,
        disposition: RemoteRelayDisposition,
        source_size: u64,
        source_modified: Option<u32>,
        source_sample_sha256: String,
    ) -> Self {
        Self {
            version: REMOTE_RELAY_PROGRESS_VERSION,
            profile_id: context.profile_id.clone(),
            profile_revision: context.profile_revision.clone(),
            source_endpoint_id: context.source_endpoint_id.clone(),
            destination_endpoint_id: context.destination_endpoint_id.clone(),
            staging_path,
            backup_path,
            disposition,
            source_size,
            source_modified,
            source_sample_sha256,
        }
    }

    pub fn supports_resume(&self) -> bool {
        self.version == REMOTE_RELAY_PROGRESS_VERSION
    }

    pub fn contains_endpoint(&self, endpoint_id: &str) -> bool {
        self.source_endpoint_id == endpoint_id || self.destination_endpoint_id == endpoint_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTransferProgress {
    pub transfer_id: String,
    pub transfer_type: TransferType,
    #[serde(default)]
    pub protocol: TransferProtocol,
    #[serde(default)]
    pub strategy: TransferStrategy,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub status: TransferStatus,
    pub last_updated: DateTime<Utc>,
    pub session_id: String,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_relay: Option<StoredRemoteRelayProgress>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransferProtocol {
    #[default]
    Sftp,
    Scp,
    Ftp,
    Ftps,
}

impl TransferProtocol {
    /// SCP can pause a live channel but cannot restart from a byte offset.
    pub const fn supports_restart_resume(self) -> bool {
        matches!(self, Self::Sftp)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferType {
    Upload,
    Download,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransferStrategy {
    #[default]
    File,
    DirectoryRecursive,
    DirectoryTar,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferStatus {
    Active,
    Paused,
    Failed,
    Completed,
    Cancelled,
}

impl StoredTransferProgress {
    pub fn new(
        transfer_id: String,
        transfer_type: TransferType,
        source_path: PathBuf,
        destination_path: PathBuf,
        total_bytes: u64,
        session_id: String,
    ) -> Self {
        Self {
            transfer_id,
            transfer_type,
            protocol: TransferProtocol::Sftp,
            strategy: TransferStrategy::File,
            source_path,
            destination_path,
            transferred_bytes: 0,
            total_bytes,
            status: TransferStatus::Active,
            last_updated: Utc::now(),
            session_id,
            error: None,
            remote_relay: None,
        }
    }

    pub fn progress_percent(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.transferred_bytes as f64 / self.total_bytes as f64) * 100.0
        }
    }

    pub fn is_incomplete(&self) -> bool {
        matches!(self.status, TransferStatus::Paused | TransferStatus::Failed)
    }

    pub fn is_active(&self) -> bool {
        self.status == TransferStatus::Active
    }

    pub fn is_directory(&self) -> bool {
        self.strategy != TransferStrategy::File
    }

    pub fn update_progress(&mut self, transferred_bytes: u64) {
        self.transferred_bytes = transferred_bytes;
        self.last_updated = Utc::now();
    }

    pub fn mark_completed(&mut self) {
        self.status = TransferStatus::Completed;
        self.transferred_bytes = self.total_bytes;
        self.error = None;
        self.last_updated = Utc::now();
    }

    pub fn mark_failed(&mut self, error: String) {
        self.status = TransferStatus::Failed;
        self.error = Some(error);
        self.last_updated = Utc::now();
    }

    pub fn mark_paused(&mut self) {
        self.status = TransferStatus::Paused;
        self.last_updated = Utc::now();
    }

    pub fn mark_cancelled(&mut self) {
        self.status = TransferStatus::Cancelled;
        self.last_updated = Utc::now();
    }

    pub fn mark_active(&mut self) {
        self.status = TransferStatus::Active;
        self.error = None;
        self.last_updated = Utc::now();
    }
}
