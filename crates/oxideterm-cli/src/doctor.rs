// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{env, fs, path::Path};

use oxideterm_cloud_sync::state::CloudSyncPersistedState;
use oxideterm_connections::{ConnectionStore, SavedUpstreamProxyPolicy};
use oxideterm_settings::{
    SettingsUpstreamProxyAuth, SettingsUpstreamProxyConfig, SettingsUpstreamProxyProtocol,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    args::DoctorArgs,
    cloud_sync_preview,
    error::CliResult,
    output::{self, OutputFormat},
    paths::{self, CliPaths, default_cloud_sync_path, default_connections_path},
    settings,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorResponse {
    pub(crate) ok: bool,
    pub(crate) strict: bool,
    pub(crate) paths: CliPaths,
    pub(crate) summary: DoctorSummary,
    pub(crate) checks: Vec<DoctorCheck>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorSummary {
    pub(crate) error_count: usize,
    pub(crate) warning_count: usize,
    pub(crate) info_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorCheck {
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

pub fn run(args: DoctorArgs) -> CliResult<i32> {
    let response = build_doctor_response(args.strict, args.json);
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json_with_ok(&response, response.ok),
        OutputFormat::Text => {
            output::write_text(format_doctor_text(&response));
            Ok(())
        }
    }?;

    Ok(if response.ok { 0 } else { 1 })
}

pub(crate) fn build_doctor_response(strict: bool, json: bool) -> DoctorResponse {
    let paths = paths::cli_paths();
    let checks = doctor_checks(json);
    let summary = summarize_checks(&checks);
    DoctorResponse {
        ok: doctor_ok(&summary, strict),
        strict,
        paths,
        summary,
        checks,
    }
}

fn doctor_checks(json: bool) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    checks.push(settings_check(json));
    checks.push(connections_check());
    checks.push(upstream_proxy_check(json));
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

fn upstream_proxy_check(json: bool) -> DoctorCheck {
    let settings_result = settings::load_settings_read_only(json);
    let connections_path = default_connections_path();
    let connections_result = ConnectionStore::load_read_only(&connections_path);
    let global_proxy = settings_result
        .as_ref()
        .ok()
        .and_then(|settings| settings.settings.network.upstream_proxy.as_ref());
    let env_proxy = env_upstream_proxy_source();
    let use_global_source = use_global_proxy_source(global_proxy, env_proxy.as_ref());
    let connection_counts = connections_result
        .as_ref()
        .ok()
        .map(|store| {
            upstream_proxy_policy_counts(
                store
                    .connections()
                    .iter()
                    .map(|connection| &connection.upstream_proxy),
                use_global_source,
            )
        })
        .unwrap_or_default();
    let incomplete = settings_result.is_err() || connections_result.is_err();

    DoctorCheck {
        name: "upstreamProxy",
        severity: if incomplete {
            DoctorSeverity::Warning
        } else {
            DoctorSeverity::Ok
        },
        ok: true,
        message: if incomplete {
            "upstream proxy source inspection is incomplete".to_string()
        } else {
            format!("upstream proxy default source is {use_global_source}")
        },
        details: json!({
            "useGlobalSource": use_global_source,
            "globalProxy": global_proxy.map(settings_proxy_summary),
            "envProxy": env_proxy,
            "connectionPolicyCounts": {
                "useGlobal": connection_counts.use_global,
                "direct": connection_counts.direct,
                "custom": connection_counts.custom,
            },
            "effectiveConnectionSources": {
                "global": connection_counts.effective_global,
                "envFallback": connection_counts.effective_env_fallback,
                "direct": connection_counts.effective_direct,
                "custom": connection_counts.effective_custom,
            },
            "settingsReadable": settings_result.is_ok(),
            "connectionsReadable": connections_result.is_ok(),
            "connectionsPath": connections_path.display().to_string(),
        }),
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct UpstreamProxyPolicyCounts {
    use_global: usize,
    direct: usize,
    custom: usize,
    effective_global: usize,
    effective_env_fallback: usize,
    effective_direct: usize,
    effective_custom: usize,
}

fn upstream_proxy_policy_counts<'a>(
    policies: impl IntoIterator<Item = &'a SavedUpstreamProxyPolicy>,
    use_global_source: &'static str,
) -> UpstreamProxyPolicyCounts {
    let mut counts = UpstreamProxyPolicyCounts::default();
    for policy in policies {
        match policy {
            SavedUpstreamProxyPolicy::UseGlobal => {
                counts.use_global += 1;
                match use_global_source {
                    "global" => counts.effective_global += 1,
                    "envFallback" => counts.effective_env_fallback += 1,
                    _ => counts.effective_direct += 1,
                }
            }
            SavedUpstreamProxyPolicy::Direct => {
                counts.direct += 1;
                counts.effective_direct += 1;
            }
            SavedUpstreamProxyPolicy::Custom { .. } => {
                counts.custom += 1;
                counts.effective_custom += 1;
            }
        }
    }
    counts
}

fn use_global_proxy_source(
    global_proxy: Option<&SettingsUpstreamProxyConfig>,
    env_proxy: Option<&Value>,
) -> &'static str {
    if global_proxy.is_some() {
        "global"
    } else if env_proxy.is_some() {
        "envFallback"
    } else {
        "direct"
    }
}

fn env_upstream_proxy_source() -> Option<Value> {
    let socks5 = env::var("OXIDETERM_SOCKS5_PROXY").ok();
    if let Some(value) = first_non_empty(socks5.as_deref()) {
        return Some(json!({
            "source": "env",
            "variable": "OXIDETERM_SOCKS5_PROXY",
            "protocol": "socks5",
            "configured": true,
            "hasAuth": value.contains('@'),
            "noProxyConfigured": first_non_empty(env::var("OXIDETERM_NO_PROXY").ok().as_deref()).is_some(),
        }));
    }

    let http = env::var("OXIDETERM_HTTP_PROXY").ok();
    first_non_empty(http.as_deref()).map(|value| {
        json!({
            "source": "env",
            "variable": "OXIDETERM_HTTP_PROXY",
            "protocol": "http_connect",
            "configured": true,
            "hasAuth": value.contains('@'),
            "noProxyConfigured": first_non_empty(env::var("OXIDETERM_NO_PROXY").ok().as_deref()).is_some(),
        })
    })
}

fn settings_proxy_summary(proxy: &SettingsUpstreamProxyConfig) -> Value {
    json!({
        "source": "global",
        "protocol": settings_proxy_protocol_label(proxy.protocol),
        "host": proxy.host,
        "port": proxy.port,
        "auth": settings_proxy_auth_label(&proxy.auth),
        "remoteDns": proxy.remote_dns,
        "noProxyConfigured": !proxy.no_proxy.trim().is_empty(),
    })
}

fn settings_proxy_protocol_label(protocol: SettingsUpstreamProxyProtocol) -> &'static str {
    match protocol {
        SettingsUpstreamProxyProtocol::Socks5 => "socks5",
        SettingsUpstreamProxyProtocol::HttpConnect => "http_connect",
    }
}

fn settings_proxy_auth_label(auth: &SettingsUpstreamProxyAuth) -> &'static str {
    match auth {
        SettingsUpstreamProxyAuth::None => "none",
        SettingsUpstreamProxyAuth::Password { .. } => "password",
    }
}

fn first_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
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

pub(crate) fn doctor_ok(summary: &DoctorSummary, strict: bool) -> bool {
    // Strict mode promotes warnings to failures so CI can catch degraded local state.
    summary.error_count == 0 && (!strict || summary.warning_count == 0)
}

fn format_doctor_text(response: &DoctorResponse) -> String {
    let mut lines = vec![format!(
        "ok: {} strict={} errors={} warnings={} info={}",
        response.ok,
        response.strict,
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
    use oxideterm_connections::{
        SavedUpstreamProxyAuth, SavedUpstreamProxyConfig, SavedUpstreamProxyProtocol,
    };

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

    #[test]
    fn strict_doctor_fails_on_warnings() {
        let summary = DoctorSummary {
            error_count: 0,
            warning_count: 1,
            info_count: 0,
        };

        assert!(doctor_ok(&summary, false));
        assert!(!doctor_ok(&summary, true));
    }

    #[test]
    fn upstream_proxy_policy_counts_resolve_effective_sources() {
        let custom = SavedUpstreamProxyPolicy::Custom {
            proxy: SavedUpstreamProxyConfig {
                protocol: SavedUpstreamProxyProtocol::HttpConnect,
                host: "proxy.example.com".to_string(),
                port: 8080,
                auth: SavedUpstreamProxyAuth::None,
                remote_dns: true,
                no_proxy: String::new(),
            },
        };
        let policies = vec![
            SavedUpstreamProxyPolicy::UseGlobal,
            SavedUpstreamProxyPolicy::Direct,
            custom,
        ];

        let counts = upstream_proxy_policy_counts(policies.iter(), "envFallback");

        assert_eq!(counts.use_global, 1);
        assert_eq!(counts.direct, 1);
        assert_eq!(counts.custom, 1);
        assert_eq!(counts.effective_env_fallback, 1);
        assert_eq!(counts.effective_direct, 1);
        assert_eq!(counts.effective_custom, 1);
    }
}
