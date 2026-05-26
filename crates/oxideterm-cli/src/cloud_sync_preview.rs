// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeSet, fs, io::ErrorKind, path::Path};

use oxideterm_cloud_sync::{
    LocalSyncMetadata, StructuredDirtySections, StructuredLocalState, StructuredSectionRevisions,
    SyncScope, compute_structured_dirty_sections, state::CloudSyncPersistedState,
};
use serde::Serialize;

use crate::{
    args::{CloudSyncDiffArgs, CloudSyncDiffCategory, CloudSyncDiffFormat, JsonArgs},
    error::{CliError, CliResult},
    output::{self, OutputFormat},
    paths::default_cloud_sync_path,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CloudSyncPreviewResponse {
    pub(crate) path: String,
    pub(crate) scope: SyncScope,
    pub(crate) local: RevisionSummary,
    pub(crate) baseline: RevisionSummary,
    pub(crate) remote: RemoteSummary,
    pub(crate) dirty: DirtySummary,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CloudSyncDiffResponse {
    pub(crate) path: String,
    pub(crate) has_dirty: bool,
    pub(crate) remote_revision: Option<String>,
    pub(crate) sections: Vec<SectionDiff>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RevisionSummary {
    connections: Option<String>,
    forwards: Option<String>,
    app_settings_count: usize,
    plugin_settings_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteSummary {
    exists: bool,
    revision: Option<String>,
    updated_at: Option<String>,
    device_id: Option<String>,
    format: Option<String>,
    section_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirtySummary {
    has_dirty: bool,
    connections: bool,
    forwards: bool,
    app_settings_count: usize,
    plugin_settings_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SectionDiff {
    pub(crate) category: &'static str,
    pub(crate) id: String,
    pub(crate) in_scope: bool,
    pub(crate) dirty: bool,
    pub(crate) local_revision: Option<String>,
    pub(crate) baseline_revision: Option<String>,
    pub(crate) remote_revision: Option<String>,
}

pub fn preview(args: JsonArgs) -> CliResult<()> {
    let path = default_cloud_sync_path();
    let state = load_persisted_state(&path, args.json)?;
    let response = preview_response(path.display().to_string(), &state);

    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format_preview_text(&response));
            Ok(())
        }
    }
}

pub fn diff(args: CloudSyncDiffArgs) -> CliResult<()> {
    let path = default_cloud_sync_path();
    let state = load_persisted_state(&path, args.json)?;
    let filter = DiffFilter::from_args(&args);
    let response = diff_response(path.display().to_string(), &state, &filter);

    let json_output = args.json || args.format == Some(CloudSyncDiffFormat::Json);
    match output::format_from_flag(json_output) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            let text = if args.format == Some(CloudSyncDiffFormat::Table) {
                format_diff_table(&response)
            } else {
                format_diff_text(&response)
            };
            output::write_text(text);
            Ok(())
        }
    }
}

pub(crate) fn load_persisted_state(path: &Path, json: bool) -> CliResult<CloudSyncPersistedState> {
    // CLI inspection preserves persisted runtime flags instead of boot-resetting them.
    match fs::read_to_string(path) {
        Ok(contents) if contents.trim().is_empty() => Ok(CloudSyncPersistedState::default()),
        Ok(contents) => serde_json::from_str(&contents).map_err(|error| {
            CliError::new(
                "cloud_sync_parse_failed",
                format!(
                    "failed to parse cloud sync state {}: {error}",
                    path.display()
                ),
                json,
            )
        }),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(CloudSyncPersistedState::default()),
        Err(error) => Err(CliError::new(
            "cloud_sync_read_failed",
            format!(
                "failed to read cloud sync state {}: {error}",
                path.display()
            ),
            json,
        )),
    }
}

pub(crate) fn preview_response(
    path: String,
    state: &CloudSyncPersistedState,
) -> CloudSyncPreviewResponse {
    let local_metadata = local_metadata(state);
    let scope = resolved_scope(state, &local_metadata);
    let baseline = state.last_synced_structured_state.as_ref();
    let dirty = compute_structured_dirty_sections(&local_metadata, baseline, &scope);
    let dirty_sections = effective_dirty_sections(state, &dirty.dirty_sections);
    let has_dirty = state.local_dirty || dirty.has_dirty || has_any_dirty(dirty_sections);
    let default_baseline = StructuredLocalState::default();
    let baseline_state = baseline.unwrap_or(&default_baseline);

    CloudSyncPreviewResponse {
        path,
        scope,
        local: summarize_local_state(&dirty.current_state),
        baseline: summarize_local_state(baseline_state),
        remote: remote_summary(state),
        dirty: dirty_summary(dirty_sections, has_dirty),
    }
}

pub(crate) struct DiffFilter {
    dirty_only: bool,
    category: Option<CloudSyncDiffCategory>,
}

impl DiffFilter {
    pub(crate) fn from_args(args: &CloudSyncDiffArgs) -> Self {
        Self {
            dirty_only: args.dirty_only,
            category: args.category,
        }
    }
}

pub(crate) fn diff_response(
    path: String,
    state: &CloudSyncPersistedState,
    filter: &DiffFilter,
) -> CloudSyncDiffResponse {
    let local_metadata = local_metadata(state);
    let scope = resolved_scope(state, &local_metadata);
    let baseline = state.last_synced_structured_state.as_ref();
    let dirty = compute_structured_dirty_sections(&local_metadata, baseline, &scope);
    let remote = remote_sections(state);
    let dirty_sections = effective_dirty_sections(state, &dirty.dirty_sections);
    let sections = filter_sections(
        section_diffs(
            &scope,
            &dirty.current_state,
            baseline,
            dirty_sections,
            remote,
        ),
        filter,
    );

    CloudSyncDiffResponse {
        path,
        has_dirty: state.local_dirty || dirty.has_dirty || has_any_dirty(dirty_sections),
        remote_revision: state.last_known_remote_revision.clone(),
        sections,
    }
}

fn filter_sections(sections: Vec<SectionDiff>, filter: &DiffFilter) -> Vec<SectionDiff> {
    sections
        .into_iter()
        .filter(|section| !filter.dirty_only || section.dirty)
        .filter(|section| {
            filter
                .category
                .is_none_or(|category| section_matches_category(section, category))
        })
        .collect()
}

fn section_matches_category(section: &SectionDiff, category: CloudSyncDiffCategory) -> bool {
    match category {
        CloudSyncDiffCategory::Connections => section.category == "connections",
        CloudSyncDiffCategory::Forwards => section.category == "forwards",
        CloudSyncDiffCategory::AppSettings => section.category == "appSettings",
        CloudSyncDiffCategory::PluginSettings => section.category == "pluginSettings",
    }
}

fn local_metadata(state: &CloudSyncPersistedState) -> LocalSyncMetadata {
    state.last_synced_local_metadata.clone().unwrap_or_default()
}

fn resolved_scope(
    state: &CloudSyncPersistedState,
    local_metadata: &LocalSyncMetadata,
) -> SyncScope {
    let available_plugin_ids = local_metadata
        .plugin_settings_revisions
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    state.sync_scope(&available_plugin_ids)
}

fn remote_sections(state: &CloudSyncPersistedState) -> Option<&StructuredSectionRevisions> {
    // Prefer the latest remote check, then fall back to the last synchronized remote baseline.
    state
        .remote_section_revisions
        .as_ref()
        .or(state.last_synced_remote_sections.as_ref())
}

fn remote_summary(state: &CloudSyncPersistedState) -> RemoteSummary {
    let sections = remote_sections(state);
    RemoteSummary {
        exists: state.remote_exists,
        revision: state.last_known_remote_revision.clone(),
        updated_at: state.remote_updated_at.clone(),
        device_id: state.remote_device_id.clone(),
        format: state.remote_format.clone(),
        section_count: sections.map(section_count).unwrap_or_default(),
    }
}

fn section_count(sections: &StructuredSectionRevisions) -> usize {
    sections.connections.iter().count()
        + sections.forwards.iter().count()
        + sections.app_settings.len()
        + sections.plugin_settings.len()
}

fn summarize_local_state(state: &StructuredLocalState) -> RevisionSummary {
    RevisionSummary {
        connections: state.connections.clone(),
        forwards: state.forwards.clone(),
        app_settings_count: state.app_settings.len(),
        plugin_settings_count: state.plugin_settings.len(),
    }
}

fn dirty_summary(sections: &StructuredDirtySections, has_dirty: bool) -> DirtySummary {
    DirtySummary {
        has_dirty,
        connections: sections.connections,
        forwards: sections.forwards,
        app_settings_count: sections
            .app_settings
            .values()
            .filter(|dirty| **dirty)
            .count(),
        plugin_settings_count: sections
            .plugin_settings
            .values()
            .filter(|dirty| **dirty)
            .count(),
    }
}

fn effective_dirty_sections<'a>(
    state: &'a CloudSyncPersistedState,
    computed: &'a StructuredDirtySections,
) -> &'a StructuredDirtySections {
    state.local_dirty_sections.as_ref().unwrap_or(computed)
}

fn has_any_dirty(sections: &StructuredDirtySections) -> bool {
    sections.connections
        || sections.forwards
        || sections.app_settings.values().any(|dirty| *dirty)
        || sections.plugin_settings.values().any(|dirty| *dirty)
}

fn section_diffs(
    scope: &SyncScope,
    current: &StructuredLocalState,
    baseline: Option<&StructuredLocalState>,
    dirty: &StructuredDirtySections,
    remote: Option<&StructuredSectionRevisions>,
) -> Vec<SectionDiff> {
    let mut sections = vec![
        SectionDiff {
            category: "connections",
            id: "connections".to_string(),
            in_scope: scope.sync_connections,
            dirty: dirty.connections,
            local_revision: current.connections.clone(),
            baseline_revision: baseline.and_then(|state| state.connections.clone()),
            remote_revision: remote.and_then(|sections| sections.connections.clone()),
        },
        SectionDiff {
            category: "forwards",
            id: "forwards".to_string(),
            in_scope: scope.sync_forwards,
            dirty: dirty.forwards,
            local_revision: current.forwards.clone(),
            baseline_revision: baseline.and_then(|state| state.forwards.clone()),
            remote_revision: remote.and_then(|sections| sections.forwards.clone()),
        },
    ];

    for section_id in app_setting_ids(scope, current, baseline, remote) {
        sections.push(SectionDiff {
            category: "appSettings",
            id: section_id.clone(),
            in_scope: scope.sync_app_settings && scope.app_settings_sections.contains(&section_id),
            dirty: dirty
                .app_settings
                .get(&section_id)
                .copied()
                .unwrap_or(false),
            local_revision: optional_map_revision(&current.app_settings, &section_id),
            baseline_revision: baseline
                .and_then(|state| optional_map_revision(&state.app_settings, &section_id)),
            remote_revision: remote
                .and_then(|sections| sections.app_settings.get(&section_id))
                .cloned(),
        });
    }

    for plugin_id in plugin_setting_ids(scope, current, baseline, remote) {
        sections.push(SectionDiff {
            category: "pluginSettings",
            id: plugin_id.clone(),
            in_scope: plugin_in_scope(scope, &plugin_id),
            dirty: dirty
                .plugin_settings
                .get(&plugin_id)
                .copied()
                .unwrap_or(false),
            local_revision: optional_map_revision(&current.plugin_settings, &plugin_id),
            baseline_revision: baseline
                .and_then(|state| optional_map_revision(&state.plugin_settings, &plugin_id)),
            remote_revision: remote
                .and_then(|sections| sections.plugin_settings.get(&plugin_id))
                .cloned(),
        });
    }

    sections
}

fn app_setting_ids(
    scope: &SyncScope,
    current: &StructuredLocalState,
    baseline: Option<&StructuredLocalState>,
    remote: Option<&StructuredSectionRevisions>,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    ids.extend(scope.app_settings_sections.iter().cloned());
    ids.extend(current.app_settings.keys().cloned());
    if let Some(baseline) = baseline {
        ids.extend(baseline.app_settings.keys().cloned());
    }
    if let Some(remote) = remote {
        ids.extend(remote.app_settings.keys().cloned());
    }
    ids.into_iter().collect()
}

fn plugin_setting_ids(
    scope: &SyncScope,
    current: &StructuredLocalState,
    baseline: Option<&StructuredLocalState>,
    remote: Option<&StructuredSectionRevisions>,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    if let Some(plugin_ids) = scope.plugin_ids.as_ref() {
        ids.extend(plugin_ids.iter().cloned());
    }
    ids.extend(current.plugin_settings.keys().cloned());
    if let Some(baseline) = baseline {
        ids.extend(baseline.plugin_settings.keys().cloned());
    }
    if let Some(remote) = remote {
        ids.extend(remote.plugin_settings.keys().cloned());
    }
    ids.into_iter().collect()
}

fn plugin_in_scope(scope: &SyncScope, plugin_id: &str) -> bool {
    scope.sync_plugin_settings
        && scope
            .plugin_ids
            .as_ref()
            .is_none_or(|plugin_ids| plugin_ids.iter().any(|candidate| candidate == plugin_id))
}

fn optional_map_revision(
    map: &std::collections::BTreeMap<String, Option<String>>,
    key: &str,
) -> Option<String> {
    map.get(key).cloned().flatten()
}

fn format_preview_text(response: &CloudSyncPreviewResponse) -> String {
    format!(
        "path: {}\nremote: exists={} revision={} sections={}\nlocal: connections={} forwards={} appSettings={} pluginSettings={}\ndirty: hasDirty={} connections={} forwards={} appSettings={} pluginSettings={}",
        response.path,
        response.remote.exists,
        response.remote.revision.as_deref().unwrap_or("-"),
        response.remote.section_count,
        response.local.connections.as_deref().unwrap_or("-"),
        response.local.forwards.as_deref().unwrap_or("-"),
        response.local.app_settings_count,
        response.local.plugin_settings_count,
        response.dirty.has_dirty,
        response.dirty.connections,
        response.dirty.forwards,
        response.dirty.app_settings_count,
        response.dirty.plugin_settings_count
    )
}

fn format_diff_text(response: &CloudSyncDiffResponse) -> String {
    if response.sections.is_empty() {
        return "No cloud sync sections".to_string();
    }

    response
        .sections
        .iter()
        .map(|section| {
            format!(
                "{}\t{}\tinScope={}\tdirty={}\tlocal={}\tbaseline={}\tremote={}",
                section.category,
                section.id,
                section.in_scope,
                section.dirty,
                section.local_revision.as_deref().unwrap_or("-"),
                section.baseline_revision.as_deref().unwrap_or("-"),
                section.remote_revision.as_deref().unwrap_or("-")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_diff_table(response: &CloudSyncDiffResponse) -> String {
    if response.sections.is_empty() {
        return "No cloud sync sections".to_string();
    }

    let headers = [
        "CATEGORY", "ID", "SCOPE", "DIRTY", "LOCAL", "BASELINE", "REMOTE",
    ];
    let mut rows = vec![headers.map(str::to_string)];
    rows.extend(response.sections.iter().map(|section| {
        [
            section.category.to_string(),
            section.id.clone(),
            section.in_scope.to_string(),
            section.dirty.to_string(),
            section.local_revision.as_deref().unwrap_or("-").to_string(),
            section
                .baseline_revision
                .as_deref()
                .unwrap_or("-")
                .to_string(),
            section
                .remote_revision
                .as_deref()
                .unwrap_or("-")
                .to_string(),
        ]
    }));

    let widths = column_widths(&rows);
    rows.into_iter()
        .map(|row| format_table_row(&row, &widths))
        .collect::<Vec<_>>()
        .join("\n")
}

fn column_widths(rows: &[[String; 7]]) -> [usize; 7] {
    let mut widths = [0; 7];
    for row in rows {
        for (index, value) in row.iter().enumerate() {
            widths[index] = widths[index].max(value.len());
        }
    }
    widths
}

fn format_table_row(row: &[String; 7], widths: &[usize; 7]) -> String {
    // Keep table formatting dependency-free and deterministic for shell output.
    row.iter()
        .enumerate()
        .map(|(index, value)| format!("{value:<width$}", width = widths[index]))
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn diff_marks_changed_connection_revision_dirty() {
        let current = StructuredLocalState {
            connections: Some("conn-2".to_string()),
            ..StructuredLocalState::default()
        };
        let baseline = StructuredLocalState {
            connections: Some("conn-1".to_string()),
            ..StructuredLocalState::default()
        };
        let dirty = StructuredDirtySections {
            connections: true,
            ..StructuredDirtySections::default()
        };

        let sections = section_diffs(
            &SyncScope::default(),
            &current,
            Some(&baseline),
            &dirty,
            None,
        );

        let connections = sections
            .iter()
            .find(|section| section.category == "connections")
            .unwrap();
        assert!(connections.dirty);
        assert_eq!(connections.local_revision.as_deref(), Some("conn-2"));
        assert_eq!(connections.baseline_revision.as_deref(), Some("conn-1"));
    }

    #[test]
    fn app_setting_ids_include_remote_only_sections() {
        let remote = StructuredSectionRevisions {
            app_settings: BTreeMap::from([("appearance".to_string(), "remote-rev".to_string())]),
            ..StructuredSectionRevisions::default()
        };

        let ids = app_setting_ids(
            &SyncScope::default(),
            &StructuredLocalState::default(),
            None,
            Some(&remote),
        );

        assert!(ids.contains(&"appearance".to_string()));
    }

    #[test]
    fn diff_filter_can_keep_only_dirty_app_settings() {
        let sections = vec![
            SectionDiff {
                category: "connections",
                id: "connections".to_string(),
                in_scope: true,
                dirty: true,
                local_revision: None,
                baseline_revision: None,
                remote_revision: None,
            },
            SectionDiff {
                category: "appSettings",
                id: "appearance".to_string(),
                in_scope: true,
                dirty: true,
                local_revision: None,
                baseline_revision: None,
                remote_revision: None,
            },
            SectionDiff {
                category: "appSettings",
                id: "general".to_string(),
                in_scope: true,
                dirty: false,
                local_revision: None,
                baseline_revision: None,
                remote_revision: None,
            },
        ];
        let filter = DiffFilter {
            dirty_only: true,
            category: Some(CloudSyncDiffCategory::AppSettings),
        };

        let filtered = filter_sections(sections, &filter);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "appearance");
    }

    #[test]
    fn diff_table_includes_headers_and_rows() {
        let response = CloudSyncDiffResponse {
            path: "cloud_sync.json".to_string(),
            has_dirty: true,
            remote_revision: None,
            sections: vec![SectionDiff {
                category: "connections",
                id: "connections".to_string(),
                in_scope: true,
                dirty: true,
                local_revision: Some("local".to_string()),
                baseline_revision: None,
                remote_revision: Some("remote".to_string()),
            }],
        };

        let table = format_diff_table(&response);

        assert!(table.contains("CATEGORY"));
        assert!(table.contains("connections"));
        assert!(table.contains("remote"));
    }
}
