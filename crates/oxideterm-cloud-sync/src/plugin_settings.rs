// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
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

pub fn plugin_settings_path(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or(settings_path)
        .join(PLUGIN_SETTINGS_FILENAME)
}

pub fn load_plugin_settings(settings_path: &Path) -> Result<Vec<EncryptedPluginSetting>, String> {
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

pub fn upsert_plugin_settings(
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

pub fn plugin_settings_revision_map(
    settings_path: &Path,
) -> Result<BTreeMap<String, String>, String> {
    let entries = load_plugin_settings(settings_path)?;
    let mut grouped = BTreeMap::<String, Vec<EncryptedPluginSetting>>::new();
    for entry in entries {
        let Some(plugin_id) = parse_plugin_setting_storage_key(&entry.storage_key) else {
            continue;
        };
        grouped.entry(plugin_id).or_default().push(entry);
    }

    let mut revisions = BTreeMap::new();
    for (plugin_id, mut entries) in grouped {
        entries.sort_by(|left, right| left.storage_key.cmp(&right.storage_key));
        let normalized = entries
            .into_iter()
            .map(|entry| vec![entry.storage_key, entry.serialized_value])
            .collect::<Vec<_>>();
        let json = serde_json::to_string(&normalized).map_err(|err| err.to_string())?;
        revisions.insert(plugin_id, tauri_fnv1a_stable_hash_text(&json));
    }
    Ok(revisions)
}

fn parse_plugin_setting_storage_key(storage_key: &str) -> Option<String> {
    const PREFIX: &str = "oxide-plugin-";
    const SEPARATOR: &str = "-setting-";
    let remainder = storage_key.strip_prefix(PREFIX)?;
    let separator_index = remainder.find(SEPARATOR)?;
    let plugin_id = &remainder[..separator_index];
    let setting_id = &remainder[separator_index + SEPARATOR.len()..];
    (!plugin_id.is_empty() && !setting_id.is_empty()).then(|| plugin_id.to_string())
}

fn tauri_fnv1a_stable_hash_text(text: &str) -> String {
    let mut hash = 2166136261u32;
    for code_unit in text.encode_utf16() {
        hash ^= u32::from(code_unit);
        hash = hash.wrapping_mul(16777619);
    }
    format!("fnv1a-{hash:x}")
}
