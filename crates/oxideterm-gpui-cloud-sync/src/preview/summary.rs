// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncPreviewSummary {
    pub fn grouped_records(&self) -> Vec<(&'static str, Vec<CloudSyncPreviewRecord>)> {
        ["import", "merge", "replace", "skip", "rename"]
            .into_iter()
            .map(|action| {
                (
                    action,
                    self.records
                        .iter()
                        .filter(|record| record.action == action)
                        .cloned()
                        .collect(),
                )
            })
            .collect()
    }

    pub fn connection_record_names(&self) -> BTreeSet<String> {
        self.records
            .iter()
            .filter(|record| record.resource == "connection")
            .map(|record| record.name.clone())
            .collect()
    }
}

/// Shapes forward detail rows and overflow text without the app knowing the limit.
pub fn cloud_sync_forward_detail_rows(
    details: &[CloudSyncForwardDetail],
) -> CloudSyncPreviewListModel<CloudSyncForwardDetailRow> {
    CloudSyncPreviewListModel {
        rows: details
            .iter()
            .take(PREVIEW_RECORD_LIMIT)
            .map(|detail| CloudSyncForwardDetailRow {
                title: detail.description.clone(),
                meta: format!("{} · {}", detail.owner_connection_name, detail.direction),
            })
            .collect(),
        overflow_count: details.len().saturating_sub(PREVIEW_RECORD_LIMIT),
    }
}

/// Builds a record group model with title key, row type, and overflow count.
pub fn cloud_sync_preview_record_group_model(
    action: &'static str,
    records: &[CloudSyncPreviewRecord],
    selection: &CloudSyncPreviewSelection,
) -> CloudSyncPreviewRecordGroupModel {
    let rows = records
        .iter()
        .take(PREVIEW_RECORD_LIMIT)
        .map(|record| {
            if record.resource == "connection" {
                CloudSyncPreviewRecordRow::Connection {
                    record: record.clone(),
                    checked: selection.import_connections
                        && selection.selected_connection_names.contains(&record.name),
                    disabled: !selection.import_connections,
                }
            } else {
                CloudSyncPreviewRecordRow::Item {
                    record: record.clone(),
                }
            }
        })
        .collect();
    CloudSyncPreviewRecordGroupModel {
        title_key: match action {
            "import" => "plugin.cloud_sync.preview.will_import",
            "merge" => "plugin.cloud_sync.preview.will_merge",
            "replace" => "plugin.cloud_sync.preview.will_replace",
            "skip" => "plugin.cloud_sync.preview.will_skip",
            "rename" => "plugin.cloud_sync.preview.will_rename",
            _ => "plugin.cloud_sync.preview.records_header",
        },
        rows,
        overflow_count: records.len().saturating_sub(PREVIEW_RECORD_LIMIT),
    }
}

/// Builds the preview card view-model without touching WorkspaceApp state.
pub fn cloud_sync_preview_card_model(
    preview: &CloudSyncPendingPreview,
    state: &CloudSyncPersistedState,
    current_selection: Option<&CloudSyncPreviewSelection>,
) -> CloudSyncPreviewCardModel {
    let summary = cloud_sync_preview_summary(preview);
    let selection = current_selection.cloned().unwrap_or_else(|| {
        CloudSyncPreviewSelection::from_preview(
            preview,
            state.settings.default_conflict_strategy.clone(),
        )
    });
    let can_apply = selection.can_apply(&summary);
    let kind = if preview.is_backup() {
        CloudSyncPreviewCardKind::Rollback
    } else {
        CloudSyncPreviewCardKind::Import
    };
    let show_local_changes_warning = kind == CloudSyncPreviewCardKind::Import && state.local_dirty;
    CloudSyncPreviewCardModel {
        copy: cloud_sync_preview_card_copy_spec(kind, show_local_changes_warning),
        fact_rows: cloud_sync_preview_fact_rows(&summary),
        body_sections: cloud_sync_preview_body_sections(&summary),
        impact_items: cloud_sync_preview_impact_items(&summary, &selection),
        summary,
        selection,
        can_apply,
        kind,
        show_local_changes_warning,
    }
}

/// Builds the current sync coverage explanation from persisted scope options.
pub fn cloud_sync_coverage_model(raw_scope: &RawSyncScope) -> Vec<CloudSyncCoverageItem> {
    let scope = normalize_sync_scope(Some(raw_scope), &[]);
    let app_settings_status = if !scope.sync_app_settings {
        CloudSyncCoverageStatus::Excluded
    } else if scope.app_settings_sections.len() < OXIDE_APP_SETTINGS_SECTION_IDS.len() {
        CloudSyncCoverageStatus::Partial
    } else {
        CloudSyncCoverageStatus::Included
    };
    let plugin_settings_status = if !scope.sync_plugin_settings {
        CloudSyncCoverageStatus::Excluded
    } else if scope.plugin_ids.as_ref().is_some_and(|ids| ids.is_empty()) {
        CloudSyncCoverageStatus::Excluded
    } else if scope.plugin_ids.is_some() {
        CloudSyncCoverageStatus::Partial
    } else {
        CloudSyncCoverageStatus::Included
    };
    vec![
        CloudSyncCoverageItem {
            label_key: "plugin.cloud_sync.settings.sync_connections",
            status: coverage_status_from_bool(scope.sync_connections),
            detail: CloudSyncCoverageDetail::Static(
                "plugin.cloud_sync.coverage.connections_detail",
            ),
        },
        CloudSyncCoverageItem {
            label_key: "plugin.cloud_sync.settings.sync_forwards",
            status: coverage_status_from_bool(scope.sync_forwards),
            detail: CloudSyncCoverageDetail::Static("plugin.cloud_sync.coverage.forwards_detail"),
        },
        CloudSyncCoverageItem {
            label_key: "plugin.cloud_sync.settings.sync_quick_commands",
            status: coverage_status_from_bool(scope.sync_quick_commands),
            detail: CloudSyncCoverageDetail::Static(
                "plugin.cloud_sync.coverage.quick_commands_detail",
            ),
        },
        CloudSyncCoverageItem {
            label_key: "plugin.cloud_sync.settings.sync_serial_profiles",
            status: coverage_status_from_bool(scope.sync_serial_profiles),
            detail: CloudSyncCoverageDetail::Static(
                "plugin.cloud_sync.coverage.serial_profiles_detail",
            ),
        },
        CloudSyncCoverageItem {
            label_key: "plugin.cloud_sync.settings.sync_app_settings",
            status: app_settings_status,
            detail: CloudSyncCoverageDetail::AppSettingsSections(scope.app_settings_sections),
        },
        CloudSyncCoverageItem {
            label_key: "plugin.cloud_sync.settings.sync_plugin_settings",
            status: plugin_settings_status,
            detail: CloudSyncCoverageDetail::PluginSettings(scope.plugin_ids),
        },
        CloudSyncCoverageItem {
            label_key: "plugin.cloud_sync.settings.sync_sensitive_credentials",
            status: coverage_status_from_bool(scope.sync_sensitive_credentials),
            detail: CloudSyncCoverageDetail::Static(if scope.sync_sensitive_credentials {
                "plugin.cloud_sync.coverage.sensitive_credentials_enabled_detail"
            } else {
                "plugin.cloud_sync.coverage.sensitive_credentials_disabled_detail"
            }),
        },
    ]
}

pub(super) fn coverage_status_from_bool(enabled: bool) -> CloudSyncCoverageStatus {
    if enabled {
        CloudSyncCoverageStatus::Included
    } else {
        CloudSyncCoverageStatus::Excluded
    }
}

/// Explains what the current preview selection will actually apply.
pub fn cloud_sync_preview_impact_items(
    summary: &CloudSyncPreviewSummary,
    selection: &CloudSyncPreviewSelection,
) -> Vec<CloudSyncPreviewImpactItem> {
    let mut items = Vec::new();
    push_preview_impact(
        &mut items,
        "plugin.cloud_sync.preview.connection_count",
        summary.connections,
        selection.effective_import_connections(summary),
    );
    push_preview_impact(
        &mut items,
        "plugin.cloud_sync.preview.total_forwards",
        summary.forwards,
        selection.import_forwards,
    );
    push_preview_impact(
        &mut items,
        "plugin.cloud_sync.preview.quick_commands_label",
        summary.quick_commands,
        selection.import_quick_commands,
    );
    push_preview_impact(
        &mut items,
        "plugin.cloud_sync.preview.serial_profiles_label",
        summary.serial_profiles,
        selection.import_serial_profiles,
    );
    push_preview_impact(
        &mut items,
        "plugin.cloud_sync.preview.sensitive_credentials_label",
        summary.sensitive_credentials,
        selection.import_sensitive_credentials,
    );
    if summary.has_app_settings {
        let selected_count = summary
            .app_settings_sections
            .iter()
            .filter(|section| {
                selection
                    .selected_app_settings_sections
                    .contains(&section.id)
            })
            .count();
        items.push(CloudSyncPreviewImpactItem {
            label_key: "plugin.cloud_sync.settings.sync_app_settings",
            count: summary.app_settings_sections.len(),
            status: if !selection.import_app_settings || selected_count == 0 {
                CloudSyncCoverageStatus::Excluded
            } else if selected_count < summary.app_settings_sections.len() {
                CloudSyncCoverageStatus::Partial
            } else {
                CloudSyncCoverageStatus::Included
            },
        });
    }
    if summary.plugin_settings_count > 0 {
        let selected_count = summary
            .plugin_settings_by_plugin
            .keys()
            .filter(|plugin_id| selection.selected_plugin_ids.contains(*plugin_id))
            .count();
        items.push(CloudSyncPreviewImpactItem {
            label_key: "plugin.cloud_sync.preview.plugin_settings_label",
            count: summary.plugin_settings_count,
            status: if !selection.import_plugin_settings || selected_count == 0 {
                CloudSyncCoverageStatus::Excluded
            } else if selected_count < summary.plugin_settings_by_plugin.len() {
                CloudSyncCoverageStatus::Partial
            } else {
                CloudSyncCoverageStatus::Included
            },
        });
    }
    items
}

/// Builds the section-level upload plan from local revisions and known remote revisions.
pub fn cloud_sync_upload_diff_items(
    snapshot: &CloudSyncLocalSnapshot,
    state: &CloudSyncPersistedState,
) -> Vec<CloudSyncSectionDiffItem> {
    let baseline = state.last_synced_structured_state.as_ref();
    let remote = state.remote_section_revisions.as_ref();
    let remote_known = remote.is_some() || state.remote_exists || state.last_check_at.is_some();
    let current = &snapshot.dirty.current_state;
    let scope = &snapshot.scope;
    let mut items = Vec::new();

    push_section_diff(
        &mut items,
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_connections"),
        scope.sync_connections,
        current.connections.as_deref(),
        baseline.and_then(|state| state.connections.as_deref()),
        remote.and_then(|sections| sections.connections.as_deref()),
        remote_known,
        Some(snapshot.connections_record_count),
    );
    push_section_diff(
        &mut items,
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_forwards"),
        scope.sync_forwards,
        current.forwards.as_deref(),
        baseline.and_then(|state| state.forwards.as_deref()),
        remote.and_then(|sections| sections.forwards.as_deref()),
        remote_known,
        Some(snapshot.forwards_record_count),
    );
    push_section_diff(
        &mut items,
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_quick_commands"),
        scope.sync_quick_commands,
        current.quick_commands.as_deref(),
        baseline.and_then(|state| state.quick_commands.as_deref()),
        remote.and_then(|sections| sections.quick_commands.as_deref()),
        remote_known,
        Some(snapshot.quick_commands_record_count),
    );
    push_section_diff(
        &mut items,
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_serial_profiles"),
        scope.sync_serial_profiles,
        current.serial_profiles.as_deref(),
        baseline.and_then(|state| state.serial_profiles.as_deref()),
        remote.and_then(|sections| sections.serial_profiles.as_deref()),
        remote_known,
        Some(snapshot.serial_profiles_record_count),
    );
    push_section_diff(
        &mut items,
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_sensitive_credentials"),
        scope.sync_sensitive_credentials,
        current.sensitive_credentials.as_deref(),
        baseline.and_then(|state| state.sensitive_credentials.as_deref()),
        remote.and_then(|sections| sections.sensitive_credentials.as_deref()),
        remote_known,
        Some(snapshot.sensitive_credentials_record_count),
    );
    push_app_settings_diff_items(&mut items, scope, current, baseline, remote, remote_known);
    push_plugin_settings_diff_items(&mut items, scope, current, baseline, remote, remote_known);
    items
}

/// Builds the section-level apply plan by comparing remote preview revisions to local revisions.
pub fn cloud_sync_apply_diff_items(
    preview: &CloudSyncPendingPreview,
    selection: &CloudSyncPreviewSelection,
    snapshot: Option<&CloudSyncLocalSnapshot>,
) -> Vec<CloudSyncSectionDiffItem> {
    let (CloudSyncPendingPreview::Structured(preview), Some(snapshot)) = (preview, snapshot) else {
        return Vec::new();
    };
    let local = &snapshot.dirty.current_state;
    let remote = &preview.manifest.section_revisions;
    let mut items = Vec::new();

    push_apply_section_diff(
        &mut items,
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_connections"),
        selection.import_connections,
        remote.connections.as_deref(),
        local.connections.as_deref(),
        Some(
            preview
                .connections_snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.records.len()),
        ),
    );
    push_apply_section_diff(
        &mut items,
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_forwards"),
        selection.import_forwards,
        remote.forwards.as_deref(),
        local.forwards.as_deref(),
        Some(
            preview
                .forwards_snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.records.len()),
        ),
    );
    push_apply_section_diff(
        &mut items,
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_quick_commands"),
        selection.import_quick_commands,
        remote.quick_commands.as_deref(),
        local.quick_commands.as_deref(),
        None,
    );
    push_apply_section_diff(
        &mut items,
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_serial_profiles"),
        selection.import_serial_profiles,
        remote.serial_profiles.as_deref(),
        local.serial_profiles.as_deref(),
        Some(
            preview
                .serial_profiles_snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.records.len()),
        ),
    );
    push_apply_section_diff(
        &mut items,
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_sensitive_credentials"),
        selection.import_sensitive_credentials,
        remote.sensitive_credentials.as_deref(),
        local.sensitive_credentials.as_deref(),
        preview
            .sensitive_credentials_preview
            .as_ref()
            .map(|preview| preview.records.len()),
    );
    for section_id in OXIDE_APP_SETTINGS_SECTION_IDS {
        let section_id = (*section_id).to_string();
        push_apply_section_diff(
            &mut items,
            CloudSyncDiffLabel::AppSettingsSection(section_id.clone()),
            selection.import_app_settings
                && selection
                    .selected_app_settings_sections
                    .contains(&section_id),
            remote.app_settings.get(&section_id).map(String::as_str),
            local
                .app_settings
                .get(&section_id)
                .and_then(Option::as_deref),
            preview
                .app_settings_sections
                .get(&section_id)
                .map(|preview| preview.field_values.len()),
        );
    }
    let plugin_ids = diff_plugin_ids(
        local.plugin_settings.keys(),
        remote.plugin_settings.keys(),
        selection.selected_plugin_ids.iter(),
    );
    for plugin_id in plugin_ids {
        push_apply_section_diff(
            &mut items,
            CloudSyncDiffLabel::PluginSettings(plugin_id.clone()),
            selection.import_plugin_settings && selection.selected_plugin_ids.contains(&plugin_id),
            remote.plugin_settings.get(&plugin_id).map(String::as_str),
            local
                .plugin_settings
                .get(&plugin_id)
                .and_then(Option::as_deref),
            preview.plugin_settings_counts.get(&plugin_id).copied(),
        );
    }
    items
}

/// Builds item/field-level diffs for the selected structured apply preview.
///
/// Only selected sections are expanded. Secret material is intentionally not
/// included; auth and managed-key changes are represented by metadata fields.
pub fn cloud_sync_apply_field_diff_items(
    preview: &CloudSyncPendingPreview,
    selection: &CloudSyncPreviewSelection,
    local: &CloudSyncLocalFieldDiffSnapshot,
) -> Vec<CloudSyncFieldDiffItem> {
    let CloudSyncPendingPreview::Structured(preview) = preview else {
        return Vec::new();
    };
    let mut items = Vec::new();
    if selection.import_connections
        && let Some(remote) = preview.connections_snapshot.as_ref()
    {
        push_connection_field_diffs(
            &mut items,
            remote,
            preview.base_connections_snapshot.as_ref(),
            local.connections.as_ref(),
            &selection.conflict_strategy,
        );
    }
    if selection.import_forwards
        && let Some(remote) = preview.forwards_snapshot.as_ref()
    {
        push_forward_field_diffs(
            &mut items,
            remote,
            preview.base_forwards_snapshot.as_ref(),
            local.forwards.as_ref(),
            &selection.conflict_strategy,
        );
    }
    if selection.import_quick_commands
        && let Some(remote_json) = preview.quick_commands_snapshot_json.as_deref()
        && let Ok(remote) = serde_json::from_str::<QuickCommandsSnapshot>(remote_json)
    {
        let base = preview
            .base_quick_commands_snapshot_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<QuickCommandsSnapshot>(json).ok());
        push_quick_command_field_diffs(
            &mut items,
            &remote,
            base.as_ref(),
            local.quick_commands.as_ref(),
            &selection.conflict_strategy,
        );
    }
    if selection.import_serial_profiles
        && let Some(remote) = preview.serial_profiles_snapshot.as_ref()
    {
        push_serial_profile_field_diffs(
            &mut items,
            remote,
            preview.base_serial_profiles_snapshot.as_ref(),
            local.serial_profiles.as_ref(),
            &selection.conflict_strategy,
        );
    }
    if selection.import_app_settings {
        push_app_settings_field_diffs(&mut items, preview, selection, local);
    }
    items
}

/// Builds item/field-level diffs for an upload preview.
///
/// The remote structured preview is the before side and the current local
/// snapshot is the after side. Secret-bearing sections remain section-level
/// only; this function only expands non-secret content.
pub fn cloud_sync_upload_field_diff_items(
    remote_preview: &CloudSyncPendingPreview,
    local: &CloudSyncLocalFieldDiffSnapshot,
    scope: &SyncScope,
) -> Vec<CloudSyncFieldDiffItem> {
    let CloudSyncPendingPreview::Structured(remote_preview) = remote_preview else {
        return Vec::new();
    };
    let mut items = Vec::new();
    if scope.sync_connections
        && let Some(local) = local.connections.as_ref()
    {
        push_upload_connection_field_diffs(
            &mut items,
            remote_preview.connections_snapshot.as_ref(),
            local,
        );
    }
    if scope.sync_forwards
        && let Some(local) = local.forwards.as_ref()
    {
        push_upload_forward_field_diffs(
            &mut items,
            remote_preview.forwards_snapshot.as_ref(),
            local,
        );
    }
    if scope.sync_quick_commands
        && let Some(local) = local.quick_commands.as_ref()
    {
        let remote = remote_preview
            .quick_commands_snapshot_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<QuickCommandsSnapshot>(json).ok());
        push_upload_quick_command_field_diffs(&mut items, remote.as_ref(), local);
    }
    if scope.sync_serial_profiles
        && let Some(local) = local.serial_profiles.as_ref()
    {
        push_upload_serial_profile_field_diffs(
            &mut items,
            remote_preview.serial_profiles_snapshot.as_ref(),
            local,
        );
    }
    if scope.sync_app_settings {
        push_upload_app_settings_field_diffs(&mut items, remote_preview, local, scope);
    }
    items
}
