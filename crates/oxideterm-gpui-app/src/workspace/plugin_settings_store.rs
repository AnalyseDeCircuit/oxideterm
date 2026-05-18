// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs,
    path::{Path, PathBuf},
};

use oxideterm_connections::oxide_file::EncryptedPluginSetting;
use serde::{Deserialize, Serialize};

const PLUGIN_SETTINGS_FILENAME: &str = "plugin-settings.json";
const PLUGIN_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginSettingsSnapshot {
    version: u32,
    settings: Vec<EncryptedPluginSetting>,
}

pub(super) fn plugin_settings_path(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(PLUGIN_SETTINGS_FILENAME)
}

pub(super) fn load_plugin_settings(
    settings_path: &Path,
) -> Result<Vec<EncryptedPluginSetting>, String> {
    let path = plugin_settings_path(settings_path);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }
    let snapshot =
        serde_json::from_str::<PluginSettingsSnapshot>(&contents).map_err(|err| err.to_string())?;
    Ok(snapshot.settings)
}

pub(super) fn upsert_plugin_settings(
    settings_path: &Path,
    incoming: &[EncryptedPluginSetting],
) -> Result<usize, String> {
    if incoming.is_empty() {
        return Ok(0);
    }
    let path = plugin_settings_path(settings_path);
    let mut existing = load_plugin_settings(settings_path)?;
    for entry in incoming {
        if let Some(current) = existing
            .iter_mut()
            .find(|candidate| candidate.storage_key == entry.storage_key)
        {
            *current = entry.clone();
        } else {
            existing.push(entry.clone());
        }
    }
    existing.sort_by(|left, right| left.storage_key.cmp(&right.storage_key));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let snapshot = PluginSettingsSnapshot {
        version: PLUGIN_SETTINGS_SCHEMA_VERSION,
        settings: existing,
    };
    let json = serde_json::to_vec_pretty(&snapshot).map_err(|err| err.to_string())?;
    fs::write(&path, json).map_err(|err| err.to_string())?;
    Ok(incoming.len())
}
