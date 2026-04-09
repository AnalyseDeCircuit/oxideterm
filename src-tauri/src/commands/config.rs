// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Configuration Commands
//!
//! Tauri commands for managing saved connections and SSH config import.

use crate::config::{
    AiProviderVault, ConfigFile, ConfigStorage, Keychain, KeychainError, ProxyHopConfig, SavedAuth,
    SavedConnection, SshConfigHost, default_ssh_config_path, parse_ssh_config,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tauri::{Manager, State};
use zeroize::Zeroizing;

use super::forwarding::ForwardingRegistry;

/// Service name for AI provider API keys in system keychain
const AI_KEYCHAIN_SERVICE: &str = "com.oxideterm.ai";

/// AI provider configuration synced from frontend settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub provider_type: String,
    pub base_url: String,
    pub default_model: String,
    pub enabled: bool,
}

/// Shared config state
pub struct ConfigState {
    storage: ConfigStorage,
    config: RwLock<ConfigFile>,
    keychain: Keychain,
    pub(crate) ai_keychain: Keychain,
    /// In-memory cache for AI provider API keys.
    /// Populated after the first successful Touch ID authentication so
    /// subsequent `get_ai_provider_api_key` calls within the same app
    /// session do not re-trigger the biometric prompt.
    pub(crate) api_key_cache: RwLock<HashMap<String, String>>,
    /// AI provider configurations synced from frontend settings.
    /// Used by CLI server to resolve providers without accessing frontend localStorage.
    pub(crate) ai_providers: RwLock<(Vec<AiProviderConfig>, Option<String>)>,
}

impl ConfigState {
    /// Create new config state, loading from disk
    pub async fn new() -> Result<Self, String> {
        let storage = ConfigStorage::new().map_err(|e| e.to_string())?;
        let config = storage.load().await.map_err(|e| e.to_string())?;

        Ok(Self {
            storage,
            config: RwLock::new(config),
            keychain: Keychain::new(),
            ai_keychain: Keychain::with_biometrics(AI_KEYCHAIN_SERVICE),
            api_key_cache: RwLock::new(HashMap::new()),
            ai_providers: RwLock::new((Vec::new(), None)),
        })
    }

    /// Save config to disk
    async fn save(&self) -> Result<(), String> {
        let config = self.config.read().clone();
        self.storage.save(&config).await.map_err(|e| e.to_string())
    }

    /// Public API: Get a snapshot of the config
    pub fn get_config_snapshot(&self) -> ConfigFile {
        self.config.read().clone()
    }

    /// Public API: Update config with a closure
    pub fn update_config<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut ConfigFile),
    {
        let mut config = self.config.write();
        f(&mut config);
        Ok(())
    }

    /// Public API: Get value from keychain
    pub fn get_keychain_value(&self, key: &str) -> Result<String, String> {
        self.keychain.get(key).map_err(|e| e.to_string())
    }

    /// Public API: Store value in keychain
    pub fn set_keychain_value(&self, key: &str, value: &str) -> Result<(), String> {
        self.keychain.store(key, value).map_err(|e| e.to_string())
    }

    /// Public API: Delete value from keychain
    pub fn delete_keychain_value(&self, key: &str) -> Result<(), String> {
        self.keychain.delete(key).map_err(|e| e.to_string())
    }

    /// Public API: Save config to disk
    pub async fn save_config(&self) -> Result<(), String> {
        self.save().await
    }
}

/// Proxy hop info for frontend (without sensitive credentials)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyHopInfo {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String, // "password", "key", "agent"
    pub key_path: Option<String>,
}

/// Connection info for frontend (without sensitive data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub id: String,
    pub name: String,
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String, // "password", "key", "agent", "certificate"
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub color: Option<String>,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_chain: Vec<ProxyHopInfo>,
}

/// Helper to convert SavedAuth to (auth_type, key_path, cert_path) tuple
fn auth_to_info(auth: &SavedAuth) -> (String, Option<String>, Option<String>) {
    match auth {
        SavedAuth::Password { .. } => ("password".to_string(), None, None),
        SavedAuth::Key { key_path, .. } => ("key".to_string(), Some(key_path.clone()), None),
        SavedAuth::Certificate {
            key_path,
            cert_path,
            ..
        } => (
            "certificate".to_string(),
            Some(key_path.clone()),
            Some(cert_path.clone()),
        ),
        SavedAuth::Agent => ("agent".to_string(), None, None),
    }
}

pub(crate) fn collect_keychain_ids_for_auth(auth: &SavedAuth) -> Vec<String> {
    match auth {
        SavedAuth::Password {
            keychain_id: Some(keychain_id),
        } => vec![keychain_id.clone()],
        SavedAuth::Password { keychain_id: None } => Vec::new(),
        SavedAuth::Key {
            passphrase_keychain_id,
            ..
        }
        | SavedAuth::Certificate {
            passphrase_keychain_id,
            ..
        } => passphrase_keychain_id.iter().cloned().collect(),
        SavedAuth::Agent => Vec::new(),
    }
}

pub(crate) fn collect_connection_keychain_ids(connection: &SavedConnection) -> Vec<String> {
    let mut ids = collect_keychain_ids_for_auth(&connection.auth);
    for hop in &connection.proxy_chain {
        ids.extend(collect_keychain_ids_for_auth(&hop.auth));
    }
    ids
}

impl From<&SavedConnection> for ConnectionInfo {
    fn from(conn: &SavedConnection) -> Self {
        let (auth_type, key_path, cert_path) = auth_to_info(&conn.auth);

        // Convert proxy_chain to ProxyHopInfo (without sensitive data)
        let proxy_chain: Vec<ProxyHopInfo> = conn
            .proxy_chain
            .iter()
            .map(|hop| {
                let (hop_auth_type, hop_key_path, _) = auth_to_info(&hop.auth);
                ProxyHopInfo {
                    host: hop.host.clone(),
                    port: hop.port,
                    username: hop.username.clone(),
                    auth_type: hop_auth_type,
                    key_path: hop_key_path,
                }
            })
            .collect();

        Self {
            id: conn.id.clone(),
            name: conn.name.clone(),
            group: conn.group.clone(),
            host: conn.host.clone(),
            port: conn.port,
            username: conn.username.clone(),
            auth_type,
            key_path,
            cert_path,
            created_at: conn.created_at.to_rfc3339(),
            last_used_at: conn.last_used_at.map(|t| t.to_rfc3339()),
            color: conn.color.clone(),
            tags: conn.tags.clone(),
            proxy_chain,
        }
    }
}

/// Request to create/update a connection
#[derive(Debug, Clone, Deserialize)]
pub struct SaveConnectionRequest {
    pub id: Option<String>, // None = create new, Some = update
    pub name: String,
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,                   // "password", "key", "agent"
    pub password: Option<Zeroizing<String>>, // Only for password auth
    pub key_path: Option<String>,            // Only for key auth
    pub cert_path: Option<String>,           // Only for certificate auth
    pub color: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub jump_host: Option<String>, // Legacy jump host for backward compatibility
    #[serde(default)]
    pub agent_forwarding: Option<bool>,
    pub proxy_chain: Option<Vec<ProxyHopRequest>>, // Multi-hop proxy chain
}

/// Request for a single proxy hop in the chain
#[derive(Debug, Clone, Deserialize)]
pub struct ProxyHopRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String, // "password", "key", "agent", "default_key"
    pub password: Option<Zeroizing<String>>, // Only for password auth
    pub key_path: Option<String>, // Only for key auth
    pub passphrase: Option<Zeroizing<String>>, // Passphrase for encrypted keys
    #[serde(default)]
    pub agent_forwarding: Option<bool>,
}

/// SSH config host info for frontend
#[derive(Debug, Clone, Serialize)]
pub struct SshHostInfo {
    pub alias: String,
    pub hostname: String,
    pub user: Option<String>,
    pub port: u16,
    pub identity_file: Option<String>,
    pub already_imported: bool,
}

impl From<&SshConfigHost> for SshHostInfo {
    fn from(host: &SshConfigHost) -> Self {
        Self {
            alias: host.alias.clone(),
            hostname: host.effective_hostname().to_string(),
            user: host.user.clone(),
            port: host.effective_port(),
            identity_file: host.identity_file.clone(),
            already_imported: false,
        }
    }
}

// =============================================================================
// Tauri Commands
// =============================================================================

/// Get all saved connections
#[tauri::command]
pub async fn get_connections(
    state: State<'_, Arc<ConfigState>>,
) -> Result<Vec<ConnectionInfo>, String> {
    let config = state.config.read();
    Ok(config
        .connections
        .iter()
        .map(ConnectionInfo::from)
        .collect())
}

/// Get recent connections
#[tauri::command]
pub async fn get_recent_connections(
    state: State<'_, Arc<ConfigState>>,
    limit: Option<usize>,
) -> Result<Vec<ConnectionInfo>, String> {
    let config = state.config.read();
    let limit = limit.unwrap_or(5);
    Ok(config
        .get_recent(limit)
        .into_iter()
        .map(ConnectionInfo::from)
        .collect())
}

/// Get connections by group
#[tauri::command]
pub async fn get_connections_by_group(
    state: State<'_, Arc<ConfigState>>,
    group: Option<String>,
) -> Result<Vec<ConnectionInfo>, String> {
    let config = state.config.read();
    Ok(config
        .get_by_group(group.as_deref())
        .into_iter()
        .map(ConnectionInfo::from)
        .collect())
}

/// Search connections
#[tauri::command]
pub async fn search_connections(
    state: State<'_, Arc<ConfigState>>,
    query: String,
) -> Result<Vec<ConnectionInfo>, String> {
    let config = state.config.read();
    Ok(config
        .search(&query)
        .into_iter()
        .map(ConnectionInfo::from)
        .collect())
}

/// Get all groups
#[tauri::command]
pub async fn get_groups(state: State<'_, Arc<ConfigState>>) -> Result<Vec<String>, String> {
    let config = state.config.read();
    Ok(config.groups.clone())
}

/// Build a SavedAuth from request fields
fn build_saved_auth(
    auth_type: &str,
    password: Option<&str>,
    key_path: Option<&str>,
    cert_path: Option<&str>,
    keychain: &crate::config::keychain::Keychain,
) -> Result<SavedAuth, String> {
    match auth_type {
        "password" => {
            if let Some(pwd) = password {
                let keychain_id = format!("oxide_conn_{}", uuid::Uuid::new_v4());
                keychain
                    .store(&keychain_id, pwd)
                    .map_err(|e| e.to_string())?;
                Ok(SavedAuth::Password {
                    keychain_id: Some(keychain_id),
                })
            } else {
                // User chose not to save password — will be prompted on connect
                Ok(SavedAuth::Password { keychain_id: None })
            }
        }
        "certificate" => {
            let kp = key_path.ok_or("Key path required for certificate authentication")?;
            let cp = cert_path.ok_or("Certificate path required for certificate authentication")?;
            Ok(SavedAuth::Certificate {
                key_path: kp.to_string(),
                cert_path: cp.to_string(),
                has_passphrase: false,
                passphrase_keychain_id: None,
            })
        }
        "key" => {
            let kp = key_path.ok_or("Key path required for key authentication")?;
            Ok(SavedAuth::Key {
                key_path: kp.to_string(),
                has_passphrase: false,
                passphrase_keychain_id: None,
            })
        }
        _ => Ok(SavedAuth::Agent),
    }
}

fn build_saved_auth_for_update(
    existing_auth: &SavedAuth,
    auth_type: &str,
    password: Option<&str>,
    key_path: Option<&str>,
    cert_path: Option<&str>,
    keychain: &crate::config::keychain::Keychain,
) -> Result<SavedAuth, String> {
    match auth_type {
        "password" => {
            if let Some(pwd) = password {
                if let SavedAuth::Password {
                    keychain_id: Some(existing_keychain_id),
                } = existing_auth
                {
                    keychain
                        .store(existing_keychain_id, pwd)
                        .map_err(|e| e.to_string())?;
                    Ok(SavedAuth::Password {
                        keychain_id: Some(existing_keychain_id.clone()),
                    })
                } else {
                    build_saved_auth(auth_type, Some(pwd), key_path, cert_path, keychain)
                }
            } else if let SavedAuth::Password { keychain_id } = existing_auth {
                Ok(SavedAuth::Password {
                    keychain_id: keychain_id.clone(),
                })
            } else {
                Ok(SavedAuth::Password { keychain_id: None })
            }
        }
        "key" => {
            let kp = key_path.ok_or("Key path required for key authentication")?;
            match existing_auth {
                SavedAuth::Key {
                    key_path: existing_key_path,
                    has_passphrase,
                    passphrase_keychain_id,
                } if existing_key_path == kp => Ok(SavedAuth::Key {
                    key_path: kp.to_string(),
                    has_passphrase: *has_passphrase,
                    passphrase_keychain_id: passphrase_keychain_id.clone(),
                }),
                _ => Ok(SavedAuth::Key {
                    key_path: kp.to_string(),
                    has_passphrase: false,
                    passphrase_keychain_id: None,
                }),
            }
        }
        "certificate" => {
            let kp = key_path.ok_or("Key path required for certificate authentication")?;
            let cp = cert_path.ok_or("Certificate path required for certificate authentication")?;
            match existing_auth {
                SavedAuth::Certificate {
                    key_path: existing_key_path,
                    cert_path: existing_cert_path,
                    has_passphrase,
                    passphrase_keychain_id,
                } if existing_key_path == kp && existing_cert_path == cp => {
                    Ok(SavedAuth::Certificate {
                        key_path: kp.to_string(),
                        cert_path: cp.to_string(),
                        has_passphrase: *has_passphrase,
                        passphrase_keychain_id: passphrase_keychain_id.clone(),
                    })
                }
                _ => Ok(SavedAuth::Certificate {
                    key_path: kp.to_string(),
                    cert_path: cp.to_string(),
                    has_passphrase: false,
                    passphrase_keychain_id: None,
                }),
            }
        }
        _ => Ok(SavedAuth::Agent),
    }
}

/// Save (create or update) a connection
#[tauri::command]
pub async fn save_connection(
    state: State<'_, Arc<ConfigState>>,
    request: SaveConnectionRequest,
) -> Result<ConnectionInfo, String> {
    let connection = {
        let mut config = state.config.write();

        if let Some(id) = request.id {
            let jump_conn = if let Some(ref jump_host) = request.jump_host {
                config
                    .connections
                    .iter()
                    .find(|c| c.options.jump_host == Some(jump_host.clone()))
                    .cloned()
            } else {
                None
            };

            let conn = config
                .get_connection_mut(&id)
                .ok_or("Connection not found")?;

            if request.jump_host.is_some() {
                if !matches!(&conn.auth, SavedAuth::Key { .. }) {
                    conn.options.jump_host = None;
                }

                let mut proxy_chain = conn.proxy_chain.clone();

                if let Some(jump_conn) = jump_conn {
                    let hop_config = match &jump_conn.auth {
                        SavedAuth::Key {
                            key_path,
                            passphrase_keychain_id,
                            ..
                        } => SavedAuth::Key {
                            key_path: key_path.clone(),
                            has_passphrase: false,
                            passphrase_keychain_id: passphrase_keychain_id.clone(),
                        },
                        _ => {
                            return Err(
                                "Jump host must use key authentication for proxy chain".to_string()
                            );
                        }
                    };

                    proxy_chain.push(ProxyHopConfig {
                        host: jump_conn.host.clone(),
                        port: jump_conn.port,
                        username: jump_conn.username.clone(),
                        auth: hop_config,
                        agent_forwarding: false,
                    });
                }

                conn.proxy_chain = proxy_chain;
                conn.options.jump_host = None;
            }

            if let Some(ref proxy_chain_req) = request.proxy_chain {
                let mut proxy_chain = Vec::new();

                for hop_req in proxy_chain_req {
                    let auth = match hop_req.auth_type.as_str() {
                        "password" => {
                            let kc_id = format!("oxide_hop_{}", uuid::Uuid::new_v4());
                            let password = hop_req
                                .password
                                .as_ref()
                                .ok_or("Password required for proxy hop")?;
                            state
                                .keychain
                                .store(&kc_id, password)
                                .map_err(|e| e.to_string())?;
                            SavedAuth::Password {
                                keychain_id: Some(kc_id),
                            }
                        }
                        "key" => {
                            let key_path = hop_req
                                .key_path
                                .as_ref()
                                .ok_or("Key path required for proxy hop")?;
                            let passphrase_keychain_id =
                                if let Some(ref passphrase) = hop_req.passphrase {
                                    let kc_id = format!("oxide_hop_key_{}", uuid::Uuid::new_v4());
                                    state
                                        .keychain
                                        .store(&kc_id, passphrase)
                                        .map_err(|e| e.to_string())?;
                                    Some(kc_id)
                                } else {
                                    None
                                };

                            SavedAuth::Key {
                                key_path: key_path.clone(),
                                has_passphrase: hop_req.passphrase.is_some(),
                                passphrase_keychain_id,
                            }
                        }
                        "default_key" => {
                            use crate::session::KeyAuth;
                            let key_auth = KeyAuth::from_default_locations(
                                hop_req.passphrase.as_ref().map(|p| p.as_str()),
                            )
                            .map_err(|e| format!("No SSH key found for proxy hop: {}", e))?;

                            SavedAuth::Key {
                                key_path: key_auth.key_path.to_string_lossy().to_string(),
                                has_passphrase: false,
                                passphrase_keychain_id: None,
                            }
                        }
                        _ => return Err(format!("Invalid auth type: {}", hop_req.auth_type)),
                    };

                    proxy_chain.push(ProxyHopConfig {
                        host: hop_req.host.clone(),
                        port: hop_req.port,
                        username: hop_req.username.clone(),
                        auth,
                        agent_forwarding: hop_req.agent_forwarding.unwrap_or(false),
                    });
                }

                conn.proxy_chain = proxy_chain;
            }

            conn.name = request.name;
            conn.group = request.group;
            conn.host = request.host;
            conn.port = request.port;
            conn.username = request.username;
            conn.color = request.color;
            conn.tags = request.tags;
            if let Some(agent_forwarding) = request.agent_forwarding {
                conn.options.agent_forwarding = agent_forwarding;
            }

            conn.auth = build_saved_auth_for_update(
                &conn.auth,
                &request.auth_type,
                request.password.as_ref().map(|s| s.as_str()),
                request.key_path.as_deref(),
                request.cert_path.as_deref(),
                &state.keychain,
            )?;

            conn.last_used_at = Some(chrono::Utc::now());

            conn.clone()
        } else {
            let auth = build_saved_auth(
                &request.auth_type,
                request.password.as_ref().map(|s| s.as_str()),
                request.key_path.as_deref(),
                request.cert_path.as_deref(),
                &state.keychain,
            )?;

            let mut proxy_chain = Vec::new();

            if let Some(ref proxy_chain_req) = request.proxy_chain {
                for hop_req in proxy_chain_req {
                    let hop_auth = match hop_req.auth_type.as_str() {
                        "password" => {
                            let kc_id = format!("oxide_hop_{}", uuid::Uuid::new_v4());
                            let password = hop_req
                                .password
                                .as_ref()
                                .ok_or("Password required for proxy hop")?;
                            state
                                .keychain
                                .store(&kc_id, password)
                                .map_err(|e| e.to_string())?;
                            SavedAuth::Password {
                                keychain_id: Some(kc_id),
                            }
                        }
                        "key" => {
                            let key_path = hop_req
                                .key_path
                                .as_ref()
                                .ok_or("Key path required for proxy hop")?;
                            let passphrase_keychain_id =
                                if let Some(ref passphrase) = hop_req.passphrase {
                                    let kc_id = format!("oxide_hop_key_{}", uuid::Uuid::new_v4());
                                    state
                                        .keychain
                                        .store(&kc_id, passphrase)
                                        .map_err(|e| e.to_string())?;
                                    Some(kc_id)
                                } else {
                                    None
                                };

                            SavedAuth::Key {
                                key_path: key_path.clone(),
                                has_passphrase: hop_req.passphrase.is_some(),
                                passphrase_keychain_id,
                            }
                        }
                        "default_key" => {
                            use crate::session::KeyAuth;
                            let key_auth = KeyAuth::from_default_locations(
                                hop_req.passphrase.as_ref().map(|p| p.as_str()),
                            )
                            .map_err(|e| format!("No SSH key found for proxy hop: {}", e))?;

                            SavedAuth::Key {
                                key_path: key_auth.key_path.to_string_lossy().to_string(),
                                has_passphrase: false,
                                passphrase_keychain_id: None,
                            }
                        }
                        _ => return Err(format!("Invalid auth type: {}", hop_req.auth_type)),
                    };

                    proxy_chain.push(ProxyHopConfig {
                        host: hop_req.host.clone(),
                        port: hop_req.port,
                        username: hop_req.username.clone(),
                        auth: hop_auth,
                        agent_forwarding: hop_req.agent_forwarding.unwrap_or(false),
                    });
                }
            }

            let group = request.group.clone();
            let conn = SavedConnection {
                id: uuid::Uuid::new_v4().to_string(),
                version: crate::config::CONFIG_VERSION,
                name: request.name,
                group: group.clone(),
                host: request.host,
                port: request.port,
                username: request.username,
                auth,
                options: crate::config::ConnectionOptions {
                    agent_forwarding: request.agent_forwarding.unwrap_or(false),
                    ..Default::default()
                },
                created_at: chrono::Utc::now(),
                last_used_at: None,
                color: request.color,
                tags: request.tags,
                proxy_chain,
            };

            if let Some(ref group) = group {
                if !config.groups.contains(group) {
                    config.groups.push(group.clone());
                }
            }

            config.add_connection(conn.clone());
            conn
        }
    };

    state.save().await?;

    Ok(ConnectionInfo::from(&connection))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_saved_auth_for_update_preserves_saved_password_when_no_new_password_is_provided() {
        let existing = SavedAuth::Password {
            keychain_id: Some("kc-1".to_string()),
        };

        let updated = build_saved_auth_for_update(
            &existing,
            "password",
            None,
            None,
            None,
            &Keychain::with_service("com.oxideterm.test"),
        )
        .unwrap();

        assert_eq!(
            updated,
            SavedAuth::Password {
                keychain_id: Some("kc-1".to_string())
            }
        );
    }

    #[test]
    fn build_saved_auth_for_update_preserves_key_passphrase_for_unchanged_key_path() {
        let existing = SavedAuth::Key {
            key_path: "/tmp/id_ed25519".to_string(),
            has_passphrase: true,
            passphrase_keychain_id: Some("kc-pass".to_string()),
        };

        let updated = build_saved_auth_for_update(
            &existing,
            "key",
            None,
            Some("/tmp/id_ed25519"),
            None,
            &Keychain::with_service("com.oxideterm.test"),
        )
        .unwrap();

        assert_eq!(updated, existing);
    }

    #[test]
    fn build_saved_auth_for_update_clears_key_passphrase_when_key_path_changes() {
        let existing = SavedAuth::Key {
            key_path: "/tmp/id_ed25519".to_string(),
            has_passphrase: true,
            passphrase_keychain_id: Some("kc-pass".to_string()),
        };

        let updated = build_saved_auth_for_update(
            &existing,
            "key",
            None,
            Some("/tmp/id_rsa"),
            None,
            &Keychain::with_service("com.oxideterm.test"),
        )
        .unwrap();

        assert_eq!(
            updated,
            SavedAuth::Key {
                key_path: "/tmp/id_rsa".to_string(),
                has_passphrase: false,
                passphrase_keychain_id: None,
            }
        );
    }

    #[test]
    fn build_saved_auth_for_update_preserves_certificate_passphrase_when_paths_are_unchanged() {
        let existing = SavedAuth::Certificate {
            key_path: "/tmp/id_ed25519".to_string(),
            cert_path: "/tmp/id_ed25519-cert.pub".to_string(),
            has_passphrase: true,
            passphrase_keychain_id: Some("kc-cert".to_string()),
        };

        let updated = build_saved_auth_for_update(
            &existing,
            "certificate",
            None,
            Some("/tmp/id_ed25519"),
            Some("/tmp/id_ed25519-cert.pub"),
            &Keychain::with_service("com.oxideterm.test"),
        )
        .unwrap();

        assert_eq!(updated, existing);
    }

    #[test]
    fn collect_connection_keychain_ids_includes_main_and_proxy_auth_entries() {
        let connection = SavedConnection {
            id: "conn-1".to_string(),
            version: crate::config::CONFIG_VERSION,
            name: "test".to_string(),
            group: None,
            host: "example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SavedAuth::Certificate {
                key_path: "/tmp/id_ed25519".to_string(),
                cert_path: "/tmp/id_ed25519-cert.pub".to_string(),
                has_passphrase: true,
                passphrase_keychain_id: Some("kc-cert".to_string()),
            },
            options: Default::default(),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            color: None,
            tags: Vec::new(),
            proxy_chain: vec![ProxyHopConfig {
                host: "jump.example.com".to_string(),
                port: 22,
                username: "jump".to_string(),
                auth: SavedAuth::Password {
                    keychain_id: Some("kc-hop".to_string()),
                },
                agent_forwarding: false,
            }],
        };

        let ids = collect_connection_keychain_ids(&connection);

        assert_eq!(ids, vec!["kc-cert".to_string(), "kc-hop".to_string()]);
    }
}

/// Delete a connection
#[tauri::command]
pub async fn delete_connection(
    state: State<'_, Arc<ConfigState>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
    id: String,
) -> Result<(), String> {
    {
        let mut config = state.config.write();
        let connection = config
            .remove_connection(&id)
            .ok_or("Connection not found")?;

        for keychain_id in collect_connection_keychain_ids(&connection) {
            let _ = state.keychain.delete(&keychain_id);
        }
    } // config lock dropped here

    forwarding_registry.delete_owned_forwards(&id).await?;

    state.save().await?;

    Ok(())
}

/// Mark connection as used (update last_used_at and recent list)
#[tauri::command]
pub async fn mark_connection_used(
    state: State<'_, Arc<ConfigState>>,
    id: String,
) -> Result<(), String> {
    {
        let mut config = state.config.write();
        config.mark_used(&id);
    }
    state.save().await?;
    Ok(())
}

/// Get password for a connection (from keychain)
#[tauri::command]
pub async fn get_connection_password(
    state: State<'_, Arc<ConfigState>>,
    id: String,
) -> Result<String, String> {
    let config = state.config.read();
    let conn = config.get_connection(&id).ok_or("Connection not found")?;

    match &conn.auth {
        SavedAuth::Password {
            keychain_id: Some(keychain_id),
        } => state.keychain.get(keychain_id).map_err(|e| e.to_string()),
        SavedAuth::Password { keychain_id: None } => {
            Err("Password not saved for this connection".to_string())
        }
        _ => Err("Connection does not use password auth".to_string()),
    }
}

/// Import hosts from SSH config
#[tauri::command]
pub async fn list_ssh_config_hosts(
    state: State<'_, Arc<ConfigState>>,
) -> Result<Vec<SshHostInfo>, String> {
    let hosts = parse_ssh_config(None).await.map_err(|e| e.to_string())?;
    let existing_names: HashSet<String> = {
        let config = state.config.read();
        config.connections.iter().map(|c| c.name.clone()).collect()
    };
    Ok(hosts
        .iter()
        .map(|h| {
            let mut info = SshHostInfo::from(h);
            info.already_imported = existing_names.contains(&h.alias);
            info
        })
        .collect())
}

/// Import a single SSH config host as a saved connection
#[tauri::command]
pub async fn import_ssh_host(
    state: State<'_, Arc<ConfigState>>,
    alias: String,
) -> Result<ConnectionInfo, String> {
    // Parse SSH config
    let hosts = parse_ssh_config(None).await.map_err(|e| e.to_string())?;
    let host = hosts
        .iter()
        .find(|h| h.alias == alias)
        .ok_or_else(|| format!("Host '{}' not found in SSH config", alias))?;

    // Create connection
    let auth = if let Some(ref key_path) = host.identity_file {
        SavedAuth::Key {
            key_path: key_path.clone(),
            has_passphrase: false,
            passphrase_keychain_id: None,
        }
    } else {
        SavedAuth::Agent
    };

    let username = host.user.clone().unwrap_or_else(whoami::username);

    let conn = SavedConnection {
        id: uuid::Uuid::new_v4().to_string(),
        version: crate::config::CONFIG_VERSION,
        name: alias.clone(),
        group: Some("Imported".to_string()),
        host: host.effective_hostname().to_string(),
        port: host.effective_port(),
        username,
        auth,
        options: Default::default(),
        created_at: chrono::Utc::now(),
        last_used_at: None,
        color: None,
        tags: vec!["ssh-config".to_string()],
        proxy_chain: Vec::new(),
    };

    {
        let mut config = state.config.write();
        config.add_connection(conn.clone());

        if !config.groups.contains(&"Imported".to_string()) {
            config.groups.push("Imported".to_string());
        }
    } // config lock dropped here

    state.save().await?;

    Ok(ConnectionInfo::from(&conn))
}

/// Batch result for importing multiple SSH config hosts
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshBatchImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

/// Import multiple SSH config hosts as saved connections
#[tauri::command]
pub async fn import_ssh_hosts(
    state: State<'_, Arc<ConfigState>>,
    aliases: Vec<String>,
) -> Result<SshBatchImportResult, String> {
    let hosts = parse_ssh_config(None).await.map_err(|e| e.to_string())?;

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut errors = Vec::new();

    // Collect existing names for conflict detection
    let existing_names: HashSet<String> = {
        let config = state.config.read();
        config.connections.iter().map(|c| c.name.clone()).collect()
    };

    for alias in &aliases {
        let host = match hosts.iter().find(|h| &h.alias == alias) {
            Some(h) => h,
            None => {
                errors.push(format!("Host '{}' not found in SSH config", alias));
                continue;
            }
        };

        if existing_names.contains(alias) {
            skipped += 1;
            continue;
        }

        let auth = if let Some(ref key_path) = host.identity_file {
            SavedAuth::Key {
                key_path: key_path.clone(),
                has_passphrase: false,
                passphrase_keychain_id: None,
            }
        } else {
            SavedAuth::Agent
        };

        let username = host.user.clone().unwrap_or_else(whoami::username);

        let conn = SavedConnection {
            id: uuid::Uuid::new_v4().to_string(),
            version: crate::config::CONFIG_VERSION,
            name: alias.clone(),
            group: Some("Imported".to_string()),
            host: host.effective_hostname().to_string(),
            port: host.effective_port(),
            username,
            auth,
            options: Default::default(),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            color: None,
            tags: vec!["ssh-config".to_string()],
            proxy_chain: Vec::new(),
        };

        {
            let mut config = state.config.write();
            config.add_connection(conn);

            if !config.groups.contains(&"Imported".to_string()) {
                config.groups.push("Imported".to_string());
            }
        }

        imported += 1;
    }

    if imported > 0 {
        state.save().await?;
    }

    Ok(SshBatchImportResult {
        imported,
        skipped,
        errors,
    })
}

/// Get SSH config file path
#[tauri::command]
pub async fn get_ssh_config_path() -> Result<String, String> {
    default_ssh_config_path()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

/// Create groups
#[tauri::command]
pub async fn create_group(state: State<'_, Arc<ConfigState>>, name: String) -> Result<(), String> {
    {
        let mut config = state.config.write();
        if !config.groups.contains(&name) {
            config.groups.push(name);
        }
    }
    state.save().await?;
    Ok(())
}

/// Delete a group (moves connections to ungrouped)
#[tauri::command]
pub async fn delete_group(state: State<'_, Arc<ConfigState>>, name: String) -> Result<(), String> {
    {
        let mut config = state.config.write();
        config.groups.retain(|g| g != &name);

        // Move connections to ungrouped
        for conn in &mut config.connections {
            if conn.group.as_ref() == Some(&name) {
                conn.group = None;
            }
        }
    }
    state.save().await?;
    Ok(())
}

/// Response from get_saved_connection_for_connect
/// Contains all info needed to connect (including credentials from keychain)
#[derive(Debug, Serialize)]
pub struct SavedConnectionForConnect {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
    pub passphrase: Option<String>,
    pub name: String,
    pub agent_forwarding: bool,
    pub proxy_chain: Vec<ProxyHopForConnect>,
}

#[derive(Debug, Serialize)]
pub struct ProxyHopForConnect {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
    pub passphrase: Option<String>,
    pub agent_forwarding: bool,
}

/// Get saved connection with credentials for connecting
/// This retrieves passwords from keychain so frontend can call connect_v2
#[tauri::command]
pub async fn get_saved_connection_for_connect(
    state: State<'_, Arc<ConfigState>>,
    id: String,
) -> Result<SavedConnectionForConnect, String> {
    let config = state.config.read();
    let conn = config.get_connection(&id).ok_or("Connection not found")?;

    // Convert main auth
    let (auth_type, password, key_path, _cert_path, passphrase) = match &conn.auth {
        SavedAuth::Password { keychain_id } => {
            let pwd = keychain_id
                .as_ref()
                .and_then(|kc_id| state.keychain.get(kc_id).ok());
            ("password".to_string(), pwd, None, None, None)
        }
        SavedAuth::Key {
            key_path,
            has_passphrase,
            passphrase_keychain_id,
        } => {
            let passphrase = if *has_passphrase {
                passphrase_keychain_id
                    .as_ref()
                    .and_then(|kc_id| state.keychain.get(kc_id).ok())
            } else {
                None
            };
            (
                "key".to_string(),
                None,
                Some(key_path.clone()),
                None,
                passphrase,
            )
        }
        SavedAuth::Certificate {
            key_path,
            cert_path,
            has_passphrase,
            passphrase_keychain_id,
        } => {
            let passphrase = if *has_passphrase {
                passphrase_keychain_id
                    .as_ref()
                    .and_then(|kc_id| state.keychain.get(kc_id).ok())
            } else {
                None
            };
            (
                "certificate".to_string(),
                None,
                Some(key_path.clone()),
                Some(cert_path.clone()),
                passphrase,
            )
        }
        SavedAuth::Agent => ("agent".to_string(), None, None, None, None),
    };

    // Convert proxy_chain
    let proxy_chain: Vec<ProxyHopForConnect> = conn
        .proxy_chain
        .iter()
        .map(|hop| {
            let (hop_auth_type, hop_password, hop_key_path, hop_cert_path, hop_passphrase) =
                match &hop.auth {
                    SavedAuth::Password { keychain_id } => {
                        let pwd = keychain_id
                            .as_ref()
                            .and_then(|kc_id| state.keychain.get(kc_id).ok());
                        ("password".to_string(), pwd, None, None, None)
                    }
                    SavedAuth::Key {
                        key_path,
                        passphrase_keychain_id,
                        ..
                    } => {
                        let passphrase = passphrase_keychain_id
                            .as_ref()
                            .and_then(|kc_id| state.keychain.get(kc_id).ok());
                        (
                            "key".to_string(),
                            None,
                            Some(key_path.clone()),
                            None,
                            passphrase,
                        )
                    }
                    SavedAuth::Certificate {
                        key_path,
                        cert_path,
                        passphrase_keychain_id,
                        ..
                    } => {
                        let passphrase = passphrase_keychain_id
                            .as_ref()
                            .and_then(|kc_id| state.keychain.get(kc_id).ok());
                        (
                            "certificate".to_string(),
                            None,
                            Some(key_path.clone()),
                            Some(cert_path.clone()),
                            passphrase,
                        )
                    }
                    SavedAuth::Agent => ("agent".to_string(), None, None, None, None),
                };

            ProxyHopForConnect {
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth_type: hop_auth_type,
                password: hop_password,
                key_path: hop_key_path,
                cert_path: hop_cert_path,
                passphrase: hop_passphrase,
                agent_forwarding: hop.agent_forwarding,
            }
        })
        .collect();

    Ok(SavedConnectionForConnect {
        host: conn.host.clone(),
        port: conn.port,
        username: conn.username.clone(),
        auth_type,
        password,
        key_path,
        passphrase,
        name: conn.name.clone(),
        agent_forwarding: conn.options.agent_forwarding,
        proxy_chain,
    })
}

// ============ AI API Key Commands (Legacy compat → routes to ai_keychain) ============

/// Legacy provider ID used when the old single-key API is called.
/// Maps to the built-in OpenAI provider ("builtin-openai").
const LEGACY_PROVIDER_ID: &str = "builtin-openai";

/// Set AI API key — legacy compat, routes to OS keychain under `builtin-openai`.
#[tauri::command]
pub async fn set_ai_api_key(
    api_key: String,
    state: State<'_, Arc<ConfigState>>,
) -> Result<(), String> {
    if api_key.is_empty() {
        tracing::info!("[legacy] Deleting AI API key for {}", LEGACY_PROVIDER_ID);
        if let Err(e) = state.ai_keychain.delete(LEGACY_PROVIDER_ID) {
            tracing::debug!("[legacy] Keychain delete (may not exist): {}", e);
        }
    } else {
        tracing::info!(
            "[legacy] Storing AI API key in keychain for {} (length: {})",
            LEGACY_PROVIDER_ID,
            api_key.len()
        );
        state
            .ai_keychain
            .store(LEGACY_PROVIDER_ID, &api_key)
            .map_err(|e| format!("Failed to store API key: {}", e))?;
    }
    Ok(())
}

/// Get AI API key — legacy compat, reads from OS keychain under `builtin-openai`.
#[tauri::command]
pub async fn get_ai_api_key(state: State<'_, Arc<ConfigState>>) -> Result<Option<String>, String> {
    match state.ai_keychain.get(LEGACY_PROVIDER_ID) {
        Ok(key) => Ok(Some(key)),
        Err(_) => Ok(None),
    }
}

/// Check if AI API key exists — legacy compat.
#[tauri::command]
pub async fn has_ai_api_key(state: State<'_, Arc<ConfigState>>) -> Result<bool, String> {
    Ok(state
        .ai_keychain
        .exists(LEGACY_PROVIDER_ID)
        .unwrap_or(false))
}

/// Delete AI API key — legacy compat.
#[tauri::command]
pub async fn delete_ai_api_key(state: State<'_, Arc<ConfigState>>) -> Result<(), String> {
    if let Err(e) = state.ai_keychain.delete(LEGACY_PROVIDER_ID) {
        tracing::debug!("[legacy] Keychain delete (may not exist): {}", e);
    }
    tracing::info!("[legacy] AI API key deleted for {}", LEGACY_PROVIDER_ID);
    Ok(())
}

// ============ AI Multi-Provider API Key Commands (OS Keychain) ============

/// Attempt to migrate a provider key from legacy XOR vault to OS keychain.
/// Called lazily on first access. Returns the key if migration succeeded.
fn try_migrate_vault_to_keychain(
    app_handle: &tauri::AppHandle,
    ai_keychain: &Keychain,
    provider_id: &str,
) -> Option<String> {
    let app_data_dir = match app_handle.path().app_data_dir() {
        Ok(d) => d,
        Err(_) => return None,
    };
    let vault = AiProviderVault::new(app_data_dir);

    if !vault.exists(provider_id) {
        return None;
    }

    match vault.load(provider_id) {
        Ok(key) => {
            tracing::info!(
                "Migrating AI key for provider {} from vault to keychain",
                provider_id
            );
            // Store in keychain
            match ai_keychain.store(provider_id, &key) {
                Ok(()) => {
                    // Delete vault file after successful migration
                    if let Err(e) = vault.delete(provider_id) {
                        tracing::warn!(
                            "Failed to delete vault file after migration for {}: {}",
                            provider_id,
                            e
                        );
                    }
                    tracing::info!(
                        "Successfully migrated AI key for provider {} to keychain",
                        provider_id
                    );
                    // Extract from Zeroizing for the cache (intentional)
                    Some((*key).clone())
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to store provider {} key in keychain: {}",
                        provider_id,
                        e
                    );
                    // Return the key anyway so the user isn't blocked
                    Some((*key).clone())
                }
            }
        }
        Err(e) => {
            tracing::warn!(
                "Failed to read vault for provider {} during migration: {}",
                provider_id,
                e
            );
            None
        }
    }
}

/// Sync AI provider configurations from frontend settings.
/// Called on app startup and whenever AI settings change.
#[tauri::command]
pub async fn sync_ai_providers(
    state: State<'_, Arc<ConfigState>>,
    providers: Vec<AiProviderConfig>,
    active_provider_id: Option<String>,
) -> Result<(), String> {
    let mut lock = state.ai_providers.write();
    *lock = (providers, active_provider_id);
    tracing::debug!(
        "AI providers synced from frontend ({} providers)",
        lock.0.len()
    );
    Ok(())
}

/// Set API key for a specific AI provider — stored in OS keychain
#[tauri::command]
pub async fn set_ai_provider_api_key(
    state: State<'_, Arc<ConfigState>>,
    provider_id: String,
    api_key: String,
) -> Result<(), String> {
    if api_key.is_empty() {
        state
            .ai_keychain
            .delete(&provider_id)
            .map_err(|e| format!("Failed to delete provider key: {}", e))?;
        // Evict from session cache
        state.api_key_cache.write().remove(&provider_id);
    } else {
        state
            .ai_keychain
            .store(&provider_id, &api_key)
            .map_err(|e| format!("Failed to save provider key to keychain: {}", e))?;
        // Update session cache so next read doesn't re-trigger Touch ID
        state
            .api_key_cache
            .write()
            .insert(provider_id.clone(), api_key);
    }
    tracing::info!(
        "AI provider key for {} saved to system keychain",
        provider_id
    );
    Ok(())
}

/// Get API key for a specific AI provider — reads from OS keychain, migrates from vault if needed
#[tauri::command]
pub async fn get_ai_provider_api_key(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<ConfigState>>,
    provider_id: String,
) -> Result<Option<String>, String> {
    // Step 0: Check in-memory cache — avoids repeated Touch ID prompts within
    // the same app session. The cache is populated after the first successful
    // keychain read (which may require biometric authentication on macOS).
    {
        let cache = state.api_key_cache.read();
        if let Some(cached_key) = cache.get(&provider_id) {
            tracing::debug!(
                "AI provider key for {} served from session cache",
                provider_id
            );
            return Ok(Some(cached_key.clone()));
        }
    }

    // Step 1: Try keychain (may trigger Touch ID on macOS)
    match state.ai_keychain.get(&provider_id) {
        Ok(key) => {
            tracing::debug!(
                "AI provider key for {} found in keychain (len={})",
                provider_id,
                key.len()
            );
            // Populate cache so subsequent calls skip Touch ID
            state.api_key_cache.write().insert(provider_id, key.clone());
            return Ok(Some(key));
        }
        Err(e) => {
            // Only continue if it's a "not found" error
            let is_not_found = matches!(&e, KeychainError::NotFound(_))
                || e.to_string().to_lowercase().contains("no entry");
            if !is_not_found {
                tracing::warn!("Keychain error for provider {}: {}", provider_id, e);
            }
        }
    }

    // Step 2: Try lazy migration from vault
    if let Some(key) = try_migrate_vault_to_keychain(&app_handle, &state.ai_keychain, &provider_id)
    {
        state.api_key_cache.write().insert(provider_id, key.clone());
        return Ok(Some(key));
    }

    Ok(None)
}

/// Check if API key exists for a specific AI provider
#[tauri::command]
pub async fn has_ai_provider_api_key(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<ConfigState>>,
    provider_id: String,
) -> Result<bool, String> {
    // Check keychain (uses biometric_exists on macOS — no Touch ID prompt)
    match state.ai_keychain.exists(&provider_id) {
        Ok(true) => return Ok(true),
        Ok(false) => {}
        Err(_) => {}
    }

    // Check if vault file exists (pending migration)
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    let vault = AiProviderVault::new(app_data_dir);
    Ok(vault.exists(&provider_id))
}

/// Delete API key for a specific AI provider
#[tauri::command]
pub async fn delete_ai_provider_api_key(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<ConfigState>>,
    provider_id: String,
) -> Result<(), String> {
    // Delete from keychain
    if let Err(e) = state.ai_keychain.delete(&provider_id) {
        tracing::debug!(
            "Keychain delete for provider {} (may not exist): {}",
            provider_id,
            e
        );
    }

    // Also clean up any remaining vault file
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    let vault = AiProviderVault::new(app_data_dir);
    if let Err(e) = vault.delete(&provider_id) {
        tracing::debug!(
            "Vault delete for provider {} (may not exist): {}",
            provider_id,
            e
        );
    }

    tracing::info!(
        "AI provider key for {} deleted from all storage locations",
        provider_id
    );
    Ok(())
}

/// List all provider IDs that have stored API keys
/// Note: This checks both keychain and legacy vault files
#[tauri::command]
pub async fn list_ai_provider_keys(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<ConfigState>>,
) -> Result<Vec<String>, String> {
    let mut providers = std::collections::HashSet::new();

    // Check legacy vault files (will be migrated on next access)
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    let vault = AiProviderVault::new(app_data_dir);
    if let Ok(vault_providers) = vault.list_providers() {
        for p in vault_providers {
            providers.insert(p);
        }
    }

    // Check known provider IDs in keychain (uses exists() to avoid Touch ID prompts)
    // Since keychain doesn't support enumeration, we probe known provider IDs
    let known_ids = [
        "builtin-openai",
        "builtin-anthropic",
        "builtin-gemini",
        "builtin-ollama",
    ];
    for id in &known_ids {
        if state.ai_keychain.exists(id).unwrap_or(false) {
            providers.insert(id.to_string());
        }
    }

    Ok(providers.into_iter().collect())
}

// ─── Data Directory Management ──────────────────────────────────────────────

#[derive(Serialize)]
pub struct DataDirInfo {
    pub path: String,
    pub is_custom: bool,
    pub default_path: String,
}

/// Get current data directory information
#[tauri::command]
pub async fn get_data_directory() -> Result<DataDirInfo, String> {
    let (effective, is_custom) =
        crate::config::storage::get_data_dir_info().map_err(|e| e.to_string())?;
    let default = crate::config::storage::default_dir().map_err(|e| e.to_string())?;

    Ok(DataDirInfo {
        path: effective.to_string_lossy().to_string(),
        is_custom,
        default_path: default.to_string_lossy().to_string(),
    })
}

/// Set a custom data directory. Writes to bootstrap.json.
/// Returns true if the path was changed (app restart required).
#[tauri::command]
pub async fn set_data_directory(new_path: String) -> Result<bool, String> {
    let path = std::path::PathBuf::from(&new_path);

    if !path.is_absolute() {
        return Err("Data directory must be an absolute path".to_string());
    }

    // Reject paths containing ".." to prevent traversal
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Data directory path must not contain '..'".to_string());
    }

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&path).map_err(|e| format!("Failed to create directory: {}", e))?;

    // Canonicalize after creation to resolve symlinks
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {}", e))?;

    // Check directory is writable using a unique temp file
    let test_filename = format!(".oxideterm_test_{}", std::process::id());
    let test_file = canonical.join(&test_filename);
    std::fs::write(&test_file, b"test").map_err(|e| format!("Directory is not writable: {}", e))?;
    if let Err(e) = std::fs::remove_file(&test_file) {
        tracing::warn!("Failed to remove write test file {:?}: {}", test_file, e);
    }

    let canonical_str = canonical.to_string_lossy().to_string();
    let bootstrap = crate::config::storage::BootstrapConfig::new_with_data_dir(canonical_str);
    tokio::task::spawn_blocking(move || crate::config::storage::save_bootstrap_config(&bootstrap))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
        .map_err(|e| e.to_string())?;

    Ok(true)
}

/// Reset data directory to default. Removes data_dir from bootstrap.json.
#[tauri::command]
pub async fn reset_data_directory() -> Result<bool, String> {
    let bootstrap = crate::config::storage::BootstrapConfig::default();
    tokio::task::spawn_blocking(move || crate::config::storage::save_bootstrap_config(&bootstrap))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// Open the log directory in the system file manager
#[tauri::command]
pub async fn open_log_directory(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let log_dir = crate::config::storage::log_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&log_dir)
        .map_err(|e| format!("Failed to create log directory: {}", e))?;
    let path_str = log_dir.to_string_lossy().to_string();
    app.opener()
        .reveal_item_in_dir(&path_str)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Check if a directory already contains OxideTerm data files
#[tauri::command]
pub async fn check_data_directory(path: String) -> Result<DataDirCheck, String> {
    let dir = std::path::PathBuf::from(&path);
    if !dir.is_dir() {
        return Ok(DataDirCheck {
            has_existing_data: false,
            files_found: Vec::new(),
        });
    }

    let known_files = [
        "connections.json",
        "state.redb",
        "chat_history.redb",
        "agent_history.redb",
        "sftp_progress.redb",
        "rag_index.redb",
        "plugin-config.json",
        "bootstrap.json",
        "topology_edges.json",
    ];

    let mut found = Vec::new();
    for name in &known_files {
        if dir.join(name).exists() {
            found.push(name.to_string());
        }
    }
    // Check known subdirectories
    for subdir in &["logs", "plugins", "rag_hnsw.bin"] {
        if dir.join(subdir).exists() {
            found.push(subdir.to_string());
        }
    }

    Ok(DataDirCheck {
        has_existing_data: !found.is_empty(),
        files_found: found,
    })
}

#[derive(Serialize)]
pub struct DataDirCheck {
    pub has_existing_data: bool,
    pub files_found: Vec<String>,
}
