// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, path::Path};

use oxideterm_cloud_sync::{
    CloudSyncStatus,
    state::{CloudSyncPersistedState, CloudSyncStateStore},
};
use oxideterm_connections::{
    ConnectionStore, SavedConnectionsConflictStrategy, SavedConnectionsSyncSnapshot,
};
use oxideterm_settings::{PersistedSettings, merge_oxide_settings_snapshot, save_settings_to_path};
use serde::Serialize;
use serde_json::Value;

use crate::{
    args::{BackupInspectSection, BackupRestoreArgs, WriteArgs},
    backup::document::{BACKUP_FORMAT, read_backup_value, resolve_backup_query},
    cloud_sync_preview,
    error::{CliError, CliResult, runtime_error},
    output::{self, OutputFormat},
    paths::{default_cloud_sync_path, default_connections_path, default_settings_path},
    settings,
    write_guard::{self, WriteGuardPlan},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupRestoreResponse {
    backup_path: String,
    applied: bool,
    dry_run: bool,
    rollback_backup_path: Option<String>,
    rollback_backup_size_bytes: Option<u64>,
    plan: BackupRestorePlan,
    changes: Vec<BackupRestoreChange>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupRestorePlan {
    sections: Vec<BackupRestoreSectionPlan>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupRestoreSectionPlan {
    section: &'static str,
    target_path: String,
    supported: bool,
    change_count: usize,
    warning: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupRestoreChange {
    section: &'static str,
    path: String,
    before: Option<Value>,
    after: Option<Value>,
}

struct RestoreInput {
    section: BackupInspectSection,
    target_path: String,
    target: RestoreTarget,
    changes: Vec<BackupRestoreChange>,
    warning: Option<String>,
}

enum RestoreTarget {
    Settings(PersistedSettings),
    Connections(SavedConnectionsSyncSnapshot),
    CloudSync(CloudSyncPersistedState),
    Unsupported,
}

pub(super) fn restore(args: BackupRestoreArgs) -> CliResult<i32> {
    let path = resolve_backup_query(&args.query);
    let backup = read_backup_value(&path, args.write.json)?;
    validate_backup_format(&backup, args.write.json)?;

    let selected_sections = selected_sections(args.section);
    let restore_inputs = restore_inputs(&backup, &selected_sections, args.write.json)?;
    reject_explicit_unsupported(args.section, &restore_inputs, args.write.json)?;
    let plan = restore_plan(&restore_inputs);

    let changes = restore_inputs
        .iter()
        .flat_map(|input| input.changes.clone())
        .collect::<Vec<_>>();
    let warnings = restore_inputs
        .iter()
        .filter_map(|input| input.warning.clone())
        .collect::<Vec<_>>();
    let write = effective_restore_write(args.write);
    let has_supported_changes = restore_inputs.iter().any(|input| {
        !matches!(input.target, RestoreTarget::Unsupported) && !input.changes.is_empty()
    });
    let mut guard = write_guard::prepare_write(&write, has_supported_changes)?;

    if has_supported_changes && !write.dry_run {
        apply_restore_inputs(restore_inputs, write.json)?;
        write_guard::mark_applied(&mut guard);
    }

    let response = restore_response(path.display().to_string(), guard, plan, changes, warnings);
    let ok = response.applied || response.dry_run || response.changes.is_empty();
    match output::format_from_flag(write.json) {
        OutputFormat::Json => output::write_json_with_ok(&response, ok),
        OutputFormat::Text => {
            output::write_text(format_restore_text(&response));
            Ok(())
        }
    }?;
    Ok(if ok { 0 } else { 1 })
}

fn restore_inputs(
    backup: &Value,
    sections: &[BackupInspectSection],
    json: bool,
) -> CliResult<Vec<RestoreInput>> {
    sections
        .iter()
        .map(|section| match section {
            BackupInspectSection::Settings => settings_restore_input(backup, json),
            BackupInspectSection::Connections => connections_restore_input(backup, json),
            BackupInspectSection::CloudSync => cloud_sync_restore_input(backup, json),
        })
        .collect()
}

fn settings_restore_input(backup: &Value, json: bool) -> CliResult<RestoreInput> {
    let snapshot = backup_section(backup, "settings", json)?;
    let snapshot_json = serde_json::to_string(snapshot)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;
    let current = settings::load_settings_read_only(json)?;
    let restored = merge_oxide_settings_snapshot(&current.settings, &snapshot_json, None)
        .map_err(|error| CliError::new("settings_restore_failed", error.to_string(), json))?;
    let mut changes = Vec::new();
    collect_value_changes(
        "settings",
        &current.settings.to_value(),
        &restored.to_value(),
        "settings",
        &mut changes,
    );
    Ok(RestoreInput {
        section: BackupInspectSection::Settings,
        target_path: default_settings_path().display().to_string(),
        target: RestoreTarget::Settings(restored),
        changes,
        warning: None,
    })
}

fn connections_restore_input(backup: &Value, json: bool) -> CliResult<RestoreInput> {
    let snapshot = backup_section(backup, "connections", json)?.clone();
    let snapshot =
        serde_json::from_value::<SavedConnectionsSyncSnapshot>(snapshot).map_err(|error| {
            CliError::new(
                "connections_restore_parse_failed",
                format!("failed to decode connections backup snapshot: {error}"),
                json,
            )
        })?;
    let current_store = ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, json))?;
    let current_snapshot = current_store
        .export_saved_connections_snapshot()
        .map_err(|error| runtime_error(error, json))?;
    let before = serde_json::to_value(&current_snapshot.records)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;
    let after = serde_json::to_value(&snapshot.records)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;
    let changes = if before == after {
        Vec::new()
    } else {
        vec![BackupRestoreChange {
            section: "connections",
            path: "connections.records".to_string(),
            before: Some(Value::String(format!(
                "{} records",
                current_snapshot.records.len()
            ))),
            after: Some(Value::String(format!("{} records", snapshot.records.len()))),
        }]
    };
    Ok(RestoreInput {
        section: BackupInspectSection::Connections,
        target_path: default_connections_path().display().to_string(),
        target: RestoreTarget::Connections(snapshot),
        changes,
        warning: None,
    })
}

fn cloud_sync_restore_input(backup: &Value, json: bool) -> CliResult<RestoreInput> {
    let Some(state_value) = backup.get("cloudSyncState") else {
        let warning = "backup does not include restorable cloudSyncState; create a new backup before restoring cloud-sync".to_string();
        return Ok(RestoreInput {
            section: BackupInspectSection::CloudSync,
            target_path: default_cloud_sync_path().display().to_string(),
            target: RestoreTarget::Unsupported,
            changes: Vec::new(),
            warning: Some(warning),
        });
    };
    let mut restored = serde_json::from_value::<CloudSyncPersistedState>(state_value.clone())
        .map_err(|error| {
            CliError::new(
                "cloud_sync_restore_parse_failed",
                format!("failed to decode cloud sync state: {error}"),
                json,
            )
        })?;
    reset_cloud_sync_runtime_flags(&mut restored);
    let current = cloud_sync_preview::load_persisted_state(&default_cloud_sync_path(), json)?;
    let before = serde_json::to_value(&current)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;
    let after = serde_json::to_value(&restored)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;
    let mut changes = Vec::new();
    collect_value_changes("cloudSyncState", &before, &after, "cloudSync", &mut changes);
    Ok(RestoreInput {
        section: BackupInspectSection::CloudSync,
        target_path: default_cloud_sync_path().display().to_string(),
        target: RestoreTarget::CloudSync(restored),
        changes,
        warning: None,
    })
}

fn apply_restore_inputs(inputs: Vec<RestoreInput>, json: bool) -> CliResult<()> {
    for input in inputs {
        if input.changes.is_empty() {
            continue;
        }
        match input.target {
            RestoreTarget::Settings(settings) => {
                let saved = save_settings_to_path(Path::new(&input.target_path), settings)
                    .map_err(|error| {
                        CliError::new("settings_restore_failed", error.to_string(), json)
                    })?;
                if !saved.validation_warnings.is_empty() {
                    return Err(CliError::new(
                        "settings_restore_failed",
                        format!(
                            "settings save produced validation warnings: {}",
                            saved.validation_warnings.join("; ")
                        ),
                        json,
                    ));
                }
            }
            RestoreTarget::Connections(snapshot) => {
                let mut store = ConnectionStore::load(default_connections_path())
                    .map_err(|error| runtime_error(error, json))?;
                store
                    .apply_saved_connections_snapshot(
                        snapshot,
                        SavedConnectionsConflictStrategy::Replace,
                    )
                    .map_err(|error| runtime_error(error, json))?;
            }
            RestoreTarget::CloudSync(state) => {
                let mut store = CloudSyncStateStore::load(default_cloud_sync_path())
                    .map_err(|error| runtime_error(error, json))?;
                store.replace_state(state);
                store.save().map_err(|error| runtime_error(error, json))?;
            }
            RestoreTarget::Unsupported => {}
        }
    }
    Ok(())
}

fn restore_response(
    backup_path: String,
    guard: WriteGuardPlan,
    plan: BackupRestorePlan,
    changes: Vec<BackupRestoreChange>,
    warnings: Vec<String>,
) -> BackupRestoreResponse {
    BackupRestoreResponse {
        backup_path,
        applied: guard.applied,
        dry_run: guard.dry_run,
        rollback_backup_path: guard.backup_path,
        rollback_backup_size_bytes: guard.backup_size_bytes,
        plan,
        changes,
        warnings,
    }
}

fn restore_plan(inputs: &[RestoreInput]) -> BackupRestorePlan {
    BackupRestorePlan {
        sections: inputs
            .iter()
            .map(|input| BackupRestoreSectionPlan {
                section: section_name(input.section),
                target_path: input.target_path.clone(),
                supported: !matches!(input.target, RestoreTarget::Unsupported),
                change_count: input.changes.len(),
                warning: input.warning.clone(),
            })
            .collect(),
    }
}

fn section_name(section: BackupInspectSection) -> &'static str {
    match section {
        BackupInspectSection::Settings => "settings",
        BackupInspectSection::Connections => "connections",
        BackupInspectSection::CloudSync => "cloudSync",
    }
}

fn selected_sections(section: Option<BackupInspectSection>) -> Vec<BackupInspectSection> {
    section.map(|section| vec![section]).unwrap_or_else(|| {
        vec![
            BackupInspectSection::Settings,
            BackupInspectSection::Connections,
            BackupInspectSection::CloudSync,
        ]
    })
}

fn effective_restore_write(mut write: WriteArgs) -> WriteArgs {
    // Backup restore previews by default; scripts must pass --yes to mutate state.
    if !write.yes {
        write.dry_run = true;
    }
    write
}

fn reject_explicit_unsupported(
    explicit_section: Option<BackupInspectSection>,
    inputs: &[RestoreInput],
    json: bool,
) -> CliResult<()> {
    if explicit_section.is_some()
        && inputs
            .iter()
            .any(|input| matches!(input.target, RestoreTarget::Unsupported))
    {
        return Err(CliError::new(
            "backup_restore_unsupported",
            "selected backup section is not restorable from this backup",
            json,
        ));
    }
    Ok(())
}

fn validate_backup_format(backup: &Value, json: bool) -> CliResult<()> {
    if backup.get("format").and_then(Value::as_str) != Some(BACKUP_FORMAT) {
        return Err(CliError::new(
            "backup_format_invalid",
            "backup format is not recognized",
            json,
        ));
    }
    Ok(())
}

fn backup_section<'a>(backup: &'a Value, name: &'static str, json: bool) -> CliResult<&'a Value> {
    backup
        .get(name)
        .ok_or_else(|| CliError::new("backup_section_not_found", name, json))
}

fn reset_cloud_sync_runtime_flags(state: &mut CloudSyncPersistedState) {
    // Restored state must not resume an in-flight operation from another process lifetime.
    state.status = CloudSyncStatus::Idle;
    state.auto_upload_blocked_by_conflict = false;
    state.conflict_details = None;
}

fn collect_value_changes(
    prefix: &str,
    before: &Value,
    after: &Value,
    section: &'static str,
    changes: &mut Vec<BackupRestoreChange>,
) {
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(before_object), Value::Object(after_object)) => {
            let keys = before_object
                .keys()
                .chain(after_object.keys())
                .cloned()
                .collect::<HashSet<_>>();
            for key in keys {
                let child_path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                match (before_object.get(&key), after_object.get(&key)) {
                    (Some(before_child), Some(after_child)) => {
                        collect_value_changes(
                            &child_path,
                            before_child,
                            after_child,
                            section,
                            changes,
                        );
                    }
                    (before_child, after_child) => changes.push(BackupRestoreChange {
                        section,
                        path: child_path,
                        before: before_child.cloned(),
                        after: after_child.cloned(),
                    }),
                }
            }
        }
        _ => changes.push(BackupRestoreChange {
            section,
            path: prefix.to_string(),
            before: Some(before.clone()),
            after: Some(after.clone()),
        }),
    }
}

fn format_restore_text(response: &BackupRestoreResponse) -> String {
    let mut lines = vec![format!(
        "applied: {} dryRun={} changes={} rollback={}",
        response.applied,
        response.dry_run,
        response.changes.len(),
        response.rollback_backup_path.as_deref().unwrap_or("-")
    )];
    for section in &response.plan.sections {
        lines.push(format!(
            "section\t{}\tchanges={}\ttarget={}",
            section.section, section.change_count, section.target_path
        ));
    }
    for warning in &response.warnings {
        lines.push(format!("warning\t{warning}"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_args(yes: bool, dry_run: bool) -> WriteArgs {
        WriteArgs {
            dry_run,
            yes,
            no_backup: false,
            backup_before_write: false,
            json: true,
            format: None,
        }
    }

    #[test]
    fn restore_defaults_to_dry_run_without_yes() {
        let write = effective_restore_write(write_args(false, false));

        assert!(write.dry_run);
    }

    #[test]
    fn restore_yes_allows_real_write() {
        let write = effective_restore_write(write_args(true, false));

        assert!(!write.dry_run);
    }

    #[test]
    fn cloud_sync_runtime_flags_are_reset_before_restore() {
        let mut state = CloudSyncPersistedState {
            status: CloudSyncStatus::Uploading,
            auto_upload_blocked_by_conflict: true,
            ..CloudSyncPersistedState::default()
        };

        reset_cloud_sync_runtime_flags(&mut state);

        assert_eq!(state.status, CloudSyncStatus::Idle);
        assert!(!state.auto_upload_blocked_by_conflict);
    }
}
