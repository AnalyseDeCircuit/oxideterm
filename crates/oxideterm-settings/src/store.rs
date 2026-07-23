// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use oxideterm_atomic_file::{durable_remove, durable_write_with_before_replace};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    model::{PersistedSettings, SETTINGS_SCHEMA_VERSION},
    normalize::sanitize_settings_value,
};

pub const SETTINGS_FILENAME: &str = "settings.json";
const MAX_SETTINGS_FILE_BYTES: u64 = 2 * 1024 * 1024;
const BOOTSTRAP_FILENAME: &str = "bootstrap.json";

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_ATOMIC_REPLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct BootstrapConfig {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    data_dir: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataDirectoryInfo {
    pub path: PathBuf,
    pub is_custom: bool,
    pub default_path: PathBuf,
    pub is_portable: bool,
    pub can_change: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataDirectoryCheck {
    pub has_existing_data: bool,
    pub files_found: Vec<String>,
}

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
    writes_blocked: bool,
}

enum SettingsFileCheckpoint {
    Missing,
    Present(Vec<u8>),
}

/// Opaque rollback state for the exact settings file and in-memory envelope.
#[must_use = "settings checkpoints should be restored or deliberately discarded"]
pub struct SettingsStoreCheckpoint {
    path: PathBuf,
    settings: PersistedSettings,
    updated_at: u64,
    writes_blocked: bool,
    file: SettingsFileCheckpoint,
}

impl std::fmt::Debug for SettingsStoreCheckpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SettingsStoreCheckpoint")
            .field("path", &self.path)
            .field("contents", &"[redacted settings checkpoint]")
            .finish()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub fn default_settings_path() -> PathBuf {
    if let Ok(Some(data_dir)) = oxideterm_portable_runtime::portable_data_dir() {
        return user_visible_data_dir_path(data_dir).join(SETTINGS_FILENAME);
    }

    if let Some(data_dir) = bootstrap_data_dir() {
        return data_dir.join(SETTINGS_FILENAME);
    }

    default_settings_dir().join(SETTINGS_FILENAME)
}

fn default_settings_dir() -> PathBuf {
    if cfg!(windows) {
        if let Some(config_home) = std::env::var_os("APPDATA") {
            return PathBuf::from(config_home).join("OxideTerm");
        }
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".oxideterm");
    }

    PathBuf::from(".")
}

pub fn data_directory_info() -> Result<DataDirectoryInfo> {
    let is_portable = oxideterm_portable_runtime::is_portable_mode()
        .map_err(|error| anyhow!("failed to detect portable mode: {}", error))?;
    let default_path = user_visible_data_dir_path(default_settings_dir());
    let path = if is_portable {
        oxideterm_portable_runtime::portable_data_dir()
            .map_err(|error| anyhow!("failed to resolve portable data directory: {}", error))?
            .map(user_visible_data_dir_path)
            .unwrap_or_else(|| default_path.clone())
    } else {
        bootstrap_data_dir().unwrap_or_else(|| default_path.clone())
    };
    Ok(DataDirectoryInfo {
        is_custom: !is_portable && path != default_path,
        can_change: !is_portable,
        is_portable,
        path,
        default_path,
    })
}

pub fn check_data_directory(path: &Path) -> Result<DataDirectoryCheck> {
    if !path.is_dir() {
        return Ok(DataDirectoryCheck {
            has_existing_data: false,
            files_found: Vec::new(),
        });
    }

    let known_files = [
        "connections.json",
        "state.redb",
        "chat_history.redb",
        "agent_history.redb",
        "sftp_progress.redb",
        "rag_index.redb",
        "plugin-config.json",
        "bootstrap.json",
        "topology_edges.json",
    ];
    let mut files_found = Vec::new();
    for name in known_files {
        if path.join(name).exists() {
            files_found.push(name.to_string());
        }
    }
    for name in ["logs", "plugins", "rag_hnsw.bin"] {
        if path.join(name).exists() {
            files_found.push(name.to_string());
        }
    }

    Ok(DataDirectoryCheck {
        has_existing_data: !files_found.is_empty(),
        files_found,
    })
}

pub fn set_data_directory(path: &Path) -> Result<()> {
    if oxideterm_portable_runtime::is_portable_mode()
        .map_err(|error| anyhow!("failed to detect portable mode: {}", error))?
    {
        return Err(anyhow!("Data directory cannot be changed in portable mode"));
    }
    if !path.is_absolute() {
        return Err(anyhow!("Data directory must be an absolute path"));
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(anyhow!("Data directory path must not contain '..'"));
    }

    fs::create_dir_all(path).context("Failed to create directory")?;
    let canonical = path.canonicalize().context("Failed to resolve path")?;
    let test_file = canonical.join(format!(".oxideterm_test_{}", std::process::id()));
    // Tauri verifies writability before writing bootstrap.json; keep the same
    // guard so a restart never points native at an unusable data directory.
    fs::write(&test_file, b"test").context("Directory is not writable")?;
    let _ = fs::remove_file(&test_file);

    let data_dir = user_visible_data_dir_path(canonical);
    save_bootstrap_config(&BootstrapConfig {
        data_dir: Some(data_dir.to_string_lossy().to_string()),
    })
}

pub fn reset_data_directory() -> Result<()> {
    if oxideterm_portable_runtime::is_portable_mode()
        .map_err(|error| anyhow!("failed to detect portable mode: {}", error))?
    {
        return Err(anyhow!("Data directory cannot be reset in portable mode"));
    }
    save_bootstrap_config(&BootstrapConfig::default())
}

fn save_bootstrap_config(config: &BootstrapConfig) -> Result<()> {
    let path = default_settings_dir().join(BOOTSTRAP_FILENAME);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create bootstrap directory")?;
    }
    let bytes =
        serde_json::to_vec_pretty(config).context("failed to serialize bootstrap config")?;
    atomic_write_file(&path, &bytes).context("failed to write bootstrap config")
}

fn bootstrap_data_dir() -> Option<PathBuf> {
    let path = default_settings_dir().join(BOOTSTRAP_FILENAME);
    let contents = fs::read_to_string(path).ok()?;
    let bootstrap: BootstrapConfig = serde_json::from_str(&contents).ok()?;
    let data_dir = user_visible_data_dir_path(PathBuf::from(bootstrap.data_dir?));
    // Tauri ignores relative bootstrap paths; native must do the same so both
    // frontends resolve to one effective data directory.
    data_dir.is_absolute().then_some(data_dir)
}

fn user_visible_data_dir_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    // Windows canonicalize() returns verbatim paths such as `\\?\D:\...`.
    // Bootstrap and settings UI should keep the user-facing path form.
    if let Some(stripped) = text.strip_prefix("\\\\?\\UNC\\") {
        return PathBuf::from(format!("\\\\{stripped}"));
    }
    if let Some(stripped) = text.strip_prefix("\\\\?\\") {
        return PathBuf::from(stripped);
    }
    path
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
        return Err(anyhow!("settings file is empty"));
    }
    let value: Value = serde_json::from_str(&contents).context("failed to parse settings file")?;
    if value.get("settings").is_some() {
        let envelope_version = value
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow!("settings envelope version is missing or invalid"))?;
        if envelope_version > u64::from(SETTINGS_SCHEMA_VERSION) {
            return Err(anyhow!(
                "settings envelope version {envelope_version} is newer than supported version {SETTINGS_SCHEMA_VERSION}"
            ));
        }
        let updated_at = value
            .get("updatedAt")
            .or_else(|| value.get("updated_at"))
            .and_then(Value::as_u64)
            .unwrap_or_else(now_ms);
        Ok(Some((value["settings"].clone(), updated_at)))
    } else {
        Ok(Some((value, now_ms())))
    }
}

fn write_envelope(path: &Path, settings: &PersistedSettings, updated_at: u64) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
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
    atomic_write_file(path, &json).context("failed to replace settings file")
}

fn atomic_write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    durable_write_with_before_replace(path, bytes, fail_before_atomic_replace_for_tests)
}

#[cfg(test)]
fn fail_before_atomic_replace_for_tests() -> io::Result<()> {
    FAIL_NEXT_ATOMIC_REPLACE.with(|fail| {
        if fail.replace(false) {
            Err(io::Error::other("injected failure before atomic replace"))
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
fn fail_before_atomic_replace_for_tests() -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn inject_atomic_replace_failure() {
    FAIL_NEXT_ATOMIC_REPLACE.with(|fail| fail.set(true));
}

impl SettingsStore {
    pub fn from_read_only(path: impl Into<PathBuf>, settings: PersistedSettings) -> Self {
        // CLI previews need the same in-memory shape as SettingsStore without triggering
        // migrations or envelope rewrites in the user's settings directory.
        Self {
            path: path.into(),
            settings,
            updated_at: 0,
            writes_blocked: true,
        }
    }

    pub fn load_default() -> Result<Self> {
        Self::load_from_path(default_settings_path())
    }

    pub fn load_from_path(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let load = load_settings_from_path(&path)?;
        Ok(Self {
            path,
            settings: load.settings,
            updated_at: load.updated_at,
            writes_blocked: load.recovered_from_corrupt_file,
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

    pub fn create_checkpoint(&self) -> Result<SettingsStoreCheckpoint> {
        let file = match fs::read(&self.path) {
            Ok(bytes) => SettingsFileCheckpoint::Present(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                SettingsFileCheckpoint::Missing
            }
            Err(error) => return Err(error).context("failed to checkpoint settings file"),
        };
        Ok(SettingsStoreCheckpoint {
            path: self.path.clone(),
            settings: self.settings.clone(),
            updated_at: self.updated_at,
            writes_blocked: self.writes_blocked,
            file,
        })
    }

    pub fn restore_checkpoint(&mut self, checkpoint: &SettingsStoreCheckpoint) -> Result<()> {
        if self.path != checkpoint.path {
            return Err(anyhow!("settings checkpoint belongs to a different store"));
        }
        // Restore disk first so memory continues to describe the current file
        // if the durable rollback itself fails.
        match &checkpoint.file {
            SettingsFileCheckpoint::Missing => {
                if self.path.exists() {
                    durable_remove(&self.path).context("failed to remove settings file")?;
                }
            }
            SettingsFileCheckpoint::Present(bytes) => {
                atomic_write_file(&self.path, bytes).context("failed to restore settings file")?;
            }
        }
        self.settings = checkpoint.settings.clone();
        self.updated_at = checkpoint.updated_at;
        self.writes_blocked = checkpoint.writes_blocked;
        Ok(())
    }

    pub fn save(&mut self) -> Result<SettingsSaveResult> {
        if self.writes_blocked {
            return Err(anyhow!(
                "refusing to overwrite an unreadable or newer settings file"
            ));
        }
        let saved = save_settings_to_path(&self.path, self.settings.clone())?;
        self.settings = saved.settings.clone();
        self.updated_at = saved.updated_at;
        Ok(saved)
    }

    pub fn replace_and_save(&mut self, settings: PersistedSettings) -> Result<SettingsSaveResult> {
        if self.writes_blocked {
            return Err(anyhow!(
                "refusing to overwrite an unreadable or newer settings file"
            ));
        }
        // Commit the in-memory replacement only after the durable file swap succeeds.
        let saved = save_settings_to_path(&self.path, settings)?;
        self.settings = saved.settings.clone();
        self.updated_at = saved.updated_at;
        Ok(saved)
    }
}

pub fn load_settings_from_path(path: &Path) -> Result<SettingsLoadResult> {
    let mut recovered_from_corrupt_file = false;
    let mut migration_warnings = Vec::new();
    let mut validation_warnings = Vec::new();

    let (raw, updated_at, should_persist) = match read_envelope(path) {
        Ok(Some((raw, updated_at))) => (raw, updated_at, true),
        Ok(None) => (PersistedSettings::default().to_value(), now_ms(), true),
        Err(err) => {
            recovered_from_corrupt_file = true;
            migration_warnings.push(format!("Recovered from unreadable settings file: {}", err));
            (PersistedSettings::default().to_value(), now_ms(), false)
        }
    };

    let sanitized = sanitize_settings_value(raw)?;
    migration_warnings.extend(sanitized.migration_warnings);
    validation_warnings.extend(sanitized.validation_warnings);
    if should_persist {
        write_envelope(path, &sanitized.settings, updated_at)?;
    }

    Ok(SettingsLoadResult {
        settings: sanitized.settings,
        version: SETTINGS_SCHEMA_VERSION,
        updated_at,
        migration_warnings,
        validation_warnings,
        recovered_from_corrupt_file,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        DEFAULT_WINDOW_OPACITY, RenderProfile,
        model::{
            ConflictAction, FontFamily, IdeAgentMode, Language, RendererType, UpdateChannel,
            default_update_channel_for_version, is_gpui_preview_version, is_prerelease_version,
        },
    };

    #[test]
    fn defaults_match_expected_settings_store_values() {
        let settings = PersistedSettings::default();
        assert_eq!(settings.version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(settings.general.language, Language::ZhCn);
        assert_eq!(
            settings.general.update_channel,
            default_update_channel_for_version(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            settings.terminal.command_bar.focus_handoff_commands,
            [
                "agy",
                "btop",
                "claude",
                "codex",
                "emacs",
                "fzf",
                "htop",
                "lazydocker",
                "lazygit",
                "less",
                "man",
                "micro",
                "nano",
                "nvim",
                "opencode",
                "ranger",
                "screen",
                "ssh",
                "tig",
                "tmux",
                "top",
                "vi",
                "vim",
                "yazi"
            ]
        );
        assert!(settings.terminal.command_bar.project_tasks);
        assert!(settings.terminal.command_bar.current_directory_awareness);
        assert_eq!(settings.terminal.theme, "default");
        assert_eq!(settings.terminal.font_family, FontFamily::Jetbrains);
        assert!(settings.terminal.cjk_font_family.is_empty());
        assert_eq!(settings.terminal.font_size, 14);
        assert_eq!(settings.terminal.line_height, 1.2);
        assert_eq!(settings.terminal.scrollback, 1000);
        assert_eq!(settings.terminal.renderer, RendererType::default());
        assert_eq!(settings.buffer.max_lines, 8000);
        assert_eq!(settings.appearance.border_radius, 6);
        assert_eq!(
            settings.appearance.ui_font_size,
            crate::DEFAULT_UI_FONT_SIZE
        );
        assert_eq!(settings.appearance.render_profile, RenderProfile::Auto);
        assert!(settings.appearance.show_window_titlebar);
        assert_eq!(settings.appearance.window_opacity, DEFAULT_WINDOW_OPACITY);
        assert_eq!(settings.connection_defaults.username, "root");
        assert_eq!(settings.sidebar_ui.width, 300);
        assert!(settings.sidebar_ui.show_app_lock_icon);
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
    fn default_update_channel_matches_tauri_version_rules() {
        assert!(!is_prerelease_version("1.4.2"));
        assert!(is_prerelease_version("1.4.2-beta.0"));
        assert!(is_prerelease_version("1.4.2-preview.1"));
        assert!(is_gpui_preview_version("1.4.2-gpui-preview.0"));
        assert!(is_gpui_preview_version("1.4.2-native-preview.0"));
        assert_eq!(
            default_update_channel_for_version("1.4.2"),
            UpdateChannel::Stable
        );
        assert_eq!(
            default_update_channel_for_version("1.4.2-beta.0"),
            UpdateChannel::Beta
        );
        assert_eq!(
            default_update_channel_for_version("1.4.2-gpui-preview.0"),
            UpdateChannel::GpuiPreview
        );
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
        assert_eq!(value["appearance"]["windowOpacity"], 1.0);
        assert_eq!(value["sftp"]["conflictAction"], "ask");
        assert_eq!(value["sftp"]["speedLimitKBps"], 0);
        assert_eq!(value["sftp"]["transferProtocol"], "auto");
        assert!(value["sftp"].get("speedLimitKbps").is_none());
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
        assert!(sanitized.settings.sidebar_ui.show_app_lock_icon);
        assert_eq!(sanitized.settings.connection_pool.idle_timeout_secs, 1);
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
    fn default_settings_path_matches_tauri_data_directory() {
        let path = default_settings_path();
        if cfg!(windows) {
            assert!(path.ends_with(Path::new("OxideTerm").join(SETTINGS_FILENAME)));
        } else {
            assert!(path.ends_with(Path::new(".oxideterm").join(SETTINGS_FILENAME)));
        }
    }

    #[test]
    fn user_visible_data_dir_path_strips_windows_verbatim_prefixes() {
        let disk =
            user_visible_data_dir_path(PathBuf::from(r"\\?\D:\DevSoftWare\Remote\OxideTerm\data"));
        assert_eq!(
            disk.to_string_lossy(),
            r"D:\DevSoftWare\Remote\OxideTerm\data"
        );

        let unc = user_visible_data_dir_path(PathBuf::from(r"\\?\UNC\server\share\OxideTerm"));
        assert_eq!(unc.to_string_lossy(), r"\\server\share\OxideTerm");
    }

    #[test]
    fn load_and_save_use_envelope_format() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("settings.json");
        let mut store = SettingsStore::load_from_path(&path).unwrap();
        store.settings_mut().terminal.font_size = 18;
        store.save().unwrap();

        let reloaded = SettingsStore::load_from_path(&path).unwrap();
        assert_eq!(reloaded.settings().terminal.font_size, 18);
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(raw["version"], SETTINGS_SCHEMA_VERSION);
        assert!(raw.get("settings").is_some());
    }

    #[test]
    fn corrupt_settings_are_preserved_and_block_later_saves() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("settings.json");
        let corrupt = b"{ not valid settings";
        fs::write(&path, corrupt).unwrap();

        let mut store = SettingsStore::load_from_path(&path).unwrap();

        assert_eq!(fs::read(&path).unwrap(), corrupt);
        assert!(store.save().is_err());
        assert_eq!(fs::read(&path).unwrap(), corrupt);
    }

    #[test]
    fn future_settings_envelope_is_preserved() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("settings.json");
        let future = serde_json::to_vec_pretty(&json!({
            "version": SETTINGS_SCHEMA_VERSION + 1,
            "settings": PersistedSettings::default().to_value(),
            "updated_at": 42
        }))
        .unwrap();
        fs::write(&path, &future).unwrap();

        let load = load_settings_from_path(&path).unwrap();

        assert!(load.recovered_from_corrupt_file);
        assert_eq!(fs::read(&path).unwrap(), future);
    }

    #[test]
    fn failed_atomic_settings_replace_preserves_previous_file() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("settings.json");
        let mut store = SettingsStore::load_from_path(&path).unwrap();
        let previous = fs::read(&path).unwrap();
        store.settings_mut().terminal.font_size = 19;
        inject_atomic_replace_failure();

        assert!(store.save().is_err());
        assert_eq!(fs::read(&path).unwrap(), previous);
    }

    #[test]
    fn failed_settings_replacement_preserves_in_memory_value() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("settings.json");
        let mut store = SettingsStore::load_from_path(&path).unwrap();
        let previous = store.settings().clone();
        let mut replacement = previous.clone();
        replacement.terminal.font_size = 19;
        inject_atomic_replace_failure();

        assert!(store.replace_and_save(replacement).is_err());
        assert_eq!(store.settings(), &previous);
    }

    #[test]
    fn settings_checkpoint_restores_exact_file_and_timestamp() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("settings.json");
        let mut store = SettingsStore::load_from_path(&path).unwrap();
        let checkpoint = store.create_checkpoint().unwrap();
        let original_bytes = fs::read(&path).unwrap();
        let original_updated_at = store.updated_at();
        let mut replacement = store.settings().clone();
        replacement.terminal.font_size = 19;
        store.replace_and_save(replacement).unwrap();

        store.restore_checkpoint(&checkpoint).unwrap();

        assert_eq!(fs::read(&path).unwrap(), original_bytes);
        assert_eq!(store.updated_at(), original_updated_at);
        assert_eq!(
            store.settings(),
            &SettingsStore::load_from_path(&path).unwrap().settings
        );
    }

    #[test]
    fn settings_checkpoint_restores_missing_file_state() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("settings.json");
        let store = SettingsStore::from_read_only(&path, PersistedSettings::default());
        let checkpoint = store.create_checkpoint().unwrap();
        fs::write(&path, b"replacement").unwrap();
        let mut store = store;

        store.restore_checkpoint(&checkpoint).unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn future_settings_value_is_rejected_before_normalization() {
        let future = json!({ "version": SETTINGS_SCHEMA_VERSION + 1 });

        assert!(sanitize_settings_value(future).is_err());
    }
}
