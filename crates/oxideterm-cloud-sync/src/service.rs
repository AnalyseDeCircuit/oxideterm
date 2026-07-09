// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, HashSet};

use anyhow::{Context, Result, bail};
use chrono::DateTime;
use oxideterm_connections::{
    ApplySavedConnectionsSyncOutcome, ConnectionStore, ManagedSshKeyInfo,
    RawTcpProfilesSyncSnapshot, RawUdpProfilesSyncSnapshot, SavedConnectionsConflictStrategy,
    SavedConnectionsSyncSnapshot, SerialProfilesSyncSnapshot, oxide_file::EncryptedPluginSetting,
};
use oxideterm_forwarding::{
    ApplySavedForwardsSyncSnapshotResult, ForwardType, ForwardingRegistry,
    SavedForwardsSyncSnapshot,
};
use oxideterm_quick_commands::QuickCommandsSnapshot;
use oxideterm_settings::{PersistedSettings, SettingsStore, export_oxide_settings_snapshot_json};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    LocalSyncMetadata, OXIDE_APP_SETTINGS_SECTION_IDS, RawSyncScope, StructuredDirtyInfo,
    StructuredLocalState, SyncScope, compute_structured_dirty_sections,
    count_structured_upload_plan_units, normalize_sync_scope, plugin_settings,
};

#[derive(Clone, Debug)]
pub struct CloudSyncLocalSnapshot {
    pub metadata: LocalSyncMetadata,
    pub scope: SyncScope,
    pub dirty: StructuredDirtyInfo,
    pub upload_units: usize,
    pub connections_record_count: usize,
    pub forwards_record_count: usize,
    pub quick_commands_record_count: usize,
    pub serial_profiles_record_count: usize,
    pub raw_tcp_profiles_record_count: usize,
    pub raw_udp_profiles_record_count: usize,
    pub sensitive_credentials_record_count: usize,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct CloudSyncApplyOutcome {
    pub connections: Option<ApplySavedConnectionsSyncOutcome>,
    pub forwards: Option<ApplySavedForwardsSyncSnapshotResult>,
    pub quick_commands_applied: usize,
    pub serial_profiles_applied: usize,
    pub raw_tcp_profiles_applied: usize,
    pub raw_udp_profiles_applied: usize,
    pub app_settings_applied: usize,
    pub plugin_settings_applied: usize,
}

pub fn build_local_snapshot(
    connection_store: &ConnectionStore,
    forwarding_registry: &ForwardingRegistry,
    settings_store: &SettingsStore,
    baseline_state: Option<&StructuredLocalState>,
    raw_scope: Option<&RawSyncScope>,
) -> Result<CloudSyncLocalSnapshot> {
    let available_plugin_ids = plugin_settings::plugin_settings_revision_map(settings_store.path())
        .map_err(anyhow::Error::msg)?
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let scope = normalize_sync_scope(raw_scope, &available_plugin_ids);

    let connections_snapshot = connection_store.export_saved_connections_snapshot()?;
    let forwards_snapshot = forwarding_registry.export_saved_forwards_snapshot()?;
    let quick_commands_json = oxideterm_quick_commands::export_snapshot_json(settings_store.path())
        .map_err(anyhow::Error::msg)?;
    let quick_commands_snapshot: oxideterm_quick_commands::QuickCommandsSnapshot =
        serde_json::from_str(&quick_commands_json)
            .context("failed to decode quick commands snapshot")?;
    let serial_profiles_snapshot = connection_store.export_serial_profiles_snapshot()?;
    let raw_tcp_profiles_snapshot = connection_store.export_raw_tcp_profiles_snapshot()?;
    let raw_udp_profiles_snapshot = connection_store.export_raw_udp_profiles_snapshot()?;
    let app_settings_section_revisions =
        build_app_settings_section_revision_map(settings_store, &scope)?;
    let plugin_settings_revisions =
        plugin_settings::plugin_settings_revision_map(settings_store.path())
            .map_err(anyhow::Error::msg)?;
    let syncable_settings_payload = build_syncable_settings_payload(settings_store);
    let sensitive_credentials_revision =
        tauri_simple_stable_hash(&build_sensitive_credentials_revision_payload(
            &connections_snapshot,
            &settings_store.settings().ai.providers,
            &referenced_managed_key_revision_payload(connection_store, &connections_snapshot),
        )?)?;

    let metadata = LocalSyncMetadata {
        saved_connections_revision: Some(connections_snapshot.revision.clone()),
        saved_forwards_revision: Some(forwards_snapshot.revision.clone()),
        quick_commands_revision: Some(tauri_simple_stable_hash(&quick_commands_json)?),
        serial_profiles_revision: Some(serial_profiles_snapshot.revision.clone()),
        raw_tcp_profiles_revision: Some(raw_tcp_profiles_snapshot.revision.clone()),
        raw_udp_profiles_revision: Some(raw_udp_profiles_snapshot.revision.clone()),
        sensitive_credentials_revision: Some(sensitive_credentials_revision),
        settings_revision: Some(tauri_simple_stable_hash(&syncable_settings_payload)?),
        app_settings_section_revisions,
        plugin_settings_revisions,
    };
    let dirty = compute_structured_dirty_sections(&metadata, baseline_state, &scope);
    let upload_units = count_structured_upload_plan_units(&metadata, &scope);

    Ok(CloudSyncLocalSnapshot {
        metadata,
        scope,
        dirty,
        upload_units,
        connections_record_count: connections_snapshot.records.len(),
        forwards_record_count: forwards_snapshot.records.len(),
        quick_commands_record_count: quick_commands_snapshot.commands.len(),
        serial_profiles_record_count: serial_profiles_snapshot.records.len(),
        raw_tcp_profiles_record_count: raw_tcp_profiles_snapshot.records.len(),
        raw_udp_profiles_record_count: raw_udp_profiles_snapshot.records.len(),
        sensitive_credentials_record_count: connections_snapshot.records.len(),
    })
}

#[allow(dead_code)]
pub fn apply_structured_snapshots(
    connection_store: &mut ConnectionStore,
    forwarding_registry: &ForwardingRegistry,
    settings_store: &mut SettingsStore,
    connections_snapshot: Option<SavedConnectionsSyncSnapshot>,
    forwards_snapshot: Option<SavedForwardsSyncSnapshot>,
    quick_commands_snapshot_json: Option<String>,
    serial_profiles_snapshot: Option<SerialProfilesSyncSnapshot>,
    raw_tcp_profiles_snapshot: Option<RawTcpProfilesSyncSnapshot>,
    raw_udp_profiles_snapshot: Option<RawUdpProfilesSyncSnapshot>,
    app_settings_snapshots: BTreeMap<String, String>,
    plugin_settings_snapshot: Vec<EncryptedPluginSetting>,
    conflict_strategy: SavedConnectionsConflictStrategy,
) -> Result<CloudSyncApplyOutcome> {
    // Validate every independently supplied resource before the coordinated
    // transaction captures owner checkpoints and performs its first write.
    let staged_app_settings = preflight_structured_snapshots(
        settings_store,
        forwards_snapshot.as_ref(),
        quick_commands_snapshot_json.as_deref(),
        serial_profiles_snapshot.as_ref(),
        raw_tcp_profiles_snapshot.as_ref(),
        raw_udp_profiles_snapshot.as_ref(),
        &app_settings_snapshots,
        &plugin_settings_snapshot,
    )?;

    // Capture every owner before the first write. The connection checkpoint is
    // always required because profile-only sync still mutates ConnectionStore.
    let settings_path = settings_store.path().to_path_buf();
    let connection_checkpoint = connection_store
        .create_checkpoint()
        .context("failed to checkpoint connection store before cloud sync apply")?;
    let forwards_checkpoint = forwarding_registry
        .checkpoint_saved_forwards()
        .map_err(anyhow::Error::msg)
        .context("failed to checkpoint saved forwards before cloud sync apply")?;
    let quick_commands_checkpoint = oxideterm_quick_commands::capture_checkpoint(&settings_path)
        .map_err(anyhow::Error::msg)
        .context("failed to checkpoint Quick Commands before cloud sync apply")?;
    let plugin_settings_checkpoint = plugin_settings::checkpoint_plugin_settings(&settings_path)
        .map_err(anyhow::Error::msg)
        .context("failed to checkpoint plugin settings before cloud sync apply")?;
    let settings_checkpoint = settings_store
        .create_checkpoint()
        .context("failed to checkpoint app settings before cloud sync apply")?;

    let mut prepared_connections = None;
    let mut forwards_attempted = false;
    let mut quick_commands_attempted = false;
    let mut settings_applied = false;
    let mut plugin_settings_attempted = false;

    let apply_result = (|| -> Result<CloudSyncApplyOutcome> {
        let connections = if let Some(snapshot) = connections_snapshot {
            let prepared = connection_store
                .prepare_saved_connections_snapshot(snapshot, conflict_strategy)
                .context("failed to prepare saved connections cloud sync")?;
            let outcome = prepared.outcome().clone();
            prepared_connections = Some(prepared);
            Some(outcome)
        } else {
            None
        };

        if let Some(outcome) = connections.as_ref() {
            forwards_attempted = true;
            for connection_id in &outcome.deleted_connection_ids {
                forwarding_registry
                    .delete_owned_forwards(connection_id)
                    .map_err(anyhow::Error::msg)?;
            }
        }

        let valid_owner_connection_ids = connection_store
            .connections()
            .iter()
            .map(|connection| connection.id.clone())
            .collect::<HashSet<_>>();
        let forwards = if let Some(snapshot) = forwards_snapshot {
            forwards_attempted = true;
            Some(
                forwarding_registry
                    .apply_saved_forwards_snapshot(snapshot, &valid_owner_connection_ids)
                    .map_err(anyhow::Error::msg)?,
            )
        } else {
            None
        };

        let quick_commands_applied = if let Some(snapshot_json) = quick_commands_snapshot_json {
            quick_commands_attempted = true;
            let result = oxideterm_quick_commands::apply_snapshot_json(
                &settings_path,
                &snapshot_json,
                oxideterm_quick_commands::QuickCommandImportStrategy::Merge,
            );
            if !result.errors.is_empty() {
                bail!(
                    "failed to apply quick commands snapshot: {}",
                    result.errors.join("; ")
                );
            }
            result.imported
        } else {
            0
        };

        let serial_profiles_applied = if let Some(snapshot) = serial_profiles_snapshot {
            connection_store.apply_serial_profiles_snapshot(snapshot)?
        } else {
            0
        };
        let raw_tcp_profiles_applied = if let Some(snapshot) = raw_tcp_profiles_snapshot {
            connection_store.apply_raw_tcp_profiles_snapshot(snapshot)?
        } else {
            0
        };
        let raw_udp_profiles_applied = if let Some(snapshot) = raw_udp_profiles_snapshot {
            connection_store.apply_raw_udp_profiles_snapshot(snapshot)?
        } else {
            0
        };
        fail_structured_apply_after(StructuredApplyStage::Profiles)?;

        let app_settings_applied = app_settings_snapshots.len();
        if let Some(next) = staged_app_settings {
            // SettingsStore changes memory only after its durable swap succeeds.
            settings_store.replace_and_save(next)?;
            settings_applied = true;
        }
        fail_structured_apply_after(StructuredApplyStage::Settings)?;

        plugin_settings_attempted = true;
        let plugin_settings_applied =
            plugin_settings::upsert_plugin_settings(&settings_path, &plugin_settings_snapshot)
                .map_err(anyhow::Error::msg)?;
        fail_structured_apply_after(StructuredApplyStage::PluginSettings)?;

        Ok(CloudSyncApplyOutcome {
            connections,
            forwards,
            quick_commands_applied,
            serial_profiles_applied,
            raw_tcp_profiles_applied,
            raw_udp_profiles_applied,
            app_settings_applied,
            plugin_settings_applied,
        })
    })();

    let outcome = match apply_result {
        Ok(outcome) => outcome,
        Err(error) => {
            let rollback_errors = rollback_structured_apply(
                connection_store,
                forwarding_registry,
                settings_store,
                &settings_path,
                &connection_checkpoint,
                forwards_checkpoint.as_ref(),
                &quick_commands_checkpoint,
                &plugin_settings_checkpoint,
                &settings_checkpoint,
                forwards_attempted,
                quick_commands_attempted,
                settings_applied,
                plugin_settings_attempted,
            );
            return Err(cloud_sync_transaction_error(error, rollback_errors));
        }
    };

    if let Some(prepared) = prepared_connections {
        let mut cleanup =
            match connection_store.commit_prepared_saved_connections_snapshot(prepared) {
                Ok(cleanup) => cleanup,
                Err(error) => {
                    let rollback_errors = rollback_structured_apply(
                        connection_store,
                        forwarding_registry,
                        settings_store,
                        &settings_path,
                        &connection_checkpoint,
                        forwards_checkpoint.as_ref(),
                        &quick_commands_checkpoint,
                        &plugin_settings_checkpoint,
                        &settings_checkpoint,
                        forwards_attempted,
                        quick_commands_attempted,
                        settings_applied,
                        plugin_settings_attempted,
                    );
                    return Err(cloud_sync_transaction_error(
                        error.context("failed to commit prepared saved connections cloud sync"),
                        rollback_errors,
                    ));
                }
            };

        // Cleanup is intentionally outside the rollback boundary: all data is
        // committed, and stale credentials are harmless if deletion fails.
        if connection_store
            .finalize_saved_connections_sync_cleanup(&mut cleanup)
            .is_err()
            && connection_store
                .finalize_saved_connections_sync_cleanup(&mut cleanup)
                .is_err()
        {
            // The synchronized data is already committed, so housekeeping must
            // not make the operation look failed and trigger a duplicate apply.
            // A future cleanup queue should persist this retry state across runs.
            eprintln!(
                "warning: cloud sync committed, but {} stale keychain entries remain after cleanup retry",
                cleanup.pending_keychain_entries()
            );
        }
    }

    Ok(outcome)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StructuredApplyStage {
    Profiles,
    Settings,
    PluginSettings,
}

#[cfg(test)]
thread_local! {
    static FAIL_STRUCTURED_APPLY_AFTER: std::cell::Cell<Option<StructuredApplyStage>> = const {
        std::cell::Cell::new(None)
    };
}

fn fail_structured_apply_after(stage: StructuredApplyStage) -> Result<()> {
    #[cfg(test)]
    if FAIL_STRUCTURED_APPLY_AFTER.with(|failure| {
        let matches = failure.get() == Some(stage);
        if matches {
            failure.set(None);
        }
        matches
    }) {
        bail!("injected cloud sync apply failure after {stage:?}");
    }
    #[cfg(not(test))]
    let _ = stage;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn rollback_structured_apply(
    connection_store: &mut ConnectionStore,
    forwarding_registry: &ForwardingRegistry,
    settings_store: &mut SettingsStore,
    settings_path: &std::path::Path,
    connection_checkpoint: &oxideterm_connections::ConnectionStoreCheckpoint,
    forwards_checkpoint: Option<&oxideterm_forwarding::SavedForwardCheckpoint>,
    quick_commands_checkpoint: &oxideterm_quick_commands::QuickCommandsCheckpoint,
    plugin_settings_checkpoint: &plugin_settings::PluginSettingsCheckpoint,
    settings_checkpoint: &oxideterm_settings::SettingsStoreCheckpoint,
    forwards_attempted: bool,
    quick_commands_attempted: bool,
    settings_applied: bool,
    plugin_settings_attempted: bool,
) -> Vec<String> {
    let mut errors = Vec::new();

    if plugin_settings_attempted
        && let Err(error) =
            plugin_settings::restore_plugin_settings(settings_path, plugin_settings_checkpoint)
    {
        errors.push(format!("plugin settings restore failed: {error}"));
    }
    if settings_applied && let Err(error) = settings_store.restore_checkpoint(settings_checkpoint) {
        errors.push(format!("app settings restore failed: {error}"));
    }
    if quick_commands_attempted
        && let Err(error) =
            oxideterm_quick_commands::restore_checkpoint(settings_path, quick_commands_checkpoint)
    {
        errors.push(format!("Quick Commands restore failed: {error}"));
    }
    if forwards_attempted
        && let Some(checkpoint) = forwards_checkpoint
        && let Err(error) = forwarding_registry.restore_saved_forwards(checkpoint)
    {
        errors.push(format!("saved forwards restore failed: {error}"));
    }
    if let Err(error) = connection_store.restore_checkpoint(connection_checkpoint) {
        errors.push(format!("connection store restore failed: {error}"));
    }

    errors
}

fn cloud_sync_transaction_error(
    error: anyhow::Error,
    rollback_errors: Vec<String>,
) -> anyhow::Error {
    if rollback_errors.is_empty() {
        error.context("cloud sync apply failed; all modified stores were restored")
    } else {
        anyhow::anyhow!(
            "cloud sync apply failed: {error:#}; rollback also failed: {}",
            rollback_errors.join("; ")
        )
    }
}

fn preflight_structured_snapshots(
    settings_store: &SettingsStore,
    forwards_snapshot: Option<&SavedForwardsSyncSnapshot>,
    quick_commands_snapshot_json: Option<&str>,
    serial_profiles_snapshot: Option<&SerialProfilesSyncSnapshot>,
    raw_tcp_profiles_snapshot: Option<&RawTcpProfilesSyncSnapshot>,
    raw_udp_profiles_snapshot: Option<&RawUdpProfilesSyncSnapshot>,
    app_settings_snapshots: &BTreeMap<String, String>,
    plugin_settings_snapshot: &[EncryptedPluginSetting],
) -> Result<Option<PersistedSettings>> {
    if let Some(snapshot) = forwards_snapshot {
        validate_forwards_snapshot(snapshot)?;
    }
    if let Some(snapshot_json) = quick_commands_snapshot_json {
        let incoming: QuickCommandsSnapshot = serde_json::from_str(snapshot_json)
            .context("failed to decode quick commands snapshot")?;
        let supported_version = oxideterm_quick_commands::load_snapshot(settings_store.path())
            .map_err(anyhow::Error::msg)?
            .version;
        if incoming.version != supported_version {
            bail!(
                "unsupported quick commands snapshot version {}",
                incoming.version
            );
        }
    }
    for profile in serial_profiles_snapshot
        .into_iter()
        .flat_map(|snapshot| &snapshot.records)
    {
        profile.validate()?;
    }
    for profile in raw_tcp_profiles_snapshot
        .into_iter()
        .flat_map(|snapshot| &snapshot.records)
    {
        profile.validate()?;
    }
    for profile in raw_udp_profiles_snapshot
        .into_iter()
        .flat_map(|snapshot| &snapshot.records)
    {
        profile.validate()?;
    }

    let mut staged_settings = settings_store.settings().clone();
    for (section_id, snapshot_json) in app_settings_snapshots {
        let selected = HashSet::from([section_id.clone()]);
        staged_settings = oxideterm_settings::merge_oxide_settings_snapshot(
            &staged_settings,
            snapshot_json,
            Some(&selected),
        )?;
    }

    if !plugin_settings_snapshot.is_empty() {
        // Reading the current plugin file before any mutation prevents a late
        // parse failure from following successful connection/profile writes.
        plugin_settings::load_plugin_settings(settings_store.path()).map_err(anyhow::Error::msg)?;
    }

    Ok((!app_settings_snapshots.is_empty()).then_some(staged_settings))
}

fn validate_forwards_snapshot(snapshot: &SavedForwardsSyncSnapshot) -> Result<()> {
    for record in &snapshot.records {
        DateTime::parse_from_rfc3339(&record.updated_at)
            .with_context(|| format!("invalid saved forward updated_at '{}'", record.updated_at))?;
        if record.deleted {
            continue;
        }
        let Some(payload) = record.payload.as_ref() else {
            continue;
        };
        DateTime::parse_from_rfc3339(&payload.created_at).with_context(|| {
            format!("invalid saved forward created_at '{}'", payload.created_at)
        })?;
        ForwardType::try_from_tauri_str(&payload.forward_type).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

fn build_app_settings_section_revision_map(
    settings_store: &SettingsStore,
    scope: &SyncScope,
) -> Result<BTreeMap<String, String>> {
    let mut revisions = BTreeMap::new();
    for section_id in OXIDE_APP_SETTINGS_SECTION_IDS {
        let section_id = (*section_id).to_string();
        let selected = HashSet::from([section_id.clone()]);
        let snapshot = export_oxide_settings_snapshot_json(
            settings_store.settings(),
            Some(&selected),
            scope.include_local_terminal_env_vars,
        )
        .with_context(|| format!("failed to export app settings section {section_id}"))?;
        revisions.insert(section_id, tauri_simple_stable_hash(&snapshot)?);
    }
    Ok(revisions)
}

fn build_syncable_settings_payload(settings_store: &SettingsStore) -> Value {
    let settings = settings_store.settings();
    json!({
        "appearance": {
            "language": settings.general.language,
            "uiDensity": settings.appearance.ui_density,
        },
        "terminal": {
            "fontSize": settings.terminal.font_size,
            "theme": settings.terminal.theme,
        },
        "reconnect": {
            "autoReconnect": settings.reconnect.enabled,
        },
    })
}

fn build_sensitive_credentials_revision_payload(
    connections_snapshot: &SavedConnectionsSyncSnapshot,
    ai_providers: &[Value],
    managed_keys: &[Value],
) -> Result<Value> {
    let provider_ids = ai_providers
        .iter()
        .filter_map(|provider| provider.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    Ok(json!({
        "connectionsRevision": connections_snapshot.revision,
        "aiProviderIds": provider_ids,
        "managedKeys": managed_keys,
    }))
}

fn referenced_managed_key_revision_payload(
    connection_store: &ConnectionStore,
    connections_snapshot: &SavedConnectionsSyncSnapshot,
) -> Vec<Value> {
    let referenced_ids = connections_snapshot
        .records
        .iter()
        .filter_map(|record| record.payload.as_ref())
        .flat_map(|payload| {
            std::iter::once(payload.managed_key_id.as_ref()).chain(
                payload
                    .proxy_chain
                    .iter()
                    .map(|hop| hop.managed_key_id.as_ref()),
            )
        })
        .flatten()
        .cloned()
        .collect::<HashSet<_>>();
    let mut managed_keys = connection_store
        .managed_ssh_keys()
        .into_iter()
        .filter(|key| referenced_ids.contains(&key.id))
        .collect::<Vec<_>>();
    managed_keys.sort_by(|left, right| left.id.cmp(&right.id));
    managed_keys
        .into_iter()
        .map(managed_key_revision_payload)
        .collect()
}

fn managed_key_revision_payload(key: ManagedSshKeyInfo) -> Value {
    json!({
        "id": key.id,
        "name": key.name,
        "fingerprint": key.fingerprint,
        "publicKey": key.public_key,
        "requiresPassphrase": key.requires_passphrase,
        "origin": key.origin,
        "updatedAt": key.updated_at,
    })
}

fn tauri_simple_stable_hash<T: Serialize>(value: &T) -> Result<String> {
    let text = serde_json::to_string(value).context("failed to serialize stable hash value")?;
    Ok(tauri_fnv1a_stable_hash_text(&text))
}

fn tauri_fnv1a_stable_hash_text(text: &str) -> String {
    let mut hash = 2166136261u32;
    for code_unit in text.encode_utf16() {
        hash ^= u32::from(code_unit);
        hash = hash.wrapping_mul(16777619);
    }
    format!("fnv1a-{hash:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use oxideterm_connections::{
        SaveConnectionRequest, SavedAuth, SavedUpstreamProxyPolicy, SerialProfile,
    };

    fn temp_path(name: &str, file_name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!(
                "oxideterm-cloud-sync-{name}-{}",
                uuid::Uuid::new_v4()
            ))
            .join(file_name)
    }

    fn set_failure_after(stage: StructuredApplyStage) {
        FAIL_STRUCTURED_APPLY_AFTER.with(|failure| failure.set(Some(stage)));
    }

    fn empty_apply_arguments(
        connection_store: &mut ConnectionStore,
        forwarding_registry: &ForwardingRegistry,
        settings_store: &mut SettingsStore,
        serial_profiles_snapshot: Option<SerialProfilesSyncSnapshot>,
        app_settings_snapshots: BTreeMap<String, String>,
        plugin_settings_snapshot: Vec<EncryptedPluginSetting>,
        quick_commands_snapshot_json: Option<String>,
    ) -> Result<CloudSyncApplyOutcome> {
        apply_structured_snapshots(
            connection_store,
            forwarding_registry,
            settings_store,
            None,
            None,
            quick_commands_snapshot_json,
            serial_profiles_snapshot,
            None,
            None,
            app_settings_snapshots,
            plugin_settings_snapshot,
            SavedConnectionsConflictStrategy::Replace,
        )
    }

    #[test]
    fn invalid_late_resource_does_not_commit_connections() {
        let source_path = std::env::temp_dir().join(format!(
            "oxideterm-cloud-sync-source-{}.json",
            uuid::Uuid::new_v4()
        ));
        let mut source = ConnectionStore::load(source_path).unwrap();
        source
            .upsert(SaveConnectionRequest {
                id: Some("conn-1".to_string()),
                name: "Production".to_string(),
                group: None,
                host: "example.test".to_string(),
                port: 22,
                username: "ops".to_string(),
                auth: SavedAuth::Agent,
                proxy_chain: Vec::new(),
                upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
                color: None,
                icon: None,
                tags: Vec::new(),
                agent_forwarding: false,
                legacy_ssh_compatibility: false,
                post_connect_command: None,
            })
            .unwrap();
        let connections_snapshot = source.export_saved_connections_snapshot().unwrap();

        let target_path = std::env::temp_dir().join(format!(
            "oxideterm-cloud-sync-target-{}.json",
            uuid::Uuid::new_v4()
        ));
        let mut target = ConnectionStore::load(&target_path).unwrap();
        let forwarding_registry = ForwardingRegistry::new();
        let settings_path = std::env::temp_dir().join(format!(
            "oxideterm-cloud-sync-settings-{}.json",
            uuid::Uuid::new_v4()
        ));
        let mut settings_store =
            SettingsStore::from_read_only(settings_path, PersistedSettings::default());

        let result = apply_structured_snapshots(
            &mut target,
            &forwarding_registry,
            &mut settings_store,
            Some(connections_snapshot),
            None,
            Some("{".to_string()),
            None,
            None,
            None,
            BTreeMap::new(),
            Vec::new(),
            SavedConnectionsConflictStrategy::Replace,
        );

        assert!(result.is_err());
        assert!(target.connections().is_empty());
        assert!(
            ConnectionStore::load(target_path)
                .unwrap()
                .connections()
                .is_empty()
        );
    }

    #[test]
    fn profile_stage_failure_restores_missing_connection_store() {
        let target_path = temp_path("profile-rollback", "connections.json");
        let mut target = ConnectionStore::load(&target_path).unwrap();
        let forwarding_registry = ForwardingRegistry::new();
        let settings_path = temp_path("profile-rollback", "settings.json");
        let mut settings_store = SettingsStore::load_from_path(&settings_path, None).unwrap();
        let profiles = SerialProfilesSyncSnapshot {
            revision: "profile-revision".to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            records: vec![SerialProfile::new("Console", "/dev/ttyUSB0")],
        };

        set_failure_after(StructuredApplyStage::Profiles);
        let error = empty_apply_arguments(
            &mut target,
            &forwarding_registry,
            &mut settings_store,
            Some(profiles),
            BTreeMap::new(),
            Vec::new(),
            None,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("all modified stores were restored")
        );
        assert!(target.serial_profiles().is_empty());
        assert!(!target_path.exists());
    }

    #[test]
    fn settings_stage_failure_restores_settings_and_missing_quick_commands_file() {
        let target_path = temp_path("settings-rollback", "connections.json");
        let mut target = ConnectionStore::load(target_path).unwrap();
        let forwarding_registry = ForwardingRegistry::new();
        let settings_path = temp_path("settings-rollback", "settings.json");
        let mut settings_store = SettingsStore::load_from_path(&settings_path, None).unwrap();
        settings_store.save().unwrap();
        let previous_settings = settings_store.settings().clone();
        let mut incoming_settings = previous_settings.clone();
        incoming_settings.terminal.font_size += 3;
        let section_id = crate::APP_SECTION_TERMINAL_APPEARANCE.to_string();
        let selected = HashSet::from([section_id.clone()]);
        let app_snapshot =
            export_oxide_settings_snapshot_json(&incoming_settings, Some(&selected), false)
                .unwrap();
        let app_snapshots = BTreeMap::from([(section_id, app_snapshot)]);
        let quick_snapshot = serde_json::to_string(
            &oxideterm_quick_commands::load_snapshot(&settings_path).unwrap(),
        )
        .unwrap();
        let quick_commands_path = oxideterm_quick_commands::quick_commands_path(&settings_path);
        assert!(!quick_commands_path.exists());

        set_failure_after(StructuredApplyStage::Settings);
        empty_apply_arguments(
            &mut target,
            &forwarding_registry,
            &mut settings_store,
            None,
            app_snapshots,
            Vec::new(),
            Some(quick_snapshot),
        )
        .unwrap_err();

        assert_eq!(settings_store.settings(), &previous_settings);
        assert!(!quick_commands_path.exists());
        assert_eq!(
            SettingsStore::load_from_path(&settings_path, None)
                .unwrap()
                .settings(),
            &previous_settings
        );
    }

    #[test]
    fn plugin_stage_failure_restores_missing_plugin_settings_file() {
        let target_path = temp_path("plugin-rollback", "connections.json");
        let mut target = ConnectionStore::load(target_path).unwrap();
        let forwarding_registry = ForwardingRegistry::new();
        let settings_path = temp_path("plugin-rollback", "settings.json");
        let mut settings_store = SettingsStore::load_from_path(&settings_path, None).unwrap();
        let plugin_settings_path = plugin_settings::plugin_settings_path(&settings_path);
        assert!(!plugin_settings_path.exists());
        let incoming = vec![EncryptedPluginSetting {
            storage_key: "oxide-plugin-example-setting-token".to_string(),
            serialized_value: "encrypted-test-value".to_string(),
        }];

        set_failure_after(StructuredApplyStage::PluginSettings);
        empty_apply_arguments(
            &mut target,
            &forwarding_registry,
            &mut settings_store,
            None,
            BTreeMap::new(),
            incoming,
            None,
        )
        .unwrap_err();

        assert!(!plugin_settings_path.exists());
        assert!(
            plugin_settings::load_plugin_settings(&settings_path)
                .unwrap()
                .is_empty()
        );
        assert!(
            fs::read_dir(settings_path.parent().unwrap())
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".tmp"))
        );
    }
}
