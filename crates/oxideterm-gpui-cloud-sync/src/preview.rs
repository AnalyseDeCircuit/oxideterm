// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync preview DTOs and summaries.

use std::collections::{BTreeMap, BTreeSet};

use oxideterm_cloud_sync::{
    PREVIEW_RECORD_LIMIT,
    operation::{LegacyPreview, StructuredPreview},
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
        summary,
        selection,
        can_apply,
        kind,
        show_local_changes_warning,
    }
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
}
