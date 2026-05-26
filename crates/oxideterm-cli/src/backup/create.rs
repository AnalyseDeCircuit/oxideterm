// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fs, path::PathBuf};

use serde::Serialize;

use crate::{
    args::{BackupCreateArgs, JsonArgs},
    backup::document::{
        BackupDocument, BackupSummary, backup_file_name, backup_summary_from_document,
        build_backup_document,
    },
    error::{CliError, CliResult},
    output::{self, OutputFormat},
    paths::default_backups_dir,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupCreateResponse {
    path: String,
    size_bytes: u64,
    backup: BackupDocument,
}

#[derive(Clone, Debug)]
pub(crate) struct CreatedBackup {
    pub(crate) path: String,
    pub(crate) size_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupPreviewResponse {
    estimated_size_bytes: u64,
    summary: BackupSummary,
}

pub(super) fn preview(args: JsonArgs) -> CliResult<()> {
    let backup = build_backup_document(args.json)?;
    let estimated_size_bytes = serde_json::to_vec_pretty(&backup)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), args.json))?
        .len() as u64;
    let response = BackupPreviewResponse {
        estimated_size_bytes,
        summary: backup_summary_from_document(&backup),
    };

    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format_backup_preview_text(&response));
            Ok(())
        }
    }
}

pub(super) fn create(args: BackupCreateArgs) -> CliResult<()> {
    let (created, backup) = write_backup_document(args.output.as_deref(), args.json)?;
    let response = BackupCreateResponse {
        path: created.path,
        size_bytes: created.size_bytes,
        backup,
    };

    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format!(
                "{}\tsize={} bytes",
                response.path, response.size_bytes
            ));
            Ok(())
        }
    }
}

pub(crate) fn create_backup_file(output: Option<&str>, json: bool) -> CliResult<CreatedBackup> {
    write_backup_document(output, json).map(|(created, _backup)| created)
}

fn write_backup_document(
    output: Option<&str>,
    json: bool,
) -> CliResult<(CreatedBackup, BackupDocument)> {
    let backup = build_backup_document(json)?;
    let path = output_path(output, backup.created_at_ms);
    let backup_dir = path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(default_backups_dir);
    fs::create_dir_all(&backup_dir).map_err(|error| {
        CliError::new(
            "backup_dir_create_failed",
            format!(
                "failed to create backup dir {}: {error}",
                backup_dir.display()
            ),
            json,
        )
    })?;
    let bytes = serde_json::to_vec_pretty(&backup)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;
    fs::write(&path, &bytes).map_err(|error| {
        CliError::new(
            "backup_write_failed",
            format!("failed to write backup {}: {error}", path.display()),
            json,
        )
    })?;
    Ok((
        CreatedBackup {
            path: path.display().to_string(),
            size_bytes: bytes.len() as u64,
        },
        backup,
    ))
}

fn output_path(output: Option<&str>, created_at_ms: u64) -> PathBuf {
    output
        .map(PathBuf::from)
        .unwrap_or_else(|| default_backups_dir().join(backup_file_name(created_at_ms)))
}

fn format_backup_preview_text(response: &BackupPreviewResponse) -> String {
    format!(
        "format: {}\nestimatedSize: {} bytes\nsettingsSections: {}\nconnections: {}\ncloudSyncHistory: {}\ncloudSyncDirty: {}",
        response.summary.format,
        response.estimated_size_bytes,
        response.summary.settings_section_count,
        response.summary.connection_record_count,
        response.summary.cloud_sync_history_count,
        response.summary.cloud_sync_local_dirty
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_path_uses_requested_file() {
        let path = output_path(Some("/tmp/custom.json"), 1);

        assert_eq!(path, PathBuf::from("/tmp/custom.json"));
    }
}
