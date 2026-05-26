// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use oxideterm_cloud_sync::state::{CloudSyncHistoryEntry, CloudSyncPersistedState};
use oxideterm_connections::{ConnectionStore, SavedConnectionsSyncSnapshot};
use oxideterm_settings::export_oxide_settings_snapshot_json;
use serde::Serialize;
use serde_json::Value;

use crate::{
    cloud_sync_preview,
    error::{CliError, CliResult, runtime_error},
    paths::{self, default_backups_dir, default_cloud_sync_path, default_connections_path},
    settings,
};

pub(super) const BACKUP_FORMAT: &str = "oxideterm-cli-backup-v1";
const BACKUP_FILE_PREFIX: &str = "oxideterm-backup-";
const BACKUP_FILE_EXTENSION: &str = "json";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BackupDocument {
    pub(super) format: &'static str,
    pub(super) created_at_ms: u64,
    pub(super) source_paths: paths::CliPaths,
    pub(super) settings: Value,
    pub(super) connections: SavedConnectionsSyncSnapshot,
    pub(super) cloud_sync: CloudSyncBackup,
    pub(super) cloud_sync_state: CloudSyncPersistedState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BackupSummary {
    pub(super) format: &'static str,
    pub(super) settings_section_count: usize,
    pub(super) connection_record_count: usize,
    pub(super) cloud_sync_history_count: usize,
    pub(super) cloud_sync_local_dirty: bool,
    pub(super) cloud_sync_remote_exists: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CloudSyncBackup {
    backend_type: String,
    auth_mode: String,
    namespace: String,
    auto_upload_enabled: bool,
    sync_scope: Value,
    pub(super) local_dirty: bool,
    local_dirty_sections: Value,
    last_sync_at: Option<String>,
    last_upload_at: Option<String>,
    last_check_at: Option<String>,
    last_known_remote_revision: Option<String>,
    pub(super) remote_exists: bool,
    pub(super) history: Vec<CloudSyncHistoryEntry>,
    rollback_backup_count: usize,
    secret_hints: Value,
    last_error: Option<String>,
}

pub(super) fn build_backup_document(json: bool) -> CliResult<BackupDocument> {
    let created_at_ms = now_ms();
    let settings = settings::load_settings_read_only(json)?;
    let settings_snapshot_json =
        export_oxide_settings_snapshot_json(&settings.settings, None, false)
            .map_err(|error| CliError::new("settings_export_failed", error.to_string(), json))?;
    let settings_snapshot = serde_json::from_str::<Value>(&settings_snapshot_json)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;

    let connections_store = ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, json))?;
    let connections = connections_store
        .export_saved_connections_snapshot()
        .map_err(|error| runtime_error(error, json))?;

    let cloud_sync_path = default_cloud_sync_path();
    let cloud_sync_state = cloud_sync_preview::load_persisted_state(&cloud_sync_path, json)?;

    Ok(BackupDocument {
        format: BACKUP_FORMAT,
        created_at_ms,
        source_paths: paths::cli_paths(),
        settings: settings_snapshot,
        connections,
        cloud_sync: cloud_sync_backup(&cloud_sync_state, json)?,
        cloud_sync_state,
    })
}

pub(super) fn backup_summary_from_document(backup: &BackupDocument) -> BackupSummary {
    BackupSummary {
        format: backup.format,
        settings_section_count: backup
            .settings
            .get("sectionIds")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default(),
        connection_record_count: backup.connections.records.len(),
        cloud_sync_history_count: backup.cloud_sync.history.len(),
        cloud_sync_local_dirty: backup.cloud_sync.local_dirty,
        cloud_sync_remote_exists: backup.cloud_sync.remote_exists,
    }
}

pub(super) fn backup_file_name(created_at_ms: u64) -> String {
    format!("{BACKUP_FILE_PREFIX}{created_at_ms}.{BACKUP_FILE_EXTENSION}")
}

pub(super) fn is_backup_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.starts_with(BACKUP_FILE_PREFIX)
                && path.extension().and_then(|ext| ext.to_str()) == Some(BACKUP_FILE_EXTENSION)
        })
}

pub(super) fn resolve_backup_query(query: &str) -> PathBuf {
    let path = PathBuf::from(query);
    if path.is_absolute() || path.components().count() > 1 {
        return path;
    }
    let file_name = if path.extension().is_some() {
        query.to_string()
    } else {
        format!("{query}.{BACKUP_FILE_EXTENSION}")
    };
    default_backups_dir().join(file_name)
}

pub(super) fn read_backup_value(path: &Path, json: bool) -> CliResult<Value> {
    let contents = fs::read_to_string(path).map_err(|error| {
        CliError::new(
            "backup_read_failed",
            format!("failed to read backup {}: {error}", path.display()),
            json,
        )
    })?;
    serde_json::from_str::<Value>(&contents).map_err(|error| {
        CliError::new(
            "backup_parse_failed",
            format!("failed to parse backup {}: {error}", path.display()),
            json,
        )
    })
}

pub(super) fn format_backup_summary(backup: &Value) -> String {
    let created_at = backup
        .get("createdAtMs")
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string());
    let connection_count = backup
        .get("connections")
        .and_then(|connections| connections.get("records"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let settings_sections = backup
        .get("settings")
        .and_then(|settings| settings.get("sectionIds"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    format!(
        "format: {}\ncreatedAtMs: {}\nsettingsSections: {}\nconnections: {}",
        backup
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        created_at,
        settings_sections,
        connection_count
    )
}

fn cloud_sync_backup(state: &CloudSyncPersistedState, json: bool) -> CliResult<CloudSyncBackup> {
    // Keep the backup useful for diagnostics without expanding backend endpoints or secrets.
    Ok(CloudSyncBackup {
        backend_type: serde_json::to_value(&state.settings.backend_type)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{:?}", state.settings.backend_type)),
        auth_mode: serde_json::to_value(&state.settings.auth_mode)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{:?}", state.settings.auth_mode)),
        namespace: state.settings.namespace.clone(),
        auto_upload_enabled: state.settings.auto_upload_enabled,
        sync_scope: serde_json::to_value(&state.sync_scope)
            .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?,
        local_dirty: state.local_dirty,
        local_dirty_sections: serde_json::to_value(&state.local_dirty_sections)
            .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?,
        last_sync_at: state.last_sync_at.clone(),
        last_upload_at: state.last_upload_at.clone(),
        last_check_at: state.last_check_at.clone(),
        last_known_remote_revision: state.last_known_remote_revision.clone(),
        remote_exists: state.remote_exists,
        history: state.sync_history.clone(),
        rollback_backup_count: state.rollback_backups.len(),
        secret_hints: serde_json::to_value(&state.secret_hints)
            .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?,
        last_error: state.last_error.clone(),
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names_are_recognized_by_prefix_and_extension() {
        let path = PathBuf::from("oxideterm-backup-123.json");

        assert!(is_backup_file(&path));
        assert!(!is_backup_file(Path::new("other.json")));
    }

    #[test]
    fn inspect_query_resolves_plain_file_names_under_backup_dir() {
        let path = resolve_backup_query("oxideterm-backup-123");

        assert!(path.ends_with("oxideterm-backup-123.json"));
    }
}
