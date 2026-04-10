// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Core StateStore implementation using redb
//!
//! Provides high-performance embedded database for session and forward state persistence.

// Allow large error types - redb::TransactionError is large (160 bytes) but we accept this
// to avoid the overhead of boxing error types in common error paths
#![allow(clippy::result_large_err)]

use redb::{Database, ReadableTable, TableDefinition};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::{error, info, warn};

/// State version for migrations
pub const STATE_VERSION: u32 = 1;

/// Table definitions
const SESSIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("sessions");
const FORWARDS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("forwards");
const METADATA_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("metadata");

/// State persistence errors
///
/// Note: This error type is intentionally large due to containing redb::TransactionError.
/// Boxing would add allocation overhead for a rare error path, so we accept the larger size.
#[derive(Debug, Error)]
#[allow(clippy::result_large_err)]
pub enum StateError {
    #[error("Database error: {0}")]
    Database(#[from] redb::DatabaseError),

    #[error("Transaction error: {0}")]
    Transaction(#[from] redb::TransactionError),

    #[error("Table error: {0}")]
    Table(#[from] redb::TableError),

    #[error("Storage error: {0}")]
    Storage(#[from] redb::StorageError),

    #[error("Commit error: {0}")]
    Commit(#[from] redb::CommitError),

    #[error("MessagePack serialization error: {0}")]
    Serialization(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Item not found: {0}")]
    NotFound(String),

    #[error("Version mismatch: found {found}, expected {expected}")]
    VersionMismatch { found: u32, expected: u32 },
}

impl From<rmp_serde::encode::Error> for StateError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        StateError::Serialization(e.to_string())
    }
}

impl From<rmp_serde::decode::Error> for StateError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        StateError::Serialization(e.to_string())
    }
}

/// High-performance state store using redb
pub struct StateStore {
    db: Arc<Database>,
}

impl StateStore {
    /// Create a new state store at the given path
    pub fn new(path: PathBuf) -> Result<Self, StateError> {
        // Try to open existing database
        let db = match Database::create(&path) {
            Ok(db) => {
                info!("State database opened at {:?}", path);
                db
            }
            Err(e) => {
                warn!(
                    "Failed to open state database: {:?}, attempting recovery",
                    e
                );

                // Backup corrupted file
                let backup_path = path.with_extension("redb.backup");
                if let Err(e) = std::fs::rename(&path, &backup_path) {
                    error!("Failed to backup corrupted database: {:?}", e);
                } else {
                    info!("Backed up corrupted database to {:?}", backup_path);
                }

                // Create new database
                Database::create(&path)?
            }
        };

        // Set file permissions to 600 (owner read/write only) for security
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            {
                warn!(
                    "Failed to set restrictive permissions on database file: {}",
                    e
                );
                // Don't fail - this is a security hardening, not critical
            } else {
                info!("Set database file permissions to 600 (owner-only)");
            }
        }

        #[cfg(windows)]
        {
            // Windows user-profile data directories are already per-user. Attempting to
            // rewrite the ACL here can leave the redb file unreadable on the next launch.
            // Keep the existing inherited ACLs to avoid self-locking the app.
            info!("Skipping Windows ACL rewrite for state database");
        }

        let store = Self { db: Arc::new(db) };

        // Initialize tables and metadata
        store.initialize()?;

        Ok(store)
    }

    /// Initialize database tables and metadata
    fn initialize(&self) -> Result<(), StateError> {
        let write_txn = self.db.begin_write().map_err(|e| {
            error!(
                "Failed to begin write transaction during initialization: {}",
                e
            );
            e
        })?;

        {
            // Create tables if they don't exist
            let _ = write_txn.open_table(SESSIONS_TABLE)?;
            let _ = write_txn.open_table(FORWARDS_TABLE)?;
            let _ = write_txn.open_table(METADATA_TABLE)?;
        }

        write_txn.commit().map_err(|e| {
            error!(
                "Failed to commit initialization transaction (possible disk full): {}",
                e
            );
            e
        })?;

        // Check/set version
        self.check_version()?;

        info!("State store initialized successfully");
        Ok(())
    }

    /// Check and set database version
    fn check_version(&self) -> Result<(), StateError> {
        let write_txn = self.db.begin_write().map_err(|e| {
            error!("Failed to begin write transaction for version check: {}", e);
            e
        })?;

        {
            let mut table = write_txn.open_table(METADATA_TABLE)?;

            let current_version = if let Some(version_bytes) = table.get("version")? {
                let version: u32 = rmp_serde::from_slice(version_bytes.value())?;

                if version > STATE_VERSION {
                    return Err(StateError::VersionMismatch {
                        found: version,
                        expected: STATE_VERSION,
                    });
                }

                if version < STATE_VERSION {
                    info!(
                        "Migrating state database from v{} to v{}",
                        version, STATE_VERSION
                    );
                    // TODO: Add migration logic here if needed
                }
                Some(version)
            } else {
                None
            };

            if current_version.is_none() {
                // First time initialization
                let version_bytes = rmp_serde::to_vec(&STATE_VERSION)?;
                table.insert("version", version_bytes.as_slice())?;
                info!("Initialized state database version: {}", STATE_VERSION);
            }
        }

        write_txn.commit().map_err(|e| {
            error!(
                "Failed to commit version check transaction (possible disk full): {}",
                e
            );
            e
        })?;
        Ok(())
    }

    /// Save a session to the database (synchronous - use save_session_async if possible)
    pub fn save_session(&self, id: &str, data: &[u8]) -> Result<(), StateError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut table = write_txn.open_table(SESSIONS_TABLE)?;
            table.insert(id, data)?;
        }

        write_txn.commit()?;
        Ok(())
    }

    /// Save a session to the database (async, non-blocking)
    pub async fn save_session_async(&self, id: String, data: Vec<u8>) -> Result<(), StateError> {
        let db = self.db.clone();

        let result = tokio::task::spawn_blocking(move || {
            // Wrap in catch_unwind to handle panics
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let write_txn = db.begin_write()?;

                {
                    let mut table = write_txn.open_table(SESSIONS_TABLE)?;
                    table.insert(id.as_str(), data.as_slice())?;
                }

                write_txn.commit()?;
                Ok(())
            }))
        })
        .await
        .map_err(|e| StateError::Io(std::io::Error::other(format!("Task join error: {}", e))))?;

        // Handle panic result
        match result {
            Ok(inner_result) => inner_result,
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                error!("Database save_session operation panicked: {}", panic_msg);
                Err(StateError::Io(std::io::Error::other(format!(
                    "Database panic: {}",
                    panic_msg
                ))))
            }
        }
    }

    /// Load a session from the database (synchronous - use load_session_async if possible)
    pub fn load_session(&self, id: &str) -> Result<Vec<u8>, StateError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SESSIONS_TABLE)?;

        if let Some(value) = table.get(id)? {
            Ok(value.value().to_vec())
        } else {
            Err(StateError::NotFound(format!("Session not found: {}", id)))
        }
    }

    /// Load a session from the database (async, non-blocking)
    pub async fn load_session_async(&self, id: String) -> Result<Vec<u8>, StateError> {
        let db = self.db.clone();

        let result = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(SESSIONS_TABLE)?;

                if let Some(value) = table.get(id.as_str())? {
                    Ok(value.value().to_vec())
                } else {
                    Err(StateError::NotFound(format!("Session not found: {}", id)))
                }
            }))
        })
        .await
        .map_err(|e| StateError::Io(std::io::Error::other(format!("Task join error: {}", e))))?;

        match result {
            Ok(inner_result) => inner_result,
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                error!("Database load_session operation panicked: {}", panic_msg);
                Err(StateError::Io(std::io::Error::other(format!(
                    "Database panic: {}",
                    panic_msg
                ))))
            }
        }
    }

    /// Delete a session from the database (synchronous - use delete_session_async if possible)
    pub fn delete_session(&self, id: &str) -> Result<(), StateError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut table = write_txn.open_table(SESSIONS_TABLE)?;
            table.remove(id)?;
        }

        write_txn.commit()?;
        Ok(())
    }

    /// Delete a session from the database (async, non-blocking)
    pub async fn delete_session_async(&self, id: String) -> Result<(), StateError> {
        let db = self.db.clone();

        let result = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let write_txn = db.begin_write()?;

                {
                    let mut table = write_txn.open_table(SESSIONS_TABLE)?;
                    table.remove(id.as_str())?;
                }

                write_txn.commit()?;
                Ok(())
            }))
        })
        .await
        .map_err(|e| StateError::Io(std::io::Error::other(format!("Task join error: {}", e))))?;

        match result {
            Ok(inner_result) => inner_result,
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                error!("Database delete_session operation panicked: {}", panic_msg);
                Err(StateError::Io(std::io::Error::other(format!(
                    "Database panic: {}",
                    panic_msg
                ))))
            }
        }
    }

    /// List all session IDs (synchronous - use list_sessions_async if possible)
    pub fn list_sessions(&self) -> Result<Vec<String>, StateError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SESSIONS_TABLE)?;

        let mut ids = Vec::new();
        for item in table.iter()? {
            let (key, _) = item?;
            ids.push(key.value().to_string());
        }

        Ok(ids)
    }

    /// List all session IDs (async, non-blocking)
    pub async fn list_sessions_async(&self) -> Result<Vec<String>, StateError> {
        let db = self.db.clone();

        let result = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(SESSIONS_TABLE)?;

                let mut ids = Vec::new();
                for item in table.iter()? {
                    let (key, _) = item?;
                    ids.push(key.value().to_string());
                }

                Ok(ids)
            }))
        })
        .await
        .map_err(|e| StateError::Io(std::io::Error::other(format!("Task join error: {}", e))))?;

        match result {
            Ok(inner_result) => inner_result,
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                error!("Database list_sessions operation panicked: {}", panic_msg);
                Err(StateError::Io(std::io::Error::other(format!(
                    "Database panic: {}",
                    panic_msg
                ))))
            }
        }
    }

    /// Load all sessions at once (async, non-blocking, efficient bulk load)
    pub async fn load_all_sessions_async(&self) -> Result<Vec<(String, Vec<u8>)>, StateError> {
        let db = self.db.clone();

        let result = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(SESSIONS_TABLE)?;

                let mut results = Vec::new();
                for item in table.iter()? {
                    let (key, value) = item?;
                    results.push((key.value().to_string(), value.value().to_vec()));
                }

                Ok(results)
            }))
        })
        .await
        .map_err(|e| StateError::Io(std::io::Error::other(format!("Task join error: {}", e))))?;

        match result {
            Ok(inner_result) => inner_result,
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                error!(
                    "Database load_all_sessions operation panicked: {}",
                    panic_msg
                );
                Err(StateError::Io(std::io::Error::other(format!(
                    "Database panic: {}",
                    panic_msg
                ))))
            }
        }
    }

    /// Save a forward rule to the database (synchronous - use save_forward_async if possible)
    pub fn save_forward(&self, id: &str, data: &[u8]) -> Result<(), StateError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut table = write_txn.open_table(FORWARDS_TABLE)?;
            table.insert(id, data)?;
        }

        write_txn.commit()?;
        Ok(())
    }

    /// Save a forward rule to the database (async, non-blocking)
    pub async fn save_forward_async(&self, id: String, data: Vec<u8>) -> Result<(), StateError> {
        let db = self.db.clone();

        let result = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let write_txn = db.begin_write()?;

                {
                    let mut table = write_txn.open_table(FORWARDS_TABLE)?;
                    table.insert(id.as_str(), data.as_slice())?;
                }

                write_txn.commit()?;
                Ok(())
            }))
        })
        .await
        .map_err(|e| StateError::Io(std::io::Error::other(format!("Task join error: {}", e))))?;

        match result {
            Ok(inner_result) => inner_result,
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                error!("Database save_forward operation panicked: {}", panic_msg);
                Err(StateError::Io(std::io::Error::other(format!(
                    "Database panic: {}",
                    panic_msg
                ))))
            }
        }
    }

    /// Load a forward rule from the database (synchronous - use load_forward_async if possible)
    pub fn load_forward(&self, id: &str) -> Result<Vec<u8>, StateError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(FORWARDS_TABLE)?;

        if let Some(value) = table.get(id)? {
            Ok(value.value().to_vec())
        } else {
            Err(StateError::NotFound(format!("Forward not found: {}", id)))
        }
    }

    /// Load a forward rule from the database (async, non-blocking)
    pub async fn load_forward_async(&self, id: String) -> Result<Vec<u8>, StateError> {
        let db = self.db.clone();

        let result = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(FORWARDS_TABLE)?;

                if let Some(value) = table.get(id.as_str())? {
                    Ok(value.value().to_vec())
                } else {
                    Err(StateError::NotFound(format!("Forward not found: {}", id)))
                }
            }))
        })
        .await
        .map_err(|e| StateError::Io(std::io::Error::other(format!("Task join error: {}", e))))?;

        match result {
            Ok(inner_result) => inner_result,
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                error!("Database load_forward operation panicked: {}", panic_msg);
                Err(StateError::Io(std::io::Error::other(format!(
                    "Database panic: {}",
                    panic_msg
                ))))
            }
        }
    }

    /// Delete a forward rule from the database (synchronous - use delete_forward_async if possible)
    pub fn delete_forward(&self, id: &str) -> Result<(), StateError> {
        let write_txn = self.db.begin_write()?;

        {
            let mut table = write_txn.open_table(FORWARDS_TABLE)?;
            table.remove(id)?;
        }

        write_txn.commit()?;
        Ok(())
    }

    /// Delete a forward rule from the database (async, non-blocking)
    pub async fn delete_forward_async(&self, id: String) -> Result<(), StateError> {
        let db = self.db.clone();

        let result = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let write_txn = db.begin_write()?;

                {
                    let mut table = write_txn.open_table(FORWARDS_TABLE)?;
                    table.remove(id.as_str())?;
                }

                write_txn.commit()?;
                Ok(())
            }))
        })
        .await
        .map_err(|e| StateError::Io(std::io::Error::other(format!("Task join error: {}", e))))?;

        match result {
            Ok(inner_result) => inner_result,
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                error!("Database delete_forward operation panicked: {}", panic_msg);
                Err(StateError::Io(std::io::Error::other(format!(
                    "Database panic: {}",
                    panic_msg
                ))))
            }
        }
    }

    /// List all forward IDs (synchronous - use list_forwards_async if possible)
    pub fn list_forwards(&self) -> Result<Vec<String>, StateError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(FORWARDS_TABLE)?;

        let mut ids = Vec::new();
        for item in table.iter()? {
            let (key, _) = item?;
            ids.push(key.value().to_string());
        }

        Ok(ids)
    }

    /// List all forward IDs (async, non-blocking)
    pub async fn list_forwards_async(&self) -> Result<Vec<String>, StateError> {
        let db = self.db.clone();

        let result = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(FORWARDS_TABLE)?;

                let mut ids = Vec::new();
                for item in table.iter()? {
                    let (key, _) = item?;
                    ids.push(key.value().to_string());
                }

                Ok(ids)
            }))
        })
        .await
        .map_err(|e| StateError::Io(std::io::Error::other(format!("Task join error: {}", e))))?;

        match result {
            Ok(inner_result) => inner_result,
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                error!("Database list_forwards operation panicked: {}", panic_msg);
                Err(StateError::Io(std::io::Error::other(format!(
                    "Database panic: {}",
                    panic_msg
                ))))
            }
        }
    }

    /// Load all forwards at once (async, non-blocking, efficient bulk load)
    pub async fn load_all_forwards_async(&self) -> Result<Vec<(String, Vec<u8>)>, StateError> {
        let db = self.db.clone();

        let result = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(FORWARDS_TABLE)?;

                let mut results = Vec::new();
                for item in table.iter()? {
                    let (key, value) = item?;
                    results.push((key.value().to_string(), value.value().to_vec()));
                }

                Ok(results)
            }))
        })
        .await
        .map_err(|e| StateError::Io(std::io::Error::other(format!("Task join error: {}", e))))?;

        match result {
            Ok(inner_result) => inner_result,
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                error!(
                    "Database load_all_forwards operation panicked: {}",
                    panic_msg
                );
                Err(StateError::Io(std::io::Error::other(format!(
                    "Database panic: {}",
                    panic_msg
                ))))
            }
        }
    }

    /// Get statistics about the database
    pub fn stats(&self) -> Result<StateStats, StateError> {
        let read_txn = self.db.begin_read()?;

        let sessions_table = read_txn.open_table(SESSIONS_TABLE)?;
        let forwards_table = read_txn.open_table(FORWARDS_TABLE)?;

        let mut session_count = 0;
        for _ in sessions_table.iter()? {
            session_count += 1;
        }

        let mut forward_count = 0;
        for _ in forwards_table.iter()? {
            forward_count += 1;
        }

        Ok(StateStats {
            session_count,
            forward_count,
        })
    }
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct StateStats {
    pub session_count: usize,
    pub forward_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (StateStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = StateStore::new(db_path).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_store_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");

        let store = StateStore::new(db_path).unwrap();
        let stats = store.stats().unwrap();

        assert_eq!(stats.session_count, 0);
        assert_eq!(stats.forward_count, 0);
    }

    #[test]
    fn test_session_crud() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = StateStore::new(db_path).unwrap();

        // Create
        let data = b"test session data";
        store.save_session("session1", data).unwrap();

        // Read
        let loaded = store.load_session("session1").unwrap();
        assert_eq!(loaded, data);

        // List
        let ids = store.list_sessions().unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], "session1");

        // Delete
        store.delete_session("session1").unwrap();
        assert!(store.load_session("session1").is_err());
    }

    #[test]
    fn test_forward_crud() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = StateStore::new(db_path).unwrap();

        // Create
        let data = b"test forward data";
        store.save_forward("forward1", data).unwrap();

        // Read
        let loaded = store.load_forward("forward1").unwrap();
        assert_eq!(loaded, data);

        // List
        let ids = store.list_forwards().unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], "forward1");

        // Delete
        store.delete_forward("forward1").unwrap();
        assert!(store.load_forward("forward1").is_err());
    }

    #[test]
    fn test_session_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = StateStore::new(db_path).unwrap();

        store.save_session("s1", b"original").unwrap();
        store.save_session("s1", b"updated").unwrap();

        let loaded = store.load_session("s1").unwrap();
        assert_eq!(loaded, b"updated");

        let stats = store.stats().unwrap();
        assert_eq!(stats.session_count, 1);
    }

    #[test]
    fn test_load_nonexistent_session() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = StateStore::new(db_path).unwrap();

        assert!(store.load_session("nonexistent").is_err());
    }

    #[test]
    fn test_load_nonexistent_forward() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = StateStore::new(db_path).unwrap();

        assert!(store.load_forward("nonexistent").is_err());
    }

    #[test]
    fn test_multiple_sessions() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = StateStore::new(db_path).unwrap();

        for i in 0..5 {
            store
                .save_session(&format!("s{}", i), format!("data{}", i).as_bytes())
                .unwrap();
        }

        let ids = store.list_sessions().unwrap();
        assert_eq!(ids.len(), 5);

        let stats = store.stats().unwrap();
        assert_eq!(stats.session_count, 5);
        assert_eq!(stats.forward_count, 0);
    }

    #[test]
    fn test_delete_nonexistent_session() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = StateStore::new(db_path).unwrap();

        // Should not error when deleting non-existent key
        store.delete_session("nonexistent").unwrap();
    }

    #[test]
    fn test_session_and_forward_independent() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = StateStore::new(db_path).unwrap();

        store.save_session("id1", b"session").unwrap();
        store.save_forward("id1", b"forward").unwrap();

        // Same key in different tables should be independent
        assert_eq!(store.load_session("id1").unwrap(), b"session");
        assert_eq!(store.load_forward("id1").unwrap(), b"forward");

        store.delete_session("id1").unwrap();
        // Forward should still exist
        assert_eq!(store.load_forward("id1").unwrap(), b"forward");
    }

    #[test]
    fn test_forward_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = StateStore::new(db_path).unwrap();

        store.save_forward("f1", b"v1").unwrap();
        store.save_forward("f1", b"v2").unwrap();

        assert_eq!(store.load_forward("f1").unwrap(), b"v2");
        assert_eq!(store.stats().unwrap().forward_count, 1);
    }

    #[test]
    fn test_empty_data() {
        let (store, _temp_dir) = create_test_store();

        store.save_session("empty", b"").unwrap();
        let loaded = store.load_session("empty").unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_bulk_write_stress_keeps_stats_consistent() {
        let (store, _temp_dir) = create_test_store();

        for index in 0..1500 {
            store
                .save_session(
                    &format!("session-{index}"),
                    format!("session-data-{index}").as_bytes(),
                )
                .unwrap();
            store
                .save_forward(
                    &format!("forward-{index}"),
                    format!("forward-data-{index}").as_bytes(),
                )
                .unwrap();
        }

        let stats = store.stats().unwrap();
        assert_eq!(stats.session_count, 1500);
        assert_eq!(stats.forward_count, 1500);
        assert_eq!(store.list_sessions().unwrap().len(), 1500);
        assert_eq!(store.list_forwards().unwrap().len(), 1500);
        assert_eq!(
            store.load_session("session-1499").unwrap(),
            b"session-data-1499"
        );
        assert_eq!(
            store.load_forward("forward-1499").unwrap(),
            b"forward-data-1499"
        );
    }

    #[test]
    fn test_repeated_overwrite_keeps_stats_accurate() {
        let (store, _temp_dir) = create_test_store();

        for version in 0..100 {
            store
                .save_session("dup-session", format!("session-{version}").as_bytes())
                .unwrap();
            store
                .save_forward("dup-forward", format!("forward-{version}").as_bytes())
                .unwrap();
        }

        let stats = store.stats().unwrap();
        assert_eq!(stats.session_count, 1);
        assert_eq!(stats.forward_count, 1);
        assert_eq!(store.load_session("dup-session").unwrap(), b"session-99");
        assert_eq!(store.load_forward("dup-forward").unwrap(), b"forward-99");
    }

    #[test]
    fn test_delete_nonexistent_forward_is_idempotent() {
        let (store, _temp_dir) = create_test_store();

        store.delete_forward("missing-forward").unwrap();
        store.delete_forward("missing-forward").unwrap();
        store.delete_session("missing-session").unwrap();
        store.delete_session("missing-session").unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.session_count, 0);
        assert_eq!(stats.forward_count, 0);
    }

    #[test]
    fn test_aborted_transaction_does_not_leave_dirty_state() {
        let (store, _temp_dir) = create_test_store();

        let write_txn = store.db.begin_write().unwrap();
        {
            let mut session_table = write_txn.open_table(SESSIONS_TABLE).unwrap();
            let mut forward_table = write_txn.open_table(FORWARDS_TABLE).unwrap();
            session_table
                .insert("dirty-session", b"session-bytes".as_slice())
                .unwrap();
            forward_table
                .insert("dirty-forward", b"forward-bytes".as_slice())
                .unwrap();
        }
        drop(write_txn);

        assert!(matches!(
            store.load_session("dirty-session"),
            Err(StateError::NotFound(_))
        ));
        assert!(matches!(
            store.load_forward("dirty-forward"),
            Err(StateError::NotFound(_))
        ));

        let stats = store.stats().unwrap();
        assert_eq!(stats.session_count, 0);
        assert_eq!(stats.forward_count, 0);
    }
}
