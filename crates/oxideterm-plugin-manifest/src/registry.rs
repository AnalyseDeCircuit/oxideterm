// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    config::NativePluginConfigEntry,
    manifest::{NativePluginEngines, NativePluginManifest},
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_url: Option<String>,
    pub version: String,
    #[serde(default, rename = "minOxideTermVersion", alias = "minOxidetermVersion")]
    pub min_oxideterm_version: Option<String>,
    #[serde(default)]
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
    /// Immutable release packages available for specific host targets.
    #[serde(default)]
    pub packages: Vec<NativePluginRegistryPackage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engines: Option<NativePluginEngines>,
    /// The top-level release remains readable by clients without history support.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub releases: Vec<NativePluginRegistryRelease>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<NativePluginRegistryHistory>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginRegistryHistory {
    pub download_url: String,
    pub checksum: String,
    pub size: u64,
}

impl NativePluginRegistryEntry {
    pub fn history_pending(&self) -> bool {
        self.history.is_some() && self.releases.is_empty() && self.packages.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginRegistryRelease {
    pub version: String,
    pub engines: NativePluginEngines,
    pub packages: Vec<NativePluginRegistryPackage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compatibility_corrections: Vec<NativePluginCompatibilityCorrection>,
}

impl NativePluginRegistryRelease {
    pub fn effective_engines(&self) -> &NativePluginEngines {
        self.compatibility_corrections
            .last()
            .map_or(&self.engines, |correction| &correction.engines)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginCompatibilityCorrection {
    pub engines: NativePluginEngines,
    pub reason: String,
    pub recorded_at: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginRegistryPackage {
    /// Rust-style target triple, or `any` for portable packages.
    pub target: String,
    pub download_url: String,
    pub checksum: String,
    #[serde(default)]
    pub size: Option<u64>,
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
