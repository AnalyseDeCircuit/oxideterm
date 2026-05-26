// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    migration::legacy_local_storage_value,
    model::{PersistedSettings, SETTINGS_SCHEMA_VERSION},
    normalize::sanitize_settings_value,
};

pub const SETTINGS_FILENAME: &str = "settings.json";
const MAX_SETTINGS_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsEnvelope {
    pub version: u32,
    pub settings: PersistedSettings,
    pub updated_at: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsLoadResult {
    pub settings: PersistedSettings,
    pub version: u32,
    pub updated_at: u64,
    pub migration_warnings: Vec<String>,
    pub validation_warnings: Vec<String>,
    pub migrated_from_legacy_local_storage: bool,
    pub recovered_from_corrupt_file: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsSaveResult {
    pub settings: PersistedSettings,
    pub version: u32,
    pub updated_at: u64,
    pub validation_warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SettingsStore {
    path: PathBuf,
    settings: PersistedSettings,
    updated_at: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub fn default_settings_path() -> PathBuf {
    if cfg!(windows) {
        if let Some(config_home) = std::env::var_os("APPDATA") {
            return PathBuf::from(config_home)
                .join("OxideTerm")
                .join(SETTINGS_FILENAME);
        }
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".oxideterm")
            .join(SETTINGS_FILENAME);
    }

    PathBuf::from(SETTINGS_FILENAME)
}

pub fn save_settings_to_path(
    path: &Path,
    settings: PersistedSettings,
) -> Result<SettingsSaveResult> {
    // Non-GPUI writers share the same sanitize-and-envelope path as SettingsStore::save.
    let sanitized = sanitize_settings_value(settings.to_value())?;
    let updated_at = now_ms();
    write_envelope(path, &sanitized.settings, updated_at)?;
    Ok(SettingsSaveResult {
        settings: sanitized.settings,
        version: SETTINGS_SCHEMA_VERSION,
        updated_at,
        validation_warnings: sanitized.validation_warnings,
    })
}

fn read_envelope(path: &Path) -> Result<Option<(Value, u64)>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(anyhow!("failed to stat settings file: {}", err)),
    };
    if metadata.len() > MAX_SETTINGS_FILE_BYTES {
        return Err(anyhow!("settings file exceeds size limit"));
    }
    let contents = fs::read_to_string(path).context("failed to read settings file")?;
    if contents.trim().is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(&contents).context("failed to parse settings file")?;
    if value.get("settings").is_some() {
        let updated_at = value
            .get("updatedAt")
            .and_then(Value::as_u64)
            .unwrap_or_else(now_ms);
        Ok(Some((value["settings"].clone(), updated_at)))
    } else {
        Ok(Some((value, now_ms())))
    }
}

fn write_envelope(path: &Path, settings: &PersistedSettings, updated_at: u64) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create settings directory")?;
    }
    let envelope = SettingsEnvelope {
        version: SETTINGS_SCHEMA_VERSION,
        settings: settings.clone(),
        updated_at,
    };
    let json = serde_json::to_vec_pretty(&envelope).context("failed to serialize settings")?;
    if json.len() as u64 > MAX_SETTINGS_FILE_BYTES {
        return Err(anyhow!("settings snapshot exceeds size limit"));
    }
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, json).context("failed to write settings temp file")?;
    fs::rename(&temp_path, path).context("failed to replace settings file")?;
    Ok(())
}

impl SettingsStore {
    pub fn from_read_only(path: impl Into<PathBuf>, settings: PersistedSettings) -> Self {
        // CLI previews need the same in-memory shape as SettingsStore without triggering
        // migrations or envelope rewrites in the user's settings directory.
        Self {
            path: path.into(),
            settings,
            updated_at: 0,
        }
    }

    pub fn load_default() -> Result<Self> {
        Self::load_from_path(default_settings_path(), None)
    }

    pub fn load_from_path(
        path: impl Into<PathBuf>,
        legacy_local_storage: Option<&HashMap<String, String>>,
    ) -> Result<Self> {
        let path = path.into();
        let load = load_settings_from_path(&path, legacy_local_storage)?;
        Ok(Self {
            path,
            settings: load.settings,
            updated_at: load.updated_at,
        })
    }

    pub fn settings(&self) -> &PersistedSettings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut PersistedSettings {
        &mut self.settings
    }

    pub fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save(&mut self) -> Result<SettingsSaveResult> {
        let saved = save_settings_to_path(&self.path, self.settings.clone())?;
        self.settings = saved.settings.clone();
        self.updated_at = saved.updated_at;
        Ok(saved)
    }

    pub fn replace_and_save(&mut self, settings: PersistedSettings) -> Result<SettingsSaveResult> {
        self.settings = settings;
        self.save()
    }
}

pub fn load_settings_from_path(
    path: &Path,
    legacy_local_storage: Option<&HashMap<String, String>>,
) -> Result<SettingsLoadResult> {
    let mut migrated_from_legacy_local_storage = false;
    let mut recovered_from_corrupt_file = false;
    let mut migration_warnings = Vec::new();
    let mut validation_warnings = Vec::new();

    let (raw, updated_at) = match read_envelope(path) {
        Ok(Some((raw, updated_at))) => (raw, updated_at),
        Ok(None) => {
            let raw = if let Some(entries) = legacy_local_storage {
                migrated_from_legacy_local_storage = true;
                migration_warnings.push("Migrated settings from frontend localStorage".to_string());
                legacy_local_storage_value(entries)
            } else {
                PersistedSettings::default().to_value()
            };
            (raw, now_ms())
        }
        Err(err) => {
            recovered_from_corrupt_file = true;
            migration_warnings.push(format!("Recovered from unreadable settings file: {}", err));
            (PersistedSettings::default().to_value(), now_ms())
        }
    };

    let sanitized = sanitize_settings_value(raw)?;
    migration_warnings.extend(sanitized.migration_warnings);
    validation_warnings.extend(sanitized.validation_warnings);
    write_envelope(path, &sanitized.settings, updated_at)?;

    Ok(SettingsLoadResult {
        settings: sanitized.settings,
        version: SETTINGS_SCHEMA_VERSION,
        updated_at,
        migration_warnings,
        validation_warnings,
        migrated_from_legacy_local_storage,
        recovered_from_corrupt_file,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;
    use crate::{
        APP_LANG_KEY, CUSTOM_THEMES_KEY, KEYBINDINGS_KEY, LAUNCHER_ENABLED_KEY,
        LEGACY_FOCUSED_NODE_KEY, LEGACY_TREE_EXPANDED_KEY, LEGACY_UI_STATE_KEY,
        NEW_CONNECTION_SAVE_KEY, RenderProfile, SETTINGS_STORAGE_KEY,
        model::{ConflictAction, FontFamily, IdeAgentMode, Language, RendererType},
    };

    #[test]
    fn defaults_match_tauri_settings_store_values() {
        let settings = PersistedSettings::default();
        assert_eq!(settings.version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(settings.general.language, Language::ZhCn);
        assert_eq!(settings.terminal.theme, "default");
        assert_eq!(settings.terminal.font_family, FontFamily::Jetbrains);
        assert_eq!(settings.terminal.font_size, 14);
        assert_eq!(settings.terminal.line_height, 1.2);
        assert_eq!(settings.terminal.scrollback, 1000);
        assert_eq!(settings.terminal.renderer, RendererType::default());
        assert_eq!(settings.buffer.max_lines, 8000);
        assert_eq!(settings.appearance.border_radius, 6);
        assert_eq!(settings.appearance.render_profile, RenderProfile::Auto);
        assert_eq!(settings.connection_defaults.username, "root");
        assert_eq!(settings.sidebar_ui.width, 300);
        assert_eq!(settings.sftp.max_concurrent_transfers, 3);
        assert_eq!(settings.sftp.directory_parallelism, 4);
        assert_eq!(settings.sftp.conflict_action, ConflictAction::Ask);
        assert_eq!(settings.ide.agent_mode, IdeAgentMode::Ask);
        assert!(!settings.ide.auto_save);
        assert!(settings.reconnect.enabled);
        assert_eq!(settings.reconnect.max_attempts, 5);
        assert_eq!(settings.reconnect.base_delay_ms, 1000);
        assert_eq!(settings.reconnect.max_delay_ms, 15_000);
        assert_eq!(settings.connection_pool.idle_timeout_secs, 1800);
        assert!(!settings.experimental.virtual_session_proxy);
        assert!(!settings.experimental.gpu_canvas);
    }

    #[test]
    fn enums_serialize_to_tauri_strings() {
        let settings = PersistedSettings::default();
        let value = settings.to_value();
        assert_eq!(value["general"]["language"], "zh-CN");
        assert_eq!(value["terminal"]["fontFamily"], "jetbrains");
        assert_eq!(
            value["terminal"]["renderer"],
            if cfg!(windows) { "canvas" } else { "auto" }
        );
        assert_eq!(value["terminal"]["terminalEncoding"], "utf-8");
        assert_eq!(value["appearance"]["uiDensity"], "comfortable");
        assert_eq!(value["appearance"]["renderProfile"], "auto");
        assert_eq!(value["sftp"]["conflictAction"], "ask");
        assert_eq!(value["ide"]["agentMode"], "ask");
        assert_eq!(value["reconnect"]["baseDelayMs"], 1000);
        assert_eq!(value["reconnect"]["maxDelayMs"], 15_000);
        assert_eq!(value["connectionPool"]["idleTimeoutSecs"], 1800);
        assert_eq!(value["experimental"]["virtualSessionProxy"], false);
    }

    #[test]
    fn invalid_numeric_values_normalize_safely() {
        let raw = json!({
            "terminal": {
                "scrollback": 1,
                "fontSize": 99,
                "lineHeight": 9.0,
                "inBandTransfer": {
                    "maxChunkBytes": 1,
                    "maxFileCount": 999999,
                    "maxTotalBytes": 1
                }
            },
            "sidebarUI": { "width": 9999 },
            "connectionPool": { "idleTimeoutSecs": 1 }
        });
        let sanitized = sanitize_settings_value(raw).unwrap();
        assert_eq!(sanitized.settings.terminal.scrollback, 500);
        assert_eq!(sanitized.settings.terminal.font_size, 32);
        assert_eq!(sanitized.settings.terminal.line_height, 3.0);
        assert_eq!(sanitized.settings.sidebar_ui.width, 600);
        assert_eq!(sanitized.settings.connection_pool.idle_timeout_secs, 60);
        assert!(!sanitized.validation_warnings.is_empty());
    }

    #[test]
    fn serde_round_trip_preserves_stable_and_future_fields() {
        let raw = json!({
            "terminal": {
                "fontFamily": "menlo",
                "futureTerminalFlag": true
            },
            "futureTopLevel": { "enabled": true }
        });
        let sanitized = sanitize_settings_value(raw).unwrap();
        let value = sanitized.settings.to_value();
        assert_eq!(value["terminal"]["fontFamily"], "menlo");
        assert_eq!(value["terminal"]["futureTerminalFlag"], true);
        assert_eq!(value["futureTopLevel"]["enabled"], true);
    }

    #[test]
    fn legacy_local_storage_fixture_migrates_into_schema() {
        let mut entries = HashMap::new();
        entries.insert(
            SETTINGS_STORAGE_KEY.to_string(),
            json!({ "terminal": { "fontSize": 16 } }).to_string(),
        );
        entries.insert(APP_LANG_KEY.to_string(), "it".to_string());
        entries.insert(
            LEGACY_TREE_EXPANDED_KEY.to_string(),
            json!(["root", "node-a"]).to_string(),
        );
        entries.insert(LEGACY_FOCUSED_NODE_KEY.to_string(), "node-a".to_string());
        entries.insert(
            LEGACY_UI_STATE_KEY.to_string(),
            json!({ "sidebarCollapsed": true, "sidebarWidth": 420 }).to_string(),
        );
        entries.insert(
            KEYBINDINGS_KEY.to_string(),
            json!({ "terminal.copy": { "mac": { "key": "c", "ctrl": false, "shift": false, "alt": false, "meta": true } } }).to_string(),
        );
        entries.insert(
            CUSTOM_THEMES_KEY.to_string(),
            json!({ "custom-dark": { "name": "Custom Dark" } }).to_string(),
        );
        entries.insert(LAUNCHER_ENABLED_KEY.to_string(), "true".to_string());
        entries.insert(NEW_CONNECTION_SAVE_KEY.to_string(), "true".to_string());

        let raw = legacy_local_storage_value(&entries);
        let sanitized = sanitize_settings_value(raw).unwrap();
        assert_eq!(sanitized.settings.terminal.font_size, 16);
        assert_eq!(sanitized.settings.general.language, Language::It);
        assert_eq!(sanitized.settings.tree_ui.expanded_ids, ["root", "node-a"]);
        assert_eq!(
            sanitized.settings.tree_ui.focused_node_id.as_deref(),
            Some("node-a")
        );
        assert!(sanitized.settings.sidebar_ui.collapsed);
        assert_eq!(sanitized.settings.sidebar_ui.width, 420);
        assert!(
            sanitized
                .settings
                .keybindings
                .overrides
                .contains_key("terminal.copy")
        );
        assert!(sanitized.settings.custom_themes.contains_key("custom-dark"));
        assert!(sanitized.settings.launcher.enabled);
        assert!(sanitized.settings.new_connection.save_connection);
    }

    #[test]
    fn app_lang_fills_language_when_saved_settings_do_not_have_one() {
        let mut entries = HashMap::new();
        entries.insert(SETTINGS_STORAGE_KEY.to_string(), json!({}).to_string());
        entries.insert(APP_LANG_KEY.to_string(), "fr-FR".to_string());
        let sanitized = sanitize_settings_value(legacy_local_storage_value(&entries)).unwrap();
        assert_eq!(sanitized.settings.general.language, Language::FrFr);
    }

    #[test]
    fn default_settings_path_matches_tauri_data_directory() {
        let path = default_settings_path();
        if cfg!(windows) {
            assert!(path.ends_with(Path::new("OxideTerm").join(SETTINGS_FILENAME)));
        } else {
            assert!(path.ends_with(Path::new(".oxideterm").join(SETTINGS_FILENAME)));
        }
    }

    #[test]
    fn load_and_save_use_envelope_format() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("settings.json");
        let mut store = SettingsStore::load_from_path(&path, None).unwrap();
        store.settings_mut().terminal.font_size = 18;
        store.save().unwrap();

        let reloaded = SettingsStore::load_from_path(&path, None).unwrap();
        assert_eq!(reloaded.settings().terminal.font_size, 18);
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(raw["version"], SETTINGS_SCHEMA_VERSION);
        assert!(raw.get("settings").is_some());
    }
}
