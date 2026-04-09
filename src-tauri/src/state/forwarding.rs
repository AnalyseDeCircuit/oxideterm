// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Port forwarding rules persistence
//!
//! Handles serialization and deserialization of forward rules for recovery.

// Allow large error types from StateError (contains redb::TransactionError ~160 bytes)
#![allow(clippy::result_large_err)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::store::{StateError, StateStore};
use crate::forwarding::manager::ForwardRule;

/// Forward type enum for persistence
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ForwardType {
    Local,
    Remote,
    Dynamic,
}

impl ForwardType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::Dynamic => "dynamic",
        }
    }

    pub fn to_runtime(&self) -> crate::forwarding::ForwardType {
        match self {
            Self::Local => crate::forwarding::ForwardType::Local,
            Self::Remote => crate::forwarding::ForwardType::Remote,
            Self::Dynamic => crate::forwarding::ForwardType::Dynamic,
        }
    }
}

impl TryFrom<&str> for ForwardType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "local" => Ok(Self::Local),
            "remote" => Ok(Self::Remote),
            "dynamic" => Ok(Self::Dynamic),
            other => Err(format!("Unsupported forward type: {}", other)),
        }
    }
}

impl From<&crate::forwarding::ForwardType> for ForwardType {
    fn from(value: &crate::forwarding::ForwardType) -> Self {
        match value {
            crate::forwarding::ForwardType::Local => Self::Local,
            crate::forwarding::ForwardType::Remote => Self::Remote,
            crate::forwarding::ForwardType::Dynamic => Self::Dynamic,
        }
    }
}

/// Persisted forward rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedForward {
    /// Unique forward ID
    pub id: String,

    /// Associated session ID
    pub session_id: String,

    /// Saved connection that owns this forward, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_connection_id: Option<String>,

    /// Forward type
    pub forward_type: ForwardType,

    /// Forward rule details
    pub rule: ForwardRule,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Whether to auto-start on session restore
    pub auto_start: bool,

    /// Version for migration support
    #[serde(default)]
    pub version: u32,
}

impl PersistedForward {
    /// Create a new persisted forward
    pub fn new(
        id: String,
        session_id: String,
        owner_connection_id: Option<String>,
        forward_type: ForwardType,
        rule: ForwardRule,
        auto_start: bool,
    ) -> Self {
        Self {
            id,
            session_id,
            owner_connection_id,
            forward_type,
            rule,
            created_at: Utc::now(),
            auto_start,
            version: 1,
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

/// Forward persistence operations
pub struct ForwardPersistence {
    store: Arc<StateStore>,
}

impl ForwardPersistence {
    /// Create a new forward persistence handler
    pub fn new(store: Arc<StateStore>) -> Self {
        Self { store }
    }

    /// Save a forward rule (synchronous)
    pub fn save(&self, forward: &PersistedForward) -> Result<(), StateError> {
        let data = forward.to_bytes()?;

        self.store.save_forward(&forward.id, &data)?;

        Ok(())
    }

    /// Save a forward rule (async, non-blocking)
    pub async fn save_async(&self, forward: PersistedForward) -> Result<(), StateError> {
        let data = forward.to_bytes()?;

        self.store.save_forward_async(forward.id, data).await?;

        Ok(())
    }

    /// Load a forward rule by ID
    pub fn load(&self, id: &str) -> Result<PersistedForward, StateError> {
        let data = self.store.load_forward(id)?;

        Ok(PersistedForward::from_bytes(&data)?)
    }

    /// Delete a forward rule (synchronous)
    pub fn delete(&self, id: &str) -> Result<(), StateError> {
        self.store.delete_forward(id)
    }

    /// Delete a forward rule (async, non-blocking)
    pub async fn delete_async(&self, id: String) -> Result<(), StateError> {
        self.store.delete_forward_async(id).await
    }

    /// Update auto-start flag for a forward
    pub fn update_auto_start(&self, id: &str, auto_start: bool) -> Result<(), StateError> {
        let mut forward = self.load(id)?;
        forward.auto_start = auto_start;
        self.save(&forward)?;
        Ok(())
    }

    /// Load all forwards (synchronous)
    pub fn load_all(&self) -> Result<Vec<PersistedForward>, StateError> {
        let ids = self.store.list_forwards()?;

        let mut forwards = Vec::new();
        for id in ids {
            match self.load(&id) {
                Ok(forward) => forwards.push(forward),
                Err(e) => {
                    tracing::warn!("Failed to load forward {}: {:?}", id, e);
                    // Continue loading other forwards
                }
            }
        }

        // Sort by creation time
        forwards.sort_by_key(|f| f.created_at);

        Ok(forwards)
    }

    /// Load all forwards (async, non-blocking, optimized bulk load)
    pub async fn load_all_async(&self) -> Result<Vec<PersistedForward>, StateError> {
        // Use bulk load to avoid N+1 queries (1 spawn_blocking instead of N+1)
        let all_data = self.store.load_all_forwards_async().await?;

        let mut forwards = Vec::new();
        for (id, data) in all_data {
            match PersistedForward::from_bytes(&data) {
                Ok(forward) => forwards.push(forward),
                Err(e) => {
                    tracing::warn!("Failed to deserialize forward {}: {:?}", id, e);
                }
            }
        }

        // Sort by creation time
        forwards.sort_by_key(|f| f.created_at);

        Ok(forwards)
    }

    /// Load forwards for a specific session
    pub fn load_by_session(&self, session_id: &str) -> Result<Vec<PersistedForward>, StateError> {
        let all_forwards = self.load_all()?;

        Ok(all_forwards
            .into_iter()
            .filter(|f| f.session_id == session_id)
            .collect())
    }

    /// Load forwards owned by a saved connection
    pub fn load_by_owner(
        &self,
        owner_connection_id: &str,
    ) -> Result<Vec<PersistedForward>, StateError> {
        let all_forwards = self.load_all()?;

        Ok(all_forwards
            .into_iter()
            .filter(|f| f.owner_connection_id.as_deref() == Some(owner_connection_id))
            .collect())
    }

    /// Delete all forwards for a session
    pub fn delete_by_session(&self, session_id: &str) -> Result<usize, StateError> {
        let forwards = self.load_by_session(session_id)?;
        let count = forwards.len();

        for forward in forwards {
            self.delete(&forward.id)?;
        }

        Ok(count)
    }

    /// Delete all forwards owned by a saved connection
    pub fn delete_by_owner(&self, owner_connection_id: &str) -> Result<usize, StateError> {
        let forwards = self.load_by_owner(owner_connection_id)?;
        let count = forwards.len();

        for forward in forwards {
            self.delete(&forward.id)?;
        }

        Ok(count)
    }

    /// Clear or delete forwards when a runtime session disappears.
    /// Owner-bound forwards are detached from the old session and preserved.
    pub fn handle_session_shutdown(&self, session_id: &str) -> Result<(usize, usize), StateError> {
        let forwards = self.load_by_session(session_id)?;
        let mut deleted = 0;
        let mut detached = 0;

        for mut forward in forwards {
            if forward.owner_connection_id.is_some() {
                forward.session_id.clear();
                self.save(&forward)?;
                detached += 1;
            } else {
                self.delete(&forward.id)?;
                deleted += 1;
            }
        }

        Ok((deleted, detached))
    }

    /// Rebind all owner-bound forwards to a newly established session.
    pub fn rebind_owner_to_session(
        &self,
        owner_connection_id: &str,
        session_id: &str,
    ) -> Result<usize, StateError> {
        let forwards = self.load_by_owner(owner_connection_id)?;
        let mut rebound = 0;

        for mut forward in forwards {
            if forward.session_id.is_empty() {
                forward.session_id = session_id.to_string();
                self.save(&forward)?;
                rebound += 1;
            }
        }

        Ok(rebound)
    }

    /// List all forward IDs
    pub fn list_ids(&self) -> Result<Vec<String>, StateError> {
        self.store.list_forwards()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forwarding::manager::ForwardRule;
    use tempfile::TempDir;

    fn create_test_store() -> (TempDir, Arc<StateStore>) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.redb");
        let store = Arc::new(StateStore::new(db_path).unwrap());
        (temp_dir, store)
    }

    fn create_test_forward_rule() -> ForwardRule {
        ForwardRule {
            id: "forward-1".to_string(),
            forward_type: crate::forwarding::manager::ForwardType::Local,
            bind_address: "127.0.0.1".to_string(),
            bind_port: 8080,
            target_host: "localhost".to_string(),
            target_port: 80,
            status: crate::forwarding::manager::ForwardStatus::Active,
            description: None,
        }
    }

    #[test]
    fn test_persisted_forward_serialization() {
        let rule = create_test_forward_rule();

        let forward = PersistedForward::new(
            "forward-1".to_string(),
            "session-1".to_string(),
            Some("conn-1".to_string()),
            ForwardType::Local,
            rule,
            false,
        );

        let bytes = forward.to_bytes().unwrap();
        let deserialized = PersistedForward::from_bytes(&bytes).unwrap();

        assert_eq!(forward.id, deserialized.id);
        assert_eq!(forward.session_id, deserialized.session_id);
        assert_eq!(
            forward.owner_connection_id,
            deserialized.owner_connection_id
        );
    }

    #[test]
    fn test_forward_persistence() {
        let (_temp_dir, store) = create_test_store();
        let persistence = ForwardPersistence::new(store);

        let rule = create_test_forward_rule();
        let forward = PersistedForward::new(
            "forward-1".to_string(),
            "session-1".to_string(),
            Some("conn-1".to_string()),
            ForwardType::Local,
            rule,
            false,
        );

        // Save
        persistence.save(&forward).unwrap();

        // Load
        let loaded = persistence.load("forward-1").unwrap();
        assert_eq!(forward.id, loaded.id);
        assert_eq!(loaded.auto_start, false);

        // Update auto_start
        persistence.update_auto_start("forward-1", true).unwrap();
        let updated = persistence.load("forward-1").unwrap();
        assert_eq!(updated.auto_start, true);

        // Delete
        persistence.delete("forward-1").unwrap();
        assert!(persistence.load("forward-1").is_err());
    }

    #[test]
    fn test_load_by_session() {
        let (_temp_dir, store) = create_test_store();
        let persistence = ForwardPersistence::new(store);

        // Create forwards for two different sessions
        for session_num in 1..=2 {
            for forward_num in 1..=2 {
                let rule = create_test_forward_rule();
                let forward = PersistedForward::new(
                    format!("forward-{}-{}", session_num, forward_num),
                    format!("session-{}", session_num),
                    Some(format!("conn-{}", session_num)),
                    ForwardType::Local,
                    rule,
                    false,
                );
                persistence.save(&forward).unwrap();
            }
        }

        // Load for session-1
        let session1_forwards = persistence.load_by_session("session-1").unwrap();
        assert_eq!(session1_forwards.len(), 2);

        // Load for session-2
        let session2_forwards = persistence.load_by_session("session-2").unwrap();
        assert_eq!(session2_forwards.len(), 2);
    }

    #[test]
    fn test_delete_by_session() {
        let (_temp_dir, store) = create_test_store();
        let persistence = ForwardPersistence::new(store);

        // Create forwards for a session
        for i in 1..=3 {
            let rule = create_test_forward_rule();
            let forward = PersistedForward::new(
                format!("forward-{}", i),
                "session-1".to_string(),
                Some("conn-1".to_string()),
                ForwardType::Local,
                rule,
                false,
            );
            persistence.save(&forward).unwrap();
        }

        // Delete all forwards for session-1
        let count = persistence.delete_by_session("session-1").unwrap();
        assert_eq!(count, 3);

        // Verify they're deleted
        let remaining = persistence.load_by_session("session-1").unwrap();
        assert_eq!(remaining.len(), 0);
    }

    #[test]
    fn test_load_and_delete_by_owner() {
        let (_temp_dir, store) = create_test_store();
        let persistence = ForwardPersistence::new(store);

        for i in 1..=3 {
            let rule = create_test_forward_rule();
            let forward = PersistedForward::new(
                format!("forward-{}", i),
                format!("session-{}", i),
                Some("conn-owner".to_string()),
                ForwardType::Local,
                rule,
                false,
            );
            persistence.save(&forward).unwrap();
        }

        assert_eq!(persistence.load_by_owner("conn-owner").unwrap().len(), 3);
        assert_eq!(persistence.delete_by_owner("conn-owner").unwrap(), 3);
        assert!(persistence.load_by_owner("conn-owner").unwrap().is_empty());
    }

    #[test]
    fn test_handle_session_shutdown_preserves_owner_bound_forwards() {
        let (_temp_dir, store) = create_test_store();
        let persistence = ForwardPersistence::new(store);

        let owner_bound = PersistedForward::new(
            "forward-owner".to_string(),
            "session-1".to_string(),
            Some("conn-1".to_string()),
            ForwardType::Local,
            create_test_forward_rule(),
            false,
        );
        let session_only = PersistedForward::new(
            "forward-session".to_string(),
            "session-1".to_string(),
            None,
            ForwardType::Local,
            create_test_forward_rule(),
            false,
        );

        persistence.save(&owner_bound).unwrap();
        persistence.save(&session_only).unwrap();

        let (deleted, detached) = persistence.handle_session_shutdown("session-1").unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(detached, 1);

        let reloaded_owner = persistence.load("forward-owner").unwrap();
        assert_eq!(reloaded_owner.session_id, "");
        assert!(persistence.load("forward-session").is_err());
    }

    #[test]
    fn test_rebind_owner_to_session_updates_all_owned_forwards() {
        let (_temp_dir, store) = create_test_store();
        let persistence = ForwardPersistence::new(store);

        for i in 1..=2 {
            let forward = PersistedForward::new(
                format!("forward-{}", i),
                String::new(),
                Some("conn-1".to_string()),
                ForwardType::Local,
                create_test_forward_rule(),
                false,
            );
            persistence.save(&forward).unwrap();
        }

        assert_eq!(
            persistence
                .rebind_owner_to_session("conn-1", "session-9")
                .unwrap(),
            2
        );
        assert!(
            persistence
                .load_by_owner("conn-1")
                .unwrap()
                .into_iter()
                .all(|forward| forward.session_id == "session-9")
        );
    }

    #[test]
    fn test_rebind_owner_to_session_does_not_override_existing_binding() {
        let (_temp_dir, store) = create_test_store();
        let persistence = ForwardPersistence::new(store);

        let forward = PersistedForward::new(
            "forward-existing".to_string(),
            "session-existing".to_string(),
            Some("conn-1".to_string()),
            ForwardType::Local,
            create_test_forward_rule(),
            false,
        );
        persistence.save(&forward).unwrap();

        assert_eq!(
            persistence
                .rebind_owner_to_session("conn-1", "session-new")
                .unwrap(),
            0
        );
        assert_eq!(
            persistence.load("forward-existing").unwrap().session_id,
            "session-existing"
        );
    }
}
