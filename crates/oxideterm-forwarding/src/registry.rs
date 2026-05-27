// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, sync::Arc, sync::mpsc::Sender};

use dashmap::DashMap;
use oxideterm_ssh::SshConnectionHandle;

use crate::{
    ApplySavedForwardsSyncSnapshotResult, ForwardEvent, ForwardRule, ForwardingError,
    ForwardingManager, OwnedForwardImportRecord, PersistedForward, PortDetectionProfiler,
    PortDetectionSnapshot, SavedForwardError, SavedForwardStore, SavedForwardsSyncSnapshot,
};

#[derive(Clone, Debug, Default)]
pub struct ForwardingRegistry {
    managers: Arc<DashMap<String, Arc<ForwardingManager>>>,
    port_profilers: Arc<DashMap<String, Arc<PortDetectionProfiler>>>,
    event_tx: Option<Sender<ForwardEvent>>,
    saved_store: Option<Arc<SavedForwardStore>>,
}

impl ForwardingRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_store(saved_store: SavedForwardStore) -> Self {
        Self {
            managers: Arc::new(DashMap::new()),
            port_profilers: Arc::new(DashMap::new()),
            event_tx: None,
            saved_store: Some(Arc::new(saved_store)),
        }
    }

    pub fn new_with_event_sender(event_tx: Sender<ForwardEvent>) -> Self {
        Self {
            managers: Arc::new(DashMap::new()),
            port_profilers: Arc::new(DashMap::new()),
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
            port_profilers: Arc::new(DashMap::new()),
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

    pub async fn register_or_rebind(
        &self,
        session_id: impl Into<String>,
        ssh_connection: SshConnectionHandle,
    ) -> (
        Arc<ForwardingManager>,
        Vec<Result<ForwardRule, ForwardingError>>,
    ) {
        let session_id = session_id.into();
        let existing_manager = self.get(&session_id);
        let previous_connection_id = existing_manager.as_ref().and_then(|manager| {
            let connection_id = manager.ssh_connection_handle().connection_id().to_string();
            (connection_id != ssh_connection.connection_id()).then_some(connection_id)
        });

        if let (Some(manager), Some(connection_id)) =
            (existing_manager.as_ref(), previous_connection_id.as_ref())
        {
            // Tauri replaces the forwarding manager with a fresh HandleController
            // after reconnect. Native keeps the manager object so GPUI state stays
            // stable, but active forward runners still captured the old SSH
            // handle. Suspend first, then recreate against the newly acquired
            // NodeRouter handle, so local/remote/dynamic forwards never keep
            // terminal-era or stale reconnect liveness.
            self.stop_port_profiler(connection_id);
            let _ = manager.suspend_all_and_save_rules().await;
        }

        let manager = self.register(session_id, ssh_connection.clone());
        let restored = if previous_connection_id.is_some() {
            manager.restore_saved_forwards(ssh_connection).await
        } else {
            Vec::new()
        };

        (manager, restored)
    }

    pub async fn register_for_reconnect_restore(
        &self,
        session_id: impl Into<String>,
        ssh_connection: SshConnectionHandle,
        expected_previous_connection_id: Option<&str>,
    ) -> Arc<ForwardingManager> {
        let session_id = session_id.into();
        if let Some(manager) = self.get(&session_id) {
            let current_connection_id = manager.ssh_connection_handle().connection_id().to_string();
            let should_replace = current_connection_id != ssh_connection.connection_id()
                && expected_previous_connection_id
                    .is_none_or(|expected| expected == current_connection_id);

            if should_replace {
                // Reconnect restore is driven by the Tauri-style snapshot phase:
                // old active forwards are destroyed with the old node state and
                // recreated through nodeCreateForward semantics below. The
                // generic rebind path intentionally preserves suspended rules
                // for UI acquisition, but using it here would resurrect stale
                // listener ids before the reconnect job generation can decide
                // whether this worker is still current.
                let _ = self.remove(&session_id).await;
            }
        }

        self.register(session_id, ssh_connection)
    }

    pub async fn remove(&self, session_id: &str) -> Option<Arc<ForwardingManager>> {
        let (_, manager) = self.managers.remove(session_id)?;
        self.stop_port_profiler(manager.ssh_connection_handle().connection_id());
        manager.stop_all().await;
        Some(manager)
    }

    pub fn start_port_profiler(
        &self,
        connection_id: impl Into<String>,
        ssh_connection: SshConnectionHandle,
    ) -> Option<Arc<PortDetectionProfiler>> {
        self.start_port_profiler_inner(connection_id, ssh_connection, false)
    }

    pub fn restart_degraded_port_profiler(
        &self,
        connection_id: impl Into<String>,
        ssh_connection: SshConnectionHandle,
    ) -> Option<Arc<PortDetectionProfiler>> {
        self.start_port_profiler_inner(connection_id, ssh_connection, true)
    }

    fn start_port_profiler_inner(
        &self,
        connection_id: impl Into<String>,
        ssh_connection: SshConnectionHandle,
        restart_degraded: bool,
    ) -> Option<Arc<PortDetectionProfiler>> {
        let event_tx = self.event_tx.clone()?;
        let connection_id = connection_id.into();
        if self
            .port_profilers
            .get(&connection_id)
            .is_some_and(|profiler| {
                profiler.is_stopped() || (restart_degraded && profiler.is_degraded())
            })
        {
            self.port_profilers.remove(&connection_id);
        }
        Some(
            self.port_profilers
                .entry(connection_id.clone())
                .or_insert_with(|| {
                    Arc::new(PortDetectionProfiler::spawn(
                        connection_id,
                        ssh_connection,
                        event_tx,
                    ))
                })
                .clone(),
        )
    }

    pub fn stop_port_profiler(&self, connection_id: &str) {
        if let Some((_, profiler)) = self.port_profilers.remove(connection_id) {
            profiler.stop();
        }
    }

    pub fn detected_ports(&self, connection_id: &str) -> Option<PortDetectionSnapshot> {
        self.port_profilers
            .get(connection_id)
            .map(|profiler| profiler.snapshot())
    }

    pub fn ignore_detected_port(&self, connection_id: &str, port: u16) {
        if let Some(profiler) = self.port_profilers.get(connection_id) {
            profiler.ignore_port(port);
        }
    }

    pub async fn suspend_session(&self, session_id: &str) -> Vec<ForwardRule> {
        let Some(manager) = self.get(session_id) else {
            return Vec::new();
        };
        manager.suspend_all_and_save_rules().await
    }

    pub async fn pause_port_forwards(&self, session_id: &str) -> Vec<ForwardRule> {
        // Tauri names this command pause_port_forwards even though the rules are
        // stored as Suspended for reconnect restoration. Preserve that command
        // vocabulary at the native API boundary.
        self.suspend_session(session_id).await
    }

    pub async fn restore_session(
        &self,
        session_id: impl Into<String>,
        ssh_connection: SshConnectionHandle,
    ) -> Vec<Result<ForwardRule, crate::ForwardingError>> {
        let (manager, mut restored) = self.register_or_rebind(session_id, ssh_connection).await;
        if restored.is_empty() {
            restored = manager
                .restore_saved_forwards(manager.ssh_connection_handle())
                .await;
        }
        restored
    }

    pub async fn restore_port_forwards(
        &self,
        session_id: impl Into<String>,
        ssh_connection: SshConnectionHandle,
    ) -> Vec<Result<ForwardRule, crate::ForwardingError>> {
        self.restore_session(session_id, ssh_connection).await
    }

    pub async fn stop_all_forwards_for_session(&self, session_id: &str) {
        if let Some(manager) = self.get(session_id) {
            manager.stop_all().await;
        }
    }

    pub async fn stop_all(&self) {
        let profilers: Vec<Arc<PortDetectionProfiler>> = self
            .port_profilers
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        for profiler in profilers {
            profiler.stop();
        }
        self.port_profilers.clear();

        let managers: Vec<Arc<ForwardingManager>> = self
            .managers
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        for manager in managers {
            manager.stop_all().await;
        }
        // Tauri's global shutdown path drains every manager and then clears the
        // registry map. Keep native from retaining stale manager -> SSH handle
        // ownership after all listeners/profilers have been stopped.
        self.managers.clear();
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

    pub fn delete_saved_forward(&self, forward_id: &str) -> Result<(), SavedForwardError> {
        self.delete_persisted_forward(forward_id)
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

    pub fn set_forward_auto_start(
        &self,
        forward_id: &str,
        auto_start: bool,
    ) -> Result<(), SavedForwardError> {
        self.update_auto_start(forward_id, auto_start)
    }

    pub fn load_owned_forwards(&self, owner_connection_id: &str) -> Vec<PersistedForward> {
        self.saved_store
            .as_ref()
            .map(|store| store.load_owned_forwards(owner_connection_id))
            .unwrap_or_default()
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

    pub fn apply_owned_forward_import_records(
        &self,
        records: &[OwnedForwardImportRecord],
        replace_owner_connection_ids: &HashSet<String>,
        merge_owner_connection_ids: &HashSet<String>,
    ) -> Result<usize, SavedForwardError> {
        let Some(store) = &self.saved_store else {
            return Ok(0);
        };
        store.apply_owned_forward_import_records(
            records,
            replace_owner_connection_ids,
            merge_owner_connection_ids,
        )
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use oxideterm_ssh::{ConnectionConsumer, SshConfig, SshConnectionRegistry};

    use super::*;

    fn test_handle(host: &str, consumer: ConnectionConsumer) -> SshConnectionHandle {
        SshConnectionRegistry::default()
            .acquire(SshConfig::password(host, 22, "tester", "pw"), consumer)
    }

    #[tokio::test]
    async fn register_or_rebind_keeps_manager_but_swaps_connection_handle() {
        let registry = ForwardingRegistry::new();
        let first = test_handle(
            "first.example",
            ConnectionConsumer::PortForward("node:a".into()),
        );
        let second = test_handle(
            "second.example",
            ConnectionConsumer::PortForward("node:a".into()),
        );
        let first_connection_id = first.connection_id().to_string();
        let second_connection_id = second.connection_id().to_string();

        let (manager, restored) = registry.register_or_rebind("node:a", first).await;
        assert!(restored.is_empty());
        assert_eq!(
            manager.ssh_connection_handle().connection_id(),
            first_connection_id
        );

        let (rebound, restored) = registry.register_or_rebind("node:a", second).await;
        assert!(restored.is_empty());
        assert!(Arc::ptr_eq(&manager, &rebound));
        assert_eq!(
            rebound.ssh_connection_handle().connection_id(),
            second_connection_id
        );
    }

    #[tokio::test]
    async fn reconnect_restore_replaces_old_manager_instead_of_auto_restoring() {
        let registry = ForwardingRegistry::new();
        let first = test_handle(
            "first.example",
            ConnectionConsumer::PortForward("node:a".into()),
        );
        let second = test_handle(
            "second.example",
            ConnectionConsumer::PortForward("node:a".into()),
        );
        let first_connection_id = first.connection_id().to_string();
        let second_connection_id = second.connection_id().to_string();

        let manager = registry.register("node:a", first);
        let restored = registry
            .register_for_reconnect_restore("node:a", second, Some(&first_connection_id))
            .await;

        assert!(!Arc::ptr_eq(&manager, &restored));
        assert_eq!(
            restored.ssh_connection_handle().connection_id(),
            second_connection_id
        );
    }

    #[tokio::test]
    async fn stop_all_clears_managers_like_tauri_shutdown() {
        let registry = ForwardingRegistry::new();
        let first = test_handle(
            "first.example",
            ConnectionConsumer::PortForward("node:a".into()),
        );
        let second = test_handle(
            "second.example",
            ConnectionConsumer::PortForward("node:b".into()),
        );

        registry.register("node:a", first);
        registry.register("node:b", second);
        assert_eq!(registry.session_ids(), vec!["node:a", "node:b"]);

        registry.stop_all().await;

        assert!(registry.session_ids().is_empty());
    }
}
