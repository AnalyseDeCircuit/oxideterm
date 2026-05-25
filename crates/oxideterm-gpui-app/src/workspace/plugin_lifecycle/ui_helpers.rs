// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_gpui_terminal::{TerminalNotice, TerminalNoticeVariant};

pub(in crate::workspace) use oxideterm_plugin_host_api::app::{
    native_plugin_custom_event_from_args, native_plugin_i18n_translate,
    native_plugin_layout_snapshot, native_plugin_platform_label, native_plugin_settings_section,
    native_plugin_theme_snapshot,
};

pub(super) fn native_plugin_notification_variant(severity: &str) -> TerminalNoticeVariant {
    match severity {
        "error" => TerminalNoticeVariant::Error,
        "warning" => TerminalNoticeVariant::Warning,
        _ => TerminalNoticeVariant::Default,
    }
}

pub(super) fn native_plugin_progress_key(plugin_id: &str, registration_id: &str) -> String {
    format!("{plugin_id}:{registration_id}")
}

pub(super) fn native_plugin_progress_is_done(value: &serde_json::Value) -> bool {
    ["done", "completed", "dismissed"]
        .iter()
        .any(|key| value.get(*key).and_then(serde_json::Value::as_bool) == Some(true))
}

pub(super) fn native_plugin_progress_notice(
    plugin_id: &str,
    registration_id: &str,
    value: serde_json::Value,
) -> TerminalNotice {
    let title = value
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or("Plugin progress")
        .to_string();
    let description = value
        .get("message")
        .or_else(|| value.get("description"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let progress = native_plugin_progress_percent(&value);
    let status_text = value
        .get("statusText")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| progress.map(|percent| format!("{percent:.0}%")));

    TerminalNotice {
        title: native_plugin_notice_title(plugin_id, title),
        description: description.or_else(|| Some(registration_id.to_string())),
        status_text,
        progress,
        variant: TerminalNoticeVariant::Default,
    }
}

fn native_plugin_progress_percent(value: &serde_json::Value) -> Option<f32> {
    if let Some(percent) = value
        .get("progress")
        .or_else(|| value.get("percent"))
        .and_then(serde_json::Value::as_f64)
    {
        return Some((percent as f32).clamp(0.0, 100.0));
    }

    let current = value
        .get("value")
        .or_else(|| value.get("current"))
        .and_then(serde_json::Value::as_f64)?;
    let total = value
        .get("total")
        .and_then(serde_json::Value::as_f64)
        .filter(|total| *total > 0.0)?;
    Some(((current / total) as f32 * 100.0).clamp(0.0, 100.0))
}

pub(super) fn native_plugin_notice_title(plugin_id: &str, title: String) -> String {
    format!("{title} ({plugin_id})")
}

pub(super) fn native_plugin_dialog_title(plugin_id: &str, title: &str) -> String {
    native_plugin_notice_title(plugin_id, title.to_string())
}
