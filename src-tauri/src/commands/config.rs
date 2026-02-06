//! Configuration Commands
//!
//! Tauri commands for managing saved connections and SSH config import.

use crate::config::{
    default_ssh_config_path, parse_ssh_config, AiProviderVault, AiVault, ConfigFile, ConfigStorage,
    Keychain, ProxyHopConfig, SavedAuth, SavedConnection, SshConfigHost,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, State};

/// Shared config state
pub struct ConfigState {
    storage: ConfigStorage,
    config: RwLock<ConfigFile>,
    keychain: Keychain,
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
    pub auth_type: String, // "password", "key", "agent"
    pub key_path: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub color: Option<String>,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_chain: Vec<ProxyHopInfo>,
}

/// Helper to convert SavedAuth to (auth_type, key_path) tuple
fn auth_to_info(auth: &SavedAuth) -> (String, Option<String>) {
    match auth {
        SavedAuth::Password { .. } => ("password".to_string(), None),
        SavedAuth::Key { key_path, .. } => ("key".to_string(), Some(key_path.clone())),
        SavedAuth::Certificate { key_path, .. } => ("certificate".to_string(), Some(key_path.clone())),
        SavedAuth::Agent => ("agent".to_string(), None),
    }
}

impl From<&SavedConnection> for ConnectionInfo {
    fn from(conn: &SavedConnection) -> Self {
        let (auth_type, key_path) = auth_to_info(&conn.auth);

        // Convert proxy_chain to ProxyHopInfo (without sensitive data)
        let proxy_chain: Vec<ProxyHopInfo> = conn
            .proxy_chain
            .iter()
            .map(|hop| {
                let (hop_auth_type, hop_key_path) = auth_to_info(&hop.auth);
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
    pub auth_type: String,        // "password", "key", "agent"
    pub password: Option<String>, // Only for password auth
    pub key_path: Option<String>, // Only for key auth
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub jump_host: Option<String>, // Legacy jump host for backward compatibility
    pub proxy_chain: Option<Vec<ProxyHopRequest>>, // Multi-hop proxy chain
}

/// Request for a single proxy hop in the chain
#[derive(Debug, Clone, Deserialize)]
pub struct ProxyHopRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,          // "password", "key", "agent", "default_key"
    pub password: Option<String>,   // Only for password auth
    pub key_path: Option<String>,   // Only for key auth
    pub passphrase: Option<String>, // Passphrase for encrypted keys
}

/// SSH config host info for frontend
#[derive(Debug, Clone, Serialize)]
pub struct SshHostInfo {
    pub alias: String,
    pub hostname: String,
    pub user: Option<String>,
    pub port: u16,
    pub identity_file: Option<String>,
}

impl From<&SshConfigHost> for SshHostInfo {
    fn from(host: &SshConfigHost) -> Self {
        Self {
            alias: host.alias.clone(),
            hostname: host.effective_hostname().to_string(),
            user: host.user.clone(),
            port: host.effective_port(),
            identity_file: host.identity_file.clone(),
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
                            )
                        }
                    };

                    proxy_chain.push(ProxyHopConfig {
                        host: jump_conn.host.clone(),
                        port: jump_conn.port,
                        username: jump_conn.username.clone(),
                        auth: hop_config,
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
                            SavedAuth::Password { keychain_id: kc_id }
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
                            let key_auth =
                                KeyAuth::from_default_locations(hop_req.passphrase.as_deref())
                                    .map_err(|e| {
                                        format!("No SSH key found for proxy hop: {}", e)
                                    })?;

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

            if let Some(ref password) = request.password {
                let keychain_id = format!("oxide_conn_{}", uuid::Uuid::new_v4());
                state
                    .keychain
                    .store(&keychain_id, password)
                    .map_err(|e| e.to_string())?;
                conn.auth = SavedAuth::Password { keychain_id };
            } else if let Some(ref key_path) = request.key_path {
                conn.auth = SavedAuth::Key {
                    key_path: key_path.clone(),
                    has_passphrase: false,
                    passphrase_keychain_id: None,
                };
            } else {
                conn.auth = SavedAuth::Agent;
            }

            conn.last_used_at = Some(chrono::Utc::now());

            conn.clone()
        } else {
            let auth = if let Some(ref password) = request.password {
                let keychain_id = format!("oxide_conn_{}", uuid::Uuid::new_v4());
                state
                    .keychain
                    .store(&keychain_id, password)
                    .map_err(|e| e.to_string())?;
                SavedAuth::Password { keychain_id }
            } else if let Some(ref key_path) = request.key_path {
                SavedAuth::Key {
                    key_path: key_path.clone(),
                    has_passphrase: false,
                    passphrase_keychain_id: None,
                }
            } else {
                SavedAuth::Agent
            };

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
                            SavedAuth::Password { keychain_id: kc_id }
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
                            let key_auth =
                                KeyAuth::from_default_locations(hop_req.passphrase.as_deref())
                                    .map_err(|e| {
                                        format!("No SSH key found for proxy hop: {}", e)
                                    })?;

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
                options: Default::default(),
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

/// Delete a connection
#[tauri::command]
pub async fn delete_connection(
    state: State<'_, Arc<ConfigState>>,
    id: String,
) -> Result<(), String> {
    {
        let mut config = state.config.write();

        // Delete keychain entry if password auth
        if let Some(conn) = config.get_connection(&id) {
            if let SavedAuth::Password { keychain_id } = &conn.auth {
                let _ = state.keychain.delete(keychain_id);
            }
        }

        config
            .remove_connection(&id)
            .ok_or("Connection not found")?;
    } // config lock dropped here

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
        SavedAuth::Password { keychain_id } => {
            state.keychain.get(keychain_id).map_err(|e| e.to_string())
        }
        _ => Err("Connection does not use password auth".to_string()),
    }
}

/// Import hosts from SSH config
#[tauri::command]
pub async fn list_ssh_config_hosts() -> Result<Vec<SshHostInfo>, String> {
    let hosts = parse_ssh_config(None).await.map_err(|e| e.to_string())?;
    Ok(hosts.iter().map(SshHostInfo::from).collect())
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
            let pwd = state.keychain.get(keychain_id).map_err(|e| e.to_string())?;
            ("password".to_string(), Some(pwd), None, None, None)
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
            ("key".to_string(), None, Some(key_path.clone()), None, passphrase)
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
            ("certificate".to_string(), None, Some(key_path.clone()), Some(cert_path.clone()), passphrase)
        }
        SavedAuth::Agent => ("agent".to_string(), None, None, None, None),
    };

    // Convert proxy_chain
    let proxy_chain: Vec<ProxyHopForConnect> = conn
        .proxy_chain
        .iter()
        .map(|hop| {
            let (hop_auth_type, hop_password, hop_key_path, hop_cert_path, hop_passphrase) = match &hop.auth {
                SavedAuth::Password { keychain_id } => {
                    let pwd = state.keychain.get(keychain_id).ok();
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
                    ("key".to_string(), None, Some(key_path.clone()), None, passphrase)
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
                    ("certificate".to_string(), None, Some(key_path.clone()), Some(cert_path.clone()), passphrase)
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
        proxy_chain,
    })
}

// ============ AI API Key Commands (Vault-based with Keychain migration) ============

/// Fixed keychain ID for AI API key (for migration only)
const AI_API_KEY_KEYCHAIN_ID: &str = "oxideterm-ai-api-key";

/// Helper to get the AI vault instance
fn get_ai_vault(app_handle: &tauri::AppHandle) -> Result<AiVault, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    Ok(AiVault::new(app_data_dir))
}

/// Set AI API key - saves to local encrypted vault file
#[tauri::command]
pub async fn set_ai_api_key(
    app_handle: tauri::AppHandle,
    api_key: String,
) -> Result<(), String> {
    let vault = get_ai_vault(&app_handle)?;
    
    if api_key.is_empty() {
        tracing::info!("Deleting AI API key");
        // Delete from vault
        vault.delete().map_err(|e| e.to_string())?;
    } else {
        tracing::info!("Storing AI API key in vault (length: {})", api_key.len());
        vault.save(&api_key).map_err(|e| format!("Failed to save API key to vault: {}", e))?;
        tracing::info!("AI API key stored successfully in vault");
    }
    Ok(())
}

/// Get AI API key - reads from vault, migrates from keychain if needed
#[tauri::command]
pub async fn get_ai_api_key(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<ConfigState>>,
) -> Result<Option<String>, String> {
    let vault = get_ai_vault(&app_handle)?;
    
    // Step 1: Try to load from vault first
    match vault.load() {
        Ok(key) => {
            tracing::debug!("AI API key found in vault (length: {})", key.len());
            return Ok(Some(key));
        }
        Err(crate::config::VaultError::NotFound) => {
            tracing::debug!("AI API key not in vault, checking keychain for migration...");
        }
        Err(e) => {
            tracing::error!("Failed to read vault: {}", e);
            return Err(format!("Failed to read vault: {}", e));
        }
    }
    
    // Step 2: Not in vault - try keychain migration
    match state.get_keychain_value(AI_API_KEY_KEYCHAIN_ID) {
        Ok(key) => {
            tracing::info!("Found API key in keychain, migrating to vault...");
            
            // Migrate to vault
            if let Err(e) = vault.save(&key) {
                tracing::error!("Failed to migrate key to vault: {}", e);
                // Still return the key even if migration fails
                return Ok(Some(key));
            }
            
            // Delete from keychain after successful migration
            if let Err(e) = state.delete_keychain_value(AI_API_KEY_KEYCHAIN_ID) {
                tracing::warn!("Failed to delete old keychain entry: {}", e);
                // Continue anyway - the vault now has the key
            }
            
            tracing::info!("Successfully migrated API key from keychain to vault");
            Ok(Some(key))
        }
        Err(e) => {
            let e_lower = e.to_lowercase();
            if e_lower.contains("not found") || e_lower.contains("noentry") || e_lower.contains("no entry") {
                tracing::debug!("AI API key not found in vault or keychain");
                Ok(None)
            } else {
                // Keychain error, but vault was already checked - return None
                tracing::warn!("Keychain error during migration check: {}", e);
                Ok(None)
            }
        }
    }
}

/// Check if AI API key exists (in vault or keychain)
#[tauri::command]
pub async fn has_ai_api_key(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<ConfigState>>,
) -> Result<bool, String> {
    let vault = get_ai_vault(&app_handle)?;
    
    // Check vault first
    if vault.exists() {
        tracing::debug!("AI API key exists in vault");
        return Ok(true);
    }
    
    // Fallback: check keychain (for users who haven't migrated yet)
    match state.get_keychain_value(AI_API_KEY_KEYCHAIN_ID) {
        Ok(_) => {
            tracing::debug!("AI API key exists in keychain (pending migration)");
            Ok(true)
        }
        Err(e) => {
            let e_lower = e.to_lowercase();
            if e_lower.contains("not found") || e_lower.contains("noentry") || e_lower.contains("no entry") {
                tracing::debug!("AI API key does not exist");
                Ok(false)
            } else {
                // Keychain error, but we can still say key doesn't exist if vault check passed
                tracing::warn!("Keychain check error: {}", e);
                Ok(false)
            }
        }
    }
}

/// Delete AI API key - clears both vault and keychain (for clean uninstall)
#[tauri::command]
pub async fn delete_ai_api_key(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<ConfigState>>,
) -> Result<(), String> {
    let vault = get_ai_vault(&app_handle)?;
    
    // Delete from vault
    if let Err(e) = vault.delete() {
        tracing::warn!("Failed to delete from vault: {}", e);
    }
    
    // Also try to delete from keychain (clean up legacy)
    if let Err(e) = state.delete_keychain_value(AI_API_KEY_KEYCHAIN_ID) {
        tracing::debug!("Keychain delete (may not exist): {}", e);
    }
    
    tracing::info!("AI API key deleted from all storage locations");
    Ok(())
}

// ============ AI Multi-Provider API Key Commands ============

/// Helper to get the AI provider vault instance
fn get_ai_provider_vault(app_handle: &tauri::AppHandle) -> Result<AiProviderVault, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    Ok(AiProviderVault::new(app_data_dir))
}

/// Set API key for a specific AI provider
#[tauri::command]
pub async fn set_ai_provider_api_key(
    app_handle: tauri::AppHandle,
    provider_id: String,
    api_key: String,
) -> Result<(), String> {
    let vault = get_ai_provider_vault(&app_handle)?;

    if api_key.is_empty() {
        vault.delete(&provider_id).map_err(|e| format!("Failed to delete provider key: {}", e))?;
    } else {
        vault.save(&provider_id, &api_key).map_err(|e| format!("Failed to save provider key: {}", e))?;
    }

    Ok(())
}

/// Get API key for a specific AI provider
#[tauri::command]
pub async fn get_ai_provider_api_key(
    app_handle: tauri::AppHandle,
    provider_id: String,
) -> Result<Option<String>, String> {
    let vault = get_ai_provider_vault(&app_handle)?;

    match vault.load(&provider_id) {
        Ok(key) => Ok(Some(key)),
        Err(crate::config::VaultError::NotFound) => Ok(None),
        Err(e) => Err(format!("Failed to load provider key: {}", e)),
    }
}

/// Check if API key exists for a specific AI provider
#[tauri::command]
pub async fn has_ai_provider_api_key(
    app_handle: tauri::AppHandle,
    provider_id: String,
) -> Result<bool, String> {
    let vault = get_ai_provider_vault(&app_handle)?;
    Ok(vault.exists(&provider_id))
}

/// Delete API key for a specific AI provider
#[tauri::command]
pub async fn delete_ai_provider_api_key(
    app_handle: tauri::AppHandle,
    provider_id: String,
) -> Result<(), String> {
    let vault = get_ai_provider_vault(&app_handle)?;
    vault.delete(&provider_id).map_err(|e| format!("Failed to delete provider key: {}", e))?;
    Ok(())
}

/// List all provider IDs that have stored API keys
#[tauri::command]
pub async fn list_ai_provider_keys(
    app_handle: tauri::AppHandle,
) -> Result<Vec<String>, String> {
    let vault = get_ai_provider_vault(&app_handle)?;
    vault.list_providers().map_err(|e| format!("Failed to list provider keys: {}", e))
}
