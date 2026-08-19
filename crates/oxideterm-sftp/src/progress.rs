// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::{RemoteRelayDisposition, SftpError};

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

#[async_trait]
pub trait ProgressStore: Send + Sync {
    async fn save(&self, progress: &StoredTransferProgress) -> Result<(), SftpError>;
    async fn load(&self, transfer_id: &str) -> Result<Option<StoredTransferProgress>, SftpError>;
    async fn list_incomplete(
        &self,
        session_id: &str,
    ) -> Result<Vec<StoredTransferProgress>, SftpError>;
    async fn list_all_incomplete(&self) -> Result<Vec<StoredTransferProgress>, SftpError>;
    async fn delete(&self, transfer_id: &str) -> Result<(), SftpError>;
    async fn delete_for_session(&self, session_id: &str) -> Result<(), SftpError>;
}

pub struct DummyProgressStore;

#[async_trait]
impl ProgressStore for DummyProgressStore {
    async fn save(&self, _progress: &StoredTransferProgress) -> Result<(), SftpError> {
        Ok(())
    }

    async fn load(&self, _transfer_id: &str) -> Result<Option<StoredTransferProgress>, SftpError> {
        Ok(None)
    }

    async fn list_incomplete(
        &self,
        _session_id: &str,
    ) -> Result<Vec<StoredTransferProgress>, SftpError> {
        Ok(Vec::new())
    }

    async fn list_all_incomplete(&self) -> Result<Vec<StoredTransferProgress>, SftpError> {
        Ok(Vec::new())
    }

    async fn delete(&self, _transfer_id: &str) -> Result<(), SftpError> {
        Ok(())
    }

    async fn delete_for_session(&self, _session_id: &str) -> Result<(), SftpError> {
        Ok(())
    }
}

pub struct LazyProgressStore {
    db_path: PathBuf,
    store: OnceLock<Arc<dyn ProgressStore>>,
}

impl LazyProgressStore {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            store: OnceLock::new(),
        }
    }

    fn store(&self) -> Arc<dyn ProgressStore> {
        self.store
            .get_or_init(|| match RedbProgressStore::new(&self.db_path) {
                Ok(store) => Arc::new(store),
                Err(error) => {
                    warn!(
                        path = %self.db_path.display(),
                        error = %error,
                        "falling back to in-memory SFTP progress store"
                    );
                    Arc::new(DummyProgressStore)
                }
            })
            .clone()
    }
}

#[async_trait]
impl ProgressStore for LazyProgressStore {
    async fn save(&self, progress: &StoredTransferProgress) -> Result<(), SftpError> {
        self.store().save(progress).await
    }

    async fn load(&self, transfer_id: &str) -> Result<Option<StoredTransferProgress>, SftpError> {
        self.store().load(transfer_id).await
    }

    async fn list_incomplete(
        &self,
        session_id: &str,
    ) -> Result<Vec<StoredTransferProgress>, SftpError> {
        self.store().list_incomplete(session_id).await
    }

    async fn list_all_incomplete(&self) -> Result<Vec<StoredTransferProgress>, SftpError> {
        self.store().list_all_incomplete().await
    }

    async fn delete(&self, transfer_id: &str) -> Result<(), SftpError> {
        self.store().delete(transfer_id).await
    }

    async fn delete_for_session(&self, session_id: &str) -> Result<(), SftpError> {
        self.store().delete_for_session(session_id).await
    }
}

const PROGRESS_TABLE: redb::TableDefinition<&str, &[u8]> =
    redb::TableDefinition::new("sftp_transfer_progress");
const INCOMPLETE_PROGRESS_TABLE: redb::TableDefinition<&str, &[u8]> =
    redb::TableDefinition::new("sftp_transfer_incomplete_progress");
const SESSION_INCOMPLETE_INDEX_TABLE: redb::TableDefinition<&str, &str> =
    redb::TableDefinition::new("sftp_transfer_incomplete_session_index");

pub struct RedbProgressStore {
    db: Arc<redb::Database>,
}

fn session_incomplete_index_key(session_id: &str, transfer_id: &str) -> String {
    format!("{session_id}:{transfer_id}")
}

fn session_incomplete_index_end_key(session_id: &str) -> String {
    format!("{session_id};")
}

impl RedbProgressStore {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self, SftpError> {
        let db_path = db_path.as_ref();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                SftpError::StorageError(format!("Failed to create progress directory: {error}"))
            })?;
        }
        info!("Creating SFTP progress store at: {:?}", db_path);
        let db = redb::Database::create(db_path)
            .map(Arc::new)
            .map_err(|error| {
                SftpError::StorageError(format!("Failed to create progress database: {error}"))
            })?;
        let store = Self { db };
        store.ensure_tables()?;
        // LazyProgressStore does not publish this store until construction
        // finishes, so normalization cannot race transfers in this process.
        store.normalize_stale_progress_and_rebuild_indexes()?;
        Ok(store)
    }

    fn ensure_tables(&self) -> Result<(), SftpError> {
        let write_txn = self.db.begin_write().map_err(|error| {
            SftpError::StorageError(format!("Failed to begin write transaction: {error}"))
        })?;
        {
            write_txn.open_table(PROGRESS_TABLE).map_err(|error| {
                SftpError::StorageError(format!("Failed to open progress table: {error}"))
            })?;
            write_txn
                .open_table(INCOMPLETE_PROGRESS_TABLE)
                .map_err(|error| {
                    SftpError::StorageError(format!(
                        "Failed to open incomplete progress table: {error}"
                    ))
                })?;
            write_txn
                .open_table(SESSION_INCOMPLETE_INDEX_TABLE)
                .map_err(|error| {
                    SftpError::StorageError(format!(
                        "Failed to open session incomplete index table: {error}"
                    ))
                })?;
        }
        write_txn.commit().map_err(|error| {
            SftpError::StorageError(format!("Failed to commit transaction: {error}"))
        })
    }

    fn normalize_stale_progress_and_rebuild_indexes(&self) -> Result<(), SftpError> {
        let read_txn = self.db.begin_read().map_err(|error| {
            SftpError::StorageError(format!("Failed to begin read transaction: {error}"))
        })?;
        let table = read_txn.open_table(PROGRESS_TABLE).map_err(|error| {
            SftpError::StorageError(format!("Failed to open progress table: {error}"))
        })?;
        let mut incomplete_entries = Vec::new();
        let mut recovered_active_entries = Vec::new();
        let mut entries_to_delete = Vec::new();
        for item in table.iter().map_err(|error| {
            SftpError::StorageError(format!("Failed to iterate progress table: {error}"))
        })? {
            let (key, value) = item.map_err(|error| {
                SftpError::StorageError(format!("Failed to read progress entry: {error}"))
            })?;
            let transfer_id = key.value().to_string();
            let progress: StoredTransferProgress = match rmp_serde::from_slice(value.value()) {
                Ok(progress) => progress,
                Err(error) => {
                    // Older native builds briefly wrote incompatible progress rows while the
                    // transfer schema was being ported. Tauri rebuilds indexes from valid
                    // resumable transfers; one stale row must not disable the whole store.
                    warn!(
                        transfer_id,
                        error = %error,
                        "dropping unreadable SFTP transfer progress row during index rebuild"
                    );
                    entries_to_delete.push(transfer_id);
                    continue;
                }
            };
            match progress.status {
                TransferStatus::Active => {
                    let mut recovered = progress;
                    recovered.mark_paused();
                    let serialized = rmp_serde::to_vec_named(&recovered).map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to serialize recovered progress: {error}"
                        ))
                    })?;
                    recovered_active_entries.push((transfer_id, recovered.session_id, serialized));
                }
                TransferStatus::Paused | TransferStatus::Failed => {
                    incomplete_entries.push((transfer_id, progress.session_id));
                }
                TransferStatus::Completed | TransferStatus::Cancelled => {
                    entries_to_delete.push(transfer_id);
                }
            }
        }
        drop(table);
        drop(read_txn);

        let write_txn = self.db.begin_write().map_err(|error| {
            SftpError::StorageError(format!("Failed to begin write transaction: {error}"))
        })?;
        {
            let mut progress_table = write_txn.open_table(PROGRESS_TABLE).map_err(|error| {
                SftpError::StorageError(format!("Failed to open progress table: {error}"))
            })?;
            let mut incomplete_table =
                write_txn
                    .open_table(INCOMPLETE_PROGRESS_TABLE)
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to open incomplete progress table: {error}"
                        ))
                    })?;
            let mut session_index_table = write_txn
                .open_table(SESSION_INCOMPLETE_INDEX_TABLE)
                .map_err(|error| {
                    SftpError::StorageError(format!(
                        "Failed to open session incomplete index table: {error}"
                    ))
                })?;
            incomplete_table
                .retain_in::<&str, _>(.., |_, _| false)
                .map_err(|error| {
                    SftpError::StorageError(format!(
                        "Failed to clear incomplete progress table: {error}"
                    ))
                })?;
            session_index_table
                .retain_in::<&str, _>(.., |_, _| false)
                .map_err(|error| {
                    SftpError::StorageError(format!(
                        "Failed to clear session incomplete index table: {error}"
                    ))
                })?;
            for transfer_id in entries_to_delete {
                progress_table
                    .remove(transfer_id.as_str())
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to remove stale progress entry: {error}"
                        ))
                    })?;
            }
            for (transfer_id, session_id, serialized) in recovered_active_entries {
                progress_table
                    .insert(transfer_id.as_str(), serialized.as_slice())
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to recover active progress entry: {error}"
                        ))
                    })?;
                incomplete_table
                    .insert(transfer_id.as_str(), serialized.as_slice())
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to index recovered progress entry: {error}"
                        ))
                    })?;
                let session_key = session_incomplete_index_key(&session_id, &transfer_id);
                session_index_table
                    .insert(session_key.as_str(), transfer_id.as_str())
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to index recovered progress session: {error}"
                        ))
                    })?;
            }
            for (transfer_id, session_id) in incomplete_entries {
                if let Some(value) = progress_table.get(transfer_id.as_str()).map_err(|error| {
                    SftpError::StorageError(format!(
                        "Failed to load progress during index rebuild: {error}"
                    ))
                })? {
                    incomplete_table
                        .insert(transfer_id.as_str(), value.value())
                        .map_err(|error| {
                            SftpError::StorageError(format!(
                                "Failed to rebuild incomplete progress table: {error}"
                            ))
                        })?;
                    let session_key = session_incomplete_index_key(&session_id, &transfer_id);
                    session_index_table
                        .insert(session_key.as_str(), transfer_id.as_str())
                        .map_err(|error| {
                            SftpError::StorageError(format!(
                                "Failed to rebuild session incomplete index: {error}"
                            ))
                        })?;
                }
            }
        }
        write_txn.commit().map_err(|error| {
            SftpError::StorageError(format!("Failed to commit transaction: {error}"))
        })
    }

    fn save_sync(db: &redb::Database, progress: &StoredTransferProgress) -> Result<(), SftpError> {
        let transfer_id = progress.transfer_id.as_str();
        let session_index_key = session_incomplete_index_key(&progress.session_id, transfer_id);
        let serialized = rmp_serde::to_vec_named(progress).map_err(|error| {
            SftpError::StorageError(format!("Failed to serialize progress: {error}"))
        })?;
        let write_txn = db.begin_write().map_err(|error| {
            SftpError::StorageError(format!("Failed to begin write transaction: {error}"))
        })?;
        {
            let mut table = write_txn.open_table(PROGRESS_TABLE).map_err(|error| {
                SftpError::StorageError(format!("Failed to open progress table: {error}"))
            })?;
            let mut incomplete_table =
                write_txn
                    .open_table(INCOMPLETE_PROGRESS_TABLE)
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to open incomplete progress table: {error}"
                        ))
                    })?;
            let mut session_index_table = write_txn
                .open_table(SESSION_INCOMPLETE_INDEX_TABLE)
                .map_err(|error| {
                    SftpError::StorageError(format!(
                        "Failed to open session incomplete index table: {error}"
                    ))
                })?;
            let previous_session_index_key = table
                .get(transfer_id)
                .map_err(|error| {
                    SftpError::StorageError(format!("Failed to read previous progress: {error}"))
                })?
                .map(|value| {
                    rmp_serde::from_slice::<StoredTransferProgress>(value.value()).map_err(
                        |error| {
                            SftpError::StorageError(format!(
                                "Failed to deserialize previous progress: {error}"
                            ))
                        },
                    )
                })
                .transpose()?
                .filter(|previous| previous.session_id != progress.session_id)
                .map(|previous| session_incomplete_index_key(&previous.session_id, transfer_id));
            if let Some(previous_session_index_key) = previous_session_index_key {
                // A reconnect moves one logical transfer to a new connection
                // generation; keep exactly one session lookup index for it.
                session_index_table
                    .remove(previous_session_index_key.as_str())
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to remove previous session index: {error}"
                        ))
                    })?;
            }
            table
                .insert(transfer_id, serialized.as_slice())
                .map_err(|error| {
                    SftpError::StorageError(format!("Failed to insert progress: {error}"))
                })?;
            if progress.is_incomplete() {
                incomplete_table
                    .insert(transfer_id, serialized.as_slice())
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to insert incomplete progress: {error}"
                        ))
                    })?;
                session_index_table
                    .insert(session_index_key.as_str(), transfer_id)
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to insert incomplete session index: {error}"
                        ))
                    })?;
            } else {
                incomplete_table.remove(transfer_id).map_err(|error| {
                    SftpError::StorageError(format!(
                        "Failed to remove incomplete progress: {error}"
                    ))
                })?;
                session_index_table
                    .remove(session_index_key.as_str())
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to remove incomplete session index: {error}"
                        ))
                    })?;
            }
        }
        write_txn.commit().map_err(|error| {
            SftpError::StorageError(format!("Failed to commit transaction: {error}"))
        })
    }
}

#[async_trait]
impl ProgressStore for RedbProgressStore {
    async fn save(&self, progress: &StoredTransferProgress) -> Result<(), SftpError> {
        let transfer_id = progress.transfer_id.clone();
        let db = Arc::clone(&self.db);
        let progress = progress.clone();
        // Redb write transactions can wait for another writer and commit synchronously.
        // Keep that work off the shared async runtime used by active SFTP transfers.
        tokio::task::spawn_blocking(move || Self::save_sync(db.as_ref(), &progress))
            .await
            .map_err(|error| {
                SftpError::StorageError(format!("SFTP progress save task failed: {error}"))
            })??;
        debug!("Progress saved successfully for transfer {}", transfer_id);
        Ok(())
    }

    async fn load(&self, transfer_id: &str) -> Result<Option<StoredTransferProgress>, SftpError> {
        let read_txn = self.db.begin_read().map_err(|error| {
            SftpError::StorageError(format!("Failed to begin read transaction: {error}"))
        })?;
        let table = read_txn.open_table(PROGRESS_TABLE).map_err(|error| {
            SftpError::StorageError(format!("Failed to open progress table: {error}"))
        })?;
        let Some(value) = table.get(transfer_id).map_err(|error| {
            SftpError::StorageError(format!("Failed to read progress: {error}"))
        })?
        else {
            return Ok(None);
        };
        let progress = rmp_serde::from_slice(value.value()).map_err(|error| {
            SftpError::StorageError(format!("Failed to deserialize progress: {error}"))
        })?;
        Ok(Some(progress))
    }

    async fn list_incomplete(
        &self,
        session_id: &str,
    ) -> Result<Vec<StoredTransferProgress>, SftpError> {
        let read_txn = self.db.begin_read().map_err(|error| {
            SftpError::StorageError(format!("Failed to begin read transaction: {error}"))
        })?;
        let session_index_table = read_txn
            .open_table(SESSION_INCOMPLETE_INDEX_TABLE)
            .map_err(|error| {
                SftpError::StorageError(format!(
                    "Failed to open session incomplete index table: {error}"
                ))
            })?;
        let incomplete_table = read_txn
            .open_table(INCOMPLETE_PROGRESS_TABLE)
            .map_err(|error| {
                SftpError::StorageError(format!(
                    "Failed to open incomplete progress table: {error}"
                ))
            })?;
        let mut results = Vec::new();
        let start_key = session_incomplete_index_key(session_id, "");
        let end_key = session_incomplete_index_end_key(session_id);
        for item in session_index_table
            .range(start_key.as_str()..end_key.as_str())
            .map_err(|error| {
                SftpError::StorageError(format!(
                    "Failed to iterate incomplete session index: {error}"
                ))
            })?
        {
            let (_key, transfer_id) = item.map_err(|error| {
                SftpError::StorageError(format!("Failed to read session index entry: {error}"))
            })?;
            if let Some(value) = incomplete_table.get(transfer_id.value()).map_err(|error| {
                SftpError::StorageError(format!(
                    "Failed to read indexed incomplete progress: {error}"
                ))
            })? {
                results.push(rmp_serde::from_slice(value.value()).map_err(|error| {
                    SftpError::StorageError(format!(
                        "Failed to deserialize indexed progress: {error}"
                    ))
                })?);
            }
        }
        Ok(results)
    }

    async fn list_all_incomplete(&self) -> Result<Vec<StoredTransferProgress>, SftpError> {
        let read_txn = self.db.begin_read().map_err(|error| {
            SftpError::StorageError(format!("Failed to begin read transaction: {error}"))
        })?;
        let table = read_txn
            .open_table(INCOMPLETE_PROGRESS_TABLE)
            .map_err(|error| {
                SftpError::StorageError(format!(
                    "Failed to open incomplete progress table: {error}"
                ))
            })?;
        let mut results = Vec::new();
        for item in table.iter().map_err(|error| {
            SftpError::StorageError(format!("Failed to iterate progress table: {error}"))
        })? {
            let (_key, value) = item.map_err(|error| {
                SftpError::StorageError(format!(
                    "Failed to read incomplete progress entry: {error}"
                ))
            })?;
            results.push(rmp_serde::from_slice(value.value()).map_err(|error| {
                SftpError::StorageError(format!("Failed to deserialize progress: {error}"))
            })?);
        }
        Ok(results)
    }

    async fn delete(&self, transfer_id: &str) -> Result<(), SftpError> {
        let existing = self.load(transfer_id).await?;
        let write_txn = self.db.begin_write().map_err(|error| {
            SftpError::StorageError(format!("Failed to begin write transaction: {error}"))
        })?;
        {
            let mut table = write_txn.open_table(PROGRESS_TABLE).map_err(|error| {
                SftpError::StorageError(format!("Failed to open progress table: {error}"))
            })?;
            let mut incomplete_table =
                write_txn
                    .open_table(INCOMPLETE_PROGRESS_TABLE)
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to open incomplete progress table: {error}"
                        ))
                    })?;
            let mut session_index_table = write_txn
                .open_table(SESSION_INCOMPLETE_INDEX_TABLE)
                .map_err(|error| {
                    SftpError::StorageError(format!(
                        "Failed to open session incomplete index table: {error}"
                    ))
                })?;
            table.remove(transfer_id).map_err(|error| {
                SftpError::StorageError(format!("Failed to delete progress: {error}"))
            })?;
            incomplete_table.remove(transfer_id).map_err(|error| {
                SftpError::StorageError(format!("Failed to delete incomplete progress: {error}"))
            })?;
            if let Some(progress) = existing.as_ref() {
                let session_key = session_incomplete_index_key(&progress.session_id, transfer_id);
                session_index_table
                    .remove(session_key.as_str())
                    .map_err(|error| {
                        SftpError::StorageError(format!(
                            "Failed to delete session incomplete index entry: {error}"
                        ))
                    })?;
            }
        }
        write_txn.commit().map_err(|error| {
            SftpError::StorageError(format!("Failed to commit transaction: {error}"))
        })
    }

    async fn delete_for_session(&self, session_id: &str) -> Result<(), SftpError> {
        for progress in self.list_incomplete(session_id).await? {
            self.delete(&progress.transfer_id).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::{Future, poll_fn},
        sync::mpsc,
        task::Poll,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn temp_progress_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("oxideterm-sftp-{name}-{nonce}.redb"))
    }

    #[test]
    fn legacy_progress_defaults_to_a_non_relay_transfer() {
        let progress: StoredTransferProgress = serde_json::from_value(serde_json::json!({
            "transfer_id": "legacy-transfer",
            "transfer_type": "Download",
            "source_path": "/remote/file.txt",
            "destination_path": "/local/file.txt",
            "transferred_bytes": 4,
            "total_bytes": 8,
            "status": "Paused",
            "last_updated": "2026-08-19T00:00:00Z",
            "session_id": "session-1",
            "error": null
        }))
        .expect("deserialize legacy progress");

        assert!(progress.remote_relay.is_none());
    }

    #[test]
    fn progress_store_rebuild_drops_unreadable_legacy_rows() {
        let path = temp_progress_path("legacy-row");
        {
            let db = redb::Database::create(&path).expect("create test redb");
            let write_txn = db.begin_write().expect("begin seed transaction");
            {
                let mut progress_table = write_txn
                    .open_table(PROGRESS_TABLE)
                    .expect("open progress table");
                let mut incomplete_table = write_txn
                    .open_table(INCOMPLETE_PROGRESS_TABLE)
                    .expect("open incomplete table");
                let mut session_index_table = write_txn
                    .open_table(SESSION_INCOMPLETE_INDEX_TABLE)
                    .expect("open session index table");

                // msgpack integer 36 is the exact incompatible legacy shape from the
                // native transfer-progress port. It must not disable the whole store.
                let legacy_integer: &[u8] = &[36_u8];
                progress_table
                    .insert("legacy", legacy_integer)
                    .expect("seed invalid progress row");
                incomplete_table
                    .insert("legacy", legacy_integer)
                    .expect("seed invalid incomplete row");
                session_index_table
                    .insert("session:legacy", "legacy")
                    .expect("seed stale session index row");
            }
            write_txn.commit().expect("commit seed transaction");
        }

        let store = RedbProgressStore::new(&path).expect("open progress store");
        let read_txn = store.db.begin_read().expect("begin read transaction");
        let progress_table = read_txn
            .open_table(PROGRESS_TABLE)
            .expect("open rebuilt progress table");
        assert!(
            progress_table
                .get("legacy")
                .expect("read legacy progress row")
                .is_none()
        );
        drop(progress_table);
        drop(read_txn);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn reopening_store_normalizes_only_records_left_by_the_previous_process() {
        let path = temp_progress_path("startup-normalization");
        let store = RedbProgressStore::new(&path).expect("open progress store");

        let active = StoredTransferProgress::new(
            "active-transfer".to_string(),
            TransferType::Download,
            PathBuf::from("/remote/active"),
            PathBuf::from("/local/active"),
            128,
            "session-1".to_string(),
        );
        let mut paused = active.clone();
        paused.transfer_id = "paused-transfer".to_string();
        paused.mark_paused();
        let mut failed = active.clone();
        failed.transfer_id = "failed-transfer".to_string();
        failed.mark_failed("connection lost".to_string());
        let mut completed = active.clone();
        completed.transfer_id = "completed-transfer".to_string();
        completed.mark_completed();
        let mut cancelled = active.clone();
        cancelled.transfer_id = "cancelled-transfer".to_string();
        cancelled.mark_cancelled();

        for progress in [&active, &paused, &failed, &completed, &cancelled] {
            store.save(progress).await.expect("seed progress record");
        }
        drop(store);

        // Reopening is the only point where no transfer can yet hold the lazy
        // store, so stale state can be normalized without racing live writes.
        let reopened = RedbProgressStore::new(&path).expect("reopen progress store");
        let recovered_active = reopened
            .load("active-transfer")
            .await
            .expect("load recovered active transfer")
            .expect("active transfer remains recoverable");
        assert_eq!(recovered_active.status, TransferStatus::Paused);
        assert!(reopened.load("completed-transfer").await.unwrap().is_none());
        assert!(reopened.load("cancelled-transfer").await.unwrap().is_none());

        let mut incomplete_ids = reopened
            .list_all_incomplete()
            .await
            .expect("list normalized incomplete transfers")
            .into_iter()
            .map(|progress| progress.transfer_id)
            .collect::<Vec<_>>();
        incomplete_ids.sort();
        assert_eq!(
            incomplete_ids,
            vec!["active-transfer", "failed-transfer", "paused-transfer"]
        );

        drop(reopened);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn redb_save_does_not_block_async_runtime_while_waiting_for_writer() {
        let path = temp_progress_path("nonblocking-save");
        let store = Arc::new(RedbProgressStore::new(&path).expect("open progress store"));
        let held_db = Arc::clone(&store.db);
        let (writer_ready_tx, writer_ready_rx) = mpsc::sync_channel(1);
        let (release_writer_tx, release_writer_rx) = mpsc::sync_channel(1);
        let writer = std::thread::spawn(move || {
            let transaction = held_db.begin_write().expect("hold write transaction");
            writer_ready_tx.send(()).expect("signal held writer");
            // A timeout prevents a broken implementation from hanging the test forever.
            let _ = release_writer_rx.recv_timeout(Duration::from_secs(2));
            drop(transaction);
        });
        writer_ready_rx.recv().expect("wait for held writer");

        let mut progress = StoredTransferProgress::new(
            "transfer-1".to_string(),
            TransferType::Download,
            PathBuf::from("/remote/file.txt"),
            PathBuf::from("/local/file.txt"),
            128,
            "session-1".to_string(),
        );
        progress.mark_paused();
        let (save_waited_asynchronously, responsiveness_elapsed) = {
            let save = store.save(&progress);
            tokio::pin!(save);

            let responsiveness_started = Instant::now();
            // Poll the save future directly so the test proves its first poll never waits
            // synchronously for Redb's single-writer lock.
            let first_poll = poll_fn(|cx| Poll::Ready(save.as_mut().poll(cx))).await;
            let responsiveness_elapsed = responsiveness_started.elapsed();
            let save_waited_asynchronously = first_poll.is_pending();

            let _ = release_writer_tx.send(());
            writer.join().expect("join held writer");
            match first_poll {
                Poll::Ready(result) => result.expect("save progress"),
                Poll::Pending => save.await.expect("save progress"),
            }
            (save_waited_asynchronously, responsiveness_elapsed)
        };
        let saved = store
            .load("transfer-1")
            .await
            .expect("load saved progress")
            .expect("saved progress exists");
        assert_eq!(saved.status, TransferStatus::Paused);
        assert_eq!(
            store
                .list_incomplete("session-1")
                .await
                .expect("list incomplete progress")
                .len(),
            1
        );
        assert!(
            save_waited_asynchronously,
            "Redb save completed while another write transaction was held"
        );
        assert!(
            responsiveness_elapsed < Duration::from_millis(500),
            "Redb save blocked the async runtime for {responsiveness_elapsed:?}"
        );

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn saving_progress_moves_the_incomplete_session_index() {
        let path = temp_progress_path("move-session-index");
        let store = RedbProgressStore::new(&path).expect("open progress store");
        let mut progress = StoredTransferProgress::new(
            "transfer-1".to_string(),
            TransferType::Download,
            PathBuf::from("/remote/file.txt"),
            PathBuf::from("/local/file.txt"),
            128,
            "connection-generation-a".to_string(),
        );
        progress.mark_failed("Connection lost".to_string());
        store.save(&progress).await.expect("save old generation");

        progress.session_id = "connection-generation-b".to_string();
        store.save(&progress).await.expect("save new generation");

        assert!(
            store
                .list_incomplete("connection-generation-a")
                .await
                .expect("list old generation")
                .is_empty()
        );
        assert_eq!(
            store
                .list_incomplete("connection-generation-b")
                .await
                .expect("list new generation")
                .len(),
            1
        );

        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
