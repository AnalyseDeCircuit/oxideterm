// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn forward_item_name(value: &PersistedForwardDto) -> String {
    format!(
        "{} {}:{} -> {}:{}",
        value.forward_type,
        value.bind_address,
        value.bind_port,
        value.target_host,
        value.target_port
    )
}

pub(super) fn push_non_empty_field_diff(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    section_label_key: &'static str,
    item_key: String,
    item_name: String,
    status: CloudSyncFieldDiffStatus,
    fields: Vec<CloudSyncFieldDiffField>,
) {
    if fields.is_empty() && status == CloudSyncFieldDiffStatus::Modified {
        return;
    }
    items.push(field_diff_item_with_key(
        section_label_key,
        item_key,
        item_name,
        status,
        fields,
    ));
}

pub(super) fn field_diff_item_with_key(
    section_label_key: &'static str,
    item_key: String,
    item_name: String,
    status: CloudSyncFieldDiffStatus,
    fields: Vec<CloudSyncFieldDiffField>,
) -> CloudSyncFieldDiffItem {
    CloudSyncFieldDiffItem {
        section_label_key,
        item_key,
        item_name,
        status,
        fields,
    }
}

pub(super) fn push_changed(
    fields: &mut Vec<CloudSyncFieldDiffField>,
    label_key: &'static str,
    before: Option<String>,
    after: Option<String>,
) {
    if before != after {
        fields.push(field(label_key, before, after));
    }
}

pub(super) fn push_merge_changed(
    fields: &mut Vec<CloudSyncFieldDiffField>,
    label_key: &'static str,
    base: Option<String>,
    local: Option<String>,
    remote: Option<String>,
    effective: Option<String>,
    conflict_strategy: &ConflictStrategy,
) {
    let merge_outcome = merge_outcome_for_values(
        base.as_deref(),
        local.as_deref(),
        remote.as_deref(),
        effective.as_deref(),
        conflict_strategy,
    );
    if local != effective || merge_outcome.is_some() {
        fields.push(field_with_merge_outcome(
            label_key,
            local,
            effective,
            merge_outcome,
        ));
    }
}

pub(super) fn merge_outcome_for_values(
    base: Option<&str>,
    local: Option<&str>,
    remote: Option<&str>,
    effective: Option<&str>,
    conflict_strategy: &ConflictStrategy,
) -> Option<CloudSyncFieldMergeOutcome> {
    if local == remote {
        return None;
    }
    if base == local && remote != base && effective == remote {
        return Some(CloudSyncFieldMergeOutcome::Remote);
    }
    if base == remote && local != base && effective == local {
        return Some(CloudSyncFieldMergeOutcome::Local);
    }
    if base != local && base != remote && local != remote {
        return match conflict_strategy {
            ConflictStrategy::Replace if effective == remote => {
                Some(CloudSyncFieldMergeOutcome::ConflictRemote)
            }
            _ if effective == local => Some(CloudSyncFieldMergeOutcome::ConflictLocal),
            _ if effective == remote => Some(CloudSyncFieldMergeOutcome::ConflictRemote),
            _ => Some(CloudSyncFieldMergeOutcome::Merged),
        };
    }
    if effective == local {
        Some(CloudSyncFieldMergeOutcome::Local)
    } else if effective == remote {
        Some(CloudSyncFieldMergeOutcome::Remote)
    } else {
        Some(CloudSyncFieldMergeOutcome::Merged)
    }
}

pub(super) fn field(
    label_key: &'static str,
    before: Option<String>,
    after: Option<String>,
) -> CloudSyncFieldDiffField {
    field_with_merge_outcome(label_key, before, after, None)
}

pub(super) fn field_with_merge_outcome(
    label_key: &'static str,
    before: Option<String>,
    after: Option<String>,
    merge_outcome: Option<CloudSyncFieldMergeOutcome>,
) -> CloudSyncFieldDiffField {
    CloudSyncFieldDiffField {
        label_key,
        before,
        after,
        merge_outcome,
    }
}

pub(super) fn redacted_changed_value() -> String {
    CLOUD_SYNC_FIELD_REDACTED_VALUE.to_string()
}

pub(super) fn redacted_presence<T>(value: Option<T>) -> Option<String> {
    value.map(|_| redacted_changed_value())
}

pub(super) fn push_app_settings_diff_items(
    items: &mut Vec<CloudSyncSectionDiffItem>,
    scope: &SyncScope,
    current: &oxideterm_cloud_sync::StructuredLocalState,
    baseline: Option<&oxideterm_cloud_sync::StructuredLocalState>,
    remote: Option<&StructuredSectionRevisions>,
    remote_known: bool,
) {
    for section_id in OXIDE_APP_SETTINGS_SECTION_IDS {
        let section_id = (*section_id).to_string();
        let included = scope.sync_app_settings && scope.app_settings_sections.contains(&section_id);
        push_section_diff(
            items,
            CloudSyncDiffLabel::AppSettingsSection(section_id.clone()),
            included,
            current
                .app_settings
                .get(&section_id)
                .and_then(|revision| revision.as_deref()),
            baseline
                .and_then(|state| state.app_settings.get(&section_id))
                .and_then(|revision| revision.as_deref()),
            remote
                .and_then(|sections| sections.app_settings.get(&section_id))
                .map(String::as_str),
            remote_known,
            None,
        );
    }
}

pub(super) fn push_plugin_settings_diff_items(
    items: &mut Vec<CloudSyncSectionDiffItem>,
    scope: &SyncScope,
    current: &oxideterm_cloud_sync::StructuredLocalState,
    baseline: Option<&oxideterm_cloud_sync::StructuredLocalState>,
    remote: Option<&StructuredSectionRevisions>,
    remote_known: bool,
) {
    let plugin_ids = diff_plugin_ids(
        current.plugin_settings.keys(),
        remote
            .map(|sections| sections.plugin_settings.keys())
            .into_iter()
            .flatten(),
        scope.plugin_ids.as_ref().into_iter().flatten(),
    );
    for plugin_id in plugin_ids {
        let included = scope.sync_plugin_settings
            && scope
                .plugin_ids
                .as_ref()
                .map_or(true, |ids| ids.contains(&plugin_id));
        push_section_diff(
            items,
            CloudSyncDiffLabel::PluginSettings(plugin_id.clone()),
            included,
            current
                .plugin_settings
                .get(&plugin_id)
                .and_then(|revision| revision.as_deref()),
            baseline
                .and_then(|state| state.plugin_settings.get(&plugin_id))
                .and_then(|revision| revision.as_deref()),
            remote
                .and_then(|sections| sections.plugin_settings.get(&plugin_id))
                .map(String::as_str),
            remote_known,
            None,
        );
    }
}

pub(super) fn push_section_diff(
    items: &mut Vec<CloudSyncSectionDiffItem>,
    label: CloudSyncDiffLabel,
    included: bool,
    current_revision: Option<&str>,
    baseline_revision: Option<&str>,
    remote_revision: Option<&str>,
    remote_known: bool,
    count: Option<usize>,
) {
    items.push(CloudSyncSectionDiffItem {
        label,
        local_status: local_diff_status(included, current_revision, baseline_revision),
        remote_status: upload_remote_diff_status(
            included,
            current_revision,
            remote_revision,
            remote_known,
        ),
        count,
    });
}

pub(super) fn push_apply_section_diff(
    items: &mut Vec<CloudSyncSectionDiffItem>,
    label: CloudSyncDiffLabel,
    selected: bool,
    remote_revision: Option<&str>,
    local_revision: Option<&str>,
    count: Option<usize>,
) {
    if remote_revision.is_none() && local_revision.is_none() && count.unwrap_or_default() == 0 {
        return;
    }
    items.push(CloudSyncSectionDiffItem {
        label,
        local_status: local_diff_status(selected, remote_revision, local_revision),
        remote_status: if selected {
            CloudSyncRemoteDiffStatus::Unchanged
        } else {
            CloudSyncRemoteDiffStatus::Excluded
        },
        count,
    });
}

pub(super) fn local_diff_status(
    included: bool,
    current_revision: Option<&str>,
    baseline_revision: Option<&str>,
) -> CloudSyncLocalDiffStatus {
    if !included {
        return CloudSyncLocalDiffStatus::Excluded;
    }
    match (current_revision, baseline_revision) {
        (Some(_), None) => CloudSyncLocalDiffStatus::Added,
        (None, Some(_)) => CloudSyncLocalDiffStatus::Deleted,
        (Some(current), Some(baseline)) if current != baseline => {
            CloudSyncLocalDiffStatus::Modified
        }
        _ => CloudSyncLocalDiffStatus::Unchanged,
    }
}

pub(super) fn upload_remote_diff_status(
    included: bool,
    current_revision: Option<&str>,
    remote_revision: Option<&str>,
    remote_known: bool,
) -> CloudSyncRemoteDiffStatus {
    if !included {
        return if remote_revision.is_some() {
            CloudSyncRemoteDiffStatus::RemovedByScope
        } else {
            CloudSyncRemoteDiffStatus::Excluded
        };
    }
    if !remote_known {
        return CloudSyncRemoteDiffStatus::Unknown;
    }
    match (current_revision, remote_revision) {
        (Some(_), None) => CloudSyncRemoteDiffStatus::Creates,
        (Some(current), Some(remote)) if current != remote => CloudSyncRemoteDiffStatus::Overwrites,
        _ => CloudSyncRemoteDiffStatus::Unchanged,
    }
}

pub(super) fn diff_plugin_ids<'a>(
    first: impl IntoIterator<Item = &'a String>,
    second: impl IntoIterator<Item = &'a String>,
    third: impl IntoIterator<Item = &'a String>,
) -> Vec<String> {
    first
        .into_iter()
        .chain(second)
        .chain(third)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Decides preview body ordering once, keeping the app renderer as an event bridge.
pub fn cloud_sync_preview_body_sections(
    summary: &CloudSyncPreviewSummary,
) -> Vec<CloudSyncPreviewBodySection> {
    let mut sections = vec![CloudSyncPreviewBodySection::Selection];
    if !summary.forward_details.is_empty() {
        sections.push(CloudSyncPreviewBodySection::ForwardDetails(
            summary.forward_details.clone(),
        ));
    }
    sections.extend(
        summary
            .grouped_records()
            .into_iter()
            .filter(|(_, records)| !records.is_empty())
            .map(|(action, records)| CloudSyncPreviewBodySection::RecordGroup { action, records }),
    );
    sections
}

/// Provides stable title/action/warning copy keys for the preview card.
pub fn cloud_sync_preview_card_copy_spec(
    kind: CloudSyncPreviewCardKind,
    show_local_changes_warning: bool,
) -> CloudSyncPreviewCardCopySpec {
    let (title_identity, title_key, apply_label_key) = match kind {
        CloudSyncPreviewCardKind::Import => (
            "import",
            "plugin.cloud_sync.sections.import_preview",
            "plugin.cloud_sync.actions.import_preview",
        ),
        CloudSyncPreviewCardKind::Rollback => (
            "rollback",
            "plugin.cloud_sync.sections.rollback_preview",
            "plugin.cloud_sync.actions.restore_selected_backup",
        ),
    };
    CloudSyncPreviewCardCopySpec {
        title_identity,
        title_key,
        apply_label_key,
        warning_key: show_local_changes_warning
            .then_some("plugin.cloud_sync.preview.local_changes_warning"),
    }
}

/// Builds the fixed fact grid rows for a preview card.
pub fn cloud_sync_preview_fact_rows(
    summary: &CloudSyncPreviewSummary,
) -> Vec<Vec<CloudSyncPreviewFactSpec>> {
    let mut rows = vec![vec![
        CloudSyncPreviewFactSpec {
            label_key: "plugin.cloud_sync.preview.connection_count",
            value: CloudSyncPreviewFactValue::Count(summary.connections),
        },
        CloudSyncPreviewFactSpec {
            label_key: "plugin.cloud_sync.preview.total_forwards",
            value: CloudSyncPreviewFactValue::Count(summary.forwards),
        },
    ]];
    if summary.quick_commands > 0
        || summary.serial_profiles > 0
        || summary.sensitive_credentials > 0
    {
        rows.push(vec![
            CloudSyncPreviewFactSpec {
                label_key: "plugin.cloud_sync.preview.quick_commands_label",
                value: CloudSyncPreviewFactValue::Count(summary.quick_commands),
            },
            CloudSyncPreviewFactSpec {
                label_key: "plugin.cloud_sync.preview.serial_profiles_label",
                value: CloudSyncPreviewFactValue::Count(summary.serial_profiles),
            },
            CloudSyncPreviewFactSpec {
                label_key: "plugin.cloud_sync.preview.sensitive_credentials_label",
                value: CloudSyncPreviewFactValue::Count(summary.sensitive_credentials),
            },
        ]);
    }
    rows.push(vec![
        CloudSyncPreviewFactSpec {
            label_key: "plugin.cloud_sync.preview.plugin_settings_label",
            value: CloudSyncPreviewFactValue::Count(summary.plugin_settings_count),
        },
        CloudSyncPreviewFactSpec {
            label_key: "plugin.cloud_sync.preview.embedded_keys_label",
            value: CloudSyncPreviewFactValue::YesNo(summary.has_embedded_keys),
        },
    ]);
    rows
}

/// A rollback backup is only needed when applying remote content over local changes.
pub fn cloud_sync_should_create_rollback_backup(
    preview: &CloudSyncPendingPreview,
    local_dirty: bool,
) -> bool {
    local_dirty
        && matches!(
            preview,
            CloudSyncPendingPreview::Structured(_)
                | CloudSyncPendingPreview::Legacy {
                    source: CloudSyncPreviewSource::Remote,
                    ..
                }
        )
}

pub fn cloud_sync_preview_summary(preview: &CloudSyncPendingPreview) -> CloudSyncPreviewSummary {
    match preview {
        CloudSyncPendingPreview::Structured(preview) => {
            let connections = preview
                .connections_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or(0);
            let forwards = preview
                .forwards_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or(0);
            let plugin_settings_by_plugin = preview
                .plugin_settings_entries
                .keys()
                .map(|id| {
                    (
                        id.clone(),
                        preview.plugin_settings_counts.get(id).copied().unwrap_or(0),
                    )
                })
                .collect();
            let plugin_settings_count = preview.plugin_settings_counts.values().sum();
            let quick_commands = preview
                .quick_commands_snapshot_json
                .as_deref()
                .and_then(|json| {
                    serde_json::from_str::<oxideterm_quick_commands::QuickCommandsSnapshot>(json)
                        .ok()
                        .map(|snapshot| snapshot.commands.len())
                })
                .unwrap_or(0);
            CloudSyncPreviewSummary {
                connections,
                forwards,
                quick_commands,
                serial_profiles: preview
                    .serial_profiles_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.records.len())
                    .unwrap_or(0),
                sensitive_credentials: preview
                    .sensitive_credentials_preview
                    .as_ref()
                    .map(|preview| preview.total_connections + preview.portable_secret_count)
                    .unwrap_or(0),
                has_app_settings: !preview.app_settings_entries.is_empty(),
                app_settings_sections: preview
                    .app_settings_entries
                    .keys()
                    .map(|id| {
                        let field_count = preview
                            .app_settings_sections
                            .get(id)
                            .map(|section| section.field_keys.len())
                            .unwrap_or(0);
                        CloudSyncAppSettingsSection {
                            id: id.clone(),
                            field_count,
                        }
                    })
                    .collect(),
                plugin_settings_count,
                plugin_settings_by_plugin,
                has_embedded_keys: false,
                forward_details: Vec::new(),
                records: Vec::new(),
            }
        }
        CloudSyncPendingPreview::Legacy { preview, .. } => CloudSyncPreviewSummary {
            connections: preview.metadata.num_connections,
            forwards: preview.preview.total_forwards,
            quick_commands: preview.metadata.quick_commands_count.unwrap_or(0),
            serial_profiles: 0,
            sensitive_credentials: preview.metadata.portable_secret_count.unwrap_or(0),
            has_app_settings: preview.preview.has_app_settings,
            app_settings_sections: preview
                .preview
                .app_settings_sections
                .iter()
                .map(|section| CloudSyncAppSettingsSection {
                    id: section.id.clone(),
                    field_count: section.field_keys.len(),
                })
                .collect(),
            plugin_settings_count: preview.preview.plugin_settings_count,
            plugin_settings_by_plugin: preview
                .preview
                .plugin_settings_by_plugin
                .iter()
                .map(|(plugin_id, count)| (plugin_id.clone(), *count))
                .collect(),
            has_embedded_keys: preview.preview.has_embedded_keys,
            forward_details: preview
                .preview
                .forward_details
                .iter()
                .map(|detail| CloudSyncForwardDetail {
                    owner_connection_name: detail.owner_connection_name.clone(),
                    direction: detail.direction.clone(),
                    description: detail.description.clone(),
                })
                .collect(),
            records: preview
                .preview
                .records
                .iter()
                .map(|record| CloudSyncPreviewRecord {
                    resource: record.resource.clone(),
                    name: record.name.clone(),
                    action: record.action.clone(),
                    reason_code: record.reason_code.clone(),
                    target_name: record.target_name.clone(),
                })
                .collect(),
        },
    }
}

pub fn cloud_sync_app_settings_section_label_key(section_id: &str) -> Option<&'static str> {
    match section_id {
        "general" => Some("plugin.cloud_sync.preview.section_general"),
        "terminalAppearance" => Some("plugin.cloud_sync.preview.section_terminal_appearance"),
        "terminalBehavior" => Some("plugin.cloud_sync.preview.section_terminal_behavior"),
        "appearance" => Some("plugin.cloud_sync.preview.section_appearance"),
        "connections" => Some("plugin.cloud_sync.preview.section_connections"),
        "network" => Some("plugin.cloud_sync.preview.section_network"),
        "fileAndEditor" => Some("plugin.cloud_sync.preview.section_file_and_editor"),
        "ai" => Some("plugin.cloud_sync.preview.section_ai"),
        "localTerminal" => Some("plugin.cloud_sync.preview.section_local_terminal"),
        "nativePreferences" => Some("plugin.cloud_sync.preview.section_native_preferences"),
        _ => None,
    }
}
