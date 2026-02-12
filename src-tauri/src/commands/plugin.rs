//! Plugin system backend commands
//!
//! Handles plugin discovery, file reading, and configuration persistence.
//! Plugin directory: config_dir()/plugins/{plugin-id}/
//! Plugin config: config_dir()/plugin-config.json

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::storage::config_dir;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Plugin manifest (plugin.json) — matches frontend PluginManifest type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    pub main: String,
    #[serde(default)]
    pub engines: Option<PluginEngines>,
    #[serde(default)]
    pub contributes: Option<PluginContributes>,
    #[serde(default)]
    pub locales: Option<String>,

    // ── v2 Package Fields ────────────────────────────────────────────────
    /// Manifest schema version (1 = legacy single-file, 2 = package)
    #[serde(default)]
    pub manifest_version: Option<u8>,
    /// Plugin format: "bundled" (single ESM) or "package" (multi-file)
    #[serde(default)]
    pub format: Option<String>,
    /// Static assets directory (relative path)
    #[serde(default)]
    pub assets: Option<String>,
    /// CSS files to auto-load on activation (relative paths)
    #[serde(default)]
    pub styles: Option<Vec<String>>,
    /// Shared dependencies the plugin expects from the host
    #[serde(default)]
    pub shared_dependencies: Option<std::collections::HashMap<String, String>>,
    /// Plugin repository URL
    #[serde(default)]
    pub repository: Option<String>,
    /// SHA-256 checksum of the plugin package
    #[serde(default)]
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEngines {
    #[serde(default)]
    pub oxideterm: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributes {
    #[serde(default)]
    pub tabs: Option<Vec<PluginTabDef>>,
    #[serde(default)]
    pub sidebar_panels: Option<Vec<PluginSidebarDef>>,
    #[serde(default)]
    pub settings: Option<Vec<PluginSettingDef>>,
    #[serde(default)]
    pub terminal_hooks: Option<PluginTerminalHooksDef>,
    #[serde(default)]
    pub connection_hooks: Option<Vec<String>>,
    #[serde(default)]
    pub api_commands: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginTabDef {
    pub id: String,
    pub title: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSidebarDef {
    pub id: String,
    pub title: String,
    pub icon: String,
    #[serde(default = "default_position")]
    pub position: String,
}

fn default_position() -> String {
    "bottom".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSettingDef {
    pub id: String,
    #[serde(rename = "type")]
    pub setting_type: String,
    pub default: serde_json::Value,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<PluginSettingOption>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSettingOption {
    pub label: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginTerminalHooksDef {
    #[serde(default)]
    pub input_interceptor: Option<bool>,
    #[serde(default)]
    pub output_processor: Option<bool>,
    #[serde(default)]
    pub shortcuts: Option<Vec<PluginShortcutDef>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginShortcutDef {
    pub key: String,
    pub command: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// Helper functions
// ═══════════════════════════════════════════════════════════════════════════

/// Get the plugins directory path
fn plugins_dir() -> Result<PathBuf, String> {
    config_dir()
        .map(|dir| dir.join("plugins"))
        .map_err(|e| e.to_string())
}

/// Get the plugin config file path
fn plugin_config_path() -> Result<PathBuf, String> {
    config_dir()
        .map(|dir| dir.join("plugin-config.json"))
        .map_err(|e| e.to_string())
}

/// Validate that a relative path does not escape the plugin directory
pub fn validate_relative_path(relative_path: &str) -> Result<(), String> {
    // Reject absolute paths
    if relative_path.starts_with('/') || relative_path.starts_with('\\') {
        return Err("Absolute paths are not allowed".to_string());
    }
    // Reject path traversal: check each component for ".."
    for component in relative_path.split(['/', '\\']) {
        if component == ".." {
            return Err("Path traversal (..) is not allowed".to_string());
        }
    }
    Ok(())
}

/// Validate that a plugin_id is a safe directory name (no traversal, no separators).
pub fn validate_plugin_id(plugin_id: &str) -> Result<(), String> {
    if plugin_id.is_empty() {
        return Err("Plugin ID cannot be empty".to_string());
    }
    if plugin_id.contains("..") {
        return Err("Plugin ID cannot contain path traversal (..)".to_string());
    }
    if plugin_id.contains('/') || plugin_id.contains('\\') {
        return Err("Plugin ID cannot contain path separators".to_string());
    }
    // Reject null bytes and other control characters
    if plugin_id.bytes().any(|b| b < 0x20) {
        return Err("Plugin ID contains invalid characters".to_string());
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri Commands
// ═══════════════════════════════════════════════════════════════════════════

/// List all installed plugins by scanning the plugins directory.
/// Returns a Vec of PluginManifest from each subdirectory's plugin.json.
#[tauri::command]
pub async fn list_plugins() -> Result<Vec<PluginManifest>, String> {
    let dir = plugins_dir()?;

    // Create directory if it doesn't exist
    if !dir.exists() {
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| format!("Failed to create plugins directory: {}", e))?;
        return Ok(Vec::new());
    }

    let mut manifests = Vec::new();
    let mut entries = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| format!("Failed to read plugins directory: {}", e))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("Failed to read directory entry: {}", e))?
    {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("plugin.json");
        if !manifest_path.exists() {
            tracing::warn!("Plugin directory {:?} has no plugin.json, skipping", path);
            continue;
        }

        match tokio::fs::read_to_string(&manifest_path).await {
            Ok(content) => match serde_json::from_str::<PluginManifest>(&content) {
                Ok(manifest) => {
                    // Validate manifest has required fields
                    if manifest.id.is_empty()
                        || manifest.name.is_empty()
                        || manifest.main.is_empty()
                    {
                        tracing::warn!(
                            "Plugin {:?} has invalid manifest (missing required fields), skipping",
                            path
                        );
                        continue;
                    }
                    manifests.push(manifest);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse plugin.json in {:?}: {}", path, e);
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read plugin.json in {:?}: {}", path, e);
            }
        }
    }

    Ok(manifests)
}

/// Read a file from a plugin's directory.
/// Both plugin_id and relative_path are validated against path traversal.
#[tauri::command]
pub async fn read_plugin_file(plugin_id: String, relative_path: String) -> Result<Vec<u8>, String> {
    validate_plugin_id(&plugin_id)?;
    validate_relative_path(&relative_path)?;

    let file_path = plugins_dir()?.join(&plugin_id).join(&relative_path);

    // Verify the resolved path is still inside the plugin directory
    let canonical = file_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve plugin file path: {}", e))?;
    let plugin_root = plugins_dir()?.join(&plugin_id);
    if let Ok(canonical_root) = plugin_root.canonicalize() {
        if !canonical.starts_with(&canonical_root) {
            return Err("Path escapes plugin directory".to_string());
        }
    }

    // Read the canonicalized path to avoid TOCTOU (symlink swap between check and read)
    // Cap file size to 10 MB to prevent memory bloat from IPC Vec<u8> transfer
    const MAX_PLUGIN_FILE: u64 = 10 * 1024 * 1024;
    let meta = tokio::fs::metadata(&canonical)
        .await
        .map_err(|e| format!("Failed to stat plugin file '{}': {}", relative_path, e))?;
    if meta.len() > MAX_PLUGIN_FILE {
        return Err(format!(
            "Plugin file '{}' is too large ({} bytes, max {} bytes)",
            relative_path,
            meta.len(),
            MAX_PLUGIN_FILE
        ));
    }

    tokio::fs::read(&canonical)
        .await
        .map_err(|e| format!("Failed to read plugin file '{}': {}", relative_path, e))
}

/// Save plugin configuration (enabled/disabled state for each plugin).
/// Stored as JSON in config_dir()/plugin-config.json.
#[tauri::command]
pub async fn save_plugin_config(config: String) -> Result<(), String> {
    let path = plugin_config_path()?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    tokio::fs::write(&path, config.as_bytes())
        .await
        .map_err(|e| format!("Failed to save plugin config: {}", e))
}

/// Load plugin configuration from config_dir()/plugin-config.json.
/// Returns the raw JSON string, or "{}" if the file doesn't exist.
#[tauri::command]
pub async fn load_plugin_config() -> Result<String, String> {
    let path = plugin_config_path()?;

    if !path.exists() {
        return Ok("{}".to_string());
    }

    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to load plugin config: {}", e))
}
