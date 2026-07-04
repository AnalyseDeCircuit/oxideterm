// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Runtime entry path validation for Wasm plugin modules.

use std::{
    fs,
    path::{Path, PathBuf},
};

use oxideterm_plugin_protocol::PluginError;

pub fn resolve_wasm_runtime_entry(plugin_dir: &Path, entry: &str) -> Result<PathBuf, PluginError> {
    validate_plugin_relative_path(entry).map_err(|error| {
        PluginError::protocol(
            "invalid_wasm_entry",
            format!("Invalid runtime entry: {error}"),
        )
    })?;
    let plugin_dir = fs::canonicalize(plugin_dir).map_err(|error| {
        PluginError::runtime(
            "plugin_dir_unavailable",
            format!("Cannot resolve plugin directory: {error}"),
        )
    })?;
    let module = fs::canonicalize(plugin_dir.join(entry)).map_err(|error| {
        PluginError::runtime(
            "wasm_entry_unavailable",
            format!("Cannot resolve native plugin wasm entry \"{entry}\": {error}"),
        )
    })?;
    if !module.starts_with(&plugin_dir) {
        return Err(PluginError::protocol(
            "wasm_entry_escapes_plugin_dir",
            format!(
                "Native plugin wasm entry \"{}\" resolves outside plugin directory",
                entry
            ),
        ));
    }
    let bytes = fs::read(&module).map_err(|error| {
        PluginError::runtime(
            "wasm_entry_unreadable",
            format!("Cannot read native plugin WASM entry \"{entry}\": {error}"),
        )
    })?;
    if bytes.get(0..4) != Some(b"\0asm") {
        return Err(PluginError::protocol(
            "wasm_entry_invalid_magic",
            format!("Native plugin WASM entry \"{entry}\" is not a WebAssembly module"),
        ));
    }
    Ok(module)
}

fn validate_plugin_relative_path(relative_path: &str) -> Result<(), String> {
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
