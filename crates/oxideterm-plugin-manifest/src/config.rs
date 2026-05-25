// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const PLUGIN_CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginGlobalConfig {
    pub version: u32,
    #[serde(default)]
    pub plugins: HashMap<String, NativePluginConfigEntry>,
    #[serde(default)]
    pub settings: HashMap<String, HashMap<String, Value>>,
    #[serde(default)]
    pub storage: HashMap<String, HashMap<String, Value>>,
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
pub struct NativePluginConfigEntry {
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

fn default_plugin_enabled() -> bool {
    true
}
