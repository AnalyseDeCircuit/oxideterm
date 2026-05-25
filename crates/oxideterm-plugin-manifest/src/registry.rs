// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    config::NativePluginConfigEntry,
    manifest::NativePluginManifest,
    runtime::{NativePluginRuntimePlan, NativePluginState},
};

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginInfo {
    pub manifest: NativePluginManifest,
    pub install_dir: PathBuf,
    pub runtime_plan: NativePluginRuntimePlan,
    pub state: NativePluginState,
    pub config: NativePluginConfigEntry,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginProcessActivationPlan {
    pub plugin_id: String,
    pub manifest: NativePluginManifest,
    pub install_dir: PathBuf,
    pub entry: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginWasmActivationPlan {
    pub plugin_id: String,
    pub manifest: NativePluginManifest,
    pub install_dir: PathBuf,
    pub entry: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginRegistryEntry {
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
pub struct NativePluginRegistryIndex {
    pub version: u32,
    pub plugins: Vec<NativePluginRegistryEntry>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginUrlInstallResult {
    pub manifest: NativePluginManifest,
    pub checksum: String,
    pub replaced_existing: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginInstalledInfo {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePluginDiagnostic {
    pub plugin_dir: PathBuf,
    pub plugin_id: Option<String>,
    pub message: String,
}
