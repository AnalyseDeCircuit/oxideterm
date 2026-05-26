// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, fs, io::ErrorKind, path::Path};

use oxideterm_settings::{
    ALL_OXIDE_SETTINGS_SECTIONS, DEFAULT_OXIDE_SETTINGS_SECTIONS, PersistedSettings,
    default_settings_path, export_oxide_settings_snapshot_json, merge_oxide_settings_snapshot,
    sanitize_settings_value, save_settings_to_path,
};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::{
    args::{
        SettingsAction, SettingsCommand, SettingsExportArgs, SettingsSetArgs, SettingsUnsetArgs,
        SettingsValidateArgs,
    },
    error::{CliError, CliResult},
    json_query,
    output::{self, OutputFormat},
    write_guard::{self, WriteGuardPlan},
};

const MAX_SETTINGS_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsPathResponse {
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsExportResponse {
    path: String,
    section_ids: Vec<String>,
    include_local_terminal_env_vars: bool,
    snapshot: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsValidationReport {
    pub(crate) path: String,
    pub(crate) ok: bool,
    pub(crate) strict: bool,
    pub(crate) version: u32,
    pub(crate) migration_warnings: Vec<String>,
    pub(crate) validation_warnings: Vec<String>,
    pub(crate) unknown_top_level_fields: Vec<String>,
    pub(crate) default_section_ids: Vec<String>,
    pub(crate) exported_section_ids: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsSectionsResponse {
    default_section_ids: Vec<String>,
    sections: Vec<SettingsSectionInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsSectionInfo {
    id: String,
    default_enabled: bool,
    can_include_local_terminal_env_vars: bool,
}

pub(crate) struct ReadOnlySettings {
    pub(crate) path: String,
    pub(crate) settings: PersistedSettings,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsWriteResponse {
    path: String,
    applied: bool,
    dry_run: bool,
    backup_path: Option<String>,
    backup_size_bytes: Option<u64>,
    changes: Vec<SettingsChange>,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsChange {
    path: String,
    before: Option<Value>,
    after: Option<Value>,
}

pub fn run(command: SettingsCommand) -> CliResult<i32> {
    match command.action {
        SettingsAction::Path(args) => {
            let path = default_settings_path();
            match output::format_from_flag(args.json) {
                OutputFormat::Json => {
                    output::write_json(&SettingsPathResponse {
                        path: path.display().to_string(),
                    })?;
                    Ok(0)
                }
                OutputFormat::Text => {
                    output::write_text(path.display().to_string());
                    Ok(0)
                }
            }
        }
        SettingsAction::Sections(args) => {
            list_sections(args)?;
            Ok(0)
        }
        SettingsAction::Validate(args) => validate_settings(args),
        SettingsAction::Set(args) => write_setting_value(args),
        SettingsAction::Unset(args) => unset_setting_value(args),
        SettingsAction::Apply(args) => apply_settings_snapshot(args.path, None, args.write),
        SettingsAction::Import(args) => {
            let sections = selected_sections(&args.sections);
            apply_settings_snapshot(args.path, sections, args.write)
        }
        SettingsAction::Show(args) => {
            let settings = load_settings_read_only(args.json)?;
            let value = settings.settings.to_value();
            match output::format_from_flag(args.json) {
                OutputFormat::Json => {
                    output::write_json(&value)?;
                    Ok(0)
                }
                OutputFormat::Text => {
                    output::write_text(serde_json::to_string_pretty(&value).map_err(|error| {
                        CliError::new("serialization_failed", error.to_string(), args.json)
                    })?);
                    Ok(0)
                }
            }
        }
        SettingsAction::Get(args) => {
            let settings = load_settings_read_only(args.json)?;
            let value = settings.settings.to_value();
            let Some(found) = json_query::value_at_path(&value, &args.key) else {
                return Err(CliError::new(
                    "settings_key_not_found",
                    format!("settings key '{}' was not found", args.key),
                    args.json,
                ));
            };
            match output::format_from_flag(args.json) {
                OutputFormat::Json => {
                    output::write_json(found)?;
                    Ok(0)
                }
                OutputFormat::Text => {
                    output::write_text(json_query::value_to_text(found));
                    Ok(0)
                }
            }
        }
        SettingsAction::Export(args) => {
            export_settings(args)?;
            Ok(0)
        }
    }
}

fn write_setting_value(args: SettingsSetArgs) -> CliResult<i32> {
    let json = args.write.json;
    let value = parse_setting_input_value(&args.value);
    apply_settings_edit(&args.key, args.write, |settings| {
        set_existing_json_path(settings, &args.key, value, json)
    })
}

fn unset_setting_value(args: SettingsUnsetArgs) -> CliResult<i32> {
    let json = args.write.json;
    apply_settings_edit(&args.key, args.write, |settings| {
        unset_existing_json_path(settings, &args.key, json)
    })
}

fn apply_settings_edit(
    key: &str,
    write: crate::args::WriteArgs,
    edit: impl FnOnce(&mut Value) -> CliResult<()>,
) -> CliResult<i32> {
    let path = default_settings_path();
    let raw = read_settings_value(&path, write.json)?;
    let before_sanitized = sanitize_settings_value(raw.clone())
        .map_err(|error| CliError::new("settings_parse_failed", error.to_string(), write.json))?;
    let before = json_query::value_at_path(&before_sanitized.settings.to_value(), key).cloned();
    let mut edited = before_sanitized.settings.to_value();
    edit(&mut edited)?;
    let after_sanitized = sanitize_settings_value(edited)
        .map_err(|error| CliError::new("settings_parse_failed", error.to_string(), write.json))?;
    let after_value = after_sanitized.settings.to_value();
    let after = json_query::value_at_path(&after_value, key).cloned();
    let changes = changed_value(key, before, after);
    finish_settings_write(
        path.display().to_string(),
        write,
        after_sanitized.settings,
        changes,
        after_sanitized.validation_warnings,
    )
}

fn apply_settings_snapshot(
    snapshot_path: String,
    sections: Option<HashSet<String>>,
    write: crate::args::WriteArgs,
) -> CliResult<i32> {
    let path = default_settings_path();
    let current = load_settings_read_only(write.json)?;
    let snapshot_json = fs::read_to_string(&snapshot_path).map_err(|error| {
        CliError::new(
            "settings_import_read_failed",
            format!("failed to read settings snapshot {snapshot_path}: {error}"),
            write.json,
        )
    })?;
    let merged =
        merge_oxide_settings_snapshot(&current.settings, &snapshot_json, sections.as_ref())
            .map_err(|error| {
                CliError::new("settings_import_failed", error.to_string(), write.json)
            })?;
    let mut changes = Vec::new();
    collect_value_changes(
        "",
        &current.settings.to_value(),
        &merged.to_value(),
        &mut changes,
    );
    finish_settings_write(
        path.display().to_string(),
        write,
        merged,
        changes,
        Vec::new(),
    )
}

fn finish_settings_write(
    path: String,
    write: crate::args::WriteArgs,
    settings: PersistedSettings,
    changes: Vec<SettingsChange>,
    warnings: Vec<String>,
) -> CliResult<i32> {
    let mut guard = if warnings.is_empty() {
        write_guard::prepare_write(&write, !changes.is_empty())?
    } else {
        WriteGuardPlan {
            dry_run: write.dry_run,
            applied: false,
            backup_path: None,
            backup_size_bytes: None,
        }
    };
    if warnings.is_empty() && !changes.is_empty() && !write.dry_run {
        let saved = save_settings_to_path(Path::new(&path), settings).map_err(|error| {
            CliError::new("settings_write_failed", error.to_string(), write.json)
        })?;
        if !saved.validation_warnings.is_empty() {
            return Err(CliError::new(
                "settings_write_failed",
                format!(
                    "settings save produced validation warnings: {}",
                    saved.validation_warnings.join("; ")
                ),
                write.json,
            ));
        }
        write_guard::mark_applied(&mut guard);
    }
    let response = settings_write_response(path, guard, changes, warnings);
    let ok = response.warnings.is_empty()
        && (response.applied || write.dry_run || response.changes.is_empty());
    match output::format_from_flag(write.json) {
        OutputFormat::Json => output::write_json_with_ok(&response, ok),
        OutputFormat::Text => {
            output::write_text(format_settings_write_text(&response));
            Ok(())
        }
    }?;
    Ok(if ok { 0 } else { 1 })
}

fn validate_settings(args: SettingsValidateArgs) -> CliResult<i32> {
    let report = validate_settings_read_only(args.json, args.strict)?;
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json_with_ok(&report, report.ok),
        OutputFormat::Text => {
            output::write_text(format_settings_validation_text(&report));
            Ok(())
        }
    }?;

    Ok(if report.ok { 0 } else { 1 })
}

fn list_sections(args: crate::args::JsonArgs) -> CliResult<()> {
    let response = settings_sections_response();
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            for section in response.sections {
                let default_marker = if section.default_enabled {
                    "default"
                } else {
                    "optional"
                };
                let env_marker = if section.can_include_local_terminal_env_vars {
                    " env-vars-optional"
                } else {
                    ""
                };
                output::write_text(format!("{}\t{}{}", section.id, default_marker, env_marker));
            }
            Ok(())
        }
    }
}

fn settings_sections_response() -> SettingsSectionsResponse {
    SettingsSectionsResponse {
        default_section_ids: DEFAULT_OXIDE_SETTINGS_SECTIONS
            .iter()
            .map(|section| (*section).to_string())
            .collect(),
        sections: ALL_OXIDE_SETTINGS_SECTIONS
            .iter()
            .map(|section| SettingsSectionInfo {
                id: (*section).to_string(),
                default_enabled: DEFAULT_OXIDE_SETTINGS_SECTIONS.contains(section),
                can_include_local_terminal_env_vars: *section == "localTerminal",
            })
            .collect(),
    }
}

fn export_settings(args: SettingsExportArgs) -> CliResult<()> {
    let settings = load_settings_read_only(args.json)?;
    let selected_sections = selected_sections(&args.sections);
    let snapshot_json = export_oxide_settings_snapshot_json(
        &settings.settings,
        selected_sections.as_ref(),
        args.include_local_terminal_env_vars,
    )
    .map_err(|error| CliError::new("settings_export_failed", error.to_string(), args.json))?;

    match output::format_from_flag(args.json) {
        OutputFormat::Json => {
            let snapshot = serde_json::from_str::<Value>(&snapshot_json).map_err(|error| {
                CliError::new("serialization_failed", error.to_string(), args.json)
            })?;
            output::write_json(&SettingsExportResponse {
                path: settings.path,
                section_ids: snapshot_section_ids(&snapshot),
                include_local_terminal_env_vars: args.include_local_terminal_env_vars,
                snapshot,
            })
        }
        OutputFormat::Text => {
            output::write_text(snapshot_json);
            Ok(())
        }
    }
}

pub(crate) fn load_settings_read_only(json: bool) -> CliResult<ReadOnlySettings> {
    let path = default_settings_path();
    let raw = read_settings_value(&path, json)?;
    let sanitized = sanitize_settings_value(raw)
        .map_err(|error| CliError::new("settings_parse_failed", error.to_string(), json))?;
    Ok(ReadOnlySettings {
        path: path.display().to_string(),
        settings: sanitized.settings,
    })
}

pub(crate) fn validate_settings_read_only(
    json: bool,
    strict: bool,
) -> CliResult<SettingsValidationReport> {
    let path = default_settings_path();
    let raw = read_settings_value(&path, json)?;
    let unknown_top_level_fields = unknown_top_level_fields(&raw);
    let sanitized = sanitize_settings_value(raw)
        .map_err(|error| CliError::new("settings_parse_failed", error.to_string(), json))?;
    let snapshot_json = export_oxide_settings_snapshot_json(&sanitized.settings, None, false)
        .map_err(|error| CliError::new("settings_export_failed", error.to_string(), json))?;
    let snapshot = serde_json::from_str::<Value>(&snapshot_json)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;
    let default_section_ids = DEFAULT_OXIDE_SETTINGS_SECTIONS
        .iter()
        .map(|section| (*section).to_string())
        .collect::<Vec<_>>();
    let exported_section_ids = snapshot_section_ids(&snapshot);
    let has_migration_warnings = !sanitized.migration_warnings.is_empty();
    let ok = sanitized.validation_warnings.is_empty()
        && unknown_top_level_fields.is_empty()
        && default_section_ids
            .iter()
            .all(|section| exported_section_ids.contains(section))
        && (!strict || !has_migration_warnings);

    Ok(SettingsValidationReport {
        path: path.display().to_string(),
        ok,
        strict,
        version: sanitized.settings.version,
        migration_warnings: sanitized.migration_warnings,
        validation_warnings: sanitized.validation_warnings,
        unknown_top_level_fields,
        default_section_ids,
        exported_section_ids,
    })
}

fn read_settings_value(path: &Path, json: bool) -> CliResult<Value> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(PersistedSettings::default().to_value());
        }
        Err(error) => {
            return Err(CliError::new(
                "settings_read_failed",
                format!("failed to stat settings file {}: {error}", path.display()),
                json,
            ));
        }
    };
    if metadata.len() > MAX_SETTINGS_FILE_BYTES {
        return Err(CliError::new(
            "settings_too_large",
            "settings file exceeds size limit",
            json,
        ));
    }

    // Accept both current envelope shape and older raw settings JSON without writing either back.
    let contents = fs::read_to_string(path).map_err(|error| {
        CliError::new(
            "settings_read_failed",
            format!("failed to read settings file {}: {error}", path.display()),
            json,
        )
    })?;
    if contents.trim().is_empty() {
        return Ok(PersistedSettings::default().to_value());
    }
    let value = serde_json::from_str::<Value>(&contents).map_err(|error| {
        CliError::new(
            "settings_parse_failed",
            format!("failed to parse settings file {}: {error}", path.display()),
            json,
        )
    })?;
    Ok(value.get("settings").cloned().unwrap_or(value))
}

fn selected_sections(sections: &[String]) -> Option<HashSet<String>> {
    let selected = sections
        .iter()
        .map(|section| section.trim())
        .filter(|section| !section.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>();
    (!selected.is_empty()).then_some(selected)
}

fn parse_setting_input_value(value: &str) -> Value {
    // Accept JSON literals for scripts, while keeping bare strings ergonomic for humans.
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()))
}

fn set_existing_json_path(
    settings: &mut Value,
    key: &str,
    value: Value,
    json: bool,
) -> CliResult<()> {
    let (parent, leaf) = parent_object_mut(settings, key, json)?;
    if !parent.contains_key(&leaf) {
        return Err(CliError::new(
            "settings_key_not_found",
            format!("settings key '{}' was not found", key),
            json,
        ));
    }
    parent.insert(leaf.to_string(), value);
    Ok(())
}

fn unset_existing_json_path(settings: &mut Value, key: &str, json: bool) -> CliResult<()> {
    let (parent, leaf) = parent_object_mut(settings, key, json)?;
    if parent.remove(&leaf).is_none() {
        return Err(CliError::new(
            "settings_key_not_found",
            format!("settings key '{}' was not found", key),
            json,
        ));
    }
    Ok(())
}

fn parent_object_mut<'a>(
    settings: &'a mut Value,
    key: &str,
    json: bool,
) -> CliResult<(&'a mut Map<String, Value>, String)> {
    let segments = key
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let Some((leaf, parents)) = segments.split_last() else {
        return Err(CliError::new(
            "settings_key_invalid",
            "settings key must not be empty",
            json,
        ));
    };
    let mut current = settings;
    for segment in parents {
        current = current.get_mut(*segment).ok_or_else(|| {
            CliError::new(
                "settings_key_not_found",
                format!("settings key '{}' was not found", key),
                json,
            )
        })?;
    }
    let Some(parent) = current.as_object_mut() else {
        return Err(CliError::new(
            "settings_key_not_object",
            format!("settings key '{}' does not have an object parent", key),
            json,
        ));
    };
    Ok((parent, (*leaf).to_string()))
}

fn changed_value(path: &str, before: Option<Value>, after: Option<Value>) -> Vec<SettingsChange> {
    if before == after {
        Vec::new()
    } else {
        vec![SettingsChange {
            path: path.to_string(),
            before,
            after,
        }]
    }
}

fn collect_value_changes(
    prefix: &str,
    before: &Value,
    after: &Value,
    changes: &mut Vec<SettingsChange>,
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
                        collect_value_changes(&child_path, before_child, after_child, changes);
                    }
                    (before_child, after_child) => changes.push(SettingsChange {
                        path: child_path,
                        before: before_child.cloned(),
                        after: after_child.cloned(),
                    }),
                }
            }
        }
        _ => changes.push(SettingsChange {
            path: prefix.to_string(),
            before: Some(before.clone()),
            after: Some(after.clone()),
        }),
    }
}

fn settings_write_response(
    path: String,
    guard: WriteGuardPlan,
    changes: Vec<SettingsChange>,
    warnings: Vec<String>,
) -> SettingsWriteResponse {
    SettingsWriteResponse {
        path,
        applied: guard.applied,
        dry_run: guard.dry_run,
        backup_path: guard.backup_path,
        backup_size_bytes: guard.backup_size_bytes,
        changes,
        warnings,
    }
}

fn format_settings_write_text(response: &SettingsWriteResponse) -> String {
    let mut lines = vec![format!(
        "applied: {} dryRun={} changes={} backup={}",
        response.applied,
        response.dry_run,
        response.changes.len(),
        response.backup_path.as_deref().unwrap_or("-")
    )];
    for change in &response.changes {
        lines.push(format!(
            "{}\t{}\t=>\t{}",
            change.path,
            optional_json_text(change.before.as_ref()),
            optional_json_text(change.after.as_ref())
        ));
    }
    for warning in &response.warnings {
        lines.push(format!("warning\t{warning}"));
    }
    lines.join("\n")
}

fn optional_json_text(value: Option<&Value>) -> String {
    value
        .map(|value| serde_json::to_string(value).unwrap_or_else(|_| "<json>".to_string()))
        .unwrap_or_else(|| "-".to_string())
}

fn snapshot_section_ids(snapshot: &Value) -> Vec<String> {
    snapshot
        .get("sectionIds")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn unknown_top_level_fields(raw: &Value) -> Vec<String> {
    let Some(raw_object) = raw.as_object() else {
        return Vec::new();
    };
    let default_value = PersistedSettings::default().to_value();
    let Some(default_object) = default_value.as_object() else {
        return Vec::new();
    };
    let mut fields = raw_object
        .keys()
        .filter(|key| !default_object.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    fields.sort();
    fields
}

fn format_settings_validation_text(report: &SettingsValidationReport) -> String {
    let mut lines = vec![format!(
        "ok: {} version={} validationWarnings={} migrationWarnings={} unknownTopLevelFields={} exportedSections={}/{}",
        report.ok,
        report.version,
        report.validation_warnings.len(),
        report.migration_warnings.len(),
        report.unknown_top_level_fields.len(),
        report.exported_section_ids.len(),
        ALL_OXIDE_SETTINGS_SECTIONS.len()
    )];
    for warning in &report.validation_warnings {
        lines.push(format!("warning\tvalidation\t{warning}"));
    }
    for warning in &report.migration_warnings {
        lines.push(format!("info\tmigration\t{warning}"));
    }
    for field in &report.unknown_top_level_fields {
        lines.push(format!("warning\tunknownTopLevelField\t{field}"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_sections_ignores_empty_names() {
        let sections = selected_sections(&["general".to_string(), " ".to_string()]).unwrap();

        assert!(sections.contains("general"));
        assert_eq!(sections.len(), 1);
        assert!(selected_sections(&[" ".to_string()]).is_none());
    }

    #[test]
    fn settings_sections_marks_local_terminal_env_vars() {
        let response = settings_sections_response();
        let local_terminal = response
            .sections
            .iter()
            .find(|section| section.id == "localTerminal")
            .unwrap();

        assert!(local_terminal.can_include_local_terminal_env_vars);
        assert!(
            response
                .default_section_ids
                .contains(&"general".to_string())
        );
    }

    #[test]
    fn unknown_top_level_fields_reports_only_extra_root_keys() {
        let mut raw = PersistedSettings::default().to_value();
        raw.as_object_mut()
            .unwrap()
            .insert("legacyThing".to_string(), Value::Bool(true));

        assert_eq!(unknown_top_level_fields(&raw), ["legacyThing"]);
    }

    #[test]
    fn setting_input_parses_json_or_bare_string() {
        assert_eq!(parse_setting_input_value("2000"), Value::from(2000));
        assert_eq!(
            parse_setting_input_value("Maple Mono"),
            Value::from("Maple Mono")
        );
    }

    #[test]
    fn set_and_unset_existing_json_path_require_existing_leaf() {
        let mut value = PersistedSettings::default().to_value();

        set_existing_json_path(&mut value, "terminal.scrollback", Value::from(2000), true).unwrap();
        assert_eq!(value["terminal"]["scrollback"], Value::from(2000));
        unset_existing_json_path(&mut value, "ai.customSystemPrompt", true).unwrap();
        assert!(value["ai"].get("customSystemPrompt").is_none());
        assert!(set_existing_json_path(&mut value, "missing.key", Value::Null, true).is_err());
    }

    #[test]
    fn collect_value_changes_reports_leaf_paths() {
        let before = serde_json::json!({
            "terminal": { "scrollback": 1000 },
            "ai": { "enabled": false }
        });
        let after = serde_json::json!({
            "terminal": { "scrollback": 2000 },
            "ai": { "enabled": false }
        });
        let mut changes = Vec::new();

        collect_value_changes("", &before, &after, &mut changes);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "terminal.scrollback");
        assert_eq!(changes[0].before, Some(Value::from(1000)));
        assert_eq!(changes[0].after, Some(Value::from(2000)));
    }
}
