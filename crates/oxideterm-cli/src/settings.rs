// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, fs, io::ErrorKind, path::Path};

use oxideterm_settings::{
    ALL_OXIDE_SETTINGS_SECTIONS, DEFAULT_OXIDE_SETTINGS_SECTIONS, PersistedSettings,
    default_settings_path, export_oxide_settings_snapshot_json, sanitize_settings_value,
};
use serde::Serialize;
use serde_json::Value;

use crate::{
    args::{SettingsAction, SettingsCommand, SettingsExportArgs},
    error::{CliError, CliResult},
    json_query,
    output::{self, OutputFormat},
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

pub fn run(command: SettingsCommand) -> CliResult<()> {
    match command.action {
        SettingsAction::Path(args) => {
            let path = default_settings_path();
            match output::format_from_flag(args.json) {
                OutputFormat::Json => output::write_json(&SettingsPathResponse {
                    path: path.display().to_string(),
                }),
                OutputFormat::Text => {
                    output::write_text(path.display().to_string());
                    Ok(())
                }
            }
        }
        SettingsAction::Sections(args) => list_sections(args),
        SettingsAction::Show(args) => {
            let settings = load_settings_read_only(args.json)?;
            let value = settings.settings.to_value();
            match output::format_from_flag(args.json) {
                OutputFormat::Json => output::write_json(&value),
                OutputFormat::Text => {
                    output::write_text(serde_json::to_string_pretty(&value).map_err(|error| {
                        CliError::new("serialization_failed", error.to_string(), args.json)
                    })?);
                    Ok(())
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
                OutputFormat::Json => output::write_json(found),
                OutputFormat::Text => {
                    output::write_text(json_query::value_to_text(found));
                    Ok(())
                }
            }
        }
        SettingsAction::Export(args) => export_settings(args),
    }
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
}
