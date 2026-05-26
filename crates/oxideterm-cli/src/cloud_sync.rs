// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use oxideterm_cloud_sync::state::{
    CloudSyncHistoryEntry, CloudSyncPersistedState, CloudSyncRollbackBackup, CloudSyncStateStore,
};
use serde::Serialize;

use crate::{
    args::{CloudSyncAction, CloudSyncCommand, JsonArgs},
    cloud_sync_preview, cloud_sync_state,
    error::{CliResult, runtime_error},
    output::{self, OutputFormat},
    paths::default_cloud_sync_path,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncStatusResponse {
    path: String,
    status: String,
    backend_type: String,
    namespace: String,
    auto_upload_enabled: bool,
    local_dirty: bool,
    last_sync_at: Option<String>,
    last_upload_at: Option<String>,
    last_check_at: Option<String>,
    last_known_remote_revision: Option<String>,
    history_count: usize,
    rollback_backup_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncHistoryResponse {
    path: String,
    count: usize,
    history: Vec<CloudSyncHistoryEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncBackupsResponse {
    path: String,
    count: usize,
    backups: Vec<CloudSyncRollbackBackup>,
}

pub fn run(command: CloudSyncCommand) -> CliResult<()> {
    match command.action {
        CloudSyncAction::Status(args) => status(args),
        CloudSyncAction::Preview(args) => cloud_sync_preview::preview(args),
        CloudSyncAction::Diff(args) => cloud_sync_preview::diff(args),
        CloudSyncAction::State(command) => cloud_sync_state::run(command),
        CloudSyncAction::History(args) => history(args),
        CloudSyncAction::Backups(args) => backups(args),
    }
}

fn status(args: JsonArgs) -> CliResult<()> {
    let path = default_cloud_sync_path();
    // Loading the state store resets transient runtime fields in memory only.
    let store =
        CloudSyncStateStore::load(&path).map_err(|error| runtime_error(error, args.json))?;
    let response = status_response(path, store.state());
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format_status_text(&response));
            Ok(())
        }
    }
}

fn history(args: JsonArgs) -> CliResult<()> {
    let path = default_cloud_sync_path();
    let store =
        CloudSyncStateStore::load(&path).map_err(|error| runtime_error(error, args.json))?;
    let history = store.state().sync_history.clone();
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&CloudSyncHistoryResponse {
            path: path.display().to_string(),
            count: history.len(),
            history,
        }),
        OutputFormat::Text => {
            if history.is_empty() {
                output::write_text("No cloud sync history");
            } else {
                for entry in history {
                    output::write_text(format_history_row(&entry));
                }
            }
            Ok(())
        }
    }
}

fn backups(args: JsonArgs) -> CliResult<()> {
    let path = default_cloud_sync_path();
    let store =
        CloudSyncStateStore::load(&path).map_err(|error| runtime_error(error, args.json))?;
    let backups = store.state().rollback_backups.clone();
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&CloudSyncBackupsResponse {
            path: path.display().to_string(),
            count: backups.len(),
            backups,
        }),
        OutputFormat::Text => {
            if backups.is_empty() {
                output::write_text("No cloud sync rollback backups");
            } else {
                for backup in backups {
                    output::write_text(format_backup_row(&backup));
                }
            }
            Ok(())
        }
    }
}

fn status_response(path: PathBuf, state: &CloudSyncPersistedState) -> CloudSyncStatusResponse {
    CloudSyncStatusResponse {
        path: path.display().to_string(),
        status: serde_json::to_value(&state.status)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{:?}", state.status)),
        backend_type: serde_json::to_value(&state.settings.backend_type)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{:?}", state.settings.backend_type)),
        namespace: state.settings.namespace.clone(),
        auto_upload_enabled: state.settings.auto_upload_enabled,
        local_dirty: state.local_dirty,
        last_sync_at: state.last_sync_at.clone(),
        last_upload_at: state.last_upload_at.clone(),
        last_check_at: state.last_check_at.clone(),
        last_known_remote_revision: state.last_known_remote_revision.clone(),
        history_count: state.sync_history.len(),
        rollback_backup_count: state.rollback_backups.len(),
    }
}

fn format_status_text(response: &CloudSyncStatusResponse) -> String {
    format!(
        "status: {}\nbackend: {}\nnamespace: {}\nautoUpload: {}\nlocalDirty: {}\nlastSync: {}",
        response.status,
        response.backend_type,
        response.namespace,
        response.auto_upload_enabled,
        response.local_dirty,
        response.last_sync_at.as_deref().unwrap_or("-")
    )
}

fn format_history_row(entry: &CloudSyncHistoryEntry) -> String {
    let result = if entry.success { "ok" } else { "failed" };
    let revision = entry.remote_revision.as_deref().unwrap_or("-");
    let error = entry.error.as_deref().unwrap_or("-");
    format!(
        "{}\t{}\t{}\tconnections={}\tforwards={}\tplugins={}\trevision={}\terror={}",
        entry.timestamp,
        entry.action,
        result,
        entry.summary.connections,
        entry.summary.forwards,
        entry.summary.plugin_settings_count,
        revision,
        error
    )
}

fn format_backup_row(backup: &CloudSyncRollbackBackup) -> String {
    let revision = backup.source_revision.as_deref().unwrap_or("-");
    let summary = backup
        .metadata
        .as_ref()
        .map(|metadata| {
            format!(
                "connections={} forwards={} plugins={}",
                metadata.num_connections, metadata.forwards, metadata.plugin_settings_count
            )
        })
        .unwrap_or_else(|| "metadata=-".to_string());
    format!(
        "{}\t{}\tsize={} sourceRevision={} {}",
        backup.id, backup.created_at, backup.size_bytes, revision, summary
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_response_uses_serialized_status_names() {
        let response = status_response(
            PathBuf::from("cloud_sync.json"),
            &CloudSyncPersistedState::default(),
        );

        assert_eq!(response.status, "idle");
        assert_eq!(response.backend_type, "webdav");
    }

    #[test]
    fn formats_history_row_with_counts() {
        let entry = CloudSyncHistoryEntry {
            id: "history-1".to_string(),
            action: "upload".to_string(),
            timestamp: "2026-05-26T00:00:00Z".to_string(),
            success: true,
            summary: oxideterm_cloud_sync::state::CloudSyncHistorySummary {
                connections: 2,
                forwards: 1,
                has_app_settings: true,
                plugin_settings_count: 3,
            },
            error: None,
            remote_revision: Some("rev-1".to_string()),
        };

        let row = format_history_row(&entry);

        assert!(row.contains("upload"));
        assert!(row.contains("connections=2"));
        assert!(row.contains("revision=rev-1"));
    }
}
