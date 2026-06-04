// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Application settings JSON persistence and validation.
//!
//! JSON remains the storage format, but settings rules live here so WebView,
//! import/export, portable mode, and future native UI share one authority.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;

const SETTINGS_FILENAME: &str = "settings.json";
const SETTINGS_SCHEMA_VERSION: u32 = 3;
const MAX_SETTINGS_FILE_BYTES: u64 = 2 * 1024 * 1024;
const DEFAULT_TERMINAL_SCROLLBACK: i64 = 1000;
const TERMINAL_SCROLLBACK_MIN: i64 = 500;
const TERMINAL_SCROLLBACK_MAX: i64 = 20_000;
const DEFAULT_BACKEND_HOT_BUFFER_LINES: i64 = 8_000;
const BACKEND_HOT_BUFFER_MIN: i64 = 5_000;
const BACKEND_HOT_BUFFER_MAX: i64 = 12_000;
const IN_BAND_TRANSFER_CHUNK_MIN: i64 = 64 * 1024;
const IN_BAND_TRANSFER_CHUNK_MAX: i64 = 8 * 1024 * 1024;
const IN_BAND_TRANSFER_FILE_COUNT_MIN: i64 = 1;
const IN_BAND_TRANSFER_FILE_COUNT_MAX: i64 = 10_000;
const IN_BAND_TRANSFER_TOTAL_BYTES_MIN: i64 = 100 * 1024 * 1024;
const IN_BAND_TRANSFER_TOTAL_BYTES_MAX: i64 = 100 * 1024 * 1024 * 1024;
const DEFAULT_AI_TOOL_MAX_ROUNDS: i64 = 10;
const MIN_AI_TOOL_MAX_ROUNDS: i64 = 1;
const MAX_AI_TOOL_MAX_ROUNDS: i64 = 30;

const OXIDE_APP_SETTINGS_ENVELOPE_FORMAT: &str = "oxide-settings-sections-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsEnvelope {
    pub version: u32,
    pub settings: Value,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsLoadResult {
    pub settings: Value,
    pub version: u32,
    pub updated_at: u64,
    pub migration_warnings: Vec<String>,
    pub validation_warnings: Vec<String>,
    pub migrated_from_legacy_local_storage: bool,
    pub recovered_from_corrupt_file: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSaveResult {
    pub settings: Value,
    pub version: u32,
    pub updated_at: u64,
    pub validation_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsValidationResult {
    pub settings: Value,
    pub version: u32,
    pub validation_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportAppSettingsSnapshotOptions {
    #[serde(default)]
    pub selected_sections: Option<Vec<String>>,
    #[serde(default)]
    pub include_local_terminal_env_vars: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyAppSettingsSnapshotOptions {
    #[serde(default)]
    pub selected_sections: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsImportResult {
    pub imported: bool,
    pub settings: Value,
    pub version: u32,
    pub updated_at: u64,
    pub migration_warnings: Vec<String>,
    pub validation_warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug)]
struct SanitizedSettings {
    settings: Value,
    migration_warnings: Vec<String>,
    validation_warnings: Vec<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn settings_path() -> Result<PathBuf, String> {
    crate::config::storage::config_dir()
        .map(|dir| dir.join(SETTINGS_FILENAME))
        .map_err(|err| err.to_string())
}

fn default_settings() -> Value {
    json!({
        "version": SETTINGS_SCHEMA_VERSION,
        "general": {
            "language": "zh-CN",
            "updateChannel": "beta"
        },
        "terminal": {
            "theme": "default",
            "fontFamily": "jetbrains",
            "customFontFamily": "",
            "fontSize": 14,
            "lineHeight": 1.2,
            "cursorStyle": "block",
            "cursorBlink": true,
            "scrollback": DEFAULT_TERMINAL_SCROLLBACK,
            "renderer": if cfg!(windows) { "canvas" } else { "auto" },
            "terminalEncoding": "utf-8",
            "adaptiveRenderer": "auto",
            "showFpsOverlay": false,
            "pasteProtection": true,
            "smartCopy": true,
            "osc52Clipboard": true,
            "copyOnSelect": false,
            "middleClickPaste": false,
            "selectionRequiresShift": false,
            "autosuggest": { "localShellHistory": true },
            "commandBar": {
                "enabled": true,
                "showLegacyToolbar": false,
                "gitStatus": true,
                "smartCompletion": true,
                "quickCommandsEnabled": true,
                "quickCommandsConfirmBeforeRun": false,
                "quickCommandsShowToast": true,
                "focusHandoffCommands": [
                    "vim", "nvim", "vi", "nano", "emacs", "less", "more",
                    "top", "htop", "btop", "yazi", "ranger", "lf", "lazygit",
                    "tmux", "screen", "ssh", "python", "node"
                ]
            },
            "commandMarks": {
                "enabled": true,
                "userInputObserved": false,
                "heuristicDetection": false,
                "showHoverActions": true
            },
            "backgroundEnabled": true,
            "backgroundImage": null,
            "backgroundOpacity": 0.15,
            "backgroundBlur": 0,
            "backgroundFit": "cover",
            "backgroundEnabledTabs": ["terminal", "local_terminal"],
            "highlightRules": [],
            "inBandTransfer": {
                "enabled": false,
                "provider": "trzsz",
                "allowDirectory": true,
                "maxChunkBytes": 1048576,
                "maxFileCount": 1024,
                "maxTotalBytes": 10737418240_i64
            }
        },
        "buffer": { "maxLines": DEFAULT_BACKEND_HOT_BUFFER_LINES },
        "appearance": {
            "sidebarCollapsedDefault": false,
            "uiDensity": "comfortable",
            "borderRadius": 6,
            "uiFontFamily": "",
            "animationSpeed": "normal",
            "frostedGlass": "off"
        },
        "connectionDefaults": {
            "username": "root",
            "port": 22
        },
        "treeUI": {
            "expandedIds": [],
            "focusedNodeId": null
        },
        "sidebarUI": {
            "collapsed": false,
            "activeSection": "sessions",
            "width": 300,
            "aiSidebarCollapsed": true,
            "aiSidebarWidth": 340,
            "zenMode": false
        },
        "ai": {
            "enabled": false,
            "enabledConfirmed": false,
            "baseUrl": "https://api.openai.com/v1",
            "model": "gpt-4o-mini",
            "providers": [],
            "activeProviderId": null,
            "activeModel": null,
            "contextMaxChars": 8000,
            "contextVisibleLines": 120,
            "thinkingStyle": "detailed",
            "reasoningEffort": "auto",
            "reasoningProviderOverrides": {},
            "reasoningModelOverrides": {},
            "thinkingDefaultExpanded": false,
            "customSystemPrompt": "",
            "memory": { "enabled": true, "content": "" },
            "toolUse": {
                "enabled": false,
                "maxRounds": DEFAULT_AI_TOOL_MAX_ROUNDS,
                "autoApproveTools": {
                    "list_targets": true,
                    "select_target": true,
                    "observe_terminal": true,
                    "read_resource": true,
                    "get_state": true,
                    "recall_preferences": true,
                    "connect_target": false,
                    "run_command": false,
                    "send_terminal_input": false,
                    "write_resource": false,
                    "write_resource:settings": false,
                    "write_resource:file": false,
                    "transfer_resource": false,
                    "open_app_surface": false,
                    "remember_preference": false
                },
                "disabledTools": []
            },
            "contextSources": { "ide": true, "sftp": true },
            "executionProfiles": {
                "defaultProfileId": "default",
                "profiles": [{
                    "id": "default",
                    "name": "Default",
                    "providerId": null,
                    "model": null,
                    "reasoningEffort": "auto",
                    "toolUse": {
                        "enabled": false,
                        "maxRounds": DEFAULT_AI_TOOL_MAX_ROUNDS,
                        "autoApproveTools": {},
                        "disabledTools": []
                    },
                    "context": {
                        "includeRuntimeChips": true,
                        "includeMemory": true,
                        "includeRag": true
                    },
                    "commandPolicy": {
                        "allow": [],
                        "deny": []
                    },
                    "createdAt": 0,
                    "updatedAt": 0
                }]
            }
        },
        "localTerminal": {
            "defaultShellId": null,
            "recentShellIds": [],
            "defaultCwd": null,
            "gitBashPath": null,
            "loadShellProfile": true,
            "ohMyPoshEnabled": false,
            "ohMyPoshTheme": null,
            "customEnvVars": {}
        },
        "sftp": {
            "maxConcurrentTransfers": 3,
            "directoryParallelism": 4,
            "speedLimitEnabled": false,
            "speedLimitKBps": 0,
            "conflictAction": "ask"
        },
        "ide": {
            "autoSave": false,
            "fontSize": null,
            "lineHeight": null,
            "agentMode": "ask",
            "wordWrap": false
        },
        "reconnect": {
            "enabled": true,
            "maxAttempts": 5,
            "baseDelayMs": 1000,
            "maxDelayMs": 15000
        },
        "connectionPool": {
            "idleTimeoutSecs": 1800
        },
        "experimental": {
            "virtualSessionProxy": false,
            "gpuCanvas": false
        },
        "onboardingCompleted": false
    })
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

fn clamp_backend_hot_lines(lines: i64) -> i64 {
    lines.clamp(BACKEND_HOT_BUFFER_MIN, BACKEND_HOT_BUFFER_MAX)
}

fn clamp_terminal_scrollback(lines: i64) -> i64 {
    lines.clamp(TERMINAL_SCROLLBACK_MIN, TERMINAL_SCROLLBACK_MAX)
}

fn derive_backend_hot_lines(scrollback: i64) -> i64 {
    clamp_backend_hot_lines(clamp_terminal_scrollback(scrollback) * 2)
}

fn object_mut<'a>(value: &'a mut Value, key: &str) -> Option<&'a mut Map<String, Value>> {
    value.get_mut(key).and_then(Value::as_object_mut)
}

fn get_path_mut<'a>(value: &'a mut Value, path: &[&str]) -> Option<&'a mut Value> {
    let mut current = value;
    for segment in path {
        current = current.get_mut(*segment)?;
    }
    Some(current)
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

fn sanitize_settings(raw: Value) -> SanitizedSettings {
    let saved_version = raw.get("version").and_then(Value::as_u64).unwrap_or(0) as u32;
    let mut migration_warnings = Vec::new();
    let mut validation_warnings = Vec::new();
    let mut settings = default_settings();

    merge_json(&mut settings, &raw);
    if let Some(object) = settings.as_object_mut() {
        object.insert("version".to_string(), json!(SETTINGS_SCHEMA_VERSION));
    }

    if saved_version < SETTINGS_SCHEMA_VERSION {
        if let Some(old_scrollback) = raw
            .get("terminal")
            .and_then(|terminal| terminal.get("scrollback"))
            .and_then(Value::as_i64)
        {
            let terminal_scrollback = old_scrollback.min(DEFAULT_TERMINAL_SCROLLBACK);
            if let Some(value) = get_path_mut(&mut settings, &["terminal", "scrollback"]) {
                *value = json!(terminal_scrollback);
            }
            if let Some(value) = get_path_mut(&mut settings, &["buffer", "maxLines"]) {
                *value = json!(derive_backend_hot_lines(old_scrollback));
            }
            migration_warnings.push(
                "Migrated legacy terminal.scrollback into terminal.scrollback + buffer.maxLines"
                    .to_string(),
            );
        }
    }

    if let Some(value) = get_path_mut(&mut settings, &["terminal", "scrollback"]) {
        clamp_i64(
            value,
            DEFAULT_TERMINAL_SCROLLBACK,
            TERMINAL_SCROLLBACK_MIN,
            TERMINAL_SCROLLBACK_MAX,
            "terminal.scrollback",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["buffer", "maxLines"]) {
        clamp_i64(
            value,
            DEFAULT_BACKEND_HOT_BUFFER_LINES,
            BACKEND_HOT_BUFFER_MIN,
            BACKEND_HOT_BUFFER_MAX,
            "buffer.maxLines",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["terminal", "fontSize"]) {
        clamp_i64(
            value,
            14,
            8,
            32,
            "terminal.fontSize",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["terminal", "lineHeight"]) {
        clamp_f64(
            value,
            1.2,
            0.8,
            3.0,
            "terminal.lineHeight",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["appearance", "borderRadius"]) {
        clamp_i64(
            value,
            6,
            0,
            16,
            "appearance.borderRadius",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["connectionDefaults", "port"]) {
        clamp_i64(
            value,
            22,
            1,
            65_535,
            "connectionDefaults.port",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["sftp", "maxConcurrentTransfers"]) {
        clamp_i64(
            value,
            3,
            1,
            10,
            "sftp.maxConcurrentTransfers",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["sftp", "directoryParallelism"]) {
        clamp_i64(
            value,
            4,
            1,
            16,
            "sftp.directoryParallelism",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["sftp", "speedLimitKBps"]) {
        clamp_i64(
            value,
            0,
            0,
            10_000_000,
            "sftp.speedLimitKBps",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["reconnect", "maxAttempts"]) {
        clamp_i64(
            value,
            5,
            1,
            20,
            "reconnect.maxAttempts",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["reconnect", "baseDelayMs"]) {
        clamp_i64(
            value,
            1000,
            500,
            10_000,
            "reconnect.baseDelayMs",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["reconnect", "maxDelayMs"]) {
        clamp_i64(
            value,
            15_000,
            5_000,
            60_000,
            "reconnect.maxDelayMs",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["connectionPool", "idleTimeoutSecs"]) {
        clamp_i64(
            value,
            1800,
            60,
            86_400,
            "connectionPool.idleTimeoutSecs",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(&mut settings, &["ai", "toolUse", "maxRounds"]) {
        clamp_i64(
            value,
            DEFAULT_AI_TOOL_MAX_ROUNDS,
            MIN_AI_TOOL_MAX_ROUNDS,
            MAX_AI_TOOL_MAX_ROUNDS,
            "ai.toolUse.maxRounds",
            &mut validation_warnings,
        );
    }

    if let Some(value) = get_path_mut(
        &mut settings,
        &["terminal", "inBandTransfer", "maxChunkBytes"],
    ) {
        clamp_i64(
            value,
            1024 * 1024,
            IN_BAND_TRANSFER_CHUNK_MIN,
            IN_BAND_TRANSFER_CHUNK_MAX,
            "terminal.inBandTransfer.maxChunkBytes",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(
        &mut settings,
        &["terminal", "inBandTransfer", "maxFileCount"],
    ) {
        clamp_i64(
            value,
            1024,
            IN_BAND_TRANSFER_FILE_COUNT_MIN,
            IN_BAND_TRANSFER_FILE_COUNT_MAX,
            "terminal.inBandTransfer.maxFileCount",
            &mut validation_warnings,
        );
    }
    if let Some(value) = get_path_mut(
        &mut settings,
        &["terminal", "inBandTransfer", "maxTotalBytes"],
    ) {
        clamp_i64(
            value,
            10 * 1024 * 1024 * 1024,
            IN_BAND_TRANSFER_TOTAL_BYTES_MIN,
            IN_BAND_TRANSFER_TOTAL_BYTES_MAX,
            "terminal.inBandTransfer.maxTotalBytes",
            &mut validation_warnings,
        );
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
        &["stable", "beta", "gpui-preview"],
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
        &["off", "css", "native"],
        "off",
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

    if let Some(object) = object_mut(&mut settings, "terminal") {
        if let Some(in_band) = object
            .get_mut("inBandTransfer")
            .and_then(Value::as_object_mut)
        {
            in_band.insert("provider".to_string(), json!("trzsz"));
        }
    }

    SanitizedSettings {
        settings,
        migration_warnings,
        validation_warnings,
    }
}

async fn write_envelope(path: &Path, envelope: &AppSettingsEnvelope) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|err| format!("Failed to create settings directory: {}", err))?;
    }
    let json = serde_json::to_vec_pretty(envelope)
        .map_err(|err| format!("Failed to serialize settings: {}", err))?;
    if json.len() as u64 > MAX_SETTINGS_FILE_BYTES {
        return Err("Settings snapshot exceeds size limit".to_string());
    }
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, json)
        .await
        .map_err(|err| format!("Failed to write settings temp file: {}", err))?;
    fs::rename(&temp_path, path)
        .await
        .map_err(|err| format!("Failed to replace settings file: {}", err))?;
    Ok(())
}

async fn save_sanitized(settings: Value) -> Result<AppSettingsEnvelope, String> {
    let envelope = AppSettingsEnvelope {
        version: SETTINGS_SCHEMA_VERSION,
        settings,
        updated_at: now_ms(),
    };
    write_envelope(&settings_path()?, &envelope).await?;
    Ok(envelope)
}

async fn backup_corrupt_file(path: &Path) -> Result<(), String> {
    let backup = path.with_extension(format!("corrupt.{}.json", now_ms()));
    fs::rename(path, &backup)
        .await
        .map_err(|err| format!("Failed to back up corrupt settings file: {}", err))
}

async fn read_settings_file(path: &Path) -> Result<Option<AppSettingsEnvelope>, String> {
    match fs::metadata(path).await {
        Ok(metadata) => {
            if metadata.len() > MAX_SETTINGS_FILE_BYTES {
                return Err("Settings file exceeds size limit".to_string());
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("Failed to stat settings file: {}", err)),
    }

    let contents = fs::read_to_string(path)
        .await
        .map_err(|err| format!("Failed to read settings file: {}", err))?;
    if contents.trim().is_empty() {
        return Ok(None);
    }

    let value: Value = serde_json::from_str(&contents)
        .map_err(|err| format!("Failed to parse settings file: {}", err))?;

    if value.get("settings").is_some() {
        let envelope: AppSettingsEnvelope = serde_json::from_value(value)
            .map_err(|err| format!("Failed to parse settings envelope: {}", err))?;
        return Ok(Some(envelope));
    }

    let sanitized = sanitize_settings(value);
    Ok(Some(AppSettingsEnvelope {
        version: SETTINGS_SCHEMA_VERSION,
        settings: sanitized.settings,
        updated_at: now_ms(),
    }))
}

#[tauri::command]
pub async fn load_app_settings(
    legacy_settings_json: Option<String>,
) -> Result<SettingsLoadResult, String> {
    let path = settings_path()?;
    let mut recovered_from_corrupt_file = false;
    let mut migrated_from_legacy_local_storage = false;
    let mut migration_warnings = Vec::new();
    let mut validation_warnings = Vec::new();

    let envelope = match read_settings_file(&path).await {
        Ok(Some(envelope)) => envelope,
        Ok(None) => {
            let raw = if let Some(legacy) = legacy_settings_json.as_deref() {
                migrated_from_legacy_local_storage = true;
                migration_warnings.push("Migrated settings from legacy localStorage".to_string());
                serde_json::from_str::<Value>(legacy).unwrap_or_else(|_| default_settings())
            } else {
                default_settings()
            };
            let sanitized = sanitize_settings(raw);
            migration_warnings.extend(sanitized.migration_warnings);
            validation_warnings.extend(sanitized.validation_warnings);
            save_sanitized(sanitized.settings).await?
        }
        Err(err) => {
            recovered_from_corrupt_file = true;
            if path.exists() {
                let _ = backup_corrupt_file(&path).await;
            }
            migration_warnings.push(format!("Recovered from unreadable settings file: {}", err));
            save_sanitized(default_settings()).await?
        }
    };

    let sanitized = sanitize_settings(envelope.settings.clone());
    migration_warnings.extend(sanitized.migration_warnings);
    validation_warnings.extend(sanitized.validation_warnings);
    let envelope =
        if sanitized.settings != envelope.settings || envelope.version != SETTINGS_SCHEMA_VERSION {
            save_sanitized(sanitized.settings).await?
        } else {
            envelope
        };

    Ok(SettingsLoadResult {
        settings: envelope.settings,
        version: SETTINGS_SCHEMA_VERSION,
        updated_at: envelope.updated_at,
        migration_warnings,
        validation_warnings,
        migrated_from_legacy_local_storage,
        recovered_from_corrupt_file,
    })
}

#[tauri::command]
pub async fn save_app_settings(settings: Value) -> Result<SettingsSaveResult, String> {
    let sanitized = sanitize_settings(settings);
    let envelope = save_sanitized(sanitized.settings).await?;
    Ok(SettingsSaveResult {
        settings: envelope.settings,
        version: envelope.version,
        updated_at: envelope.updated_at,
        validation_warnings: sanitized.validation_warnings,
    })
}

#[tauri::command]
pub async fn validate_app_settings(settings: Value) -> Result<SettingsValidationResult, String> {
    let sanitized = sanitize_settings(settings);
    Ok(SettingsValidationResult {
        settings: sanitized.settings,
        version: SETTINGS_SCHEMA_VERSION,
        validation_warnings: sanitized.validation_warnings,
    })
}

#[tauri::command]
pub async fn reset_app_settings() -> Result<SettingsLoadResult, String> {
    let envelope = save_sanitized(default_settings()).await?;
    Ok(SettingsLoadResult {
        settings: envelope.settings,
        version: envelope.version,
        updated_at: envelope.updated_at,
        migration_warnings: Vec::new(),
        validation_warnings: Vec::new(),
        migrated_from_legacy_local_storage: false,
        recovered_from_corrupt_file: false,
    })
}

const APP_SETTINGS_SECTION_IDS: &[&str] = &[
    "general",
    "terminalAppearance",
    "terminalBehavior",
    "appearance",
    "connections",
    "fileAndEditor",
    "ai",
    "localTerminal",
];

const DEFAULT_EXPORT_SECTIONS: &[&str] = &[
    "general",
    "terminalAppearance",
    "terminalBehavior",
    "appearance",
    "connections",
    "fileAndEditor",
];

const GENERAL_KEYS: &[&str] = &["language", "updateChannel"];
const TERMINAL_APPEARANCE_KEYS: &[&str] = &[
    "theme",
    "fontFamily",
    "customFontFamily",
    "fontSize",
    "lineHeight",
    "cursorStyle",
    "cursorBlink",
    "backgroundEnabled",
    "backgroundImage",
    "backgroundOpacity",
    "backgroundBlur",
    "backgroundFit",
    "backgroundEnabledTabs",
];
const TERMINAL_BEHAVIOR_KEYS: &[&str] = &[
    "scrollback",
    "renderer",
    "adaptiveRenderer",
    "showFpsOverlay",
    "pasteProtection",
    "smartCopy",
    "osc52Clipboard",
    "copyOnSelect",
    "middleClickPaste",
    "selectionRequiresShift",
    "autosuggest",
    "commandBar",
    "highlightRules",
    "inBandTransfer",
];
const APPEARANCE_KEYS: &[&str] = &[
    "sidebarCollapsedDefault",
    "uiDensity",
    "borderRadius",
    "uiFontFamily",
    "animationSpeed",
    "frostedGlass",
];
const CONNECTION_DEFAULT_KEYS: &[&str] = &["username", "port"];
const RECONNECT_KEYS: &[&str] = &["enabled", "maxAttempts", "baseDelayMs", "maxDelayMs"];
const CONNECTION_POOL_KEYS: &[&str] = &["idleTimeoutSecs"];
const SFTP_KEYS: &[&str] = &[
    "maxConcurrentTransfers",
    "directoryParallelism",
    "speedLimitEnabled",
    "speedLimitKBps",
    "conflictAction",
];
const IDE_KEYS: &[&str] = &[
    "autoSave",
    "fontSize",
    "lineHeight",
    "agentMode",
    "wordWrap",
];
const LOCAL_TERMINAL_KEYS: &[&str] = &[
    "defaultShellId",
    "recentShellIds",
    "defaultCwd",
    "gitBashPath",
    "loadShellProfile",
    "ohMyPoshEnabled",
    "ohMyPoshTheme",
];

fn pick_fields(source: Option<&Map<String, Value>>, keys: &[&str]) -> Option<Value> {
    let source = source?;
    let mut result = Map::new();
    for key in keys {
        if let Some(value) = source.get(*key) {
            result.insert((*key).to_string(), value.clone());
        }
    }
    (!result.is_empty()).then_some(Value::Object(result))
}

fn merge_object_field(target: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(Value::Object(value)) = value {
        let entry = target
            .entry(key.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(object) = entry.as_object_mut() {
            for (field, field_value) in value {
                object.insert(field, field_value);
            }
        }
    }
}

fn selected_sections(options: Option<ExportAppSettingsSnapshotOptions>) -> Vec<String> {
    let raw = options
        .and_then(|options| options.selected_sections)
        .filter(|sections| !sections.is_empty())
        .unwrap_or_else(|| {
            DEFAULT_EXPORT_SECTIONS
                .iter()
                .map(|item| item.to_string())
                .collect()
        });
    let mut seen = Vec::new();
    for section in raw {
        if APP_SETTINGS_SECTION_IDS.contains(&section.as_str()) && !seen.contains(&section) {
            seen.push(section);
        }
    }
    seen
}

fn build_sectioned_snapshot(
    settings: &Value,
    options: Option<ExportAppSettingsSnapshotOptions>,
) -> Option<Value> {
    let include_env = options
        .as_ref()
        .and_then(|item| item.include_local_terminal_env_vars)
        .unwrap_or(false);
    let sections = selected_sections(options);
    if sections.is_empty() {
        return None;
    }

    let root = settings.as_object()?;
    let mut partial = Map::new();

    for section in &sections {
        match section.as_str() {
            "general" => merge_object_field(
                &mut partial,
                "general",
                pick_fields(root.get("general").and_then(Value::as_object), GENERAL_KEYS),
            ),
            "terminalAppearance" => merge_object_field(
                &mut partial,
                "terminal",
                pick_fields(
                    root.get("terminal").and_then(Value::as_object),
                    TERMINAL_APPEARANCE_KEYS,
                ),
            ),
            "terminalBehavior" => merge_object_field(
                &mut partial,
                "terminal",
                pick_fields(
                    root.get("terminal").and_then(Value::as_object),
                    TERMINAL_BEHAVIOR_KEYS,
                ),
            ),
            "appearance" => merge_object_field(
                &mut partial,
                "appearance",
                pick_fields(
                    root.get("appearance").and_then(Value::as_object),
                    APPEARANCE_KEYS,
                ),
            ),
            "connections" => {
                merge_object_field(
                    &mut partial,
                    "connectionDefaults",
                    pick_fields(
                        root.get("connectionDefaults").and_then(Value::as_object),
                        CONNECTION_DEFAULT_KEYS,
                    ),
                );
                merge_object_field(
                    &mut partial,
                    "reconnect",
                    pick_fields(
                        root.get("reconnect").and_then(Value::as_object),
                        RECONNECT_KEYS,
                    ),
                );
                merge_object_field(
                    &mut partial,
                    "connectionPool",
                    pick_fields(
                        root.get("connectionPool").and_then(Value::as_object),
                        CONNECTION_POOL_KEYS,
                    ),
                );
            }
            "fileAndEditor" => {
                merge_object_field(
                    &mut partial,
                    "sftp",
                    pick_fields(root.get("sftp").and_then(Value::as_object), SFTP_KEYS),
                );
                merge_object_field(
                    &mut partial,
                    "ide",
                    pick_fields(root.get("ide").and_then(Value::as_object), IDE_KEYS),
                );
            }
            "ai" => merge_object_field(&mut partial, "ai", root.get("ai").cloned()),
            "localTerminal" => {
                if let Some(local) = root.get("localTerminal").and_then(Value::as_object) {
                    let mut value = pick_fields(Some(local), LOCAL_TERMINAL_KEYS)
                        .and_then(|value| value.as_object().cloned())
                        .unwrap_or_default();
                    if include_env {
                        if let Some(env) = local.get("customEnvVars") {
                            value.insert("customEnvVars".to_string(), env.clone());
                        }
                    }
                    if !value.is_empty() {
                        partial.insert("localTerminal".to_string(), Value::Object(value));
                    }
                }
            }
            _ => {}
        }
    }

    Some(json!({
        "format": OXIDE_APP_SETTINGS_ENVELOPE_FORMAT,
        "version": 1,
        "sectionIds": sections,
        "settings": partial,
    }))
}

fn parse_settings_snapshot(snapshot_json: &str) -> Result<(bool, Vec<String>, Value), String> {
    let parsed: Value = serde_json::from_str(snapshot_json)
        .map_err(|err| format!("Failed to parse app settings snapshot: {}", err))?;
    if parsed.get("format").and_then(Value::as_str) == Some(OXIDE_APP_SETTINGS_ENVELOPE_FORMAT) {
        let sections = parsed
            .get("sectionIds")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|id| APP_SETTINGS_SECTION_IDS.contains(id))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let settings = parsed
            .get("settings")
            .cloned()
            .ok_or_else(|| "Sectioned app settings snapshot is missing settings".to_string())?;
        return Ok((true, sections, settings));
    }
    Ok((false, vec!["legacy".to_string()], parsed))
}

fn merge_selected_import_sections(current: Value, imported: Value, selected: &[String]) -> Value {
    let mut next = current;
    let Some(imported_root) = imported.as_object() else {
        return next;
    };

    for section in selected {
        match section.as_str() {
            "general" => merge_object_field(
                next.as_object_mut().unwrap(),
                "general",
                imported_root.get("general").cloned(),
            ),
            "terminalAppearance" => merge_object_field(
                next.as_object_mut().unwrap(),
                "terminal",
                pick_fields(
                    imported_root.get("terminal").and_then(Value::as_object),
                    TERMINAL_APPEARANCE_KEYS,
                ),
            ),
            "terminalBehavior" => merge_object_field(
                next.as_object_mut().unwrap(),
                "terminal",
                pick_fields(
                    imported_root.get("terminal").and_then(Value::as_object),
                    TERMINAL_BEHAVIOR_KEYS,
                ),
            ),
            "appearance" => merge_object_field(
                next.as_object_mut().unwrap(),
                "appearance",
                imported_root.get("appearance").cloned(),
            ),
            "connections" => {
                let root = next.as_object_mut().unwrap();
                merge_object_field(
                    root,
                    "connectionDefaults",
                    imported_root.get("connectionDefaults").cloned(),
                );
                merge_object_field(root, "reconnect", imported_root.get("reconnect").cloned());
                merge_object_field(
                    root,
                    "connectionPool",
                    imported_root.get("connectionPool").cloned(),
                );
            }
            "fileAndEditor" => {
                let root = next.as_object_mut().unwrap();
                merge_object_field(root, "sftp", imported_root.get("sftp").cloned());
                merge_object_field(root, "ide", imported_root.get("ide").cloned());
            }
            "ai" => merge_object_field(
                next.as_object_mut().unwrap(),
                "ai",
                imported_root.get("ai").cloned(),
            ),
            "localTerminal" => merge_object_field(
                next.as_object_mut().unwrap(),
                "localTerminal",
                imported_root.get("localTerminal").cloned(),
            ),
            _ => {}
        }
    }
    next
}

#[tauri::command]
pub async fn export_app_settings_snapshot(
    options: Option<ExportAppSettingsSnapshotOptions>,
) -> Result<Option<String>, String> {
    let settings = load_app_settings(None).await?.settings;
    let Some(snapshot) = build_sectioned_snapshot(&settings, options) else {
        return Ok(None);
    };
    serde_json::to_string(&snapshot)
        .map(Some)
        .map_err(|err| format!("Failed to serialize app settings snapshot: {}", err))
}

#[tauri::command]
pub async fn apply_app_settings_snapshot(
    snapshot_json: String,
    options: Option<ApplyAppSettingsSnapshotOptions>,
) -> Result<SettingsImportResult, String> {
    let mut errors = Vec::new();
    let (sectioned, snapshot_sections, imported) = match parse_settings_snapshot(&snapshot_json) {
        Ok(parsed) => parsed,
        Err(err) => {
            return Ok(SettingsImportResult {
                imported: false,
                settings: load_app_settings(None).await?.settings,
                version: SETTINGS_SCHEMA_VERSION,
                updated_at: now_ms(),
                migration_warnings: Vec::new(),
                validation_warnings: Vec::new(),
                errors: vec![err],
            });
        }
    };

    let selected = options
        .and_then(|options| options.selected_sections)
        .filter(|sections| !sections.is_empty())
        .unwrap_or_else(|| snapshot_sections.clone());
    if selected.is_empty() {
        let loaded = load_app_settings(None).await?;
        return Ok(SettingsImportResult {
            imported: false,
            settings: loaded.settings,
            version: loaded.version,
            updated_at: loaded.updated_at,
            migration_warnings: loaded.migration_warnings,
            validation_warnings: loaded.validation_warnings,
            errors,
        });
    }

    let current = load_app_settings(None).await?.settings;
    let merged = if sectioned {
        let selected = selected
            .into_iter()
            .filter(|section| snapshot_sections.contains(section))
            .collect::<Vec<_>>();
        if selected.is_empty() {
            errors.push("No selected app settings sections are present in snapshot".to_string());
            current
        } else {
            merge_selected_import_sections(current, imported, &selected)
        }
    } else {
        imported
    };

    let sanitized = sanitize_settings(merged);
    let envelope = save_sanitized(sanitized.settings).await?;
    Ok(SettingsImportResult {
        imported: errors.is_empty(),
        settings: envelope.settings,
        version: envelope.version,
        updated_at: envelope.updated_at,
        migration_warnings: sanitized.migration_warnings,
        validation_warnings: sanitized.validation_warnings,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_terminal_scrollback_and_buffer_lines() {
        let sanitized = sanitize_settings(json!({
            "version": SETTINGS_SCHEMA_VERSION,
            "terminal": { "scrollback": 999999 },
            "buffer": { "maxLines": 1 }
        }));
        assert_eq!(
            sanitized.settings["terminal"]["scrollback"],
            json!(TERMINAL_SCROLLBACK_MAX)
        );
        assert_eq!(
            sanitized.settings["buffer"]["maxLines"],
            json!(BACKEND_HOT_BUFFER_MIN)
        );
        assert!(!sanitized.validation_warnings.is_empty());
    }

    #[test]
    fn migrates_legacy_scrollback_to_backend_buffer() {
        let sanitized = sanitize_settings(json!({
            "version": 2,
            "terminal": { "scrollback": 5000 }
        }));
        assert_eq!(sanitized.settings["terminal"]["scrollback"], json!(1000));
        assert_eq!(sanitized.settings["buffer"]["maxLines"], json!(10000));
        assert!(!sanitized.migration_warnings.is_empty());
    }

    #[test]
    fn preserves_gpui_preview_update_channel() {
        let sanitized = sanitize_settings(json!({
            "version": SETTINGS_SCHEMA_VERSION,
            "general": { "updateChannel": "gpui-preview" }
        }));
        assert_eq!(
            sanitized.settings["general"]["updateChannel"],
            json!("gpui-preview")
        );
        assert!(sanitized.validation_warnings.is_empty());
    }

    #[test]
    fn exports_sectioned_snapshot() {
        let snapshot = build_sectioned_snapshot(
            &default_settings(),
            Some(ExportAppSettingsSnapshotOptions {
                selected_sections: Some(vec![
                    "general".to_string(),
                    "terminalBehavior".to_string(),
                ]),
                include_local_terminal_env_vars: None,
            }),
        )
        .unwrap();
        assert_eq!(
            snapshot["format"],
            json!(OXIDE_APP_SETTINGS_ENVELOPE_FORMAT)
        );
        assert!(snapshot["settings"]["general"]["language"].is_string());
        assert!(snapshot["settings"]["terminal"]["scrollback"].is_number());
        assert!(snapshot["settings"]["terminal"]["fontFamily"].is_null());
    }
}
