// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Tauri commands for .oxide file export

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use tauri::ipc::Channel;
use tracing::info;

use crate::commands::config::ConfigState;
use crate::commands::forwarding::ForwardingRegistry;
use crate::config::types::SavedAuth;
use crate::oxide_file::{
    EncryptedAuth, EncryptedConnection, EncryptedForward, EncryptedPayload, EncryptedPluginSetting,
    EncryptedProxyHop, OxideMetadata, compute_checksum, encrypt_oxide_file,
    encrypt_oxide_file_with_progress,
};
use zeroize::Zeroizing;

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OxideExportProgressEvent {
    pub stage: String,
    pub current: usize,
    pub total: usize,
}

fn emit_export_progress(
    on_progress: Option<&Channel<OxideExportProgressEvent>>,
    stage: &str,
    current: usize,
    total: usize,
) {
    let Some(channel) = on_progress else {
        return;
    };

    let _ = channel.send(OxideExportProgressEvent {
        stage: stage.to_string(),
        current,
        total,
    });
}

/// Validate password strength.
/// The Argon2id KDF (256 MB, 4 iterations) makes brute-force impractical,
/// so we only enforce a minimum length of 6 characters.
fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < 6 {
        return Err("Password must be at least 6 characters".to_string());
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

fn export_forward(forward: &crate::state::PersistedForward) -> EncryptedForward {
    EncryptedForward {
        forward_type: forward.forward_type.as_str().to_string(),
        bind_address: forward.rule.bind_address.clone(),
        bind_port: forward.rule.bind_port,
        target_host: forward.rule.target_host.clone(),
        target_port: forward.rule.target_port,
        description: forward.rule.description.clone(),
        auto_start: forward.auto_start,
    }
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
    selected_forward_ids: Option<Vec<String>>,
    app_settings_json: Option<String>,
    plugin_settings: Option<Vec<EncryptedPluginSetting>>,
    config_state: State<'_, Arc<ConfigState>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
) -> Result<Vec<u8>, String> {
    export_to_oxide_inner(
        connection_ids,
        password,
        description,
        embed_keys,
        selected_forward_ids,
        app_settings_json,
        plugin_settings,
        config_state,
        forwarding_registry,
        None,
    )
    .await
}

#[tauri::command]
pub async fn export_to_oxide_with_progress(
    connection_ids: Vec<String>,
    password: String,
    description: Option<String>,
    embed_keys: Option<bool>,
    selected_forward_ids: Option<Vec<String>>,
    app_settings_json: Option<String>,
    plugin_settings: Option<Vec<EncryptedPluginSetting>>,
    on_progress: Channel<OxideExportProgressEvent>,
    config_state: State<'_, Arc<ConfigState>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
) -> Result<Vec<u8>, String> {
    export_to_oxide_inner(
        connection_ids,
        password,
        description,
        embed_keys,
        selected_forward_ids,
        app_settings_json,
        plugin_settings,
        config_state,
        forwarding_registry,
        Some(on_progress),
    )
    .await
}

async fn export_to_oxide_inner(
    connection_ids: Vec<String>,
    password: String,
    description: Option<String>,
    embed_keys: Option<bool>,
    selected_forward_ids: Option<Vec<String>>,
    app_settings_json: Option<String>,
    plugin_settings: Option<Vec<EncryptedPluginSetting>>,
    config_state: State<'_, Arc<ConfigState>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
    on_progress: Option<Channel<OxideExportProgressEvent>>,
) -> Result<Vec<u8>, String> {
    let should_embed_keys = embed_keys.unwrap_or(false);
    info!(
        "Exporting {} connections to .oxide file (embed_keys={})",
        connection_ids.len(),
        should_embed_keys
    );

    let total_steps = connection_ids.len() + 8;
    let mut current_step = 0usize;
    let report_progress = |stage: &str, current_step: &mut usize| {
        *current_step += 1;
        emit_export_progress(on_progress.as_ref(), stage, *current_step, total_steps);
    };

    // 1. Validate password strength
    validate_password(&password)?;

    // 2. Load selected connections from config
    let config = config_state.get_config_snapshot();
    let selected_forward_ids =
        selected_forward_ids.map(|ids| ids.into_iter().collect::<std::collections::HashSet<_>>());
    let mut connections = Vec::new();

    for id in &connection_ids {
        let saved_conn = config
            .get_connection(id)
            .ok_or_else(|| format!("Connection {} not found", id))?;

        // Helper function to convert SavedAuth to EncryptedAuth
        let convert_auth = |auth: &SavedAuth, context: &str| -> Result<EncryptedAuth, String> {
            match auth {
                SavedAuth::Password { .. } => Ok(EncryptedAuth::Password {
                    password: Zeroizing::new(String::new()),
                }),
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
                            .map(Zeroizing::new)
                    } else {
                        None
                    };

                    Ok(EncryptedAuth::Key {
                        key_path: key_path.clone(),
                        passphrase: passphrase.map(Zeroizing::new),
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
                            read_and_embed_key(key_path)
                                .map_err(|e| format!("Failed to embed key for {}: {}", context, e))?
                                .map(Zeroizing::new),
                            read_and_embed_key(cert_path)
                                .map_err(|e| {
                                    format!("Failed to embed cert for {}: {}", context, e)
                                })?
                                .map(Zeroizing::new),
                        )
                    } else {
                        (None, None)
                    };

                    Ok(EncryptedAuth::Certificate {
                        key_path: key_path.clone(),
                        cert_path: cert_path.clone(),
                        passphrase: passphrase.map(Zeroizing::new),
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
        let owned_forwards = forwarding_registry
            .load_owned_forwards(&saved_conn.id)
            .await?;
        let forwards = owned_forwards
            .iter()
            .filter(|forward| match &selected_forward_ids {
                Some(selected) => selected.contains(&forward.id),
                None => true,
            })
            .map(export_forward)
            .collect();

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
            forwards,
        });

        report_progress("collecting_connections", &mut current_step);
    }

    // 3. Compute checksum and build payload
    let mut payload = EncryptedPayload {
        version: if app_settings_json.is_some()
            || plugin_settings
                .as_ref()
                .is_some_and(|entries| !entries.is_empty())
        {
            2
        } else {
            1
        },
        connections: connections.clone(),
        app_settings_json,
        plugin_settings: plugin_settings.unwrap_or_default(),
        checksum: String::new(),
    };
    payload.checksum =
        compute_checksum(&payload).map_err(|e| format!("Failed to compute checksum: {:?}", e))?;
    report_progress("computing_checksum", &mut current_step);

    // 4. Build metadata
    let metadata = OxideMetadata {
        exported_at: Utc::now(),
        exported_by: format!("OxideTerm v{}", env!("CARGO_PKG_VERSION")),
        description,
        num_connections: connections.len(),
        connection_names: connections.iter().map(|c| c.name.clone()).collect(),
        has_app_settings: payload.app_settings_json.as_ref().map(|_| true),
        plugin_settings_count: (!payload.plugin_settings.is_empty())
            .then_some(payload.plugin_settings.len()),
    };
    report_progress("building_metadata", &mut current_step);

    // 5. Encrypt
    let oxide_file = if on_progress.is_some() {
        encrypt_oxide_file_with_progress(&payload, &password, metadata, |stage| {
            report_progress(stage, &mut current_step);
        })
        .map_err(|e| format!("Encryption failed: {:?}", e))?
    } else {
        encrypt_oxide_file(&payload, &password, metadata)
            .map_err(|e| format!("Encryption failed: {:?}", e))?
    };

    // 6. Serialize to bytes
    let bytes = oxide_file
        .to_bytes()
        .map_err(|e| format!("Serialization failed: {:?}", e))?;
    report_progress("serializing_file", &mut current_step);

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
        assert!(validate_password("abc").is_err());
        assert!(validate_password("12345").is_err());

        // Exactly 6 characters — valid
        assert!(validate_password("abcdef").is_ok());

        // Longer passwords — valid
        assert!(validate_password("mysyncpass").is_ok());
        assert!(validate_password("ValidPass123!").is_ok());
    }
}
