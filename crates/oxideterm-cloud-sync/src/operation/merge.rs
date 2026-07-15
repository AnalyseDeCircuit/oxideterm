// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn upload_error_after_revision(
    error: impl std::fmt::Display,
    revision_sequence: u64,
) -> CloudSyncUploadError {
    CloudSyncUploadError {
        message: error.to_string(),
        remote_metadata: None,
        revision_sequence_consumed: Some(revision_sequence),
    }
}

pub(super) async fn read_optional_snapshot_at_revision<T>(
    service: &CloudSyncOperationService,
    settings: &CloudSyncSettings,
    secrets: &crate::secrets::CloudSyncSecrets,
    revision: Option<&str>,
    path_for_revision: impl FnOnce(&str) -> String,
) -> Result<Option<T>>
where
    T: DeserializeOwned,
{
    let Some(revision) = revision.filter(|revision| !revision.is_empty()) else {
        return Ok(None);
    };
    let path = path_for_revision(revision);
    service
        .read_optional_object(settings, secrets, &path)
        .await?
        .map(|object| serde_json::from_slice(&object.bytes).map_err(anyhow::Error::from))
        .transpose()
}

pub(super) async fn read_optional_text_at_revision(
    service: &CloudSyncOperationService,
    settings: &CloudSyncSettings,
    secrets: &crate::secrets::CloudSyncSecrets,
    revision: Option<&str>,
    path_for_revision: impl FnOnce(&str) -> String,
) -> Result<Option<String>> {
    let Some(revision) = revision.filter(|revision| !revision.is_empty()) else {
        return Ok(None);
    };
    let path = path_for_revision(revision);
    service
        .read_optional_object(settings, secrets, &path)
        .await?
        .map(|object| String::from_utf8(object.bytes).map_err(anyhow::Error::from))
        .transpose()
}

pub(super) fn merge_structured_preview_fields(
    connection_store: &ConnectionStore,
    forwarding_registry: &ForwardingRegistry,
    settings_store: &SettingsStore,
    preview: &mut StructuredPreview,
    selection: &StructuredApplySelection,
    conflict_strategy: &ConflictStrategy,
) -> Result<bool> {
    let now_rfc3339 = Utc::now().to_rfc3339();
    let mut changed = false;
    if selection.connections
        && let (Some(remote), Some(base)) = (
            preview.connections_snapshot.as_mut(),
            preview.base_connections_snapshot.as_ref(),
        )
    {
        let local = connection_store.export_saved_connections_snapshot()?;
        changed |= merge_connection_records(remote, base, &local, conflict_strategy, &now_rfc3339)?;
    }
    if selection.forwards
        && let (Some(remote), Some(base)) = (
            preview.forwards_snapshot.as_mut(),
            preview.base_forwards_snapshot.as_ref(),
        )
    {
        let local = forwarding_registry.export_saved_forwards_snapshot()?;
        changed |= merge_forward_records(remote, base, &local, conflict_strategy, &now_rfc3339)?;
    }
    if selection.quick_commands
        && let (Some(remote_json), Some(base_json)) = (
            preview.quick_commands_snapshot_json.as_mut(),
            preview.base_quick_commands_snapshot_json.as_deref(),
        )
    {
        let local_json = oxideterm_quick_commands::export_snapshot_json(settings_store.path())
            .map_err(anyhow::Error::msg)?;
        changed |= merge_quick_command_records(
            remote_json,
            base_json,
            &local_json,
            conflict_strategy,
            Utc::now().timestamp_millis().max(0) as u64,
        )?;
    }
    if selection.serial_profiles
        && let (Some(remote), Some(base)) = (
            preview.serial_profiles_snapshot.as_mut(),
            preview.base_serial_profiles_snapshot.as_ref(),
        )
    {
        let local = connection_store.export_serial_profiles_snapshot()?;
        changed |=
            merge_serial_profile_records(remote, base, &local, conflict_strategy, Utc::now())?;
    }
    Ok(changed)
}

pub(super) fn merge_connection_records(
    remote: &mut SavedConnectionsSyncSnapshot,
    base: &SavedConnectionsSyncSnapshot,
    local: &SavedConnectionsSyncSnapshot,
    conflict_strategy: &ConflictStrategy,
    merged_at: &str,
) -> Result<bool> {
    let base_records = sync_records_by_id(&base.records);
    let local_records = sync_records_by_id(&local.records);
    let mut changed = false;
    for remote_record in &mut remote.records {
        if remote_record.deleted {
            continue;
        }
        let Some(remote_payload) = remote_record.payload.as_ref() else {
            continue;
        };
        let Some(base_record) = base_records
            .get(remote_record.id.as_str())
            .filter(|record| !record.deleted)
        else {
            continue;
        };
        let Some(local_record) = local_records
            .get(remote_record.id.as_str())
            .filter(|record| !record.deleted)
        else {
            continue;
        };
        let Some(base_payload) = base_record.payload.as_ref() else {
            continue;
        };
        let Some(local_payload) = local_record.payload.as_ref() else {
            continue;
        };
        let merged_payload = merge_structured_model_fields(
            base_payload,
            local_payload,
            remote_payload,
            conflict_strategy,
        )?;
        let merged_options = merge_structured_model_fields(
            &base_record.options,
            &local_record.options,
            &remote_record.options,
            conflict_strategy,
        )?;
        let payload_changed = merged_payload.is_some();
        let options_changed = merged_options.is_some();
        if let Some(merged_payload) = merged_payload {
            remote_record.payload = Some(merged_payload);
        }
        if let Some(merged_options) = merged_options {
            remote_record.options = merged_options;
        }
        if payload_changed || options_changed {
            remote_record.updated_at = merged_at.to_string();
            remote_record.revision = saved_connection_record_revision(remote_record)?;
            changed = true;
        }
    }
    Ok(changed)
}

pub(super) fn saved_connection_record_revision(
    record: &oxideterm_connections::SavedConnectionSyncRecord,
) -> Result<String> {
    let payload = record
        .payload
        .as_ref()
        .context("saved connection sync record is missing its payload")?;
    let bytes = match record.options.as_ref() {
        Some(options) => serde_json::to_vec(&(payload, options))?,
        None => serde_json::to_vec(payload)?,
    };
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(super) fn merge_forward_records(
    remote: &mut SavedForwardsSyncSnapshot,
    base: &SavedForwardsSyncSnapshot,
    local: &SavedForwardsSyncSnapshot,
    conflict_strategy: &ConflictStrategy,
    merged_at: &str,
) -> Result<bool> {
    let base_records = forward_records_by_id(&base.records);
    let local_records = forward_records_by_id(&local.records);
    let mut changed = false;
    for remote_record in &mut remote.records {
        if remote_record.deleted {
            continue;
        }
        let Some(remote_payload) = remote_record.payload.as_ref() else {
            continue;
        };
        let Some(base_payload) = base_records
            .get(remote_record.id.as_str())
            .filter(|record| !record.deleted)
            .and_then(|record| record.payload.as_ref())
        else {
            continue;
        };
        let Some(local_payload) = local_records
            .get(remote_record.id.as_str())
            .filter(|record| !record.deleted)
            .and_then(|record| record.payload.as_ref())
        else {
            continue;
        };
        if let Some(merged_payload) = merge_structured_model_fields(
            base_payload,
            local_payload,
            remote_payload,
            conflict_strategy,
        )? {
            remote_record.payload = Some(merged_payload);
            remote_record.updated_at = merged_at.to_string();
            changed = true;
        }
    }
    Ok(changed)
}

pub(super) fn merge_serial_profile_records(
    remote: &mut SerialProfilesSyncSnapshot,
    base: &SerialProfilesSyncSnapshot,
    local: &SerialProfilesSyncSnapshot,
    conflict_strategy: &ConflictStrategy,
    merged_at: chrono::DateTime<Utc>,
) -> Result<bool> {
    let base_records = base
        .records
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<BTreeMap<_, _>>();
    let local_records = local
        .records
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<BTreeMap<_, _>>();
    let mut changed = false;
    for remote_profile in &mut remote.records {
        let Some(base_profile) = base_records.get(remote_profile.id.as_str()).copied() else {
            continue;
        };
        let Some(local_profile) = local_records.get(remote_profile.id.as_str()).copied() else {
            continue;
        };
        if let Some(mut merged_profile) = merge_structured_model_fields(
            base_profile,
            local_profile,
            remote_profile,
            conflict_strategy,
        )? {
            merged_profile.updated_at = merged_at;
            *remote_profile = merged_profile;
            changed = true;
        }
    }
    Ok(changed)
}

pub(super) fn merge_quick_command_records(
    remote_json: &mut String,
    base_json: &str,
    local_json: &str,
    conflict_strategy: &ConflictStrategy,
    merged_at: u64,
) -> Result<bool> {
    let base = serde_json::from_str::<QuickCommandsSnapshot>(base_json)?;
    let local = serde_json::from_str::<QuickCommandsSnapshot>(local_json)?;
    let mut remote = serde_json::from_str::<QuickCommandsSnapshot>(remote_json)?;
    let mut changed = false;
    changed |= merge_quick_command_categories(
        &mut remote.categories,
        &base.categories,
        &local.categories,
        conflict_strategy,
    )?;
    changed |= merge_quick_commands(
        &mut remote.commands,
        &base.commands,
        &local.commands,
        conflict_strategy,
        merged_at,
    )?;
    if changed {
        remote.updated_at = merged_at;
        *remote_json = serde_json::to_string(&remote)?;
    }
    Ok(changed)
}

pub(super) fn merge_quick_command_categories(
    remote: &mut [QuickCommandCategory],
    base: &[QuickCommandCategory],
    local: &[QuickCommandCategory],
    conflict_strategy: &ConflictStrategy,
) -> Result<bool> {
    let base_records = base
        .iter()
        .map(|category| (category.id.as_str(), category))
        .collect::<BTreeMap<_, _>>();
    let local_records = local
        .iter()
        .map(|category| (category.id.as_str(), category))
        .collect::<BTreeMap<_, _>>();
    let mut changed = false;
    for remote_category in remote {
        let Some(base_category) = base_records.get(remote_category.id.as_str()).copied() else {
            continue;
        };
        let Some(local_category) = local_records.get(remote_category.id.as_str()).copied() else {
            continue;
        };
        if let Some(merged_category) = merge_structured_model_fields(
            base_category,
            local_category,
            remote_category,
            conflict_strategy,
        )? {
            *remote_category = merged_category;
            changed = true;
        }
    }
    Ok(changed)
}

pub(super) fn merge_quick_commands(
    remote: &mut [QuickCommand],
    base: &[QuickCommand],
    local: &[QuickCommand],
    conflict_strategy: &ConflictStrategy,
    merged_at: u64,
) -> Result<bool> {
    let base_records = base
        .iter()
        .map(|command| (command.id.as_str(), command))
        .collect::<BTreeMap<_, _>>();
    let local_records = local
        .iter()
        .map(|command| (command.id.as_str(), command))
        .collect::<BTreeMap<_, _>>();
    let mut changed = false;
    for remote_command in remote {
        let Some(base_command) = base_records.get(remote_command.id.as_str()).copied() else {
            continue;
        };
        let Some(local_command) = local_records.get(remote_command.id.as_str()).copied() else {
            continue;
        };
        if let Some(mut merged_command) = merge_structured_model_fields(
            base_command,
            local_command,
            remote_command,
            conflict_strategy,
        )? {
            merged_command.updated_at = merged_at;
            *remote_command = merged_command;
            changed = true;
        }
    }
    Ok(changed)
}

pub(super) fn sync_records_by_id(
    records: &[oxideterm_connections::SavedConnectionSyncRecord],
) -> BTreeMap<&str, &oxideterm_connections::SavedConnectionSyncRecord> {
    records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect()
}

pub(super) fn forward_records_by_id(
    records: &[oxideterm_forwarding::SavedForwardSyncRecord],
) -> BTreeMap<&str, &oxideterm_forwarding::SavedForwardSyncRecord> {
    records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect()
}

/// Three-way merges a structured sync model using base/local/remote values.
///
/// Returns `None` when the remote model already represents the effective result.
pub fn merge_structured_model_fields<T>(
    base: &T,
    local: &T,
    remote: &T,
    conflict_strategy: &ConflictStrategy,
) -> Result<Option<T>>
where
    T: Serialize + DeserializeOwned,
{
    let base_value = serde_json::to_value(base)?;
    let local_value = serde_json::to_value(local)?;
    let remote_value = serde_json::to_value(remote)?;
    let (Some(merged_value), used_local) = merge_structured_json_value(
        Some(&base_value),
        Some(&local_value),
        Some(&remote_value),
        conflict_strategy,
    ) else {
        return Ok(None);
    };
    if !used_local || merged_value == remote_value {
        return Ok(None);
    }
    serde_json::from_value(merged_value)
        .map(Some)
        .map_err(anyhow::Error::from)
}

pub(super) fn merge_structured_json_value(
    base: Option<&Value>,
    local: Option<&Value>,
    remote: Option<&Value>,
    conflict_strategy: &ConflictStrategy,
) -> (Option<Value>, bool) {
    if local == remote {
        return (remote.cloned(), false);
    }
    if base == local {
        return (remote.cloned(), false);
    }
    if base == remote {
        return (local.cloned(), true);
    }
    if let (
        Some(Value::Object(base_object)),
        Some(Value::Object(local_object)),
        Some(Value::Object(remote_object)),
    ) = (base, local, remote)
    {
        let mut keys = BTreeSet::new();
        keys.extend(base_object.keys().map(String::as_str));
        keys.extend(local_object.keys().map(String::as_str));
        keys.extend(remote_object.keys().map(String::as_str));
        let mut merged = serde_json::Map::new();
        let mut used_local = false;
        for key in keys {
            let (value, child_used_local) = merge_structured_json_value(
                base_object.get(key),
                local_object.get(key),
                remote_object.get(key),
                conflict_strategy,
            );
            used_local |= child_used_local;
            if let Some(value) = value {
                merged.insert(key.to_string(), value);
            }
        }
        return (Some(Value::Object(merged)), used_local);
    }
    if merge_conflict_prefers_local(conflict_strategy) {
        (local.cloned(), true)
    } else {
        (remote.cloned(), false)
    }
}

pub(super) fn merge_conflict_prefers_local(conflict_strategy: &ConflictStrategy) -> bool {
    !matches!(conflict_strategy, ConflictStrategy::Replace)
}

pub(super) fn fractional_import_progress(current: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (current as f64 / total as f64).clamp(0.0, 1.0)
    }
}

pub(super) fn export_progress_current(
    completed_exports: usize,
    current: usize,
    total: usize,
) -> f64 {
    let fraction = if total == 0 {
        0.0
    } else {
        (current as f64 / total as f64).clamp(0.0, 0.95)
    };
    2.0 + completed_exports as f64 + fraction
}

pub(super) fn structured_preview_progress_current(
    completed_entries: usize,
    total_entries: usize,
    active_fraction: f64,
) -> f64 {
    if total_entries == 0 {
        3.0
    } else {
        let fraction = ((completed_entries as f64 + active_fraction.clamp(0.0, 1.0))
            / total_entries as f64)
            .clamp(0.0, 1.0);
        (2.0 + fraction).min(3.0)
    }
}

pub(super) fn host_import_progress_stage(stage: &str, preview: bool) -> CloudSyncProgressStage {
    match stage {
        "parsing_file"
        | "deriving_key"
        | "decrypting_payload"
        | "deserializing_payload"
        | "verifying_checksum" => {
            if preview {
                CloudSyncProgressStage::PreviewingImport
            } else {
                CloudSyncProgressStage::Importing
            }
        }
        "collecting_existing" | "building_preview" | "analyzing_preview" => {
            CloudSyncProgressStage::PreviewingImport
        }
        "filtering_selection"
        | "preparing_connections"
        | "applying_connections"
        | "saving_config" => CloudSyncProgressStage::Importing,
        _ => {
            if preview {
                CloudSyncProgressStage::PreviewingImport
            } else {
                CloudSyncProgressStage::Importing
            }
        }
    }
}
