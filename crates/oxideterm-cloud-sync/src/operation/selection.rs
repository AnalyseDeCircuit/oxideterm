// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn required_sync_password(password: Option<&str>) -> Result<&str> {
    password
        .filter(|password| !password.is_empty())
        .context("missing_sync_password: cloud sync password is required")
}

pub(super) fn import_strategy_from_cloud(strategy: ConflictStrategy) -> ImportConflictStrategy {
    match strategy {
        ConflictStrategy::Merge => ImportConflictStrategy::Merge,
        ConflictStrategy::Replace => ImportConflictStrategy::Replace,
        ConflictStrategy::Skip => ImportConflictStrategy::Skip,
        ConflictStrategy::Rename => ImportConflictStrategy::Rename,
    }
}

pub(super) fn legacy_preview_selected_names(
    import_connections: bool,
    selected_connection_names: Option<Vec<String>>,
) -> Option<Vec<String>> {
    if import_connections {
        selected_connection_names
    } else {
        Some(Vec::new())
    }
}

pub(super) fn filter_saved_connection_snapshot(
    snapshot: &mut SavedConnectionsSyncSnapshot,
    selected_ids: Option<&BTreeSet<String>>,
) {
    if let Some(selected_ids) = selected_ids {
        snapshot
            .records
            .retain(|record| selected_ids.contains(&record.id));
    }
}

pub(super) fn filter_saved_forwards_snapshot(
    snapshot: &mut SavedForwardsSyncSnapshot,
    selected_ids: Option<&BTreeSet<String>>,
) {
    if let Some(selected_ids) = selected_ids {
        snapshot
            .records
            .retain(|record| selected_ids.contains(&record.id));
    }
}

pub(super) fn filter_serial_profiles_snapshot(
    snapshot: &mut SerialProfilesSyncSnapshot,
    selected_ids: Option<&BTreeSet<String>>,
) {
    if let Some(selected_ids) = selected_ids {
        snapshot
            .records
            .retain(|profile| selected_ids.contains(&profile.id));
    }
}

pub(super) fn filter_quick_commands_snapshot_json(
    snapshot_json: &mut String,
    selected_ids: Option<&BTreeSet<String>>,
) -> usize {
    // Keep filtering at the serialized snapshot boundary so upload writes exactly the chosen object.
    let Ok(mut snapshot) =
        serde_json::from_str::<oxideterm_quick_commands::QuickCommandsSnapshot>(snapshot_json)
    else {
        return 0;
    };
    if let Some(selected_ids) = selected_ids {
        snapshot
            .commands
            .retain(|command| selected_ids.contains(&command.id));
        if let Ok(filtered_json) = serde_json::to_string(&snapshot) {
            *snapshot_json = filtered_json;
        }
    }
    snapshot.commands.len()
}

#[derive(Clone, Debug)]
pub(super) struct StructuredUploadPlan {
    pub(super) manifest: crate::StructuredManifest,
    pub(super) objects: Vec<StructuredUploadObject>,
}

#[derive(Clone, Debug)]
pub(super) struct StructuredUploadObject {
    pub(super) path: String,
    pub(super) bytes: Vec<u8>,
    pub(super) content_type: String,
}

pub(super) fn ensure_no_remote_conflict(
    local_snapshot: &CloudSyncLocalSnapshot,
    remote_metadata: &RemoteMetadata,
    previous_remote_revision: Option<&str>,
    previous_remote_sections: Option<&StructuredSectionRevisions>,
) -> Result<()> {
    if remote_metadata.format.as_deref() != Some(STRUCTURED_MANIFEST_FORMAT) {
        if local_snapshot.dirty.has_dirty
            && remote_metadata.revision.as_deref().is_some_and(|revision| {
                previous_remote_revision.map_or(true, |previous| previous != revision)
            })
        {
            bail!(
                "remote_changed_before_upload: remote snapshot exists while local state is dirty"
            );
        }
        return Ok(());
    }
    if local_snapshot.dirty.has_dirty
        && has_structured_conflict(
            &local_snapshot.dirty.dirty_sections,
            remote_metadata.section_revisions.as_ref(),
            previous_remote_sections,
        )
    {
        bail!(
            "remote_changed_before_upload: remote structured snapshot exists while local state is dirty"
        );
    }
    Ok(())
}

pub(super) fn has_structured_conflict(
    dirty_sections: &crate::StructuredDirtySections,
    remote_sections: Option<&StructuredSectionRevisions>,
    previous_remote_sections: Option<&StructuredSectionRevisions>,
) -> bool {
    let Some(previous) = previous_remote_sections else {
        return dirty_sections.connections
            || dirty_sections.forwards
            || dirty_sections.quick_commands
            || dirty_sections.serial_profiles
            || dirty_sections.sensitive_credentials
            || dirty_sections.app_settings.values().any(|dirty| *dirty)
            || dirty_sections.plugin_settings.values().any(|dirty| *dirty);
    };
    let remote = remote_sections.cloned().unwrap_or_default();
    if dirty_sections.connections && remote.connections != previous.connections {
        return true;
    }
    if dirty_sections.forwards && remote.forwards != previous.forwards {
        return true;
    }
    if dirty_sections.quick_commands && remote.quick_commands != previous.quick_commands {
        return true;
    }
    if dirty_sections.serial_profiles && remote.serial_profiles != previous.serial_profiles {
        return true;
    }
    if dirty_sections.sensitive_credentials
        && remote.sensitive_credentials != previous.sensitive_credentials
    {
        return true;
    }
    for (section_id, dirty) in &dirty_sections.app_settings {
        if *dirty && remote.app_settings.get(section_id) != previous.app_settings.get(section_id) {
            return true;
        }
    }
    for (plugin_id, dirty) in &dirty_sections.plugin_settings {
        if *dirty
            && remote.plugin_settings.get(plugin_id) != previous.plugin_settings.get(plugin_id)
        {
            return true;
        }
    }
    false
}

pub(super) fn manifest_from_metadata(metadata: &RemoteMetadata) -> Result<StructuredManifest> {
    let sections = metadata
        .sections
        .clone()
        .context("missing structured manifest sections")?;
    Ok(StructuredManifest {
        format: metadata
            .format
            .clone()
            .unwrap_or_else(|| STRUCTURED_MANIFEST_FORMAT.to_string()),
        revision: metadata.revision.clone().unwrap_or_default(),
        uploaded_at: metadata.uploaded_at.clone().unwrap_or_default(),
        device_id: metadata.device_id.clone().unwrap_or_default(),
        content_type: metadata
            .content_type
            .clone()
            .unwrap_or_else(|| STRUCTURED_MANIFEST_CONTENT_TYPE.to_string()),
        scope: metadata.scope.clone().unwrap_or_default(),
        sections: serde_json::from_value::<StructuredManifestSections>(sections)?,
        section_revisions: metadata.section_revisions.clone().unwrap_or_default(),
    })
}

pub(super) fn scoped_plugin_ids(local_snapshot: &CloudSyncLocalSnapshot) -> Vec<String> {
    match local_snapshot.scope.plugin_ids.as_ref() {
        Some(plugin_ids) => crate::get_syncable_plugin_ids(plugin_ids),
        None => crate::get_syncable_plugin_ids(
            &local_snapshot
                .metadata
                .plugin_settings_revisions
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
        ),
    }
}

pub(super) fn plugin_id_from_setting_storage_key(storage_key: &str) -> Option<String> {
    const PREFIX: &str = "oxide-plugin-";
    const SEPARATOR: &str = "-setting-";
    let remainder = storage_key.strip_prefix(PREFIX)?;
    let separator_index = remainder.find(SEPARATOR)?;
    let plugin_id = &remainder[..separator_index];
    let setting_id = &remainder[separator_index + SEPARATOR.len()..];
    (!plugin_id.is_empty() && !setting_id.is_empty()).then(|| plugin_id.to_string())
}

pub(super) fn include_managed_keys_in_connection_preflight(scope: &crate::SyncScope) -> bool {
    // Managed key material is exported only through the encrypted sensitive credentials object.
    scope.sync_sensitive_credentials
}
