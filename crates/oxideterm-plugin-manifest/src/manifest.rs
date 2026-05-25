// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const NATIVE_PLUGIN_DECLARATIVE_UI_FORM_KIND: &str = "form";

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginManifest {
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
pub struct NativePluginEngines {
    #[serde(default)]
    pub oxideterm: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginRuntime {
    pub kind: NativePluginRuntimeKind,
    pub entry: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativePluginRuntimeKind {
    Wasm,
    Process,
    ManifestOnly,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginContributes {
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
pub struct NativePluginTabDef {
    pub id: String,
    pub title: String,
    pub icon: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct NativePluginSidebarDef {
    pub id: String,
    pub title: String,
    pub icon: String,
    #[serde(default = "default_sidebar_position")]
    pub position: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct NativePluginSettingDef {
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
pub struct NativePluginSettingOption {
    pub label: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginTerminalHooksDef {
    #[serde(default)]
    pub input_interceptor: Option<bool>,
    #[serde(default)]
    pub output_processor: Option<bool>,
    #[serde(default)]
    pub shortcuts: Option<Vec<NativePluginShortcutDef>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct NativePluginShortcutDef {
    pub key: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginDeclarativeUiSchema {
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
pub struct NativePluginDeclarativeUiSection {
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
pub struct NativePluginDeclarativeUiControl {
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
pub struct NativePluginDeclarativeUiOption {
    pub label: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct NativePluginAiToolDef {
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

fn default_sidebar_position() -> String {
    "bottom".to_string()
}

fn default_declarative_ui_kind() -> String {
    NATIVE_PLUGIN_DECLARATIVE_UI_FORM_KIND.to_string()
}
