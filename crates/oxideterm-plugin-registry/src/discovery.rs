// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Native plugin discovery, manifest loading, and config persistence.

use super::*;

pub(crate) fn discover_native_plugins_in_dir(
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

pub(crate) fn load_native_plugin_manifest(
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

pub(crate) fn native_plugin_diagnostic(
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

pub fn load_native_plugin_config(config_path: &Path) -> NativePluginGlobalConfig {
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

pub fn save_native_plugin_config(
    config_path: &Path,
    config: &NativePluginGlobalConfig,
) -> Result<(), String> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_vec_pretty(config).map_err(|error| error.to_string())?;
    fs::write(config_path, json).map_err(|error| error.to_string())
}
