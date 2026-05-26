// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fs, path::Path};

use oxideterm_cloud_sync::state::CloudSyncPersistedState;
use oxideterm_connections::ConnectionStore;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    args::OutputArgs,
    cloud_sync_preview,
    error::CliResult,
    output::{self, OutputFormat},
    paths::{self, CliPaths, default_cloud_sync_path, default_connections_path},
    settings,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorResponse {
    ok: bool,
    paths: CliPaths,
    summary: DoctorSummary,
    checks: Vec<DoctorCheck>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorSummary {
    error_count: usize,
    warning_count: usize,
    info_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorCheck {
    name: &'static str,
    severity: DoctorSeverity,
    ok: bool,
    message: String,
    details: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum DoctorSeverity {
    Ok,
    Info,
    Warning,
    Error,
}

pub fn run(args: OutputArgs) -> CliResult<()> {
    let paths = paths::cli_paths();
    let checks = doctor_checks(args.json);
    let summary = summarize_checks(&checks);
    let response = DoctorResponse {
        ok: summary.error_count == 0,
        paths,
        summary,
        checks,
    };

    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format_doctor_text(&response));
            Ok(())
        }
    }
}

fn doctor_checks(json: bool) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    checks.push(settings_check(json));
    checks.push(connections_check());
    checks.extend(cloud_sync_checks(json));
    checks
}

fn settings_check(json: bool) -> DoctorCheck {
    match settings::load_settings_read_only(json) {
        Ok(settings) => DoctorCheck {
            name: "settings",
            severity: DoctorSeverity::Ok,
            ok: true,
            message: "settings file is readable and sanitizes successfully".to_string(),
            details: json!({
                "path": settings.path,
                "version": settings.settings.version,
            }),
        },
        Err(error) => DoctorCheck {
            name: "settings",
            severity: DoctorSeverity::Error,
            ok: false,
            message: error.message,
            details: json!({ "code": error.code }),
        },
    }
}

fn connections_check() -> DoctorCheck {
    let path = default_connections_path();
    match ConnectionStore::load_read_only(&path) {
        Ok(store) => {
            let connection_count = store.connections().len();
            let group_count = store.groups().len();
            let severity = if connection_count == 0 {
                DoctorSeverity::Info
            } else {
                DoctorSeverity::Ok
            };
            DoctorCheck {
                name: "connections",
                severity,
                ok: true,
                message: if connection_count == 0 {
                    "no saved connections are configured".to_string()
                } else {
                    "connections store is readable".to_string()
                },
                details: json!({
                    "path": path.display().to_string(),
                    "connectionCount": connection_count,
                    "groupCount": group_count,
                }),
            }
        }
        Err(error) => DoctorCheck {
            name: "connections",
            severity: DoctorSeverity::Error,
            ok: false,
            message: error.to_string(),
            details: json!({ "path": path.display().to_string() }),
        },
    }
}

fn cloud_sync_checks(json: bool) -> Vec<DoctorCheck> {
    let path = default_cloud_sync_path();
    let mut checks = Vec::new();
    checks.push(file_presence_check("cloudSyncFile", &path));
    match cloud_sync_preview::load_persisted_state(&path, json) {
        Ok(state) => {
            checks.push(cloud_sync_state_check(&path, &state));
            if state
                .last_error
                .as_deref()
                .is_some_and(|error| !error.trim().is_empty())
            {
                checks.push(DoctorCheck {
                    name: "cloudSyncLastError",
                    severity: DoctorSeverity::Warning,
                    ok: true,
                    message: "cloud sync has a persisted last error".to_string(),
                    details: json!({ "lastError": state.last_error }),
                });
            }
            if cloud_sync_has_dirty(&state)
                && !state.remote_exists
                && state.last_known_remote_revision.is_none()
            {
                checks.push(DoctorCheck {
                    name: "cloudSyncDirtyWithoutRemote",
                    severity: DoctorSeverity::Warning,
                    ok: true,
                    message: "cloud sync has local dirty state but no remote baseline is recorded"
                        .to_string(),
                    details: json!({
                        "localDirty": state.local_dirty,
                        "remoteExists": state.remote_exists,
                        "lastKnownRemoteRevision": state.last_known_remote_revision,
                    }),
                });
            }
        }
        Err(error) => checks.push(DoctorCheck {
            name: "cloudSyncState",
            severity: DoctorSeverity::Error,
            ok: false,
            message: error.message,
            details: json!({ "code": error.code, "path": path.display().to_string() }),
        }),
    }
    checks
}

fn cloud_sync_state_check(path: &Path, state: &CloudSyncPersistedState) -> DoctorCheck {
    let has_dirty = cloud_sync_has_dirty(state);
    DoctorCheck {
        name: "cloudSyncState",
        severity: if has_dirty {
            DoctorSeverity::Warning
        } else {
            DoctorSeverity::Ok
        },
        ok: true,
        message: if has_dirty {
            "cloud sync has local dirty sections".to_string()
        } else {
            "cloud sync state is clean".to_string()
        },
        details: json!({
            "path": path.display().to_string(),
            "localDirty": state.local_dirty,
            "historyCount": state.sync_history.len(),
            "rollbackBackupCount": state.rollback_backups.len(),
            "lastKnownRemoteRevision": state.last_known_remote_revision,
        }),
    }
}

fn file_presence_check(name: &'static str, path: &Path) -> DoctorCheck {
    match fs::metadata(path) {
        Ok(metadata) => DoctorCheck {
            name,
            severity: DoctorSeverity::Ok,
            ok: true,
            message: "file exists".to_string(),
            details: json!({
                "path": path.display().to_string(),
                "sizeBytes": metadata.is_file().then_some(metadata.len()),
            }),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => DoctorCheck {
            name,
            severity: DoctorSeverity::Info,
            ok: true,
            message: "file does not exist; defaults will be used".to_string(),
            details: json!({ "path": path.display().to_string() }),
        },
        Err(error) => DoctorCheck {
            name,
            severity: DoctorSeverity::Error,
            ok: false,
            message: error.to_string(),
            details: json!({ "path": path.display().to_string() }),
        },
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

fn summarize_checks(checks: &[DoctorCheck]) -> DoctorSummary {
    let mut summary = DoctorSummary::default();
    for check in checks {
        match check.severity {
            DoctorSeverity::Error => summary.error_count += 1,
            DoctorSeverity::Warning => summary.warning_count += 1,
            DoctorSeverity::Info => summary.info_count += 1,
            DoctorSeverity::Ok => {}
        }
    }
    summary
}

fn format_doctor_text(response: &DoctorResponse) -> String {
    let mut lines = vec![format!(
        "ok: {} errors={} warnings={} info={}",
        response.ok,
        response.summary.error_count,
        response.summary.warning_count,
        response.summary.info_count
    )];
    for check in &response.checks {
        lines.push(format!(
            "{}\t{}\t{}",
            severity_label(check.severity),
            check.name,
            check.message
        ));
    }
    lines.join("\n")
}

fn severity_label(severity: DoctorSeverity) -> &'static str {
    match severity {
        DoctorSeverity::Ok => "ok",
        DoctorSeverity::Info => "info",
        DoctorSeverity::Warning => "warning",
        DoctorSeverity::Error => "error",
    }
}

#[cfg(test)]
mod tests {
    use oxideterm_cloud_sync::StructuredDirtySections;

    use super::*;

    #[test]
    fn dirty_cloud_sync_state_is_detected() {
        let state = CloudSyncPersistedState {
            local_dirty_sections: Some(StructuredDirtySections {
                connections: true,
                ..StructuredDirtySections::default()
            }),
            ..CloudSyncPersistedState::default()
        };

        assert!(cloud_sync_has_dirty(&state));
    }

    #[test]
    fn summary_counts_non_ok_severities() {
        let checks = vec![
            DoctorCheck {
                name: "a",
                severity: DoctorSeverity::Ok,
                ok: true,
                message: String::new(),
                details: json!({}),
            },
            DoctorCheck {
                name: "b",
                severity: DoctorSeverity::Warning,
                ok: true,
                message: String::new(),
                details: json!({}),
            },
            DoctorCheck {
                name: "c",
                severity: DoctorSeverity::Error,
                ok: false,
                message: String::new(),
                details: json!({}),
            },
        ];

        let summary = summarize_checks(&checks);

        assert_eq!(summary.warning_count, 1);
        assert_eq!(summary.error_count, 1);
    }
}
