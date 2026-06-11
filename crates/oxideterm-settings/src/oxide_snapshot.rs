// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

use crate::{PersistedSettings, SETTINGS_SCHEMA_VERSION, sanitize_settings_value};

pub const OXIDE_SETTINGS_FORMAT: &str = "oxide-settings-sections-v1";
pub const OXIDE_SETTINGS_VERSION: u32 = 1;

pub const DEFAULT_OXIDE_SETTINGS_SECTIONS: &[&str] = &[
    "general",
    "terminalAppearance",
    "terminalBehavior",
    "appearance",
    "connections",
    "network",
    "fileAndEditor",
];
pub const ALL_OXIDE_SETTINGS_SECTIONS: &[&str] = &[
    "general",
    "terminalAppearance",
    "terminalBehavior",
    "appearance",
    "connections",
    "network",
    "fileAndEditor",
    "ai",
    "localTerminal",
    "nativePreferences",
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
    "commandMarks",
    "highlightRules",
    "inBandTransfer",
    "terminalEncoding",
    "graphics",
    "unicode",
];
const APPEARANCE_KEYS: &[&str] = &[
    "sidebarCollapsedDefault",
    "uiDensity",
    "borderRadius",
    "uiFontFamily",
    "animationSpeed",
    "frostedGlass",
    "renderProfile",
];
const CONNECTION_DEFAULT_KEYS: &[&str] = &["username", "port"];
const RECONNECT_KEYS: &[&str] = &["enabled", "maxAttempts", "baseDelayMs", "maxDelayMs"];
const CONNECTION_POOL_KEYS: &[&str] = &["idleTimeoutSecs"];
const NETWORK_KEYS: &[&str] = &["upstreamProxy", "upstreamProxyDisclaimerAccepted"];
const AI_KEYS: &[&str] = &[
    "enabled",
    "enabledConfirmed",
    "baseUrl",
    "model",
    "providers",
    "activeProviderId",
    "activeModel",
    "contextMaxChars",
    "contextVisibleLines",
    "thinkingStyle",
    "reasoningEffort",
    "reasoningProviderOverrides",
    "reasoningModelOverrides",
    "thinkingDefaultExpanded",
    "modelContextWindows",
    "userContextWindows",
    "customSystemPrompt",
    "memory",
    "modelMaxResponseTokens",
    "toolUse",
    "contextSources",
    "mcpServers",
    "acpAgents",
    "embeddingConfig",
    "agentRoles",
    "executionProfiles",
];
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
const NATIVE_PREFERENCES_KEYS: &[&str] = &[
    "keybindings",
    "customThemes",
    "launcher",
    "experimental",
    "newConnection",
];

pub fn export_oxide_settings_snapshot_json(
    settings: &PersistedSettings,
    selected_sections: Option<&HashSet<String>>,
    include_local_terminal_env_vars: bool,
) -> Result<String> {
    let source = settings.to_value();
    let mut partial = json!({ "version": SETTINGS_SCHEMA_VERSION });
    let section_ids = effective_section_ids(selected_sections);

    for section in &section_ids {
        copy_section(
            &source,
            &mut partial,
            section,
            include_local_terminal_env_vars,
        );
    }

    let envelope = json!({
        "format": OXIDE_SETTINGS_FORMAT,
        "version": OXIDE_SETTINGS_VERSION,
        "exportedAt": now_ms(),
        "sectionIds": section_ids,
        "settings": partial,
    });
    serde_json::to_string_pretty(&envelope).context("failed to serialize app settings snapshot")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub fn merge_oxide_settings_snapshot(
    current: &PersistedSettings,
    snapshot_json: &str,
    selected_sections: Option<&HashSet<String>>,
) -> Result<PersistedSettings> {
    let parsed: Value =
        serde_json::from_str(snapshot_json).context("failed to parse app settings snapshot")?;
    let (snapshot_settings, snapshot_sections) =
        if parsed.get("format").and_then(Value::as_str) == Some(OXIDE_SETTINGS_FORMAT) {
            let settings = parsed
                .get("settings")
                .cloned()
                .context("app settings snapshot is missing settings")?;
            let sections = parsed
                .get("sectionIds")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<HashSet<_>>()
                })
                .unwrap_or_else(|| {
                    DEFAULT_OXIDE_SETTINGS_SECTIONS
                        .iter()
                        .map(|s| s.to_string())
                        .collect()
                });
            (settings, sections)
        } else {
            (
                parsed,
                DEFAULT_OXIDE_SETTINGS_SECTIONS
                    .iter()
                    .map(|section| section.to_string())
                    .collect(),
            )
        };

    let requested = selected_sections
        .cloned()
        .unwrap_or_else(|| snapshot_sections.clone());
    let allowed = requested
        .intersection(&snapshot_sections)
        .cloned()
        .collect::<HashSet<_>>();
    let mut merged = current.to_value();
    for section in allowed {
        copy_section(&snapshot_settings, &mut merged, &section, false);
    }
    sanitize_settings_value(merged).map(|sanitized| sanitized.settings)
}

fn effective_section_ids(selected_sections: Option<&HashSet<String>>) -> Vec<String> {
    let mut sections = selected_sections
        .map(|selected| {
            ALL_OXIDE_SETTINGS_SECTIONS
                .iter()
                .filter(|section| selected.contains(**section))
                .map(|section| (*section).to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            DEFAULT_OXIDE_SETTINGS_SECTIONS
                .iter()
                .map(|section| (*section).to_string())
                .collect()
        });
    sections.retain(|section| !section.is_empty());
    sections
}

fn copy_section(
    source: &Value,
    target: &mut Value,
    section_id: &str,
    include_local_terminal_env_vars: bool,
) {
    match section_id {
        "general" => copy_object_keys(source, target, &["general"], GENERAL_KEYS),
        "terminalAppearance" => {
            copy_object_keys(source, target, &["terminal"], TERMINAL_APPEARANCE_KEYS)
        }
        "terminalBehavior" => {
            copy_object_keys(source, target, &["terminal"], TERMINAL_BEHAVIOR_KEYS)
        }
        "appearance" => copy_object_keys(source, target, &["appearance"], APPEARANCE_KEYS),
        "connections" => {
            copy_object_keys(
                source,
                target,
                &["connectionDefaults"],
                CONNECTION_DEFAULT_KEYS,
            );
            copy_object_keys(source, target, &["reconnect"], RECONNECT_KEYS);
            copy_object_keys(source, target, &["connectionPool"], CONNECTION_POOL_KEYS);
        }
        "network" => copy_object_keys(source, target, &["network"], NETWORK_KEYS),
        "ai" => copy_object_keys(source, target, &["ai"], AI_KEYS),
        "fileAndEditor" => {
            copy_sftp_keys(source, target);
            copy_object_keys(source, target, &["ide"], IDE_KEYS);
        }
        "localTerminal" => {
            copy_object_keys(source, target, &["localTerminal"], LOCAL_TERMINAL_KEYS);
            if include_local_terminal_env_vars {
                copy_object_keys(source, target, &["localTerminal"], &["customEnvVars"]);
            }
        }
        "nativePreferences" => copy_root_keys(source, target, NATIVE_PREFERENCES_KEYS),
        _ => {}
    }
}

fn copy_sftp_keys(source: &Value, target: &mut Value) {
    copy_object_keys(source, target, &["sftp"], SFTP_KEYS);

    let Some(source_obj) = get_path(source, &["sftp"]).and_then(Value::as_object) else {
        return;
    };
    let Some(value) = source_obj
        .get("speedLimitKBps")
        .or_else(|| source_obj.get("speedLimitKbps"))
    else {
        return;
    };

    // Older native settings may still contain serde's plain camelCase acronym.
    // Sectioned snapshots stay on Tauri's KBps spelling.
    ensure_object_path(target, &["sftp"]).insert("speedLimitKBps".to_string(), value.clone());
}

fn copy_object_keys(source: &Value, target: &mut Value, path: &[&str], keys: &[&str]) {
    let Some(source_obj) = get_path(source, path).and_then(Value::as_object) else {
        return;
    };
    let target_obj = ensure_object_path(target, path);
    for key in keys {
        if let Some(value) = source_obj.get(*key) {
            target_obj.insert((*key).to_string(), value.clone());
        }
    }
}

fn copy_root_keys(source: &Value, target: &mut Value, keys: &[&str]) {
    let Some(source_obj) = source.as_object() else {
        return;
    };
    let target_obj = target
        .as_object_mut()
        .expect("settings root must be an object");
    for key in keys {
        if let Some(value) = source_obj.get(*key) {
            target_obj.insert((*key).to_string(), value.clone());
        }
    }
}

fn get_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

fn ensure_object_path<'a>(value: &'a mut Value, path: &[&str]) -> &'a mut Map<String, Value> {
    let mut current = value;
    for segment in path {
        if !current.get(*segment).is_some_and(Value::is_object) {
            current
                .as_object_mut()
                .expect("settings root must be an object")
                .insert((*segment).to_string(), Value::Object(Map::new()));
        }
        current = current
            .as_object_mut()
            .expect("settings root must be an object")
            .get_mut(*segment)
            .expect("path segment was just inserted");
    }
    current
        .as_object_mut()
        .expect("path target must be an object")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Language, PersistedSettings};

    #[test]
    fn merge_sectioned_snapshot_applies_only_selected_sections() {
        let current = PersistedSettings::default();
        let snapshot = json!({
            "format": OXIDE_SETTINGS_FORMAT,
            "version": OXIDE_SETTINGS_VERSION,
            "sectionIds": ["general", "terminalAppearance", "ai"],
            "settings": {
                "general": { "language": "en" },
                "terminal": { "fontSize": 18 },
                "ai": { "enabled": true, "enabledConfirmed": true }
            }
        });
        let selected = ["general", "ai"]
            .into_iter()
            .map(str::to_string)
            .collect::<HashSet<_>>();

        let merged =
            merge_oxide_settings_snapshot(&current, &snapshot.to_string(), Some(&selected))
                .expect("merge");

        assert_eq!(merged.general.language, Language::En);
        assert_eq!(merged.terminal.font_size, current.terminal.font_size);
        assert!(merged.ai.enabled);
    }

    #[test]
    fn export_default_snapshot_uses_tauri_default_sections() {
        let mut settings = PersistedSettings::default();
        settings.ai.enabled = true;
        settings.local_terminal.default_cwd = Some("/tmp".to_string());

        let exported = export_oxide_settings_snapshot_json(&settings, None, false).expect("export");
        let parsed: Value = serde_json::from_str(&exported).expect("json");
        let section_ids = parsed["sectionIds"]
            .as_array()
            .expect("section ids")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();

        assert!(section_ids.contains(&"general"));
        assert!(section_ids.contains(&"network"));
        assert!(!section_ids.contains(&"ai"));
        assert!(!section_ids.contains(&"localTerminal"));
        assert!(parsed["settings"].get("ai").is_none());
        assert!(parsed["settings"].get("localTerminal").is_none());
        assert!(
            parsed["settings"]["terminal"]
                .get("terminalEncoding")
                .is_some()
        );
        assert!(parsed["settings"]["terminal"].get("commandMarks").is_some());
        assert!(parsed["settings"]["terminal"].get("graphics").is_some());
        assert!(parsed["settings"]["terminal"].get("unicode").is_some());
        assert!(
            parsed["settings"]["appearance"]
                .get("renderProfile")
                .is_some()
        );
        assert!(parsed["settings"].get("network").is_some());
        assert!(parsed["settings"]["sftp"].get("speedLimitKBps").is_some());
        assert!(parsed["settings"]["sftp"].get("speedLimitKbps").is_none());
    }

    #[test]
    fn export_selected_extended_sections() {
        let mut settings = PersistedSettings::default();
        settings.ai.enabled = true;
        settings.local_terminal.default_cwd = Some("/tmp".to_string());
        settings
            .local_terminal
            .custom_env_vars
            .insert("FOO".to_string(), Value::String("bar".to_string()));
        let selected = ["ai", "localTerminal", "nativePreferences"]
            .into_iter()
            .map(str::to_string)
            .collect::<HashSet<_>>();

        let exported =
            export_oxide_settings_snapshot_json(&settings, Some(&selected), false).expect("export");
        let parsed: Value = serde_json::from_str(&exported).expect("json");
        let section_ids = parsed["sectionIds"]
            .as_array()
            .expect("section ids")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();

        assert_eq!(
            section_ids,
            vec!["ai", "localTerminal", "nativePreferences"]
        );
        assert!(parsed["settings"].get("ai").is_some());
        assert!(parsed["settings"]["ai"].get("acpAgents").is_some());
        assert!(parsed["settings"]["ai"].get("executionProfiles").is_some());
        assert_eq!(
            parsed["settings"]["localTerminal"]["defaultCwd"].as_str(),
            Some("/tmp")
        );
        assert!(parsed["settings"].get("keybindings").is_some());
        assert!(parsed["settings"].get("customThemes").is_some());
        assert!(parsed["settings"].get("launcher").is_some());
        assert!(parsed["settings"].get("experimental").is_some());
        assert!(parsed["settings"].get("newConnection").is_some());
        assert!(
            parsed["settings"]["localTerminal"]
                .get("customEnvVars")
                .is_none()
        );

        let exported_with_env =
            export_oxide_settings_snapshot_json(&settings, Some(&selected), true).expect("export");
        let parsed_with_env: Value = serde_json::from_str(&exported_with_env).expect("json");
        assert_eq!(
            parsed_with_env["settings"]["localTerminal"]["customEnvVars"]["FOO"].as_str(),
            Some("bar")
        );
    }

    #[test]
    fn merge_sectioned_snapshot_accepts_tauri_sftp_speed_limit_key() {
        let current = PersistedSettings::default();
        let snapshot = json!({
            "format": OXIDE_SETTINGS_FORMAT,
            "version": OXIDE_SETTINGS_VERSION,
            "sectionIds": ["fileAndEditor"],
            "settings": {
                "sftp": {
                    "speedLimitEnabled": true,
                    "speedLimitKBps": 4096
                }
            }
        });

        let merged =
            merge_oxide_settings_snapshot(&current, &snapshot.to_string(), None).expect("merge");

        assert!(merged.sftp.speed_limit_enabled);
        assert_eq!(merged.sftp.speed_limit_kbps, 4096);
        assert!(!merged.sftp.extra.contains_key("speedLimitKBps"));
    }
}
