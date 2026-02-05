//! Tauri commands for .oxide file import

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tracing::info;
use uuid::Uuid;

use crate::commands::config::ConfigState;
use crate::config::types::{ProxyHopConfig, SavedAuth, SavedConnection, CONFIG_VERSION};
use crate::oxide_file::{decrypt_oxide_file, EncryptedAuth, EncryptedProxyHop, OxideMetadata};

/// Result of importing connections from .oxide file
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
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
    /// Whether any embedded keys will be extracted
    pub has_embedded_keys: bool,
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
    let key_data = BASE64.decode(base64_data)
        .map_err(|e| format!("Failed to decode embedded key: {}", e))?;
    
    // Create ~/.ssh/imported/ directory
    let home = dirs::home_dir()
        .ok_or_else(|| "Cannot determine home directory".to_string())?;
    
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
    fs::write(&target_path, &key_data)
        .map_err(|e| format!("Failed to write key file: {}", e))?;
    
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
    value: String,
}

/// Pending connection with all resolved auth data
struct PendingConnection {
    connection: SavedConnection,
    keychain_entries: Vec<PendingKeychainEntry>,
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
    config_state: State<'_, Arc<ConfigState>>,
) -> Result<ImportPreview, String> {
    info!("Previewing import from .oxide file ({} bytes)", file_data.len());

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

    // 3. Build set of existing connection names for conflict detection
    let config_snapshot = config_state.get_config_snapshot();
    let mut existing_names: HashSet<String> = config_snapshot
        .connections
        .iter()
        .map(|c| c.name.clone())
        .collect();

    // 4. Compute what will happen for each connection
    let mut unchanged: Vec<String> = Vec::new();
    let mut will_rename: Vec<(String, String)> = Vec::new();
    let mut has_embedded_keys = false;

    for conn in &payload.connections {
        // Check for embedded keys
        if let crate::oxide_file::EncryptedAuth::Key { embedded_key, .. } = &conn.auth {
            if embedded_key.is_some() {
                has_embedded_keys = true;
            }
        }
        if let crate::oxide_file::EncryptedAuth::Certificate { embedded_key, embedded_cert, .. } = &conn.auth {
            if embedded_key.is_some() || embedded_cert.is_some() {
                has_embedded_keys = true;
            }
        }

        // Check if name conflicts
        let new_name = resolve_name_conflict(&conn.name, &existing_names);
        if new_name != conn.name {
            will_rename.push((conn.name.clone(), new_name.clone()));
            existing_names.insert(new_name);
        } else {
            unchanged.push(conn.name.clone());
            existing_names.insert(conn.name.clone());
        }
    }

    Ok(ImportPreview {
        total_connections: payload.connections.len(),
        unchanged,
        will_rename,
        has_embedded_keys,
    })
}

/// Import connections from encrypted .oxide file
#[tauri::command]
pub async fn import_from_oxide(
    file_data: Vec<u8>,
    password: String,
    config_state: State<'_, Arc<ConfigState>>,
) -> Result<ImportResult, String> {
    info!("Importing from .oxide file ({} bytes)", file_data.len());

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

    // 3. Phase 1: Build all connections in memory first (no keychain writes yet)
    //    This ensures we don't leave orphan keychain entries if something fails
    let mut pending_connections: Vec<PendingConnection> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut renames: Vec<(String, String)> = Vec::new();

    // Build set of existing connection names for conflict detection
    let config_snapshot = config_state.get_config_snapshot();
    let mut existing_names: HashSet<String> = config_snapshot
        .connections
        .iter()
        .map(|c| c.name.clone())
        .collect();

    // Helper function to convert EncryptedAuth to SavedAuth WITHOUT writing to keychain
    // Returns (SavedAuth, Vec<PendingKeychainEntry>)
    fn prepare_auth(auth: EncryptedAuth, id: &str) -> (SavedAuth, Vec<PendingKeychainEntry>) {
        let mut entries = Vec::new();

        let saved_auth = match auth {
            EncryptedAuth::Password { password } => {
                let keychain_id = format!("oxide_conn_{}", id);
                entries.push(PendingKeychainEntry {
                    id: keychain_id.clone(),
                    value: password,
                });
                SavedAuth::Password { keychain_id }
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
            });
        }

        (hops, all_entries)
    }

    for enc_conn in payload.connections {
        let new_id = Uuid::new_v4().to_string();
        let original_name = enc_conn.name.clone();
        
        // Resolve name conflicts
        let resolved_name = resolve_name_conflict(&original_name, &existing_names);
        if resolved_name != original_name {
            info!(
                "Name conflict: '{}' -> '{}'",
                original_name, resolved_name
            );
            renames.push((original_name, resolved_name.clone()));
        }
        // Add to existing names to prevent duplicates within the same import batch
        existing_names.insert(resolved_name.clone());

        // Prepare main connection auth
        let (auth, mut keychain_entries) = prepare_auth(enc_conn.auth, &new_id);

        // Prepare proxy_chain auth
        let (proxy_chain, hop_entries) = prepare_proxy_chain(enc_conn.proxy_chain, &new_id);
        keychain_entries.extend(hop_entries);

        let saved_conn = SavedConnection {
            id: new_id,
            version: CONFIG_VERSION,
            name: resolved_name,
            group: enc_conn.group,
            host: enc_conn.host,
            port: enc_conn.port,
            username: enc_conn.username,
            auth,
            options: enc_conn.options,
            created_at: Utc::now(),
            last_used_at: None,
            color: enc_conn.color,
            tags: enc_conn.tags,
            proxy_chain,
        };

        pending_connections.push(PendingConnection {
            connection: saved_conn,
            keychain_entries,
        });
    }

    // 4. Phase 2: All connections validated - now write keychain entries and config atomically
    let mut imported_count = 0;
    let mut written_keychain_ids: Vec<String> = Vec::new();

    for pending in pending_connections {
        // Write all keychain entries for this connection
        let mut keychain_ok = true;
        for entry in &pending.keychain_entries {
            if let Err(e) = config_state.set_keychain_value(&entry.id, &entry.value) {
                errors.push(format!(
                    "Failed to store credentials for {}: {}",
                    pending.connection.name, e
                ));
                keychain_ok = false;
                break;
            }
            written_keychain_ids.push(entry.id.clone());
        }

        if !keychain_ok {
            // Rollback: try to delete already-written keychain entries for this connection
            for entry in &pending.keychain_entries {
                let _ = config_state.delete_keychain_value(&entry.id);
            }
            continue;
        }

        // Add to config
        if let Err(e) = config_state.update_config(|config| {
            config.add_connection(pending.connection);
        }) {
            errors.push(format!("Failed to save connection: {}", e));
            // Rollback keychain entries for this connection
            for entry in &pending.keychain_entries {
                let _ = config_state.delete_keychain_value(&entry.id);
            }
            continue;
        }

        imported_count += 1;
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
        skipped: 0,
        renamed: renames.len(),
        errors,
        renames,
    })
}
