// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync preview DTOs and summaries.

use std::collections::{BTreeMap, BTreeSet};

use oxideterm_cloud_sync::{
    ConflictStrategy, OXIDE_APP_SETTINGS_SECTION_IDS, PREVIEW_RECORD_LIMIT, RawSyncScope,
    StructuredSectionRevisions, SyncScope, normalize_sync_scope,
    operation::{LegacyPreview, StructuredPreview, merge_structured_model_fields},
    service::CloudSyncLocalSnapshot,
    state::CloudSyncPersistedState,
};
use oxideterm_connections::{
    ConnectionInfo, RawTcpProfile, RawTcpProfilesSyncSnapshot, SavedConnectionsSyncSnapshot,
    SerialProfile, SerialProfilesSyncSnapshot, oxide_file::AppSettingsSectionPreview,
};
use oxideterm_forwarding::{PersistedForwardDto, SavedForwardsSyncSnapshot};
use oxideterm_quick_commands::{QuickCommand, QuickCommandsSnapshot};

use crate::selection::CloudSyncPreviewSelection;

pub const CLOUD_SYNC_FIELD_REDACTED_VALUE: &str = "<redacted>";

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
    pub raw_tcp_profiles: usize,
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

#[derive(Clone, Debug, Default)]
pub struct CloudSyncLocalFieldDiffSnapshot {
    pub connections: Option<SavedConnectionsSyncSnapshot>,
    pub forwards: Option<SavedForwardsSyncSnapshot>,
    pub quick_commands: Option<QuickCommandsSnapshot>,
    pub serial_profiles: Option<SerialProfilesSyncSnapshot>,
    pub raw_tcp_profiles: Option<RawTcpProfilesSyncSnapshot>,
    pub app_settings_sections: Vec<AppSettingsSectionPreview>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncFieldDiffStatus {
    Added,
    Modified,
    Deleted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncFieldDiffItem {
    pub section_label_key: &'static str,
    pub item_key: String,
    pub item_name: String,
    pub status: CloudSyncFieldDiffStatus,
    pub fields: Vec<CloudSyncFieldDiffField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncFieldDiffField {
    pub label_key: &'static str,
    pub before: Option<String>,
    pub after: Option<String>,
    pub merge_outcome: Option<CloudSyncFieldMergeOutcome>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudSyncFieldMergeOutcome {
    Remote,
    Local,
    Merged,
    ConflictLocal,
    ConflictRemote,
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
            label_key: "plugin.cloud_sync.settings.sync_raw_tcp_profiles",
            status: coverage_status_from_bool(scope.sync_raw_tcp_profiles),
            detail: CloudSyncCoverageDetail::Static(
                "plugin.cloud_sync.coverage.raw_tcp_profiles_detail",
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
        "plugin.cloud_sync.preview.raw_tcp_profiles_label",
        summary.raw_tcp_profiles,
        selection.import_raw_tcp_profiles,
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
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_raw_tcp_profiles"),
        scope.sync_raw_tcp_profiles,
        current.raw_tcp_profiles.as_deref(),
        baseline.and_then(|state| state.raw_tcp_profiles.as_deref()),
        remote.and_then(|sections| sections.raw_tcp_profiles.as_deref()),
        remote_known,
        Some(snapshot.raw_tcp_profiles_record_count),
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
        CloudSyncDiffLabel::Key("plugin.cloud_sync.settings.sync_raw_tcp_profiles"),
        selection.import_raw_tcp_profiles,
        remote.raw_tcp_profiles.as_deref(),
        local.raw_tcp_profiles.as_deref(),
        Some(
            preview
                .raw_tcp_profiles_snapshot
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
    if selection.import_raw_tcp_profiles
        && let Some(remote) = preview.raw_tcp_profiles_snapshot.as_ref()
    {
        push_raw_tcp_profile_field_diffs(
            &mut items,
            remote,
            preview.base_raw_tcp_profiles_snapshot.as_ref(),
            local.raw_tcp_profiles.as_ref(),
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
    if scope.sync_raw_tcp_profiles
        && let Some(local) = local.raw_tcp_profiles.as_ref()
    {
        push_upload_raw_tcp_profile_field_diffs(
            &mut items,
            remote_preview.raw_tcp_profiles_snapshot.as_ref(),
            local,
        );
    }
    if scope.sync_app_settings {
        push_upload_app_settings_field_diffs(&mut items, remote_preview, local, scope);
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

fn push_upload_connection_field_diffs(
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

fn push_connection_field_diffs(
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

fn push_upload_forward_field_diffs(
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

fn push_forward_field_diffs(
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

fn push_upload_quick_command_field_diffs(
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

fn push_quick_command_field_diffs(
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

fn push_upload_serial_profile_field_diffs(
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

fn push_serial_profile_field_diffs(
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

fn push_upload_raw_tcp_profile_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    remote: Option<&RawTcpProfilesSyncSnapshot>,
    local: &RawTcpProfilesSyncSnapshot,
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
            .map(|remote_profile| raw_tcp_profile_changed_fields(remote_profile, local_profile))
            .unwrap_or_else(|| raw_tcp_profile_summary_fields(local_profile));
        let status = if remote_profile.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_raw_tcp_profiles",
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
            "plugin.cloud_sync.settings.sync_raw_tcp_profiles",
            remote_profile.id.clone(),
            remote_profile.name.clone(),
            CloudSyncFieldDiffStatus::Deleted,
            Vec::new(),
        ));
    }
}

fn push_raw_tcp_profile_field_diffs(
    items: &mut Vec<CloudSyncFieldDiffItem>,
    remote: &RawTcpProfilesSyncSnapshot,
    base: Option<&RawTcpProfilesSyncSnapshot>,
    local: Option<&RawTcpProfilesSyncSnapshot>,
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
            (Some(base_profile), Some(local_profile)) => raw_tcp_profile_merge_fields(
                base_profile,
                local_profile,
                remote_profile,
                &effective_remote,
                conflict_strategy,
            ),
            (_, Some(local_profile)) => {
                raw_tcp_profile_changed_fields(local_profile, &effective_remote)
            }
            _ => raw_tcp_profile_summary_fields(remote_profile),
        };
        let status = if local_profile.is_some() {
            CloudSyncFieldDiffStatus::Modified
        } else {
            CloudSyncFieldDiffStatus::Added
        };
        push_non_empty_field_diff(
            items,
            "plugin.cloud_sync.settings.sync_raw_tcp_profiles",
            remote_profile.id.clone(),
            remote_profile.name.clone(),
            status,
            fields,
        );
    }
}

fn push_upload_app_settings_field_diffs(
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

fn push_app_settings_field_diffs(
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

fn app_settings_changed_fields(
    before: &std::collections::HashMap<String, String>,
    after: &std::collections::HashMap<String, String>,
) -> Vec<CloudSyncFieldDiffField> {
    before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|field_key| {
            let before = before
                .get(&field_key)
                .map(|value| format!("{field_key}: {value}"));
            let after = after
                .get(&field_key)
                .map(|value| format!("{field_key}: {value}"));
            (before != after)
                .then(|| field("plugin.cloud_sync.diff_fields.setting_field", before, after))
        })
        .collect()
}

fn app_settings_summary_fields(
    values: &std::collections::HashMap<String, String>,
) -> Vec<CloudSyncFieldDiffField> {
    values
        .iter()
        .map(|(field_key, value)| {
            field(
                "plugin.cloud_sync.diff_fields.setting_field",
                None,
                Some(format!("{field_key}: {value}")),
            )
        })
        .collect()
}

fn connection_changed_fields(
    before: &ConnectionInfo,
    after: &ConnectionInfo,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.name",
        Some(before.name.clone()),
        Some(after.name.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.group",
        before.group.clone(),
        after.group.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.host",
        Some(before.host.clone()),
        Some(after.host.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.port",
        Some(before.port.to_string()),
        Some(after.port.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.username",
        Some(before.username.clone()),
        Some(after.username.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.auth_type",
        Some(format!("{:?}", before.auth_type)),
        Some(format!("{:?}", after.auth_type)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.key_path",
        before.key_path.clone(),
        after.key_path.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.cert_path",
        before.cert_path.clone(),
        after.cert_path.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.managed_key",
        before.managed_key_id.clone(),
        after.managed_key_id.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.proxy_chain",
        Some(before.proxy_chain.len().to_string()),
        Some(after.proxy_chain.len().to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.agent_forwarding",
        Some(before.agent_forwarding.to_string()),
        Some(after.agent_forwarding.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.post_connect_command",
        before
            .post_connect_command
            .as_ref()
            .map(|_| redacted_changed_value()),
        after
            .post_connect_command
            .as_ref()
            .map(|_| redacted_changed_value()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.color",
        before.color.clone(),
        after.color.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.tags",
        Some(before.tags.join(", ")),
        Some(after.tags.join(", ")),
    );
    fields
}

fn connection_merge_fields(
    base: &ConnectionInfo,
    local: &ConnectionInfo,
    remote: &ConnectionInfo,
    effective: &ConnectionInfo,
    conflict_strategy: &ConflictStrategy,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.name",
        Some(base.name.clone()),
        Some(local.name.clone()),
        Some(remote.name.clone()),
        Some(effective.name.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.group",
        base.group.clone(),
        local.group.clone(),
        remote.group.clone(),
        effective.group.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.host",
        Some(base.host.clone()),
        Some(local.host.clone()),
        Some(remote.host.clone()),
        Some(effective.host.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.port",
        Some(base.port.to_string()),
        Some(local.port.to_string()),
        Some(remote.port.to_string()),
        Some(effective.port.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.username",
        Some(base.username.clone()),
        Some(local.username.clone()),
        Some(remote.username.clone()),
        Some(effective.username.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.auth_type",
        Some(format!("{:?}", base.auth_type)),
        Some(format!("{:?}", local.auth_type)),
        Some(format!("{:?}", remote.auth_type)),
        Some(format!("{:?}", effective.auth_type)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.key_path",
        base.key_path.clone(),
        local.key_path.clone(),
        remote.key_path.clone(),
        effective.key_path.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.cert_path",
        base.cert_path.clone(),
        local.cert_path.clone(),
        remote.cert_path.clone(),
        effective.cert_path.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.managed_key",
        base.managed_key_id.clone(),
        local.managed_key_id.clone(),
        remote.managed_key_id.clone(),
        effective.managed_key_id.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.proxy_chain",
        Some(base.proxy_chain.len().to_string()),
        Some(local.proxy_chain.len().to_string()),
        Some(remote.proxy_chain.len().to_string()),
        Some(effective.proxy_chain.len().to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.agent_forwarding",
        Some(base.agent_forwarding.to_string()),
        Some(local.agent_forwarding.to_string()),
        Some(remote.agent_forwarding.to_string()),
        Some(effective.agent_forwarding.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.post_connect_command",
        redacted_presence(base.post_connect_command.as_ref()),
        redacted_presence(local.post_connect_command.as_ref()),
        redacted_presence(remote.post_connect_command.as_ref()),
        redacted_presence(effective.post_connect_command.as_ref()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.color",
        base.color.clone(),
        local.color.clone(),
        remote.color.clone(),
        effective.color.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.tags",
        Some(base.tags.join(", ")),
        Some(local.tags.join(", ")),
        Some(remote.tags.join(", ")),
        Some(effective.tags.join(", ")),
        conflict_strategy,
    );
    fields
}

fn connection_summary_fields(value: &ConnectionInfo) -> Vec<CloudSyncFieldDiffField> {
    vec![
        field(
            "plugin.cloud_sync.diff_fields.host",
            None,
            Some(value.host.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.port",
            None,
            Some(value.port.to_string()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.username",
            None,
            Some(value.username.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.auth_type",
            None,
            Some(format!("{:?}", value.auth_type)),
        ),
    ]
}

fn forward_changed_fields(
    before: &PersistedForwardDto,
    after: &PersistedForwardDto,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.forward_type",
        Some(before.forward_type.clone()),
        Some(after.forward_type.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.bind_address",
        Some(before.bind_address.clone()),
        Some(after.bind_address.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.bind_port",
        Some(before.bind_port.to_string()),
        Some(after.bind_port.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.target_host",
        Some(before.target_host.clone()),
        Some(after.target_host.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.target_port",
        Some(before.target_port.to_string()),
        Some(after.target_port.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.description",
        before.description.clone(),
        after.description.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.auto_start",
        Some(before.auto_start.to_string()),
        Some(after.auto_start.to_string()),
    );
    fields
}

fn forward_merge_fields(
    base: &PersistedForwardDto,
    local: &PersistedForwardDto,
    remote: &PersistedForwardDto,
    effective: &PersistedForwardDto,
    conflict_strategy: &ConflictStrategy,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.forward_type",
        Some(base.forward_type.clone()),
        Some(local.forward_type.clone()),
        Some(remote.forward_type.clone()),
        Some(effective.forward_type.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.bind_address",
        Some(base.bind_address.clone()),
        Some(local.bind_address.clone()),
        Some(remote.bind_address.clone()),
        Some(effective.bind_address.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.bind_port",
        Some(base.bind_port.to_string()),
        Some(local.bind_port.to_string()),
        Some(remote.bind_port.to_string()),
        Some(effective.bind_port.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.target_host",
        Some(base.target_host.clone()),
        Some(local.target_host.clone()),
        Some(remote.target_host.clone()),
        Some(effective.target_host.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.target_port",
        Some(base.target_port.to_string()),
        Some(local.target_port.to_string()),
        Some(remote.target_port.to_string()),
        Some(effective.target_port.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.description",
        base.description.clone(),
        local.description.clone(),
        remote.description.clone(),
        effective.description.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.auto_start",
        Some(base.auto_start.to_string()),
        Some(local.auto_start.to_string()),
        Some(remote.auto_start.to_string()),
        Some(effective.auto_start.to_string()),
        conflict_strategy,
    );
    fields
}

fn forward_summary_fields(value: &PersistedForwardDto) -> Vec<CloudSyncFieldDiffField> {
    vec![
        field(
            "plugin.cloud_sync.diff_fields.forward_type",
            None,
            Some(value.forward_type.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.bind_address",
            None,
            Some(value.bind_address.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.bind_port",
            None,
            Some(value.bind_port.to_string()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.target_host",
            None,
            Some(value.target_host.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.target_port",
            None,
            Some(value.target_port.to_string()),
        ),
    ]
}

fn quick_command_changed_fields(
    before: &QuickCommand,
    after: &QuickCommand,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.name",
        Some(before.name.clone()),
        Some(after.name.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.command",
        Some(before.command.clone()),
        Some(after.command.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.category",
        Some(before.category.clone()),
        Some(after.category.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.description",
        before.description.clone(),
        after.description.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.host_pattern",
        before.host_pattern.clone(),
        after.host_pattern.clone(),
    );
    fields
}

fn quick_command_merge_fields(
    base: &QuickCommand,
    local: &QuickCommand,
    remote: &QuickCommand,
    effective: &QuickCommand,
    conflict_strategy: &ConflictStrategy,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.name",
        Some(base.name.clone()),
        Some(local.name.clone()),
        Some(remote.name.clone()),
        Some(effective.name.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.command",
        Some(base.command.clone()),
        Some(local.command.clone()),
        Some(remote.command.clone()),
        Some(effective.command.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.category",
        Some(base.category.clone()),
        Some(local.category.clone()),
        Some(remote.category.clone()),
        Some(effective.category.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.description",
        base.description.clone(),
        local.description.clone(),
        remote.description.clone(),
        effective.description.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.host_pattern",
        base.host_pattern.clone(),
        local.host_pattern.clone(),
        remote.host_pattern.clone(),
        effective.host_pattern.clone(),
        conflict_strategy,
    );
    fields
}

fn quick_command_summary_fields(value: &QuickCommand) -> Vec<CloudSyncFieldDiffField> {
    vec![
        field(
            "plugin.cloud_sync.diff_fields.command",
            None,
            Some(value.command.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.category",
            None,
            Some(value.category.clone()),
        ),
    ]
}

fn serial_profile_changed_fields(
    before: &SerialProfile,
    after: &SerialProfile,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.name",
        Some(before.name.clone()),
        Some(after.name.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.group",
        before.group.clone(),
        after.group.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.port_path",
        Some(before.port_path.clone()),
        Some(after.port_path.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.baud_rate",
        Some(before.baud_rate.to_string()),
        Some(after.baud_rate.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.data_bits",
        Some(before.data_bits.to_string()),
        Some(after.data_bits.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.stop_bits",
        Some(before.stop_bits.to_string()),
        Some(after.stop_bits.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.parity",
        Some(format!("{:?}", before.parity)),
        Some(format!("{:?}", after.parity)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.flow_control",
        Some(format!("{:?}", before.flow_control)),
        Some(format!("{:?}", after.flow_control)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.connect_on_open",
        Some(before.connect_on_open.to_string()),
        Some(after.connect_on_open.to_string()),
    );
    fields
}

fn serial_profile_merge_fields(
    base: &SerialProfile,
    local: &SerialProfile,
    remote: &SerialProfile,
    effective: &SerialProfile,
    conflict_strategy: &ConflictStrategy,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.name",
        Some(base.name.clone()),
        Some(local.name.clone()),
        Some(remote.name.clone()),
        Some(effective.name.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.group",
        base.group.clone(),
        local.group.clone(),
        remote.group.clone(),
        effective.group.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.port_path",
        Some(base.port_path.clone()),
        Some(local.port_path.clone()),
        Some(remote.port_path.clone()),
        Some(effective.port_path.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.baud_rate",
        Some(base.baud_rate.to_string()),
        Some(local.baud_rate.to_string()),
        Some(remote.baud_rate.to_string()),
        Some(effective.baud_rate.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.data_bits",
        Some(base.data_bits.to_string()),
        Some(local.data_bits.to_string()),
        Some(remote.data_bits.to_string()),
        Some(effective.data_bits.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.stop_bits",
        Some(base.stop_bits.to_string()),
        Some(local.stop_bits.to_string()),
        Some(remote.stop_bits.to_string()),
        Some(effective.stop_bits.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.parity",
        Some(format!("{:?}", base.parity)),
        Some(format!("{:?}", local.parity)),
        Some(format!("{:?}", remote.parity)),
        Some(format!("{:?}", effective.parity)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.flow_control",
        Some(format!("{:?}", base.flow_control)),
        Some(format!("{:?}", local.flow_control)),
        Some(format!("{:?}", remote.flow_control)),
        Some(format!("{:?}", effective.flow_control)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.connect_on_open",
        Some(base.connect_on_open.to_string()),
        Some(local.connect_on_open.to_string()),
        Some(remote.connect_on_open.to_string()),
        Some(effective.connect_on_open.to_string()),
        conflict_strategy,
    );
    fields
}

fn serial_profile_summary_fields(value: &SerialProfile) -> Vec<CloudSyncFieldDiffField> {
    vec![
        field(
            "plugin.cloud_sync.diff_fields.port_path",
            None,
            Some(value.port_path.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.baud_rate",
            None,
            Some(value.baud_rate.to_string()),
        ),
    ]
}

fn raw_tcp_profile_changed_fields(
    before: &RawTcpProfile,
    after: &RawTcpProfile,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.name",
        Some(before.name.clone()),
        Some(after.name.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.group",
        before.group.clone(),
        after.group.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.host",
        Some(before.host.clone()),
        Some(after.host.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.port",
        Some(before.port.to_string()),
        Some(after.port.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.line_ending",
        Some(format!("{:?}", before.line_ending)),
        Some(format!("{:?}", after.line_ending)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.display_mode",
        Some(format!("{:?}", before.display_mode)),
        Some(format!("{:?}", after.display_mode)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.send_mode",
        Some(format!("{:?}", before.send_mode)),
        Some(format!("{:?}", after.send_mode)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.tls_mode",
        Some(format!("{:?}", before.tls_mode)),
        Some(format!("{:?}", after.tls_mode)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.tls_verification",
        Some(format!("{:?}", before.tls_verification)),
        Some(format!("{:?}", after.tls_verification)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.tls_server_name",
        before.tls_server_name.clone(),
        after.tls_server_name.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.connect_on_open",
        Some(before.connect_on_open.to_string()),
        Some(after.connect_on_open.to_string()),
    );
    fields
}

fn raw_tcp_profile_merge_fields(
    base: &RawTcpProfile,
    local: &RawTcpProfile,
    remote: &RawTcpProfile,
    effective: &RawTcpProfile,
    conflict_strategy: &ConflictStrategy,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.name",
        Some(base.name.clone()),
        Some(local.name.clone()),
        Some(remote.name.clone()),
        Some(effective.name.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.group",
        base.group.clone(),
        local.group.clone(),
        remote.group.clone(),
        effective.group.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.host",
        Some(base.host.clone()),
        Some(local.host.clone()),
        Some(remote.host.clone()),
        Some(effective.host.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.port",
        Some(base.port.to_string()),
        Some(local.port.to_string()),
        Some(remote.port.to_string()),
        Some(effective.port.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.line_ending",
        Some(format!("{:?}", base.line_ending)),
        Some(format!("{:?}", local.line_ending)),
        Some(format!("{:?}", remote.line_ending)),
        Some(format!("{:?}", effective.line_ending)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.display_mode",
        Some(format!("{:?}", base.display_mode)),
        Some(format!("{:?}", local.display_mode)),
        Some(format!("{:?}", remote.display_mode)),
        Some(format!("{:?}", effective.display_mode)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.send_mode",
        Some(format!("{:?}", base.send_mode)),
        Some(format!("{:?}", local.send_mode)),
        Some(format!("{:?}", remote.send_mode)),
        Some(format!("{:?}", effective.send_mode)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.tls_mode",
        Some(format!("{:?}", base.tls_mode)),
        Some(format!("{:?}", local.tls_mode)),
        Some(format!("{:?}", remote.tls_mode)),
        Some(format!("{:?}", effective.tls_mode)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.tls_verification",
        Some(format!("{:?}", base.tls_verification)),
        Some(format!("{:?}", local.tls_verification)),
        Some(format!("{:?}", remote.tls_verification)),
        Some(format!("{:?}", effective.tls_verification)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.tls_server_name",
        base.tls_server_name.clone(),
        local.tls_server_name.clone(),
        remote.tls_server_name.clone(),
        effective.tls_server_name.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.connect_on_open",
        Some(base.connect_on_open.to_string()),
        Some(local.connect_on_open.to_string()),
        Some(remote.connect_on_open.to_string()),
        Some(effective.connect_on_open.to_string()),
        conflict_strategy,
    );
    fields
}

fn raw_tcp_profile_summary_fields(value: &RawTcpProfile) -> Vec<CloudSyncFieldDiffField> {
    vec![
        field(
            "plugin.cloud_sync.diff_fields.host",
            None,
            Some(value.host.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.port",
            None,
            Some(value.port.to_string()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.tls_mode",
            None,
            Some(format!("{:?}", value.tls_mode)),
        ),
    ]
}

fn forward_item_name(value: &PersistedForwardDto) -> String {
    format!(
        "{} {}:{} -> {}:{}",
        value.forward_type,
        value.bind_address,
        value.bind_port,
        value.target_host,
        value.target_port
    )
}

fn push_non_empty_field_diff(
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

fn field_diff_item_with_key(
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

fn push_changed(
    fields: &mut Vec<CloudSyncFieldDiffField>,
    label_key: &'static str,
    before: Option<String>,
    after: Option<String>,
) {
    if before != after {
        fields.push(field(label_key, before, after));
    }
}

fn push_merge_changed(
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

fn merge_outcome_for_values(
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

fn field(
    label_key: &'static str,
    before: Option<String>,
    after: Option<String>,
) -> CloudSyncFieldDiffField {
    field_with_merge_outcome(label_key, before, after, None)
}

fn field_with_merge_outcome(
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

fn redacted_changed_value() -> String {
    CLOUD_SYNC_FIELD_REDACTED_VALUE.to_string()
}

fn redacted_presence<T>(value: Option<T>) -> Option<String> {
    value.map(|_| redacted_changed_value())
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
        || summary.raw_tcp_profiles > 0
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
                label_key: "plugin.cloud_sync.preview.raw_tcp_profiles_label",
                value: CloudSyncPreviewFactValue::Count(summary.raw_tcp_profiles),
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
                raw_tcp_profiles: preview
                    .raw_tcp_profiles_snapshot
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
            raw_tcp_profiles: preview.metadata.raw_tcp_profiles_count.unwrap_or(0),
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
            selected_connection_ids: Default::default(),
            import_quick_commands: false,
            selected_quick_command_ids: Default::default(),
            import_serial_profiles: false,
            selected_serial_profile_ids: Default::default(),
            import_raw_tcp_profiles: false,
            selected_raw_tcp_profile_ids: Default::default(),
            import_sensitive_credentials: false,
            import_app_settings: true,
            selected_app_settings_sections: ["general".to_string()].into_iter().collect(),
            import_plugin_settings: false,
            selected_plugin_ids: Default::default(),
            import_forwards: true,
            selected_forward_ids: Default::default(),
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

    #[test]
    fn apply_field_diff_items_show_changed_quick_command_fields() {
        let preview = CloudSyncPendingPreview::Structured(StructuredPreview {
            remote_metadata: Default::default(),
            manifest: oxideterm_cloud_sync::create_manifest_base(
                "rev-1",
                "2026-06-12T00:00:00Z",
                "device",
                SyncScope::default(),
            ),
            connections_snapshot: None,
            forwards_snapshot: None,
            quick_commands_snapshot_json: Some(
                serde_json::to_string(&QuickCommandsSnapshot {
                    version: 1,
                    categories: Vec::new(),
                    commands: vec![quick_command("cmd-1", "Deploy", "deploy --prod")],
                    updated_at: 2,
                })
                .expect("remote quick commands"),
            ),
            serial_profiles_snapshot: None,
            raw_tcp_profiles_snapshot: None,
            base_connections_snapshot: None,
            base_forwards_snapshot: None,
            base_quick_commands_snapshot_json: None,
            base_serial_profiles_snapshot: None,
            base_raw_tcp_profiles_snapshot: None,
            sensitive_credentials_entry: None,
            sensitive_credentials_preview: None,
            app_settings_entries: Default::default(),
            app_settings_sections: Default::default(),
            plugin_settings_entries: Default::default(),
            plugin_settings_counts: Default::default(),
        });
        let selection = CloudSyncPreviewSelection {
            import_connections: false,
            selected_connection_names: Default::default(),
            selected_connection_ids: Default::default(),
            import_quick_commands: true,
            selected_quick_command_ids: Default::default(),
            import_serial_profiles: false,
            selected_serial_profile_ids: Default::default(),
            import_raw_tcp_profiles: false,
            selected_raw_tcp_profile_ids: Default::default(),
            import_sensitive_credentials: false,
            import_app_settings: false,
            selected_app_settings_sections: Default::default(),
            import_plugin_settings: false,
            selected_plugin_ids: Default::default(),
            import_forwards: false,
            selected_forward_ids: Default::default(),
            conflict_strategy: ConflictStrategy::Merge,
        };
        let local = CloudSyncLocalFieldDiffSnapshot {
            quick_commands: Some(QuickCommandsSnapshot {
                version: 1,
                categories: Vec::new(),
                commands: vec![quick_command("cmd-1", "Deploy", "deploy --staging")],
                updated_at: 1,
            }),
            ..CloudSyncLocalFieldDiffSnapshot::default()
        };

        let items = cloud_sync_apply_field_diff_items(&preview, &selection, &local);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].status, CloudSyncFieldDiffStatus::Modified);
        assert!(items[0].fields.iter().any(|field| {
            field.label_key == "plugin.cloud_sync.diff_fields.command"
                && field.before.as_deref() == Some("deploy --staging")
                && field.after.as_deref() == Some("deploy --prod")
        }));
    }

    #[test]
    fn apply_field_diff_items_show_effective_field_merge_result() {
        let base_command = quick_command("cmd-1", "Deploy", "deploy --old");
        let mut local_command = base_command.clone();
        local_command.description = Some("local note".to_string());
        let mut remote_command = base_command.clone();
        remote_command.command = "deploy --prod".to_string();
        let preview = CloudSyncPendingPreview::Structured(StructuredPreview {
            remote_metadata: Default::default(),
            manifest: oxideterm_cloud_sync::create_manifest_base(
                "rev-1",
                "2026-06-12T00:00:00Z",
                "device",
                SyncScope::default(),
            ),
            connections_snapshot: None,
            forwards_snapshot: None,
            quick_commands_snapshot_json: Some(
                serde_json::to_string(&QuickCommandsSnapshot {
                    version: 1,
                    categories: Vec::new(),
                    commands: vec![remote_command],
                    updated_at: 2,
                })
                .expect("remote quick commands"),
            ),
            serial_profiles_snapshot: None,
            raw_tcp_profiles_snapshot: None,
            base_connections_snapshot: None,
            base_forwards_snapshot: None,
            base_quick_commands_snapshot_json: Some(
                serde_json::to_string(&QuickCommandsSnapshot {
                    version: 1,
                    categories: Vec::new(),
                    commands: vec![base_command],
                    updated_at: 1,
                })
                .expect("base quick commands"),
            ),
            base_serial_profiles_snapshot: None,
            base_raw_tcp_profiles_snapshot: None,
            sensitive_credentials_entry: None,
            sensitive_credentials_preview: None,
            app_settings_entries: Default::default(),
            app_settings_sections: Default::default(),
            plugin_settings_entries: Default::default(),
            plugin_settings_counts: Default::default(),
        });
        let selection = CloudSyncPreviewSelection {
            import_connections: false,
            selected_connection_names: Default::default(),
            selected_connection_ids: Default::default(),
            import_quick_commands: true,
            selected_quick_command_ids: Default::default(),
            import_serial_profiles: false,
            selected_serial_profile_ids: Default::default(),
            import_raw_tcp_profiles: false,
            selected_raw_tcp_profile_ids: Default::default(),
            import_sensitive_credentials: false,
            import_app_settings: false,
            selected_app_settings_sections: Default::default(),
            import_plugin_settings: false,
            selected_plugin_ids: Default::default(),
            import_forwards: false,
            selected_forward_ids: Default::default(),
            conflict_strategy: ConflictStrategy::Merge,
        };
        let local = CloudSyncLocalFieldDiffSnapshot {
            quick_commands: Some(QuickCommandsSnapshot {
                version: 1,
                categories: Vec::new(),
                commands: vec![local_command],
                updated_at: 3,
            }),
            ..CloudSyncLocalFieldDiffSnapshot::default()
        };

        let items = cloud_sync_apply_field_diff_items(&preview, &selection, &local);

        assert_eq!(items.len(), 1);
        assert!(items[0].fields.iter().any(|field| {
            field.label_key == "plugin.cloud_sync.diff_fields.command"
                && field.before.as_deref() == Some("deploy --old")
                && field.after.as_deref() == Some("deploy --prod")
                && field.merge_outcome == Some(CloudSyncFieldMergeOutcome::Remote)
        }));
        assert!(items[0].fields.iter().any(|field| {
            field.label_key == "plugin.cloud_sync.diff_fields.description"
                && field.before.as_deref() == Some("local note")
                && field.after.as_deref() == Some("local note")
                && field.merge_outcome == Some(CloudSyncFieldMergeOutcome::Local)
        }));
    }

    #[test]
    fn upload_field_diff_items_show_local_after_remote_before() {
        let preview = CloudSyncPendingPreview::Structured(StructuredPreview {
            remote_metadata: Default::default(),
            manifest: oxideterm_cloud_sync::create_manifest_base(
                "rev-1",
                "2026-06-12T00:00:00Z",
                "device",
                SyncScope::default(),
            ),
            connections_snapshot: None,
            forwards_snapshot: None,
            quick_commands_snapshot_json: Some(
                serde_json::to_string(&QuickCommandsSnapshot {
                    version: 1,
                    categories: Vec::new(),
                    commands: vec![quick_command("cmd-1", "Deploy", "deploy --prod")],
                    updated_at: 2,
                })
                .expect("remote quick commands"),
            ),
            serial_profiles_snapshot: None,
            raw_tcp_profiles_snapshot: None,
            base_connections_snapshot: None,
            base_forwards_snapshot: None,
            base_quick_commands_snapshot_json: None,
            base_serial_profiles_snapshot: None,
            base_raw_tcp_profiles_snapshot: None,
            sensitive_credentials_entry: None,
            sensitive_credentials_preview: None,
            app_settings_entries: Default::default(),
            app_settings_sections: Default::default(),
            plugin_settings_entries: Default::default(),
            plugin_settings_counts: Default::default(),
        });
        let local = CloudSyncLocalFieldDiffSnapshot {
            quick_commands: Some(QuickCommandsSnapshot {
                version: 1,
                categories: Vec::new(),
                commands: vec![quick_command("cmd-1", "Deploy", "deploy --staging")],
                updated_at: 3,
            }),
            ..CloudSyncLocalFieldDiffSnapshot::default()
        };

        let items = cloud_sync_upload_field_diff_items(&preview, &local, &SyncScope::default());

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].status, CloudSyncFieldDiffStatus::Modified);
        assert!(items[0].fields.iter().any(|field| {
            field.label_key == "plugin.cloud_sync.diff_fields.command"
                && field.before.as_deref() == Some("deploy --prod")
                && field.after.as_deref() == Some("deploy --staging")
        }));
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
            raw_tcp_profiles_record_count: 0,
            sensitive_credentials_record_count: 0,
        }
    }

    fn quick_command(id: &str, name: &str, command: &str) -> QuickCommand {
        QuickCommand {
            id: id.to_string(),
            name: name.to_string(),
            command: command.to_string(),
            category: "default".to_string(),
            description: None,
            host_pattern: None,
            created_at: 1,
            updated_at: 1,
        }
    }
}
