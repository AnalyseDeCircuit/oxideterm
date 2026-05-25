// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::plugin_runtime::{PluginOutboundMessage, PluginRegistration, PluginRegistrationKind};

const PLUGINS_DIR_NAME: &str = "plugins";
const PLUGIN_CONFIG_FILENAME: &str = "plugin-config.json";
const PLUGIN_CONFIG_CORRUPT_MARKER: &str = "corrupt";
const PLUGIN_MANIFEST_FILENAME: &str = "plugin.json";
const PLUGIN_CONFIG_SCHEMA_VERSION: u32 = 1;
const PLUGIN_STORAGE_MAX_KEY_BYTES: usize = 256;
const PLUGIN_STORAGE_MAX_PLUGIN_BYTES: usize = 256 * 1024;
#[allow(dead_code)]
const PLUGIN_PACKAGE_MAX_BYTES: u64 = 50 * 1024 * 1024;
#[allow(dead_code)]
const PLUGIN_PACKAGE_MAX_EXTRACTED_BYTES: u64 = 100 * 1024 * 1024;
#[allow(dead_code)]
const PLUGIN_PACKAGE_MAX_ENTRIES: usize = 2048;
pub(super) const NATIVE_PLUGIN_UI_EVENT: &str = "ui.event";
const NATIVE_PLUGIN_DECLARATIVE_UI_FORM_KIND: &str = "form";
const NATIVE_PLUGIN_DECLARATIVE_UI_CONTROL_KINDS: &[&str] = &[
    "text",
    "password",
    "number",
    "checkbox",
    "select",
    "button",
    "markdown",
    "code",
    "codeBlock",
    "code-block",
    "statusBadge",
    "status-badge",
    "progress",
    "table",
    "list",
    "emptyState",
    "empty-state",
    "divider",
    "keyValue",
    "key-value",
    "keyValueRow",
    "key-value-row",
];
pub(super) const NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT: &str = "app.themeChanged";
pub(super) const NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT: &str = "app.settingsChanged";
pub(super) const NATIVE_PLUGIN_I18N_LANGUAGE_CHANGED_EVENT: &str = "i18n.languageChanged";
pub(super) const NATIVE_PLUGIN_SETTING_CHANGED_EVENT: &str = "settings.changed";
pub(super) const NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT: &str = "ui.layoutChanged";
pub(super) const NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT: &str = "sessions.treeChanged";
pub(super) const NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT: &str = "sessions.nodeStateChanged";
pub(super) const NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT: &str = "eventLog.entry";
pub(super) const NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT: &str =
    "forward.savedForwardsChanged";
pub(super) const NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT: &str = "transfers.progress";
pub(super) const NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT: &str = "transfers.complete";
pub(super) const NATIVE_PLUGIN_TRANSFER_ERROR_EVENT: &str = "transfers.error";
pub(super) const NATIVE_PLUGIN_PROFILER_METRICS_EVENT: &str = "profiler.metrics";
pub(super) const NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT: &str = "ide.fileOpen";
pub(super) const NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT: &str = "ide.fileClose";
pub(super) const NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT: &str = "ide.activeFileChanged";
pub(super) const NATIVE_PLUGIN_AI_MESSAGE_EVENT: &str = "ai.message";
pub(super) const NATIVE_PLUGIN_LIFECYCLE_CONNECT_EVENT: &str = "lifecycle.onConnect";
pub(super) const NATIVE_PLUGIN_LIFECYCLE_DISCONNECT_EVENT: &str = "lifecycle.onDisconnect";
pub(super) const NATIVE_PLUGIN_LIFECYCLE_LINK_DOWN_EVENT: &str = "lifecycle.onLinkDown";
pub(super) const NATIVE_PLUGIN_LIFECYCLE_RECONNECT_EVENT: &str = "lifecycle.onReconnect";
const NATIVE_PLUGIN_PHASE4_SUBSCRIPTION_EVENTS: &[&str] = &[
    NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT,
    NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT,
    NATIVE_PLUGIN_I18N_LANGUAGE_CHANGED_EVENT,
    NATIVE_PLUGIN_SETTING_CHANGED_EVENT,
    NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT,
    NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT,
    NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT,
    NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT,
    NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT,
    NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT,
    NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT,
    NATIVE_PLUGIN_TRANSFER_ERROR_EVENT,
    NATIVE_PLUGIN_PROFILER_METRICS_EVENT,
    NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT,
    NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT,
    NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT,
    NATIVE_PLUGIN_AI_MESSAGE_EVENT,
    NATIVE_PLUGIN_LIFECYCLE_CONNECT_EVENT,
    NATIVE_PLUGIN_LIFECYCLE_DISCONNECT_EVENT,
    NATIVE_PLUGIN_LIFECYCLE_LINK_DOWN_EVENT,
    NATIVE_PLUGIN_LIFECYCLE_RECONNECT_EVENT,
];

#[derive(Clone, Debug, Default)]
pub(super) struct NativePluginRegistry {
    plugins: Vec<NativePluginInfo>,
    diagnostics: Vec<NativePluginDiagnostic>,
    contributions: NativePluginContributionStore,
    config: NativePluginGlobalConfig,
    config_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginInfo {
    pub manifest: NativePluginManifest,
    pub install_dir: PathBuf,
    pub runtime_plan: NativePluginRuntimePlan,
    pub state: NativePluginState,
    pub config: NativePluginConfigEntry,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginProcessActivationPlan {
    pub plugin_id: String,
    pub manifest: NativePluginManifest,
    pub install_dir: PathBuf,
    pub entry: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginWasmActivationPlan {
    pub plugin_id: String,
    pub manifest: NativePluginManifest,
    pub install_dir: PathBuf,
    pub entry: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginRegistryEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    pub version: String,
    #[serde(default)]
    pub min_oxideterm_version: Option<String>,
    pub download_url: String,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub capabilities_summary: Option<Vec<String>>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(super) struct NativePluginRegistryIndex {
    pub version: u32,
    pub plugins: Vec<NativePluginRegistryEntry>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginUrlInstallResult {
    pub manifest: NativePluginManifest,
    pub checksum: String,
    pub replaced_existing: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginInstalledInfo {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NativePluginDiagnostic {
    pub plugin_dir: PathBuf,
    pub plugin_id: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct NativePluginContributionStore {
    pub tabs: Vec<NativePluginTabContribution>,
    pub sidebar_panels: Vec<NativePluginSidebarContribution>,
    pub settings: Vec<NativePluginSettingContribution>,
    pub ai_tools: Vec<NativePluginAiToolContribution>,
    pub terminal_shortcuts: Vec<NativePluginShortcutContribution>,
    pub terminal_transports: Vec<NativePluginTransportContribution>,
    pub connection_hooks: Vec<NativePluginConnectionHookContribution>,
    pub api_commands: Vec<NativePluginApiCommandContribution>,
    pub runtime_commands: Vec<NativePluginRuntimeCommandContribution>,
    pub runtime_keybindings: Vec<NativePluginRuntimeKeybindingContribution>,
    pub runtime_context_menus: Vec<NativePluginRuntimeContextMenuContribution>,
    pub runtime_status_items: Vec<NativePluginRuntimeStatusItemContribution>,
    pub runtime_tab_views: Vec<NativePluginRuntimeTabViewContribution>,
    pub runtime_sidebar_panels: Vec<NativePluginRuntimeSidebarPanelContribution>,
    pub runtime_event_subscriptions: Vec<NativePluginRuntimeEventSubscriptionContribution>,
    pub runtime_terminal_input_interceptors: Vec<NativePluginRuntimeTerminalHookContribution>,
    pub runtime_terminal_output_processors: Vec<NativePluginRuntimeTerminalHookContribution>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginTabContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub definition: NativePluginTabDef,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginSidebarContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub definition: NativePluginSidebarDef,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginSettingContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub definition: NativePluginSettingDef,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginAiToolContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub definition: NativePluginAiToolDef,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginShortcutContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub definition: NativePluginShortcutDef,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginTransportContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub transport: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginConnectionHookContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub hook: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginApiCommandContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeCommandContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub command: String,
    pub label: String,
    pub icon: Option<String>,
    pub shortcut: Option<String>,
    pub section: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeKeybindingContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub keybinding: String,
    pub normalized_keybinding: String,
    pub command: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeTerminalHookContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeContextMenuContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub target: String,
    pub items: Vec<NativePluginRuntimeContextMenuItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeContextMenuItem {
    pub label: String,
    pub icon: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeStatusItemContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub text: String,
    pub icon: Option<String>,
    pub tooltip: Option<String>,
    pub alignment: String,
    pub priority: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeTabViewContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub tab_id: String,
    pub title: String,
    pub icon: String,
    pub schema: NativePluginDeclarativeUiSchema,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeSidebarPanelContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub panel_id: String,
    pub title: String,
    pub icon: String,
    pub position: String,
    pub schema: NativePluginDeclarativeUiSchema,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginRuntimeEventSubscriptionContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub event: String,
    pub filter: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub engines: Option<NativePluginEngines>,
    #[serde(default)]
    pub manifest_version: Option<u8>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub styles: Option<Vec<String>>,
    #[serde(default)]
    pub shared_dependencies: Option<HashMap<String, String>>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub contributes: Option<NativePluginContributes>,
    #[serde(default)]
    pub locales: Option<String>,
    #[serde(default)]
    pub runtime: Option<NativePluginRuntime>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(super) struct NativePluginEngines {
    #[serde(default)]
    pub oxideterm: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginRuntime {
    pub kind: NativePluginRuntimeKind,
    pub entry: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum NativePluginRuntimeKind {
    Wasm,
    Process,
    ManifestOnly,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginContributes {
    #[serde(default)]
    pub tabs: Option<Vec<NativePluginTabDef>>,
    #[serde(default)]
    pub sidebar_panels: Option<Vec<NativePluginSidebarDef>>,
    #[serde(default)]
    pub settings: Option<Vec<NativePluginSettingDef>>,
    #[serde(default)]
    pub terminal_hooks: Option<NativePluginTerminalHooksDef>,
    #[serde(default)]
    pub terminal_transports: Option<Vec<String>>,
    #[serde(default)]
    pub connection_hooks: Option<Vec<String>>,
    #[serde(default)]
    pub ai_tools: Option<Vec<NativePluginAiToolDef>>,
    #[serde(default)]
    pub api_commands: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(super) struct NativePluginTabDef {
    pub id: String,
    pub title: String,
    pub icon: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(super) struct NativePluginSidebarDef {
    pub id: String,
    pub title: String,
    pub icon: String,
    #[serde(default = "default_sidebar_position")]
    pub position: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(super) struct NativePluginSettingDef {
    pub id: String,
    #[serde(rename = "type")]
    pub setting_type: String,
    pub default: Value,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<NativePluginSettingOption>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(super) struct NativePluginSettingOption {
    pub label: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginTerminalHooksDef {
    #[serde(default)]
    pub input_interceptor: Option<bool>,
    #[serde(default)]
    pub output_processor: Option<bool>,
    #[serde(default)]
    pub shortcuts: Option<Vec<NativePluginShortcutDef>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(super) struct NativePluginShortcutDef {
    pub key: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginDeclarativeUiSchema {
    #[serde(default = "default_declarative_ui_kind")]
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub sections: Vec<NativePluginDeclarativeUiSection>,
    #[serde(default)]
    pub controls: Vec<NativePluginDeclarativeUiControl>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginDeclarativeUiSection {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub controls: Vec<NativePluginDeclarativeUiControl>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginDeclarativeUiControl {
    pub kind: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<NativePluginDeclarativeUiOption>>,
    #[serde(default)]
    pub rows: Option<Vec<Value>>,
    #[serde(default)]
    pub columns: Option<Vec<String>>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub loading: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginDeclarativeUiOption {
    pub label: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(super) struct NativePluginAiToolDef {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: Option<Value>,
    #[serde(default)]
    pub capabilities: Option<Vec<String>>,
    #[serde(default)]
    pub risk: Option<String>,
    #[serde(default)]
    pub target_kinds: Option<Vec<String>>,
    #[serde(default)]
    pub result_schema: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NativePluginRuntimePlan {
    ManifestOnly,
    Wasm { entry: String },
    Process { entry: String },
    UnsupportedLegacyJs { entry: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativePluginState {
    #[allow(dead_code)]
    Discovered,
    Disabled,
    UnsupportedLegacyJs,
    ReadyManifestOnly,
    ReadyWasm,
    ReadyProcess,
    #[allow(dead_code)]
    Loading,
    #[allow(dead_code)]
    Active,
    Error,
    AutoDisabled,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginGlobalConfig {
    version: u32,
    #[serde(default)]
    plugins: HashMap<String, NativePluginConfigEntry>,
    #[serde(default)]
    settings: HashMap<String, HashMap<String, Value>>,
    #[serde(default)]
    storage: HashMap<String, HashMap<String, Value>>,
}

impl Default for NativePluginGlobalConfig {
    fn default() -> Self {
        Self {
            version: PLUGIN_CONFIG_SCHEMA_VERSION,
            plugins: HashMap::new(),
            settings: HashMap::new(),
            storage: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePluginConfigEntry {
    #[serde(default = "default_plugin_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub auto_disabled: bool,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub install_path: Option<String>,
    #[serde(default)]
    pub runtime_kind: Option<String>,
    #[serde(default)]
    pub last_loaded_version: Option<String>,
    #[serde(default)]
    pub error_count: u32,
    #[serde(default)]
    pub error_window_started_at_ms: Option<u64>,
}

impl Default for NativePluginConfigEntry {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_disabled: false,
            last_error: None,
            install_path: None,
            runtime_kind: None,
            last_loaded_version: None,
            error_count: 0,
            error_window_started_at_ms: None,
        }
    }
}

fn default_sidebar_position() -> String {
    "bottom".to_string()
}

fn default_declarative_ui_kind() -> String {
    NATIVE_PLUGIN_DECLARATIVE_UI_FORM_KIND.to_string()
}

fn default_plugin_enabled() -> bool {
    true
}

impl NativePluginRegistry {
    pub fn discover(settings_path: &Path) -> Self {
        let plugins_dir = native_plugins_dir(settings_path);
        let config_path = native_plugin_config_path(settings_path);
        let config = load_native_plugin_config(&config_path);
        // Phase 1 owns the native plugin config file. Persist a missing file so
        // later enable/disable/error transitions have a stable location without
        // falling back to ad hoc state.
        if !config_path.exists() {
            let _ = save_native_plugin_config(&config_path, &config);
        }
        let (plugins, diagnostics) = discover_native_plugins_in_dir(&plugins_dir, &config);
        let contributions = NativePluginContributionStore::from_plugins(&plugins);
        Self {
            plugins,
            diagnostics,
            contributions,
            config,
            config_path,
        }
    }

    pub fn plugins(&self) -> &[NativePluginInfo] {
        &self.plugins
    }

    pub fn diagnostics(&self) -> &[NativePluginDiagnostic] {
        &self.diagnostics
    }

    pub fn contributions(&self) -> &NativePluginContributionStore {
        &self.contributions
    }

    #[allow(dead_code)]
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    #[allow(dead_code)]
    pub fn configured_plugin_count(&self) -> usize {
        self.config.plugins.len()
    }

    pub fn process_activation_plans(&self) -> Vec<NativePluginProcessActivationPlan> {
        self.plugins
            .iter()
            .filter_map(|plugin| {
                if !matches!(plugin.state, NativePluginState::ReadyProcess) {
                    return None;
                }
                let NativePluginRuntimePlan::Process { entry } = &plugin.runtime_plan else {
                    return None;
                };
                Some(NativePluginProcessActivationPlan {
                    plugin_id: plugin.manifest.id.clone(),
                    manifest: plugin.manifest.clone(),
                    install_dir: plugin.install_dir.clone(),
                    entry: entry.clone(),
                })
            })
            .collect()
    }

    pub fn wasm_activation_plans(&self) -> Vec<NativePluginWasmActivationPlan> {
        self.plugins
            .iter()
            .filter_map(|plugin| {
                if !matches!(plugin.state, NativePluginState::ReadyWasm) {
                    return None;
                }
                let NativePluginRuntimePlan::Wasm { entry } = &plugin.runtime_plan else {
                    return None;
                };
                Some(NativePluginWasmActivationPlan {
                    plugin_id: plugin.manifest.id.clone(),
                    manifest: plugin.manifest.clone(),
                    install_dir: plugin.install_dir.clone(),
                    entry: entry.clone(),
                })
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn install_plugin_package(
        settings_path: &Path,
        expected_id: &str,
        checksum: Option<&str>,
        package_bytes: &[u8],
    ) -> Result<NativePluginManifest, String> {
        validate_native_plugin_id(expected_id)?;
        let result =
            install_native_plugin_package_bytes(settings_path, package_bytes, checksum, true)?;
        if result.manifest.id != expected_id {
            return Err(format!(
                "Plugin ID mismatch: expected \"{}\", got \"{}\"",
                expected_id, result.manifest.id
            ));
        }
        Ok(result.manifest)
    }

    #[allow(dead_code)]
    pub fn install_plugin_package_from_bytes(
        settings_path: &Path,
        package_bytes: &[u8],
        checksum: Option<&str>,
        overwrite: bool,
    ) -> Result<NativePluginUrlInstallResult, String> {
        install_native_plugin_package_bytes(settings_path, package_bytes, checksum, overwrite)
    }

    #[allow(dead_code)]
    pub async fn fetch_plugin_registry(url: &str) -> Result<NativePluginRegistryIndex, String> {
        validate_native_plugin_package_url(url)?;
        let response = reqwest::get(url)
            .await
            .map_err(|error| format!("Failed to fetch registry: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Registry returned HTTP {}",
                response.status().as_u16()
            ));
        }
        let body = response
            .text()
            .await
            .map_err(|error| format!("Failed to read registry response: {error}"))?;
        serde_json::from_str(&body)
            .map_err(|error| format!("Failed to parse registry index: {error}"))
    }

    #[allow(dead_code)]
    pub async fn install_plugin_package_from_url(
        settings_path: &Path,
        download_url: &str,
        checksum: Option<&str>,
        overwrite: bool,
    ) -> Result<NativePluginUrlInstallResult, String> {
        validate_native_plugin_package_url(download_url)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|error| format!("Failed to create HTTP client: {error}"))?;
        let response = client
            .get(download_url)
            .send()
            .await
            .map_err(|error| format!("Failed to download plugin: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Download returned HTTP {}",
                response.status().as_u16()
            ));
        }
        if let Some(content_length) = response.content_length()
            && content_length > PLUGIN_PACKAGE_MAX_BYTES
        {
            return Err(format!(
                "Plugin package too large: {} bytes (max {} bytes)",
                content_length, PLUGIN_PACKAGE_MAX_BYTES
            ));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("Failed to read download body: {error}"))?;
        install_native_plugin_package_bytes(settings_path, &bytes, checksum, overwrite)
    }

    #[allow(dead_code)]
    pub fn check_plugin_updates(
        registry: NativePluginRegistryIndex,
        installed: &[NativePluginInstalledInfo],
    ) -> Vec<NativePluginRegistryEntry> {
        let installed_versions = installed
            .iter()
            .map(|plugin| (plugin.id.as_str(), plugin.version.as_str()))
            .collect::<HashMap<_, _>>();
        registry
            .plugins
            .into_iter()
            .filter(|entry| {
                installed_versions
                    .get(entry.id.as_str())
                    .is_some_and(|version| native_plugin_version_is_newer(&entry.version, version))
            })
            .collect()
    }

    pub fn uninstall_plugin(
        &mut self,
        plugin_id: &str,
        remove_settings: bool,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        let plugin_dir = native_plugins_dir_from_config_path(&self.config_path).join(plugin_id);
        if !plugin_dir.exists() {
            return Err(format!("Plugin \"{plugin_id}\" is not installed"));
        }
        if !plugin_dir.join(PLUGIN_MANIFEST_FILENAME).exists() {
            return Err(format!(
                "Directory \"{plugin_id}\" does not appear to be a valid plugin"
            ));
        }

        fs::remove_dir_all(&plugin_dir)
            .map_err(|error| format!("Failed to remove plugin directory: {error}"))?;
        self.cleanup_runtime_plugin_contributions(plugin_id);
        self.config.plugins.remove(plugin_id);
        if remove_settings {
            self.config.settings.remove(plugin_id);
            self.config.storage.remove(plugin_id);
        }
        save_native_plugin_config(&self.config_path, &self.config)?;
        let settings_path = settings_path_from_native_plugin_config_path(&self.config_path);
        *self = NativePluginRegistry::discover(&settings_path);
        Ok(())
    }

    pub fn mark_runtime_loading(&mut self, plugin_id: &str) -> Result<(), String> {
        self.set_runtime_state(plugin_id, NativePluginState::Loading, None)
    }

    pub fn mark_runtime_active(&mut self, plugin_id: &str) -> Result<(), String> {
        self.set_runtime_state(plugin_id, NativePluginState::Active, None)
    }

    pub fn mark_runtime_error(&mut self, plugin_id: &str, message: String) -> Result<(), String> {
        self.set_runtime_state(plugin_id, NativePluginState::Error, Some(message.clone()))?;
        self.record_manager_error(plugin_id.to_string(), message);
        Ok(())
    }

    fn set_runtime_state(
        &mut self,
        plugin_id: &str,
        state: NativePluginState,
        last_error: Option<String>,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        let plugin = self
            .plugins
            .iter_mut()
            .find(|plugin| plugin.manifest.id == plugin_id)
            .ok_or_else(|| format!("Plugin \"{plugin_id}\" is not discovered"))?;
        // Tauri stores transient plugin lifecycle separately from persisted
        // plugin-config. Native keeps active/loading in memory while persisting
        // runtime errors so Plugin Manager still explains failed activation
        // after restart.
        plugin.state = state;
        if let Some(error) = last_error {
            plugin.config.last_error = Some(error.clone());
            let entry = self
                .config
                .plugins
                .entry(plugin_id.to_string())
                .or_default();
            entry.last_error = Some(error);
            entry.runtime_kind = Some(native_runtime_kind_label(&plugin.runtime_plan).to_string());
            save_native_plugin_config(&self.config_path, &self.config)?;
        } else if matches!(
            state,
            NativePluginState::Active | NativePluginState::Loading
        ) {
            plugin.config.last_error = None;
        }
        Ok(())
    }

    // Phase 3 process/WASM bridges feed dynamic registrations through these
    // entry points once WorkspaceApp owns live runtime supervisors.
    #[allow(dead_code)]
    pub fn apply_runtime_registration(
        &mut self,
        registration: PluginRegistration,
    ) -> Result<(), String> {
        validate_native_plugin_id(&registration.plugin_id)?;
        let plugin = self
            .plugins
            .iter()
            .find(|plugin| plugin.manifest.id == registration.plugin_id)
            .ok_or_else(|| format!("Plugin \"{}\" is not discovered", registration.plugin_id))?;
        let plugin_name = plugin.manifest.name.clone();
        if registration.kind == PluginRegistrationKind::TerminalShortcut {
            return self.contributions.apply_runtime_terminal_shortcut(
                registration,
                plugin_name,
                &plugin.manifest,
            );
        }
        if registration.kind == PluginRegistrationKind::Tab {
            return self.contributions.apply_runtime_tab_view(
                registration,
                plugin_name,
                &plugin.manifest,
            );
        }
        if registration.kind == PluginRegistrationKind::SidebarPanel {
            return self.contributions.apply_runtime_sidebar_panel(
                registration,
                plugin_name,
                &plugin.manifest,
            );
        }
        if matches!(
            registration.kind,
            PluginRegistrationKind::TerminalInputInterceptor
                | PluginRegistrationKind::TerminalOutputProcessor
        ) {
            return self.contributions.apply_runtime_terminal_hook(
                registration,
                plugin_name,
                &plugin.manifest,
            );
        }
        self.contributions
            .apply_runtime_registration(registration, plugin_name)
    }

    #[allow(dead_code)]
    pub fn dispose_runtime_registration(&mut self, plugin_id: &str, registration_id: &str) -> bool {
        self.contributions
            .dispose_runtime_registration(plugin_id, registration_id)
    }

    #[allow(dead_code)]
    pub fn cleanup_runtime_plugin_contributions(&mut self, plugin_id: &str) -> usize {
        self.contributions
            .cleanup_runtime_plugin_contributions(plugin_id)
    }

    // Process/WASM runtimes emit protocol messages, while this registry owns
    // the host-visible contribution rows. Keep the bridge explicit so runtime
    // transports cannot mutate UI state outside the same validation path used
    // by manifest-only contributions.
    #[allow(dead_code)]
    pub fn apply_runtime_outbound_message(
        &mut self,
        plugin_id: &str,
        message: &PluginOutboundMessage,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        match message {
            PluginOutboundMessage::RegisterContribution { registration } => {
                if registration.plugin_id != plugin_id {
                    return Err(format!(
                        "Runtime registration plugin id \"{}\" does not match owner \"{}\"",
                        registration.plugin_id, plugin_id
                    ));
                }
                self.apply_runtime_registration(registration.clone())
            }
            PluginOutboundMessage::DisposeContribution { registration_id } => {
                self.dispose_runtime_registration(plugin_id, registration_id);
                Ok(())
            }
            PluginOutboundMessage::RuntimeError { error } => {
                self.record_manager_error(plugin_id.to_string(), error.message.clone());
                Ok(())
            }
            PluginOutboundMessage::Log { level, message } => {
                if matches!(level, super::plugin_runtime::PluginRuntimeLogLevel::Error) {
                    self.record_manager_error(plugin_id.to_string(), message.clone());
                }
                Ok(())
            }
            PluginOutboundMessage::RuntimeReady
            | PluginOutboundMessage::ReportProgress { .. }
            | PluginOutboundMessage::EmitEvent { .. }
            | PluginOutboundMessage::CallHostApi { .. } => Ok(()),
        }
    }

    pub fn record_manager_error(&mut self, plugin_id: String, message: String) {
        // Manager-side persistence failures should be visible in the same
        // diagnostics stream as manifest validation failures instead of being
        // lost in stdout/stderr.
        self.diagnostics.push(NativePluginDiagnostic {
            plugin_dir: self.config_path.clone(),
            plugin_id: Some(plugin_id),
            message,
        });
    }

    pub fn set_plugin_enabled(&mut self, plugin_id: &str, enabled: bool) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        let plugin_snapshot = self
            .plugins
            .iter()
            .find(|plugin| plugin.manifest.id == plugin_id)
            .cloned()
            .ok_or_else(|| format!("Plugin \"{plugin_id}\" is not discovered"))?;

        if matches!(
            plugin_snapshot.runtime_plan,
            NativePluginRuntimePlan::UnsupportedLegacyJs { .. }
        ) && enabled
        {
            return Err(
                "Legacy Tauri JavaScript plugins cannot be enabled in native mode".to_string(),
            );
        }

        let entry = self
            .config
            .plugins
            .entry(plugin_id.to_string())
            .or_default();
        entry.enabled = enabled;
        entry.install_path = Some(plugin_snapshot.install_dir.display().to_string());
        entry.runtime_kind =
            Some(native_runtime_kind_label(&plugin_snapshot.runtime_plan).to_string());
        entry.last_loaded_version = Some(plugin_snapshot.manifest.version.clone());

        if enabled {
            // Tauri reload clears the disabled/error path before trying to load
            // again. Native Phase 1 has no runtime yet, but the config state must
            // still be ready for Phase 3 activation.
            entry.auto_disabled = false;
            entry.last_error = None;
            entry.error_count = 0;
            entry.error_window_started_at_ms = None;
        }

        save_native_plugin_config(&self.config_path, &self.config)?;
        self.refresh_plugin_state(plugin_id);
        self.contributions = NativePluginContributionStore::from_plugins(&self.plugins);
        Ok(())
    }

    pub fn plugin_setting_value(&self, plugin_id: &str, setting_id: &str) -> Option<Value> {
        validate_native_plugin_id(plugin_id).ok()?;
        let setting = self.find_plugin_setting(plugin_id, setting_id)?;
        Some(
            self.config
                .settings
                .get(plugin_id)
                .and_then(|values| values.get(setting_id))
                .cloned()
                .unwrap_or_else(|| setting.definition.default.clone()),
        )
    }

    // Phase 2 settings controls will call this once the manifest-only settings
    // panel is wired; keeping the typed writer here prevents page-local state
    // from inventing a different persistence path.
    #[allow(dead_code)]
    pub fn set_plugin_setting_value(
        &mut self,
        plugin_id: &str,
        setting_id: &str,
        value: Value,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        let setting = self
            .find_plugin_setting(plugin_id, setting_id)
            .ok_or_else(|| {
                format!("Plugin setting \"{plugin_id}.{setting_id}\" is not declared")
            })?;
        validate_plugin_setting_value(&setting.definition, &value)?;
        self.config
            .settings
            .entry(plugin_id.to_string())
            .or_default()
            .insert(setting_id.to_string(), value);
        save_native_plugin_config(&self.config_path, &self.config)
    }

    #[allow(dead_code)]
    pub fn plugin_storage_value(&self, plugin_id: &str, key: &str) -> Option<Value> {
        validate_native_plugin_id(plugin_id).ok()?;
        validate_plugin_storage_key(key).ok()?;
        self.config
            .storage
            .get(plugin_id)
            .and_then(|values| values.get(key))
            .cloned()
    }

    pub fn set_plugin_storage_value(
        &mut self,
        plugin_id: &str,
        key: &str,
        value: Value,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        validate_plugin_storage_key(key)?;
        // Tauri scoped localStorage serializes JSON by plugin id. Native stores
        // the same JSON values under a plugin-owned map and validates the whole
        // plugin bucket before writing so one plugin cannot bloat the shared
        // config file.
        let mut plugin_values = self
            .config
            .storage
            .get(plugin_id)
            .cloned()
            .unwrap_or_default();
        plugin_values.insert(key.to_string(), value);
        validate_plugin_storage_size(&plugin_values)?;
        self.config
            .storage
            .insert(plugin_id.to_string(), plugin_values);
        save_native_plugin_config(&self.config_path, &self.config)
    }

    pub fn remove_plugin_storage_value(
        &mut self,
        plugin_id: &str,
        key: &str,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        validate_plugin_storage_key(key)?;
        if let Some(values) = self.config.storage.get_mut(plugin_id) {
            values.remove(key);
            if values.is_empty() {
                self.config.storage.remove(plugin_id);
            }
        }
        save_native_plugin_config(&self.config_path, &self.config)
    }

    #[allow(dead_code)]
    pub fn clear_plugin_storage(&mut self, plugin_id: &str) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        self.config.storage.remove(plugin_id);
        save_native_plugin_config(&self.config_path, &self.config)
    }

    fn find_plugin_setting(
        &self,
        plugin_id: &str,
        setting_id: &str,
    ) -> Option<&NativePluginSettingContribution> {
        self.contributions
            .settings
            .iter()
            .find(|setting| setting.plugin_id == plugin_id && setting.definition.id == setting_id)
    }

    fn refresh_plugin_state(&mut self, plugin_id: &str) {
        for plugin in &mut self.plugins {
            if plugin.manifest.id == plugin_id {
                let config_entry = self
                    .config
                    .plugins
                    .get(plugin_id)
                    .cloned()
                    .unwrap_or_else(NativePluginConfigEntry::default);
                plugin.state = native_plugin_state_for(&plugin.runtime_plan, &config_entry);
                plugin.config = config_entry;
                break;
            }
        }
    }
}

impl NativePluginContributionStore {
    fn from_plugins(plugins: &[NativePluginInfo]) -> Self {
        let mut store = Self::default();
        for plugin in plugins {
            if !native_plugin_contributions_enabled(plugin) {
                continue;
            }
            store.extend_from_plugin(plugin);
        }
        store
    }

    fn extend_from_plugin(&mut self, plugin: &NativePluginInfo) {
        let Some(contributes) = &plugin.manifest.contributes else {
            return;
        };
        let plugin_id = plugin.manifest.id.clone();
        let plugin_name = plugin.manifest.name.clone();

        if let Some(tabs) = &contributes.tabs {
            self.tabs.extend(
                tabs.iter()
                    .cloned()
                    .map(|definition| NativePluginTabContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        definition,
                    }),
            );
        }
        if let Some(sidebar_panels) = &contributes.sidebar_panels {
            self.sidebar_panels
                .extend(sidebar_panels.iter().cloned().map(|definition| {
                    NativePluginSidebarContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        definition,
                    }
                }));
        }
        if let Some(settings) = &contributes.settings {
            self.settings
                .extend(settings.iter().cloned().map(|definition| {
                    NativePluginSettingContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        definition,
                    }
                }));
        }
        if let Some(ai_tools) = &contributes.ai_tools {
            self.ai_tools
                .extend(ai_tools.iter().cloned().map(|definition| {
                    NativePluginAiToolContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        definition,
                    }
                }));
        }
        if let Some(hooks) = &contributes.terminal_hooks
            && let Some(shortcuts) = &hooks.shortcuts
        {
            self.terminal_shortcuts
                .extend(shortcuts.iter().cloned().map(|definition| {
                    NativePluginShortcutContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        definition,
                    }
                }));
        }
        if let Some(transports) = &contributes.terminal_transports {
            self.terminal_transports
                .extend(transports.iter().cloned().map(|transport| {
                    NativePluginTransportContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        transport,
                    }
                }));
        }
        if let Some(connection_hooks) = &contributes.connection_hooks {
            self.connection_hooks
                .extend(connection_hooks.iter().cloned().map(|hook| {
                    NativePluginConnectionHookContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        hook,
                    }
                }));
        }
        if let Some(api_commands) = &contributes.api_commands {
            self.api_commands
                .extend(api_commands.iter().cloned().map(|command| {
                    NativePluginApiCommandContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        command,
                    }
                }));
        }
    }

    #[allow(dead_code)]
    pub fn total_count(&self) -> usize {
        self.tabs.len()
            + self.sidebar_panels.len()
            + self.settings.len()
            + self.ai_tools.len()
            + self.terminal_shortcuts.len()
            + self.terminal_transports.len()
            + self.connection_hooks.len()
            + self.api_commands.len()
            + self.runtime_commands.len()
            + self.runtime_keybindings.len()
            + self.runtime_context_menus.len()
            + self.runtime_status_items.len()
            + self.runtime_tab_views.len()
            + self.runtime_sidebar_panels.len()
            + self.runtime_event_subscriptions.len()
            + self.runtime_terminal_input_interceptors.len()
            + self.runtime_terminal_output_processors.len()
    }

    pub fn ai_tool_definitions(&self) -> Vec<oxideterm_ai::AiToolDefinition> {
        self.ai_tools
            .iter()
            .map(|tool| {
                let qualified_name =
                    native_plugin_ai_tool_name(&tool.plugin_id, &tool.definition.name);
                // Phase 2 exposes metadata to the model but keeps execution
                // guarded by the native runtime boundary that starts in Phase 3.
                oxideterm_ai::AiToolDefinition {
                    name: qualified_name,
                    description: format!(
                        "[Plugin: {}] {}",
                        tool.plugin_name, tool.definition.description
                    ),
                    parameters: tool.definition.parameters.clone().unwrap_or_else(
                        || serde_json::json!({ "type": "object", "properties": {} }),
                    ),
                }
            })
            .collect()
    }

    pub fn ai_tool_names(&self) -> Vec<String> {
        self.ai_tools
            .iter()
            .map(|tool| native_plugin_ai_tool_name(&tool.plugin_id, &tool.definition.name))
            .collect()
    }

    pub fn runtime_keybinding_for_normalized_key(
        &self,
        normalized_keybinding: &str,
    ) -> Option<&NativePluginRuntimeKeybindingContribution> {
        // Tauri's plugin store iterates Map values and returns the first
        // normalized match, so earlier plugin registrations keep priority when
        // two plugins claim the same keybinding.
        self.runtime_keybindings
            .iter()
            .find(|entry| entry.normalized_keybinding == normalized_keybinding)
    }

    pub fn runtime_event_subscriptions_for(
        &self,
        event: &str,
    ) -> Vec<NativePluginRuntimeEventSubscriptionContribution> {
        // Event delivery runs outside render, so clone the compact subscription
        // rows here and let WorkspaceApp hand them to the async runtime bridge.
        self.runtime_event_subscriptions
            .iter()
            .filter(|entry| entry.event == event)
            .cloned()
            .collect()
    }

    pub fn tab_contribution(
        &self,
        plugin_id: &str,
        tab_id: &str,
    ) -> Option<NativePluginTabContribution> {
        self.tabs
            .iter()
            .find(|entry| entry.plugin_id == plugin_id && entry.definition.id == tab_id)
            .cloned()
    }

    pub fn runtime_tab_view(
        &self,
        plugin_id: &str,
        tab_id: &str,
    ) -> Option<NativePluginRuntimeTabViewContribution> {
        self.runtime_tab_views
            .iter()
            .find(|entry| entry.plugin_id == plugin_id && entry.tab_id == tab_id)
            .cloned()
    }

    pub fn runtime_sidebar_panels(&self) -> Vec<NativePluginRuntimeSidebarPanelContribution> {
        let mut panels = self.runtime_sidebar_panels.clone();
        // Tauri lets sidebar panel definitions opt into top/bottom groups. Keep
        // that position ordering before title sorting so native panels do not
        // reshuffle every render.
        panels.sort_by(|left, right| {
            native_plugin_sidebar_position_sort_key(&left.position)
                .cmp(&native_plugin_sidebar_position_sort_key(&right.position))
                .then_with(|| left.title.cmp(&right.title))
                .then_with(|| left.panel_id.cmp(&right.panel_id))
        });
        panels
    }

    fn apply_runtime_tab_view(
        &mut self,
        registration: PluginRegistration,
        plugin_name: String,
        manifest: &NativePluginManifest,
    ) -> Result<(), String> {
        validate_manifest_text_field("runtime.tab.registrationId", &registration.registration_id)?;
        let tab_id = runtime_metadata_string(&registration.metadata, "tabId")
            .or_else(|| runtime_metadata_string(&registration.metadata, "id"))
            .ok_or_else(|| "Runtime tab registration requires metadata.tabId".to_string())?;
        validate_manifest_text_field("runtime.tab.tabId", &tab_id)?;
        let tab_def = manifest_declared_tab(manifest, &tab_id)
            .ok_or_else(|| format!("Tab \"{tab_id}\" not declared in manifest contributes.tabs"))?;
        let schema = runtime_declarative_ui_schema(&registration.metadata)?;
        validate_native_plugin_declarative_ui_schema(&schema)?;

        // Native replaces the Tauri React component with a validated data
        // schema. Re-registering the same id acts as the first patch mechanism
        // and goes through identical validation before replacing host state.
        self.dispose_runtime_registration(&registration.plugin_id, &registration.registration_id);
        self.runtime_tab_views
            .push(NativePluginRuntimeTabViewContribution {
                plugin_id: registration.plugin_id,
                plugin_name,
                registration_id: registration.registration_id,
                tab_id,
                title: tab_def.title.clone(),
                icon: tab_def.icon.clone(),
                schema,
            });
        Ok(())
    }

    fn apply_runtime_sidebar_panel(
        &mut self,
        registration: PluginRegistration,
        plugin_name: String,
        manifest: &NativePluginManifest,
    ) -> Result<(), String> {
        validate_manifest_text_field(
            "runtime.sidebarPanel.registrationId",
            &registration.registration_id,
        )?;
        let panel_id = runtime_metadata_string(&registration.metadata, "panelId")
            .or_else(|| runtime_metadata_string(&registration.metadata, "id"))
            .ok_or_else(|| {
                "Runtime sidebar panel registration requires metadata.panelId".to_string()
            })?;
        validate_manifest_text_field("runtime.sidebarPanel.panelId", &panel_id)?;
        let panel_def = manifest_declared_sidebar_panel(manifest, &panel_id).ok_or_else(|| {
            format!(
                "Sidebar panel \"{panel_id}\" not declared in manifest contributes.sidebarPanels"
            )
        })?;
        let schema = runtime_declarative_ui_schema(&registration.metadata)?;
        validate_native_plugin_declarative_ui_schema(&schema)?;

        self.dispose_runtime_registration(&registration.plugin_id, &registration.registration_id);
        self.runtime_sidebar_panels
            .push(NativePluginRuntimeSidebarPanelContribution {
                plugin_id: registration.plugin_id,
                plugin_name,
                registration_id: registration.registration_id,
                panel_id,
                title: panel_def.title.clone(),
                icon: panel_def.icon.clone(),
                position: panel_def.position.clone(),
                schema,
            });
        Ok(())
    }

    fn apply_runtime_terminal_shortcut(
        &mut self,
        registration: PluginRegistration,
        plugin_name: String,
        manifest: &NativePluginManifest,
    ) -> Result<(), String> {
        validate_manifest_text_field(
            "runtime.terminalShortcut.registrationId",
            &registration.registration_id,
        )?;
        let command = runtime_metadata_string(&registration.metadata, "command")
            .or_else(|| runtime_metadata_string(&registration.metadata, "id"))
            .unwrap_or_else(|| registration.registration_id.clone());
        validate_manifest_text_field("runtime.terminalShortcut.command", &command)?;
        let declared = manifest
            .contributes
            .as_ref()
            .and_then(|contributes| contributes.terminal_hooks.as_ref())
            .and_then(|hooks| hooks.shortcuts.as_ref())
            .and_then(|shortcuts| shortcuts.iter().find(|shortcut| shortcut.command == command))
            .ok_or_else(|| {
                format!(
                    "Shortcut command \"{command}\" not declared in manifest contributes.terminalHooks.shortcuts"
                )
            })?;
        let normalized_keybinding = crate::keybindings::normalize_plugin_key_combo(&declared.key)
            .ok_or_else(|| {
            "Runtime terminal shortcut registration has no usable key parts".to_string()
        })?;

        // Tauri registerShortcut stores a key-to-handler map after validating
        // the command against the manifest. Native dispatches the same handler
        // by reusing the runtime command RPC path keyed by the declared command.
        self.dispose_runtime_registration(&registration.plugin_id, &registration.registration_id);
        self.runtime_keybindings
            .push(NativePluginRuntimeKeybindingContribution {
                plugin_id: registration.plugin_id,
                plugin_name,
                registration_id: registration.registration_id,
                keybinding: declared.key.clone(),
                normalized_keybinding,
                command: command.clone(),
                label: command,
            });
        Ok(())
    }

    fn apply_runtime_terminal_hook(
        &mut self,
        registration: PluginRegistration,
        plugin_name: String,
        manifest: &NativePluginManifest,
    ) -> Result<(), String> {
        validate_manifest_text_field(
            "runtime.terminalHook.registrationId",
            &registration.registration_id,
        )?;
        let hooks = manifest
            .contributes
            .as_ref()
            .and_then(|contributes| contributes.terminal_hooks.as_ref());
        let declared = match registration.kind {
            PluginRegistrationKind::TerminalInputInterceptor => hooks
                .and_then(|hooks| hooks.input_interceptor)
                .unwrap_or(false),
            PluginRegistrationKind::TerminalOutputProcessor => hooks
                .and_then(|hooks| hooks.output_processor)
                .unwrap_or(false),
            _ => false,
        };
        if !declared {
            let declaration = match registration.kind {
                PluginRegistrationKind::TerminalInputInterceptor => {
                    "inputInterceptor not declared in manifest contributes.terminalHooks"
                }
                PluginRegistrationKind::TerminalOutputProcessor => {
                    "outputProcessor not declared in manifest contributes.terminalHooks"
                }
                _ => "terminal hook not declared in manifest contributes.terminalHooks",
            };
            return Err(declaration.to_string());
        }
        let command = runtime_metadata_string(&registration.metadata, "command")
            .or_else(|| runtime_metadata_string(&registration.metadata, "id"))
            .unwrap_or_else(|| registration.registration_id.clone());
        validate_manifest_text_field("runtime.terminalHook.command", &command)?;
        let contribution = NativePluginRuntimeTerminalHookContribution {
            plugin_id: registration.plugin_id.clone(),
            plugin_name,
            registration_id: registration.registration_id.clone(),
            command,
        };

        // Tauri stores terminal hooks in registration order and removes them
        // through disposables. Native records the same ordered rows here; the
        // terminal I/O pipeline consumes these rows when hooks are executed.
        self.dispose_runtime_registration(&registration.plugin_id, &registration.registration_id);
        match registration.kind {
            PluginRegistrationKind::TerminalInputInterceptor => {
                self.runtime_terminal_input_interceptors.push(contribution);
            }
            PluginRegistrationKind::TerminalOutputProcessor => {
                self.runtime_terminal_output_processors.push(contribution);
            }
            _ => {}
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn apply_runtime_registration(
        &mut self,
        registration: PluginRegistration,
        plugin_name: String,
    ) -> Result<(), String> {
        validate_manifest_text_field("runtime.registrationId", &registration.registration_id)?;
        // Tauri disposables replace state by key. Native mirrors that by
        // removing an existing registration id before applying the latest
        // runtime payload from process/WASM.
        self.dispose_runtime_registration(&registration.plugin_id, &registration.registration_id);
        match registration.kind {
            PluginRegistrationKind::Command => {
                let command = runtime_metadata_string(&registration.metadata, "id")
                    .or_else(|| runtime_metadata_string(&registration.metadata, "command"))
                    .ok_or_else(|| {
                        "Runtime command registration requires metadata.id".to_string()
                    })?;
                let label = runtime_metadata_string(&registration.metadata, "label")
                    .unwrap_or_else(|| command.clone());
                validate_manifest_text_field("runtime.command.id", &command)?;
                validate_manifest_text_field("runtime.command.label", &label)?;
                self.runtime_commands
                    .push(NativePluginRuntimeCommandContribution {
                        plugin_id: registration.plugin_id,
                        plugin_name,
                        registration_id: registration.registration_id,
                        command,
                        label,
                        icon: runtime_metadata_string(&registration.metadata, "icon"),
                        shortcut: runtime_metadata_string(&registration.metadata, "shortcut"),
                        section: runtime_metadata_string(&registration.metadata, "section"),
                    });
            }
            PluginRegistrationKind::Keybinding => {
                let keybinding = runtime_metadata_string(&registration.metadata, "keybinding")
                    .or_else(|| runtime_metadata_string(&registration.metadata, "key"))
                    .ok_or_else(|| {
                        "Runtime keybinding registration requires metadata.keybinding".to_string()
                    })?;
                let command = runtime_metadata_string(&registration.metadata, "command")
                    .unwrap_or_else(|| registration.registration_id.clone());
                let label = runtime_metadata_string(&registration.metadata, "label")
                    .unwrap_or_else(|| command.clone());
                validate_manifest_text_field("runtime.keybinding.keybinding", &keybinding)?;
                validate_manifest_text_field("runtime.keybinding.command", &command)?;
                validate_manifest_text_field("runtime.keybinding.label", &label)?;
                let normalized_keybinding =
                    crate::keybindings::normalize_plugin_key_combo(&keybinding).ok_or_else(
                        || "Runtime keybinding registration has no usable key parts".to_string(),
                    )?;
                self.runtime_keybindings
                    .push(NativePluginRuntimeKeybindingContribution {
                        plugin_id: registration.plugin_id,
                        plugin_name,
                        registration_id: registration.registration_id,
                        keybinding,
                        normalized_keybinding,
                        command,
                        label,
                    });
            }
            PluginRegistrationKind::ContextMenu => {
                let target =
                    runtime_metadata_string(&registration.metadata, "target").ok_or_else(|| {
                        "Runtime context menu registration requires metadata.target".to_string()
                    })?;
                validate_one_of(
                    "runtime.contextMenu.target",
                    &target,
                    &["terminal", "sftp", "tab", "sidebar"],
                )?;
                let items = runtime_context_menu_items(&registration.metadata)?;
                self.runtime_context_menus
                    .push(NativePluginRuntimeContextMenuContribution {
                        plugin_id: registration.plugin_id,
                        plugin_name,
                        registration_id: registration.registration_id,
                        target,
                        items,
                    });
            }
            PluginRegistrationKind::StatusBar => {
                let text =
                    runtime_metadata_string(&registration.metadata, "text").ok_or_else(|| {
                        "Runtime status item registration requires metadata.text".to_string()
                    })?;
                validate_manifest_text_field("runtime.statusBar.text", &text)?;
                let alignment = runtime_metadata_string(&registration.metadata, "alignment")
                    .unwrap_or_else(|| "left".to_string());
                validate_one_of(
                    "runtime.statusBar.alignment",
                    &alignment,
                    &["left", "right"],
                )?;
                self.runtime_status_items
                    .push(NativePluginRuntimeStatusItemContribution {
                        plugin_id: registration.plugin_id,
                        plugin_name,
                        registration_id: registration.registration_id,
                        text,
                        icon: runtime_metadata_string(&registration.metadata, "icon"),
                        tooltip: runtime_metadata_string(&registration.metadata, "tooltip"),
                        alignment,
                        priority: registration
                            .metadata
                            .get("priority")
                            .and_then(serde_json::Value::as_i64),
                    });
            }
            PluginRegistrationKind::Tab | PluginRegistrationKind::SidebarPanel => {
                return Err(format!(
                    "Runtime registration kind {:?} must be validated against manifest declarations",
                    registration.kind
                ));
            }
            PluginRegistrationKind::EventSubscription => {
                let event =
                    runtime_subscription_event(&registration.metadata, &registration.plugin_id)?;
                let filter = registration
                    .metadata
                    .get("filter")
                    .cloned()
                    .or_else(|| runtime_metadata_node_filter(&registration.metadata));
                self.runtime_event_subscriptions.push(
                    NativePluginRuntimeEventSubscriptionContribution {
                        plugin_id: registration.plugin_id,
                        plugin_name,
                        registration_id: registration.registration_id,
                        event,
                        filter,
                    },
                );
            }
            _ => {
                return Err(format!(
                    "Runtime registration kind {:?} is not a Phase 3 UI contribution",
                    registration.kind
                ));
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn dispose_runtime_registration(&mut self, plugin_id: &str, registration_id: &str) -> bool {
        let before = self.runtime_commands.len()
            + self.runtime_keybindings.len()
            + self.runtime_context_menus.len()
            + self.runtime_status_items.len()
            + self.runtime_tab_views.len()
            + self.runtime_sidebar_panels.len()
            + self.runtime_event_subscriptions.len()
            + self.runtime_terminal_input_interceptors.len()
            + self.runtime_terminal_output_processors.len();
        self.runtime_commands.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_keybindings.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_context_menus.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_status_items.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_tab_views.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_sidebar_panels.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_event_subscriptions.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_terminal_input_interceptors.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_terminal_output_processors.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        let after = self.runtime_commands.len()
            + self.runtime_keybindings.len()
            + self.runtime_context_menus.len()
            + self.runtime_status_items.len()
            + self.runtime_tab_views.len()
            + self.runtime_sidebar_panels.len()
            + self.runtime_event_subscriptions.len()
            + self.runtime_terminal_input_interceptors.len()
            + self.runtime_terminal_output_processors.len();
        before != after
    }

    #[allow(dead_code)]
    fn cleanup_runtime_plugin_contributions(&mut self, plugin_id: &str) -> usize {
        let before = self.runtime_commands.len()
            + self.runtime_keybindings.len()
            + self.runtime_context_menus.len()
            + self.runtime_status_items.len()
            + self.runtime_tab_views.len()
            + self.runtime_sidebar_panels.len()
            + self.runtime_event_subscriptions.len()
            + self.runtime_terminal_input_interceptors.len()
            + self.runtime_terminal_output_processors.len();
        self.runtime_commands
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_keybindings
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_context_menus
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_status_items
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_tab_views
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_sidebar_panels
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_event_subscriptions
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_terminal_input_interceptors
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_terminal_output_processors
            .retain(|entry| entry.plugin_id != plugin_id);
        let after = self.runtime_commands.len()
            + self.runtime_keybindings.len()
            + self.runtime_context_menus.len()
            + self.runtime_status_items.len()
            + self.runtime_tab_views.len()
            + self.runtime_sidebar_panels.len()
            + self.runtime_event_subscriptions.len()
            + self.runtime_terminal_input_interceptors.len()
            + self.runtime_terminal_output_processors.len();
        before.saturating_sub(after)
    }
}

fn native_plugin_contributions_enabled(plugin: &NativePluginInfo) -> bool {
    matches!(
        plugin.state,
        NativePluginState::ReadyManifestOnly
            | NativePluginState::ReadyWasm
            | NativePluginState::ReadyProcess
            | NativePluginState::Active
    )
}

pub(super) fn native_plugin_ai_tool_name(plugin_id: &str, tool_name: &str) -> String {
    format!(
        "plugin::{}::{}",
        sanitize_plugin_tool_part(plugin_id),
        sanitize_plugin_tool_part(tool_name)
    )
}

pub(super) fn is_native_plugin_ai_tool_name(tool_name: &str) -> bool {
    tool_name.starts_with("plugin::")
}

fn sanitize_plugin_tool_part(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "unnamed".to_string()
    } else {
        sanitized
    }
}

pub(super) fn native_plugins_dir(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(PLUGINS_DIR_NAME)
}

pub(super) fn native_plugin_config_path(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(PLUGIN_CONFIG_FILENAME)
}

fn native_plugins_dir_from_config_path(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or(config_path)
        .join(PLUGINS_DIR_NAME)
}

fn settings_path_from_native_plugin_config_path(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or(config_path)
        .join("settings.json")
}

fn discover_native_plugins_in_dir(
    plugins_dir: &Path,
    config: &NativePluginGlobalConfig,
) -> (Vec<NativePluginInfo>, Vec<NativePluginDiagnostic>) {
    let entries = match fs::read_dir(plugins_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (Vec::new(), Vec::new());
        }
        Err(error) => {
            return (
                Vec::new(),
                vec![NativePluginDiagnostic {
                    plugin_dir: plugins_dir.to_path_buf(),
                    plugin_id: None,
                    message: format!("Cannot read plugin directory: {error}"),
                }],
            );
        }
    };

    let mut plugins = Vec::new();
    let mut diagnostics = Vec::new();
    for entry in entries.flatten() {
        let plugin_dir = entry.path();
        if !plugin_dir.is_dir() {
            continue;
        }
        match load_native_plugin_manifest(&plugin_dir, config) {
            Ok(info) => plugins.push(info),
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
    plugins.sort_by(|left, right| left.manifest.name.cmp(&right.manifest.name));
    diagnostics.sort_by(|left, right| left.plugin_dir.cmp(&right.plugin_dir));
    (plugins, diagnostics)
}

fn load_native_plugin_manifest(
    plugin_dir: &Path,
    config: &NativePluginGlobalConfig,
) -> Result<NativePluginInfo, NativePluginDiagnostic> {
    let manifest_path = plugin_dir.join(PLUGIN_MANIFEST_FILENAME);
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| {
        native_plugin_diagnostic(
            plugin_dir,
            None,
            format!("Cannot read plugin.json: {error}"),
        )
    })?;
    let manifest =
        serde_json::from_str::<NativePluginManifest>(&manifest_text).map_err(|error| {
            native_plugin_diagnostic(plugin_dir, None, format!("Invalid plugin.json: {error}"))
        })?;
    validate_native_plugin_manifest(&manifest)
        .map_err(|error| native_plugin_diagnostic(plugin_dir, Some(manifest.id.clone()), error))?;
    let runtime_plan = native_runtime_plan_for_manifest(&manifest)
        .map_err(|error| native_plugin_diagnostic(plugin_dir, Some(manifest.id.clone()), error))?;
    validate_runtime_entry_exists(plugin_dir, &runtime_plan)
        .map_err(|error| native_plugin_diagnostic(plugin_dir, Some(manifest.id.clone()), error))?;
    let config_entry = config
        .plugins
        .get(&manifest.id)
        .cloned()
        .unwrap_or_else(NativePluginConfigEntry::default);
    let state = native_plugin_state_for(&runtime_plan, &config_entry);
    Ok(NativePluginInfo {
        manifest,
        install_dir: plugin_dir.to_path_buf(),
        runtime_plan,
        state,
        config: config_entry,
    })
}

fn native_plugin_diagnostic(
    plugin_dir: &Path,
    plugin_id: Option<String>,
    message: String,
) -> NativePluginDiagnostic {
    NativePluginDiagnostic {
        plugin_dir: plugin_dir.to_path_buf(),
        plugin_id,
        message,
    }
}

pub(super) fn load_native_plugin_config(config_path: &Path) -> NativePluginGlobalConfig {
    let Ok(contents) = fs::read_to_string(config_path) else {
        return NativePluginGlobalConfig::default();
    };
    if contents.trim().is_empty() {
        return NativePluginGlobalConfig::default();
    }
    match serde_json::from_str::<NativePluginGlobalConfig>(&contents) {
        Ok(config) => config,
        Err(_) => {
            quarantine_corrupt_native_plugin_config(config_path);
            NativePluginGlobalConfig::default()
        }
    }
}

pub(super) fn save_native_plugin_config(
    config_path: &Path,
    config: &NativePluginGlobalConfig,
) -> Result<(), String> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_vec_pretty(config).map_err(|error| error.to_string())?;
    fs::write(config_path, json).map_err(|error| error.to_string())
}

fn validate_native_plugin_manifest(manifest: &NativePluginManifest) -> Result<(), String> {
    validate_native_plugin_id(&manifest.id)?;
    validate_manifest_text_field("name", &manifest.name)?;
    validate_manifest_text_field("version", &manifest.version)?;
    if let Some(main) = &manifest.main {
        validate_plugin_relative_path(main)?;
    }
    if let Some(runtime) = &manifest.runtime {
        validate_plugin_relative_path(&runtime.entry)?;
    }
    if let Some(assets) = &manifest.assets {
        validate_plugin_relative_path(assets)?;
    }
    if let Some(styles) = &manifest.styles {
        for style_path in styles {
            validate_plugin_relative_path(style_path)?;
        }
    }
    if let Some(locales) = &manifest.locales {
        validate_plugin_relative_path(locales)?;
    }
    if let Some(contributes) = &manifest.contributes {
        validate_native_plugin_contributions(contributes)?;
    }
    Ok(())
}

fn validate_native_plugin_contributions(
    contributes: &NativePluginContributes,
) -> Result<(), String> {
    if let Some(tabs) = &contributes.tabs {
        for tab in tabs {
            validate_manifest_text_field("contributes.tabs.id", &tab.id)?;
            validate_manifest_text_field("contributes.tabs.title", &tab.title)?;
            validate_manifest_text_field("contributes.tabs.icon", &tab.icon)?;
        }
    }
    if let Some(sidebar_panels) = &contributes.sidebar_panels {
        for panel in sidebar_panels {
            validate_manifest_text_field("contributes.sidebarPanels.id", &panel.id)?;
            validate_manifest_text_field("contributes.sidebarPanels.title", &panel.title)?;
            validate_manifest_text_field("contributes.sidebarPanels.icon", &panel.icon)?;
            validate_one_of(
                "contributes.sidebarPanels.position",
                &panel.position,
                &["top", "bottom"],
            )?;
        }
    }
    if let Some(settings) = &contributes.settings {
        for setting in settings {
            validate_manifest_text_field("contributes.settings.id", &setting.id)?;
            validate_manifest_text_field("contributes.settings.title", &setting.title)?;
            validate_one_of(
                "contributes.settings.type",
                &setting.setting_type,
                &["string", "number", "boolean", "select"],
            )?;
            if setting.setting_type == "select" {
                let options = setting.options.as_ref().ok_or_else(|| {
                    "Select plugin settings require contributes.settings.options".to_string()
                })?;
                for option in options {
                    validate_manifest_text_field(
                        "contributes.settings.options.label",
                        &option.label,
                    )?;
                    if !(option.value.is_string() || option.value.is_number()) {
                        return Err(
                            "Select plugin setting option values must be strings or numbers"
                                .to_string(),
                        );
                    }
                }
            }
            validate_plugin_setting_value(setting, &setting.default)?;
        }
    }
    if let Some(hooks) = &contributes.terminal_hooks
        && let Some(shortcuts) = &hooks.shortcuts
    {
        for shortcut in shortcuts {
            validate_manifest_text_field("contributes.terminalHooks.shortcuts.key", &shortcut.key)?;
            validate_manifest_text_field(
                "contributes.terminalHooks.shortcuts.command",
                &shortcut.command,
            )?;
        }
    }
    if let Some(transports) = &contributes.terminal_transports {
        for transport in transports {
            validate_one_of("contributes.terminalTransports", transport, &["telnet"])?;
        }
    }
    if let Some(connection_hooks) = &contributes.connection_hooks {
        for hook in connection_hooks {
            validate_one_of(
                "contributes.connectionHooks",
                hook,
                &["onConnect", "onDisconnect", "onReconnect", "onLinkDown"],
            )?;
        }
    }
    if let Some(ai_tools) = &contributes.ai_tools {
        for tool in ai_tools {
            validate_manifest_text_field("contributes.aiTools.name", &tool.name)?;
            validate_manifest_text_field("contributes.aiTools.description", &tool.description)?;
            if let Some(capabilities) = &tool.capabilities {
                for capability in capabilities {
                    validate_one_of(
                        "contributes.aiTools.capabilities",
                        capability,
                        &[
                            "command.run",
                            "terminal.send",
                            "terminal.observe",
                            "terminal.wait",
                            "filesystem.read",
                            "filesystem.write",
                            "filesystem.search",
                            "navigation.open",
                            "state.list",
                            "network.forward",
                            "settings.read",
                            "settings.write",
                            "plugin.invoke",
                            "mcp.invoke",
                        ],
                    )?;
                }
            }
            if let Some(risk) = &tool.risk {
                validate_one_of(
                    "contributes.aiTools.risk",
                    risk,
                    &[
                        "read",
                        "write-file",
                        "execute-command",
                        "interactive-input",
                        "destructive",
                        "network-expose",
                        "settings-change",
                        "credential-sensitive",
                    ],
                )?;
            }
            if let Some(target_kinds) = &tool.target_kinds {
                for target_kind in target_kinds {
                    validate_one_of(
                        "contributes.aiTools.targetKinds",
                        target_kind,
                        &[
                            "local-shell",
                            "ssh-node",
                            "terminal-session",
                            "sftp-session",
                            "ide-workspace",
                            "app-tab",
                            "mcp-server",
                            "rag-index",
                        ],
                    )?;
                }
            }
        }
    }
    if let Some(api_commands) = &contributes.api_commands {
        for command in api_commands {
            validate_manifest_text_field("contributes.apiCommands", command)?;
        }
    }
    Ok(())
}

fn validate_runtime_entry_exists(
    plugin_dir: &Path,
    runtime_plan: &NativePluginRuntimePlan,
) -> Result<(), String> {
    let entry = match runtime_plan {
        NativePluginRuntimePlan::Wasm { entry } | NativePluginRuntimePlan::Process { entry } => {
            entry
        }
        NativePluginRuntimePlan::ManifestOnly
        | NativePluginRuntimePlan::UnsupportedLegacyJs { .. } => return Ok(()),
    };
    let entry_path = plugin_dir.join(entry);
    if !entry_path.is_file() {
        return Err(format!(
            "Native plugin runtime entry \"{entry}\" does not exist"
        ));
    }
    Ok(())
}

fn quarantine_corrupt_native_plugin_config(config_path: &Path) {
    let Some(file_name) = config_path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let backup_name = format!("{file_name}.{PLUGIN_CONFIG_CORRUPT_MARKER}-{timestamp_ms}");
    let backup_path = config_path.with_file_name(backup_name);
    // Bad plugin config should not keep breaking startup. Preserve the raw file
    // next to the original path for manual inspection, then let discovery save
    // a fresh schema-valid config.
    let _ = fs::rename(config_path, backup_path);
}

#[allow(dead_code)]
fn install_native_plugin_package_bytes(
    settings_path: &Path,
    package_bytes: &[u8],
    checksum: Option<&str>,
    overwrite: bool,
) -> Result<NativePluginUrlInstallResult, String> {
    if package_bytes.len() as u64 > PLUGIN_PACKAGE_MAX_BYTES {
        return Err(format!(
            "Plugin package too large: {} bytes (max {} bytes)",
            package_bytes.len(),
            PLUGIN_PACKAGE_MAX_BYTES
        ));
    }
    let actual_checksum = native_plugin_sha256_hex(package_bytes);
    if let Some(expected_checksum) = checksum {
        verify_native_plugin_checksum(package_bytes, expected_checksum)?;
    }

    let plugins_dir = native_plugins_dir(settings_path);
    fs::create_dir_all(&plugins_dir)
        .map_err(|error| format!("Failed to create plugins directory: {error}"))?;
    let staging_dir = plugins_dir.join(native_plugin_staging_dir_name("url-install"));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir)
            .map_err(|error| format!("Failed to clean staging dir: {error}"))?;
    }
    fs::create_dir_all(&staging_dir)
        .map_err(|error| format!("Failed to create staging dir: {error}"))?;

    let install_result = (|| {
        extract_native_plugin_zip(package_bytes, &staging_dir)?;
        let source_dir = native_plugin_package_root(&staging_dir)?;
        let manifest = read_native_plugin_manifest_from_dir(&source_dir)?;
        validate_native_plugin_id(&manifest.id)
            .map_err(|error| format!("Invalid plugin ID in manifest: {error}"))?;
        let dest_dir = plugins_dir.join(&manifest.id);
        let backup_dir = plugins_dir.join(format!(".{}-backup", manifest.id));
        let replaced_existing = dest_dir.exists();
        if replaced_existing && !overwrite {
            return Err(format!("PLUGIN_ID_CONFLICT:{}", manifest.id));
        }
        if backup_dir.exists() {
            fs::remove_dir_all(&backup_dir)
                .map_err(|error| format!("Failed to remove stale backup: {error}"))?;
        }
        if dest_dir.exists() {
            fs::rename(&dest_dir, &backup_dir)
                .map_err(|error| format!("Failed to backup old plugin: {error}"))?;
        }

        // Install uses staging + backup under the plugin directory so the final
        // rename is same-filesystem and rollback can restore the previous copy.
        match fs::rename(&source_dir, &dest_dir) {
            Ok(()) => {
                if backup_dir.exists() {
                    fs::remove_dir_all(&backup_dir)
                        .map_err(|error| format!("Failed to remove plugin backup: {error}"))?;
                }
            }
            Err(error) => {
                if backup_dir.exists() {
                    fs::rename(&backup_dir, &dest_dir).map_err(|restore_error| {
                        format!(
                            "Failed to finalize plugin install: {error}. Rollback also failed: {restore_error}"
                        )
                    })?;
                }
                return Err(format!("Failed to finalize plugin install: {error}"));
            }
        }

        Ok(NativePluginUrlInstallResult {
            manifest,
            checksum: actual_checksum,
            replaced_existing,
        })
    })();

    if staging_dir.exists() {
        let _ = fs::remove_dir_all(&staging_dir);
    }
    install_result
}

#[allow(dead_code)]
fn extract_native_plugin_zip(package_bytes: &[u8], dest: &Path) -> Result<(), String> {
    let cursor = Cursor::new(package_bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|error| format!("Invalid ZIP archive: {error}"))?;
    if archive.len() > PLUGIN_PACKAGE_MAX_ENTRIES {
        return Err(format!(
            "Plugin archive contains too many entries: {} (max {})",
            archive.len(),
            PLUGIN_PACKAGE_MAX_ENTRIES
        ));
    }

    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("Failed to read ZIP entry {index}: {error}"))?;
        if native_plugin_zip_entry_is_symlink(file.unix_mode()) {
            return Err(format!(
                "Plugin archive contains unsupported symlink entry: {}",
                file.name()
            ));
        }
        let relative_path = file
            .enclosed_name()
            .ok_or_else(|| format!("Plugin archive entry escapes target dir: {}", file.name()))?
            .to_path_buf();
        let out_path = dest.join(relative_path);
        if file.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|error| format!("Failed to create dir {:?}: {error}", out_path))?;
            continue;
        }
        extracted_bytes = extracted_bytes
            .checked_add(file.size())
            .ok_or_else(|| "Plugin archive extracted size overflowed".to_string())?;
        if extracted_bytes > PLUGIN_PACKAGE_MAX_EXTRACTED_BYTES {
            return Err(format!(
                "Plugin archive expands to {} bytes (max {} bytes)",
                extracted_bytes, PLUGIN_PACKAGE_MAX_EXTRACTED_BYTES
            ));
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create parent dir {:?}: {error}", parent))?;
        }
        let mut out_file = fs::File::create(&out_path)
            .map_err(|error| format!("Failed to create file {:?}: {error}", out_path))?;
        std::io::copy(&mut file, &mut out_file)
            .map_err(|error| format!("Failed to write file {:?}: {error}", out_path))?;
    }
    Ok(())
}

#[allow(dead_code)]
fn native_plugin_package_root(staging_dir: &Path) -> Result<PathBuf, String> {
    if staging_dir.join(PLUGIN_MANIFEST_FILENAME).exists() {
        return Ok(staging_dir.to_path_buf());
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(staging_dir)
        .map_err(|error| format!("Failed to read staging directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Failed to read staging entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Failed to inspect staging entry: {error}"))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() && entry.path().join(PLUGIN_MANIFEST_FILENAME).exists() {
            candidates.push(entry.path());
        }
    }
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => Err("No plugin.json found in package (checked root and subdirectories)".to_string()),
        _ => Err("Multiple nested plugin.json files found in package".to_string()),
    }
}

#[allow(dead_code)]
fn read_native_plugin_manifest_from_dir(plugin_dir: &Path) -> Result<NativePluginManifest, String> {
    let manifest_path = plugin_dir.join(PLUGIN_MANIFEST_FILENAME);
    let manifest_json = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("Failed to read plugin.json: {error}"))?;
    serde_json::from_str(&manifest_json).map_err(|error| format!("Invalid plugin.json: {error}"))
}

#[allow(dead_code)]
fn verify_native_plugin_checksum(package_bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = native_plugin_sha256_hex(package_bytes);
    let expected_hex = expected
        .strip_prefix("sha256:")
        .unwrap_or(expected)
        .to_lowercase();
    if actual != expected_hex {
        return Err(format!(
            "Checksum mismatch: expected {expected_hex}, got {actual}"
        ));
    }
    Ok(())
}

#[allow(dead_code)]
fn native_plugin_sha256_hex(package_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(package_bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[allow(dead_code)]
fn native_plugin_zip_entry_is_symlink(unix_mode: Option<u32>) -> bool {
    unix_mode.is_some_and(|mode| (mode & 0o170000) == 0o120000)
}

#[allow(dead_code)]
fn native_plugin_staging_dir_name(prefix: &str) -> String {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!(".{prefix}-{timestamp_ms}")
}

#[allow(dead_code)]
fn native_plugin_version_is_newer(new_version: &str, old_version: &str) -> bool {
    let new_parts = native_plugin_version_parts(new_version);
    let old_parts = native_plugin_version_parts(old_version);
    for index in 0..new_parts.len().max(old_parts.len()) {
        let new_part = new_parts.get(index).copied().unwrap_or(0);
        let old_part = old_parts.get(index).copied().unwrap_or(0);
        if new_part > old_part {
            return true;
        }
        if new_part < old_part {
            return false;
        }
    }
    false
}

#[allow(dead_code)]
fn native_plugin_version_parts(version: &str) -> Vec<u32> {
    version
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok())
        .collect()
}

#[allow(dead_code)]
fn validate_native_plugin_package_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|error| format!("Invalid URL: {error}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => Err(format!(
            "Unsupported URL scheme: {scheme}. Only http and https are allowed."
        )),
    }
}

fn validate_one_of(field: &str, value: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&value) {
        return Ok(());
    }
    Err(format!(
        "Plugin manifest field \"{field}\" has unsupported value \"{value}\""
    ))
}

fn validate_plugin_setting_value(
    setting: &NativePluginSettingDef,
    value: &Value,
) -> Result<(), String> {
    match setting.setting_type.as_str() {
        "string" => {
            if value.is_string() {
                Ok(())
            } else {
                Err(format!(
                    "Plugin setting \"{}\" requires a string",
                    setting.id
                ))
            }
        }
        "number" => {
            if value.is_number() {
                Ok(())
            } else {
                Err(format!(
                    "Plugin setting \"{}\" requires a number",
                    setting.id
                ))
            }
        }
        "boolean" => {
            if value.is_boolean() {
                Ok(())
            } else {
                Err(format!(
                    "Plugin setting \"{}\" requires a boolean",
                    setting.id
                ))
            }
        }
        "select" => {
            let allowed = setting
                .options
                .as_ref()
                .is_some_and(|options| options.iter().any(|option| option.value == *value));
            if allowed {
                Ok(())
            } else {
                Err(format!(
                    "Plugin setting \"{}\" requires one of its declared select options",
                    setting.id
                ))
            }
        }
        _ => Err(format!(
            "Plugin setting \"{}\" has unsupported type \"{}\"",
            setting.id, setting.setting_type
        )),
    }
}

fn validate_plugin_storage_key(key: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("Plugin storage key cannot be empty".to_string());
    }
    if key.len() > PLUGIN_STORAGE_MAX_KEY_BYTES {
        return Err(format!(
            "Plugin storage key exceeds {} bytes",
            PLUGIN_STORAGE_MAX_KEY_BYTES
        ));
    }
    if key.bytes().any(|byte| byte < 0x20) {
        return Err("Plugin storage key contains invalid characters".to_string());
    }
    Ok(())
}

fn validate_plugin_storage_size(values: &HashMap<String, Value>) -> Result<(), String> {
    let encoded = serde_json::to_vec(values).map_err(|error| error.to_string())?;
    if encoded.len() > PLUGIN_STORAGE_MAX_PLUGIN_BYTES {
        return Err(format!(
            "Plugin storage exceeds {} bytes",
            PLUGIN_STORAGE_MAX_PLUGIN_BYTES
        ));
    }
    Ok(())
}

fn validate_manifest_text_field(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("Plugin manifest field \"{field}\" cannot be empty"));
    }
    Ok(())
}

#[allow(dead_code)]
fn runtime_metadata_string(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn runtime_metadata_node_filter(metadata: &Value) -> Option<Value> {
    runtime_metadata_string(metadata, "nodeId")
        .map(|node_id| serde_json::json!({ "nodeId": node_id }))
}

fn manifest_declared_tab<'a>(
    manifest: &'a NativePluginManifest,
    tab_id: &str,
) -> Option<&'a NativePluginTabDef> {
    manifest
        .contributes
        .as_ref()
        .and_then(|contributes| contributes.tabs.as_ref())
        .and_then(|tabs| tabs.iter().find(|tab| tab.id == tab_id))
}

fn manifest_declared_sidebar_panel<'a>(
    manifest: &'a NativePluginManifest,
    panel_id: &str,
) -> Option<&'a NativePluginSidebarDef> {
    manifest
        .contributes
        .as_ref()
        .and_then(|contributes| contributes.sidebar_panels.as_ref())
        .and_then(|panels| panels.iter().find(|panel| panel.id == panel_id))
}

fn runtime_declarative_ui_schema(
    metadata: &Value,
) -> Result<NativePluginDeclarativeUiSchema, String> {
    let schema = metadata.get("schema").unwrap_or(metadata);
    serde_json::from_value(schema.clone())
        .map_err(|error| format!("Runtime declarative UI schema is invalid: {error}"))
}

fn validate_native_plugin_declarative_ui_schema(
    schema: &NativePluginDeclarativeUiSchema,
) -> Result<(), String> {
    validate_one_of(
        "runtime.declarativeUi.kind",
        &schema.kind,
        &[NATIVE_PLUGIN_DECLARATIVE_UI_FORM_KIND],
    )?;
    if schema.sections.is_empty() && schema.controls.is_empty() {
        return Err("Runtime declarative UI schema requires sections or controls".to_string());
    }
    for section in &schema.sections {
        validate_manifest_text_field("runtime.declarativeUi.sections.id", &section.id)?;
        validate_native_plugin_declarative_controls(&section.controls)?;
    }
    validate_native_plugin_declarative_controls(&schema.controls)
}

fn validate_native_plugin_declarative_controls(
    controls: &[NativePluginDeclarativeUiControl],
) -> Result<(), String> {
    for control in controls {
        validate_one_of(
            "runtime.declarativeUi.controls.kind",
            &control.kind,
            NATIVE_PLUGIN_DECLARATIVE_UI_CONTROL_KINDS,
        )?;
        if native_plugin_declarative_control_requires_id(&control.kind)
            && control.id.as_deref().is_none_or(str::is_empty)
        {
            return Err(format!(
                "Runtime declarative UI control kind \"{}\" requires id",
                control.kind
            ));
        }
        if let Some(options) = &control.options {
            for option in options {
                validate_manifest_text_field(
                    "runtime.declarativeUi.controls.options.label",
                    &option.label,
                )?;
            }
        }
    }
    Ok(())
}

pub(super) fn native_plugin_declarative_control_is_actionable(
    control: &NativePluginDeclarativeUiControl,
) -> bool {
    control.kind == "button" && !control.disabled && !control.loading && control.id.is_some()
}

fn native_plugin_declarative_control_requires_id(kind: &str) -> bool {
    matches!(
        kind,
        "text" | "password" | "number" | "checkbox" | "select" | "button"
    )
}

fn native_plugin_sidebar_position_sort_key(position: &str) -> u8 {
    match position {
        "top" => 0,
        "bottom" => 1,
        _ => 2,
    }
}

#[allow(dead_code)]
fn runtime_context_menu_items(
    metadata: &Value,
) -> Result<Vec<NativePluginRuntimeContextMenuItem>, String> {
    let items = metadata
        .get("items")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Runtime context menu registration requires metadata.items".to_string())?;
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        let label = item
            .get("label")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "Runtime context menu item requires label".to_string())?
            .to_string();
        validate_manifest_text_field("runtime.contextMenu.items.label", &label)?;
        parsed.push(NativePluginRuntimeContextMenuItem {
            label,
            icon: runtime_metadata_string(item, "icon"),
            // Tauri allowed a render-time `when()` predicate. Native cannot run
            // arbitrary plugin code while painting a menu, so runtime plugins
            // must send the current enabled state as data.
            enabled: item
                .get("enabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
        });
    }
    Ok(parsed)
}

fn runtime_subscription_event(
    metadata: &Value,
    subscriber_plugin_id: &str,
) -> Result<String, String> {
    if runtime_metadata_string(metadata, "namespace").as_deref() == Some("events")
        && runtime_metadata_string(metadata, "method").as_deref() == Some("on")
    {
        let event_name = runtime_metadata_string(metadata, "name")
            .or_else(|| runtime_metadata_string(metadata, "event"))
            .ok_or_else(|| "Runtime events.on subscription requires metadata.name".to_string())?;
        let owner_plugin_id = runtime_metadata_string(metadata, "pluginId")
            .or_else(|| runtime_metadata_string(metadata, "ownerPluginId"))
            .unwrap_or_else(|| subscriber_plugin_id.to_string());
        return native_plugin_custom_event_key(&owner_plugin_id, &event_name);
    }

    let event = runtime_metadata_string(metadata, "event")
        .or_else(|| runtime_subscription_event_from_method(metadata))
        .ok_or_else(|| {
            "Runtime event subscription requires metadata.event or metadata.namespace/method"
                .to_string()
        })?;
    if event.starts_with("plugin.") {
        validate_plugin_event_key(&event)?;
        return Ok(event);
    }
    validate_one_of(
        "runtime.eventSubscription.event",
        &event,
        NATIVE_PLUGIN_PHASE4_SUBSCRIPTION_EVENTS,
    )?;
    Ok(event)
}

fn runtime_subscription_event_from_method(metadata: &Value) -> Option<String> {
    let namespace = runtime_metadata_string(metadata, "namespace")?;
    let method = runtime_metadata_string(metadata, "method")?;
    // Native replaces JS callback registration methods with stable event names
    // that a process/WASM runtime can receive through PluginEvent frames.
    match (namespace.as_str(), method.as_str()) {
        ("app", "onThemeChange") => Some(NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT.to_string()),
        ("app", "onSettingsChange") => Some(NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT.to_string()),
        ("i18n", "onLanguageChange") => Some(NATIVE_PLUGIN_I18N_LANGUAGE_CHANGED_EVENT.to_string()),
        ("settings", "onChange") => Some(NATIVE_PLUGIN_SETTING_CHANGED_EVENT.to_string()),
        ("ui", "onLayoutChange") => Some(NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT.to_string()),
        ("sessions", "onTreeChange") => Some(NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT.to_string()),
        ("sessions", "onNodeStateChange") => {
            Some(NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT.to_string())
        }
        ("eventLog", "onEntry") => Some(NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT.to_string()),
        ("forward", "onSavedForwardsChange") => {
            Some(NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT.to_string())
        }
        ("transfers", "onProgress") => Some(NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT.to_string()),
        ("transfers", "onComplete") => Some(NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT.to_string()),
        ("transfers", "onError") => Some(NATIVE_PLUGIN_TRANSFER_ERROR_EVENT.to_string()),
        ("profiler", "onMetrics") => Some(NATIVE_PLUGIN_PROFILER_METRICS_EVENT.to_string()),
        ("ide", "onFileOpen") => Some(NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT.to_string()),
        ("ide", "onFileClose") => Some(NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT.to_string()),
        ("ide", "onActiveFileChange") => {
            Some(NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT.to_string())
        }
        ("ai", "onMessage") => Some(NATIVE_PLUGIN_AI_MESSAGE_EVENT.to_string()),
        ("events", "onConnect") => Some(NATIVE_PLUGIN_LIFECYCLE_CONNECT_EVENT.to_string()),
        ("events", "onDisconnect") => Some(NATIVE_PLUGIN_LIFECYCLE_DISCONNECT_EVENT.to_string()),
        ("events", "onLinkDown") => Some(NATIVE_PLUGIN_LIFECYCLE_LINK_DOWN_EVENT.to_string()),
        ("events", "onReconnect") => Some(NATIVE_PLUGIN_LIFECYCLE_RECONNECT_EVENT.to_string()),
        _ => None,
    }
}

pub(super) fn native_plugin_custom_event_key(
    owner_plugin_id: &str,
    event_name: &str,
) -> Result<String, String> {
    validate_native_plugin_id(owner_plugin_id)?;
    validate_plugin_event_name(event_name)?;
    Ok(format!("plugin.{owner_plugin_id}:{event_name}"))
}

fn validate_plugin_event_key(event_key: &str) -> Result<(), String> {
    let Some(rest) = event_key.strip_prefix("plugin.") else {
        return Err("Plugin event key must start with plugin.".to_string());
    };
    let Some((owner_plugin_id, event_name)) = rest.split_once(':') else {
        return Err("Plugin event key requires owner plugin id and event name".to_string());
    };
    native_plugin_custom_event_key(owner_plugin_id, event_name).map(|_| ())
}

fn validate_plugin_event_name(event_name: &str) -> Result<(), String> {
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

pub(super) fn validate_native_plugin_id(plugin_id: &str) -> Result<(), String> {
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

pub(super) fn validate_plugin_relative_path(relative_path: &str) -> Result<(), String> {
    if relative_path.trim().is_empty() {
        return Err("Plugin relative path cannot be empty".to_string());
    }
    if relative_path.starts_with('/') || relative_path.starts_with('\\') {
        return Err("Absolute plugin paths are not allowed".to_string());
    }
    for component in relative_path.split(['/', '\\']) {
        if component == ".." {
            return Err("Plugin paths cannot escape the plugin directory".to_string());
        }
    }
    Ok(())
}

pub(super) fn native_runtime_plan_for_manifest(
    manifest: &NativePluginManifest,
) -> Result<NativePluginRuntimePlan, String> {
    if let Some(runtime) = &manifest.runtime {
        validate_plugin_relative_path(&runtime.entry)?;
        return Ok(match runtime.kind {
            NativePluginRuntimeKind::Wasm => NativePluginRuntimePlan::Wasm {
                entry: runtime.entry.clone(),
            },
            NativePluginRuntimeKind::Process => NativePluginRuntimePlan::Process {
                entry: runtime.entry.clone(),
            },
            NativePluginRuntimeKind::ManifestOnly => NativePluginRuntimePlan::ManifestOnly,
        });
    }

    // Tauri plugins use ESM activate(ctx). Native keeps these visible for
    // migration, but never evaluates JavaScript or creates a WebView.
    if let Some(main) = &manifest.main {
        validate_plugin_relative_path(main)?;
        return Ok(NativePluginRuntimePlan::UnsupportedLegacyJs {
            entry: main.clone(),
        });
    }

    Ok(NativePluginRuntimePlan::ManifestOnly)
}

pub(super) fn native_plugin_state_for(
    runtime_plan: &NativePluginRuntimePlan,
    config: &NativePluginConfigEntry,
) -> NativePluginState {
    if config.auto_disabled {
        return NativePluginState::AutoDisabled;
    }
    if !config.enabled {
        return NativePluginState::Disabled;
    }
    if config.last_error.is_some() {
        return NativePluginState::Error;
    }

    match runtime_plan {
        NativePluginRuntimePlan::ManifestOnly => NativePluginState::ReadyManifestOnly,
        NativePluginRuntimePlan::Wasm { .. } => NativePluginState::ReadyWasm,
        NativePluginRuntimePlan::Process { .. } => NativePluginState::ReadyProcess,
        NativePluginRuntimePlan::UnsupportedLegacyJs { .. } => {
            NativePluginState::UnsupportedLegacyJs
        }
    }
}

pub(super) fn native_runtime_kind_label(runtime_plan: &NativePluginRuntimePlan) -> &'static str {
    match runtime_plan {
        NativePluginRuntimePlan::ManifestOnly => "manifest-only",
        NativePluginRuntimePlan::Wasm { .. } => "wasm",
        NativePluginRuntimePlan::Process { .. } => "process",
        NativePluginRuntimePlan::UnsupportedLegacyJs { .. } => "legacy-js",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn minimal_manifest() -> NativePluginManifest {
        NativePluginManifest {
            id: "com.example.demo".to_string(),
            name: "Demo".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            author: None,
            main: None,
            engines: None,
            manifest_version: None,
            format: None,
            assets: None,
            styles: None,
            shared_dependencies: None,
            repository: None,
            checksum: None,
            contributes: None,
            locales: None,
            runtime: None,
        }
    }

    fn plugin_package(entries: &[(&str, String)]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default();
        for (path, content) in entries {
            zip.start_file(path, options).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn manifest_json(id: &str, version: &str) -> String {
        serde_json::json!({
            "id": id,
            "name": "Packaged Demo",
            "version": version,
            "contributes": {
                "settings": [{
                    "id": "enabled",
                    "type": "boolean",
                    "default": true,
                    "title": "Enabled"
                }]
            }
        })
        .to_string()
    }

    #[test]
    fn legacy_tauri_manifest_is_visible_but_not_executable() {
        let mut manifest = minimal_manifest();
        manifest.main = Some("main.js".to_string());

        let plan = native_runtime_plan_for_manifest(&manifest).unwrap();
        assert_eq!(
            plan,
            NativePluginRuntimePlan::UnsupportedLegacyJs {
                entry: "main.js".to_string()
            }
        );
    }

    #[test]
    fn native_wasm_runtime_uses_explicit_runtime_block() {
        let mut manifest = minimal_manifest();
        manifest.runtime = Some(NativePluginRuntime {
            kind: NativePluginRuntimeKind::Wasm,
            entry: "plugin.wasm".to_string(),
        });

        let plan = native_runtime_plan_for_manifest(&manifest).unwrap();
        assert_eq!(
            plan,
            NativePluginRuntimePlan::Wasm {
                entry: "plugin.wasm".to_string()
            }
        );
    }

    #[test]
    fn plugin_paths_cannot_escape_install_directory() {
        assert!(validate_plugin_relative_path("panel/native.json").is_ok());
        assert!(validate_plugin_relative_path("../secret").is_err());
        assert!(validate_plugin_relative_path("/tmp/plugin.wasm").is_err());
        assert!(validate_native_plugin_package_url("https://example.invalid/plugin.zip").is_ok());
        assert!(validate_native_plugin_package_url("file:///tmp/plugin.zip").is_err());
    }

    #[test]
    fn registry_index_parses_capabilities_summary() {
        let registry: NativePluginRegistryIndex = serde_json::from_value(serde_json::json!({
            "version": 1,
            "plugins": [{
                "id": "com.example.demo",
                "name": "Demo",
                "version": "1.2.0",
                "description": "demo plugin",
                "downloadUrl": "https://example.invalid/demo.zip",
                "checksum": "sha256:abc",
                "capabilitiesSummary": ["terminal read", "status item"]
            }]
        }))
        .unwrap();

        assert_eq!(
            registry.plugins[0].capabilities_summary.as_deref(),
            Some(&["terminal read".to_string(), "status item".to_string()][..])
        );
    }

    #[test]
    fn plugin_package_install_supports_flat_nested_conflict_and_updates() {
        let temp_dir = unique_temp_dir("plugin-package-install");
        let settings_path = temp_dir.join("settings.json");
        let flat_package = plugin_package(&[
            ("plugin.json", manifest_json("com.example.demo", "1.0.0")),
            ("README.md", "demo".to_string()),
        ]);
        let result = NativePluginRegistry::install_plugin_package_from_bytes(
            &settings_path,
            &flat_package,
            None,
            false,
        )
        .unwrap();
        assert_eq!(result.manifest.id, "com.example.demo");
        assert!(!result.replaced_existing);
        assert_eq!(result.checksum, native_plugin_sha256_hex(&flat_package));

        let conflict = NativePluginRegistry::install_plugin_package_from_bytes(
            &settings_path,
            &flat_package,
            None,
            false,
        )
        .unwrap_err();
        assert!(conflict.contains("PLUGIN_ID_CONFLICT:com.example.demo"));

        let nested_package = plugin_package(&[
            (
                "oxideterm-demo-main/plugin.json",
                manifest_json("com.example.demo", "1.1.0"),
            ),
            ("oxideterm-demo-main/bin/plugin", "#!/bin/sh".to_string()),
        ]);
        let replaced = NativePluginRegistry::install_plugin_package_from_bytes(
            &settings_path,
            &nested_package,
            Some(&format!(
                "sha256:{}",
                native_plugin_sha256_hex(&nested_package)
            )),
            true,
        )
        .unwrap();
        assert!(replaced.replaced_existing);

        let registry = NativePluginRegistry::discover(&settings_path);
        assert_eq!(registry.plugins()[0].manifest.version, "1.1.0");
        let updates = NativePluginRegistry::check_plugin_updates(
            NativePluginRegistryIndex {
                version: 1,
                plugins: vec![
                    NativePluginRegistryEntry {
                        id: "com.example.demo".to_string(),
                        name: "Demo".to_string(),
                        description: None,
                        author: None,
                        version: "1.2.0".to_string(),
                        min_oxideterm_version: None,
                        download_url: "https://example.invalid/demo.zip".to_string(),
                        checksum: None,
                        size: None,
                        tags: None,
                        capabilities_summary: Some(vec![
                            "terminal read".to_string(),
                            "status item".to_string(),
                        ]),
                        homepage: None,
                        updated_at: None,
                    },
                    NativePluginRegistryEntry {
                        id: "com.example.other".to_string(),
                        name: "Other".to_string(),
                        description: None,
                        author: None,
                        version: "9.0.0".to_string(),
                        min_oxideterm_version: None,
                        download_url: "https://example.invalid/other.zip".to_string(),
                        checksum: None,
                        size: None,
                        tags: None,
                        capabilities_summary: None,
                        homepage: None,
                        updated_at: None,
                    },
                ],
            },
            &[NativePluginInstalledInfo {
                id: "com.example.demo".to_string(),
                version: "1.1.0".to_string(),
            }],
        );
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].version, "1.2.0");
        assert_eq!(
            updates[0].capabilities_summary.as_deref(),
            Some(&["terminal read".to_string(), "status item".to_string()][..])
        );
        let expected_package = plugin_package(&[(
            "plugin.json",
            manifest_json("com.example.expected", "1.0.0"),
        )]);
        let expected_manifest = NativePluginRegistry::install_plugin_package(
            &settings_path,
            "com.example.expected",
            None,
            &expected_package,
        )
        .unwrap();
        assert_eq!(expected_manifest.id, "com.example.expected");
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn plugin_package_rejects_zip_slip_and_checksum_mismatch_without_replacing_existing() {
        let temp_dir = unique_temp_dir("plugin-package-safety");
        let settings_path = temp_dir.join("settings.json");
        let installed =
            plugin_package(&[("plugin.json", manifest_json("com.example.demo", "1.0.0"))]);
        NativePluginRegistry::install_plugin_package_from_bytes(
            &settings_path,
            &installed,
            None,
            false,
        )
        .unwrap();

        let bad_path_package =
            plugin_package(&[("../plugin.json", manifest_json("com.bad", "1.0.0"))]);
        let bad_path_error = NativePluginRegistry::install_plugin_package_from_bytes(
            &settings_path,
            &bad_path_package,
            None,
            true,
        )
        .unwrap_err();
        assert!(bad_path_error.contains("escapes target dir"));

        let replacement =
            plugin_package(&[("plugin.json", manifest_json("com.example.demo", "2.0.0"))]);
        let checksum_error = NativePluginRegistry::install_plugin_package_from_bytes(
            &settings_path,
            &replacement,
            Some("sha256:0000"),
            true,
        )
        .unwrap_err();
        assert!(checksum_error.contains("Checksum mismatch"));
        let registry = NativePluginRegistry::discover(&settings_path);
        assert_eq!(registry.plugins()[0].manifest.version, "1.0.0");
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn uninstall_plugin_removes_directory_contributions_and_optional_state() {
        let temp_dir = unique_temp_dir("plugin-uninstall");
        let settings_path = temp_dir.join("settings.json");
        let package =
            plugin_package(&[("plugin.json", manifest_json("com.example.demo", "1.0.0"))]);
        NativePluginRegistry::install_plugin_package_from_bytes(
            &settings_path,
            &package,
            None,
            false,
        )
        .unwrap();

        let mut registry = NativePluginRegistry::discover(&settings_path);
        assert_eq!(registry.contributions().settings.len(), 1);
        registry
            .set_plugin_storage_value("com.example.demo", "recent", serde_json::json!("yes"))
            .unwrap();
        assert!(
            registry
                .plugin_storage_value("com.example.demo", "recent")
                .is_some()
        );
        registry.uninstall_plugin("com.example.demo", true).unwrap();
        assert!(registry.plugins().is_empty());
        assert_eq!(registry.contributions().total_count(), 0);
        assert!(
            !native_plugins_dir(&settings_path)
                .join("com.example.demo")
                .exists()
        );
        assert_eq!(
            registry.plugin_storage_value("com.example.demo", "recent"),
            None
        );
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn plugin_config_round_trips_disabled_and_error_state() {
        let temp_dir = unique_temp_dir("plugin-config-round-trip");
        fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join(PLUGIN_CONFIG_FILENAME);
        let mut config = NativePluginGlobalConfig::default();
        config.plugins.insert(
            "com.example.demo".to_string(),
            NativePluginConfigEntry {
                enabled: false,
                last_error: Some("disabled by test".to_string()),
                runtime_kind: Some("wasm".to_string()),
                ..NativePluginConfigEntry::default()
            },
        );

        save_native_plugin_config(&config_path, &config).unwrap();
        let loaded = load_native_plugin_config(&config_path);
        let entry = loaded.plugins.get("com.example.demo").unwrap();
        assert!(!entry.enabled);
        assert_eq!(entry.last_error.as_deref(), Some("disabled by test"));
        assert_eq!(entry.runtime_kind.as_deref(), Some("wasm"));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn corrupt_plugin_config_is_quarantined_and_recreated() {
        let temp_dir = unique_temp_dir("plugin-config-corrupt-recovery");
        fs::create_dir_all(&temp_dir).unwrap();
        let settings_path = temp_dir.join("settings.json");
        let config_path = native_plugin_config_path(&settings_path);
        fs::write(&config_path, b"{ not valid json").unwrap();

        let registry = NativePluginRegistry::discover(&settings_path);

        assert_eq!(registry.configured_plugin_count(), 0);
        assert!(config_path.exists());
        let backup_count = fs::read_dir(&temp_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_name().to_string_lossy().starts_with(&format!(
                    "{PLUGIN_CONFIG_FILENAME}.{PLUGIN_CONFIG_CORRUPT_MARKER}-"
                ))
            })
            .count();
        assert_eq!(backup_count, 1);
        let loaded = load_native_plugin_config(&config_path);
        assert_eq!(loaded.version, PLUGIN_CONFIG_SCHEMA_VERSION);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn runtime_state_respects_config_before_runtime_kind() {
        let disabled = NativePluginConfigEntry {
            enabled: false,
            ..NativePluginConfigEntry::default()
        };
        assert_eq!(
            native_plugin_state_for(
                &NativePluginRuntimePlan::Wasm {
                    entry: "plugin.wasm".to_string()
                },
                &disabled,
            ),
            NativePluginState::Disabled
        );

        let auto_disabled = NativePluginConfigEntry {
            auto_disabled: true,
            ..NativePluginConfigEntry::default()
        };
        assert_eq!(
            native_plugin_state_for(&NativePluginRuntimePlan::ManifestOnly, &auto_disabled),
            NativePluginState::AutoDisabled
        );
    }

    #[test]
    fn executable_native_runtime_requires_existing_entry() {
        let temp_dir = unique_temp_dir("plugin-runtime-entry");
        fs::create_dir_all(&temp_dir).unwrap();
        let plan = NativePluginRuntimePlan::Process {
            entry: "bin/plugin".to_string(),
        };
        assert!(validate_runtime_entry_exists(&temp_dir, &plan).is_err());

        let bin_dir = temp_dir.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join("plugin"), b"#!/bin/sh\n").unwrap();
        assert!(validate_runtime_entry_exists(&temp_dir, &plan).is_ok());
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn invalid_manifest_reports_diagnostic_without_crashing_discovery() {
        let temp_dir = unique_temp_dir("plugin-invalid-manifest");
        let plugins_dir = temp_dir.join(PLUGINS_DIR_NAME);
        let broken_dir = plugins_dir.join("broken");
        fs::create_dir_all(&broken_dir).unwrap();
        fs::write(broken_dir.join(PLUGIN_MANIFEST_FILENAME), b"{").unwrap();

        let (plugins, diagnostics) =
            discover_native_plugins_in_dir(&plugins_dir, &NativePluginGlobalConfig::default());
        assert!(plugins.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Invalid plugin.json"));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn missing_executable_runtime_entry_reports_diagnostic() {
        let temp_dir = unique_temp_dir("plugin-missing-runtime-entry");
        let plugins_dir = temp_dir.join(PLUGINS_DIR_NAME);
        let plugin_dir = plugins_dir.join("native-process");
        fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = minimal_manifest();
        manifest.id = "com.example.process".to_string();
        manifest.runtime = Some(NativePluginRuntime {
            kind: NativePluginRuntimeKind::Process,
            entry: "bin/plugin".to_string(),
        });
        write_manifest(&plugin_dir, &manifest);

        let (plugins, diagnostics) =
            discover_native_plugins_in_dir(&plugins_dir, &NativePluginGlobalConfig::default());
        assert!(plugins.is_empty());
        assert_eq!(
            diagnostics[0].plugin_id.as_deref(),
            Some("com.example.process")
        );
        assert!(diagnostics[0].message.contains("does not exist"));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn discovery_classifies_native_wasm_and_process_runtime_states() {
        let temp_dir = unique_temp_dir("plugin-runtime-state");
        let plugins_dir = temp_dir.join(PLUGINS_DIR_NAME);
        let wasm_dir = plugins_dir.join("wasm");
        let process_dir = plugins_dir.join("process");
        fs::create_dir_all(&wasm_dir).unwrap();
        fs::create_dir_all(&process_dir).unwrap();

        let mut wasm_manifest = minimal_manifest();
        wasm_manifest.id = "com.example.wasm".to_string();
        wasm_manifest.name = "Wasm".to_string();
        wasm_manifest.runtime = Some(NativePluginRuntime {
            kind: NativePluginRuntimeKind::Wasm,
            entry: "plugin.wasm".to_string(),
        });
        fs::write(wasm_dir.join("plugin.wasm"), b"\0asm").unwrap();
        write_manifest(&wasm_dir, &wasm_manifest);

        let mut process_manifest = minimal_manifest();
        process_manifest.id = "com.example.process".to_string();
        process_manifest.name = "Process".to_string();
        process_manifest.runtime = Some(NativePluginRuntime {
            kind: NativePluginRuntimeKind::Process,
            entry: "bin/plugin".to_string(),
        });
        fs::create_dir_all(process_dir.join("bin")).unwrap();
        fs::write(process_dir.join("bin/plugin"), b"#!/bin/sh\n").unwrap();
        write_manifest(&process_dir, &process_manifest);

        let (plugins, diagnostics) =
            discover_native_plugins_in_dir(&plugins_dir, &NativePluginGlobalConfig::default());
        assert!(diagnostics.is_empty());
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0].state, NativePluginState::ReadyProcess);
        assert_eq!(plugins[1].state, NativePluginState::ReadyWasm);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn process_activation_plans_and_runtime_state_transitions_are_host_owned() {
        let temp_dir = unique_temp_dir("plugin-process-activation-plan");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("process");
        fs::create_dir_all(plugin_dir.join("bin")).unwrap();
        fs::write(plugin_dir.join("bin/plugin"), b"#!/bin/sh\n").unwrap();

        let mut manifest = minimal_manifest();
        manifest.id = "com.example.process".to_string();
        manifest.name = "Process".to_string();
        manifest.runtime = Some(NativePluginRuntime {
            kind: NativePluginRuntimeKind::Process,
            entry: "bin/plugin".to_string(),
        });
        write_manifest(&plugin_dir, &manifest);

        let mut registry = NativePluginRegistry::discover(&settings_path);
        let plans = registry.process_activation_plans();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].plugin_id, "com.example.process");
        assert_eq!(plans[0].entry, "bin/plugin");

        registry
            .mark_runtime_loading("com.example.process")
            .unwrap();
        assert_eq!(registry.plugins()[0].state, NativePluginState::Loading);
        registry.mark_runtime_active("com.example.process").unwrap();
        assert_eq!(registry.plugins()[0].state, NativePluginState::Active);
        registry
            .mark_runtime_error("com.example.process", "activate failed".to_string())
            .unwrap();
        assert_eq!(registry.plugins()[0].state, NativePluginState::Error);

        let config = load_native_plugin_config(registry.config_path());
        assert_eq!(
            config.plugins["com.example.process"].last_error.as_deref(),
            Some("activate failed")
        );
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn wasm_activation_plans_are_host_owned() {
        let temp_dir = unique_temp_dir("plugin-wasm-activation-plan");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("wasm");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(plugin_dir.join("plugin.wasm"), b"\0asm\x01\0\0\0").unwrap();

        let mut manifest = minimal_manifest();
        manifest.id = "com.example.wasm".to_string();
        manifest.name = "Wasm".to_string();
        manifest.runtime = Some(NativePluginRuntime {
            kind: NativePluginRuntimeKind::Wasm,
            entry: "plugin.wasm".to_string(),
        });
        write_manifest(&plugin_dir, &manifest);

        let registry = NativePluginRegistry::discover(&settings_path);
        let plans = registry.wasm_activation_plans();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].plugin_id, "com.example.wasm");
        assert_eq!(plans[0].entry, "plugin.wasm");
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn set_plugin_enabled_persists_config_and_refreshes_state() {
        let temp_dir = unique_temp_dir("plugin-toggle-enabled");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(&plugin_dir, &minimal_manifest());

        let mut registry = NativePluginRegistry::discover(&settings_path);
        assert_eq!(
            registry.plugins()[0].state,
            NativePluginState::ReadyManifestOnly
        );

        registry
            .set_plugin_enabled("com.example.demo", false)
            .unwrap();
        assert_eq!(registry.plugins()[0].state, NativePluginState::Disabled);

        let config = load_native_plugin_config(registry.config_path());
        assert!(!config.plugins["com.example.demo"].enabled);
        assert_eq!(
            config.plugins["com.example.demo"].runtime_kind.as_deref(),
            Some("manifest-only")
        );

        registry
            .set_plugin_enabled("com.example.demo", true)
            .unwrap();
        assert_eq!(
            registry.plugins()[0].state,
            NativePluginState::ReadyManifestOnly
        );
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn manifest_only_contributions_are_indexed_without_runtime_execution() {
        let temp_dir = unique_temp_dir("plugin-contributions");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = minimal_manifest();
        manifest.contributes = Some(sample_contributes());
        write_manifest(&plugin_dir, &manifest);

        let registry = NativePluginRegistry::discover(&settings_path);
        let contributions = registry.contributions();
        assert_eq!(contributions.tabs.len(), 1);
        assert_eq!(contributions.sidebar_panels.len(), 1);
        assert_eq!(contributions.settings.len(), 1);
        assert_eq!(contributions.ai_tools.len(), 1);
        assert_eq!(contributions.terminal_shortcuts.len(), 1);
        assert_eq!(contributions.terminal_transports.len(), 1);
        assert_eq!(contributions.connection_hooks.len(), 1);
        assert_eq!(contributions.api_commands.len(), 1);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn disabling_plugin_removes_manifest_only_contributions() {
        let temp_dir = unique_temp_dir("plugin-contributions-disabled");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = minimal_manifest();
        manifest.contributes = Some(sample_contributes());
        write_manifest(&plugin_dir, &manifest);

        let mut registry = NativePluginRegistry::discover(&settings_path);
        assert_eq!(registry.contributions().total_count(), 8);
        registry
            .set_plugin_enabled("com.example.demo", false)
            .unwrap();
        assert_eq!(registry.contributions().total_count(), 0);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn plugin_ai_tool_metadata_uses_namespaced_pending_runtime_definitions() {
        let temp_dir = unique_temp_dir("plugin-ai-tool-definitions");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = minimal_manifest();
        manifest.id = "com.example.demo-plugin".to_string();
        manifest.contributes = Some(sample_contributes());
        write_manifest(&plugin_dir, &manifest);

        let registry = NativePluginRegistry::discover(&settings_path);
        let definitions = registry.contributions().ai_tool_definitions();
        assert_eq!(definitions.len(), 1);
        assert_eq!(
            definitions[0].name,
            "plugin::com_example_demo-plugin::demo_tool"
        );
        assert!(definitions[0].description.contains("[Plugin: Demo]"));
        assert_eq!(
            definitions[0].parameters,
            serde_json::json!({"type": "object"})
        );
        assert!(is_native_plugin_ai_tool_name(&definitions[0].name));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn plugin_setting_values_resolve_defaults_validate_and_persist() {
        let temp_dir = unique_temp_dir("plugin-setting-values");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = minimal_manifest();
        manifest.contributes = Some(sample_contributes());
        write_manifest(&plugin_dir, &manifest);

        let mut registry = NativePluginRegistry::discover(&settings_path);
        assert_eq!(
            registry.plugin_setting_value("com.example.demo", "mode"),
            Some(Value::String("auto".to_string()))
        );
        assert!(
            registry
                .set_plugin_setting_value(
                    "com.example.demo",
                    "mode",
                    Value::String("manual".to_string()),
                )
                .is_err()
        );
        registry
            .set_plugin_setting_value(
                "com.example.demo",
                "mode",
                Value::String("auto".to_string()),
            )
            .unwrap();

        let loaded = load_native_plugin_config(registry.config_path());
        assert_eq!(
            loaded.settings["com.example.demo"]["mode"],
            Value::String("auto".to_string())
        );
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn plugin_storage_values_are_plugin_scoped_validated_and_persisted() {
        let temp_dir = unique_temp_dir("plugin-storage-values");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let first_plugin_dir = plugins_dir.join("demo-a");
        let second_plugin_dir = plugins_dir.join("demo-b");
        fs::create_dir_all(&first_plugin_dir).unwrap();
        fs::create_dir_all(&second_plugin_dir).unwrap();
        write_manifest(&first_plugin_dir, &minimal_manifest());
        let mut second_manifest = minimal_manifest();
        second_manifest.id = "com.example.other".to_string();
        write_manifest(&second_plugin_dir, &second_manifest);

        let mut registry = NativePluginRegistry::discover(&settings_path);
        registry
            .set_plugin_storage_value(
                "com.example.demo",
                "recent",
                serde_json::json!({"path": "/tmp/a"}),
            )
            .unwrap();
        registry
            .set_plugin_storage_value(
                "com.example.other",
                "recent",
                serde_json::json!({"path": "/tmp/b"}),
            )
            .unwrap();

        assert_eq!(
            registry.plugin_storage_value("com.example.demo", "recent"),
            Some(serde_json::json!({"path": "/tmp/a"}))
        );
        assert_eq!(
            registry.plugin_storage_value("com.example.other", "recent"),
            Some(serde_json::json!({"path": "/tmp/b"}))
        );

        let loaded = load_native_plugin_config(registry.config_path());
        assert_eq!(
            loaded.storage["com.example.demo"]["recent"],
            serde_json::json!({"path": "/tmp/a"})
        );
        assert!(
            registry
                .set_plugin_storage_value(
                    "com.example.demo",
                    "",
                    serde_json::json!({"invalid": true}),
                )
                .is_err()
        );
        let oversized_key = "x".repeat(PLUGIN_STORAGE_MAX_KEY_BYTES + 1);
        assert!(
            registry
                .set_plugin_storage_value("com.example.demo", &oversized_key, Value::Null)
                .is_err()
        );
        assert!(
            registry
                .set_plugin_storage_value(
                    "com.example.demo",
                    "too-large",
                    Value::String("x".repeat(PLUGIN_STORAGE_MAX_PLUGIN_BYTES + 1)),
                )
                .is_err()
        );

        registry
            .remove_plugin_storage_value("com.example.demo", "recent")
            .unwrap();
        assert_eq!(
            registry.plugin_storage_value("com.example.demo", "recent"),
            None
        );
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn runtime_registrations_feed_host_owned_contribution_store_and_cleanup() {
        let temp_dir = unique_temp_dir("plugin-runtime-registrations");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = minimal_manifest();
        manifest.contributes = Some(NativePluginContributes {
            terminal_hooks: Some(NativePluginTerminalHooksDef {
                input_interceptor: Some(true),
                output_processor: Some(true),
                shortcuts: Some(vec![NativePluginShortcutDef {
                    key: "Ctrl+Shift+K".to_string(),
                    command: "demo.focus".to_string(),
                }]),
            }),
            ..NativePluginContributes::default()
        });
        write_manifest(&plugin_dir, &manifest);

        let mut registry = NativePluginRegistry::discover(&settings_path);
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "cmd-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::Command,
                metadata: serde_json::json!({
                    "id": "demo.run",
                    "label": "Run Demo",
                    "icon": "play",
                    "shortcut": "cmd+shift+d",
                    "section": "Demo",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "key-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::Keybinding,
                metadata: serde_json::json!({
                    "keybinding": "Cmd+Shift+R",
                    "command": "demo.run",
                    "label": "Run Demo",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "status-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::StatusBar,
                metadata: serde_json::json!({
                    "text": "Demo Ready",
                    "alignment": "right",
                    "priority": 10,
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "menu-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::ContextMenu,
                metadata: serde_json::json!({
                    "target": "terminal",
                    "items": [
                        { "label": "Run Demo", "icon": "play", "enabled": true }
                    ],
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "theme-sub-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::EventSubscription,
                metadata: serde_json::json!({
                    "namespace": "app",
                    "method": "onThemeChange",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "custom-sub-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::EventSubscription,
                metadata: serde_json::json!({
                    "namespace": "events",
                    "method": "on",
                    "name": "build.done",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "layout-sub-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::EventSubscription,
                metadata: serde_json::json!({
                    "namespace": "ui",
                    "method": "onLayoutChange",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "saved-forwards-sub-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::EventSubscription,
                metadata: serde_json::json!({
                    "namespace": "forward",
                    "method": "onSavedForwardsChange",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "transfer-progress-sub-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::EventSubscription,
                metadata: serde_json::json!({
                    "namespace": "transfers",
                    "method": "onProgress",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "profiler-metrics-sub-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::EventSubscription,
                metadata: serde_json::json!({
                    "namespace": "profiler",
                    "method": "onMetrics",
                    "nodeId": "node-1",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "ide-active-sub-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::EventSubscription,
                metadata: serde_json::json!({
                    "namespace": "ide",
                    "method": "onActiveFileChange",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "ai-message-sub-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::EventSubscription,
                metadata: serde_json::json!({
                    "namespace": "ai",
                    "method": "onMessage",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "terminal-shortcut-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::TerminalShortcut,
                metadata: serde_json::json!({
                    "command": "demo.focus",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "terminal-input-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::TerminalInputInterceptor,
                metadata: serde_json::json!({
                    "command": "demo.input",
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "terminal-output-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::TerminalOutputProcessor,
                metadata: serde_json::json!({
                    "command": "demo.output",
                }),
            })
            .unwrap();

        let contributions = registry.contributions();
        assert_eq!(contributions.runtime_commands[0].command, "demo.run");
        assert_eq!(contributions.runtime_commands[0].label, "Run Demo");
        assert_eq!(
            contributions.runtime_keybindings[0].keybinding,
            "Cmd+Shift+R"
        );
        assert_eq!(
            contributions.runtime_keybindings[0].normalized_keybinding,
            "ctrl+r+shift"
        );
        assert_eq!(contributions.runtime_keybindings[0].command, "demo.run");
        assert_eq!(
            contributions.runtime_keybindings[1].keybinding,
            "Ctrl+Shift+K"
        );
        assert_eq!(
            contributions.runtime_keybindings[1].normalized_keybinding,
            "ctrl+k+shift"
        );
        assert_eq!(contributions.runtime_keybindings[1].command, "demo.focus");
        assert_eq!(
            contributions.runtime_terminal_input_interceptors[0].command,
            "demo.input"
        );
        assert_eq!(
            contributions.runtime_terminal_output_processors[0].command,
            "demo.output"
        );
        assert_eq!(
            contributions
                .runtime_keybinding_for_normalized_key("ctrl+r+shift")
                .map(|entry| entry.command.as_str()),
            Some("demo.run")
        );
        assert_eq!(contributions.runtime_status_items[0].alignment, "right");
        assert_eq!(contributions.runtime_context_menus[0].target, "terminal");
        assert_eq!(
            contributions.runtime_event_subscriptions_for(NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT)[0]
                .registration_id,
            "theme-sub-1"
        );
        assert_eq!(
            contributions.runtime_event_subscriptions_for("plugin.com.example.demo:build.done")[0]
                .registration_id,
            "custom-sub-1"
        );
        assert_eq!(
            contributions.runtime_event_subscriptions_for(NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT)[0]
                .registration_id,
            "layout-sub-1"
        );
        assert_eq!(
            contributions.runtime_event_subscriptions_for(
                NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT
            )[0]
            .registration_id,
            "saved-forwards-sub-1"
        );
        assert_eq!(
            contributions.runtime_event_subscriptions_for(NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT)[0]
                .registration_id,
            "transfer-progress-sub-1"
        );
        assert_eq!(
            contributions.runtime_event_subscriptions_for(NATIVE_PLUGIN_PROFILER_METRICS_EVENT)[0]
                .filter,
            Some(serde_json::json!({ "nodeId": "node-1" }))
        );
        assert_eq!(
            contributions
                .runtime_event_subscriptions_for(NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT)[0]
                .registration_id,
            "ide-active-sub-1"
        );
        assert_eq!(
            contributions.runtime_event_subscriptions_for(NATIVE_PLUGIN_AI_MESSAGE_EVENT)[0]
                .registration_id,
            "ai-message-sub-1"
        );
        assert_eq!(contributions.total_count(), 16);

        assert!(registry.dispose_runtime_registration("com.example.demo", "cmd-1"));
        assert!(registry.contributions().runtime_commands.is_empty());
        assert_eq!(
            registry.cleanup_runtime_plugin_contributions("com.example.demo"),
            14
        );
        assert_eq!(registry.contributions().total_count(), 1);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn terminal_shortcut_registration_requires_manifest_declaration() {
        let temp_dir = unique_temp_dir("plugin-terminal-shortcut-gate");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(&plugin_dir, &minimal_manifest());

        let mut registry = NativePluginRegistry::discover(&settings_path);
        let error = registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "terminal-shortcut-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::TerminalShortcut,
                metadata: serde_json::json!({
                    "command": "demo.focus",
                }),
            })
            .unwrap_err();

        assert!(error.contains("not declared in manifest contributes.terminalHooks.shortcuts"));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn terminal_hook_registration_requires_manifest_declaration() {
        let temp_dir = unique_temp_dir("plugin-terminal-hook-gate");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(&plugin_dir, &minimal_manifest());

        let mut registry = NativePluginRegistry::discover(&settings_path);
        let input_error = registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "terminal-input-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::TerminalInputInterceptor,
                metadata: serde_json::json!({
                    "command": "demo.input",
                }),
            })
            .unwrap_err();
        let output_error = registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "terminal-output-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::TerminalOutputProcessor,
                metadata: serde_json::json!({
                    "command": "demo.output",
                }),
            })
            .unwrap_err();

        assert!(input_error.contains("inputInterceptor not declared"));
        assert!(output_error.contains("outputProcessor not declared"));
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn runtime_tab_and_sidebar_views_require_manifest_declarations_and_valid_schema() {
        let temp_dir = unique_temp_dir("plugin-declarative-ui");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = minimal_manifest();
        manifest.contributes = Some(NativePluginContributes {
            tabs: Some(vec![NativePluginTabDef {
                id: "deploy".to_string(),
                title: "Deploy".to_string(),
                icon: "rocket".to_string(),
            }]),
            sidebar_panels: Some(vec![NativePluginSidebarDef {
                id: "jobs".to_string(),
                title: "Jobs".to_string(),
                icon: "list".to_string(),
                position: "top".to_string(),
            }]),
            ..NativePluginContributes::default()
        });
        write_manifest(&plugin_dir, &manifest);

        let schema = serde_json::json!({
            "kind": "form",
            "sections": [{
                "id": "deploy",
                "title": "Deploy",
                "controls": [
                    { "kind": "text", "id": "target", "label": "Target" },
                    { "kind": "select", "id": "env", "label": "Environment", "options": [
                        { "label": "Prod", "value": "prod" }
                    ] },
                    { "kind": "button", "id": "run", "label": "Run" }
                ]
            }]
        });
        let mut registry = NativePluginRegistry::discover(&settings_path);
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "tab-view-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::Tab,
                metadata: serde_json::json!({
                    "tabId": "deploy",
                    "schema": schema,
                }),
            })
            .unwrap();
        registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "sidebar-view-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::SidebarPanel,
                metadata: serde_json::json!({
                    "panelId": "jobs",
                    "schema": {
                        "kind": "form",
                        "controls": [
                            { "kind": "emptyState", "label": "No jobs" }
                        ]
                    },
                }),
            })
            .unwrap();

        let contributions = registry.contributions();
        assert_eq!(
            contributions
                .runtime_tab_view("com.example.demo", "deploy")
                .unwrap()
                .title,
            "Deploy"
        );
        assert_eq!(contributions.runtime_sidebar_panels()[0].panel_id, "jobs");

        let undeclared_error = registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "tab-view-2".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::Tab,
                metadata: serde_json::json!({
                    "tabId": "unknown",
                    "schema": { "kind": "form", "controls": [{ "kind": "divider" }] },
                }),
            })
            .unwrap_err();
        assert!(undeclared_error.contains("not declared"));

        let schema_error = registry
            .apply_runtime_registration(PluginRegistration {
                registration_id: "tab-view-3".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::Tab,
                metadata: serde_json::json!({
                    "tabId": "deploy",
                    "schema": { "kind": "form", "controls": [{ "kind": "reactComponent" }] },
                }),
            })
            .unwrap_err();
        assert!(schema_error.contains("unsupported value"));

        assert!(registry.dispose_runtime_registration("com.example.demo", "tab-view-1"));
        assert!(
            registry
                .contributions()
                .runtime_tab_view("com.example.demo", "deploy")
                .is_none()
        );
        assert_eq!(
            registry.cleanup_runtime_plugin_contributions("com.example.demo"),
            1
        );
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn disabled_or_loading_declarative_buttons_are_not_actionable() {
        let active = NativePluginDeclarativeUiControl {
            kind: "button".to_string(),
            id: Some("run".to_string()),
            label: Some("Run".to_string()),
            description: None,
            value: None,
            text: None,
            language: None,
            options: None,
            rows: None,
            columns: None,
            disabled: false,
            loading: false,
        };
        let mut disabled = active.clone();
        disabled.disabled = true;
        let mut loading = active.clone();
        loading.loading = true;

        assert!(native_plugin_declarative_control_is_actionable(&active));
        assert!(!native_plugin_declarative_control_is_actionable(&disabled));
        assert!(!native_plugin_declarative_control_is_actionable(&loading));
    }

    #[test]
    fn runtime_registration_rejects_render_time_context_menu_predicate_shape() {
        let mut store = NativePluginContributionStore::default();
        let error = store
            .apply_runtime_registration(
                PluginRegistration {
                    registration_id: "menu-1".to_string(),
                    plugin_id: "com.example.demo".to_string(),
                    kind: PluginRegistrationKind::ContextMenu,
                    metadata: serde_json::json!({
                        "target": "terminal",
                        "items": [
                            { "label": "" }
                        ],
                    }),
                },
                "Demo".to_string(),
            )
            .unwrap_err();

        assert!(error.contains("label"));
    }

    #[test]
    fn runtime_event_subscription_rejects_invalid_custom_event_name() {
        let mut store = NativePluginContributionStore::default();
        let error = store
            .apply_runtime_registration(
                PluginRegistration {
                    registration_id: "custom-sub-1".to_string(),
                    plugin_id: "com.example.demo".to_string(),
                    kind: PluginRegistrationKind::EventSubscription,
                    metadata: serde_json::json!({
                        "namespace": "events",
                        "method": "on",
                        "name": "../escape",
                    }),
                },
                "Demo".to_string(),
            )
            .unwrap_err();

        assert!(error.contains("Plugin event name"));
    }

    #[test]
    fn malformed_contribution_definition_is_rejected_with_diagnostic() {
        let temp_dir = unique_temp_dir("plugin-bad-contribution");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("demo");
        fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = minimal_manifest();
        manifest.contributes = Some(NativePluginContributes {
            settings: Some(vec![NativePluginSettingDef {
                id: "mode".to_string(),
                setting_type: "select".to_string(),
                default: Value::String("auto".to_string()),
                title: "Mode".to_string(),
                description: None,
                options: None,
            }]),
            ..NativePluginContributes::default()
        });
        write_manifest(&plugin_dir, &manifest);

        let registry = NativePluginRegistry::discover(&settings_path);
        assert!(registry.plugins().is_empty());
        assert_eq!(registry.diagnostics().len(), 1);
        assert!(
            registry.diagnostics()[0]
                .message
                .contains("Select plugin settings require")
        );
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn legacy_js_plugin_cannot_be_enabled_by_native_toggle() {
        let temp_dir = unique_temp_dir("plugin-legacy-enable");
        let settings_path = temp_dir.join("settings.json");
        let plugins_dir = native_plugins_dir(&settings_path);
        let plugin_dir = plugins_dir.join("legacy");
        fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = minimal_manifest();
        manifest.main = Some("main.js".to_string());
        write_manifest(&plugin_dir, &manifest);

        let mut registry = NativePluginRegistry::discover(&settings_path);
        assert_eq!(
            registry.plugins()[0].state,
            NativePluginState::UnsupportedLegacyJs
        );
        registry
            .set_plugin_enabled("com.example.demo", false)
            .unwrap();
        assert_eq!(registry.plugins()[0].state, NativePluginState::Disabled);
        assert!(
            registry
                .set_plugin_enabled("com.example.demo", true)
                .is_err()
        );
        let _ = fs::remove_dir_all(temp_dir);
    }

    fn write_manifest(plugin_dir: &Path, manifest: &NativePluginManifest) {
        let manifest_json = serde_json::to_vec_pretty(manifest).unwrap();
        fs::write(plugin_dir.join(PLUGIN_MANIFEST_FILENAME), manifest_json).unwrap();
    }

    fn sample_contributes() -> NativePluginContributes {
        NativePluginContributes {
            tabs: Some(vec![NativePluginTabDef {
                id: "demo-tab".to_string(),
                title: "Demo".to_string(),
                icon: "Puzzle".to_string(),
            }]),
            sidebar_panels: Some(vec![NativePluginSidebarDef {
                id: "demo-sidebar".to_string(),
                title: "Demo".to_string(),
                icon: "Puzzle".to_string(),
                position: "bottom".to_string(),
            }]),
            settings: Some(vec![NativePluginSettingDef {
                id: "mode".to_string(),
                setting_type: "select".to_string(),
                default: Value::String("auto".to_string()),
                title: "Mode".to_string(),
                description: Some("Mode description".to_string()),
                options: Some(vec![NativePluginSettingOption {
                    label: "Auto".to_string(),
                    value: Value::String("auto".to_string()),
                }]),
            }]),
            terminal_hooks: Some(NativePluginTerminalHooksDef {
                input_interceptor: Some(true),
                output_processor: None,
                shortcuts: Some(vec![NativePluginShortcutDef {
                    key: "Ctrl+Shift+D".to_string(),
                    command: "demo.run".to_string(),
                }]),
            }),
            terminal_transports: Some(vec!["telnet".to_string()]),
            connection_hooks: Some(vec!["onConnect".to_string()]),
            ai_tools: Some(vec![NativePluginAiToolDef {
                name: "demo_tool".to_string(),
                description: "Demo tool".to_string(),
                parameters: Some(serde_json::json!({"type": "object"})),
                capabilities: Some(vec!["state.list".to_string()]),
                risk: Some("read".to_string()),
                target_kinds: Some(vec!["app-tab".to_string()]),
                result_schema: None,
            }]),
            api_commands: Some(vec!["demo_command".to_string()]),
        }
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("oxideterm-{label}-{nanos}"))
    }
}
