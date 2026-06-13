// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, fs};

use serde::{Deserialize, Serialize};

use crate::{
    args::{
        BatchAction, BatchApplyArgs, BatchCommand, CloudSyncAction, CloudSyncAuthModeArg,
        CloudSyncBackendArg, CloudSyncCommand, CloudSyncConfigureArgs, CloudSyncConflictStrategy,
        ConnectionsApplyStrategy, WriteArgs,
    },
    cloud_sync, connections,
    error::{CliError, CliResult},
    output::{self, OutputFormat},
    settings,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchPlan {
    settings: Option<BatchSettingsApply>,
    connections: Option<BatchConnectionsApply>,
    cloud_sync: Option<BatchCloudSync>,
}

#[derive(Debug, Deserialize)]
struct BatchSettingsApply {
    path: String,
    #[serde(default)]
    sections: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BatchConnectionsApply {
    path: String,
    #[serde(default = "default_connections_strategy")]
    strategy: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchCloudSync {
    configure: Option<BatchCloudSyncConfigure>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchCloudSyncConfigure {
    backend: Option<String>,
    auth_mode: Option<String>,
    endpoint: Option<String>,
    namespace: Option<String>,
    s3_bucket: Option<String>,
    s3_region: Option<String>,
    git_repository: Option<String>,
    git_branch: Option<String>,
    github_oauth_client_id: Option<String>,
    microsoft_oauth_client_id: Option<String>,
    google_oauth_client_id: Option<String>,
    auto_upload_enabled: Option<bool>,
    auto_upload_interval_mins: Option<f64>,
    default_conflict_strategy: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchApplyResponse {
    path: String,
    applied: bool,
    dry_run: bool,
    steps: Vec<&'static str>,
}

pub(crate) fn run(command: BatchCommand) -> CliResult<i32> {
    match command.action {
        BatchAction::Apply(args) => apply(args),
    }
}

fn apply(args: BatchApplyArgs) -> CliResult<i32> {
    let text = fs::read_to_string(&args.path).map_err(|error| {
        CliError::new(
            "batch_read_failed",
            format!("failed to read batch plan {}: {error}", args.path),
            args.write.json,
        )
    })?;
    let plan = serde_json::from_str::<BatchPlan>(&text)
        .map_err(|error| CliError::new("batch_parse_failed", error.to_string(), args.write.json))?;

    let mut steps = Vec::new();
    let write = args.write.clone();
    if let Some(settings_plan) = plan.settings {
        steps.push("settings");
        let sections = selected_sections(&settings_plan.sections);
        settings::apply_settings_snapshot(settings_plan.path, sections, write.clone())?;
    }
    if let Some(connections_plan) = plan.connections {
        steps.push("connections");
        connections::apply_connections_snapshot(
            connections_plan.path,
            parse_connections_strategy(&connections_plan.strategy, write.json)?,
            write.clone(),
        )?;
    }
    if let Some(cloud_sync_plan) = plan.cloud_sync {
        if let Some(configure) = cloud_sync_plan.configure {
            steps.push("cloudSync.configure");
            cloud_sync::run(CloudSyncCommand {
                action: CloudSyncAction::Configure(cloud_sync_configure_args(
                    configure,
                    write.clone(),
                )?),
            })?;
        }
    }

    let response = BatchApplyResponse {
        path: args.path,
        applied: write.yes && !write.dry_run,
        dry_run: write.dry_run || !write.yes,
        steps,
    };
    match output::format_from_flag(write.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format!(
                "applied: {} dryRun={} steps={}",
                response.applied,
                response.dry_run,
                response.steps.join(",")
            ));
            Ok(())
        }
    }?;
    Ok(0)
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

fn parse_connections_strategy(value: &str, json: bool) -> CliResult<ConnectionsApplyStrategy> {
    match value {
        "skip" => Ok(ConnectionsApplyStrategy::Skip),
        "replace" => Ok(ConnectionsApplyStrategy::Replace),
        "merge" => Ok(ConnectionsApplyStrategy::Merge),
        _ => Err(CliError::new(
            "batch_invalid_strategy",
            format!("unsupported connections strategy: {value}"),
            json,
        )),
    }
}

fn cloud_sync_configure_args(
    configure: BatchCloudSyncConfigure,
    write: WriteArgs,
) -> CliResult<CloudSyncConfigureArgs> {
    let json = write.json;
    Ok(CloudSyncConfigureArgs {
        backend: configure
            .backend
            .as_deref()
            .map(|value| parse_cloud_sync_backend(value, json))
            .transpose()?,
        auth_mode: configure
            .auth_mode
            .as_deref()
            .map(|value| parse_cloud_sync_auth_mode(value, json))
            .transpose()?,
        endpoint: configure.endpoint,
        namespace: configure.namespace,
        s3_bucket: configure.s3_bucket,
        s3_region: configure.s3_region,
        git_repository: configure.git_repository,
        git_branch: configure.git_branch,
        github_oauth_client_id: configure.github_oauth_client_id,
        microsoft_oauth_client_id: configure.microsoft_oauth_client_id,
        google_oauth_client_id: configure.google_oauth_client_id,
        auto_upload_enabled: configure.auto_upload_enabled,
        auto_upload_interval_mins: configure.auto_upload_interval_mins,
        default_conflict_strategy: configure
            .default_conflict_strategy
            .as_deref()
            .map(|value| parse_cloud_sync_conflict_strategy(value, json))
            .transpose()?,
        write,
    })
}

fn parse_cloud_sync_backend(value: &str, json: bool) -> CliResult<CloudSyncBackendArg> {
    match value {
        "webdav" => Ok(CloudSyncBackendArg::Webdav),
        "http-json" => Ok(CloudSyncBackendArg::HttpJson),
        "dropbox" => Ok(CloudSyncBackendArg::Dropbox),
        "onedrive" => Ok(CloudSyncBackendArg::OneDrive),
        "google-drive" | "googledrive" => Ok(CloudSyncBackendArg::GoogleDrive),
        "github-gist" => Ok(CloudSyncBackendArg::GithubGist),
        "s3" => Ok(CloudSyncBackendArg::S3),
        "git" => Ok(CloudSyncBackendArg::Git),
        _ => Err(invalid_cloud_sync_value("backend", value, json)),
    }
}

fn parse_cloud_sync_auth_mode(value: &str, json: bool) -> CliResult<CloudSyncAuthModeArg> {
    match value {
        "bearer" => Ok(CloudSyncAuthModeArg::Bearer),
        "basic" => Ok(CloudSyncAuthModeArg::Basic),
        "none" => Ok(CloudSyncAuthModeArg::None),
        _ => Err(invalid_cloud_sync_value("authMode", value, json)),
    }
}

fn parse_cloud_sync_conflict_strategy(
    value: &str,
    json: bool,
) -> CliResult<CloudSyncConflictStrategy> {
    match value {
        "merge" => Ok(CloudSyncConflictStrategy::Merge),
        "replace" => Ok(CloudSyncConflictStrategy::Replace),
        "skip" => Ok(CloudSyncConflictStrategy::Skip),
        "rename" => Ok(CloudSyncConflictStrategy::Rename),
        _ => Err(invalid_cloud_sync_value(
            "defaultConflictStrategy",
            value,
            json,
        )),
    }
}

fn invalid_cloud_sync_value(field: &str, value: &str, json: bool) -> CliError {
    CliError::new(
        "batch_invalid_cloud_sync_value",
        format!("unsupported cloudSync.configure {field}: {value}"),
        json,
    )
}

fn default_connections_strategy() -> String {
    "skip".to_string()
}
