// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync preview selection rules.

use std::collections::{BTreeSet, HashSet};

use oxideterm_cloud_sync::{
    ConflictStrategy, StructuredApplySelection, StructuredDirtySections, StructuredManifest,
    StructuredSectionRevisions, operation::LegacyPreview, state::CloudSyncHistorySummary,
};
use oxideterm_connections::oxide_file::{ImportConflictStrategy, OxideImportOptions};

use crate::{
    CloudSyncApplySuccessCopySpec, CloudSyncPendingPreview, CloudSyncPreviewSource,
    CloudSyncPreviewSummary, cloud_sync_legacy_apply_success_copy_spec,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncPreviewSelectionAction {
    ToggleConnections,
    ToggleQuickCommands,
    ToggleSerialProfiles,
    ToggleSensitiveCredentials,
    ToggleAppSettings,
    ToggleAppSettingsSection(String),
    TogglePluginSettings,
    TogglePlugin(String),
    ToggleForwards,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloudSyncPreviewSelectionLabel {
    I18nCount {
        key: &'static str,
        count_name: &'static str,
        count: usize,
    },
    AppSettings,
    AppSettingsSection {
        section_id: String,
    },
    PluginId(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloudSyncPreviewSelectionRow {
    pub label: CloudSyncPreviewSelectionLabel,
    pub meta: Option<CloudSyncPreviewSelectionLabel>,
    pub checked: bool,
    pub disabled: bool,
    pub action: CloudSyncPreviewSelectionAction,
}

#[derive(Clone, Debug)]
pub struct CloudSyncLegacyImportOptions {
    pub oxide_options: OxideImportOptions,
    pub import_plugin_settings: bool,
    pub selected_plugin_ids: Option<HashSet<String>>,
    pub import_app_settings: bool,
    pub selected_app_settings_sections: Option<HashSet<String>>,
}

#[derive(Clone, Debug)]
pub struct CloudSyncLegacyApplyPlan {
    pub import_options: CloudSyncLegacyImportOptions,
    pub success_copy: CloudSyncApplySuccessCopySpec,
}

#[derive(Clone, Debug)]
pub struct CloudSyncPreviewSelection {
    pub import_connections: bool,
    pub selected_connection_names: BTreeSet<String>,
    pub import_quick_commands: bool,
    pub import_serial_profiles: bool,
    pub import_sensitive_credentials: bool,
    pub import_app_settings: bool,
    pub selected_app_settings_sections: BTreeSet<String>,
    pub import_plugin_settings: bool,
    pub selected_plugin_ids: BTreeSet<String>,
    pub import_forwards: bool,
    pub conflict_strategy: ConflictStrategy,
}

/// Plans a legacy preview apply without touching app-owned stores or GPUI state.
pub fn cloud_sync_legacy_apply_plan(
    preview: &LegacyPreview,
    source: &CloudSyncPreviewSource,
    selection: &CloudSyncPreviewSelection,
) -> CloudSyncLegacyApplyPlan {
    let summary = crate::cloud_sync_preview_summary(&CloudSyncPendingPreview::Legacy {
        preview: preview.clone(),
        source: source.clone(),
    });
    CloudSyncLegacyApplyPlan {
        import_options: cloud_sync_legacy_import_options(&summary, selection),
        success_copy: cloud_sync_legacy_apply_success_copy_spec(source),
    }
}

/// Converts Cloud Sync's conflict setting into the legacy `.oxide` importer mode.
pub fn import_strategy_from_cloud_settings(strategy: ConflictStrategy) -> ImportConflictStrategy {
    match strategy {
        ConflictStrategy::Merge => ImportConflictStrategy::Merge,
        ConflictStrategy::Replace => ImportConflictStrategy::Replace,
        ConflictStrategy::Skip => ImportConflictStrategy::Skip,
        ConflictStrategy::Rename => ImportConflictStrategy::Rename,
    }
}

/// Builds the non-UI import plan for applying a legacy Cloud Sync preview.
pub fn cloud_sync_legacy_import_options(
    summary: &CloudSyncPreviewSummary,
    selection: &CloudSyncPreviewSelection,
) -> CloudSyncLegacyImportOptions {
    let import_portable_secrets = selection.effective_import_connections(summary);
    CloudSyncLegacyImportOptions {
        oxide_options: OxideImportOptions {
            selected_names: selection.selected_connection_names_for_import(summary),
            selected_forward_ids: None,
            conflict_strategy: import_strategy_from_cloud_settings(
                selection.conflict_strategy.clone(),
            ),
            import_forwards: selection.import_forwards,
            import_portable_secrets,
            ..OxideImportOptions::default()
        },
        import_plugin_settings: selection.effective_import_plugin_settings(),
        selected_plugin_ids: selection.selected_plugin_hash_set(),
        import_app_settings: selection.effective_import_app_settings(summary),
        selected_app_settings_sections: selection.selected_app_settings_hash_set(summary),
    }
}

impl CloudSyncPreviewSelection {
    pub fn from_preview(
        preview: &CloudSyncPendingPreview,
        default_conflict_strategy: ConflictStrategy,
    ) -> Self {
        let summary = crate::cloud_sync_preview_summary(preview);
        let conflict_strategy = match preview {
            CloudSyncPendingPreview::Legacy {
                source: CloudSyncPreviewSource::Backup { .. },
                ..
            } => ConflictStrategy::Replace,
            _ => default_conflict_strategy,
        };
        Self {
            import_connections: summary.connections > 0,
            selected_connection_names: summary.connection_record_names(),
            import_quick_commands: summary.quick_commands > 0,
            import_serial_profiles: summary.serial_profiles > 0,
            import_sensitive_credentials: summary.sensitive_credentials > 0,
            import_app_settings: summary.has_app_settings,
            selected_app_settings_sections: summary
                .app_settings_sections
                .iter()
                .map(|section| section.id.clone())
                .collect(),
            import_plugin_settings: summary.plugin_settings_count > 0,
            selected_plugin_ids: summary.plugin_settings_by_plugin.keys().cloned().collect(),
            import_forwards: summary.forwards > 0,
            conflict_strategy,
        }
    }

    pub fn effective_import_connections(&self, summary: &CloudSyncPreviewSummary) -> bool {
        if !self.import_connections {
            return false;
        }
        let record_names = summary.connection_record_names();
        record_names.is_empty()
            || record_names
                .iter()
                .any(|name| self.selected_connection_names.contains(name))
    }

    pub fn selected_connection_names_for_import(
        &self,
        summary: &CloudSyncPreviewSummary,
    ) -> Option<Vec<String>> {
        if !self.import_connections {
            return Some(Vec::new());
        }
        let record_names = summary.connection_record_names();
        if record_names.is_empty() {
            return None;
        }
        Some(
            record_names
                .into_iter()
                .filter(|name| self.selected_connection_names.contains(name))
                .collect(),
        )
    }

    pub fn effective_import_app_settings(&self, summary: &CloudSyncPreviewSummary) -> bool {
        self.import_app_settings
            && (!self.selected_app_settings_sections.is_empty()
                || summary.app_settings_sections.is_empty())
    }

    pub fn effective_import_plugin_settings(&self) -> bool {
        self.import_plugin_settings && !self.selected_plugin_ids.is_empty()
    }

    pub fn can_apply(&self, summary: &CloudSyncPreviewSummary) -> bool {
        self.effective_import_connections(summary)
            || self.import_forwards
            || self.import_quick_commands
            || self.import_serial_profiles
            || self.import_sensitive_credentials
            || self.effective_import_app_settings(summary)
            || self.effective_import_plugin_settings()
    }

    pub fn structured_selection(&self) -> StructuredApplySelection {
        StructuredApplySelection {
            connections: self.import_connections,
            forwards: self.import_forwards,
            quick_commands: self.import_quick_commands,
            serial_profiles: self.import_serial_profiles,
            sensitive_credentials: self.import_sensitive_credentials,
            app_settings_sections: if self.import_app_settings {
                self.selected_app_settings_sections
                    .iter()
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            },
            plugin_ids: if self.import_plugin_settings {
                self.selected_plugin_ids.iter().cloned().collect()
            } else {
                Vec::new()
            },
        }
    }

    pub fn selected_app_settings_hash_set(
        &self,
        summary: &CloudSyncPreviewSummary,
    ) -> Option<HashSet<String>> {
        if !self.effective_import_app_settings(summary) {
            return Some(HashSet::new());
        }
        if self.selected_app_settings_sections.is_empty() {
            None
        } else {
            Some(
                self.selected_app_settings_sections
                    .iter()
                    .cloned()
                    .collect(),
            )
        }
    }

    pub fn selected_plugin_hash_set(&self) -> Option<HashSet<String>> {
        if !self.import_plugin_settings {
            return Some(HashSet::new());
        }
        if self.selected_plugin_ids.is_empty() {
            Some(HashSet::new())
        } else {
            Some(self.selected_plugin_ids.iter().cloned().collect())
        }
    }

    pub fn preview_rows(
        &self,
        summary: &CloudSyncPreviewSummary,
    ) -> Vec<CloudSyncPreviewSelectionRow> {
        let mut rows = Vec::new();
        if summary.connections > 0 {
            rows.push(CloudSyncPreviewSelectionRow {
                label: CloudSyncPreviewSelectionLabel::I18nCount {
                    key: "plugin.cloud_sync.preview.toggle_connections",
                    count_name: "count",
                    count: summary.connections,
                },
                meta: None,
                checked: self.import_connections,
                disabled: false,
                action: CloudSyncPreviewSelectionAction::ToggleConnections,
            });
        }
        if summary.quick_commands > 0 {
            rows.push(CloudSyncPreviewSelectionRow {
                label: CloudSyncPreviewSelectionLabel::I18nCount {
                    key: "plugin.cloud_sync.preview.toggle_quick_commands",
                    count_name: "count",
                    count: summary.quick_commands,
                },
                meta: None,
                checked: self.import_quick_commands,
                disabled: false,
                action: CloudSyncPreviewSelectionAction::ToggleQuickCommands,
            });
        }
        if summary.serial_profiles > 0 {
            rows.push(CloudSyncPreviewSelectionRow {
                label: CloudSyncPreviewSelectionLabel::I18nCount {
                    key: "plugin.cloud_sync.preview.toggle_serial_profiles",
                    count_name: "count",
                    count: summary.serial_profiles,
                },
                meta: None,
                checked: self.import_serial_profiles,
                disabled: false,
                action: CloudSyncPreviewSelectionAction::ToggleSerialProfiles,
            });
        }
        if summary.sensitive_credentials > 0 {
            rows.push(CloudSyncPreviewSelectionRow {
                label: CloudSyncPreviewSelectionLabel::I18nCount {
                    key: "plugin.cloud_sync.preview.toggle_sensitive_credentials",
                    count_name: "count",
                    count: summary.sensitive_credentials,
                },
                meta: None,
                checked: self.import_sensitive_credentials,
                disabled: false,
                action: CloudSyncPreviewSelectionAction::ToggleSensitiveCredentials,
            });
        }
        if summary.has_app_settings {
            rows.push(CloudSyncPreviewSelectionRow {
                label: CloudSyncPreviewSelectionLabel::AppSettings,
                meta: None,
                checked: self.import_app_settings,
                disabled: false,
                action: CloudSyncPreviewSelectionAction::ToggleAppSettings,
            });
            rows.extend(summary.app_settings_sections.iter().map(|section| {
                CloudSyncPreviewSelectionRow {
                    label: CloudSyncPreviewSelectionLabel::AppSettingsSection {
                        section_id: section.id.clone(),
                    },
                    meta: Some(CloudSyncPreviewSelectionLabel::I18nCount {
                        key: "plugin.cloud_sync.preview.section_field_count",
                        count_name: "count",
                        count: section.field_count,
                    }),
                    checked: self.import_app_settings
                        && self.selected_app_settings_sections.contains(&section.id),
                    disabled: !self.import_app_settings,
                    action: CloudSyncPreviewSelectionAction::ToggleAppSettingsSection(
                        section.id.clone(),
                    ),
                }
            }));
        }
        if summary.plugin_settings_count > 0 {
            rows.push(CloudSyncPreviewSelectionRow {
                label: CloudSyncPreviewSelectionLabel::I18nCount {
                    key: "plugin.cloud_sync.preview.toggle_plugin_settings",
                    count_name: "count",
                    count: summary.plugin_settings_count,
                },
                meta: None,
                checked: self.import_plugin_settings,
                disabled: false,
                action: CloudSyncPreviewSelectionAction::TogglePluginSettings,
            });
            rows.extend(
                summary
                    .plugin_settings_by_plugin
                    .iter()
                    .map(|(plugin_id, count)| CloudSyncPreviewSelectionRow {
                        label: CloudSyncPreviewSelectionLabel::PluginId(plugin_id.clone()),
                        meta: Some(CloudSyncPreviewSelectionLabel::I18nCount {
                            key: "plugin.cloud_sync.preview.plugin_settings",
                            count_name: "count",
                            count: *count,
                        }),
                        checked: self.import_plugin_settings
                            && self.selected_plugin_ids.contains(plugin_id),
                        disabled: !self.import_plugin_settings,
                        action: CloudSyncPreviewSelectionAction::TogglePlugin(plugin_id.clone()),
                    }),
            );
        }
        if summary.forwards > 0 {
            rows.push(CloudSyncPreviewSelectionRow {
                label: CloudSyncPreviewSelectionLabel::I18nCount {
                    key: "plugin.cloud_sync.preview.toggle_forwards",
                    count_name: "count",
                    count: summary.forwards,
                },
                meta: None,
                checked: self.import_forwards,
                disabled: false,
                action: CloudSyncPreviewSelectionAction::ToggleForwards,
            });
        }
        rows
    }

    pub fn apply_action(
        &mut self,
        action: CloudSyncPreviewSelectionAction,
        all_connection_names: BTreeSet<String>,
    ) {
        match action {
            CloudSyncPreviewSelectionAction::ToggleConnections => {
                self.import_connections = !self.import_connections;
                if self.import_connections && self.selected_connection_names.is_empty() {
                    self.selected_connection_names = all_connection_names;
                }
            }
            CloudSyncPreviewSelectionAction::ToggleQuickCommands => {
                self.import_quick_commands = !self.import_quick_commands;
            }
            CloudSyncPreviewSelectionAction::ToggleSerialProfiles => {
                self.import_serial_profiles = !self.import_serial_profiles;
            }
            CloudSyncPreviewSelectionAction::ToggleSensitiveCredentials => {
                self.import_sensitive_credentials = !self.import_sensitive_credentials;
            }
            CloudSyncPreviewSelectionAction::ToggleAppSettings => {
                self.import_app_settings = !self.import_app_settings;
            }
            CloudSyncPreviewSelectionAction::ToggleAppSettingsSection(section_id) => {
                if !self.selected_app_settings_sections.remove(&section_id) {
                    self.selected_app_settings_sections.insert(section_id);
                }
            }
            CloudSyncPreviewSelectionAction::TogglePluginSettings => {
                self.import_plugin_settings = !self.import_plugin_settings;
            }
            CloudSyncPreviewSelectionAction::TogglePlugin(plugin_id) => {
                if !self.selected_plugin_ids.remove(&plugin_id) {
                    self.selected_plugin_ids.insert(plugin_id);
                }
            }
            CloudSyncPreviewSelectionAction::ToggleForwards => {
                self.import_forwards = !self.import_forwards;
            }
        }
    }
}

pub fn structured_apply_covers_full_remote(
    manifest: &StructuredManifest,
    selection: &StructuredApplySelection,
) -> bool {
    (manifest.sections.connections.is_none() || selection.connections)
        && (manifest.sections.forwards.is_none() || selection.forwards)
        && (manifest.sections.quick_commands.is_none() || selection.quick_commands)
        && (manifest.sections.serial_profiles.is_none() || selection.serial_profiles)
        && (manifest.sections.sensitive_credentials.is_none() || selection.sensitive_credentials)
        && manifest
            .sections
            .app_settings
            .keys()
            .all(|section_id| selection.app_settings_sections.contains(section_id))
        && manifest
            .sections
            .plugin_settings
            .keys()
            .filter(|plugin_id| plugin_id.as_str() != oxideterm_cloud_sync::CLOUD_SYNC_PLUGIN_ID)
            .all(|plugin_id| selection.plugin_ids.contains(plugin_id))
}

pub fn merge_structured_remote_baseline(
    previous: Option<&StructuredSectionRevisions>,
    next: &StructuredSectionRevisions,
    selection: &StructuredApplySelection,
) -> StructuredSectionRevisions {
    let mut merged = previous.cloned().unwrap_or_default();
    if selection.connections {
        merged.connections = next.connections.clone();
    }
    if selection.forwards {
        merged.forwards = next.forwards.clone();
    }
    if selection.quick_commands {
        merged.quick_commands = next.quick_commands.clone();
    }
    if selection.serial_profiles {
        merged.serial_profiles = next.serial_profiles.clone();
    }
    if selection.sensitive_credentials {
        merged.sensitive_credentials = next.sensitive_credentials.clone();
    }
    for section_id in &selection.app_settings_sections {
        if let Some(revision) = next.app_settings.get(section_id) {
            merged
                .app_settings
                .insert(section_id.clone(), revision.clone());
        }
    }
    for plugin_id in &selection.plugin_ids {
        if let Some(revision) = next.plugin_settings.get(plugin_id) {
            merged
                .plugin_settings
                .insert(plugin_id.clone(), revision.clone());
        }
    }
    merged
}

pub fn legacy_apply_covers_full_remote(
    summary: &CloudSyncPreviewSummary,
    selection: &CloudSyncPreviewSelection,
) -> bool {
    let remote_connection_names = summary.connection_record_names();
    let remote_app_section_ids = summary
        .app_settings_sections
        .iter()
        .map(|section| section.id.as_str())
        .collect::<Vec<_>>();
    let remote_plugin_ids = summary
        .plugin_settings_by_plugin
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();

    (summary.connections == 0
        || (selection.import_connections
            && (remote_connection_names.is_empty()
                || remote_connection_names
                    .iter()
                    .all(|name| selection.selected_connection_names.contains(name)))))
        && (summary.forwards == 0 || selection.import_forwards)
        && (summary.quick_commands == 0 || selection.import_quick_commands)
        && (summary.serial_profiles == 0 || selection.import_serial_profiles)
        && (summary.sensitive_credentials == 0 || selection.import_sensitive_credentials)
        && (!summary.has_app_settings
            || (selection.effective_import_app_settings(summary)
                && remote_app_section_ids
                    .iter()
                    .all(|id| selection.selected_app_settings_sections.contains(*id))))
        && (remote_plugin_ids.is_empty()
            || (selection.effective_import_plugin_settings()
                && remote_plugin_ids
                    .iter()
                    .all(|id| selection.selected_plugin_ids.contains(*id))))
}

pub fn cloud_sync_apply_total_units(
    preview: &CloudSyncPendingPreview,
    selection: &CloudSyncPreviewSelection,
    create_rollback_backup: bool,
) -> f64 {
    let rollback_units = usize::from(create_rollback_backup);
    let import_units = match preview {
        CloudSyncPendingPreview::Structured(preview) => {
            let structured_selection = selection.structured_selection();
            usize::from(structured_selection.connections && preview.connections_snapshot.is_some())
                + usize::from(structured_selection.forwards && preview.forwards_snapshot.is_some())
                + usize::from(
                    structured_selection.quick_commands
                        && preview.quick_commands_snapshot_json.is_some(),
                )
                + usize::from(
                    structured_selection.serial_profiles
                        && preview.serial_profiles_snapshot.is_some(),
                )
                + usize::from(
                    structured_selection.sensitive_credentials
                        && preview.sensitive_credentials_entry.is_some(),
                )
                + structured_selection
                    .app_settings_sections
                    .iter()
                    .filter(|section_id| preview.app_settings_entries.contains_key(*section_id))
                    .count()
                + structured_selection
                    .plugin_ids
                    .iter()
                    .filter(|plugin_id| preview.plugin_settings_entries.contains_key(*plugin_id))
                    .count()
        }
        CloudSyncPendingPreview::Legacy { .. } => 1,
    };
    (rollback_units + import_units).max(1) as f64
}

pub fn history_summary_from_manifest(manifest: &StructuredManifest) -> CloudSyncHistorySummary {
    CloudSyncHistorySummary {
        connections: manifest
            .sections
            .connections
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        forwards: manifest
            .sections
            .forwards
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        quick_commands: manifest
            .sections
            .quick_commands
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        serial_profiles: manifest
            .sections
            .serial_profiles
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        sensitive_credentials: manifest
            .sections
            .sensitive_credentials
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        has_app_settings: !manifest.sections.app_settings.is_empty(),
        plugin_settings_count: manifest.sections.plugin_settings.len(),
    }
}

pub fn history_summary_from_legacy_preview(preview: &LegacyPreview) -> CloudSyncHistorySummary {
    CloudSyncHistorySummary {
        connections: preview.metadata.num_connections,
        forwards: preview.preview.total_forwards,
        quick_commands: preview.metadata.quick_commands_count.unwrap_or(0),
        serial_profiles: 0,
        sensitive_credentials: preview.metadata.portable_secret_count.unwrap_or(0),
        has_app_settings: preview.preview.has_app_settings,
        plugin_settings_count: preview.preview.plugin_settings_count,
    }
}

pub fn has_cloud_sync_structured_conflict(
    dirty: &StructuredDirtySections,
    remote: Option<&StructuredSectionRevisions>,
    previous: Option<&StructuredSectionRevisions>,
) -> bool {
    let Some(previous) = previous else {
        return dirty.connections
            || dirty.forwards
            || dirty.quick_commands
            || dirty.serial_profiles
            || dirty.sensitive_credentials
            || dirty.app_settings.values().any(|value| *value)
            || dirty.plugin_settings.values().any(|value| *value);
    };
    let remote = remote.cloned().unwrap_or_default();
    if dirty.connections && remote.connections != previous.connections {
        return true;
    }
    if dirty.forwards && remote.forwards != previous.forwards {
        return true;
    }
    if dirty.quick_commands && remote.quick_commands != previous.quick_commands {
        return true;
    }
    if dirty.serial_profiles && remote.serial_profiles != previous.serial_profiles {
        return true;
    }
    if dirty.sensitive_credentials && remote.sensitive_credentials != previous.sensitive_credentials
    {
        return true;
    }
    dirty.app_settings.iter().any(|(section_id, value)| {
        *value && remote.app_settings.get(section_id) != previous.app_settings.get(section_id)
    }) || dirty.plugin_settings.iter().any(|(plugin_id, value)| {
        *value && remote.plugin_settings.get(plugin_id) != previous.plugin_settings.get(plugin_id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CloudSyncPreviewRecord;

    fn connection_record(name: &str) -> CloudSyncPreviewRecord {
        CloudSyncPreviewRecord {
            resource: "connection".to_string(),
            name: name.to_string(),
            action: "import".to_string(),
            reason_code: "new".to_string(),
            target_name: None,
        }
    }

    fn summary_with_connections(names: &[&str]) -> CloudSyncPreviewSummary {
        CloudSyncPreviewSummary {
            connections: names.len(),
            records: names.iter().map(|name| connection_record(name)).collect(),
            ..CloudSyncPreviewSummary::default()
        }
    }

    #[test]
    fn legacy_preview_selection_exports_selected_connection_names() {
        let summary = summary_with_connections(&["Prod", "Staging"]);
        let mut selection = CloudSyncPreviewSelection {
            import_connections: true,
            selected_connection_names: BTreeSet::from(["Prod".to_string()]),
            import_quick_commands: false,
            import_serial_profiles: false,
            import_sensitive_credentials: false,
            import_app_settings: false,
            selected_app_settings_sections: BTreeSet::new(),
            import_plugin_settings: false,
            selected_plugin_ids: BTreeSet::new(),
            import_forwards: false,
            conflict_strategy: ConflictStrategy::Rename,
        };

        assert_eq!(
            selection.selected_connection_names_for_import(&summary),
            Some(vec!["Prod".to_string()])
        );
        assert!(selection.can_apply(&summary));
        assert!(!legacy_apply_covers_full_remote(&summary, &selection));

        selection
            .selected_connection_names
            .insert("Staging".to_string());
        assert!(legacy_apply_covers_full_remote(&summary, &selection));
    }

    #[test]
    fn legacy_preview_selection_disables_connection_import_when_none_checked() {
        let summary = summary_with_connections(&["Prod"]);
        let selection = CloudSyncPreviewSelection {
            import_connections: true,
            selected_connection_names: BTreeSet::new(),
            import_quick_commands: false,
            import_serial_profiles: false,
            import_sensitive_credentials: false,
            import_app_settings: false,
            selected_app_settings_sections: BTreeSet::new(),
            import_plugin_settings: false,
            selected_plugin_ids: BTreeSet::new(),
            import_forwards: false,
            conflict_strategy: ConflictStrategy::Rename,
        };

        assert_eq!(
            selection.selected_connection_names_for_import(&summary),
            Some(Vec::new())
        );
        assert!(!selection.can_apply(&summary));
        assert!(!legacy_apply_covers_full_remote(&summary, &selection));
    }
}
