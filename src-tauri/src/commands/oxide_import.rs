// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Tauri commands for .oxide file import

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tracing::info;
use uuid::Uuid;

use crate::commands::config::{ConfigState, collect_connection_keychain_ids};
use crate::commands::forwarding::ForwardingRegistry;
use crate::config::types::{
    CONFIG_VERSION, ConnectionOptions, ProxyHopConfig, SavedAuth, SavedConnection,
};
use crate::forwarding::{ForwardRule, ForwardStatus};
use crate::oxide_file::{
    EncryptedAuth, EncryptedForward, EncryptedProxyHop, OxideMetadata, decrypt_oxide_file,
};
use crate::state::PersistedForward;
use zeroize::Zeroizing;

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
        created_at: Utc::now(),
        auto_start: forward.auto_start,
        version: 1,
    })
}

fn merge_owned_forwards(
    existing_forwards: Vec<PersistedForward>,
    imported_forwards: Vec<EncryptedForward>,
    owner_connection_id: &str,
) -> Result<Vec<PersistedForward>, String> {
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
        } else {
            let persisted = encrypted_forward_to_persisted(imported, owner_connection_id)?;
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
    info!(
        "Previewing import from .oxide file ({} bytes)",
        file_data.len()
    );

    // 1. Parse file
    let oxide_file = crate::oxide_file::OxideFile::from_bytes(&file_data)
        .map_err(|e| format!("Invalid .oxide file: {:?}", e))?;

    // 2. Decrypt (password validation happens here)
    let payload = decrypt_oxide_file(&oxide_file, &password).map_err(|e| match e {
        crate::oxide_file::OxideFileError::DecryptionFailed => "密码错误或文件已损坏".to_string(),
        crate::oxide_file::OxideFileError::ChecksumMismatch => {
            "文件校验失败，数据可能被篡改".to_string()
        }
        _ => format!("解密失败: {:?}", e),
    })?;
    let conflict_strategy = ImportConflictStrategy::parse(conflict_strategy)?;

    // 3. Build set of existing connection names for conflict detection
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

    for conn in &payload.connections {
        // Check for embedded keys
        if let crate::oxide_file::EncryptedAuth::Key { embedded_key, .. } = &conn.auth {
            if embedded_key.is_some() {
                has_embedded_keys = true;
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
            }
        }

        match plan_import_action(
            &conn.name,
            &existing_connections_by_name,
            &mut existing_names,
            &mut replaced_names,
            conflict_strategy,
        ) {
            PlannedImportAction::Import => unchanged.push(conn.name.clone()),
            PlannedImportAction::Rename(new_name) => {
                will_rename.push((conn.name.clone(), new_name));
            }
            PlannedImportAction::Skip => will_skip.push(conn.name.clone()),
            PlannedImportAction::Replace(_) => will_replace.push(conn.name.clone()),
            PlannedImportAction::Merge(_) => will_merge.push(conn.name.clone()),
        }
    }

    Ok(ImportPreview {
        total_connections: payload.connections.len(),
        unchanged,
        will_rename,
        will_skip,
        will_replace,
        will_merge,
        has_embedded_keys,
        total_forwards: payload.connections.iter().map(|c| c.forwards.len()).sum(),
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
    config_state: State<'_, Arc<ConfigState>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
) -> Result<ImportResult, String> {
    info!("Importing from .oxide file ({} bytes)", file_data.len());
    let conflict_strategy = ImportConflictStrategy::parse(conflict_strategy)?;

    // 1. Parse file
    let oxide_file = crate::oxide_file::OxideFile::from_bytes(&file_data)
        .map_err(|e| format!("Invalid .oxide file: {:?}", e))?;

    // 2. Decrypt (password validation happens here)
    let payload = decrypt_oxide_file(&oxide_file, &password).map_err(|e| match e {
        crate::oxide_file::OxideFileError::DecryptionFailed => "密码错误或文件已损坏".to_string(),
        crate::oxide_file::OxideFileError::ChecksumMismatch => {
            "文件校验失败，数据可能被篡改".to_string()
        }
        _ => format!("解密失败: {:?}", e),
    })?;

    info!(
        "Decryption successful, importing {} connections",
        payload.connections.len()
    );

    // Filter connections by selected_names if provided
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

    // 3. Phase 1: Build all connections in memory first (no keychain writes yet)
    //    This ensures we don't leave orphan keychain entries if something fails
    let mut pending_connections: Vec<PendingConnection> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut renames: Vec<(String, String)> = Vec::new();
    let mut skipped_count = 0;
    let mut merged_count = 0;
    let mut replaced_count = 0;

    // Build set of existing connection names for conflict detection
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

    for enc_conn in connections_to_import {
        let original_name = enc_conn.name.clone();
        let imported_forwards = enc_conn.forwards.clone();

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

        let (saved_conn, old_keychain_ids, forward_ids_to_delete, forwards_to_persist) =
            match action {
                PlannedImportAction::Import => {
                    let saved_conn = build_saved_connection(
                        credential_base_id.clone(),
                        Utc::now(),
                        None,
                        imported_connection,
                    );
                    let forwards_to_persist = imported_forwards
                        .into_iter()
                        .map(|forward| encrypted_forward_to_persisted(forward, &saved_conn.id))
                        .collect::<Result<Vec<_>, _>>()?;
                    (saved_conn, Vec::new(), Vec::new(), forwards_to_persist)
                }
                PlannedImportAction::Rename(new_name) => {
                    info!("Name conflict: '{}' -> '{}'", original_name, new_name);
                    renames.push((original_name.clone(), new_name.clone()));
                    imported_connection.name = new_name;
                    {
                        let saved_conn = build_saved_connection(
                            credential_base_id.clone(),
                            Utc::now(),
                            None,
                            imported_connection,
                        );
                        let forwards_to_persist = imported_forwards
                            .into_iter()
                            .map(|forward| encrypted_forward_to_persisted(forward, &saved_conn.id))
                            .collect::<Result<Vec<_>, _>>()?;
                        (saved_conn, Vec::new(), Vec::new(), forwards_to_persist)
                    }
                }
                PlannedImportAction::Replace(target) => {
                    replaced_count += 1;
                    imported_connection.name = target.existing.name.clone();
                    {
                        let existing_forwards = forwarding_registry
                            .load_owned_forwards(&target.existing.id)
                            .await?;
                        let forward_ids_to_delete = existing_forwards
                            .iter()
                            .map(|forward| forward.id.clone())
                            .collect();
                        let saved_conn = build_saved_connection(
                            target.existing.id.clone(),
                            target.existing.created_at,
                            target.existing.last_used_at,
                            imported_connection,
                        );
                        let forwards_to_persist = imported_forwards
                            .into_iter()
                            .map(|forward| encrypted_forward_to_persisted(forward, &saved_conn.id))
                            .collect::<Result<Vec<_>, _>>()?;
                        (
                            saved_conn,
                            target.old_keychain_ids,
                            forward_ids_to_delete,
                            forwards_to_persist,
                        )
                    }
                }
                PlannedImportAction::Merge(target) => {
                    merged_count += 1;
                    {
                        let existing_forwards = forwarding_registry
                            .load_owned_forwards(&target.existing.id)
                            .await?;
                        let forward_ids_to_delete = existing_forwards
                            .iter()
                            .map(|forward| forward.id.clone())
                            .collect();
                        let saved_conn =
                            merge_saved_connection(&target.existing, imported_connection);
                        let forwards_to_persist = merge_owned_forwards(
                            existing_forwards,
                            imported_forwards,
                            &saved_conn.id,
                        )?;
                        (
                            saved_conn,
                            target.old_keychain_ids,
                            forward_ids_to_delete,
                            forwards_to_persist,
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
        });
    }

    // 4. Phase 2: All connections validated - now write keychain entries and config atomically
    let mut imported_count = 0;

    for pending in pending_connections {
        let PendingConnection {
            connection,
            keychain_entries,
            old_keychain_ids,
            forward_ids_to_delete,
            forwards_to_persist,
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
        }

        if !forward_cleanup_ok {
            continue;
        }

        for forward in forwards_to_persist {
            if let Err(e) = forwarding_registry.persist_forward(forward).await {
                errors.push(format!(
                    "Failed to save imported forward for {}: {}",
                    &connection_name, e
                ));
            }
        }

        for old_keychain_id in &stale_old_keychain_ids {
            let _ = config_state.delete_keychain_value(old_keychain_id);
        }
    }

    // 5. Persist to storage
    if imported_count > 0 {
        config_state
            .save_config()
            .await
            .map_err(|e| format!("Failed to save config: {}", e))?;
    }

    info!("Successfully imported {} connections", imported_count);

    Ok(ImportResult {
        imported: imported_count,
        skipped: skipped_count,
        merged: merged_count,
        replaced: replaced_count,
        renamed: renames.len(),
        errors,
        renames,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConnectionOptions, ProxyHopConfig};

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
        match second {
            PlannedImportAction::Rename(name) => assert_eq!(name, "prod (Copy)"),
            _ => panic!("expected second duplicate to be renamed"),
        }
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

        match first {
            PlannedImportAction::Replace(target) => {
                assert_eq!(target.existing.id, "conn-1");
                assert_eq!(target.old_keychain_ids.len(), 2);
            }
            _ => panic!("expected first conflict to replace existing connection"),
        }

        match second {
            PlannedImportAction::Rename(name) => assert_eq!(name, "prod (Copy)"),
            _ => panic!("expected second duplicate to be renamed"),
        }
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
