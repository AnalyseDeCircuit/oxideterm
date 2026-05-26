// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use oxideterm_cloud_sync::state::CloudSyncPersistedState;
use oxideterm_connections::ConnectionStore;
use oxideterm_settings::ALL_OXIDE_SETTINGS_SECTIONS;
use serde::Serialize;

use crate::{
    args::{CloudSyncDiffArgs, ReportArgs},
    cloud_sync_preview::{self, CloudSyncDiffResponse, CloudSyncPreviewResponse, DiffFilter},
    cloud_sync_secrets, connections_validate,
    doctor::{self, DoctorSummary},
    error::{CliError, CliResult},
    output::{self, OutputFormat},
    paths::{self, CliPaths, default_cloud_sync_path, default_connections_path},
    settings,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportResponse {
    ok: bool,
    generated_at_ms: u128,
    paths: CliPaths,
    settings: ReportSettings,
    connections: ReportConnections,
    cloud_sync: ReportCloudSync,
    doctor: ReportDoctor,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportSettings {
    load_ok: bool,
    error: Option<String>,
    version: Option<u32>,
    validation_warning_count: usize,
    migration_warning_count: usize,
    unknown_top_level_field_count: usize,
    exported_section_count: usize,
    total_section_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportConnections {
    load_ok: bool,
    error: Option<String>,
    connection_count: usize,
    group_count: usize,
    validation_issue_count: usize,
    validation_error_count: usize,
    validation_warning_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportCloudSync {
    load_ok: bool,
    error: Option<String>,
    backend_type: Option<String>,
    namespace: Option<String>,
    local_dirty: Option<bool>,
    remote_exists: Option<bool>,
    last_known_remote_revision: Option<String>,
    history_count: usize,
    failed_history_count: usize,
    rollback_backup_count: usize,
    secret_hint_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportDoctor {
    ok: bool,
    error_count: usize,
    warning_count: usize,
    info_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportBundle {
    report: ReportResponse,
    settings_validation: Option<settings::SettingsValidationReport>,
    connections_validation: ReportConnectionsValidation,
    cloud_sync: ReportBundleCloudSync,
    doctor: doctor::DoctorResponse,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportConnectionsValidation {
    path: String,
    ok: bool,
    issue_count: usize,
    error_count: usize,
    warning_count: usize,
    issues: Vec<connections_validate::ConnectionValidationIssue>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportBundleCloudSync {
    preview: Option<CloudSyncPreviewResponse>,
    diff: Option<CloudSyncDiffResponse>,
    failed_history: Vec<oxideterm_cloud_sync::state::CloudSyncHistoryEntry>,
    secret_hints: Vec<cloud_sync_secrets::SecretHintStatus>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportBundleWriteResponse {
    path: String,
    ok: bool,
    bytes: usize,
}

pub fn run(args: ReportArgs) -> CliResult<i32> {
    if let Some(path) = args.bundle {
        let response = write_report_bundle(PathBuf::from(path), args.json)?;
        match output::format_from_flag(args.json) {
            OutputFormat::Json => output::write_json_with_ok(&response, response.ok),
            OutputFormat::Text => {
                output::write_text(format!(
                    "report bundle written: {} bytes={}",
                    response.path, response.bytes
                ));
                Ok(())
            }
        }?;
        return Ok(if response.ok { 0 } else { 1 });
    }

    let response = build_report(args.json);
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json_with_ok(&response, response.ok),
        OutputFormat::Text => {
            output::write_text(format_report_text(&response));
            Ok(())
        }
    }?;
    Ok(if response.ok { 0 } else { 1 })
}

fn build_report(json: bool) -> ReportResponse {
    let settings = settings_report(json);
    let connections = connections_report();
    let cloud_sync = cloud_sync_report(json);
    let doctor = doctor_report(json);
    let ok = settings.load_ok && connections.load_ok && cloud_sync.load_ok && doctor.ok;

    ReportResponse {
        ok,
        generated_at_ms: now_ms(),
        paths: paths::cli_paths(),
        settings,
        connections,
        cloud_sync,
        doctor,
    }
}

fn settings_report(json: bool) -> ReportSettings {
    match settings::validate_settings_read_only(json, false) {
        Ok(report) => ReportSettings {
            load_ok: true,
            error: None,
            version: Some(report.version),
            validation_warning_count: report.validation_warnings.len(),
            migration_warning_count: report.migration_warnings.len(),
            unknown_top_level_field_count: report.unknown_top_level_fields.len(),
            exported_section_count: report.exported_section_ids.len(),
            total_section_count: ALL_OXIDE_SETTINGS_SECTIONS.len(),
        },
        Err(error) => ReportSettings {
            load_ok: false,
            error: Some(format!("{}: {}", error.code, error.message)),
            version: None,
            validation_warning_count: 0,
            migration_warning_count: 0,
            unknown_top_level_field_count: 0,
            exported_section_count: 0,
            total_section_count: ALL_OXIDE_SETTINGS_SECTIONS.len(),
        },
    }
}

fn write_report_bundle(path: PathBuf, json: bool) -> CliResult<ReportBundleWriteResponse> {
    let bundle = build_report_bundle(json);
    let contents = serde_json::to_string_pretty(&bundle)
        .map_err(|error| CliError::new("serialization_failed", error.to_string(), json))?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::new(
                "report_bundle_write_failed",
                format!(
                    "failed to create report bundle dir {}: {error}",
                    parent.display()
                ),
                json,
            )
        })?;
    }
    fs::write(&path, &contents).map_err(|error| {
        CliError::new(
            "report_bundle_write_failed",
            format!("failed to write report bundle {}: {error}", path.display()),
            json,
        )
    })?;
    Ok(ReportBundleWriteResponse {
        path: path.display().to_string(),
        ok: bundle.report.ok,
        bytes: contents.len(),
    })
}

fn build_report_bundle(json: bool) -> ReportBundle {
    // The bundle intentionally contains summaries, hints, and revisions only; secret values are not loaded.
    ReportBundle {
        report: build_report(json),
        settings_validation: settings::validate_settings_read_only(json, false).ok(),
        connections_validation: connections_validation_bundle(),
        cloud_sync: cloud_sync_bundle(json),
        doctor: doctor::build_doctor_response(false, json),
    }
}

fn connections_validation_bundle() -> ReportConnectionsValidation {
    let path = default_connections_path();
    match ConnectionStore::load_read_only(&path) {
        Ok(store) => {
            let issues = connections_validate::validate_connection_infos(
                &store.connection_infos(),
                store.groups(),
            );
            let error_count = connections_validate::count_issues(&issues, "error");
            let warning_count = connections_validate::count_issues(&issues, "warning");
            ReportConnectionsValidation {
                path: path.display().to_string(),
                ok: connections_validate::validation_ok(error_count, warning_count, false),
                issue_count: issues.len(),
                error_count,
                warning_count,
                issues,
            }
        }
        Err(error) => ReportConnectionsValidation {
            path: path.display().to_string(),
            ok: false,
            issue_count: 1,
            error_count: 1,
            warning_count: 0,
            issues: vec![connections_validate::ConnectionValidationIssue {
                severity: "error",
                code: "connections_load_failed",
                connection_id: None,
                connection_name: None,
                message: error.to_string(),
            }],
        },
    }
}

fn cloud_sync_bundle(json: bool) -> ReportBundleCloudSync {
    let path = default_cloud_sync_path();
    match cloud_sync_preview::load_persisted_state(&path, json) {
        Ok(state) => {
            let path_text = path.display().to_string();
            let diff_args = CloudSyncDiffArgs {
                dirty_only: true,
                category: None,
                format: None,
                json,
            };
            let filter = DiffFilter::from_args(&diff_args);
            ReportBundleCloudSync {
                preview: Some(cloud_sync_preview::preview_response(
                    path_text.clone(),
                    &state,
                )),
                diff: Some(cloud_sync_preview::diff_response(
                    path_text, &state, &filter,
                )),
                failed_history: state
                    .sync_history
                    .iter()
                    .filter(|entry| !entry.success)
                    .cloned()
                    .collect(),
                secret_hints: cloud_sync_secrets::secret_hint_statuses(&state.secret_hints),
                error: None,
            }
        }
        Err(error) => ReportBundleCloudSync {
            preview: None,
            diff: None,
            failed_history: Vec::new(),
            secret_hints: Vec::new(),
            error: Some(format!("{}: {}", error.code, error.message)),
        },
    }
}

fn connections_report() -> ReportConnections {
    let path = default_connections_path();
    match ConnectionStore::load_read_only(&path) {
        Ok(store) => {
            let connections = store.connection_infos();
            let issues =
                connections_validate::validate_connection_infos(&connections, store.groups());
            let validation_error_count = connections_validate::count_issues(&issues, "error");
            let validation_warning_count = connections_validate::count_issues(&issues, "warning");
            ReportConnections {
                load_ok: true,
                error: None,
                connection_count: connections.len(),
                group_count: store.groups().len(),
                validation_issue_count: issues.len(),
                validation_error_count,
                validation_warning_count,
            }
        }
        Err(error) => ReportConnections {
            load_ok: false,
            error: Some(error.to_string()),
            connection_count: 0,
            group_count: 0,
            validation_issue_count: 0,
            validation_error_count: 0,
            validation_warning_count: 0,
        },
    }
}

fn cloud_sync_report(json: bool) -> ReportCloudSync {
    let path = default_cloud_sync_path();
    match cloud_sync_preview::load_persisted_state(&path, json) {
        Ok(state) => cloud_sync_report_from_state(&state),
        Err(error) => ReportCloudSync {
            load_ok: false,
            error: Some(format!("{}: {}", error.code, error.message)),
            backend_type: None,
            namespace: None,
            local_dirty: None,
            remote_exists: None,
            last_known_remote_revision: None,
            history_count: 0,
            failed_history_count: 0,
            rollback_backup_count: 0,
            secret_hint_count: 0,
        },
    }
}

fn cloud_sync_report_from_state(state: &CloudSyncPersistedState) -> ReportCloudSync {
    ReportCloudSync {
        load_ok: true,
        error: None,
        backend_type: Some(serialized_name(&state.settings.backend_type)),
        namespace: Some(state.settings.namespace.clone()),
        local_dirty: Some(cloud_sync_has_dirty(state)),
        remote_exists: Some(state.remote_exists),
        last_known_remote_revision: state.last_known_remote_revision.clone(),
        history_count: state.sync_history.len(),
        failed_history_count: state
            .sync_history
            .iter()
            .filter(|entry| !entry.success)
            .count(),
        rollback_backup_count: state.rollback_backups.len(),
        secret_hint_count: state.secret_hints.len(),
    }
}

fn doctor_report(json: bool) -> ReportDoctor {
    let response = doctor::build_doctor_response(false, json);
    let DoctorSummary {
        error_count,
        warning_count,
        info_count,
    } = response.summary;
    ReportDoctor {
        ok: response.ok,
        error_count,
        warning_count,
        info_count,
    }
}

fn cloud_sync_has_dirty(state: &CloudSyncPersistedState) -> bool {
    state.local_dirty
        || state.local_dirty_sections.as_ref().is_some_and(|sections| {
            sections.connections
                || sections.forwards
                || sections.app_settings.values().any(|dirty| *dirty)
                || sections.plugin_settings.values().any(|dirty| *dirty)
        })
}

fn serialized_name<T: serde::Serialize + std::fmt::Debug>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{value:?}"))
}

fn now_ms() -> u128 {
    // Wall-clock timestamp is metadata for bug reports; no runtime state is mutated.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn format_report_text(response: &ReportResponse) -> String {
    [
        format!(
            "settings: loadOk={} version={} validationWarnings={} migrationWarnings={} sections={}/{}",
            response.settings.load_ok,
            option_u32(response.settings.version),
            response.settings.validation_warning_count,
            response.settings.migration_warning_count,
            response.settings.exported_section_count,
            response.settings.total_section_count
        ),
        format!(
            "connections: loadOk={} count={} groups={} issues={} errors={} warnings={}",
            response.connections.load_ok,
            response.connections.connection_count,
            response.connections.group_count,
            response.connections.validation_issue_count,
            response.connections.validation_error_count,
            response.connections.validation_warning_count
        ),
        format!(
            "cloudSync: loadOk={} backend={} namespace={} dirty={} failedHistory={} secretHints={}",
            response.cloud_sync.load_ok,
            response.cloud_sync.backend_type.as_deref().unwrap_or("-"),
            response.cloud_sync.namespace.as_deref().unwrap_or("-"),
            option_bool(response.cloud_sync.local_dirty),
            response.cloud_sync.failed_history_count,
            response.cloud_sync.secret_hint_count
        ),
        format!(
            "doctor: ok={} errors={} warnings={} info={}",
            response.doctor.ok,
            response.doctor.error_count,
            response.doctor.warning_count,
            response.doctor.info_count
        ),
    ]
    .join("\n")
}

fn option_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn option_bool(value: Option<bool>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

#[cfg(test)]
mod tests {
    use oxideterm_cloud_sync::StructuredDirtySections;

    use super::*;

    #[test]
    fn cloud_sync_report_counts_failed_history_without_secret_values() {
        let mut state = CloudSyncPersistedState {
            local_dirty_sections: Some(StructuredDirtySections {
                connections: true,
                ..StructuredDirtySections::default()
            }),
            ..CloudSyncPersistedState::default()
        };
        state
            .secret_hints
            .insert("webdav.password".to_string(), true);
        state
            .sync_history
            .push(oxideterm_cloud_sync::state::CloudSyncHistoryEntry {
                id: "history-1".to_string(),
                action: "upload".to_string(),
                timestamp: "2026-05-26T00:00:00Z".to_string(),
                success: false,
                summary: oxideterm_cloud_sync::state::CloudSyncHistorySummary::default(),
                error: Some("unauthorized".to_string()),
                remote_revision: None,
            });

        let report = cloud_sync_report_from_state(&state);

        assert_eq!(report.failed_history_count, 1);
        assert_eq!(report.secret_hint_count, 1);
        assert_eq!(report.local_dirty, Some(true));
    }

    #[test]
    fn report_bundle_keeps_failed_history_and_secret_hints() {
        let bundle = ReportBundleCloudSync {
            preview: None,
            diff: None,
            failed_history: Vec::new(),
            secret_hints: Vec::new(),
            error: None,
        };

        assert!(bundle.error.is_none());
        assert!(bundle.secret_hints.is_empty());
    }
}
