// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, sync::Arc};

use crate::{
    ApplySavedForwardsSyncSnapshotResult, PersistedForward, SavedForwardError, SavedForwardStore,
    SavedForwardsSyncSnapshot,
};

/// Saved-forward facade used by headless crates that must not pull in SSH or GPUI runtime code.
#[derive(Clone, Debug, Default)]
pub struct ForwardingRegistry {
    saved_store: Option<Arc<SavedForwardStore>>,
}

impl ForwardingRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_store(saved_store: SavedForwardStore) -> Self {
        Self {
            saved_store: Some(Arc::new(saved_store)),
        }
    }

    pub fn delete_owned_forwards(
        &self,
        owner_connection_id: &str,
    ) -> Result<usize, SavedForwardError> {
        let Some(store) = &self.saved_store else {
            return Ok(0);
        };
        store.delete_owned_forwards(owner_connection_id)
    }

    pub fn load_persisted_forwards(&self, session_id: &str) -> Vec<PersistedForward> {
        self.saved_store
            .as_ref()
            .map(|store| store.load_persisted_forwards(session_id))
            .unwrap_or_default()
    }

    pub fn list_saved_forwards(&self, session_id: &str) -> Vec<PersistedForward> {
        self.load_persisted_forwards(session_id)
    }

    pub fn list_all_saved_forwards(&self) -> Vec<PersistedForward> {
        self.saved_store
            .as_ref()
            .map(|store| store.load_syncable_forwards())
            .unwrap_or_default()
    }

    pub fn export_saved_forwards_snapshot(
        &self,
    ) -> Result<SavedForwardsSyncSnapshot, SavedForwardError> {
        let Some(store) = &self.saved_store else {
            return Ok(SavedForwardsSyncSnapshot {
                revision: String::new(),
                exported_at: chrono::Utc::now().to_rfc3339(),
                records: Vec::new(),
            });
        };
        store.export_snapshot()
    }

    pub fn apply_saved_forwards_snapshot(
        &self,
        snapshot: SavedForwardsSyncSnapshot,
        valid_owner_connection_ids: &HashSet<String>,
    ) -> Result<ApplySavedForwardsSyncSnapshotResult, SavedForwardError> {
        let Some(store) = &self.saved_store else {
            return Ok(ApplySavedForwardsSyncSnapshotResult::default());
        };
        store.apply_snapshot(snapshot, valid_owner_connection_ids)
    }
}
