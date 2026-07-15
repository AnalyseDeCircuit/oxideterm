// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn push_preview_impact(
    items: &mut Vec<CloudSyncPreviewImpactItem>,
    label_key: &'static str,
    count: usize,
    selected: bool,
) {
    if count == 0 {
        return;
    }
    items.push(CloudSyncPreviewImpactItem {
        label_key,
        count,
        status: coverage_status_from_bool(selected),
    });
}

pub(super) fn push_upload_connection_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    remote: Option<&SavedConnectionsSyncSnapshot>,
    local: &SavedConnectionsSyncSnapshot,
) {
    let remote_records = remote
        .into_iter()
        .flat_map(|snapshot| snapshot.records.iter())
        .map(|record| (record.id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    let mut seen_ids = BTreeSet::new();
    for record in &local.records {
        let Some(local_payload) = record.payload.as_ref().filter(|_| !record.deleted) else {
            continue;
        };
        seen_ids.insert(record.id.as_str());
        let remote_payload = remote_records
            .get(record.id.as_str())
            .copied()
            .filter(|record| !record.deleted)
            .and_then(|record| record.payload.as_ref());
        let fields = remote_payload
            .map(|remote_payload| connection_changed_fields(remote_payload, local_payload))
            .unwrap_or_else(|| connection_summary_fields(local_payload));
        let status = if remote_payload.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_connections",
            record.id.clone(),
            local_payload.name.clone(),
            status,
            fields,
        );
    }
    for (id, remote_record) in remote_records {
        if seen_ids.contains(id) || remote_record.deleted {
            continue;
        }
        let item_name = remote_record
            .payload
            .as_ref()
            .map(|payload| payload.name.clone())
            .unwrap_or_else(|| remote_record.id.clone());
        items.push(field_diff_item_with_key(
            "plugin.cloud_sync.settings.sync_connections",
            remote_record.id.clone(),
            item_name,
            CloudSyncFieldDiffStatus::Deleted,
            Vec::new(),
        ));
    }
}

pub(super) fn push_connection_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    remote: &SavedConnectionsSyncSnapshot,
    base: Option<&SavedConnectionsSyncSnapshot>,
    local: Option<&SavedConnectionsSyncSnapshot>,
    conflict_strategy: &ConflictStrategy,
) {
    let base_records = base
        .into_iter()
        .flat_map(|snapshot| snapshot.records.iter())
        .map(|record| (record.id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    let local_records = local
        .into_iter()
        .flat_map(|snapshot| snapshot.records.iter())
        .map(|record| (record.id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    for record in &remote.records {
        let local_record = local_records.get(record.id.as_str()).copied();
        let item_name = record
            .payload
            .as_ref()
            .map(|payload| payload.name.clone())
            .or_else(|| {
                local_record
                    .and_then(|record| record.payload.as_ref().map(|payload| payload.name.clone()))
            })
            .unwrap_or_else(|| record.id.clone());
        if record.deleted {
            if local_record.is_some() {
                items.push(field_diff_item_with_key(
                    "plugin.cloud_sync.settings.sync_connections",
                    record.id.clone(),
                    item_name,
                    CloudSyncFieldDiffStatus::Deleted,
                    Vec::new(),
                ));
            }
            continue;
        }
        let Some(remote_payload) = record.payload.as_ref() else {
            continue;
        };
        let local_payload = local_record.and_then(|record| record.payload.as_ref());
        let base_payload = base_records
            .get(record.id.as_str())
            .and_then(|record| record.payload.as_ref());
        let effective_remote = local_payload
            .and_then(|local_payload| {
                base_payload.and_then(|base_payload| {
                    merge_structured_model_fields(
                        base_payload,
                        local_payload,
                        remote_payload,
                        conflict_strategy,
                    )
                    .ok()
                    .flatten()
                })
            })
            .unwrap_or_else(|| remote_payload.clone());
        let fields = match (base_payload, local_payload) {
            (Some(base_payload), Some(local_payload)) => connection_merge_fields(
                base_payload,
                local_payload,
                remote_payload,
                &effective_remote,
                conflict_strategy,
            ),
            (_, Some(local_payload)) => connection_changed_fields(local_payload, &effective_remote),
            _ => connection_summary_fields(remote_payload),
        };
        let status = if local_payload.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_connections",
            record.id.clone(),
            item_name,
            status,
            fields,
        );
    }
}

pub(super) fn push_upload_forward_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    remote: Option<&SavedForwardsSyncSnapshot>,
    local: &SavedForwardsSyncSnapshot,
) {
    let remote_records = remote
        .into_iter()
        .flat_map(|snapshot| snapshot.records.iter())
        .map(|record| (record.id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    let mut seen_ids = BTreeSet::new();
    for record in &local.records {
        let Some(local_payload) = record.payload.as_ref().filter(|_| !record.deleted) else {
            continue;
        };
        seen_ids.insert(record.id.as_str());
        let remote_payload = remote_records
            .get(record.id.as_str())
            .copied()
            .filter(|record| !record.deleted)
            .and_then(|record| record.payload.as_ref());
        let fields = remote_payload
            .map(|remote_payload| forward_changed_fields(remote_payload, local_payload))
            .unwrap_or_else(|| forward_summary_fields(local_payload));
        let status = if remote_payload.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_forwards",
            record.id.clone(),
            forward_item_name(local_payload),
            status,
            fields,
        );
    }
    for (id, remote_record) in remote_records {
        if seen_ids.contains(id) || remote_record.deleted {
            continue;
        }
        let item_name = remote_record
            .payload
            .as_ref()
            .map(forward_item_name)
            .unwrap_or_else(|| remote_record.id.clone());
        items.push(field_diff_item_with_key(
            "plugin.cloud_sync.settings.sync_forwards",
            remote_record.id.clone(),
            item_name,
            CloudSyncFieldDiffStatus::Deleted,
            Vec::new(),
        ));
    }
}

pub(super) fn push_forward_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    remote: &SavedForwardsSyncSnapshot,
    base: Option<&SavedForwardsSyncSnapshot>,
    local: Option<&SavedForwardsSyncSnapshot>,
    conflict_strategy: &ConflictStrategy,
) {
    let base_records = base
        .into_iter()
        .flat_map(|snapshot| snapshot.records.iter())
        .map(|record| (record.id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    let local_records = local
        .into_iter()
        .flat_map(|snapshot| snapshot.records.iter())
        .map(|record| (record.id.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    for record in &remote.records {
        let local_record = local_records.get(record.id.as_str()).copied();
        let item_name = record
            .payload
            .as_ref()
            .map(forward_item_name)
            .or_else(|| {
                local_record.and_then(|record| record.payload.as_ref().map(forward_item_name))
            })
            .unwrap_or_else(|| record.id.clone());
        if record.deleted {
            if local_record.is_some() {
                items.push(field_diff_item_with_key(
                    "plugin.cloud_sync.settings.sync_forwards",
                    record.id.clone(),
                    item_name,
                    CloudSyncFieldDiffStatus::Deleted,
                    Vec::new(),
                ));
            }
            continue;
        }
        let Some(remote_payload) = record.payload.as_ref() else {
            continue;
        };
        let local_payload = local_record.and_then(|record| record.payload.as_ref());
        let base_payload = base_records
            .get(record.id.as_str())
            .and_then(|record| record.payload.as_ref());
        let effective_remote = local_payload
            .and_then(|local_payload| {
                base_payload.and_then(|base_payload| {
                    merge_structured_model_fields(
                        base_payload,
                        local_payload,
                        remote_payload,
                        conflict_strategy,
                    )
                    .ok()
                    .flatten()
                })
            })
            .unwrap_or_else(|| remote_payload.clone());
        let fields = match (base_payload, local_payload) {
            (Some(base_payload), Some(local_payload)) => forward_merge_fields(
                base_payload,
                local_payload,
                remote_payload,
                &effective_remote,
                conflict_strategy,
            ),
            (_, Some(local_payload)) => forward_changed_fields(local_payload, &effective_remote),
            _ => forward_summary_fields(remote_payload),
        };
        let status = if local_payload.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_forwards",
            record.id.clone(),
            item_name,
            status,
            fields,
        );
    }
}

pub(super) fn push_upload_quick_command_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    remote: Option<&QuickCommandsSnapshot>,
    local: &QuickCommandsSnapshot,
) {
    let remote_commands = remote
        .into_iter()
        .flat_map(|snapshot| snapshot.commands.iter())
        .map(|command| (command.id.as_str(), command))
        .collect::<BTreeMap<_, _>>();
    let mut seen_ids = BTreeSet::new();
    for local_command in &local.commands {
        seen_ids.insert(local_command.id.as_str());
        let remote_command = remote_commands.get(local_command.id.as_str()).copied();
        let fields = remote_command
            .map(|remote_command| quick_command_changed_fields(remote_command, local_command))
            .unwrap_or_else(|| quick_command_summary_fields(local_command));
        let status = if remote_command.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_quick_commands",
            local_command.id.clone(),
            local_command.name.clone(),
            status,
            fields,
        );
    }
    for (id, remote_command) in remote_commands {
        if seen_ids.contains(id) {
            continue;
        }
        items.push(field_diff_item_with_key(
            "plugin.cloud_sync.settings.sync_quick_commands",
            remote_command.id.clone(),
            remote_command.name.clone(),
            CloudSyncFieldDiffStatus::Deleted,
            Vec::new(),
        ));
    }
}

pub(super) fn push_quick_command_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    remote: &QuickCommandsSnapshot,
    base: Option<&QuickCommandsSnapshot>,
    local: Option<&QuickCommandsSnapshot>,
    conflict_strategy: &ConflictStrategy,
) {
    let base_commands = base
        .into_iter()
        .flat_map(|snapshot| snapshot.commands.iter())
        .map(|command| (command.id.as_str(), command))
        .collect::<BTreeMap<_, _>>();
    let local_commands = local
        .into_iter()
        .flat_map(|snapshot| snapshot.commands.iter())
        .map(|command| (command.id.as_str(), command))
        .collect::<BTreeMap<_, _>>();
    for remote_command in &remote.commands {
        let local_command = local_commands.get(remote_command.id.as_str()).copied();
        let base_command = base_commands.get(remote_command.id.as_str()).copied();
        let effective_remote = local_command
            .and_then(|local_command| {
                base_command.and_then(|base_command| {
                    merge_structured_model_fields(
                        base_command,
                        local_command,
                        remote_command,
                        conflict_strategy,
                    )
                    .ok()
                    .flatten()
                })
            })
            .unwrap_or_else(|| remote_command.clone());
        let fields = match (base_command, local_command) {
            (Some(base_command), Some(local_command)) => quick_command_merge_fields(
                base_command,
                local_command,
                remote_command,
                &effective_remote,
                conflict_strategy,
            ),
            (_, Some(local_command)) => {
                quick_command_changed_fields(local_command, &effective_remote)
            }
            _ => quick_command_summary_fields(remote_command),
        };
        let status = if local_command.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_quick_commands",
            remote_command.id.clone(),
            remote_command.name.clone(),
            status,
            fields,
        );
    }
}

pub(super) fn push_upload_serial_profile_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    remote: Option<&SerialProfilesSyncSnapshot>,
    local: &SerialProfilesSyncSnapshot,
) {
    let remote_profiles = remote
        .into_iter()
        .flat_map(|snapshot| snapshot.records.iter())
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<BTreeMap<_, _>>();
    let mut seen_ids = BTreeSet::new();
    for local_profile in &local.records {
        seen_ids.insert(local_profile.id.as_str());
        let remote_profile = remote_profiles.get(local_profile.id.as_str()).copied();
        let fields = remote_profile
            .map(|remote_profile| serial_profile_changed_fields(remote_profile, local_profile))
            .unwrap_or_else(|| serial_profile_summary_fields(local_profile));
        let status = if remote_profile.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_serial_profiles",
            local_profile.id.clone(),
            local_profile.name.clone(),
            status,
            fields,
        );
    }
    for (id, remote_profile) in remote_profiles {
        if seen_ids.contains(id) {
            continue;
        }
        items.push(field_diff_item_with_key(
            "plugin.cloud_sync.settings.sync_serial_profiles",
            remote_profile.id.clone(),
            remote_profile.name.clone(),
            CloudSyncFieldDiffStatus::Deleted,
            Vec::new(),
        ));
    }
}

pub(super) fn push_serial_profile_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    remote: &SerialProfilesSyncSnapshot,
    base: Option<&SerialProfilesSyncSnapshot>,
    local: Option<&SerialProfilesSyncSnapshot>,
    conflict_strategy: &ConflictStrategy,
) {
    let base_profiles = base
        .into_iter()
        .flat_map(|snapshot| snapshot.records.iter())
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<BTreeMap<_, _>>();
    let local_profiles = local
        .into_iter()
        .flat_map(|snapshot| snapshot.records.iter())
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<BTreeMap<_, _>>();
    for remote_profile in &remote.records {
        let local_profile = local_profiles.get(remote_profile.id.as_str()).copied();
        let base_profile = base_profiles.get(remote_profile.id.as_str()).copied();
        let effective_remote = local_profile
            .and_then(|local_profile| {
                base_profile.and_then(|base_profile| {
                    merge_structured_model_fields(
                        base_profile,
                        local_profile,
                        remote_profile,
                        conflict_strategy,
                    )
                    .ok()
                    .flatten()
                })
            })
            .unwrap_or_else(|| remote_profile.clone());
        let fields = match (base_profile, local_profile) {
            (Some(base_profile), Some(local_profile)) => serial_profile_merge_fields(
                base_profile,
                local_profile,
                remote_profile,
                &effective_remote,
                conflict_strategy,
            ),
            (_, Some(local_profile)) => {
                serial_profile_changed_fields(local_profile, &effective_remote)
            }
            _ => serial_profile_summary_fields(remote_profile),
        };
        let status = if local_profile.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_serial_profiles",
            remote_profile.id.clone(),
            remote_profile.name.clone(),
            status,
            fields,
        );
    }
}

pub(super) fn push_upload_app_settings_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    remote_preview: &StructuredPreview,
    local: &CloudSyncLocalFieldDiffSnapshot,
    scope: &SyncScope,
) {
    let local_sections = local
        .app_settings_sections
        .iter()
        .filter(|section| scope.app_settings_sections.contains(&section.id))
        .map(|section| (section.id.as_str(), section))
        .collect::<BTreeMap<_, _>>();
    let mut seen_ids = BTreeSet::new();
    for (section_id, local_section) in &local_sections {
        seen_ids.insert(*section_id);
        let remote_section = remote_preview.app_settings_sections.get(*section_id);
        let fields = remote_section
            .map(|remote_section| {
                app_settings_changed_fields(
                    &remote_section.field_values,
                    &local_section.field_values,
                )
            })
            .unwrap_or_else(|| app_settings_summary_fields(&local_section.field_values));
        let status = if remote_section.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_app_settings",
            (*section_id).to_string(),
            (*section_id).to_string(),
            status,
            fields,
        );
    }
    for (section_id, remote_section) in &remote_preview.app_settings_sections {
        if seen_ids.contains(section_id.as_str())
            || !scope.app_settings_sections.contains(section_id)
        {
            continue;
        }
        if remote_section.field_values.is_empty() {
            continue;
        }
        items.push(field_diff_item_with_key(
            "plugin.cloud_sync.settings.sync_app_settings",
            section_id.clone(),
            section_id.clone(),
            CloudSyncFieldDiffStatus::Deleted,
            Vec::new(),
        ));
    }
}

pub(super) fn push_app_settings_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    preview: &StructuredPreview,
    selection: &CloudSyncPreviewSelection,
    local: &CloudSyncLocalFieldDiffSnapshot,
) {
    let local_sections = local
        .app_settings_sections
        .iter()
        .map(|section| (section.id.as_str(), section))
        .collect::<BTreeMap<_, _>>();
    for (section_id, remote_section) in &preview.app_settings_sections {
        if !selection
            .selected_app_settings_sections
            .contains(section_id)
        {
            continue;
        }
        let local_section = local_sections.get(section_id.as_str()).copied();
        let fields = local_section
            .map(|local_section| {
                app_settings_changed_fields(
                    &local_section.field_values,
                    &remote_section.field_values,
                )
            })
            .unwrap_or_else(|| app_settings_summary_fields(&remote_section.field_values));
        let status = if local_section.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_app_settings",
            section_id.clone(),
            section_id.clone(),
            status,
            fields,
        );
    }
}
