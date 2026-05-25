// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_settings::{Language, UiDensity};
use serde_json::{Map, Value, json};

pub struct NativeSyncableSettingsNormalization {
    pub payload: Value,
    pub warnings: Vec<Value>,
}

pub fn native_syncable_settings_payload(settings: &Value) -> Value {
    let mut payload = Map::new();
    let mut appearance = Map::new();
    let mut terminal = Map::new();
    let mut reconnect = Map::new();

    if let Some(language) = settings
        .get("general")
        .and_then(|general| general.get("language"))
        .and_then(Value::as_str)
    {
        appearance.insert("language".to_string(), json!(language));
    }
    if let Some(ui_density) = settings
        .get("appearance")
        .and_then(|appearance| appearance.get("uiDensity"))
        .and_then(Value::as_str)
    {
        appearance.insert("uiDensity".to_string(), json!(ui_density));
    }
    if let Some(font_size) = settings
        .get("terminal")
        .and_then(|terminal| terminal.get("fontSize"))
        .and_then(Value::as_i64)
    {
        terminal.insert("fontSize".to_string(), json!(font_size));
    }
    if let Some(theme) = settings
        .get("terminal")
        .and_then(|terminal| terminal.get("theme"))
        .and_then(Value::as_str)
    {
        terminal.insert("theme".to_string(), json!(theme));
    }
    if let Some(auto_reconnect) = settings
        .get("reconnect")
        .and_then(|reconnect| reconnect.get("enabled"))
        .and_then(Value::as_bool)
    {
        reconnect.insert("autoReconnect".to_string(), json!(auto_reconnect));
    }

    if !appearance.is_empty() {
        payload.insert("appearance".to_string(), Value::Object(appearance));
    }
    if !terminal.is_empty() {
        payload.insert("terminal".to_string(), Value::Object(terminal));
    }
    if !reconnect.is_empty() {
        payload.insert("reconnect".to_string(), Value::Object(reconnect));
    }
    Value::Object(payload)
}

pub fn native_syncable_settings_payload_arg(args: Value) -> Value {
    // Process plugins usually pass `{ payload }`; accepting the raw payload too
    // keeps the protocol tolerant for early SDK/demo runtimes.
    args.get("payload")
        .filter(|payload| payload.is_object())
        .cloned()
        .unwrap_or(args)
}

pub fn native_normalize_syncable_settings_payload(
    payload: &Value,
) -> NativeSyncableSettingsNormalization {
    let mut normalized = Map::new();
    let mut warnings = Vec::new();

    if let Some(source) = payload.get("appearance").and_then(Value::as_object) {
        let mut appearance = Map::new();
        if let Some(language) = source.get("language") {
            if let Some(language) = language
                .as_str()
                .filter(|value| native_language_supported(value))
            {
                appearance.insert("language".to_string(), json!(language));
            } else if !language.is_null() {
                warnings.push(native_syncable_settings_warning(
                    "appearance.language",
                    "unsupported-language",
                    false,
                    format!(
                        "Unsupported language: {}",
                        native_syncable_warning_value(language)
                    ),
                    None,
                ));
            }
        }
        if let Some(ui_density) = source.get("uiDensity") {
            if let Some(ui_density) = ui_density
                .as_str()
                .filter(|value| native_ui_density_supported(value))
            {
                appearance.insert("uiDensity".to_string(), json!(ui_density));
            } else if !ui_density.is_null() {
                warnings.push(native_syncable_settings_warning(
                    "appearance.uiDensity",
                    "invalid-ui-density",
                    false,
                    format!(
                        "Unsupported ui density: {}",
                        native_syncable_warning_value(ui_density)
                    ),
                    None,
                ));
            }
        }
        if !appearance.is_empty() {
            normalized.insert("appearance".to_string(), Value::Object(appearance));
        }
    }

    if let Some(source) = payload.get("terminal").and_then(Value::as_object) {
        let mut terminal = Map::new();
        if let Some(font_size) = source.get("fontSize") {
            if let Some(font_size) = font_size.as_f64().filter(|value| value.is_finite()) {
                let normalized_font_size = (font_size.round() as i64).clamp(8, 32);
                terminal.insert("fontSize".to_string(), json!(normalized_font_size));
                if (normalized_font_size as f64 - font_size).abs() > f64::EPSILON {
                    warnings.push(native_syncable_settings_warning(
                        "terminal.fontSize",
                        "font-size-clamped",
                        true,
                        format!("Font size was clamped to {normalized_font_size}"),
                        Some(json!(normalized_font_size)),
                    ));
                }
            } else {
                warnings.push(native_syncable_settings_warning(
                    "terminal.fontSize",
                    "invalid-font-size",
                    false,
                    "Font size must be a finite number".to_string(),
                    None,
                ));
            }
        }
        if let Some(theme) = source.get("theme") {
            let theme = theme.as_str().map(str::trim).unwrap_or_default();
            if theme.is_empty() {
                warnings.push(native_syncable_settings_warning(
                    "terminal.theme",
                    "missing-theme",
                    false,
                    "Theme id cannot be empty".to_string(),
                    None,
                ));
            } else {
                terminal.insert("theme".to_string(), json!(theme));
            }
        }
        if !terminal.is_empty() {
            normalized.insert("terminal".to_string(), Value::Object(terminal));
        }
    }

    if let Some(source) = payload.get("reconnect").and_then(Value::as_object) {
        let mut reconnect = Map::new();
        if let Some(auto_reconnect) = source.get("autoReconnect") {
            if let Some(auto_reconnect) = auto_reconnect.as_bool() {
                reconnect.insert("autoReconnect".to_string(), json!(auto_reconnect));
            } else {
                warnings.push(native_syncable_settings_warning(
                    "reconnect.autoReconnect",
                    "invalid-auto-reconnect",
                    false,
                    "autoReconnect must be a boolean".to_string(),
                    None,
                ));
            }
        }
        if !reconnect.is_empty() {
            normalized.insert("reconnect".to_string(), Value::Object(reconnect));
        }
    }

    NativeSyncableSettingsNormalization {
        payload: Value::Object(normalized),
        warnings,
    }
}

fn native_syncable_settings_warning(
    path: &str,
    code: &str,
    applied: bool,
    message: String,
    normalized_value: Option<Value>,
) -> Value {
    let mut warning = Map::new();
    warning.insert("path".to_string(), json!(path));
    warning.insert("code".to_string(), json!(code));
    warning.insert("applied".to_string(), json!(applied));
    warning.insert("message".to_string(), json!(message));
    if let Some(normalized_value) = normalized_value {
        warning.insert("normalizedValue".to_string(), normalized_value);
    }
    Value::Object(warning)
}

fn native_language_supported(language: &str) -> bool {
    serde_json::from_value::<Language>(json!(language)).is_ok()
}

fn native_ui_density_supported(ui_density: &str) -> bool {
    serde_json::from_value::<UiDensity>(json!(ui_density)).is_ok()
}

fn native_syncable_warning_value(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

pub fn native_syncable_settings_revision(payload: &Value) -> String {
    let text = native_syncable_settings_json_string(payload);
    let mut hash = 2166136261u32;
    for byte in text.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    format!("fnv1a-{hash:x}")
}

fn native_syncable_settings_json_string(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let keys = [
                "appearance",
                "language",
                "uiDensity",
                "terminal",
                "fontSize",
                "theme",
                "reconnect",
                "autoReconnect",
            ];
            let ordered = keys
                .iter()
                .filter_map(|key| object.get(*key).map(|value| (*key, value)))
                .chain(
                    object
                        .iter()
                        .filter(|(key, _)| !keys.contains(&key.as_str()))
                        .map(|(key, value)| (key.as_str(), value)),
                )
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_string()),
                        native_syncable_settings_json_string(value)
                    )
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", ordered.join(","))
        }
        Value::Array(values) => {
            let values = values
                .iter()
                .map(native_syncable_settings_json_string)
                .collect::<Vec<_>>();
            format!("[{}]", values.join(","))
        }
        Value::String(value) => serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string()),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_string(),
    }
}
