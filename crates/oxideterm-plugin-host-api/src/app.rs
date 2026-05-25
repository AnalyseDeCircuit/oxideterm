// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! App-level host API projections shared by native plugin runtimes.

use serde_json::{Value, json};

pub fn native_plugin_i18n_translate(
    i18n: &oxideterm_i18n::I18n,
    plugin_id: &str,
    key: &str,
) -> String {
    let full_key = format!("plugin.{plugin_id}.{key}");
    let translated = i18n.t(&full_key);
    // Tauri pluginI18nManager auto-prefixes plugin keys and falls back to the
    // raw plugin key when no bundle is loaded. Native keeps that contract while
    // plugin locale-bundle loading is completed in the rest of Phase 4.
    if translated == full_key {
        key.to_string()
    } else {
        translated
    }
}

pub fn native_plugin_layout_snapshot(
    sidebar_collapsed: bool,
    active_tab_id: Option<String>,
    tab_count: usize,
) -> Value {
    // Tauri exposes this exact app-store shape and freezes it before returning
    // to plugins. Native mirrors the field names so process runtimes can share
    // the same plugin-facing API contract.
    json!({
        "sidebarCollapsed": sidebar_collapsed,
        "activeTabId": active_tab_id,
        "tabCount": tab_count,
    })
}

pub fn native_plugin_platform_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

pub fn native_plugin_theme_snapshot(theme_name: &str) -> Value {
    json!({
        "name": theme_name,
        "isDark": native_plugin_theme_is_dark(theme_name),
    })
}

fn native_plugin_theme_is_dark(theme_name: &str) -> bool {
    !theme_name.to_ascii_lowercase().contains("light")
}

pub fn native_plugin_settings_section(settings: &Value, category: &str) -> Value {
    settings
        .get(category)
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}))
}

pub fn native_plugin_custom_event_from_args(
    plugin_id: &str,
    args: Value,
) -> Result<(String, Value), String> {
    let event_name = args
        .get("name")
        .or_else(|| args.get("event"))
        .and_then(Value::as_str)
        .ok_or_else(|| "events.emit requires args.name".to_string())?;
    let owner_plugin_id = args
        .get("pluginId")
        .or_else(|| args.get("ownerPluginId"))
        .and_then(Value::as_str)
        .unwrap_or(plugin_id);
    let event_key = native_plugin_custom_event_key(owner_plugin_id, event_name)?;
    // Custom plugin events are scoped to the emitting plugin by default. The
    // payload names both the owner and public event name so subscribers do not
    // need to parse the internal routing key.
    Ok((
        event_key,
        json!({
            "pluginId": owner_plugin_id,
            "name": event_name,
            "payload": args.get("payload").cloned().unwrap_or(Value::Null),
        }),
    ))
}

pub fn native_plugin_custom_event_key(
    owner_plugin_id: &str,
    event_name: &str,
) -> Result<String, String> {
    native_plugin_validate_plugin_id(owner_plugin_id)?;
    native_plugin_validate_event_name(event_name)?;
    Ok(format!("plugin.{owner_plugin_id}:{event_name}"))
}

pub fn native_plugin_validate_plugin_id(plugin_id: &str) -> Result<(), String> {
    if plugin_id.is_empty() {
        return Err("Plugin ID cannot be empty".to_string());
    }
    if plugin_id.contains("..") {
        return Err("Plugin ID cannot contain path traversal (..)".to_string());
    }
    if plugin_id.contains('/') || plugin_id.contains('\\') {
        return Err("Plugin ID cannot contain path separators".to_string());
    }
    if plugin_id.bytes().any(|byte| byte < 0x20) {
        return Err("Plugin ID contains invalid characters".to_string());
    }
    Ok(())
}

pub fn native_plugin_validate_event_name(event_name: &str) -> Result<(), String> {
    if event_name.trim().is_empty() {
        return Err("Plugin event name cannot be empty".to_string());
    }
    if event_name.len() > 128 {
        return Err("Plugin event name is too long".to_string());
    }
    if event_name.contains("..") || event_name.contains('/') || event_name.contains('\\') {
        return Err("Plugin event name cannot contain path separators or traversal".to_string());
    }
    if event_name
        .bytes()
        .any(|byte| byte < 0x20 || byte == b'*' || byte == b' ')
    {
        return Err("Plugin event name contains invalid characters".to_string());
    }
    Ok(())
}
