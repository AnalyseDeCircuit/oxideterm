//! Tauri commands for .oxide file export

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use tracing::info;

use crate::commands::config::ConfigState;
use crate::config::types::SavedAuth;
use crate::oxide_file::{
    compute_checksum, encrypt_oxide_file, EncryptedAuth, EncryptedConnection, EncryptedPayload,
    EncryptedProxyHop, OxideMetadata,
};

/// Pre-flight check result for export
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreflightResult {
    /// Total connections to export
    pub total_connections: usize,
    /// Connections with missing private keys (name, key_path)
    pub missing_keys: Vec<(String, String)>,
    /// Connections using key authentication (can have keys embedded)
    pub connections_with_keys: usize,
    /// Connections using password authentication
    pub connections_with_passwords: usize,
    /// Connections using SSH agent
    pub connections_with_agent: usize,
    /// Total bytes of key files (if embed_keys is enabled)
    pub total_key_bytes: u64,
    /// Whether all connections can be exported
    pub can_export: bool,
}

/// Validate password strength
fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < 12 {
        return Err("密码长度至少 12 个字符".to_string());
    }

    let has_upper = password.chars().any(|c| c.is_uppercase());
    let has_lower = password.chars().any(|c| c.is_lowercase());
    let has_digit = password.chars().any(|c| c.is_numeric());
    let has_special = password.chars().any(|c| !c.is_alphanumeric());

    if !(has_upper && has_lower && has_digit && has_special) {
        return Err("密码必须包含大写、小写、数字和特殊字符".to_string());
    }

    Ok(())
}

/// Read a key or certificate file and encode for embedding
/// Returns None if the file cannot be read (non-fatal for portability)
fn read_and_embed_key(path: &str) -> Result<Option<String>, String> {
    use std::fs;
    use std::path::Path;

    let path = Path::new(path);

    // Expand ~ to home directory
    let expanded_path = if path.starts_with("~") {
        if let Some(home) = dirs::home_dir() {
            home.join(path.strip_prefix("~").unwrap_or(path))
        } else {
            return Ok(None); // Can't expand ~, skip embedding
        }
    } else {
        path.to_path_buf()
    };

    // Check if file exists and is readable
    if !expanded_path.exists() {
        // File doesn't exist on this machine - skip embedding but don't fail
        // This allows exporting connections even if key file is missing
        return Ok(None);
    }

    // Read file content (limit to 1MB to prevent memory issues)
    let metadata =
        fs::metadata(&expanded_path).map_err(|e| format!("Cannot read file metadata: {}", e))?;

    if metadata.len() > 1_048_576 {
        return Err("Key file exceeds 1MB limit".to_string());
    }

    let content = fs::read(&expanded_path).map_err(|e| format!("Cannot read file: {}", e))?;

    // Encode as base64
    Ok(Some(BASE64.encode(&content)))
}

/// Helper to check if a key file exists
fn check_key_file_exists(path: &str) -> Option<u64> {
    use std::fs;
    use std::path::Path;

    let path_obj = Path::new(path);

    // Expand ~ to home directory
    let expanded_path = if path_obj.starts_with("~") {
        if let Some(home) = dirs::home_dir() {
            home.join(path_obj.strip_prefix("~").unwrap_or(path_obj))
        } else {
            return None;
        }
    } else {
        path_obj.to_path_buf()
    };

    // Check if file exists and return its size
    fs::metadata(&expanded_path).ok().map(|m| m.len())
}

/// Pre-flight check before export - detects issues early
#[tauri::command]
pub async fn preflight_export(
    connection_ids: Vec<String>,
    embed_keys: Option<bool>,
    config_state: State<'_, Arc<ConfigState>>,
) -> Result<ExportPreflightResult, String> {
    info!(
        "Running pre-flight check for {} connections",
        connection_ids.len()
    );

    let config = config_state.get_config_snapshot();
    let should_embed_keys = embed_keys.unwrap_or(false);

    let mut missing_keys: Vec<(String, String)> = Vec::new();
    let mut connections_with_keys = 0;
    let mut connections_with_passwords = 0;
    let mut connections_with_agent = 0;
    let mut total_key_bytes: u64 = 0;

    for id in &connection_ids {
        let saved_conn = match config.get_connection(id) {
            Some(c) => c,
            None => continue,
        };

        // Check main connection auth
        match &saved_conn.auth {
            SavedAuth::Password { .. } => {
                connections_with_passwords += 1;
            }
            SavedAuth::Key { key_path, .. } => {
                connections_with_keys += 1;
                if should_embed_keys {
                    if let Some(size) = check_key_file_exists(key_path) {
                        total_key_bytes += size;
                    } else {
                        missing_keys.push((saved_conn.name.clone(), key_path.clone()));
                    }
                }
            }
            SavedAuth::Certificate {
                key_path,
                cert_path,
                ..
            } => {
                connections_with_keys += 1;
                if should_embed_keys {
                    if let Some(size) = check_key_file_exists(key_path) {
                        total_key_bytes += size;
                    } else {
                        missing_keys.push((saved_conn.name.clone(), key_path.clone()));
                    }
                    if let Some(size) = check_key_file_exists(cert_path) {
                        total_key_bytes += size;
                    } else {
                        missing_keys.push((saved_conn.name.clone(), cert_path.clone()));
                    }
                }
            }
            SavedAuth::Agent => {
                connections_with_agent += 1;
            }
        }

        // Check proxy chain auth
        for hop in &saved_conn.proxy_chain {
            match &hop.auth {
                SavedAuth::Password { .. } => {
                    // Don't double count, proxy passwords are fine
                }
                SavedAuth::Key { key_path, .. } => {
                    if should_embed_keys {
                        if let Some(size) = check_key_file_exists(key_path) {
                            total_key_bytes += size;
                        } else {
                            missing_keys
                                .push((format!("{} (proxy)", saved_conn.name), key_path.clone()));
                        }
                    }
                }
                SavedAuth::Certificate {
                    key_path,
                    cert_path,
                    ..
                } => {
                    if should_embed_keys {
                        if let Some(size) = check_key_file_exists(key_path) {
                            total_key_bytes += size;
                        } else {
                            missing_keys
                                .push((format!("{} (proxy)", saved_conn.name), key_path.clone()));
                        }
                        if let Some(size) = check_key_file_exists(cert_path) {
                            total_key_bytes += size;
                        } else {
                            missing_keys
                                .push((format!("{} (proxy)", saved_conn.name), cert_path.clone()));
                        }
                    }
                }
                SavedAuth::Agent => {}
            }
        }
    }

    Ok(ExportPreflightResult {
        total_connections: connection_ids.len(),
        missing_keys,
        connections_with_keys,
        connections_with_passwords,
        connections_with_agent,
        total_key_bytes,
        can_export: true, // We can always export, missing keys just won't be embedded
    })
}

/// Export connections to encrypted .oxide file
#[tauri::command]
pub async fn export_to_oxide(
    connection_ids: Vec<String>,
    password: String,
    description: Option<String>,
    embed_keys: Option<bool>,
    config_state: State<'_, Arc<ConfigState>>,
) -> Result<Vec<u8>, String> {
    let should_embed_keys = embed_keys.unwrap_or(false);
    info!(
        "Exporting {} connections to .oxide file (embed_keys={})",
        connection_ids.len(),
        should_embed_keys
    );

    // 1. Validate password strength
    validate_password(&password)?;

    // 2. Load selected connections from config
    let config = config_state.get_config_snapshot();
    let mut connections = Vec::new();

    for id in &connection_ids {
        let saved_conn = config
            .get_connection(id)
            .ok_or_else(|| format!("Connection {} not found", id))?;

        // Helper function to convert SavedAuth to EncryptedAuth
        let convert_auth = |auth: &SavedAuth, context: &str| -> Result<EncryptedAuth, String> {
            match auth {
                SavedAuth::Password { keychain_id } => {
                    let password = config_state
                        .get_keychain_value(keychain_id)
                        .map_err(|e| format!("Keychain error for {}: {}", context, e))?;
                    Ok(EncryptedAuth::Password { password })
                }
                SavedAuth::Key {
                    key_path,
                    has_passphrase,
                    passphrase_keychain_id,
                } => {
                    let passphrase =
                        if *has_passphrase {
                            if let Some(kc_id) = passphrase_keychain_id {
                                Some(config_state.get_keychain_value(kc_id).map_err(|e| {
                                    format!("Keychain error for {}: {}", context, e)
                                })?)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                    // Optionally embed the private key content
                    let embedded_key = if should_embed_keys {
                        read_and_embed_key(key_path)
                            .map_err(|e| format!("Failed to embed key for {}: {}", context, e))?
                    } else {
                        None
                    };

                    Ok(EncryptedAuth::Key {
                        key_path: key_path.clone(),
                        passphrase,
                        embedded_key,
                    })
                }
                SavedAuth::Certificate {
                    key_path,
                    cert_path,
                    has_passphrase,
                    passphrase_keychain_id,
                } => {
                    let passphrase =
                        if *has_passphrase {
                            if let Some(kc_id) = passphrase_keychain_id {
                                Some(config_state.get_keychain_value(kc_id).map_err(|e| {
                                    format!("Keychain error for {}: {}", context, e)
                                })?)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                    // Optionally embed key and cert content
                    let (embedded_key, embedded_cert) = if should_embed_keys {
                        (
                            read_and_embed_key(key_path).map_err(|e| {
                                format!("Failed to embed key for {}: {}", context, e)
                            })?,
                            read_and_embed_key(cert_path).map_err(|e| {
                                format!("Failed to embed cert for {}: {}", context, e)
                            })?,
                        )
                    } else {
                        (None, None)
                    };

                    Ok(EncryptedAuth::Certificate {
                        key_path: key_path.clone(),
                        cert_path: cert_path.clone(),
                        passphrase,
                        embedded_key,
                        embedded_cert,
                    })
                }
                SavedAuth::Agent => Ok(EncryptedAuth::Agent),
            }
        };

        // Build encrypted proxy_chain from saved proxy_chain OR legacy jump_host
        let mut encrypted_proxy_chain: Vec<EncryptedProxyHop> = Vec::new();

        if !saved_conn.proxy_chain.is_empty() {
            // New proxy_chain format
            for (hop_index, hop) in saved_conn.proxy_chain.iter().enumerate() {
                let hop_auth = convert_auth(
                    &hop.auth,
                    &format!("hop {} of {}", hop_index, saved_conn.name),
                )?;
                encrypted_proxy_chain.push(EncryptedProxyHop {
                    host: hop.host.clone(),
                    port: hop.port,
                    username: hop.username.clone(),
                    auth: hop_auth,
                });
            }
        } else if let Some(jump_id) = &saved_conn.options.jump_host {
            // Legacy jump_host format - convert to proxy_chain
            let jump_conn = config.get_connection(jump_id).ok_or_else(|| {
                format!(
                    "Connection '{}' references jump host '{}' which does not exist. \
                    Please ensure all jump hosts are saved before exporting.",
                    saved_conn.name, jump_id
                )
            })?;
            let hop_auth = convert_auth(
                &jump_conn.auth,
                &format!("jump host of {}", saved_conn.name),
            )?;
            encrypted_proxy_chain.push(EncryptedProxyHop {
                host: jump_conn.host.clone(),
                port: jump_conn.port,
                username: jump_conn.username.clone(),
                auth: hop_auth,
            });
        }

        // Export target server with its proxy_chain
        let target_auth = convert_auth(&saved_conn.auth, &saved_conn.name)?;

        connections.push(EncryptedConnection {
            name: saved_conn.name.clone(),
            group: saved_conn.group.clone(),
            host: saved_conn.host.clone(),
            port: saved_conn.port,
            username: saved_conn.username.clone(),
            auth: target_auth,
            color: saved_conn.color.clone(),
            tags: saved_conn.tags.clone(),
            options: saved_conn.options.clone(),
            proxy_chain: encrypted_proxy_chain,
        });
    }

    // 3. Compute checksum and build payload
    let checksum = compute_checksum(&connections)
        .map_err(|e| format!("Failed to compute checksum: {:?}", e))?;

    let payload = EncryptedPayload {
        version: 1,
        connections: connections.clone(),
        checksum,
    };

    // 4. Build metadata
    let metadata = OxideMetadata {
        exported_at: Utc::now(),
        exported_by: format!("OxideTerm v{}", env!("CARGO_PKG_VERSION")),
        description,
        num_connections: connections.len(),
        connection_names: connections.iter().map(|c| c.name.clone()).collect(),
    };

    // 5. Encrypt
    let oxide_file = encrypt_oxide_file(&payload, &password, metadata)
        .map_err(|e| format!("Encryption failed: {:?}", e))?;

    // 6. Serialize to bytes
    let bytes = oxide_file
        .to_bytes()
        .map_err(|e| format!("Serialization failed: {:?}", e))?;

    info!(
        "Successfully exported {} connections ({} bytes)",
        connections.len(),
        bytes.len()
    );

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_validation() {
        // Too short
        assert!(validate_password("Short1!").is_err());

        // No uppercase
        assert!(validate_password("nouppercase1!").is_err());

        // No lowercase
        assert!(validate_password("NOLOWERCASE1!").is_err());

        // No digits
        assert!(validate_password("NoDigits!abc").is_err());

        // No special characters
        assert!(validate_password("NoSpecial123Abc").is_err());

        // Valid
        assert!(validate_password("ValidPass123!").is_ok());
        assert!(validate_password("MySecureP@ssw0rd").is_ok());
    }
}
