// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, HashSet};

use anyhow::{Context, Result};
use oxideterm_connections::{
    ApplySavedConnectionsSyncOutcome, ConnectionStore, RawTcpProfilesSyncSnapshot,
    RawUdpProfilesSyncSnapshot, SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
    SerialProfilesSyncSnapshot, oxide_file::EncryptedPluginSetting,
};
use oxideterm_forwarding::{
    ApplySavedForwardsSyncSnapshotResult, ForwardingRegistry, SavedForwardsSyncSnapshot,
};
use oxideterm_settings::{SettingsStore, export_oxide_settings_snapshot_json};
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
    let connections = if let Some(snapshot) = connections_snapshot {
        Some(connection_store.apply_saved_connections_snapshot(snapshot, conflict_strategy)?)
    } else {
        None
    };

    if let Some(outcome) = connections.as_ref() {
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
        Some(
            forwarding_registry
                .apply_saved_forwards_snapshot(snapshot, &valid_owner_connection_ids)
                .map_err(anyhow::Error::msg)?,
        )
    } else {
        None
    };

    let quick_commands_applied = if let Some(snapshot_json) = quick_commands_snapshot_json {
        let result = oxideterm_quick_commands::apply_snapshot_json(
            settings_store.path(),
            &snapshot_json,
            oxideterm_quick_commands::QuickCommandImportStrategy::Merge,
        );
        if !result.errors.is_empty() {
            anyhow::bail!(
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

    let mut app_settings_applied = 0usize;
    for (section_id, snapshot_json) in app_settings_snapshots {
        let selected = HashSet::from([section_id]);
        let next = oxideterm_settings::merge_oxide_settings_snapshot(
            settings_store.settings(),
            &snapshot_json,
            Some(&selected),
        )?;
        settings_store.replace_and_save(next)?;
        app_settings_applied += 1;
    }

    let plugin_settings_applied =
        plugin_settings::upsert_plugin_settings(settings_store.path(), &plugin_settings_snapshot)
            .map_err(anyhow::Error::msg)?;

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
) -> Result<Value> {
    let provider_ids = ai_providers
        .iter()
        .filter_map(|provider| provider.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    Ok(json!({
        "connectionsRevision": connections_snapshot.revision,
        "aiProviderIds": provider_ids,
    }))
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
