//! Configuration Commands
//!
//! Tauri commands for managing saved connections and SSH config import.

use crate::config::{
    default_ssh_config_path, parse_ssh_config, AiProviderVault, ConfigFile, ConfigStorage,
    Keychain, KeychainError, ProxyHopConfig, SavedAuth, SavedConnection, SshConfigHost,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, State};

/// Service name for AI provider API keys in system keychain
const AI_KEYCHAIN_SERVICE: &str = "com.oxideterm.ai";

/// Shared config state
pub struct ConfigState {
    storage: ConfigStorage,
    config: RwLock<ConfigFile>,
    keychain: Keychain,
    ai_keychain: Keychain,
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
            ai_keychain: Keychain::with_service(AI_KEYCHAIN_SERVICE),
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
        SavedAuth::Certificate { key_path, .. } => {
            ("certificate".to_string(), Some(key_path.clone()))
        }
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
                    Some(key)
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to store provider {} key in keychain: {}",
                        provider_id,
                        e
                    );
                    // Return the key anyway so the user isn't blocked
                    Some(key)
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
    } else {
        state
            .ai_keychain
            .store(&provider_id, &api_key)
            .map_err(|e| format!("Failed to save provider key to keychain: {}", e))?;
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
    // Step 1: Try keychain first
    match state.ai_keychain.get(&provider_id) {
        Ok(key) => {
            tracing::debug!(
                "AI provider key for {} found in keychain (len={})",
                provider_id,
                key.len()
            );
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
    // Check keychain
    match state.ai_keychain.get(&provider_id) {
        Ok(_) => return Ok(true),
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

    // Check known provider IDs in keychain
    // Since keychain doesn't support enumeration, we probe known provider IDs
    let known_ids = [
        "builtin-openai",
        "builtin-anthropic",
        "builtin-gemini",
        "builtin-ollama",
    ];
    for id in &known_ids {
        if state.ai_keychain.get(id).is_ok() {
            providers.insert(id.to_string());
        }
    }

    Ok(providers.into_iter().collect())
}
