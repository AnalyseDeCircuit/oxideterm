// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Session metadata persistence
//!
//! Handles serialization and deserialization of session metadata for recovery.

// Allow large error types from StateError (contains redb::TransactionError ~160 bytes)
#![allow(clippy::result_large_err)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::store::{StateError, StateStore};
use crate::session::types::SessionConfig;

/// Persisted session metadata (excludes runtime data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSession {
    /// Unique session ID
    pub id: String,

    /// Session configuration
    pub config: SessionConfig,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Tab order for UI
    pub order: usize,

    /// Version for migration support
    #[serde(default)]
    pub version: u32,

    /// Terminal buffer content (optional, can be large)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_buffer: Option<Vec<u8>>,

    /// Buffer configuration
    #[serde(default)]
    pub buffer_config: BufferConfig,
}

/// Terminal buffer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferConfig {
    /// Maximum lines to keep in buffer
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,

    /// Whether to save buffer on disconnect
    #[serde(default = "default_save_on_disconnect")]
    pub save_on_disconnect: bool,
}

fn default_max_lines() -> usize {
    8_000
}

fn default_save_on_disconnect() -> bool {
    true
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            max_lines: default_max_lines(),
            save_on_disconnect: default_save_on_disconnect(),
        }
    }
}

impl PersistedSession {
    /// Create a new persisted session
    pub fn new(id: String, config: SessionConfig, order: usize) -> Self {
        Self {
            id,
            config,
            created_at: Utc::now(),
            order,
            version: 2, // Incremented for buffer support
            terminal_buffer: None,
            buffer_config: BufferConfig::default(),
        }
    }

    /// Create a persisted session with explicit buffer config but no saved buffer payload.
    pub fn with_config(
        id: String,
        config: SessionConfig,
        order: usize,
        buffer_config: BufferConfig,
    ) -> Self {
        Self {
            id,
            config,
            created_at: Utc::now(),
            order,
            version: 2,
            terminal_buffer: None,
            buffer_config,
        }
    }

    /// Create with terminal buffer
    pub fn with_buffer(
        id: String,
        config: SessionConfig,
        order: usize,
        terminal_buffer: Vec<u8>,
        buffer_config: BufferConfig,
    ) -> Self {
        Self {
            id,
            config,
            created_at: Utc::now(),
            order,
            version: 2,
            terminal_buffer: Some(terminal_buffer),
            buffer_config,
        }
    }

    /// Serialize to bytes (using MessagePack for binary persistence)
    pub fn to_bytes(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        rmp_serde::to_vec_named(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(data)
    }
}

/// Session persistence operations
pub struct SessionPersistence {
    store: Arc<StateStore>,
}

impl SessionPersistence {
    /// Create a new session persistence handler
    pub fn new(store: Arc<StateStore>) -> Self {
        Self { store }
    }

    /// Save a session (synchronous)
    pub fn save(&self, session: &PersistedSession) -> Result<(), StateError> {
        let data = session.to_bytes()?;

        self.store.save_session(&session.id, &data)?;

        Ok(())
    }

    /// Save a session (async, non-blocking)
    pub async fn save_async(&self, session: PersistedSession) -> Result<(), StateError> {
        let data = session.to_bytes()?;

        self.store.save_session_async(session.id, data).await?;

        Ok(())
    }

    /// Load a session by ID
    pub fn load(&self, id: &str) -> Result<PersistedSession, StateError> {
        let data = self.store.load_session(id)?;

        Ok(PersistedSession::from_bytes(&data)?)
    }

    /// Delete a session (synchronous)
    pub fn delete(&self, id: &str) -> Result<(), StateError> {
        self.store.delete_session(id)
    }

    /// Delete a session (async, non-blocking)
    pub async fn delete_async(&self, id: String) -> Result<(), StateError> {
        self.store.delete_session_async(id).await
    }

    /// Load all sessions (synchronous)
    pub fn load_all(&self) -> Result<Vec<PersistedSession>, StateError> {
        let ids = self.store.list_sessions()?;

        let mut sessions = Vec::new();
        for id in ids {
            match self.load(&id) {
                Ok(session) => sessions.push(session),
                Err(e) => {
                    tracing::warn!("Failed to load session {}: {:?}", id, e);
                    // Continue loading other sessions
                }
            }
        }

        // Sort by order
        sessions.sort_by_key(|s| s.order);

        Ok(sessions)
    }

    /// Load all sessions (async, non-blocking, optimized bulk load)
    pub async fn load_all_async(&self) -> Result<Vec<PersistedSession>, StateError> {
        // Use bulk load to avoid N+1 queries
        let all_data = self.store.load_all_sessions_async().await?;

        let mut sessions = Vec::new();
        for (id, data) in all_data {
            match PersistedSession::from_bytes(&data) {
                Ok(session) => sessions.push(session),
                Err(e) => {
                    tracing::warn!("Failed to deserialize session {}: {:?}", id, e);
                }
            }
        }

        // Sort by order
        sessions.sort_by_key(|s| s.order);

        Ok(sessions)
    }

    /// List all session IDs
    pub fn list_ids(&self) -> Result<Vec<String>, StateError> {
        self.store.list_sessions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::SessionConfig;
    use tempfile::TempDir;

    fn create_test_store() -> (TempDir, Arc<StateStore>) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = Arc::new(StateStore::new(db_path).unwrap());
        (temp_dir, store)
    }

    #[test]
    fn test_persisted_session_serialization() {
        let config = SessionConfig::with_password("example.com", 22, "user", "pass");

        let session = PersistedSession::new("session-1".to_string(), config, 0);

        let bytes = session.to_bytes().unwrap();
        let deserialized = PersistedSession::from_bytes(&bytes).unwrap();

        assert_eq!(session.id, deserialized.id);
        assert_eq!(session.config.host, deserialized.config.host);
    }

    #[test]
    fn test_session_persistence() {
        let (_temp_dir, store) = create_test_store();
        let persistence = SessionPersistence::new(store);

        let config = SessionConfig::with_password("example.com", 22, "user", "pass");

        let session = PersistedSession::new("session-1".to_string(), config, 0);

        // Save
        persistence.save(&session).unwrap();

        // Load
        let loaded = persistence.load("session-1").unwrap();
        assert_eq!(session.id, loaded.id);

        // Delete
        persistence.delete("session-1").unwrap();
        assert!(persistence.load("session-1").is_err());
    }

    #[test]
    fn test_load_all_sessions() {
        let (_temp_dir, store) = create_test_store();
        let persistence = SessionPersistence::new(store);

        // Create multiple sessions
        for i in 0..3 {
            let config = SessionConfig::with_password(format!("host{}.com", i), 22, "user", "pass");

            let session = PersistedSession::new(format!("session-{}", i), config, i);

            persistence.save(&session).unwrap();
        }

        // Load all
        let sessions = persistence.load_all().unwrap();
        assert_eq!(sessions.len(), 3);

        // Check ordering
        for (i, session) in sessions.iter().enumerate() {
            assert_eq!(session.order, i);
        }
    }
}
