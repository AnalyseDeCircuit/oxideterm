// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{HashMap, HashSet};

use oxideterm_connections::{AuthType, ConnectionInfo, ConnectionStore, ProxyHopInfo};
use serde::Serialize;

use crate::{
    args::ConnectionsValidateArgs,
    error::{CliResult, runtime_error},
    output::{self, OutputFormat},
    paths::default_connections_path,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnectionsValidationResponse {
    path: String,
    ok: bool,
    strict: bool,
    checked_count: usize,
    issue_count: usize,
    error_count: usize,
    warning_count: usize,
    issues: Vec<ConnectionValidationIssue>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnectionValidationIssue {
    pub(crate) severity: &'static str,
    pub(crate) code: &'static str,
    pub(crate) connection_id: Option<String>,
    pub(crate) connection_name: Option<String>,
    pub(crate) message: String,
}

pub fn run(args: ConnectionsValidateArgs) -> CliResult<i32> {
    let store = load_connection_store(args.json)?;
    let connections = store.connection_infos();
    // Validation is intentionally structural only: no network probes, keychain reads, or writes.
    let issues = validate_connection_infos(&connections, store.groups());
    let error_count = count_issues(&issues, "error");
    let warning_count = count_issues(&issues, "warning");
    let ok = validation_ok(error_count, warning_count, args.strict);
    let response = ConnectionsValidationResponse {
        path: store.path().display().to_string(),
        ok,
        strict: args.strict,
        checked_count: connections.len(),
        issue_count: issues.len(),
        error_count,
        warning_count,
        issues,
    };

    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json_with_ok(&response, response.ok),
        OutputFormat::Text => {
            if response.issues.is_empty() {
                output::write_text("Connections validation passed");
            } else {
                for issue in &response.issues {
                    output::write_text(format_validation_issue(issue));
                }
            }
            Ok(())
        }
    }?;

    Ok(if response.ok { 0 } else { 1 })
}

fn load_connection_store(json: bool) -> CliResult<ConnectionStore> {
    ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, json))
}

pub(crate) fn validate_connection_infos(
    connections: &[ConnectionInfo],
    groups: &[String],
) -> Vec<ConnectionValidationIssue> {
    let mut issues = Vec::new();
    let group_names = groups.iter().map(String::as_str).collect::<HashSet<_>>();
    let mut ids: HashMap<&str, usize> = HashMap::new();
    let mut names: HashMap<String, usize> = HashMap::new();

    for connection in connections {
        *ids.entry(connection.id.as_str()).or_default() += 1;
        *names
            .entry(connection.name.trim().to_ascii_lowercase())
            .or_default() += 1;
    }

    for connection in connections {
        validate_required_field(&mut issues, connection, "empty_id", "id", &connection.id);
        validate_required_field(
            &mut issues,
            connection,
            "empty_name",
            "name",
            &connection.name,
        );
        validate_required_field(
            &mut issues,
            connection,
            "empty_host",
            "host",
            &connection.host,
        );
        validate_required_field(
            &mut issues,
            connection,
            "empty_username",
            "username",
            &connection.username,
        );
        if connection.port == 0 {
            push_connection_issue(
                &mut issues,
                connection,
                "error",
                "invalid_port",
                "port must be greater than 0",
            );
        }
        if ids.get(connection.id.as_str()).copied().unwrap_or_default() > 1 {
            push_connection_issue(
                &mut issues,
                connection,
                "error",
                "duplicate_id",
                "connection id is duplicated",
            );
        }
        if names
            .get(&connection.name.trim().to_ascii_lowercase())
            .copied()
            .unwrap_or_default()
            > 1
        {
            push_connection_issue(
                &mut issues,
                connection,
                "warning",
                "duplicate_name",
                "connection name is duplicated and may make CLI lookup ambiguous",
            );
        }
        if let Some(group) = connection
            .group
            .as_deref()
            .filter(|group| !group.trim().is_empty())
        {
            if !group_names.contains(group) {
                push_connection_issue(
                    &mut issues,
                    connection,
                    "warning",
                    "missing_group",
                    format!("group '{group}' is not present in the group list"),
                );
            }
        }
        validate_connection_auth(&mut issues, connection);
        for (index, hop) in connection.proxy_chain.iter().enumerate() {
            validate_proxy_hop(&mut issues, connection, index, hop);
        }
    }

    issues
}

pub(crate) fn count_issues(issues: &[ConnectionValidationIssue], severity: &str) -> usize {
    issues
        .iter()
        .filter(|issue| issue.severity == severity)
        .count()
}

pub(crate) fn validation_ok(error_count: usize, warning_count: usize, strict: bool) -> bool {
    // Strict mode is for scripts: warnings become a failing validation result.
    error_count == 0 && (!strict || warning_count == 0)
}

fn validate_required_field(
    issues: &mut Vec<ConnectionValidationIssue>,
    connection: &ConnectionInfo,
    code: &'static str,
    field: &'static str,
    value: &str,
) {
    if value.trim().is_empty() {
        push_connection_issue(
            issues,
            connection,
            "error",
            code,
            format!("{field} must not be empty"),
        );
    }
}

fn validate_connection_auth(
    issues: &mut Vec<ConnectionValidationIssue>,
    connection: &ConnectionInfo,
) {
    match connection.auth_type {
        AuthType::Key if blank_option(connection.key_path.as_deref()) => push_connection_issue(
            issues,
            connection,
            "error",
            "missing_key_path",
            "key authentication requires keyPath",
        ),
        AuthType::Certificate => {
            if blank_option(connection.key_path.as_deref()) {
                push_connection_issue(
                    issues,
                    connection,
                    "error",
                    "missing_key_path",
                    "certificate authentication requires keyPath",
                );
            }
            if blank_option(connection.cert_path.as_deref()) {
                push_connection_issue(
                    issues,
                    connection,
                    "error",
                    "missing_cert_path",
                    "certificate authentication requires certPath",
                );
            }
        }
        _ => {}
    }
}

fn validate_proxy_hop(
    issues: &mut Vec<ConnectionValidationIssue>,
    connection: &ConnectionInfo,
    index: usize,
    hop: &ProxyHopInfo,
) {
    let hop_label = format!("proxy hop {index}");
    if hop.host.trim().is_empty() {
        push_connection_issue(
            issues,
            connection,
            "error",
            "empty_proxy_host",
            format!("{hop_label} host must not be empty"),
        );
    }
    if hop.username.trim().is_empty() {
        push_connection_issue(
            issues,
            connection,
            "error",
            "empty_proxy_username",
            format!("{hop_label} username must not be empty"),
        );
    }
    if hop.port == 0 {
        push_connection_issue(
            issues,
            connection,
            "error",
            "invalid_proxy_port",
            format!("{hop_label} port must be greater than 0"),
        );
    }
    match hop.auth_type {
        AuthType::Key if blank_option(hop.key_path.as_deref()) => push_connection_issue(
            issues,
            connection,
            "error",
            "missing_proxy_key_path",
            format!("{hop_label} key authentication requires keyPath"),
        ),
        AuthType::Certificate => {
            if blank_option(hop.key_path.as_deref()) {
                push_connection_issue(
                    issues,
                    connection,
                    "error",
                    "missing_proxy_key_path",
                    format!("{hop_label} certificate authentication requires keyPath"),
                );
            }
            if blank_option(hop.cert_path.as_deref()) {
                push_connection_issue(
                    issues,
                    connection,
                    "error",
                    "missing_proxy_cert_path",
                    format!("{hop_label} certificate authentication requires certPath"),
                );
            }
        }
        _ => {}
    }
}

fn blank_option(value: Option<&str>) -> bool {
    value.is_none_or(|value| value.trim().is_empty())
}

fn push_connection_issue(
    issues: &mut Vec<ConnectionValidationIssue>,
    connection: &ConnectionInfo,
    severity: &'static str,
    code: &'static str,
    message: impl Into<String>,
) {
    issues.push(ConnectionValidationIssue {
        severity,
        code,
        connection_id: Some(connection.id.clone()),
        connection_name: Some(connection.name.clone()),
        message: message.into(),
    });
}

fn format_validation_issue(issue: &ConnectionValidationIssue) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        issue.severity,
        issue.code,
        issue.connection_name.as_deref().unwrap_or("-"),
        issue.message
    )
}

#[cfg(test)]
mod tests {
    use oxideterm_connections::AuthType;

    use super::*;

    fn sample_connection(id: &str, name: &str) -> ConnectionInfo {
        ConnectionInfo {
            id: id.to_string(),
            name: name.to_string(),
            group: Some("prod".to_string()),
            host: "example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            auth_type: AuthType::Password,
            key_path: None,
            cert_path: None,
            proxy_chain: Vec::new(),
            created_at: "2026-05-26T00:00:00Z".to_string(),
            last_used_at: None,
            color: None,
            tags: vec!["primary".to_string()],
            agent_forwarding: false,
            post_connect_command: None,
        }
    }

    #[test]
    fn validation_reports_duplicate_names_and_missing_key_path() {
        let connections = vec![
            sample_connection("id-1", "Prod"),
            ConnectionInfo {
                id: "id-2".to_string(),
                auth_type: AuthType::Key,
                key_path: None,
                ..sample_connection("id-2", "prod")
            },
        ];

        let issues = validate_connection_infos(&connections, &["prod".to_string()]);

        assert!(issues.iter().any(|issue| issue.code == "duplicate_name"));
        assert!(issues.iter().any(|issue| issue.code == "missing_key_path"));
    }

    #[test]
    fn strict_validation_fails_on_warnings() {
        assert!(validation_ok(0, 1, false));
        assert!(!validation_ok(0, 1, true));
        assert!(!validation_ok(1, 0, false));
    }
}
