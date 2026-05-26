// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync display formatting and error classification.

use chrono::{DateTime, Local};

pub fn cloud_sync_number_string(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

pub fn cloud_sync_error_code(error: &str) -> Option<&str> {
    let trimmed = error.trim();
    let code = trimmed
        .split_once(':')
        .map(|(code, _)| code.trim())
        .unwrap_or(trimmed);
    if cloud_sync_error_is_unauthorized(code) {
        return Some("http_unauthorized");
    }
    match code {
        "operation_in_progress"
        | "missing_endpoint"
        | "missing_namespace"
        | "missing_backend_token"
        | "network_request_failed"
        | "missing_git_repository"
        | "missing_s3_bucket"
        | "missing_s3_region"
        | "missing_s3_access_key_id"
        | "missing_s3_secret_access_key"
        | "missing_sync_password"
        | "etag_conflict_detected"
        | "remote_changed_before_upload"
        | "preflight_failed"
        | "snapshot_too_large"
        | "remote_not_found" => Some(code),
        _ => {
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("secret unlock required") {
                Some("secret_unlock_required")
            } else if lower.starts_with("secret access cancelled")
                || lower.contains("authentication canceled")
                || lower.contains("authentication cancelled")
            {
                Some("secret_access_cancelled")
            } else if lower.starts_with("secret access failed") {
                Some("secret_access_failed")
            } else {
                None
            }
        }
    }
}

fn cloud_sync_error_is_unauthorized(code: &str) -> bool {
    let lower = code.to_ascii_lowercase();
    (lower.starts_with("http_") || lower.starts_with("webdav_")) && lower.contains("401")
}

pub fn cloud_sync_snapshot_limit_bytes(error: &str) -> Option<usize> {
    let (_, after_max) = error.split_once("max ")?;
    let digits = after_max
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
}

pub fn format_cloud_sync_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

pub fn cloud_sync_value_prefers_mono(value: &str) -> bool {
    value != "—"
        && value.chars().count() >= 16
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '/' | '.'))
}

pub fn cloud_sync_format_timestamp(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Local)
                .format("%Y/%-m/%-d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
}

pub fn cloud_sync_progress_unit(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{}", value as usize)
    } else {
        format!("{value:.1}")
    }
}

pub fn non_empty_secret(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

pub fn cloud_sync_platform_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "native"
    }
}
