// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::Serialize;
use serde_json::Value;

use crate::{
    args::BackupInspectArgs,
    backup::document::{BACKUP_FORMAT, read_backup_value, resolve_backup_query},
    error::CliResult,
    output::{self, OutputFormat},
};

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

pub(super) fn verify(args: BackupInspectArgs) -> CliResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
