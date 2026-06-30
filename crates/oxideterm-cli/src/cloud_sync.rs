// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use oxideterm_cloud_sync::{
    AuthMode, BackendType, CloudSyncSettings, ConflictStrategy,
    state::{
        CloudSyncHistoryEntry, CloudSyncPersistedState, CloudSyncRollbackBackup,
        CloudSyncStateStore,
    },
};
use serde::Serialize;

use crate::{
    args::{
        CloudSyncAction, CloudSyncAuthModeArg, CloudSyncBackendAction, CloudSyncBackendArg,
        CloudSyncBackendConfigureAction, CloudSyncCommand, CloudSyncConfigureArgs,
        CloudSyncConflictStrategy, CloudSyncHistoryArgs, JsonArgs,
    },
    cloud_sync_preview, cloud_sync_secrets, cloud_sync_state, cloud_sync_write,
    error::{CliResult, runtime_error},
    output::{self, OutputFormat},
    paths::default_cloud_sync_path,
    write_guard::{self, WriteGuardPlan},
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
    failed_only: bool,
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncConfigureChange {
    field: &'static str,
    before: String,
    after: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncConfigureResponse {
    path: String,
    applied: bool,
    dry_run: bool,
    backup_path: Option<String>,
    backup_size_bytes: Option<u64>,
    changes: Vec<CloudSyncConfigureChange>,
    settings: CloudSyncSettings,
}

pub fn run(command: CloudSyncCommand) -> CliResult<()> {
    match command.action {
        CloudSyncAction::Status(args) => status(args),
        CloudSyncAction::Configure(args) => configure(args),
        CloudSyncAction::Preview(args) => cloud_sync_preview::preview(args),
        CloudSyncAction::Diff(args) => cloud_sync_preview::diff(args),
        CloudSyncAction::Push(args) => cloud_sync_write::push(args),
        CloudSyncAction::Pull(args) => cloud_sync_write::pull(args),
        CloudSyncAction::Apply(args) => cloud_sync_write::apply(args),
        CloudSyncAction::Resolve(args) => cloud_sync_write::resolve(args),
        CloudSyncAction::State(command) => cloud_sync_state::run(command),
        CloudSyncAction::History(args) => history(args),
        CloudSyncAction::Backups(args) => backups(args),
        CloudSyncAction::Secrets(command) => cloud_sync_secrets::run(command),
        CloudSyncAction::Backend(command) => match command.action {
            CloudSyncBackendAction::Webdav(command) => match command.action {
                CloudSyncBackendConfigureAction::Configure(args) => {
                    configure_backend(CloudSyncBackendArg::Webdav, args)
                }
            },
            CloudSyncBackendAction::OneDrive(command) => match command.action {
                CloudSyncBackendConfigureAction::Configure(args) => {
                    configure_backend(CloudSyncBackendArg::OneDrive, args)
                }
            },
            CloudSyncBackendAction::GoogleDrive(command) => match command.action {
                CloudSyncBackendConfigureAction::Configure(args) => {
                    configure_backend(CloudSyncBackendArg::GoogleDrive, args)
                }
            },
            CloudSyncBackendAction::GithubGist(command) => match command.action {
                CloudSyncBackendConfigureAction::Configure(args) => {
                    configure_backend(CloudSyncBackendArg::GithubGist, args)
                }
            },
            CloudSyncBackendAction::S3(command) => match command.action {
                CloudSyncBackendConfigureAction::Configure(args) => {
                    configure_backend(CloudSyncBackendArg::S3, args)
                }
            },
            CloudSyncBackendAction::Git(command) => match command.action {
                CloudSyncBackendConfigureAction::Configure(args) => {
                    configure_backend(CloudSyncBackendArg::Git, args)
                }
            },
        },
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

fn configure(args: CloudSyncConfigureArgs) -> CliResult<()> {
    let path = default_cloud_sync_path();
    let mut store =
        CloudSyncStateStore::load(&path).map_err(|error| runtime_error(error, args.write.json))?;
    let before = store.state().settings.clone();
    let mut after = before.clone();
    apply_configure_args(&mut after, &args);
    let changes = cloud_sync_configure_changes(&before, &after);
    let mut guard = write_guard::prepare_write(&args.write, !changes.is_empty())?;
    if !args.write.dry_run && !changes.is_empty() {
        store.state_mut().settings = after.clone();
        store
            .save()
            .map_err(|error| runtime_error(error, args.write.json))?;
        write_guard::mark_applied(&mut guard);
    }
    let response = cloud_sync_configure_response(path, guard, changes, after);
    let ok = response.applied || response.dry_run || response.changes.is_empty();
    match output::format_from_flag(args.write.json) {
        OutputFormat::Json => output::write_json_with_ok(&response, ok),
        OutputFormat::Text => {
            output::write_text(format_configure_text(&response));
            Ok(())
        }
    }
}

fn configure_backend(
    backend: CloudSyncBackendArg,
    mut args: CloudSyncConfigureArgs,
) -> CliResult<()> {
    args.backend = Some(backend);
    validate_backend_configure_args(backend, &args)?;
    configure(args)
}

fn validate_backend_configure_args(
    backend: CloudSyncBackendArg,
    args: &CloudSyncConfigureArgs,
) -> CliResult<()> {
    match backend {
        CloudSyncBackendArg::Webdav => require_non_empty(
            args.endpoint.as_deref(),
            "--endpoint is required for WebDAV backend configuration",
            args.write.json,
        ),
        CloudSyncBackendArg::S3 => require_non_empty(
            args.s3_bucket.as_deref(),
            "--s3-bucket is required for S3 backend configuration",
            args.write.json,
        ),
        CloudSyncBackendArg::Git => require_non_empty(
            args.git_repository.as_deref(),
            "--git-repository is required for Git backend configuration",
            args.write.json,
        ),
        CloudSyncBackendArg::HttpJson
        | CloudSyncBackendArg::Dropbox
        | CloudSyncBackendArg::OneDrive
        | CloudSyncBackendArg::GoogleDrive
        | CloudSyncBackendArg::GithubGist => Ok(()),
    }
}

fn require_non_empty(value: Option<&str>, message: &str, json: bool) -> CliResult<()> {
    if value.is_some_and(|value| !value.trim().is_empty()) {
        Ok(())
    } else {
        Err(crate::error::CliError::new(
            "cloud_sync_backend_config_invalid",
            message,
            json,
        ))
    }
}

fn history(args: CloudSyncHistoryArgs) -> CliResult<()> {
    let path = default_cloud_sync_path();
    let store =
        CloudSyncStateStore::load(&path).map_err(|error| runtime_error(error, args.json))?;
    let mut history = store.state().sync_history.clone();
    if args.failed_only {
        history.retain(|entry| !entry.success);
    }
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&CloudSyncHistoryResponse {
            path: path.display().to_string(),
            failed_only: args.failed_only,
            count: history.len(),
            history,
        }),
        OutputFormat::Text => {
            if history.is_empty() {
                if args.failed_only {
                    output::write_text("No failed cloud sync history");
                } else {
                    output::write_text("No cloud sync history");
                }
            } else {
                for entry in history {
                    output::write_text(format_history_row(&entry));
                }
            }
            Ok(())
        }
    }
}

fn apply_configure_args(settings: &mut CloudSyncSettings, args: &CloudSyncConfigureArgs) {
    if let Some(backend) = args.backend {
        settings.backend_type = match backend {
            CloudSyncBackendArg::Webdav => BackendType::Webdav,
            CloudSyncBackendArg::HttpJson => BackendType::HttpJson,
            CloudSyncBackendArg::Dropbox => BackendType::Dropbox,
            CloudSyncBackendArg::OneDrive => BackendType::OneDrive,
            CloudSyncBackendArg::GoogleDrive => BackendType::GoogleDrive,
            CloudSyncBackendArg::GithubGist => BackendType::GithubGist,
            CloudSyncBackendArg::S3 => BackendType::S3,
            CloudSyncBackendArg::Git => BackendType::Git,
        };
    }
    if let Some(auth_mode) = args.auth_mode {
        settings.auth_mode = match auth_mode {
            CloudSyncAuthModeArg::Bearer => AuthMode::Bearer,
            CloudSyncAuthModeArg::Basic => AuthMode::Basic,
            CloudSyncAuthModeArg::None => AuthMode::None,
        };
    }
    assign_optional_string(&mut settings.endpoint, &args.endpoint);
    assign_optional_string(&mut settings.namespace, &args.namespace);
    assign_optional_string(&mut settings.s3_bucket, &args.s3_bucket);
    assign_optional_string(&mut settings.s3_region, &args.s3_region);
    assign_optional_string(&mut settings.git_repository, &args.git_repository);
    assign_optional_string(&mut settings.git_branch, &args.git_branch);
    assign_optional_string(
        &mut settings.github_oauth_client_id,
        &args.github_oauth_client_id,
    );
    assign_optional_string(
        &mut settings.microsoft_oauth_client_id,
        &args.microsoft_oauth_client_id,
    );
    assign_optional_string(
        &mut settings.google_oauth_client_id,
        &args.google_oauth_client_id,
    );
    if let Some(enabled) = args.auto_upload_enabled {
        settings.auto_upload_enabled = enabled;
    }
    if let Some(interval) = args.auto_upload_interval_mins {
        settings.auto_upload_interval_mins = interval;
    }
    if let Some(strategy) = args.default_conflict_strategy {
        settings.default_conflict_strategy = conflict_strategy_arg(strategy);
    }
}

fn assign_optional_string(target: &mut String, value: &Option<String>) {
    if let Some(value) = value {
        *target = value.clone();
    }
}

fn conflict_strategy_arg(strategy: CloudSyncConflictStrategy) -> ConflictStrategy {
    match strategy {
        CloudSyncConflictStrategy::Merge => ConflictStrategy::Merge,
        CloudSyncConflictStrategy::Replace => ConflictStrategy::Replace,
        CloudSyncConflictStrategy::Skip => ConflictStrategy::Skip,
        CloudSyncConflictStrategy::Rename => ConflictStrategy::Rename,
    }
}

fn cloud_sync_configure_response(
    path: PathBuf,
    guard: WriteGuardPlan,
    changes: Vec<CloudSyncConfigureChange>,
    settings: CloudSyncSettings,
) -> CloudSyncConfigureResponse {
    CloudSyncConfigureResponse {
        path: path.display().to_string(),
        applied: guard.applied,
        dry_run: guard.dry_run,
        backup_path: guard.backup_path,
        backup_size_bytes: guard.backup_size_bytes,
        changes,
        settings,
    }
}

fn cloud_sync_configure_changes(
    before: &CloudSyncSettings,
    after: &CloudSyncSettings,
) -> Vec<CloudSyncConfigureChange> {
    let mut changes = Vec::new();
    push_configure_change(
        &mut changes,
        "backendType",
        serialized_field(&before.backend_type),
        serialized_field(&after.backend_type),
    );
    push_configure_change(
        &mut changes,
        "authMode",
        serialized_field(&before.auth_mode),
        serialized_field(&after.auth_mode),
    );
    push_configure_change(
        &mut changes,
        "endpoint",
        before.endpoint.clone(),
        after.endpoint.clone(),
    );
    push_configure_change(
        &mut changes,
        "namespace",
        before.namespace.clone(),
        after.namespace.clone(),
    );
    push_configure_change(
        &mut changes,
        "s3Bucket",
        before.s3_bucket.clone(),
        after.s3_bucket.clone(),
    );
    push_configure_change(
        &mut changes,
        "s3Region",
        before.s3_region.clone(),
        after.s3_region.clone(),
    );
    push_configure_change(
        &mut changes,
        "gitRepository",
        before.git_repository.clone(),
        after.git_repository.clone(),
    );
    push_configure_change(
        &mut changes,
        "gitBranch",
        before.git_branch.clone(),
        after.git_branch.clone(),
    );
    push_configure_change(
        &mut changes,
        "githubOauthClientId",
        before.github_oauth_client_id.clone(),
        after.github_oauth_client_id.clone(),
    );
    push_configure_change(
        &mut changes,
        "microsoftOauthClientId",
        before.microsoft_oauth_client_id.clone(),
        after.microsoft_oauth_client_id.clone(),
    );
    push_configure_change(
        &mut changes,
        "googleOauthClientId",
        before.google_oauth_client_id.clone(),
        after.google_oauth_client_id.clone(),
    );
    push_configure_change(
        &mut changes,
        "autoUploadEnabled",
        before.auto_upload_enabled.to_string(),
        after.auto_upload_enabled.to_string(),
    );
    push_configure_change(
        &mut changes,
        "autoUploadIntervalMins",
        before.auto_upload_interval_mins.to_string(),
        after.auto_upload_interval_mins.to_string(),
    );
    push_configure_change(
        &mut changes,
        "defaultConflictStrategy",
        serialized_field(&before.default_conflict_strategy),
        serialized_field(&after.default_conflict_strategy),
    );
    changes
}

fn push_configure_change(
    changes: &mut Vec<CloudSyncConfigureChange>,
    field: &'static str,
    before: String,
    after: String,
) {
    if before != after {
        changes.push(CloudSyncConfigureChange {
            field,
            before,
            after,
        });
    }
}

fn serialized_field<T: Serialize + std::fmt::Debug>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{value:?}"))
}

fn format_configure_text(response: &CloudSyncConfigureResponse) -> String {
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
            change.field, change.before, change.after
        ));
    }
    lines.join("\n")
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
                quick_commands: 0,
                serial_profiles: 0,
                raw_tcp_profiles: 0,
                sensitive_credentials: 0,
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

    #[test]
    fn history_filter_keeps_failed_entries() {
        let failed = CloudSyncHistoryEntry {
            id: "failed".to_string(),
            action: "upload".to_string(),
            timestamp: "2026-05-26T00:00:00Z".to_string(),
            success: false,
            summary: oxideterm_cloud_sync::state::CloudSyncHistorySummary::default(),
            error: Some("unauthorized".to_string()),
            remote_revision: None,
        };
        let ok = CloudSyncHistoryEntry {
            success: true,
            id: "ok".to_string(),
            ..failed.clone()
        };
        let mut history = vec![failed, ok];

        history.retain(|entry| !entry.success);

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, "failed");
    }
}
