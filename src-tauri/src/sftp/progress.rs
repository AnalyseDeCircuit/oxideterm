//! SFTP Transfer Progress Persistence
//!
//! Provides durable storage for transfer progress, enabling resume functionality
//! after interruptions (network failures, app crashes, user pauses).

use crate::sftp::error::SftpError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info};

/// Stored transfer progress record (for persistence)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTransferProgress {
    /// Unique transfer ID (UUID)
    pub transfer_id: String,

    /// Transfer type
    pub transfer_type: TransferType,

    /// Source path (local for upload, remote for download)
    pub source_path: PathBuf,

    /// Destination path (remote for upload, local for download)
    pub destination_path: PathBuf,

    /// Bytes transferred so far
    pub transferred_bytes: u64,

    /// Total bytes to transfer
    pub total_bytes: u64,

    /// Transfer status
    pub status: TransferStatus,

    /// Last update timestamp
    pub last_updated: DateTime<Utc>,

    /// Session ID (for reconnection recovery)
    pub session_id: String,

    /// Error message if failed
    pub error: Option<String>,
}

/// Transfer type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferType {
    Upload,
    Download,
}

/// Transfer status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferStatus {
    /// Currently transferring
    Active,
    /// Paused by user
    Paused,
    /// Failed (recoverable)
    Failed,
    /// Completed successfully
    Completed,
    /// Cancelled by user
    Cancelled,
}

impl StoredTransferProgress {
    /// Calculate completion percentage (0-100)
    pub fn progress_percent(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (self.transferred_bytes as f64 / self.total_bytes as f64) * 100.0
    }

    /// Check if transfer is incomplete (can be resumed)
    pub fn is_incomplete(&self) -> bool {
        matches!(self.status, TransferStatus::Paused | TransferStatus::Failed)
    }

    /// Check if transfer is active (currently running)
    pub fn is_active(&self) -> bool {
        self.status == TransferStatus::Active
    }

    /// Create a new transfer progress record
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
            source_path,
            destination_path,
            transferred_bytes: 0,
            total_bytes,
            status: TransferStatus::Active,
            last_updated: Utc::now(),
            session_id,
            error: None,
        }
    }

    /// Update transferred bytes and timestamp
    pub fn update_progress(&mut self, transferred_bytes: u64) {
        self.transferred_bytes = transferred_bytes;
        self.last_updated = Utc::now();
    }

    /// Mark as completed
    pub fn mark_completed(&mut self) {
        self.status = TransferStatus::Completed;
        self.transferred_bytes = self.total_bytes;
        self.last_updated = Utc::now();
        self.error = None;
    }

    /// Mark as failed
    pub fn mark_failed(&mut self, error: String) {
        self.status = TransferStatus::Failed;
        self.error = Some(error);
        self.last_updated = Utc::now();
    }

    /// Mark as paused
    pub fn mark_paused(&mut self) {
        self.status = TransferStatus::Paused;
        self.last_updated = Utc::now();
    }

    /// Mark as cancelled
    pub fn mark_cancelled(&mut self) {
        self.status = TransferStatus::Cancelled;
        self.last_updated = Utc::now();
    }

    /// Mark as active (resuming)
    pub fn mark_active(&mut self) {
        self.status = TransferStatus::Active;
        self.last_updated = Utc::now();
        self.error = None;
    }
}

/// Progress storage interface
#[async_trait]
pub trait ProgressStore: Send + Sync {
    /// Save or update progress record
    async fn save(&self, progress: &StoredTransferProgress) -> Result<(), SftpError>;

    /// Load progress record by transfer ID
    async fn load(&self, transfer_id: &str) -> Result<Option<StoredTransferProgress>, SftpError>;

    /// List all incomplete transfers for a session
    async fn list_incomplete(
        &self,
        session_id: &str,
    ) -> Result<Vec<StoredTransferProgress>, SftpError>;

    /// List all incomplete transfers across all sessions
    async fn list_all_incomplete(&self) -> Result<Vec<StoredTransferProgress>, SftpError>;

    /// Delete progress record
    async fn delete(&self, transfer_id: &str) -> Result<(), SftpError>;

    /// Delete all progress records for a session
    async fn delete_for_session(&self, session_id: &str) -> Result<(), SftpError>;
}

/// Table definition for progress storage
const PROGRESS_TABLE: redb::TableDefinition<&str, &[u8]> =
    redb::TableDefinition::new("sftp_transfer_progress");

/// redb-based progress store implementation
pub struct RedbProgressStore {
    db: redb::Database,
}

/// Dummy progress store that doesn't persist (used when storage is unavailable)
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
        Ok(vec![])
    }

    async fn list_all_incomplete(&self) -> Result<Vec<StoredTransferProgress>, SftpError> {
        Ok(vec![])
    }

    async fn delete(&self, _transfer_id: &str) -> Result<(), SftpError> {
        Ok(())
    }

    async fn delete_for_session(&self, _session_id: &str) -> Result<(), SftpError> {
        Ok(())
    }
}

impl RedbProgressStore {
    /// Create a new progress store with database at given path
    pub fn new(db_path: &PathBuf) -> Result<Self, SftpError> {
        info!("Creating SFTP progress store at: {:?}", db_path);

        let db = redb::Database::create(db_path).map_err(|e| {
            SftpError::StorageError(format!("Failed to create progress database: {}", e))
        })?;

        // Ensure table exists
        let write_txn = db.begin_write().map_err(|e| {
            SftpError::StorageError(format!("Failed to begin write transaction: {}", e))
        })?;

        {
            let _table = write_txn.open_table(PROGRESS_TABLE).map_err(|e| {
                SftpError::StorageError(format!("Failed to open progress table: {}", e))
            })?;
        }

        write_txn
            .commit()
            .map_err(|e| SftpError::StorageError(format!("Failed to commit transaction: {}", e)))?;

        debug!("SFTP progress store initialized successfully");

        Ok(Self { db })
    }

    /// Get default progress store path (in ~/.oxideterm)
    pub fn default_path() -> Result<PathBuf, SftpError> {
        let home = dirs::home_dir().ok_or_else(|| {
            SftpError::StorageError("Cannot determine home directory".to_string())
        })?;

        let config_dir = home.join(".oxideterm");

        // Ensure directory exists
        std::fs::create_dir_all(&config_dir).map_err(|e| {
            SftpError::StorageError(format!("Failed to create config directory: {}", e))
        })?;

        Ok(config_dir.join("sftp_progress.redb"))
    }
}

#[async_trait]
impl ProgressStore for RedbProgressStore {
    async fn save(&self, progress: &StoredTransferProgress) -> Result<(), SftpError> {
        let transfer_id = progress.transfer_id.clone();

        debug!(
            "Saving progress for transfer {}: {} / {} bytes ({:.1}%)",
            transfer_id,
            progress.transferred_bytes,
            progress.total_bytes,
            progress.progress_percent()
        );

        let serialized = rmp_serde::to_vec_named(progress)
            .map_err(|e| SftpError::StorageError(format!("Failed to serialize progress: {}", e)))?;

        let write_txn = self.db.begin_write().map_err(|e| {
            SftpError::StorageError(format!("Failed to begin write transaction: {}", e))
        })?;

        {
            let mut table = write_txn.open_table(PROGRESS_TABLE).map_err(|e| {
                SftpError::StorageError(format!("Failed to open progress table: {}", e))
            })?;

            table
                .insert(transfer_id.as_str(), serialized.as_slice())
                .map_err(|e| {
                    SftpError::StorageError(format!("Failed to insert progress: {}", e))
                })?;
        }

        write_txn
            .commit()
            .map_err(|e| SftpError::StorageError(format!("Failed to commit transaction: {}", e)))?;

        debug!("Progress saved successfully for transfer {}", transfer_id);

        Ok(())
    }

    async fn load(&self, transfer_id: &str) -> Result<Option<StoredTransferProgress>, SftpError> {
        debug!("Loading progress for transfer {}", transfer_id);

        let read_txn = self.db.begin_read().map_err(|e| {
            SftpError::StorageError(format!("Failed to begin read transaction: {}", e))
        })?;

        let table = read_txn.open_table(PROGRESS_TABLE).map_err(|e| {
            SftpError::StorageError(format!("Failed to open progress table: {}", e))
        })?;

        match table
            .get(transfer_id)
            .map_err(|e| SftpError::StorageError(format!("Failed to read progress: {}", e)))?
        {
            Some(value) => {
                let progress: StoredTransferProgress = rmp_serde::from_slice(&value.value())
                    .map_err(|e| {
                        SftpError::StorageError(format!("Failed to deserialize progress: {}", e))
                    })?;

                debug!(
                    "Loaded progress for transfer {}: {} / {} bytes, status: {:?}",
                    transfer_id, progress.transferred_bytes, progress.total_bytes, progress.status
                );

                Ok(Some(progress))
            }
            None => {
                debug!("No progress found for transfer {}", transfer_id);
                Ok(None)
            }
        }
    }

    async fn list_incomplete(
        &self,
        session_id: &str,
    ) -> Result<Vec<StoredTransferProgress>, SftpError> {
        debug!("Listing incomplete transfers for session {}", session_id);

        let read_txn = self.db.begin_read().map_err(|e| {
            SftpError::StorageError(format!("Failed to begin read transaction: {}", e))
        })?;

        let table = read_txn.open_table(PROGRESS_TABLE).map_err(|e| {
            SftpError::StorageError(format!("Failed to open progress table: {}", e))
        })?;

        let mut results = Vec::new();

        for item in table.iter().map_err(|e| {
            SftpError::StorageError(format!("Failed to iterate progress table: {}", e))
        })? {
            let (key, value) = item.map_err(|e| {
                SftpError::StorageError(format!("Failed to read progress entry: {}", e))
            })?;

            let progress: StoredTransferProgress =
                rmp_serde::from_slice(value.value()).map_err(|e| {
                    SftpError::StorageError(format!("Failed to deserialize progress: {}", e))
                })?;

            // Filter by session ID and incomplete status
            if progress.session_id == session_id && progress.is_incomplete() {
                debug!(
                    "Found incomplete transfer {}: {:?}",
                    key.value(),
                    progress.status
                );
                results.push(progress);
            }
        }

        debug!(
            "Found {} incomplete transfers for session {}",
            results.len(),
            session_id
        );

        Ok(results)
    }

    async fn list_all_incomplete(&self) -> Result<Vec<StoredTransferProgress>, SftpError> {
        debug!("Listing all incomplete transfers");

        let read_txn = self.db.begin_read().map_err(|e| {
            SftpError::StorageError(format!("Failed to begin read transaction: {}", e))
        })?;

        let table = read_txn.open_table(PROGRESS_TABLE).map_err(|e| {
            SftpError::StorageError(format!("Failed to open progress table: {}", e))
        })?;

        let mut results = Vec::new();

        for item in table.iter().map_err(|e| {
            SftpError::StorageError(format!("Failed to iterate progress table: {}", e))
        })? {
            let (_key, value) = item.map_err(|e| {
                SftpError::StorageError(format!("Failed to read progress entry: {}", e))
            })?;

            let progress: StoredTransferProgress =
                rmp_serde::from_slice(value.value()).map_err(|e| {
                    SftpError::StorageError(format!("Failed to deserialize progress: {}", e))
                })?;

            if progress.is_incomplete() {
                results.push(progress);
            }
        }

        debug!("Found {} incomplete transfers total", results.len());

        Ok(results)
    }

    async fn delete(&self, transfer_id: &str) -> Result<(), SftpError> {
        debug!("Deleting progress for transfer {}", transfer_id);

        let write_txn = self.db.begin_write().map_err(|e| {
            SftpError::StorageError(format!("Failed to begin write transaction: {}", e))
        })?;

        {
            let mut table = write_txn.open_table(PROGRESS_TABLE).map_err(|e| {
                SftpError::StorageError(format!("Failed to open progress table: {}", e))
            })?;

            table.remove(transfer_id).map_err(|e| {
                SftpError::StorageError(format!("Failed to delete progress: {}", e))
            })?;
        }

        write_txn
            .commit()
            .map_err(|e| SftpError::StorageError(format!("Failed to commit transaction: {}", e)))?;

        debug!("Progress deleted for transfer {}", transfer_id);

        Ok(())
    }

    async fn delete_for_session(&self, session_id: &str) -> Result<(), SftpError> {
        debug!("Deleting all progress for session {}", session_id);

        // First, list all incomplete transfers for this session
        let transfers = self.list_incomplete(session_id).await?;

        if transfers.is_empty() {
            debug!("No transfers to delete for session {}", session_id);
            return Ok(());
        }

        let transfer_count = transfers.len();

        // Delete each one
        for transfer in transfers {
            self.delete(&transfer.transfer_id).await?;
        }

        debug!(
            "Deleted {} transfers for session {}",
            transfer_count, session_id
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_progress_store_save_and_load() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = RedbProgressStore::new(&db_path).unwrap();

        let progress = StoredTransferProgress::new(
            "test-1".to_string(),
            TransferType::Download,
            "/remote/file.txt".into(),
            "/local/file.txt".into(),
            2048,
            "session-1".to_string(),
        );

        // Save
        store.save(&progress).await.unwrap();

        // Load
        let loaded = store.load("test-1").await.unwrap().unwrap();
        assert_eq!(loaded.transfer_id, "test-1");
        assert_eq!(loaded.total_bytes, 2048);
        assert_eq!(loaded.status, TransferStatus::Active);
    }

    #[tokio::test]
    async fn test_progress_store_list_incomplete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = RedbProgressStore::new(&db_path).unwrap();

        // Create multiple transfers
        let mut progress1 = StoredTransferProgress::new(
            "test-1".to_string(),
            TransferType::Download,
            "/remote/file1.txt".into(),
            "/local/file1.txt".into(),
            2048,
            "session-1".to_string(),
        );

        let mut progress2 = StoredTransferProgress::new(
            "test-2".to_string(),
            TransferType::Upload,
            "/local/file2.txt".into(),
            "/remote/file2.txt".into(),
            4096,
            "session-1".to_string(),
        );

        progress1.mark_failed("Network error".to_string());
        progress2.mark_paused();

        store.save(&progress1).await.unwrap();
        store.save(&progress2).await.unwrap();

        // List incomplete
        let incomplete = store.list_incomplete("session-1").await.unwrap();
        assert_eq!(incomplete.len(), 2);
    }

    #[tokio::test]
    async fn test_progress_store_delete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = RedbProgressStore::new(&db_path).unwrap();

        let progress = StoredTransferProgress::new(
            "test-1".to_string(),
            TransferType::Download,
            "/remote/file.txt".into(),
            "/local/file.txt".into(),
            2048,
            "session-1".to_string(),
        );

        store.save(&progress).await.unwrap();

        // Delete
        store.delete("test-1").await.unwrap();

        // Verify deleted
        let loaded = store.load("test-1").await.unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_progress_percent() {
        let progress = StoredTransferProgress {
            transfer_id: "test".to_string(),
            transfer_type: TransferType::Download,
            source_path: "/remote/file.txt".into(),
            destination_path: "/local/file.txt".into(),
            transferred_bytes: 1024,
            total_bytes: 2048,
            status: TransferStatus::Active,
            last_updated: Utc::now(),
            session_id: "session-1".to_string(),
            error: None,
        };

        assert_eq!(progress.progress_percent(), 50.0);
    }

    #[test]
    fn test_progress_status_transitions() {
        let mut progress = StoredTransferProgress::new(
            "test".to_string(),
            TransferType::Download,
            "/remote/file.txt".into(),
            "/local/file.txt".into(),
            2048,
            "session-1".to_string(),
        );

        assert!(progress.is_active());
        assert!(!progress.is_incomplete());

        progress.mark_failed("Error".to_string());
        assert!(!progress.is_active());
        assert!(progress.is_incomplete());

        progress.mark_active();
        assert!(progress.is_active());
        assert!(!progress.is_incomplete());

        progress.mark_completed();
        assert!(!progress.is_active());
        assert!(!progress.is_incomplete());
    }
}
