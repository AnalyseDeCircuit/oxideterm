// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fs, path::Path};

use serde::Serialize;
use serde_json::Value;

use crate::{
    args::{BackupInspectArgs, BackupInspectSection, JsonArgs},
    backup::document::{
        BACKUP_FORMAT, format_backup_summary, is_backup_file, read_backup_value,
        resolve_backup_query,
    },
    error::{CliError, CliResult},
    output::{self, OutputFormat},
    paths::default_backups_dir,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupListResponse {
    dir: String,
    count: usize,
    backups: Vec<BackupListEntry>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupListEntry {
    file_name: String,
    path: String,
    size_bytes: u64,
    created_at_ms: Option<u64>,
    inspect_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupInspectResponse {
    path: String,
    backup: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupInspectSummaryResponse {
    path: String,
    summary: BackupInspectSummary,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupInspectSectionResponse {
    path: String,
    section: &'static str,
    value: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupInspectSummary {
    format: String,
    created_at_ms: Option<u64>,
    settings_section_count: usize,
    connection_record_count: usize,
    cloud_sync_history_count: usize,
}

pub(super) fn list(args: JsonArgs) -> CliResult<()> {
    let backup_dir = default_backups_dir();
    let mut backups = Vec::new();
    match fs::read_dir(&backup_dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|error| {
                    CliError::new("backup_list_failed", error.to_string(), args.json)
                })?;
                let path = entry.path();
                if is_backup_file(&path) {
                    backups.push(backup_list_entry(&path));
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CliError::new(
                "backup_list_failed",
                format!(
                    "failed to list backup dir {}: {error}",
                    backup_dir.display()
                ),
                args.json,
            ));
        }
    }
    backups.sort_by(|left, right| right.created_at_ms.cmp(&left.created_at_ms));
    let response = BackupListResponse {
        dir: backup_dir.display().to_string(),
        count: backups.len(),
        backups,
    };

    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            if response.backups.is_empty() {
                output::write_text("No local backups");
            } else {
                for backup in response.backups {
                    output::write_text(format_backup_list_entry(&backup));
                }
            }
            Ok(())
        }
    }
}

pub(super) fn inspect(args: BackupInspectArgs) -> CliResult<()> {
    let path = resolve_backup_query(&args.query);
    let backup = read_backup_value(&path, args.json)?;
    let path_text = path.display().to_string();
    let full = args.full;
    if let Some(section) = args.section {
        return inspect_section(path_text, &backup, section, args.json);
    }

    match output::format_from_flag(args.json) {
        OutputFormat::Json if full => output::write_json(&BackupInspectResponse {
            path: path_text,
            backup,
        }),
        OutputFormat::Json => output::write_json(&BackupInspectSummaryResponse {
            path: path_text,
            summary: inspect_summary(&backup),
        }),
        OutputFormat::Text => {
            output::write_text(format_backup_summary(&backup));
            Ok(())
        }
    }
}

fn inspect_section(
    path: String,
    backup: &Value,
    section: BackupInspectSection,
    json: bool,
) -> CliResult<()> {
    let section_name = backup_section_name(section);
    let value = backup
        .get(section_name)
        .cloned()
        .ok_or_else(|| CliError::new("backup_section_not_found", section_name, json))?;
    match output::format_from_flag(json) {
        OutputFormat::Json => output::write_json(&BackupInspectSectionResponse {
            path,
            section: section_name,
            value,
        }),
        OutputFormat::Text => {
            output::write_text(
                serde_json::to_string_pretty(&value).map_err(|error| {
                    CliError::new("serialization_failed", error.to_string(), json)
                })?,
            );
            Ok(())
        }
    }
}

fn backup_section_name(section: BackupInspectSection) -> &'static str {
    match section {
        BackupInspectSection::Connections => "connections",
        BackupInspectSection::Settings => "settings",
        BackupInspectSection::CloudSync => "cloudSync",
    }
}

fn backup_list_entry(path: &Path) -> BackupListEntry {
    let metadata = fs::metadata(path).ok();
    let (created_at_ms, inspect_error) = read_backup_created_at(path);
    BackupListEntry {
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        path: path.display().to_string(),
        size_bytes: metadata
            .as_ref()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len())
            .unwrap_or_default(),
        created_at_ms,
        inspect_error,
    }
}

fn read_backup_created_at(path: &Path) -> (Option<u64>, Option<String>) {
    match fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
    {
        Some(value) => (
            value.get("createdAtMs").and_then(Value::as_u64),
            validate_backup_format(&value),
        ),
        None => (None, Some("backup is not readable JSON".to_string())),
    }
}

fn validate_backup_format(value: &Value) -> Option<String> {
    (value.get("format").and_then(Value::as_str) != Some(BACKUP_FORMAT))
        .then(|| "backup format is not recognized".to_string())
}

fn format_backup_list_entry(entry: &BackupListEntry) -> String {
    let created_at = entry
        .created_at_ms
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string());
    let error = entry.inspect_error.as_deref().unwrap_or("-");
    format!(
        "{}\tcreatedAtMs={}\tsize={} error={}",
        entry.file_name, created_at, entry.size_bytes, error
    )
}

fn inspect_summary(backup: &Value) -> BackupInspectSummary {
    BackupInspectSummary {
        format: backup
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        created_at_ms: backup.get("createdAtMs").and_then(Value::as_u64),
        settings_section_count: backup
            .get("settings")
            .and_then(|settings| settings.get("sectionIds"))
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default(),
        connection_record_count: backup
            .get("connections")
            .and_then(|connections| connections.get("records"))
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default(),
        cloud_sync_history_count: backup
            .get("cloudSync")
            .and_then(|cloud_sync| cloud_sync.get("history"))
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn inspect_summary_counts_sections_and_records() {
        let summary = inspect_summary(&json!({
            "format": "oxideterm-cli-backup-v1",
            "createdAtMs": 1,
            "settings": { "sectionIds": ["general"] },
            "connections": { "records": [1, 2] },
            "cloudSync": { "history": [1] }
        }));

        assert_eq!(summary.settings_section_count, 1);
        assert_eq!(summary.connection_record_count, 2);
        assert_eq!(summary.cloud_sync_history_count, 1);
    }

    #[test]
    fn backup_section_names_match_document_fields() {
        assert_eq!(
            backup_section_name(BackupInspectSection::CloudSync),
            "cloudSync"
        );
        assert_eq!(
            backup_section_name(BackupInspectSection::Connections),
            "connections"
        );
    }
}
