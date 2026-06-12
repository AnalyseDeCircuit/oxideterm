// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync preview DTOs and summaries.

use std::collections::{BTreeMap, BTreeSet};

use oxideterm_cloud_sync::{
    OXIDE_APP_SETTINGS_SECTION_IDS, PREVIEW_RECORD_LIMIT, RawSyncScope, StructuredSectionRevisions,
    SyncScope, normalize_sync_scope,
    operation::{LegacyPreview, StructuredPreview},
    service::CloudSyncLocalSnapshot,
    state::CloudSyncPersistedState,
};

use crate::selection::CloudSyncPreviewSelection;

#[derive(Clone, Debug)]
pub enum CloudSyncPendingPreview {
    Structured(StructuredPreview),
    Legacy {
        preview: LegacyPreview,
        source: CloudSyncPreviewSource,
    },
}

impl CloudSyncPendingPreview {
    pub fn is_backup(&self) -> bool {
        matches!(
            self,
            Self::Legacy {
                source: CloudSyncPreviewSource::Backup { .. },
                ..
            }
        )
    }
}

#[derive(Clone, Debug)]
pub enum CloudSyncPreviewSource {
    Remote,
    Backup { id: String, created_at: String },
}

impl CloudSyncPreviewSource {
    pub fn is_backup(&self) -> bool {
        matches!(self, Self::Backup { .. })
    }
}

#[derive(Clone, Debug, Default)]
pub struct CloudSyncPreviewSummary {
    pub connections: usize,
    pub forwards: usize,
    pub quick_commands: usize,
    pub serial_profiles: usize,
    pub sensitive_credentials: usize,
    pub has_app_settings: bool,
    pub app_settings_sections: Vec<CloudSyncAppSettingsSection>,
    pub plugin_settings_count: usize,
    pub plugin_settings_by_plugin: BTreeMap<String, usize>,
    pub has_embedded_keys: bool,
    pub forward_details: Vec<CloudSyncForwardDetail>,
    pub records: Vec<CloudSyncPreviewRecord>,
}

#[derive(Clone, Debug)]
pub struct CloudSyncAppSettingsSection {
    pub id: String,
    pub field_count: usize,
}

#[derive(Clone, Debug)]
pub struct CloudSyncForwardDetail {
    pub owner_connection_name: String,
    pub direction: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewRecord {
    pub resource: String,
    pub name: String,
    pub action: String,
    pub reason_code: String,
    pub target_name: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncPreviewCardKind {
    Import,
    Rollback,
}

#[derive(Clone, Debug)]
pub struct CloudSyncPreviewCardModel {
    pub summary: CloudSyncPreviewSummary,
    pub selection: CloudSyncPreviewSelection,
    pub can_apply: bool,
    pub kind: CloudSyncPreviewCardKind,
    pub copy: CloudSyncPreviewCardCopySpec,
    pub fact_rows: Vec<Vec<CloudSyncPreviewFactSpec>>,
    pub body_sections: Vec<CloudSyncPreviewBodySection>,
    pub impact_items: Vec<CloudSyncPreviewImpactItem>,
    pub show_local_changes_warning: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewCardCopySpec {
    pub title_identity: &'static str,
    pub title_key: &'static str,
    pub apply_label_key: &'static str,
    pub warning_key: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewFactSpec {
    pub label_key: &'static str,
    pub value: CloudSyncPreviewFactValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncPreviewFactValue {
    Count(usize),
    YesNo(bool),
}

#[derive(Clone, Debug)]
pub enum CloudSyncPreviewBodySection {
    Selection,
    ForwardDetails(Vec<CloudSyncForwardDetail>),
    RecordGroup {
        action: &'static str,
        records: Vec<CloudSyncPreviewRecord>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncCoverageStatus {
    Included,
    Excluded,
    Partial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncCoverageDetail {
    Static(&'static str),
    AppSettingsSections(Vec<String>),
    PluginSettings(Option<Vec<String>>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncCoverageItem {
    pub label_key: &'static str,
    pub status: CloudSyncCoverageStatus,
    pub detail: CloudSyncCoverageDetail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewImpactItem {
    pub label_key: &'static str,
    pub count: usize,
    pub status: CloudSyncCoverageStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncDiffLabel {
    Key(&'static str),
    AppSettingsSection(String),
    PluginSettings(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncLocalDiffStatus {
    Added,
    Modified,
    Deleted,
    Unchanged,
    Excluded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncRemoteDiffStatus {
    Creates,
    Overwrites,
    Unchanged,
    RemovedByScope,
    Excluded,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncSectionDiffItem {
    pub label: CloudSyncDiffLabel,
    pub local_status: CloudSyncLocalDiffStatus,
    pub remote_status: CloudSyncRemoteDiffStatus,
    pub count: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncForwardDetailRow {
    pub title: String,
    pub meta: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncPreviewRecordRow {
    Connection {
        record: CloudSyncPreviewRecord,
        checked: bool,
        disabled: bool,
    },
    Item {
        record: CloudSyncPreviewRecord,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewListModel<T> {
    pub rows: Vec<T>,
    pub overflow_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewRecordGroupModel {
    pub title_key: &'static str,
    pub rows: Vec<CloudSyncPreviewRecordRow>,
    pub overflow_count: usize,
}

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

fn coverage_status_from_bool(enabled: bool) -> CloudSyncCoverageStatus {
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

fn push_preview_impact(
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

fn push_app_settings_diff_items(
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

fn push_plugin_settings_diff_items(
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

fn push_section_diff(
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

fn push_apply_section_diff(
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

fn local_diff_status(
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

fn upload_remote_diff_status(
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

fn diff_plugin_ids<'a>(
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

#[cfg(test)]
mod tests {
    use oxideterm_cloud_sync::{
        ConflictStrategy, RawSyncScope, StructuredDirtyInfo, StructuredDirtySections,
        StructuredLocalState, StructuredSectionRevisions, SyncScope,
        service::CloudSyncLocalSnapshot, state::CloudSyncPersistedState,
    };

    use super::*;

    #[test]
    fn preview_fact_rows_preserve_display_order() {
        let summary = CloudSyncPreviewSummary {
            connections: 2,
            forwards: 3,
            plugin_settings_count: 4,
            has_embedded_keys: true,
            ..CloudSyncPreviewSummary::default()
        };

        assert_eq!(
            cloud_sync_preview_fact_rows(&summary),
            vec![
                vec![
                    CloudSyncPreviewFactSpec {
                        label_key: "plugin.cloud_sync.preview.connection_count",
                        value: CloudSyncPreviewFactValue::Count(2),
                    },
                    CloudSyncPreviewFactSpec {
                        label_key: "plugin.cloud_sync.preview.total_forwards",
                        value: CloudSyncPreviewFactValue::Count(3),
                    },
                ],
                vec![
                    CloudSyncPreviewFactSpec {
                        label_key: "plugin.cloud_sync.preview.plugin_settings_label",
                        value: CloudSyncPreviewFactValue::Count(4),
                    },
                    CloudSyncPreviewFactSpec {
                        label_key: "plugin.cloud_sync.preview.embedded_keys_label",
                        value: CloudSyncPreviewFactValue::YesNo(true),
                    },
                ],
            ]
        );
    }

    #[test]
    fn preview_body_sections_keep_selection_first() {
        let summary = CloudSyncPreviewSummary {
            forward_details: vec![CloudSyncForwardDetail {
                owner_connection_name: "prod".to_string(),
                direction: "local".to_string(),
                description: "Local tunnel".to_string(),
            }],
            records: vec![CloudSyncPreviewRecord {
                resource: "connection".to_string(),
                name: "prod".to_string(),
                action: "import".to_string(),
                reason_code: "new".to_string(),
                target_name: None,
            }],
            ..CloudSyncPreviewSummary::default()
        };

        let sections = cloud_sync_preview_body_sections(&summary);

        assert!(matches!(
            sections[0],
            CloudSyncPreviewBodySection::Selection
        ));
        assert!(matches!(
            sections[1],
            CloudSyncPreviewBodySection::ForwardDetails(_)
        ));
        assert!(matches!(
            sections[2],
            CloudSyncPreviewBodySection::RecordGroup {
                action: "import",
                ..
            }
        ));
    }

    #[test]
    fn coverage_model_marks_partial_sections_and_sensitive_exclusion() {
        let raw_scope = RawSyncScope {
            app_settings_sections: Some(vec!["general".to_string(), "network".to_string()]),
            sync_sensitive_credentials: Some(false),
            ..RawSyncScope::default()
        };

        let items = cloud_sync_coverage_model(&raw_scope);

        let app_settings = items
            .iter()
            .find(|item| item.label_key == "plugin.cloud_sync.settings.sync_app_settings")
            .expect("app settings coverage item");
        assert_eq!(app_settings.status, CloudSyncCoverageStatus::Partial);
        assert_eq!(
            app_settings.detail,
            CloudSyncCoverageDetail::AppSettingsSections(vec![
                "general".to_string(),
                "network".to_string()
            ])
        );

        let sensitive = items
            .iter()
            .find(|item| item.label_key == "plugin.cloud_sync.settings.sync_sensitive_credentials")
            .expect("sensitive credentials coverage item");
        assert_eq!(sensitive.status, CloudSyncCoverageStatus::Excluded);
    }

    #[test]
    fn preview_impact_items_explain_excluded_and_partial_selection() {
        let summary = CloudSyncPreviewSummary {
            connections: 2,
            forwards: 1,
            quick_commands: 3,
            has_app_settings: true,
            app_settings_sections: vec![
                CloudSyncAppSettingsSection {
                    id: "general".to_string(),
                    field_count: 2,
                },
                CloudSyncAppSettingsSection {
                    id: "network".to_string(),
                    field_count: 1,
                },
            ],
            ..CloudSyncPreviewSummary::default()
        };
        let mut selection = CloudSyncPreviewSelection {
            import_connections: true,
            selected_connection_names: summary.connection_record_names(),
            import_quick_commands: false,
            import_serial_profiles: false,
            import_sensitive_credentials: false,
            import_app_settings: true,
            selected_app_settings_sections: ["general".to_string()].into_iter().collect(),
            import_plugin_settings: false,
            selected_plugin_ids: Default::default(),
            import_forwards: true,
            conflict_strategy: ConflictStrategy::Merge,
        };

        let items = cloud_sync_preview_impact_items(&summary, &selection);

        assert!(items.iter().any(|item| {
            item.label_key == "plugin.cloud_sync.preview.quick_commands_label"
                && item.status == CloudSyncCoverageStatus::Excluded
        }));
        assert!(items.iter().any(|item| {
            item.label_key == "plugin.cloud_sync.settings.sync_app_settings"
                && item.status == CloudSyncCoverageStatus::Partial
        }));

        selection.selected_app_settings_sections.clear();
        let items = cloud_sync_preview_impact_items(&summary, &selection);
        assert!(items.iter().any(|item| {
            item.label_key == "plugin.cloud_sync.settings.sync_app_settings"
                && item.status == CloudSyncCoverageStatus::Excluded
        }));
    }

    #[test]
    fn upload_diff_items_mark_local_changes_and_remote_overwrites() {
        let snapshot = test_snapshot(
            SyncScope::default(),
            StructuredLocalState {
                connections: Some("local-connections-2".to_string()),
                forwards: Some("forwards-1".to_string()),
                ..StructuredLocalState::default()
            },
        );
        let state = CloudSyncPersistedState {
            last_check_at: Some("2026-06-12T00:00:00Z".to_string()),
            last_synced_structured_state: Some(StructuredLocalState {
                connections: Some("local-connections-1".to_string()),
                forwards: Some("forwards-1".to_string()),
                ..StructuredLocalState::default()
            }),
            remote_section_revisions: Some(StructuredSectionRevisions {
                connections: Some("remote-connections".to_string()),
                forwards: Some("forwards-1".to_string()),
                ..StructuredSectionRevisions::default()
            }),
            ..CloudSyncPersistedState::default()
        };

        let items = cloud_sync_upload_diff_items(&snapshot, &state);

        let connections = items
            .iter()
            .find(|item| {
                item.label == CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_connections")
            })
            .expect("connections diff item");
        assert_eq!(connections.local_status, CloudSyncLocalDiffStatus::Modified);
        assert_eq!(
            connections.remote_status,
            CloudSyncRemoteDiffStatus::Overwrites
        );
        let forwards = items
            .iter()
            .find(|item| {
                item.label == CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_forwards")
            })
            .expect("forwards diff item");
        assert_eq!(forwards.local_status, CloudSyncLocalDiffStatus::Unchanged);
        assert_eq!(forwards.remote_status, CloudSyncRemoteDiffStatus::Unchanged);
    }

    #[test]
    fn upload_diff_items_show_scope_exclusions_that_remove_remote_sections() {
        let mut scope = SyncScope::default();
        scope.sync_sensitive_credentials = false;
        let snapshot = test_snapshot(scope, StructuredLocalState::default());
        let state = CloudSyncPersistedState {
            last_check_at: Some("2026-06-12T00:00:00Z".to_string()),
            remote_section_revisions: Some(StructuredSectionRevisions {
                sensitive_credentials: Some("remote-secrets".to_string()),
                ..StructuredSectionRevisions::default()
            }),
            ..CloudSyncPersistedState::default()
        };

        let items = cloud_sync_upload_diff_items(&snapshot, &state);

        let sensitive = items
            .iter()
            .find(|item| {
                item.label
                    == CloudSyncDiffLabel::Key(
                        "plugin.cloud_sync.settings.sync_sensitive_credentials",
                    )
            })
            .expect("sensitive credentials diff item");
        assert_eq!(sensitive.local_status, CloudSyncLocalDiffStatus::Excluded);
        assert_eq!(
            sensitive.remote_status,
            CloudSyncRemoteDiffStatus::RemovedByScope
        );
    }

    fn test_snapshot(
        scope: SyncScope,
        current_state: StructuredLocalState,
    ) -> CloudSyncLocalSnapshot {
        CloudSyncLocalSnapshot {
            metadata: Default::default(),
            scope,
            dirty: StructuredDirtyInfo {
                current_state,
                dirty_sections: StructuredDirtySections::default(),
                has_dirty: true,
            },
            upload_units: 0,
            connections_record_count: 2,
            forwards_record_count: 1,
            quick_commands_record_count: 0,
            serial_profiles_record_count: 0,
            sensitive_credentials_record_count: 0,
        }
    }
}
