// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const PLUGINS_DIR_NAME: &str = "plugins";
const PLUGIN_MANIFEST_FILENAME: &str = "plugin.json";

#[derive(Clone, Debug, Default)]
pub(super) struct NativePluginRegistry {
    plugins: Vec<NativePluginInfo>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NativePluginInfo {
    pub manifest: NativePluginManifest,
    pub install_dir: PathBuf,
    pub runtime_plan: NativePluginRuntimePlan,
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

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
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

fn default_sidebar_position() -> String {
    "bottom".to_string()
}

impl NativePluginRegistry {
    pub fn discover(settings_path: &Path) -> Self {
        let plugins_dir = native_plugins_dir(settings_path);
        let plugins = discover_native_plugins_in_dir(&plugins_dir);
        Self { plugins }
    }

    pub fn plugins(&self) -> &[NativePluginInfo] {
        &self.plugins
    }
}

pub(super) fn native_plugins_dir(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(PLUGINS_DIR_NAME)
}

fn discover_native_plugins_in_dir(plugins_dir: &Path) -> Vec<NativePluginInfo> {
    let entries = match fs::read_dir(plugins_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(_) => return Vec::new(),
    };

    let mut plugins = Vec::new();
    for entry in entries.flatten() {
        let plugin_dir = entry.path();
        if !plugin_dir.is_dir() {
            continue;
        }
        if let Some(info) = load_native_plugin_manifest(&plugin_dir) {
            plugins.push(info);
        }
    }
    plugins.sort_by(|left, right| left.manifest.name.cmp(&right.manifest.name));
    plugins
}

fn load_native_plugin_manifest(plugin_dir: &Path) -> Option<NativePluginInfo> {
    let manifest_path = plugin_dir.join(PLUGIN_MANIFEST_FILENAME);
    let manifest_text = fs::read_to_string(manifest_path).ok()?;
    let manifest = serde_json::from_str::<NativePluginManifest>(&manifest_text).ok()?;
    validate_native_plugin_manifest(&manifest).ok()?;
    let runtime_plan = native_runtime_plan_for_manifest(&manifest).ok()?;
    Some(NativePluginInfo {
        manifest,
        install_dir: plugin_dir.to_path_buf(),
        runtime_plan,
    })
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
    Ok(())
}

fn validate_manifest_text_field(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("Plugin manifest field \"{field}\" cannot be empty"));
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
