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
    args::{BackupAction, BackupCommand, BackupInspectArgs, JsonArgs},
    cloud_sync_preview,
    error::{CliError, CliResult, runtime_error},
    output::{self, OutputFormat},
    paths::{self, default_backups_dir, default_cloud_sync_path, default_connections_path},
    settings,
};

const BACKUP_FORMAT: &str = "oxideterm-cli-backup-v1";
const BACKUP_FILE_PREFIX: &str = "oxideterm-backup-";
const BACKUP_FILE_EXTENSION: &str = "json";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupCreateResponse {
    path: String,
    size_bytes: u64,
    backup: BackupDocument,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupPreviewResponse {
    estimated_size_bytes: u64,
    summary: BackupSummary,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupSummary {
    format: &'static str,
    settings_section_count: usize,
    connection_record_count: usize,
    cloud_sync_history_count: usize,
    cloud_sync_local_dirty: bool,
    cloud_sync_remote_exists: bool,
}

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
struct BackupVerifyResponse {
    path: String,
    ok: bool,
    issue_count: usize,
    issues: Vec<BackupVerifyIssue>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupVerifyIssue {
    severity: &'static str,
    code: &'static str,
    message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupDocument {
    format: &'static str,
    created_at_ms: u64,
    source_paths: paths::CliPaths,
    settings: Value,
    connections: SavedConnectionsSyncSnapshot,
    cloud_sync: CloudSyncBackup,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncBackup {
    backend_type: String,
    auth_mode: String,
    namespace: String,
    auto_upload_enabled: bool,
    sync_scope: Value,
    local_dirty: bool,
    local_dirty_sections: Value,
    last_sync_at: Option<String>,
    last_upload_at: Option<String>,
    last_check_at: Option<String>,
    last_known_remote_revision: Option<String>,
    remote_exists: bool,
    history: Vec<CloudSyncHistoryEntry>,
    rollback_backup_count: usize,
    secret_hints: Value,
    last_error: Option<String>,
}

pub fn run(command: BackupCommand) -> CliResult<()> {
    match command.action {
        BackupAction::Preview(args) => preview_backup(args),
        BackupAction::Create(args) => create_backup(args),
        BackupAction::List(args) => list_backups(args),
        BackupAction::Inspect(args) => inspect_backup(args),
        BackupAction::Verify(args) => verify_backup(args),
    }
}

fn preview_backup(args: JsonArgs) -> CliResult<()> {
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

fn create_backup(args: JsonArgs) -> CliResult<()> {
    let backup = build_backup_document(args.json)?;
    let backup_dir = default_backups_dir();
    fs::create_dir_all(&backup_dir).map_err(|error| {
        CliError::new(
            "backup_dir_create_failed",
            format!(
                "failed to create backup dir {}: {error}",
                backup_dir.display()
            ),
            args.json,
        )
    })?;
    let path = backup_dir.join(backup_file_name(backup.created_at_ms));
    let bytes = serde_json::to_vec_pretty(&backup)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), args.json))?;
    fs::write(&path, &bytes).map_err(|error| {
        CliError::new(
            "backup_write_failed",
            format!("failed to write backup {}: {error}", path.display()),
            args.json,
        )
    })?;
    let response = BackupCreateResponse {
        path: path.display().to_string(),
        size_bytes: bytes.len() as u64,
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

fn list_backups(args: JsonArgs) -> CliResult<()> {
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

fn inspect_backup(args: BackupInspectArgs) -> CliResult<()> {
    let path = resolve_backup_query(&args.query);
    let contents = fs::read_to_string(&path).map_err(|error| {
        CliError::new(
            "backup_read_failed",
            format!("failed to read backup {}: {error}", path.display()),
            args.json,
        )
    })?;
    let backup = serde_json::from_str::<Value>(&contents).map_err(|error| {
        CliError::new(
            "backup_parse_failed",
            format!("failed to parse backup {}: {error}", path.display()),
            args.json,
        )
    })?;
    let response = BackupInspectResponse {
        path: path.display().to_string(),
        backup,
    };

    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format_backup_summary(&response.backup));
            Ok(())
        }
    }
}

fn verify_backup(args: BackupInspectArgs) -> CliResult<()> {
    let path = resolve_backup_query(&args.query);
    let backup = read_backup_value(&path, args.json)?;
    let issues = verify_backup_value(&backup);
    let response = BackupVerifyResponse {
        path: path.display().to_string(),
        ok: issues.iter().all(|issue| issue.severity != "error"),
        issue_count: issues.len(),
        issues,
    };

    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            if response.issues.is_empty() {
                output::write_text("Backup verification passed");
            } else {
                for issue in &response.issues {
                    output::write_text(format_verify_issue(issue));
                }
            }
            Ok(())
        }
    }
}

fn read_backup_value(path: &Path, json: bool) -> CliResult<Value> {
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

fn build_backup_document(json: bool) -> CliResult<BackupDocument> {
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
    })
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

fn resolve_backup_query(query: &str) -> PathBuf {
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

fn backup_file_name(created_at_ms: u64) -> String {
    format!("{BACKUP_FILE_PREFIX}{created_at_ms}.{BACKUP_FILE_EXTENSION}")
}

fn is_backup_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.starts_with(BACKUP_FILE_PREFIX)
                && path.extension().and_then(|ext| ext.to_str()) == Some(BACKUP_FILE_EXTENSION)
        })
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

fn format_backup_summary(backup: &Value) -> String {
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

fn verify_backup_value(backup: &Value) -> Vec<BackupVerifyIssue> {
    let mut issues = Vec::new();
    require_string(
        &mut issues,
        backup,
        "format",
        "missing_format",
        "backup format is required",
    );
    if backup.get("format").and_then(Value::as_str) != Some(BACKUP_FORMAT) {
        push_verify_issue(
            &mut issues,
            "error",
            "unsupported_format",
            "backup format is not supported",
        );
    }
    require_u64(
        &mut issues,
        backup,
        "createdAtMs",
        "missing_created_at",
        "backup creation timestamp is required",
    );
    verify_settings_snapshot(&mut issues, backup.get("settings"));
    verify_connections_snapshot(&mut issues, backup.get("connections"));
    verify_cloud_sync_backup(&mut issues, backup.get("cloudSync"));
    issues
}

fn verify_settings_snapshot(issues: &mut Vec<BackupVerifyIssue>, settings: Option<&Value>) {
    let Some(settings) = settings else {
        push_verify_issue(
            issues,
            "error",
            "missing_settings",
            "settings snapshot is missing",
        );
        return;
    };
    if settings.get("format").and_then(Value::as_str) != Some("oxide-settings-sections-v1") {
        push_verify_issue(
            issues,
            "error",
            "invalid_settings_format",
            "settings snapshot format is invalid",
        );
    }
    if settings
        .get("sectionIds")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
    {
        push_verify_issue(
            issues,
            "warning",
            "empty_settings_sections",
            "settings snapshot does not contain any section ids",
        );
    }
    if settings.get("settings").is_none() {
        push_verify_issue(
            issues,
            "error",
            "missing_settings_payload",
            "settings snapshot payload is missing",
        );
    }
}

fn verify_connections_snapshot(issues: &mut Vec<BackupVerifyIssue>, connections: Option<&Value>) {
    let Some(connections) = connections else {
        push_verify_issue(
            issues,
            "error",
            "missing_connections",
            "connections snapshot is missing",
        );
        return;
    };
    require_string(
        issues,
        connections,
        "revision",
        "missing_connections_revision",
        "connections snapshot revision is required",
    );
    if connections
        .get("records")
        .and_then(Value::as_array)
        .is_none()
    {
        push_verify_issue(
            issues,
            "error",
            "missing_connections_records",
            "connections snapshot records array is missing",
        );
    }
}

fn verify_cloud_sync_backup(issues: &mut Vec<BackupVerifyIssue>, cloud_sync: Option<&Value>) {
    let Some(cloud_sync) = cloud_sync else {
        push_verify_issue(
            issues,
            "error",
            "missing_cloud_sync",
            "cloud sync backup is missing",
        );
        return;
    };
    require_string(
        issues,
        cloud_sync,
        "backendType",
        "missing_cloud_sync_backend",
        "cloud sync backend type is required",
    );
    require_string(
        issues,
        cloud_sync,
        "namespace",
        "missing_cloud_sync_namespace",
        "cloud sync namespace is required",
    );
    if cloud_sync
        .get("history")
        .and_then(Value::as_array)
        .is_none()
    {
        push_verify_issue(
            issues,
            "error",
            "missing_cloud_sync_history",
            "cloud sync history array is missing",
        );
    }
}

fn require_string(
    issues: &mut Vec<BackupVerifyIssue>,
    value: &Value,
    field: &'static str,
    code: &'static str,
    message: &'static str,
) {
    if value
        .get(field)
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        push_verify_issue(issues, "error", code, message);
    }
}

fn require_u64(
    issues: &mut Vec<BackupVerifyIssue>,
    value: &Value,
    field: &'static str,
    code: &'static str,
    message: &'static str,
) {
    if value.get(field).and_then(Value::as_u64).is_none() {
        push_verify_issue(issues, "error", code, message);
    }
}

fn push_verify_issue(
    issues: &mut Vec<BackupVerifyIssue>,
    severity: &'static str,
    code: &'static str,
    message: impl Into<String>,
) {
    issues.push(BackupVerifyIssue {
        severity,
        code,
        message: message.into(),
    });
}

fn format_verify_issue(issue: &BackupVerifyIssue) -> String {
    format!("{}\t{}\t{}", issue.severity, issue.code, issue.message)
}

fn backup_summary_from_document(backup: &BackupDocument) -> BackupSummary {
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

    #[test]
    fn verify_reports_missing_backup_sections() {
        let issues = verify_backup_value(&serde_json::json!({
            "format": BACKUP_FORMAT,
            "createdAtMs": 1
        }));

        assert!(issues.iter().any(|issue| issue.code == "missing_settings"));
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "missing_connections")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "missing_cloud_sync")
        );
    }
}
