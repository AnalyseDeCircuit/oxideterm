//! Runtime entry path validation for native plugin runners.
//!
//! Path resolution lives beside the process runtime bridge because process
//! plugins load host-visible executable content.

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
