// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Plugin Registry Commands
//!
//! Handles remote plugin discovery, download, installation, update checking,
//! and uninstallation.
//!
//! Security:
//! - SHA-256 checksum verification on downloaded packages
//! - zip-slip protection via `enclosed_name()`
//! - Plugin ID matching validation (manifest ID must match expected ID)
//! - Maximum package size limit (50 MB)

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use zip::ZipArchive;

use super::plugin::PluginManifest;
use crate::config::storage::config_dir;

/// Maximum plugin package size: 50 MB
const MAX_PACKAGE_SIZE: u64 = 50 * 1024 * 1024;
/// Maximum extracted plugin size: 100 MB
const MAX_EXTRACTED_SIZE: u64 = 100 * 1024 * 1024;
/// Maximum number of files/directories inside a plugin archive
const MAX_ARCHIVE_ENTRIES: usize = 2048;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// A plugin entry from the remote registry index.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
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
    pub homepage: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// The registry index fetched from a remote URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndex {
    pub version: u32,
    pub plugins: Vec<RegistryEntry>,
}

/// Result payload for URL-based plugin installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlInstallResult {
    pub manifest: PluginManifest,
    pub checksum: String,
    pub replaced_existing: bool,
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Get the plugins directory path.
fn plugins_dir() -> Result<std::path::PathBuf, String> {
    config_dir()
        .map(|dir| dir.join("plugins"))
        .map_err(|e| e.to_string())
}

/// Verify SHA-256 checksum of data against an expected hex string.
fn verify_checksum(data: &[u8], expected: &str) -> Result<(), String> {
    let actual = sha256_hex(data);

    // Support both raw hex and "sha256:" prefixed format
    let expected_hex = expected
        .strip_prefix("sha256:")
        .unwrap_or(expected)
        .to_lowercase();

    if actual != expected_hex {
        return Err(format!(
            "Checksum mismatch: expected {}, got {}",
            expected_hex, actual
        ));
    }
    Ok(())
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Extract a plugin ZIP archive to a destination directory.
/// Uses `enclosed_name()` for zip-slip protection.
fn extract_plugin_zip(data: &[u8], dest: &Path) -> Result<(), String> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = ZipArchive::new(cursor).map_err(|e| format!("Invalid ZIP archive: {}", e))?;

    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!(
            "Plugin archive contains too many entries: {} (max {})",
            archive.len(),
            MAX_ARCHIVE_ENTRIES
        ));
    }

    let mut extracted_bytes: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry {}: {}", i, e))?;

        // Zip-slip protection: enclosed_name() returns None for paths that escape
        let relative_path = match file.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => {
                tracing::warn!(
                    "[PluginRegistry] Skipping potentially unsafe path: {:?}",
                    file.name()
                );
                continue;
            }
        };

        let out_path = dest.join(&relative_path);

        if file.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("Failed to create dir {:?}: {}", out_path, e))?;
        } else {
            extracted_bytes = extracted_bytes
                .checked_add(file.size())
                .ok_or_else(|| "Plugin archive extracted size overflowed".to_string())?;

            if extracted_bytes > MAX_EXTRACTED_SIZE {
                return Err(format!(
                    "Plugin archive expands to {} bytes (max {} bytes)",
                    extracted_bytes, MAX_EXTRACTED_SIZE
                ));
            }

            // Ensure parent directory exists
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent dir {:?}: {}", parent, e))?;
            }

            let mut out_file = std::fs::File::create(&out_path)
                .map_err(|e| format!("Failed to create file {:?}: {}", out_path, e))?;

            std::io::copy(&mut file, &mut out_file)
                .map_err(|e| format!("Failed to write file {:?}: {}", out_path, e))?;
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tauri Commands
// ═══════════════════════════════════════════════════════════════════════════

/// Fetch the plugin registry index from a remote URL.
#[tauri::command]
pub async fn fetch_plugin_registry(url: String) -> Result<RegistryIndex, String> {
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch registry: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Registry returned HTTP {}",
            response.status().as_u16()
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read registry response: {}", e))?;

    serde_json::from_str::<RegistryIndex>(&body)
        .map_err(|e| format!("Failed to parse registry index: {}", e))
}

/// Download, verify, and install a plugin from a remote URL.
/// Returns the installed plugin's manifest.
#[tauri::command]
pub async fn install_plugin(
    download_url: String,
    expected_id: String,
    checksum: Option<String>,
) -> Result<PluginManifest, String> {
    // Validate expected_id to prevent path traversal via crafted IPC calls
    super::plugin::validate_plugin_id(&expected_id)?;

    // 1. Download the ZIP package
    let response = reqwest::get(&download_url)
        .await
        .map_err(|e| format!("Failed to download plugin: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download returned HTTP {}",
            response.status().as_u16()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download body: {}", e))?;

    // 2. Enforce size limit
    if bytes.len() as u64 > MAX_PACKAGE_SIZE {
        return Err(format!(
            "Plugin package too large: {} bytes (max {} bytes)",
            bytes.len(),
            MAX_PACKAGE_SIZE
        ));
    }

    // 3. Verify checksum if provided
    if let Some(ref expected_checksum) = checksum {
        verify_checksum(&bytes, expected_checksum)?;
    }

    // 4. Extract ZIP to a temporary directory first
    let dest_dir = plugins_dir()?.join(&expected_id);
    let temp_dir = plugins_dir()?.join(format!(".{}-installing", expected_id));

    // Clean up any previous failed install
    if temp_dir.exists() {
        tokio::fs::remove_dir_all(&temp_dir)
            .await
            .map_err(|e| format!("Failed to clean temp dir: {}", e))?;
    }
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| format!("Failed to create temp dir: {}", e))?;

    // Extract ZIP (blocking I/O — run in spawn_blocking)
    let temp_dir_clone = temp_dir.clone();
    let bytes_vec = bytes.to_vec();
    tokio::task::spawn_blocking(move || extract_plugin_zip(&bytes_vec, &temp_dir_clone))
        .await
        .map_err(|e| format!("Extract task failed: {}", e))??;

    // 5. Read and validate the extracted manifest
    let manifest_path = temp_dir.join("plugin.json");
    let manifest_str = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(|e| format!("No plugin.json in package: {}", e))?;

    let manifest: PluginManifest =
        serde_json::from_str(&manifest_str).map_err(|e| format!("Invalid plugin.json: {}", e))?;

    // 6. Validate manifest ID matches expected
    if manifest.id != expected_id {
        // Clean up temp dir
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        return Err(format!(
            "Plugin ID mismatch: expected \"{}\", got \"{}\"",
            expected_id, manifest.id
        ));
    }

    // 7. Atomic swap: remove old version, rename temp → final
    if dest_dir.exists() {
        tokio::fs::remove_dir_all(&dest_dir)
            .await
            .map_err(|e| format!("Failed to remove old plugin: {}", e))?;
    }

    tokio::fs::rename(&temp_dir, &dest_dir)
        .await
        .map_err(|e| format!("Failed to finalize plugin install: {}", e))?;

    tracing::info!(
        "[PluginRegistry] Installed plugin \"{}\" v{}",
        manifest.id,
        manifest.version
    );

    Ok(manifest)
}

/// Download and install a plugin from a URL without requiring a pre-known plugin ID.
/// The plugin ID is auto-detected from the package's plugin.json manifest.
/// Returns the installed plugin's manifest.
#[tauri::command]
pub async fn install_plugin_from_url(
    download_url: String,
    checksum: Option<String>,
    overwrite: Option<bool>,
) -> Result<UrlInstallResult, String> {
    // 0. Validate URL scheme — only allow http/https to prevent SSRF via file://, ftp://, etc.
    let parsed_url =
        reqwest::Url::parse(&download_url).map_err(|e| format!("Invalid URL: {}", e))?;
    match parsed_url.scheme() {
        "https" | "http" => {}
        scheme => {
            return Err(format!(
                "Unsupported URL scheme: {}. Only http and https are allowed.",
                scheme
            ));
        }
    }

    // 1. Download the ZIP package with timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download plugin: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download returned HTTP {}",
            response.status().as_u16()
        ));
    }

    // 2. Early size check via Content-Length header (before buffering the body)
    if let Some(content_length) = response.content_length() {
        if content_length > MAX_PACKAGE_SIZE {
            return Err(format!(
                "Plugin package too large: {} bytes (max {} bytes)",
                content_length, MAX_PACKAGE_SIZE
            ));
        }
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download body: {}", e))?;

    // Post-download size check (Content-Length can be absent or lying)
    if bytes.len() as u64 > MAX_PACKAGE_SIZE {
        return Err(format!(
            "Plugin package too large: {} bytes (max {} bytes)",
            bytes.len(),
            MAX_PACKAGE_SIZE
        ));
    }

    let actual_checksum = sha256_hex(&bytes);

    // 3. Verify checksum if provided
    if let Some(ref expected_checksum) = checksum {
        verify_checksum(&bytes, expected_checksum)?;
    }

    // 4. Extract ZIP to a unique staging temp directory
    let staging_id = format!(
        ".url-install-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let staging_dir = plugins_dir()?.join(&staging_id);

    // Clean up any previous failed install with the same name (unlikely but defensive)
    if staging_dir.exists() {
        tokio::fs::remove_dir_all(&staging_dir)
            .await
            .map_err(|e| format!("Failed to clean staging dir: {}", e))?;
    }
    tokio::fs::create_dir_all(&staging_dir)
        .await
        .map_err(|e| format!("Failed to create staging dir: {}", e))?;

    // Extract ZIP (blocking I/O — run in spawn_blocking)
    let staging_clone = staging_dir.clone();
    let bytes_vec = bytes.to_vec();
    let extract_result =
        tokio::task::spawn_blocking(move || extract_plugin_zip(&bytes_vec, &staging_clone))
            .await
            .map_err(|e| format!("Extract task failed: {}", e));

    // Clean up staging on extract failure
    match extract_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let _ = tokio::fs::remove_dir_all(&staging_dir).await;
            return Err(e);
        }
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&staging_dir).await;
            return Err(e);
        }
    }

    // 5. Read manifest to discover plugin ID
    //    Support both flat layout (plugin.json at root) and nested layout
    //    (e.g. GitHub archives wrap files in a repo-branch/ subdirectory).
    let manifest_path = staging_dir.join("plugin.json");
    let (manifest_str, nested_root) = if manifest_path.exists() {
        // Flat layout — plugin.json at ZIP root
        let s = tokio::fs::read_to_string(&manifest_path)
            .await
            .map_err(|e| {
                let dir = staging_dir.clone();
                tokio::spawn(async move {
                    let _ = tokio::fs::remove_dir_all(&dir).await;
                });
                format!("Failed to read plugin.json: {}", e)
            })?;
        (s, None)
    } else {
        // Look for plugin.json inside a single top-level subdirectory
        // (common with GitHub archive ZIPs: repo-branch/plugin.json)
        let mut entries = tokio::fs::read_dir(&staging_dir).await.map_err(|e| {
            let dir = staging_dir.clone();
            tokio::spawn(async move {
                let _ = tokio::fs::remove_dir_all(&dir).await;
            });
            format!("Failed to read staging directory: {}", e)
        })?;

        let mut nested: Option<std::path::PathBuf> = None;
        while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
            let ft = entry.file_type().await.map_err(|e| e.to_string())?;
            // Skip symlinks to prevent escaping the staging directory
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                let candidate = entry.path().join("plugin.json");
                if candidate.exists() {
                    nested = Some(entry.path());
                    break;
                }
            }
        }

        match nested {
            Some(subdir) => {
                let s = tokio::fs::read_to_string(subdir.join("plugin.json"))
                    .await
                    .map_err(|e| {
                        let dir = staging_dir.clone();
                        tokio::spawn(async move {
                            let _ = tokio::fs::remove_dir_all(&dir).await;
                        });
                        format!("Failed to read plugin.json: {}", e)
                    })?;
                (s, Some(subdir))
            }
            None => {
                let _ = tokio::fs::remove_dir_all(&staging_dir).await;
                return Err(
                    "No plugin.json found in package (checked root and subdirectories)".to_string(),
                );
            }
        }
    };

    let manifest: PluginManifest = serde_json::from_str(&manifest_str).map_err(|e| {
        let dir = staging_dir.clone();
        tokio::spawn(async move {
            let _ = tokio::fs::remove_dir_all(&dir).await;
        });
        format!("Invalid plugin.json: {}", e)
    })?;

    // 6. Validate the discovered plugin ID
    if let Err(e) = super::plugin::validate_plugin_id(&manifest.id) {
        let _ = tokio::fs::remove_dir_all(&staging_dir).await;
        return Err(format!("Invalid plugin ID in manifest: {}", e));
    }

    // 7. Safe swap: backup old version, move source into place, clean up.
    //    If nested (e.g. GitHub ZIP), the actual plugin content is inside a subdirectory.
    let source_dir = nested_root.unwrap_or_else(|| staging_dir.clone());
    let dest_dir = plugins_dir()?.join(&manifest.id);
    let backup_dir = plugins_dir()?.join(format!(".{}-backup", manifest.id));
    let replaced_existing = dest_dir.exists();

    if replaced_existing && !overwrite.unwrap_or(false) {
        let _ = tokio::fs::remove_dir_all(&staging_dir).await;
        return Err(format!("PLUGIN_ID_CONFLICT:{}", manifest.id));
    }

    // Remove any stale backup from a previous failed install
    if backup_dir.exists() {
        let _ = tokio::fs::remove_dir_all(&backup_dir).await;
    }

    // Move old version to backup (if it exists) instead of deleting first
    if dest_dir.exists() {
        tokio::fs::rename(&dest_dir, &backup_dir)
            .await
            .map_err(|e| format!("Failed to backup old plugin: {}", e))?;
    }

    // Move new version into place
    match tokio::fs::rename(&source_dir, &dest_dir).await {
        Ok(()) => {
            // Success — remove backup
            if backup_dir.exists() {
                if let Err(cleanup_err) = tokio::fs::remove_dir_all(&backup_dir).await {
                    tracing::warn!(
                        "[PluginRegistry] Installed plugin \"{}\" but failed to remove backup dir {:?}: {}",
                        manifest.id,
                        backup_dir,
                        cleanup_err
                    );
                }
            }
        }
        Err(e) => {
            // Rename failed — restore backup if we had one
            if backup_dir.exists() {
                if let Err(restore_err) = tokio::fs::rename(&backup_dir, &dest_dir).await {
                    let _ = tokio::fs::remove_dir_all(&staging_dir).await;
                    return Err(format!(
                        "Failed to finalize plugin install: {}. Rollback also failed: {}",
                        e, restore_err
                    ));
                }
            }
            let _ = tokio::fs::remove_dir_all(&staging_dir).await;
            return Err(format!("Failed to finalize plugin install: {}", e));
        }
    }

    // Clean up the staging directory if we used a nested subdirectory
    if staging_dir.exists() {
        let _ = tokio::fs::remove_dir_all(&staging_dir).await;
    }

    tracing::info!(
        "[PluginRegistry] Installed plugin \"{}\" v{} from URL",
        manifest.id,
        manifest.version
    );

    Ok(UrlInstallResult {
        manifest,
        checksum: actual_checksum,
        replaced_existing,
    })
}

/// Uninstall a plugin by removing its directory.
#[tauri::command]
pub async fn uninstall_plugin(plugin_id: String) -> Result<(), String> {
    // Validate plugin ID to prevent path traversal
    super::plugin::validate_plugin_id(&plugin_id)?;

    let plugin_dir = plugins_dir()?.join(&plugin_id);

    if !plugin_dir.exists() {
        return Err(format!("Plugin \"{}\" is not installed", plugin_id));
    }

    // Verify it's actually a plugin directory (has plugin.json)
    let manifest_path = plugin_dir.join("plugin.json");
    if !manifest_path.exists() {
        return Err(format!(
            "Directory \"{}\" does not appear to be a valid plugin",
            plugin_id
        ));
    }

    tokio::fs::remove_dir_all(&plugin_dir)
        .await
        .map_err(|e| format!("Failed to remove plugin directory: {}", e))?;

    tracing::info!("[PluginRegistry] Uninstalled plugin \"{}\"", plugin_id);

    Ok(())
}

/// Installed plugin info for update checking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginInfo {
    pub id: String,
    pub version: String,
}

/// Check for available updates by comparing installed versions with registry.
/// Returns registry entries for plugins that have newer versions available.
#[tauri::command]
pub async fn check_plugin_updates(
    registry_url: String,
    installed: Vec<InstalledPluginInfo>,
) -> Result<Vec<RegistryEntry>, String> {
    // Fetch the registry
    let registry = fetch_plugin_registry(registry_url).await?;

    // Build a map of installed plugins for quick lookup
    let installed_map: std::collections::HashMap<&str, &str> = installed
        .iter()
        .map(|p| (p.id.as_str(), p.version.as_str()))
        .collect();

    // Find plugins with available updates
    let updates: Vec<RegistryEntry> = registry
        .plugins
        .into_iter()
        .filter(|entry| {
            if let Some(&installed_version) = installed_map.get(entry.id.as_str()) {
                // Simple semver comparison: split by dots and compare numerically
                is_newer_version(&entry.version, installed_version)
            } else {
                false
            }
        })
        .collect();

    Ok(updates)
}

/// Simple semver comparison: returns true if `new_ver` is newer than `old_ver`.
/// Handles versions like "1.2.3", "1.2", "1".
fn is_newer_version(new_ver: &str, old_ver: &str) -> bool {
    let parse_parts =
        |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse::<u32>().ok()).collect() };

    let new_parts = parse_parts(new_ver);
    let old_parts = parse_parts(old_ver);

    for i in 0..new_parts.len().max(old_parts.len()) {
        let new_num = new_parts.get(i).copied().unwrap_or(0);
        let old_num = old_parts.get(i).copied().unwrap_or(0);

        if new_num > old_num {
            return true;
        }
        if new_num < old_num {
            return false;
        }
    }

    false // Equal versions
}
