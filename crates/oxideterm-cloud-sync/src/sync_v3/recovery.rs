// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use anyhow::{Result, bail};
use oxideterm_connections::{
    ConnectionStore, ConnectionStoreCheckpoint, PreparedProfileCredentials,
    SavedConnectionsSyncCleanup,
};
use oxideterm_forwarding::{ForwardingRegistry, SavedForwardCheckpoint};
use oxideterm_quick_commands::QuickCommandsCheckpoint;
use oxideterm_settings::{SettingsStore, SettingsStoreCheckpoint};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{LocalReplica, ReplicaStore};
use crate::plugin_settings::{self, PluginSettingsCheckpoint};

/// Written before the first domain mutation. Only ReplicaStore's authenticated
/// local container may persist this complete, potentially sensitive checkpoint.
#[derive(Serialize, Deserialize)]
pub struct RecoveryJournal {
    schema: u32,
    committed: bool,
    connections: ConnectionStoreCheckpoint,
    forwards: Option<SavedForwardCheckpoint>,
    settings: SettingsStoreCheckpoint,
    commands: QuickCommandsCheckpoint,
    plugins: PluginSettingsCheckpoint,
    replica: LocalReplica,
    created_slots: Vec<String>,
    connection_cleanup: Option<SavedConnectionsSyncCleanup>,
    credential_cleanup: Option<PreparedProfileCredentials>,
    auxiliary_before: super::auxiliary_secrets::AuxiliarySecretCheckpoint,
    managed_created: Vec<String>,
    managed_stale: Vec<String>,
    privilege_created: Vec<String>,
    privilege_stale: Vec<String>,
}

impl RecoveryJournal {
    /// Run before startup constructs consumers of settings or saved connections.
    /// Scan every target: changing the configured backend must not strand a journal.
    pub fn recover_pending(
        connections: &mut ConnectionStore,
        settings: &mut SettingsStore,
        forwards_path: &std::path::Path,
        provider: &mut impl crate::secrets::CloudSyncSecretProvider,
    ) -> Result<usize> {
        let forwards = ForwardingRegistry::new_with_store(
            oxideterm_forwarding::SavedForwardStore::load(forwards_path)?,
        );
        Self::recover_pending_on_owner(connections, settings, &forwards, provider)
    }

    pub fn recover_pending_on_owner(
        connections: &mut ConnectionStore,
        settings: &mut SettingsStore,
        forwards: &ForwardingRegistry,
        provider: &mut impl crate::secrets::CloudSyncSecretProvider,
    ) -> Result<usize> {
        let pending = Self::pending_directories(settings.path())?;
        for directory in &pending {
            let store = ReplicaStore::open(directory, provider)?;
            Self::recover(&store, connections, forwards, settings)?;
        }
        Ok(pending.len())
    }

    pub fn has_pending(settings_path: &std::path::Path) -> Result<bool> {
        Ok(!Self::pending_directories(settings_path)?.is_empty())
    }

    fn pending_directories(settings_path: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
        let root = settings_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Settings directory is unavailable"))?
            .join("cloud-sync-v3");
        let entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut pending = Vec::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir() && entry.path().join("recovery.oxide").try_exists()? {
                pending.push(entry.path());
            }
        }
        pending.sort();
        Ok(pending)
    }

    pub fn begin(
        store: &ReplicaStore,
        replica: &LocalReplica,
        connections: &ConnectionStore,
        forwards: &ForwardingRegistry,
        settings: &SettingsStore,
    ) -> Result<Self> {
        if store.read_recovery()?.is_some() {
            bail!("Cloud sync recovery must complete before applying data");
        }
        let journal = Self {
            schema: 3,
            committed: false,
            connections: connections.create_checkpoint()?,
            forwards: forwards.checkpoint_saved_forwards()?,
            settings: settings.create_checkpoint()?,
            commands: oxideterm_quick_commands::capture_checkpoint(settings.path())
                .map_err(anyhow::Error::msg)?,
            plugins: plugin_settings::checkpoint_plugin_settings(settings.path())
                .map_err(anyhow::Error::msg)?,
            replica: replica.clone(),
            created_slots: Vec::new(),
            connection_cleanup: None,
            credential_cleanup: None,
            auxiliary_before: Default::default(),
            managed_created: Vec::new(),
            managed_stale: Vec::new(),
            privilege_created: Vec::new(),
            privilege_stale: Vec::new(),
        };
        journal.persist(store)?;
        Ok(journal)
    }

    pub fn record_credential_creation(
        &mut self,
        store: &ReplicaStore,
        reference: &str,
    ) -> Result<()> {
        self.created_slots.push(reference.to_owned());
        self.persist(store)
    }

    pub(crate) fn record_managed_creation(
        &mut self,
        store: &ReplicaStore,
        reference: &str,
    ) -> Result<()> {
        self.managed_created.push(reference.into());
        self.persist(store)
    }

    pub(crate) fn defer_managed_cleanup(&mut self, reference: String) {
        self.managed_stale.push(reference);
    }
    pub(crate) fn record_privilege_creation(
        &mut self,
        store: &ReplicaStore,
        reference: &str,
    ) -> Result<()> {
        self.privilege_created.push(reference.into());
        self.persist(store)
    }
    pub(crate) fn defer_privilege_cleanup(&mut self, references: Vec<String>) {
        self.privilege_stale.extend(references);
    }

    pub(crate) fn checkpoint_auxiliary(
        &mut self,
        store: &ReplicaStore,
        incoming: &[oxideterm_connections::oxide_file::EncryptedPortableSecret],
    ) -> Result<()> {
        self.auxiliary_before = super::auxiliary_secrets::checkpoint(incoming)?;
        self.persist(store)
    }

    pub fn commit(
        &mut self,
        store: &ReplicaStore,
        next: &LocalReplica,
        connection_cleanup: Option<SavedConnectionsSyncCleanup>,
        credential_cleanup: Option<PreparedProfileCredentials>,
    ) -> Result<()> {
        self.connection_cleanup = connection_cleanup;
        self.credential_cleanup = credential_cleanup;
        self.persist(store)?;
        store.save(next)?;
        self.committed = true;
        self.persist(store)
    }

    pub fn recover(
        store: &ReplicaStore,
        connections: &mut ConnectionStore,
        forwards: &ForwardingRegistry,
        settings: &mut SettingsStore,
    ) -> Result<bool> {
        let Some(bytes) = store.read_recovery()? else {
            return Ok(false);
        };
        let mut journal: Self = rmp_serde::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("Invalid cloud sync recovery record"))?;
        if journal.schema != 3 {
            bail!("Unsupported cloud sync recovery record");
        }
        if journal.committed {
            connections.remove_synced_privilege_slots(&journal.privilege_stale)?;
            connections.remove_synced_managed_key_slots(&journal.managed_stale)?;
            if let Some(cleanup) = journal.connection_cleanup.as_mut() {
                connections.finalize_saved_connections_sync_cleanup(cleanup)?;
            }
            if let Some(cleanup) = journal.credential_cleanup.as_mut() {
                connections.commit_profile_credentials(cleanup)?;
            }
        } else {
            super::auxiliary_secrets::restore(&journal.auxiliary_before)?;
            connections.restore_checkpoint(&journal.connections)?;
            settings.restore_checkpoint(&journal.settings)?;
            if let Some(checkpoint) = &journal.forwards {
                forwards.restore_saved_forwards(checkpoint)?;
            }
            oxideterm_quick_commands::restore_checkpoint(settings.path(), &journal.commands)
                .map_err(anyhow::Error::msg)?;
            plugin_settings::restore_plugin_settings(settings.path(), &journal.plugins)
                .map_err(anyhow::Error::msg)?;
            connections.remove_staged_profile_credential_slots(&journal.created_slots)?;
            connections.remove_synced_managed_key_slots(&journal.managed_created)?;
            connections.remove_synced_privilege_slots(&journal.privilege_created)?;
            store.save(&journal.replica)?;
        }
        store.clear_recovery()?;
        Ok(true)
    }

    fn persist(&self, store: &ReplicaStore) -> Result<()> {
        let bytes = Zeroizing::new(
            rmp_serde::to_vec_named(self)
                .map_err(|_| anyhow::anyhow!("Cannot encode cloud sync recovery record"))?,
        );
        store.write_recovery(&bytes)
    }
}
