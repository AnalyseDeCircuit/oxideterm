//! Runtime entry path validation for native plugin runners.
//!
//! Path resolution lives beside the runtime bridges because both process and
//! WASM plugins need the same registry-backed relative path safety checks before
//! loading host-visible executable content.

use super::*;

pub fn resolve_process_runtime_entry(
    plugin_dir: &Path,
    entry: &str,
) -> Result<PathBuf, PluginError> {
    validate_plugin_relative_path(entry).map_err(|error| {
        PluginError::protocol(
            "invalid_process_entry",
            format!("Invalid runtime entry: {error}"),
        )
    })?;
    let plugin_dir = fs::canonicalize(plugin_dir).map_err(|error| {
        PluginError::runtime(
            "plugin_dir_unavailable",
            format!("Cannot resolve plugin directory: {error}"),
        )
    })?;
    let executable = fs::canonicalize(plugin_dir.join(entry)).map_err(|error| {
        PluginError::runtime(
            "process_entry_unavailable",
            format!("Cannot resolve native plugin process entry \"{entry}\": {error}"),
        )
    })?;
    if !executable.starts_with(&plugin_dir) {
        return Err(PluginError::protocol(
            "process_entry_escapes_plugin_dir",
            format!(
                "Native plugin process entry \"{}\" resolves outside plugin directory",
                entry
            ),
        ));
    }
    Ok(executable)
}

pub fn resolve_wasm_runtime_entry(plugin_dir: &Path, entry: &str) -> Result<PathBuf, PluginError> {
    let module = resolve_plugin_runtime_entry(plugin_dir, entry, "wasm")?;
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

fn resolve_plugin_runtime_entry(
    plugin_dir: &Path,
    entry: &str,
    runtime_kind: &str,
) -> Result<PathBuf, PluginError> {
    validate_plugin_relative_path(entry).map_err(|error| {
        PluginError::protocol(
            format!("invalid_{runtime_kind}_entry"),
            format!("Invalid runtime entry: {error}"),
        )
    })?;
    let plugin_dir = fs::canonicalize(plugin_dir).map_err(|error| {
        PluginError::runtime(
            "plugin_dir_unavailable",
            format!("Cannot resolve plugin directory: {error}"),
        )
    })?;
    let executable = fs::canonicalize(plugin_dir.join(entry)).map_err(|error| {
        PluginError::runtime(
            format!("{runtime_kind}_entry_unavailable"),
            format!("Cannot resolve native plugin {runtime_kind} entry \"{entry}\": {error}"),
        )
    })?;
    if !executable.starts_with(&plugin_dir) {
        return Err(PluginError::protocol(
            format!("{runtime_kind}_entry_escapes_plugin_dir"),
            format!(
                "Native plugin {runtime_kind} entry \"{}\" resolves outside plugin directory",
                entry
            ),
        ));
    }
    Ok(executable)
}
