// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, sync::Arc, sync::mpsc::Sender};

use dashmap::DashMap;
use oxideterm_ssh::SshConnectionHandle;

use crate::{
    ApplySavedForwardsSyncSnapshotResult, ForwardEvent, ForwardRule, ForwardingManager,
    PersistedForward, SavedForwardError, SavedForwardStore, SavedForwardsSyncSnapshot,
};

#[derive(Clone, Debug, Default)]
pub struct ForwardingRegistry {
    managers: Arc<DashMap<String, Arc<ForwardingManager>>>,
    event_tx: Option<Sender<ForwardEvent>>,
    saved_store: Option<Arc<SavedForwardStore>>,
}

impl ForwardingRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_event_sender(event_tx: Sender<ForwardEvent>) -> Self {
        Self {
            managers: Arc::new(DashMap::new()),
            event_tx: Some(event_tx),
            saved_store: None,
        }
    }

    pub fn new_with_event_sender_and_store(
        event_tx: Sender<ForwardEvent>,
        saved_store: SavedForwardStore,
    ) -> Self {
        Self {
            managers: Arc::new(DashMap::new()),
            event_tx: Some(event_tx),
            saved_store: Some(Arc::new(saved_store)),
        }
    }

    pub fn register(
        &self,
        session_id: impl Into<String>,
        ssh_connection: SshConnectionHandle,
    ) -> Arc<ForwardingManager> {
        let session_id = session_id.into();
        self.managers
            .entry(session_id.clone())
            .and_modify(|manager| manager.replace_ssh_connection(ssh_connection.clone()))
            .or_insert_with(|| {
                Arc::new(ForwardingManager::new_with_event_sender(
                    session_id,
                    ssh_connection,
                    self.event_tx.clone(),
                ))
            })
            .clone()
    }

    pub fn get(&self, session_id: &str) -> Option<Arc<ForwardingManager>> {
        self.managers
            .get(session_id)
            .map(|manager| manager.value().clone())
    }

    pub async fn remove(&self, session_id: &str) -> Option<Arc<ForwardingManager>> {
        let (_, manager) = self.managers.remove(session_id)?;
        manager.stop_all().await;
        Some(manager)
    }

    pub async fn suspend_session(&self, session_id: &str) -> Vec<ForwardRule> {
        let Some(manager) = self.get(session_id) else {
            return Vec::new();
        };
        manager.suspend_all_and_save_rules().await
    }

    pub async fn restore_session(
        &self,
        session_id: impl Into<String>,
        ssh_connection: SshConnectionHandle,
    ) -> Vec<Result<ForwardRule, crate::ForwardingError>> {
        let session_id = session_id.into();
        let manager = self.register(session_id, ssh_connection.clone());
        manager.restore_saved_forwards(ssh_connection).await
    }

    pub async fn stop_all(&self) {
        let managers: Vec<Arc<ForwardingManager>> = self
            .managers
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        for manager in managers {
            manager.stop_all().await;
        }
    }

    pub fn session_ids(&self) -> Vec<String> {
        let mut session_ids: Vec<String> = self
            .managers
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        session_ids.sort();
        session_ids
    }

    pub fn saved_store(&self) -> Option<Arc<SavedForwardStore>> {
        self.saved_store.clone()
    }

    pub fn sync_persisted_forward_rule(
        &self,
        forward_id: &str,
        session_id: &str,
        owner_connection_id: Option<String>,
        rule: ForwardRule,
    ) -> Result<Option<PersistedForward>, SavedForwardError> {
        let Some(store) = &self.saved_store else {
            return Ok(None);
        };
        store.sync_persisted_forward_rule(forward_id, session_id, owner_connection_id, rule)
    }

    pub fn delete_persisted_forward(&self, forward_id: &str) -> Result<(), SavedForwardError> {
        let Some(store) = &self.saved_store else {
            return Ok(());
        };
        store.delete_persisted_forward(forward_id)
    }

    pub fn update_auto_start(
        &self,
        forward_id: &str,
        auto_start: bool,
    ) -> Result<(), SavedForwardError> {
        let Some(store) = &self.saved_store else {
            return Ok(());
        };
        store.update_auto_start(forward_id, auto_start)
    }

    pub fn load_owned_forwards(&self, owner_connection_id: &str) -> Vec<PersistedForward> {
        self.saved_store
            .as_ref()
            .map(|store| store.load_owned_forwards(owner_connection_id))
            .unwrap_or_default()
    }

    pub fn load_persisted_forwards(&self, session_id: &str) -> Vec<PersistedForward> {
        self.saved_store
            .as_ref()
            .map(|store| store.load_persisted_forwards(session_id))
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
