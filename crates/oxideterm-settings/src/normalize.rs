// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::*;

#[derive(Clone, Debug, PartialEq)]
pub struct SanitizedSettings {
    pub settings: PersistedSettings,
    pub migration_warnings: Vec<String>,
    pub validation_warnings: Vec<String>,
}

fn merge_json(defaults: &mut Value, incoming: &Value) {
    match (defaults, incoming) {
        (Value::Object(default_map), Value::Object(incoming_map)) => {
            for (key, value) in incoming_map {
                if let Some(target) = default_map.get_mut(key) {
                    merge_json(target, value);
                } else {
                    default_map.insert(key.clone(), value.clone());
                }
            }
        }
        (target, incoming_value) => *target = incoming_value.clone(),
    }
}

fn get_path_mut<'a>(value: &'a mut Value, path: &[&str]) -> Option<&'a mut Value> {
    let mut current = value;
    for segment in path {
        current = current.get_mut(*segment)?;
    }
    Some(current)
}

fn object_mut<'a>(value: &'a mut Value, key: &str) -> Option<&'a mut Map<String, Value>> {
    value.get_mut(key).and_then(Value::as_object_mut)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn migrate_ai_providers(settings: &mut Value, warnings: &mut Vec<String>) {
    let Some(ai) = settings.get_mut("ai").and_then(Value::as_object_mut) else {
        return;
    };
    if ai
        .get("providers")
        .and_then(Value::as_array)
        .is_some_and(|providers| !providers.is_empty())
    {
        return;
    }

    let base_url = ai
        .get("baseUrl")
        .and_then(Value::as_str)
        .unwrap_or("https://api.openai.com/v1")
        .to_string();
    let legacy_model = ai
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .unwrap_or("gpt-4o-mini")
        .to_string();
    let created_at = now_ms();
    let mut providers = vec![
        json!({
            "id": "builtin-openai",
            "type": "openai",
            "name": "OpenAI",
            "baseUrl": "https://api.openai.com/v1",
            "defaultModel": "gpt-4o-mini",
            "models": [],
            "enabled": true,
            "createdAt": created_at,
        }),
        json!({
            "id": "builtin-anthropic",
            "type": "anthropic",
            "name": "Anthropic",
            "baseUrl": "https://api.anthropic.com",
            "defaultModel": "claude-sonnet-4-20250514",
            "models": [],
            "enabled": true,
            "createdAt": created_at,
        }),
        json!({
            "id": "builtin-deepseek",
            "type": "deepseek",
            "name": "DeepSeek",
            "baseUrl": "https://api.deepseek.com",
            "defaultModel": "deepseek-v4-flash",
            "models": ["deepseek-v4-flash", "deepseek-v4-pro", "deepseek-chat", "deepseek-reasoner"],
            "enabled": true,
            "createdAt": created_at,
        }),
        json!({
            "id": "builtin-gemini",
            "type": "gemini",
            "name": "Google Gemini",
            "baseUrl": "https://generativelanguage.googleapis.com/v1beta",
            "defaultModel": "gemini-2.0-flash",
            "models": [],
            "enabled": true,
            "createdAt": created_at,
        }),
        json!({
            "id": "builtin-ollama",
            "type": "ollama",
            "name": "Ollama (Local)",
            "baseUrl": "http://localhost:11434",
            "defaultModel": "",
            "models": [],
            "enabled": false,
            "createdAt": created_at,
        }),
    ];

    let default_openai_url = "https://api.openai.com/v1";
    let active_provider_id = if !base_url.is_empty() && base_url != default_openai_url {
        providers.insert(
            0,
            json!({
                "id": format!("custom-migrated-{created_at}"),
                "type": "openai_compatible",
                "name": "Custom (Migrated)",
                "baseUrl": base_url.clone(),
                "defaultModel": legacy_model.clone(),
                "models": [legacy_model.clone()],
                "enabled": true,
                "createdAt": created_at,
            }),
        );
        providers
            .first()
            .and_then(|provider| provider.get("id"))
            .cloned()
            .unwrap_or_else(|| json!("builtin-openai"))
    } else {
        json!("builtin-openai")
    };

    ai.insert("providers".to_string(), Value::Array(providers));
    ai.insert("activeProviderId".to_string(), active_provider_id);
    ai.insert("activeModel".to_string(), json!(legacy_model));
    warnings.push("Migrated AI settings to multi-provider format".to_string());
}

fn clamp_i64(
    value: &mut Value,
    fallback: i64,
    min: i64,
    max: i64,
    path: &str,
    warnings: &mut Vec<String>,
) {
    let Some(number) = value
        .as_i64()
        .or_else(|| value.as_f64().map(|v| v.round() as i64))
    else {
        *value = json!(fallback);
        warnings.push(format!("{} reset to default {}", path, fallback));
        return;
    };
    let clamped = number.clamp(min, max);
    if clamped != number {
        warnings.push(format!("{} clamped from {} to {}", path, number, clamped));
    }
    *value = json!(clamped);
}

fn clamp_f64(
    value: &mut Value,
    fallback: f64,
    min: f64,
    max: f64,
    path: &str,
    warnings: &mut Vec<String>,
) {
    let Some(number) = value.as_f64() else {
        *value = json!(fallback);
        warnings.push(format!("{} reset to default {}", path, fallback));
        return;
    };
    let clamped = number.clamp(min, max);
    if (clamped - number).abs() > f64::EPSILON {
        warnings.push(format!("{} clamped from {} to {}", path, number, clamped));
    }
    *value = json!(clamped);
}

fn sanitize_enum(
    root: &mut Value,
    path: &[&str],
    allowed: &[&str],
    fallback: &str,
    warnings: &mut Vec<String>,
) {
    let Some(value) = get_path_mut(root, path) else {
        return;
    };
    if value.as_str().is_some_and(|item| allowed.contains(&item)) {
        return;
    }
    *value = json!(fallback);
    warnings.push(format!("{} reset to {}", path.join("."), fallback));
}

fn clamp_backend_hot_lines(lines: i64) -> i64 {
    lines.clamp(BACKEND_HOT_BUFFER_MIN, BACKEND_HOT_BUFFER_MAX)
}

fn clamp_terminal_scrollback(lines: i64) -> i64 {
    lines.clamp(TERMINAL_SCROLLBACK_MIN, TERMINAL_SCROLLBACK_MAX)
}

fn derive_backend_hot_lines(scrollback: i64) -> i64 {
    clamp_backend_hot_lines(clamp_terminal_scrollback(scrollback) * 2)
}

pub fn sanitize_settings_value(raw: Value) -> Result<SanitizedSettings> {
    let saved_version = raw.get("version").and_then(Value::as_u64).unwrap_or(0) as u32;
    let mut migration_warnings = Vec::new();
    let mut validation_warnings = Vec::new();
    let mut settings = PersistedSettings::default().to_value();

    merge_json(&mut settings, &raw);
    if let Some(object) = settings.as_object_mut() {
        object.insert("version".to_string(), json!(SETTINGS_SCHEMA_VERSION));
    }
    migrate_ai_providers(&mut settings, &mut migration_warnings);

    if saved_version < SETTINGS_SCHEMA_VERSION
        && let Some(old_scrollback) = raw
            .get("terminal")
            .and_then(|terminal| terminal.get("scrollback"))
            .and_then(Value::as_i64)
    {
        if let Some(value) = get_path_mut(&mut settings, &["terminal", "scrollback"]) {
            *value = json!(old_scrollback.min(DEFAULT_TERMINAL_SCROLLBACK));
        }
        if let Some(value) = get_path_mut(&mut settings, &["buffer", "maxLines"]) {
            *value = json!(derive_backend_hot_lines(old_scrollback));
        }
        migration_warnings.push(
            "Migrated legacy terminal.scrollback into terminal.scrollback + buffer.maxLines"
                .to_string(),
        );
    }

    for (path, fallback, min, max) in [
        (
            "terminal.scrollback",
            DEFAULT_TERMINAL_SCROLLBACK,
            TERMINAL_SCROLLBACK_MIN,
            TERMINAL_SCROLLBACK_MAX,
        ),
        (
            "buffer.maxLines",
            DEFAULT_BACKEND_HOT_BUFFER_LINES,
            BACKEND_HOT_BUFFER_MIN,
            BACKEND_HOT_BUFFER_MAX,
        ),
        ("terminal.fontSize", 14, 8, 32),
        ("terminal.backgroundBlur", 0, 0, 20),
        ("appearance.borderRadius", 6, 0, 16),
        ("connectionDefaults.port", 22, 1, 65_535),
        ("sidebarUI.width", 300, 200, 600),
        ("sidebarUI.aiSidebarWidth", 340, 280, 500),
        ("sftp.maxConcurrentTransfers", 3, 1, 10),
        ("sftp.directoryParallelism", 4, 1, 16),
        ("sftp.speedLimitKBps", 0, 0, 10_000_000),
        ("reconnect.maxAttempts", 5, 1, 20),
        ("reconnect.baseDelayMs", 1000, 500, 10_000),
        ("reconnect.maxDelayMs", 15_000, 5_000, 60_000),
        ("connectionPool.idleTimeoutSecs", 1800, 60, 86_400),
        (
            "ai.toolUse.maxRounds",
            DEFAULT_AI_TOOL_MAX_ROUNDS,
            MIN_AI_TOOL_MAX_ROUNDS,
            MAX_AI_TOOL_MAX_ROUNDS,
        ),
        (
            "terminal.inBandTransfer.maxChunkBytes",
            1024 * 1024,
            64 * 1024,
            8 * 1024 * 1024,
        ),
        ("terminal.inBandTransfer.maxFileCount", 1024, 1, 10_000),
        (
            "terminal.inBandTransfer.maxTotalBytes",
            10 * 1024 * 1024 * 1024,
            100 * 1024 * 1024,
            100 * 1024 * 1024 * 1024,
        ),
    ] {
        let segments: Vec<_> = path.split('.').collect();
        if let Some(value) = get_path_mut(&mut settings, &segments) {
            clamp_i64(value, fallback, min, max, path, &mut validation_warnings);
        }
    }

    for (path, fallback, min, max) in [
        ("terminal.lineHeight", 1.2, 0.8, 3.0),
        ("terminal.backgroundOpacity", 0.15, 0.03, 0.5),
    ] {
        let segments: Vec<_> = path.split('.').collect();
        if let Some(value) = get_path_mut(&mut settings, &segments) {
            clamp_f64(value, fallback, min, max, path, &mut validation_warnings);
        }
    }

    sanitize_enum(
        &mut settings,
        &["general", "language"],
        &[
            "zh-CN", "en", "fr-FR", "ja", "es-ES", "pt-BR", "vi", "ko", "de", "it", "zh-TW",
        ],
        "zh-CN",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["general", "updateChannel"],
        &["stable", "beta"],
        "beta",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "fontFamily"],
        &[
            "jetbrains",
            "meslo",
            "maple",
            "cascadia",
            "consolas",
            "menlo",
            "custom",
        ],
        "jetbrains",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "cursorStyle"],
        &["block", "underline", "bar"],
        "block",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "renderer"],
        &["auto", "webgl", "canvas"],
        if cfg!(windows) { "canvas" } else { "auto" },
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "terminalEncoding"],
        &[
            "utf-8",
            "gbk",
            "gb18030",
            "big5",
            "shift_jis",
            "euc-jp",
            "euc-kr",
            "windows-1252",
        ],
        "utf-8",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "adaptiveRenderer"],
        &["auto", "always-60", "off"],
        "auto",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["terminal", "backgroundFit"],
        &["cover", "contain", "fill", "tile"],
        "cover",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["appearance", "uiDensity"],
        &["compact", "comfortable", "spacious"],
        "comfortable",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["appearance", "animationSpeed"],
        &["off", "reduced", "normal", "fast"],
        "normal",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["appearance", "frostedGlass"],
        &["off", "native", "system", "mica", "acrylic"],
        "off",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["appearance", "renderProfile"],
        &["auto", "quality", "low-power", "compatibility"],
        "auto",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["sftp", "conflictAction"],
        &["ask", "overwrite", "skip", "rename"],
        "ask",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["ide", "agentMode"],
        &["ask", "enabled", "disabled"],
        "ask",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["ai", "thinkingStyle"],
        &["detailed", "compact"],
        "detailed",
        &mut validation_warnings,
    );
    sanitize_enum(
        &mut settings,
        &["ai", "reasoningEffort"],
        &["none", "minimal", "low", "medium", "high", "xhigh", "auto"],
        "auto",
        &mut validation_warnings,
    );

    if let Some(terminal) = object_mut(&mut settings, "terminal")
        && let Some(in_band) = terminal
            .get_mut("inBandTransfer")
            .and_then(Value::as_object_mut)
    {
        in_band.insert("provider".to_string(), json!("trzsz"));
    }

    if let Some(value) = get_path_mut(&mut settings, &["terminal", "highlightRules"]) {
        *value = sanitize_highlight_rules_value(value);
    }

    let settings =
        serde_json::from_value(settings).context("sanitized settings did not match schema")?;
    Ok(SanitizedSettings {
        settings,
        migration_warnings,
        validation_warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_empty_ai_providers_to_tauri_builtin_defaults() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "providers": []
            }
        }))
        .expect("sanitize settings");

        let providers = sanitized.settings.ai.providers;
        assert_eq!(providers.len(), 5);
        assert_eq!(
            providers
                .first()
                .and_then(|provider| provider.get("id"))
                .and_then(Value::as_str),
            Some("builtin-openai")
        );
        assert_eq!(
            providers
                .iter()
                .find(
                    |provider| provider.get("id").and_then(Value::as_str) == Some("builtin-ollama")
                )
                .and_then(|provider| provider.get("enabled"))
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            sanitized.settings.ai.active_provider_id.as_deref(),
            Some("builtin-openai")
        );
        assert_eq!(
            sanitized.settings.ai.active_model.as_deref(),
            Some("gpt-4o-mini")
        );
    }

    #[test]
    fn migrates_legacy_custom_ai_base_url_first() {
        let sanitized = sanitize_settings_value(json!({
            "ai": {
                "baseUrl": "https://gateway.example/v1",
                "model": "gateway-model",
                "providers": []
            }
        }))
        .expect("sanitize settings");

        let first = sanitized
            .settings
            .ai
            .providers
            .first()
            .expect("first provider");
        assert_eq!(
            first.get("type").and_then(Value::as_str),
            Some("openai_compatible")
        );
        assert_eq!(
            first.get("baseUrl").and_then(Value::as_str),
            Some("https://gateway.example/v1")
        );
        assert_eq!(
            first
                .get("models")
                .and_then(Value::as_array)
                .and_then(|models| models.first())
                .and_then(Value::as_str),
            Some("gateway-model")
        );
        assert_eq!(
            sanitized.settings.ai.active_provider_id.as_deref(),
            first.get("id").and_then(Value::as_str)
        );
        assert_eq!(
            sanitized.settings.ai.active_model.as_deref(),
            Some("gateway-model")
        );
    }
}
