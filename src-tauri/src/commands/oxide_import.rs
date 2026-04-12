// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Tauri commands for .oxide file import

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tauri::ipc::Channel;
use tracing::info;
use uuid::Uuid;

use crate::commands::config::{ConfigState, collect_connection_keychain_ids};
use crate::commands::forwarding::ForwardingRegistry;
use crate::config::types::{
    CONFIG_VERSION, ConnectionOptions, ProxyHopConfig, SavedAuth, SavedConnection,
};
use crate::forwarding::{ForwardRule, ForwardStatus};
use crate::oxide_file::{
    EncryptedAuth, EncryptedForward, EncryptedPluginSetting, EncryptedProxyHop, OxideMetadata,
    decrypt_oxide_file_with_progress,
};
use crate::state::PersistedForward;
use zeroize::Zeroizing;

const PREVIEW_IMPORT_TOTAL_STEPS: usize = 8;
const APPLY_IMPORT_TOTAL_STEPS: usize = 10;

/// Result of importing connections from .oxide file
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub merged: usize,
    pub replaced: usize,
    pub renamed: usize,
    pub errors: Vec<String>,
    /// List of name changes: [(original_name, new_name)]
    pub renames: Vec<(String, String)>,
    pub imported_forwards: usize,
    pub skipped_forwards: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultEnvelope {
    pub imported: usize,
    pub skipped: usize,
    pub merged: usize,
    pub replaced: usize,
    pub renamed: usize,
    pub errors: Vec<String>,
    pub renames: Vec<(String, String)>,
    pub imported_forwards: usize,
    pub skipped_forwards: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_settings_json: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugin_settings: Vec<EncryptedPluginSetting>,
}

/// Preview information before import
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    /// Total number of connections in the file
    pub total_connections: usize,
    /// Connections that will be imported without changes
    pub unchanged: Vec<String>,
    /// Connections that will be renamed: [(original_name, new_name)]
    pub will_rename: Vec<(String, String)>,
    /// Connections that will be skipped due to existing local conflicts
    pub will_skip: Vec<String>,
    /// Connections that will replace existing local connections in place
    pub will_replace: Vec<String>,
    /// Connections that will be merged into an existing local connection
    pub will_merge: Vec<String>,
    /// Whether any embedded keys will be extracted
    pub has_embedded_keys: bool,
    /// Total number of port forwarding rules across all connections
    pub total_forwards: usize,
    /// Whether the payload includes a global app settings snapshot.
    pub has_app_settings: bool,
    /// App settings preview format: legacy or sectioned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_settings_format: Option<String>,
    /// Top-level app settings keys present in the imported snapshot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_settings_keys: Vec<String>,
    /// Stringified top-level app settings values for shallow diff preview.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub app_settings_preview: HashMap<String, String>,
    /// App settings grouped by importable section.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_settings_sections: Vec<AppSettingsSectionPreview>,
    /// Number of plugin setting entries bundled in the payload.
    pub plugin_settings_count: usize,
    /// Plugin settings grouped by plugin id.
    pub plugin_settings_by_plugin: HashMap<String, usize>,
    /// Flattened saved forward details for the preview UI.
    pub forward_details: Vec<ForwardDetail>,
    /// Record-level preview details for richer plugin UIs.
    pub records: Vec<ImportPreviewRecord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardDetail {
    pub owner_connection_name: String,
    pub direction: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsSectionPreview {
    pub id: String,
    pub field_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub field_values: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contains_env_vars: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewRecord {
    pub resource: String,
    pub name: String,
    pub action: String,
    pub reason_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_connection_id: Option<String>,
    pub forward_count: usize,
    pub has_embedded_keys: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OxideImportProgressEvent {
    pub stage: String,
    pub current: usize,
    pub total: usize,
}

fn emit_import_progress(
    on_progress: Option<&Channel<OxideImportProgressEvent>>,
    stage: &str,
    current: usize,
    total: usize,
) {
    let Some(channel) = on_progress else {
        return;
    };

    let _ = channel.send(OxideImportProgressEvent {
        stage: stage.to_string(),
        current,
        total,
    });
}

fn map_import_error(error: crate::oxide_file::OxideFileError) -> String {
    match error {
        crate::oxide_file::OxideFileError::DecryptionFailed => {
            "Decryption failed: incorrect password or corrupted file".to_string()
        }
        crate::oxide_file::OxideFileError::ChecksumMismatch => {
            "Verification failed: file contents may have been tampered with".to_string()
        }
        _ => "Failed to decrypt .oxide file".to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportConflictStrategy {
    Rename,
    Skip,
    Replace,
    Merge,
}

impl ImportConflictStrategy {
    fn parse(value: Option<String>) -> Result<Self, String> {
        match value.as_deref().unwrap_or("rename") {
            "rename" => Ok(Self::Rename),
            "skip" => Ok(Self::Skip),
            "replace" => Ok(Self::Replace),
            "merge" => Ok(Self::Merge),
            other => Err(format!("Unsupported conflict strategy: {}", other)),
        }
    }
}

#[derive(Debug, Clone)]
struct ReplaceTarget {
    existing: SavedConnection,
    old_keychain_ids: Vec<String>,
}

#[derive(Debug, Clone)]
enum PlannedImportAction {
    Import,
    Rename(String),
    Skip,
    Replace(ReplaceTarget),
    Merge(ReplaceTarget),
}

fn preview_reason_code(action: &PlannedImportAction) -> &'static str {
    match action {
        PlannedImportAction::Import => "new-connection",
        PlannedImportAction::Rename(_) => "name-conflict",
        PlannedImportAction::Skip => "name-conflict-skipped",
        PlannedImportAction::Replace(_) => "replace-existing",
        PlannedImportAction::Merge(_) => "merge-existing",
    }
}

fn parse_plugin_id_from_setting_storage_key(storage_key: &str) -> Option<String> {
    const PREFIX: &str = "oxide-plugin-";
    const SEPARATOR: &str = "-setting-";

    let remainder = storage_key.strip_prefix(PREFIX)?;
    let separator_index = remainder.find(SEPARATOR)?;
    let plugin_id = &remainder[..separator_index];
    let setting_id = &remainder[separator_index + SEPARATOR.len()..];

    if plugin_id.is_empty() || setting_id.is_empty() {
        return None;
    }

    Some(plugin_id.to_string())
}

fn format_forward_preview_description(forward: &EncryptedForward) -> String {
    let summary = match forward.forward_type.as_str() {
        "local" => format!(
            "L:{} -> {}:{}",
            forward.bind_port, forward.target_host, forward.target_port
        ),
        "remote" => format!(
            "R:{} -> {}:{}",
            forward.bind_port, forward.target_host, forward.target_port
        ),
        "dynamic" => format!("D:{} -> SOCKS", forward.bind_port),
        other => format!(
            "{}:{} -> {}:{}",
            other, forward.bind_port, forward.target_host, forward.target_port
        ),
    };

    match forward.description.as_deref().map(str::trim) {
        Some("") | None => summary,
        Some(description) => format!("{} ({})", description, summary),
    }
}

const OXIDE_APP_SETTINGS_ENVELOPE_FORMAT: &str = "oxide-settings-sections-v1";

fn add_preview_fields(
    object: Option<&serde_json::Map<String, Value>>,
    keys: &[&str],
    prefix: Option<&str>,
    target: &mut HashMap<String, String>,
) {
    let Some(object) = object else {
        return;
    };

    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };

        let preview_key = prefix
            .map(|prefix| format!("{}.{}", prefix, key))
            .unwrap_or_else(|| key.to_string());

        if let Ok(serialized) = serde_json::to_string(value) {
            target.insert(preview_key, serialized);
        }
    }
}

fn add_env_var_preview(value: Option<&Value>, target: &mut HashMap<String, String>) -> bool {
    let Some(Value::Object(map)) = value else {
        return false;
    };

    let mut env_var_names: Vec<String> = map.keys().cloned().collect();
    env_var_names.sort();
    target.insert(
        "customEnvVars".to_string(),
        if env_var_names.is_empty() {
            "0".to_string()
        } else {
            env_var_names.join(", ")
        },
    );
    true
}

fn build_section_preview(
    id: &str,
    field_values: HashMap<String, String>,
    contains_env_vars: bool,
) -> Option<AppSettingsSectionPreview> {
    if field_values.is_empty() && !contains_env_vars {
        return None;
    }

    let mut field_keys: Vec<String> = field_values.keys().cloned().collect();
    field_keys.sort();

    Some(AppSettingsSectionPreview {
        id: id.to_string(),
        field_keys,
        field_values,
        contains_env_vars,
    })
}

fn build_sectioned_app_settings_sections(
    settings: &serde_json::Map<String, Value>,
    section_ids: &[String],
) -> Vec<AppSettingsSectionPreview> {
    let mut sections = Vec::new();

    for section_id in section_ids {
        let mut field_values = HashMap::new();
        let mut contains_env_vars = false;

        match section_id.as_str() {
            "general" => {
                add_preview_fields(
                    settings.get("general").and_then(Value::as_object),
                    &["language", "updateChannel"],
                    None,
                    &mut field_values,
                );
            }
            "terminalAppearance" => {
                add_preview_fields(
                    settings.get("terminal").and_then(Value::as_object),
                    &[
                        "theme",
                        "fontFamily",
                        "customFontFamily",
                        "fontSize",
                        "lineHeight",
                        "cursorStyle",
                        "cursorBlink",
                        "backgroundEnabled",
                        "backgroundImage",
                        "backgroundOpacity",
                        "backgroundBlur",
                        "backgroundFit",
                        "backgroundEnabledTabs",
                    ],
                    None,
                    &mut field_values,
                );
            }
            "terminalBehavior" => {
                add_preview_fields(
                    settings.get("terminal").and_then(Value::as_object),
                    &[
                        "scrollback",
                        "renderer",
                        "adaptiveRenderer",
                        "showFpsOverlay",
                        "pasteProtection",
                        "smartCopy",
                        "osc52Clipboard",
                    ],
                    None,
                    &mut field_values,
                );
            }
            "appearance" => {
                add_preview_fields(
                    settings.get("appearance").and_then(Value::as_object),
                    &[
                        "sidebarCollapsedDefault",
                        "uiDensity",
                        "borderRadius",
                        "uiFontFamily",
                        "animationSpeed",
                        "frostedGlass",
                    ],
                    None,
                    &mut field_values,
                );
            }
            "connections" => {
                add_preview_fields(
                    settings
                        .get("connectionDefaults")
                        .and_then(Value::as_object),
                    &["username", "port"],
                    Some("connectionDefaults"),
                    &mut field_values,
                );
                add_preview_fields(
                    settings.get("reconnect").and_then(Value::as_object),
                    &["enabled", "maxAttempts", "baseDelayMs", "maxDelayMs"],
                    Some("reconnect"),
                    &mut field_values,
                );
                add_preview_fields(
                    settings.get("connectionPool").and_then(Value::as_object),
                    &["idleTimeoutSecs"],
                    Some("connectionPool"),
                    &mut field_values,
                );
            }
            "fileAndEditor" => {
                add_preview_fields(
                    settings.get("sftp").and_then(Value::as_object),
                    &[
                        "maxConcurrentTransfers",
                        "speedLimitEnabled",
                        "speedLimitKBps",
                        "conflictAction",
                    ],
                    Some("sftp"),
                    &mut field_values,
                );
                add_preview_fields(
                    settings.get("ide").and_then(Value::as_object),
                    &[
                        "autoSave",
                        "fontSize",
                        "lineHeight",
                        "agentMode",
                        "wordWrap",
                    ],
                    Some("ide"),
                    &mut field_values,
                );
            }
            "localTerminal" => {
                let local_terminal = settings.get("localTerminal").and_then(Value::as_object);
                add_preview_fields(
                    local_terminal,
                    &[
                        "defaultShellId",
                        "recentShellIds",
                        "defaultCwd",
                        "loadShellProfile",
                        "ohMyPoshEnabled",
                        "ohMyPoshTheme",
                    ],
                    None,
                    &mut field_values,
                );
                contains_env_vars = add_env_var_preview(
                    local_terminal.and_then(|object| object.get("customEnvVars")),
                    &mut field_values,
                );
            }
            _ => {}
        }

        if let Some(section) = build_section_preview(section_id, field_values, contains_env_vars) {
            sections.push(section);
        }
    }

    sections
}

fn build_app_settings_preview(
    app_settings_json: Option<&str>,
) -> (
    Option<String>,
    Vec<String>,
    HashMap<String, String>,
    Vec<AppSettingsSectionPreview>,
) {
    let Some(app_settings_json) = app_settings_json else {
        return (None, Vec::new(), HashMap::new(), Vec::new());
    };

    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(app_settings_json) else {
        return (None, Vec::new(), HashMap::new(), Vec::new());
    };

    if map.get("format").and_then(Value::as_str) == Some(OXIDE_APP_SETTINGS_ENVELOPE_FORMAT) {
        let Some(settings) = map.get("settings").and_then(Value::as_object) else {
            return (
                Some("sectioned".to_string()),
                Vec::new(),
                HashMap::new(),
                Vec::new(),
            );
        };

        let section_ids: Vec<String> = map
            .get("sectionIds")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let mut keys: Vec<String> = settings.keys().cloned().collect();
        keys.sort();

        return (
            Some("sectioned".to_string()),
            keys,
            HashMap::new(),
            build_sectioned_app_settings_sections(settings, &section_ids),
        );
    }

    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();

    let mut preview = HashMap::new();
    for key in &keys {
        if let Some(value) = map.get(key) {
            if let Ok(serialized) = serde_json::to_string(value) {
                preview.insert(key.clone(), serialized);
            }
        }
    }

    (
        Some("legacy".to_string()),
        keys.clone(),
        preview,
        vec![AppSettingsSectionPreview {
            id: "legacy".to_string(),
            field_keys: keys,
            field_values: HashMap::new(),
            contains_env_vars: false,
        }],
    )
}

/// Resolve name conflicts by appending a suffix like macOS does
/// "Server" -> "Server (Copy)" -> "Server (Copy 2)" -> ...
fn resolve_name_conflict(name: &str, existing_names: &HashSet<String>) -> String {
    if !existing_names.contains(name) {
        return name.to_string();
    }

    // Try "Name (Copy)" first
    let copy_name = format!("{} (Copy)", name);
    if !existing_names.contains(&copy_name) {
        return copy_name;
    }

    // Then try "Name (Copy 2)", "Name (Copy 3)", ...
    let mut n = 2;
    loop {
        let new_name = format!("{} (Copy {})", name, n);
        if !existing_names.contains(&new_name) {
            return new_name;
        }
        n += 1;
        // Safety limit to prevent infinite loop
        if n > 1000 {
            return format!("{} ({})", name, Uuid::new_v4());
        }
    }
}

/// Extract an embedded key to ~/.ssh/imported/ directory
/// Returns the new path where the key was saved
fn extract_embedded_key(original_path: &str, base64_data: &str) -> Result<String, String> {
    // Decode base64 data
    let key_data = BASE64
        .decode(base64_data)
        .map_err(|e| format!("Failed to decode embedded key: {}", e))?;

    // Create ~/.ssh/imported/ directory
    let home = dirs::home_dir().ok_or_else(|| "Cannot determine home directory".to_string())?;

    let imported_dir = home.join(".ssh").join("imported");
    fs::create_dir_all(&imported_dir)
        .map_err(|e| format!("Failed to create import directory: {}", e))?;

    // Extract filename from original path
    let original_filename = PathBuf::from(original_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("imported_key")
        .to_string();

    // Generate unique filename if it exists
    let mut target_path = imported_dir.join(&original_filename);
    let mut counter = 1;
    while target_path.exists() {
        let stem = PathBuf::from(&original_filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("key")
            .to_string();
        let ext = PathBuf::from(&original_filename)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e))
            .unwrap_or_default();
        target_path = imported_dir.join(format!("{}_{}{}", stem, counter, ext));
        counter += 1;
        if counter > 1000 {
            return Err("Too many files with same name".to_string());
        }
    }

    // Write key file
    fs::write(&target_path, &key_data).map_err(|e| format!("Failed to write key file: {}", e))?;

    // Set permissions to 600 (owner read/write only) for SSH key
    #[cfg(unix)]
    {
        let metadata = fs::metadata(&target_path)
            .map_err(|e| format!("Failed to read file metadata: {}", e))?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&target_path, permissions)
            .map_err(|e| format!("Failed to set file permissions: {}", e))?;
    }

    let final_path = target_path.to_string_lossy().to_string();
    info!("Extracted embedded key to: {}", final_path);

    Ok(final_path)
}

/// Pending keychain entry to be written
struct PendingKeychainEntry {
    id: String,
    value: Zeroizing<String>,
}

/// Pending connection with all resolved auth data
struct PendingConnection {
    connection: SavedConnection,
    keychain_entries: Vec<PendingKeychainEntry>,
    old_keychain_ids: Vec<String>,
    forward_ids_to_delete: Vec<String>,
    forwards_to_persist: Vec<PersistedForward>,
    existing_forwards_backup: Vec<PersistedForward>,
    imported_forward_count: usize,
}

struct PreparedImportConnection {
    name: String,
    group: Option<String>,
    host: String,
    port: u16,
    username: String,
    auth: SavedAuth,
    options: ConnectionOptions,
    color: Option<String>,
    tags: Vec<String>,
    proxy_chain: Vec<ProxyHopConfig>,
}

fn plan_import_action(
    name: &str,
    existing_connections_by_name: &HashMap<String, SavedConnection>,
    reserved_names: &mut HashSet<String>,
    replaced_names: &mut HashSet<String>,
    strategy: ImportConflictStrategy,
) -> PlannedImportAction {
    if let Some(existing) = existing_connections_by_name.get(name) {
        match strategy {
            ImportConflictStrategy::Skip => PlannedImportAction::Skip,
            ImportConflictStrategy::Replace if !replaced_names.contains(name) => {
                replaced_names.insert(name.to_string());
                PlannedImportAction::Replace(ReplaceTarget {
                    existing: existing.clone(),
                    old_keychain_ids: collect_connection_keychain_ids(existing),
                })
            }
            ImportConflictStrategy::Merge if !replaced_names.contains(name) => {
                replaced_names.insert(name.to_string());
                PlannedImportAction::Merge(ReplaceTarget {
                    existing: existing.clone(),
                    old_keychain_ids: collect_connection_keychain_ids(existing),
                })
            }
            ImportConflictStrategy::Rename
            | ImportConflictStrategy::Replace
            | ImportConflictStrategy::Merge => {
                let renamed = resolve_name_conflict(name, reserved_names);
                reserved_names.insert(renamed.clone());
                PlannedImportAction::Rename(renamed)
            }
        }
    } else if reserved_names.contains(name) {
        let renamed = resolve_name_conflict(name, reserved_names);
        reserved_names.insert(renamed.clone());
        PlannedImportAction::Rename(renamed)
    } else {
        reserved_names.insert(name.to_string());
        PlannedImportAction::Import
    }
}

fn normalize_imported_options(mut options: ConnectionOptions) -> ConnectionOptions {
    // jump_host stores a local saved-connection ID and cannot be restored from a portable export.
    options.jump_host = None;
    options
}

fn merge_optional_string(existing: Option<String>, imported: Option<String>) -> Option<String> {
    imported.or(existing)
}

fn merge_tags(existing: &[String], imported: &[String]) -> Vec<String> {
    let mut merged = existing.to_vec();
    for tag in imported {
        if !merged.contains(tag) {
            merged.push(tag.clone());
        }
    }
    merged
}

fn merge_saved_auth(existing: &SavedAuth, imported: SavedAuth) -> SavedAuth {
    match (existing, imported) {
        (
            SavedAuth::Password {
                keychain_id: Some(existing_keychain_id),
            },
            SavedAuth::Password { keychain_id: None },
        ) => SavedAuth::Password {
            keychain_id: Some(existing_keychain_id.clone()),
        },
        (
            SavedAuth::Key {
                key_path: existing_key_path,
                has_passphrase: true,
                passphrase_keychain_id: Some(existing_passphrase_keychain_id),
            },
            SavedAuth::Key {
                key_path,
                has_passphrase: false,
                passphrase_keychain_id: None,
            },
        ) if existing_key_path == &key_path => SavedAuth::Key {
            key_path,
            has_passphrase: true,
            passphrase_keychain_id: Some(existing_passphrase_keychain_id.clone()),
        },
        (
            SavedAuth::Certificate {
                key_path: existing_key_path,
                cert_path: existing_cert_path,
                has_passphrase: true,
                passphrase_keychain_id: Some(existing_passphrase_keychain_id),
            },
            SavedAuth::Certificate {
                key_path,
                cert_path,
                has_passphrase: false,
                passphrase_keychain_id: None,
            },
        ) if existing_key_path == &key_path && existing_cert_path == &cert_path => {
            SavedAuth::Certificate {
                key_path,
                cert_path,
                has_passphrase: true,
                passphrase_keychain_id: Some(existing_passphrase_keychain_id.clone()),
            }
        }
        (_, imported) => imported,
    }
}

fn merge_connection_options(
    existing: &ConnectionOptions,
    imported: ConnectionOptions,
    imported_has_proxy_chain: bool,
) -> ConnectionOptions {
    let mut merged = existing.clone();

    if imported.keep_alive_interval != 0 {
        merged.keep_alive_interval = imported.keep_alive_interval;
    }
    if imported.compression {
        merged.compression = true;
    }
    if let Some(term_type) = imported.term_type {
        merged.term_type = Some(term_type);
    }
    if imported.agent_forwarding {
        merged.agent_forwarding = true;
    }
    if imported_has_proxy_chain {
        merged.jump_host = None;
    }

    merged
}

fn build_saved_connection(
    id: String,
    created_at: chrono::DateTime<Utc>,
    last_used_at: Option<chrono::DateTime<Utc>>,
    imported: PreparedImportConnection,
) -> SavedConnection {
    SavedConnection {
        id,
        version: CONFIG_VERSION,
        name: imported.name,
        group: imported.group,
        host: imported.host,
        port: imported.port,
        username: imported.username,
        auth: imported.auth,
        options: imported.options,
        created_at,
        last_used_at,
        updated_at: Some(last_used_at.unwrap_or(created_at)),
        color: imported.color,
        tags: imported.tags,
        proxy_chain: imported.proxy_chain,
    }
}

fn merge_saved_connection(
    existing: &SavedConnection,
    imported: PreparedImportConnection,
) -> SavedConnection {
    let imported_has_proxy_chain = !imported.proxy_chain.is_empty();

    SavedConnection {
        id: existing.id.clone(),
        version: CONFIG_VERSION,
        name: existing.name.clone(),
        group: merge_optional_string(existing.group.clone(), imported.group),
        host: imported.host,
        port: imported.port,
        username: imported.username,
        auth: merge_saved_auth(&existing.auth, imported.auth),
        options: merge_connection_options(
            &existing.options,
            imported.options,
            imported_has_proxy_chain,
        ),
        created_at: existing.created_at,
        last_used_at: existing.last_used_at,
        updated_at: Some(Utc::now()),
        color: merge_optional_string(existing.color.clone(), imported.color),
        tags: merge_tags(&existing.tags, &imported.tags),
        proxy_chain: if imported_has_proxy_chain {
            imported.proxy_chain
        } else {
            existing.proxy_chain.clone()
        },
    }
}

fn stale_keychain_ids(old_keychain_ids: &[String], connection: &SavedConnection) -> Vec<String> {
    let retained_ids: HashSet<String> = collect_connection_keychain_ids(connection)
        .into_iter()
        .collect();
    old_keychain_ids
        .iter()
        .filter(|keychain_id| !retained_ids.contains(keychain_id.as_str()))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ForwardIdentity {
    forward_type: String,
    bind_address: String,
    bind_port: u16,
    target_host: String,
    target_port: u16,
}

impl ForwardIdentity {
    fn from_encrypted(forward: &EncryptedForward) -> Self {
        Self {
            forward_type: forward.forward_type.clone(),
            bind_address: forward.bind_address.clone(),
            bind_port: forward.bind_port,
            target_host: forward.target_host.clone(),
            target_port: forward.target_port,
        }
    }

    fn from_persisted(forward: &PersistedForward) -> Self {
        Self {
            forward_type: forward.forward_type.as_str().to_string(),
            bind_address: forward.rule.bind_address.clone(),
            bind_port: forward.rule.bind_port,
            target_host: forward.rule.target_host.clone(),
            target_port: forward.rule.target_port,
        }
    }
}

fn encrypted_forward_to_persisted(
    forward: EncryptedForward,
    owner_connection_id: &str,
    persisted_at: DateTime<Utc>,
) -> Result<PersistedForward, String> {
    let forward_type =
        crate::state::forwarding::ForwardType::try_from(forward.forward_type.as_str())?;
    let rule = ForwardRule {
        id: Uuid::new_v4().to_string(),
        forward_type: forward_type.to_runtime(),
        bind_address: forward.bind_address,
        bind_port: forward.bind_port,
        target_host: forward.target_host,
        target_port: forward.target_port,
        status: ForwardStatus::Stopped,
        description: forward.description,
    };

    Ok(PersistedForward {
        id: rule.id.clone(),
        session_id: String::new(),
        owner_connection_id: Some(owner_connection_id.to_string()),
        forward_type,
        rule,
        created_at: persisted_at,
        updated_at: Some(persisted_at),
        auto_start: forward.auto_start,
        version: 1,
    })
}

fn merge_owned_forwards(
    existing_forwards: Vec<PersistedForward>,
    imported_forwards: Vec<EncryptedForward>,
    owner_connection_id: &str,
) -> Result<Vec<PersistedForward>, String> {
    let merged_at = Utc::now();
    let mut merged_by_identity: HashMap<ForwardIdentity, PersistedForward> = existing_forwards
        .into_iter()
        .map(|forward| (ForwardIdentity::from_persisted(&forward), forward))
        .collect();

    for imported in imported_forwards {
        let identity = ForwardIdentity::from_encrypted(&imported);
        if let Some(existing) = merged_by_identity.get_mut(&identity) {
            existing.auto_start = imported.auto_start;
            existing.rule.description = imported.description;
            existing.owner_connection_id = Some(owner_connection_id.to_string());
            existing.updated_at = Some(merged_at);
        } else {
            let persisted =
                encrypted_forward_to_persisted(imported, owner_connection_id, merged_at)?;
            merged_by_identity.insert(identity, persisted);
        }
    }

    Ok(merged_by_identity.into_values().collect())
}

/// Validate .oxide file and extract metadata (without decryption)
#[tauri::command]
pub async fn validate_oxide_file(file_data: Vec<u8>) -> Result<OxideMetadata, String> {
    info!("Validating .oxide file ({} bytes)", file_data.len());

    let oxide_file = crate::oxide_file::OxideFile::from_bytes(&file_data)
        .map_err(|e| format!("Invalid .oxide file: {:?}", e))?;

    info!(
        "Valid .oxide file: {} connections",
        oxide_file.metadata.num_connections
    );

    Ok(oxide_file.metadata)
}

/// Preview what will happen when importing (decrypt and compute renames without saving)
#[tauri::command]
pub async fn preview_oxide_import(
    file_data: Vec<u8>,
    password: String,
    conflict_strategy: Option<String>,
    config_state: State<'_, Arc<ConfigState>>,
) -> Result<ImportPreview, String> {
    preview_oxide_import_inner(file_data, password, conflict_strategy, config_state, None).await
}

#[tauri::command]
pub async fn preview_oxide_import_with_progress(
    file_data: Vec<u8>,
    password: String,
    conflict_strategy: Option<String>,
    on_progress: Channel<OxideImportProgressEvent>,
    config_state: State<'_, Arc<ConfigState>>,
) -> Result<ImportPreview, String> {
    preview_oxide_import_inner(
        file_data,
        password,
        conflict_strategy,
        config_state,
        Some(on_progress),
    )
    .await
}

async fn preview_oxide_import_inner(
    file_data: Vec<u8>,
    password: String,
    conflict_strategy: Option<String>,
    config_state: State<'_, Arc<ConfigState>>,
    on_progress: Option<Channel<OxideImportProgressEvent>>,
) -> Result<ImportPreview, String> {
    info!(
        "Previewing import from .oxide file ({} bytes)",
        file_data.len()
    );

    // 1. Parse file
    let oxide_file = crate::oxide_file::OxideFile::from_bytes(&file_data)
        .map_err(|e| format!("Invalid .oxide file: {:?}", e))?;
    let mut current_step = 1usize;
    emit_import_progress(
        on_progress.as_ref(),
        "parsing_file",
        current_step,
        PREVIEW_IMPORT_TOTAL_STEPS,
    );

    // 2. Decrypt (password validation happens here)
    let payload = decrypt_oxide_file_with_progress(&oxide_file, &password, |stage| {
        current_step += 1;
        emit_import_progress(
            on_progress.as_ref(),
            stage,
            current_step,
            PREVIEW_IMPORT_TOTAL_STEPS,
        );
    })
    .map_err(map_import_error)?;
    let conflict_strategy = ImportConflictStrategy::parse(conflict_strategy)?;

    // 3. Build set of existing connection names for conflict detection
    current_step += 1;
    emit_import_progress(
        on_progress.as_ref(),
        "collecting_existing",
        current_step,
        PREVIEW_IMPORT_TOTAL_STEPS,
    );
    let config_snapshot = config_state.get_config_snapshot();
    let existing_connections_by_name: HashMap<String, SavedConnection> = config_snapshot
        .connections
        .iter()
        .cloned()
        .map(|connection| (connection.name.clone(), connection))
        .collect();
    let mut existing_names: HashSet<String> = config_snapshot
        .connections
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let mut replaced_names = HashSet::new();

    // 4. Compute what will happen for each connection
    let mut unchanged: Vec<String> = Vec::new();
    let mut will_rename: Vec<(String, String)> = Vec::new();
    let mut will_skip: Vec<String> = Vec::new();
    let mut will_replace: Vec<String> = Vec::new();
    let mut will_merge: Vec<String> = Vec::new();
    let mut has_embedded_keys = false;
    let mut records: Vec<ImportPreviewRecord> = Vec::new();
    let mut plugin_settings_by_plugin: HashMap<String, usize> = HashMap::new();
    let mut forward_details: Vec<ForwardDetail> = Vec::new();
    current_step += 1;
    emit_import_progress(
        on_progress.as_ref(),
        "building_preview",
        current_step,
        PREVIEW_IMPORT_TOTAL_STEPS,
    );
    let (app_settings_format, app_settings_keys, app_settings_preview, app_settings_sections) =
        build_app_settings_preview(payload.app_settings_json.as_deref());

    for setting in &payload.plugin_settings {
        if let Some(plugin_id) = parse_plugin_id_from_setting_storage_key(&setting.storage_key) {
            *plugin_settings_by_plugin.entry(plugin_id).or_insert(0) += 1;
        }
    }

    for conn in &payload.connections {
        for forward in &conn.forwards {
            forward_details.push(ForwardDetail {
                owner_connection_name: conn.name.clone(),
                direction: forward.forward_type.clone(),
                description: format_forward_preview_description(forward),
            });
        }
    }

    for conn in &payload.connections {
        // Check for embedded keys
        let mut record_has_embedded_keys = false;
        if let crate::oxide_file::EncryptedAuth::Key { embedded_key, .. } = &conn.auth {
            if embedded_key.is_some() {
                has_embedded_keys = true;
                record_has_embedded_keys = true;
            }
        }
        if let crate::oxide_file::EncryptedAuth::Certificate {
            embedded_key,
            embedded_cert,
            ..
        } = &conn.auth
        {
            if embedded_key.is_some() || embedded_cert.is_some() {
                has_embedded_keys = true;
                record_has_embedded_keys = true;
            }
        }

        let action = plan_import_action(
            &conn.name,
            &existing_connections_by_name,
            &mut existing_names,
            &mut replaced_names,
            conflict_strategy,
        );
        let reason_code = preview_reason_code(&action).to_string();

        match action {
            PlannedImportAction::Import => {
                unchanged.push(conn.name.clone());
                records.push(ImportPreviewRecord {
                    resource: "connection".to_string(),
                    name: conn.name.clone(),
                    action: "import".to_string(),
                    reason_code,
                    target_name: None,
                    target_connection_id: None,
                    forward_count: conn.forwards.len(),
                    has_embedded_keys: record_has_embedded_keys,
                });
            }
            PlannedImportAction::Rename(new_name) => {
                let target_name = new_name.clone();
                will_rename.push((conn.name.clone(), new_name));
                records.push(ImportPreviewRecord {
                    resource: "connection".to_string(),
                    name: conn.name.clone(),
                    action: "rename".to_string(),
                    reason_code,
                    target_name: Some(target_name),
                    target_connection_id: None,
                    forward_count: conn.forwards.len(),
                    has_embedded_keys: record_has_embedded_keys,
                });
            }
            PlannedImportAction::Skip => {
                will_skip.push(conn.name.clone());
                records.push(ImportPreviewRecord {
                    resource: "connection".to_string(),
                    name: conn.name.clone(),
                    action: "skip".to_string(),
                    reason_code,
                    target_name: None,
                    target_connection_id: None,
                    forward_count: conn.forwards.len(),
                    has_embedded_keys: record_has_embedded_keys,
                });
            }
            PlannedImportAction::Replace(target) => {
                will_replace.push(conn.name.clone());
                records.push(ImportPreviewRecord {
                    resource: "connection".to_string(),
                    name: conn.name.clone(),
                    action: "replace".to_string(),
                    reason_code,
                    target_name: Some(target.existing.name.clone()),
                    target_connection_id: Some(target.existing.id.clone()),
                    forward_count: conn.forwards.len(),
                    has_embedded_keys: record_has_embedded_keys,
                });
            }
            PlannedImportAction::Merge(target) => {
                will_merge.push(conn.name.clone());
                records.push(ImportPreviewRecord {
                    resource: "connection".to_string(),
                    name: conn.name.clone(),
                    action: "merge".to_string(),
                    reason_code,
                    target_name: Some(target.existing.name.clone()),
                    target_connection_id: Some(target.existing.id.clone()),
                    forward_count: conn.forwards.len(),
                    has_embedded_keys: record_has_embedded_keys,
                });
            }
        }
    }

    current_step += 1;
    emit_import_progress(
        on_progress.as_ref(),
        "analyzing_preview",
        current_step,
        PREVIEW_IMPORT_TOTAL_STEPS,
    );

    Ok(ImportPreview {
        total_connections: payload.connections.len(),
        unchanged,
        will_rename,
        will_skip,
        will_replace,
        will_merge,
        has_embedded_keys,
        total_forwards: payload.connections.iter().map(|c| c.forwards.len()).sum(),
        has_app_settings: payload.app_settings_json.is_some(),
        app_settings_format,
        app_settings_keys,
        app_settings_preview,
        app_settings_sections,
        plugin_settings_count: payload.plugin_settings.len(),
        plugin_settings_by_plugin,
        forward_details,
        records,
    })
}

/// Import connections from encrypted .oxide file
/// If `selected_names` is provided, only import connections whose names are in the list
#[tauri::command]
pub async fn import_from_oxide(
    file_data: Vec<u8>,
    password: String,
    selected_names: Option<Vec<String>>,
    conflict_strategy: Option<String>,
    import_forwards: Option<bool>,
    config_state: State<'_, Arc<ConfigState>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
) -> Result<ImportResultEnvelope, String> {
    import_from_oxide_inner(
        file_data,
        password,
        selected_names,
        conflict_strategy,
        import_forwards,
        config_state,
        forwarding_registry,
        None,
    )
    .await
}

#[tauri::command]
pub async fn import_from_oxide_with_progress(
    file_data: Vec<u8>,
    password: String,
    selected_names: Option<Vec<String>>,
    conflict_strategy: Option<String>,
    import_forwards: Option<bool>,
    on_progress: Channel<OxideImportProgressEvent>,
    config_state: State<'_, Arc<ConfigState>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
) -> Result<ImportResultEnvelope, String> {
    import_from_oxide_inner(
        file_data,
        password,
        selected_names,
        conflict_strategy,
        import_forwards,
        config_state,
        forwarding_registry,
        Some(on_progress),
    )
    .await
}

async fn import_from_oxide_inner(
    file_data: Vec<u8>,
    password: String,
    selected_names: Option<Vec<String>>,
    conflict_strategy: Option<String>,
    import_forwards: Option<bool>,
    config_state: State<'_, Arc<ConfigState>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
    on_progress: Option<Channel<OxideImportProgressEvent>>,
) -> Result<ImportResultEnvelope, String> {
    info!("Importing from .oxide file ({} bytes)", file_data.len());
    let conflict_strategy = ImportConflictStrategy::parse(conflict_strategy)?;
    let should_import_forwards = import_forwards.unwrap_or(true);

    // 1. Parse file
    let oxide_file = crate::oxide_file::OxideFile::from_bytes(&file_data)
        .map_err(|e| format!("Invalid .oxide file: {:?}", e))?;
    let mut current_step = 1usize;
    emit_import_progress(
        on_progress.as_ref(),
        "parsing_file",
        current_step,
        APPLY_IMPORT_TOTAL_STEPS,
    );

    // 2. Decrypt (password validation happens here)
    let payload = decrypt_oxide_file_with_progress(&oxide_file, &password, |stage| {
        current_step += 1;
        emit_import_progress(
            on_progress.as_ref(),
            stage,
            current_step,
            APPLY_IMPORT_TOTAL_STEPS,
        );
    })
    .map_err(map_import_error)?;

    info!(
        "Decryption successful, importing {} connections",
        payload.connections.len()
    );

    // Filter connections by selected_names if provided
    current_step += 1;
    emit_import_progress(
        on_progress.as_ref(),
        "filtering_selection",
        current_step,
        APPLY_IMPORT_TOTAL_STEPS,
    );
    let connections_to_import: Vec<_> = if let Some(ref names) = selected_names {
        let name_set: HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
        payload
            .connections
            .into_iter()
            .filter(|c| name_set.contains(c.name.as_str()))
            .collect()
    } else {
        payload.connections
    };
    let total_selected_forwards: usize = connections_to_import
        .iter()
        .map(|connection| connection.forwards.len())
        .sum();

    // 3. Phase 1: Build all connections in memory first (no keychain writes yet)
    //    This ensures we don't leave orphan keychain entries if something fails
    let mut pending_connections: Vec<PendingConnection> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut renames: Vec<(String, String)> = Vec::new();
    let mut skipped_count = 0;
    let mut merged_count = 0;
    let mut replaced_count = 0;

    // Build set of existing connection names for conflict detection
    current_step += 1;
    emit_import_progress(
        on_progress.as_ref(),
        "collecting_existing",
        current_step,
        APPLY_IMPORT_TOTAL_STEPS,
    );
    let config_snapshot = config_state.get_config_snapshot();
    let existing_connections_by_name: HashMap<String, SavedConnection> = config_snapshot
        .connections
        .iter()
        .cloned()
        .map(|connection| (connection.name.clone(), connection))
        .collect();
    let mut existing_names: HashSet<String> = config_snapshot
        .connections
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let mut replaced_names = HashSet::new();

    // Helper function to convert EncryptedAuth to SavedAuth WITHOUT writing to keychain
    // Returns (SavedAuth, Vec<PendingKeychainEntry>)
    fn prepare_auth(auth: EncryptedAuth, id: &str) -> (SavedAuth, Vec<PendingKeychainEntry>) {
        let mut entries = Vec::new();

        let saved_auth = match auth {
            EncryptedAuth::Password { password } => {
                if password.is_empty() {
                    // Password was not saved — preserve that intent
                    SavedAuth::Password { keychain_id: None }
                } else {
                    let keychain_id = format!("oxide_conn_{}", id);
                    entries.push(PendingKeychainEntry {
                        id: keychain_id.clone(),
                        value: password,
                    });
                    SavedAuth::Password {
                        keychain_id: Some(keychain_id),
                    }
                }
            }
            EncryptedAuth::Key {
                key_path,
                passphrase,
                embedded_key,
            } => {
                let passphrase_keychain_id = if let Some(pass) = passphrase {
                    let kc_id = format!("oxide_key_{}", id);
                    entries.push(PendingKeychainEntry {
                        id: kc_id.clone(),
                        value: pass,
                    });
                    Some(kc_id)
                } else {
                    None
                };

                // If key is embedded, extract it to ~/.ssh/imported/
                let final_key_path = if let Some(key_data) = embedded_key {
                    match extract_embedded_key(&key_path, &key_data) {
                        Ok(path) => path,
                        Err(_) => key_path, // Fall back to original path on error
                    }
                } else {
                    key_path
                };

                SavedAuth::Key {
                    key_path: final_key_path,
                    has_passphrase: passphrase_keychain_id.is_some(),
                    passphrase_keychain_id,
                }
            }
            EncryptedAuth::Certificate {
                key_path,
                cert_path,
                passphrase,
                embedded_key,
                embedded_cert,
            } => {
                let passphrase_keychain_id = if let Some(pass) = passphrase {
                    let kc_id = format!("oxide_cert_{}", id);
                    entries.push(PendingKeychainEntry {
                        id: kc_id.clone(),
                        value: pass,
                    });
                    Some(kc_id)
                } else {
                    None
                };

                // Extract embedded key and cert if present
                let final_key_path = if let Some(key_data) = embedded_key {
                    match extract_embedded_key(&key_path, &key_data) {
                        Ok(path) => path,
                        Err(_) => key_path,
                    }
                } else {
                    key_path
                };

                let final_cert_path = if let Some(cert_data) = embedded_cert {
                    match extract_embedded_key(&cert_path, &cert_data) {
                        Ok(path) => path,
                        Err(_) => cert_path,
                    }
                } else {
                    cert_path
                };

                SavedAuth::Certificate {
                    key_path: final_key_path,
                    cert_path: final_cert_path,
                    has_passphrase: passphrase_keychain_id.is_some(),
                    passphrase_keychain_id,
                }
            }
            EncryptedAuth::Agent => SavedAuth::Agent,
        };

        (saved_auth, entries)
    }

    fn prepare_proxy_chain(
        proxy_chain: Vec<EncryptedProxyHop>,
        base_id: &str,
    ) -> (Vec<ProxyHopConfig>, Vec<PendingKeychainEntry>) {
        let mut hops = Vec::new();
        let mut all_entries = Vec::new();

        for (hop_index, enc_hop) in proxy_chain.into_iter().enumerate() {
            let hop_id = format!("{}_hop{}", base_id, hop_index);
            let (hop_auth, entries) = prepare_auth(enc_hop.auth, &hop_id);
            all_entries.extend(entries);

            hops.push(ProxyHopConfig {
                host: enc_hop.host,
                port: enc_hop.port,
                username: enc_hop.username,
                auth: hop_auth,
                agent_forwarding: false,
            });
        }

        (hops, all_entries)
    }

    current_step += 1;
    emit_import_progress(
        on_progress.as_ref(),
        "preparing_connections",
        current_step,
        APPLY_IMPORT_TOTAL_STEPS,
    );

    for enc_conn in connections_to_import {
        let original_name = enc_conn.name.clone();
        let imported_forwards = enc_conn.forwards.clone();
        let imported_forward_count = if should_import_forwards {
            imported_forwards.len()
        } else {
            0
        };

        let action = plan_import_action(
            &original_name,
            &existing_connections_by_name,
            &mut existing_names,
            &mut replaced_names,
            conflict_strategy,
        );

        if matches!(action, PlannedImportAction::Skip) {
            skipped_count += 1;
            continue;
        }

        let credential_base_id = match &action {
            PlannedImportAction::Import | PlannedImportAction::Rename(_) => {
                Uuid::new_v4().to_string()
            }
            PlannedImportAction::Replace(target) | PlannedImportAction::Merge(target) => {
                format!("{}_{}", target.existing.id, Uuid::new_v4())
            }
            PlannedImportAction::Skip => unreachable!(),
        };

        // Prepare main connection auth
        let (auth, mut keychain_entries) = prepare_auth(enc_conn.auth, &credential_base_id);

        // Prepare proxy_chain auth
        let (proxy_chain, hop_entries) =
            prepare_proxy_chain(enc_conn.proxy_chain, &credential_base_id);
        keychain_entries.extend(hop_entries);

        let mut imported_connection = PreparedImportConnection {
            name: original_name.clone(),
            group: enc_conn.group,
            host: enc_conn.host,
            port: enc_conn.port,
            username: enc_conn.username,
            auth,
            options: normalize_imported_options(enc_conn.options),
            color: enc_conn.color,
            tags: enc_conn.tags,
            proxy_chain,
        };

        let (
            saved_conn,
            old_keychain_ids,
            forward_ids_to_delete,
            forwards_to_persist,
            existing_forwards_backup,
            imported_forward_count,
        ) = match action {
            PlannedImportAction::Import => {
                let imported_at = Utc::now();
                let saved_conn = build_saved_connection(
                    credential_base_id.clone(),
                    imported_at,
                    None,
                    imported_connection,
                );
                let forwards_to_persist = if should_import_forwards {
                    imported_forwards
                        .into_iter()
                        .map(|forward| {
                            encrypted_forward_to_persisted(forward, &saved_conn.id, imported_at)
                        })
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    Vec::new()
                };
                (
                    saved_conn,
                    Vec::new(),
                    Vec::new(),
                    forwards_to_persist,
                    Vec::new(),
                    imported_forward_count,
                )
            }
            PlannedImportAction::Rename(new_name) => {
                info!("Name conflict: '{}' -> '{}'", original_name, new_name);
                renames.push((original_name.clone(), new_name.clone()));
                imported_connection.name = new_name;
                {
                    let imported_at = Utc::now();
                    let saved_conn = build_saved_connection(
                        credential_base_id.clone(),
                        imported_at,
                        None,
                        imported_connection,
                    );
                    let forwards_to_persist = if should_import_forwards {
                        imported_forwards
                            .into_iter()
                            .map(|forward| {
                                encrypted_forward_to_persisted(forward, &saved_conn.id, imported_at)
                            })
                            .collect::<Result<Vec<_>, _>>()?
                    } else {
                        Vec::new()
                    };
                    (
                        saved_conn,
                        Vec::new(),
                        Vec::new(),
                        forwards_to_persist,
                        Vec::new(),
                        imported_forward_count,
                    )
                }
            }
            PlannedImportAction::Replace(target) => {
                replaced_count += 1;
                imported_connection.name = target.existing.name.clone();
                {
                    let imported_at = Utc::now();
                    let mut saved_conn = build_saved_connection(
                        target.existing.id.clone(),
                        target.existing.created_at,
                        target.existing.last_used_at,
                        imported_connection,
                    );
                    saved_conn.updated_at = Some(imported_at);
                    let (forward_ids_to_delete, forwards_to_persist, existing_forwards_backup) = if should_import_forwards {
                        let existing_forwards = forwarding_registry
                            .load_owned_forwards(&target.existing.id)
                            .await?;
                        let forward_ids_to_delete = existing_forwards
                            .iter()
                            .map(|forward| forward.id.clone())
                            .collect();
                        let forwards_to_persist = imported_forwards
                            .into_iter()
                            .map(|forward| {
                                encrypted_forward_to_persisted(forward, &saved_conn.id, imported_at)
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        (
                            forward_ids_to_delete,
                            forwards_to_persist,
                            existing_forwards,
                        )
                    } else {
                        (Vec::new(), Vec::new(), Vec::new())
                    };
                    (
                        saved_conn,
                        target.old_keychain_ids,
                        forward_ids_to_delete,
                        forwards_to_persist,
                        existing_forwards_backup,
                        imported_forward_count,
                    )
                }
            }
            PlannedImportAction::Merge(target) => {
                merged_count += 1;
                {
                    let saved_conn = merge_saved_connection(&target.existing, imported_connection);
                    let (forward_ids_to_delete, forwards_to_persist, existing_forwards_backup) =
                        if should_import_forwards {
                            let existing_forwards = forwarding_registry
                                .load_owned_forwards(&target.existing.id)
                                .await?;
                            let forward_ids_to_delete = existing_forwards
                                .iter()
                                .map(|forward| forward.id.clone())
                                .collect();
                            let forwards_to_persist = merge_owned_forwards(
                                existing_forwards.clone(),
                                imported_forwards,
                                &saved_conn.id,
                            )?;
                            (
                                forward_ids_to_delete,
                                forwards_to_persist,
                                existing_forwards,
                            )
                        } else {
                            (Vec::new(), Vec::new(), Vec::new())
                        };
                    (
                        saved_conn,
                        target.old_keychain_ids,
                        forward_ids_to_delete,
                        forwards_to_persist,
                        existing_forwards_backup,
                        imported_forward_count,
                    )
                }
            }
            PlannedImportAction::Skip => unreachable!(),
        };

        pending_connections.push(PendingConnection {
            connection: saved_conn,
            keychain_entries,
            old_keychain_ids,
            forward_ids_to_delete,
            forwards_to_persist,
            existing_forwards_backup,
            imported_forward_count,
        });
    }

    // 4. Phase 2: All connections validated - now write keychain entries and config atomically
    let mut imported_count = 0;
    let mut imported_forward_count = 0;

    current_step += 1;
    emit_import_progress(
        on_progress.as_ref(),
        "applying_connections",
        current_step,
        APPLY_IMPORT_TOTAL_STEPS,
    );

    for pending in pending_connections {
        let PendingConnection {
            connection,
            keychain_entries,
            old_keychain_ids,
            forward_ids_to_delete,
            forwards_to_persist,
            existing_forwards_backup,
            imported_forward_count: intended_forward_count,
        } = pending;
        let stale_old_keychain_ids = stale_keychain_ids(&old_keychain_ids, &connection);
        let connection_name = connection.name.clone();
        let connection_group = connection.group.clone();

        // Write all keychain entries for this connection
        let mut keychain_ok = true;
        for entry in &keychain_entries {
            if let Err(e) = config_state.set_keychain_value(&entry.id, &entry.value) {
                errors.push(format!(
                    "Failed to store credentials for {}: {}",
                    &connection_name, e
                ));
                keychain_ok = false;
                break;
            }
        }

        if !keychain_ok {
            // Rollback: try to delete already-written keychain entries for this connection
            for entry in &keychain_entries {
                let _ = config_state.delete_keychain_value(&entry.id);
            }
            continue;
        }

        // Add to config
        if let Err(e) = config_state.update_config(|config| {
            if let Some(group) = connection_group.clone() {
                if !config.groups.contains(&group) {
                    config.groups.push(group);
                }
            }
            config.add_connection(connection);
        }) {
            errors.push(format!("Failed to save connection: {}", e));
            // Rollback keychain entries for this connection
            for entry in &keychain_entries {
                let _ = config_state.delete_keychain_value(&entry.id);
            }
            continue;
        }

        imported_count += 1;

        let backup_forward_ids: std::collections::HashSet<String> = existing_forwards_backup
            .iter()
            .map(|forward| forward.id.clone())
            .collect();
        let mut deleted_forward_ids = Vec::new();
        let mut forward_cleanup_ok = true;
        for forward_id in &forward_ids_to_delete {
            if let Err(e) = forwarding_registry
                .delete_persisted_forward(forward_id.clone())
                .await
            {
                errors.push(format!(
                    "Failed to replace saved forwards for {}: {}",
                    &connection_name, e
                ));
                forward_cleanup_ok = false;
                break;
            }

            deleted_forward_ids.push(forward_id.clone());
        }

        if !forward_cleanup_ok {
            for backup_forward in &existing_forwards_backup {
                let _ = forwarding_registry
                    .persist_forward(backup_forward.clone())
                    .await;
            }
            continue;
        }

        let mut persisted_forward_successes = 0;
        let mut persisted_forward_ids = Vec::new();
        let mut forward_persist_ok = true;
        for forward in forwards_to_persist {
            let forward_id = forward.id.clone();
            if let Err(e) = forwarding_registry.persist_forward(forward).await {
                errors.push(format!(
                    "Failed to save imported forward for {}: {}",
                    &connection_name, e
                ));
                forward_persist_ok = false;
                break;
            } else {
                persisted_forward_successes += 1;
                persisted_forward_ids.push(forward_id);
            }
        }

        if !forward_persist_ok {
            for persisted_forward_id in &persisted_forward_ids {
                if !backup_forward_ids.contains(persisted_forward_id) {
                    let _ = forwarding_registry
                        .delete_persisted_forward(persisted_forward_id.clone())
                        .await;
                }
            }
            for backup_forward in &existing_forwards_backup {
                let _ = forwarding_registry
                    .persist_forward(backup_forward.clone())
                    .await;
            }
            continue;
        }

        for old_keychain_id in &stale_old_keychain_ids {
            let _ = config_state.delete_keychain_value(old_keychain_id);
        }
        imported_forward_count += persisted_forward_successes.min(intended_forward_count);
    }

    // 5. Persist to storage
    current_step += 1;
    emit_import_progress(
        on_progress.as_ref(),
        "saving_config",
        current_step,
        APPLY_IMPORT_TOTAL_STEPS,
    );
    if imported_count > 0 {
        config_state
            .save_config()
            .await
            .map_err(|e| format!("Failed to save config: {}", e))?;
    }

    info!("Successfully imported {} connections", imported_count);

    Ok(ImportResultEnvelope {
        imported: imported_count,
        skipped: skipped_count,
        merged: merged_count,
        replaced: replaced_count,
        renamed: renames.len(),
        errors,
        renames,
        imported_forwards: imported_forward_count,
        skipped_forwards: if should_import_forwards {
            0
        } else {
            total_selected_forwards
        },
        app_settings_json: payload.app_settings_json,
        plugin_settings: payload.plugin_settings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConnectionOptions, ProxyHopConfig};

    #[test]
    fn parse_plugin_id_from_storage_key_extracts_plugin_id() {
        assert_eq!(
            parse_plugin_id_from_setting_storage_key("oxide-plugin-com.example.demo-setting-theme"),
            Some("com.example.demo".to_string())
        );
        assert_eq!(
            parse_plugin_id_from_setting_storage_key("oxide-plugin--setting-theme"),
            None
        );
    }

    #[test]
    fn format_forward_preview_description_prefers_summary_with_label() {
        let forward = EncryptedForward {
            forward_type: "local".to_string(),
            bind_address: "127.0.0.1".to_string(),
            bind_port: 8080,
            target_host: "remote".to_string(),
            target_port: 80,
            description: Some("web".to_string()),
            auto_start: true,
        };

        assert_eq!(
            format_forward_preview_description(&forward),
            "web (L:8080 -> remote:80)"
        );
    }

    fn build_existing_connection(name: &str, id: &str) -> SavedConnection {
        SavedConnection {
            id: id.to_string(),
            version: CONFIG_VERSION,
            name: name.to_string(),
            group: None,
            host: "example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SavedAuth::Password {
                keychain_id: Some(format!("oxide_conn_{}", id)),
            },
            options: ConnectionOptions {
                keep_alive_interval: 30,
                compression: true,
                jump_host: Some("legacy-jump-id".to_string()),
                term_type: Some("xterm-256color".to_string()),
                agent_forwarding: true,
            },
            created_at: Utc::now(),
            last_used_at: None,
            updated_at: Some(Utc::now()),
            color: Some("#112233".to_string()),
            tags: vec!["prod".to_string(), "linux".to_string()],
            proxy_chain: vec![ProxyHopConfig {
                host: "jump.example.com".to_string(),
                port: 22,
                username: "jump".to_string(),
                auth: SavedAuth::Key {
                    key_path: "/tmp/id_ed25519".to_string(),
                    has_passphrase: true,
                    passphrase_keychain_id: Some(format!("oxide_key_{}_hop0", id)),
                },
                agent_forwarding: false,
            }],
        }
    }

    #[test]
    fn plan_import_action_merges_first_match_then_renames_duplicate_imports() {
        let existing = build_existing_connection("prod", "conn-1");
        let existing_by_name = HashMap::from([(existing.name.clone(), existing)]);
        let mut reserved_names = HashSet::from(["prod".to_string()]);
        let mut replaced_names = HashSet::new();

        let first = plan_import_action(
            "prod",
            &existing_by_name,
            &mut reserved_names,
            &mut replaced_names,
            ImportConflictStrategy::Merge,
        );
        let second = plan_import_action(
            "prod",
            &existing_by_name,
            &mut reserved_names,
            &mut replaced_names,
            ImportConflictStrategy::Merge,
        );

        assert!(matches!(first, PlannedImportAction::Merge(_)));
        match &second {
            PlannedImportAction::Rename(name) => assert_eq!(name, "prod (Copy)"),
            _ => panic!("expected second duplicate to be renamed"),
        }

        assert_eq!(preview_reason_code(&first), "merge-existing");
        assert_eq!(preview_reason_code(&second), "name-conflict");
    }

    #[test]
    fn merge_saved_auth_preserves_existing_secret_when_imported_secret_is_missing() {
        let existing = SavedAuth::Key {
            key_path: "/tmp/id_ed25519".to_string(),
            has_passphrase: true,
            passphrase_keychain_id: Some("kc-existing".to_string()),
        };

        let merged = merge_saved_auth(
            &existing,
            SavedAuth::Key {
                key_path: "/tmp/id_ed25519".to_string(),
                has_passphrase: false,
                passphrase_keychain_id: None,
            },
        );

        assert_eq!(merged, existing);
    }

    #[test]
    fn merge_saved_connection_combines_local_metadata_without_destroying_defaults() {
        let existing = build_existing_connection("prod", "conn-1");
        let imported = PreparedImportConnection {
            name: "prod".to_string(),
            group: None,
            host: "prod-new.example.com".to_string(),
            port: 2200,
            username: "deploy".to_string(),
            auth: SavedAuth::Password { keychain_id: None },
            options: normalize_imported_options(ConnectionOptions {
                keep_alive_interval: 0,
                compression: false,
                jump_host: Some("remote-jump-id".to_string()),
                term_type: None,
                agent_forwarding: false,
            }),
            color: None,
            tags: vec!["ops".to_string(), "linux".to_string()],
            proxy_chain: Vec::new(),
        };

        let merged = merge_saved_connection(&existing, imported);

        assert_eq!(merged.id, existing.id);
        assert_eq!(merged.name, existing.name);
        assert_eq!(merged.host, "prod-new.example.com");
        assert_eq!(merged.port, 2200);
        assert_eq!(merged.username, "deploy");
        assert_eq!(merged.group, existing.group);
        assert_eq!(merged.color, existing.color);
        assert_eq!(merged.tags, vec!["prod", "linux", "ops"]);
        assert_eq!(merged.proxy_chain.len(), existing.proxy_chain.len());
        assert_eq!(merged.options.keep_alive_interval, 30);
        assert!(merged.options.compression);
        assert_eq!(merged.options.jump_host, existing.options.jump_host);
        assert_eq!(merged.options.term_type, existing.options.term_type);
        assert!(merged.options.agent_forwarding);
        assert_eq!(
            merged.auth,
            SavedAuth::Password {
                keychain_id: Some("oxide_conn_conn-1".to_string()),
            }
        );
    }

    #[test]
    fn stale_keychain_ids_keeps_credentials_retained_by_merge() {
        let merged_connection = SavedConnection {
            id: "conn-1".to_string(),
            version: CONFIG_VERSION,
            name: "prod".to_string(),
            group: None,
            host: "example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SavedAuth::Password {
                keychain_id: Some("oxide_conn_conn-1".to_string()),
            },
            options: ConnectionOptions::default(),
            created_at: Utc::now(),
            last_used_at: None,
            updated_at: Some(Utc::now()),
            color: None,
            tags: Vec::new(),
            proxy_chain: vec![ProxyHopConfig {
                host: "jump.example.com".to_string(),
                port: 22,
                username: "jump".to_string(),
                auth: SavedAuth::Key {
                    key_path: "/tmp/id_ed25519".to_string(),
                    has_passphrase: false,
                    passphrase_keychain_id: None,
                },
                agent_forwarding: false,
            }],
        };

        let stale = stale_keychain_ids(
            &[
                "oxide_conn_conn-1".to_string(),
                "oxide_key_conn-1_hop0".to_string(),
                "oxide_cert_legacy".to_string(),
            ],
            &merged_connection,
        );

        assert_eq!(
            stale,
            vec![
                "oxide_key_conn-1_hop0".to_string(),
                "oxide_cert_legacy".to_string()
            ]
        );
    }

    #[test]
    fn plan_import_action_skips_existing_name_when_strategy_is_skip() {
        let existing = build_existing_connection("prod", "conn-1");
        let existing_by_name = HashMap::from([(existing.name.clone(), existing)]);
        let mut reserved_names = HashSet::from(["prod".to_string()]);
        let mut replaced_names = HashSet::new();

        let action = plan_import_action(
            "prod",
            &existing_by_name,
            &mut reserved_names,
            &mut replaced_names,
            ImportConflictStrategy::Skip,
        );

        assert!(matches!(action, PlannedImportAction::Skip));
        assert_eq!(preview_reason_code(&action), "name-conflict-skipped");
    }

    #[test]
    fn plan_import_action_replaces_first_match_then_renames_duplicate_imports() {
        let existing = build_existing_connection("prod", "conn-1");
        let existing_by_name = HashMap::from([(existing.name.clone(), existing)]);
        let mut reserved_names = HashSet::from(["prod".to_string()]);
        let mut replaced_names = HashSet::new();

        let first = plan_import_action(
            "prod",
            &existing_by_name,
            &mut reserved_names,
            &mut replaced_names,
            ImportConflictStrategy::Replace,
        );
        let second = plan_import_action(
            "prod",
            &existing_by_name,
            &mut reserved_names,
            &mut replaced_names,
            ImportConflictStrategy::Replace,
        );

        match &first {
            PlannedImportAction::Replace(target) => {
                assert_eq!(target.existing.id, "conn-1");
                assert_eq!(target.old_keychain_ids.len(), 2);
            }
            _ => panic!("expected first conflict to replace existing connection"),
        }

        match &second {
            PlannedImportAction::Rename(name) => assert_eq!(name, "prod (Copy)"),
            _ => panic!("expected second duplicate to be renamed"),
        }

        assert_eq!(preview_reason_code(&first), "replace-existing");
        assert_eq!(preview_reason_code(&second), "name-conflict");
    }

    #[test]
    fn preview_reason_code_marks_new_connection_imports() {
        assert_eq!(
            preview_reason_code(&PlannedImportAction::Import),
            "new-connection"
        );
    }

    #[test]
    fn encrypted_forward_to_persisted_creates_detached_owner_bound_rule() {
        let persisted = encrypted_forward_to_persisted(
            EncryptedForward {
                forward_type: "local".to_string(),
                bind_address: "127.0.0.1".to_string(),
                bind_port: 8080,
                target_host: "localhost".to_string(),
                target_port: 3000,
                description: Some("web".to_string()),
                auto_start: true,
            },
            "conn-1",
            Utc::now(),
        )
        .unwrap();

        assert_eq!(persisted.owner_connection_id.as_deref(), Some("conn-1"));
        assert_eq!(persisted.session_id, "");
        assert!(matches!(persisted.rule.status, ForwardStatus::Stopped));
        assert_eq!(persisted.rule.description.as_deref(), Some("web"));
        assert!(persisted.auto_start);
    }

    #[test]
    fn merge_owned_forwards_updates_matching_rules_and_keeps_unique_entries() {
        let existing = PersistedForward {
            id: "forward-1".to_string(),
            session_id: "session-1".to_string(),
            owner_connection_id: Some("conn-1".to_string()),
            forward_type: crate::state::forwarding::ForwardType::Local,
            rule: ForwardRule {
                id: "forward-1".to_string(),
                forward_type: crate::forwarding::ForwardType::Local,
                bind_address: "127.0.0.1".to_string(),
                bind_port: 8080,
                target_host: "localhost".to_string(),
                target_port: 3000,
                status: ForwardStatus::Active,
                description: Some("old".to_string()),
            },
            created_at: Utc::now(),
            updated_at: Some(Utc::now()),
            auto_start: false,
            version: 1,
        };

        let merged = merge_owned_forwards(
            vec![existing],
            vec![
                EncryptedForward {
                    forward_type: "local".to_string(),
                    bind_address: "127.0.0.1".to_string(),
                    bind_port: 8080,
                    target_host: "localhost".to_string(),
                    target_port: 3000,
                    description: Some("new".to_string()),
                    auto_start: true,
                },
                EncryptedForward {
                    forward_type: "remote".to_string(),
                    bind_address: "0.0.0.0".to_string(),
                    bind_port: 9000,
                    target_host: "localhost".to_string(),
                    target_port: 9000,
                    description: None,
                    auto_start: false,
                },
            ],
            "conn-1",
        )
        .unwrap();

        assert_eq!(merged.len(), 2);

        let updated = merged
            .iter()
            .find(|forward| forward.rule.bind_port == 8080)
            .unwrap();
        assert_eq!(updated.id, "forward-1");
        assert_eq!(updated.session_id, "session-1");
        assert_eq!(updated.rule.description.as_deref(), Some("new"));
        assert!(updated.auto_start);

        let added = merged
            .iter()
            .find(|forward| forward.rule.bind_port == 9000)
            .unwrap();
        assert_eq!(added.owner_connection_id.as_deref(), Some("conn-1"));
        assert_eq!(added.session_id, "");
        assert!(matches!(added.rule.status, ForwardStatus::Stopped));
    }
}
