// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Configuration Commands
//!
//! Tauri commands for managing saved connections and SSH config import.

use crate::config::{
    AiProviderVault, CONFIG_ENCRYPTION_KEY_LEN, ConfigFile, ConfigStorage, ConfigStorageFormat,
    Keychain, KeychainError, ProxyHopConfig, SavedAuth, SavedConnection, SshConfigHost,
    default_ssh_config_path, parse_ssh_config,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use parking_lot::RwLock;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use zeroize::Zeroizing;

use super::forwarding::ForwardingRegistry;

/// Service name for AI provider API keys in system keychain
const AI_KEYCHAIN_SERVICE: &str = "com.oxideterm.ai";
const CONFIG_KEYCHAIN_SERVICE: &str = "com.oxideterm.config";
const CONFIG_KEYCHAIN_ID: &str = "local-config-master-key";

fn decode_config_encryption_key(secret: &str) -> Result<[u8; CONFIG_ENCRYPTION_KEY_LEN], String> {
    let decoded = BASE64
        .decode(secret)
        .map_err(|e| format!("Failed to decode local config key: {}", e))?;
    decoded.try_into().map_err(|_| {
        format!(
            "Invalid local config key length: expected {} bytes",
            CONFIG_ENCRYPTION_KEY_LEN
        )
    })
}

fn load_config_encryption_key(
    keychain: &Keychain,
) -> Result<Option<[u8; CONFIG_ENCRYPTION_KEY_LEN]>, String> {
    match keychain.get(CONFIG_KEYCHAIN_ID) {
        Ok(secret) => decode_config_encryption_key(&secret).map(Some),
        Err(KeychainError::NotFound(_)) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

fn create_config_encryption_key(
    keychain: &Keychain,
) -> Result<[u8; CONFIG_ENCRYPTION_KEY_LEN], String> {
    let mut key = [0u8; CONFIG_ENCRYPTION_KEY_LEN];
    rand::rngs::OsRng.fill_bytes(&mut key);
    let encoded = Zeroizing::new(BASE64.encode(key));

    keychain
        .store(CONFIG_KEYCHAIN_ID, encoded.as_str())
        .map_err(|e| e.to_string())?;

    Ok(key)
}

fn get_or_create_config_encryption_key(
    keychain: &Keychain,
) -> Result<([u8; CONFIG_ENCRYPTION_KEY_LEN], bool), String> {
    if let Some(existing) = load_config_encryption_key(keychain)? {
        return Ok((existing, false));
    }

    Ok((create_config_encryption_key(keychain)?, true))
}

fn rollback_new_config_key(keychain: &Keychain) {
    if let Err(err) = keychain.delete(CONFIG_KEYCHAIN_ID) {
        tracing::warn!(
            "Failed to roll back newly created local config key after save failure: {}",
            err
        );
    }
}

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
    config_keychain: Keychain,
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
        let config_keychain = Keychain::with_service(CONFIG_KEYCHAIN_SERVICE);
        let loaded = match storage.load_with_key(None).await {
            Ok(loaded) => loaded,
            Err(crate::config::StorageError::MissingEncryptionKey) => {
                let existing_config_key = load_config_encryption_key(&config_keychain)
                    .map_err(|err| {
                        format!(
                            "Unable to unlock encrypted local config because the OS keychain is unavailable: {}",
                            err
                        )
                    })?
                    .ok_or_else(|| {
                        "Encrypted local config found but the OS keychain entry is missing. Restore the keychain entry or recover from backup."
                            .to_string()
                    })?;

                storage
                    .load_with_key(Some(&existing_config_key))
                    .await
                    .map_err(|e| e.to_string())?
            }
            Err(err) => return Err(err.to_string()),
        };

        if loaded.format == ConfigStorageFormat::Plaintext {
            let (config_key, created_key) = get_or_create_config_encryption_key(&config_keychain)
                .map_err(|err| {
                    format!(
                        "Unable to migrate plaintext local config to encrypted storage because the OS keychain is unavailable: {}",
                        err
                    )
                })?;

            if let Err(err) = storage.save_encrypted(&loaded.config, &config_key).await {
                if created_key {
                    rollback_new_config_key(&config_keychain);
                }

                return Err(format!(
                    "Loaded legacy plaintext local config but failed to migrate it to encrypted storage: {}",
                    err
                ));
            }

            tracing::info!(
                "Migrated local config storage from plaintext JSON to encrypted envelope"
            );
        }

        Ok(Self {
            storage,
            config: RwLock::new(loaded.config),
            config_keychain,
            keychain: Keychain::new(),
            ai_keychain: Keychain::with_biometrics(AI_KEYCHAIN_SERVICE),
            api_key_cache: RwLock::new(HashMap::new()),
            ai_providers: RwLock::new((Vec::new(), None)),
        })
    }

    /// Save config to disk
    async fn save(&self) -> Result<(), String> {
        let config = self.config.read().clone();
        let (config_key, created_key) = get_or_create_config_encryption_key(&self.config_keychain)?;

        match self.storage.save_encrypted(&config, &config_key).await {
            Ok(()) => Ok(()),
            Err(err) => {
                if created_key {
                    rollback_new_config_key(&self.config_keychain);
                }

                Err(err.to_string())
            }
        }
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
    pub agent_forwarding: bool,
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
    pub agent_forwarding: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_chain: Vec<ProxyHopInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnectionSyncRecord {
    pub id: String,
    pub revision: String,
    pub updated_at: String,
    pub deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<ConnectionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnectionsSyncSnapshot {
    pub revision: String,
    pub exported_at: String,
    pub records: Vec<SavedConnectionSyncRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySavedConnectionsSyncSnapshotResult {
    pub applied: usize,
    pub skipped: usize,
    pub conflicts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSyncMetadata {
    pub saved_connections_revision: String,
    pub saved_connections_updated_at: String,
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
                    agent_forwarding: hop.agent_forwarding,
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
            agent_forwarding: conn.options.agent_forwarding,
            proxy_chain,
        }
    }
}

fn sha256_hex<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|e| format!("Failed to serialize sync payload: {}", e))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn parse_sync_timestamp(
    value: &str,
    field_name: &str,
) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&chrono::Utc))
        .map_err(|e| format!("Invalid {} '{}': {}", field_name, value, e))
}

fn connection_sync_updated_at(conn: &SavedConnection) -> String {
    conn.updated_at
        .or(conn.last_used_at)
        .unwrap_or(conn.created_at)
        .to_rfc3339()
}

fn build_saved_connection_tombstone_record(
    tombstone: &crate::config::types::DeletedConnectionTombstone,
) -> Result<SavedConnectionSyncRecord, String> {
    let revision = sha256_hex(&(
        tombstone.id.as_str(),
        tombstone.deleted_at.to_rfc3339(),
        true,
    ))?;

    Ok(SavedConnectionSyncRecord {
        id: tombstone.id.clone(),
        revision,
        updated_at: tombstone.deleted_at.to_rfc3339(),
        deleted: true,
        payload: None,
    })
}

fn build_saved_connection_sync_record(
    conn: &SavedConnection,
) -> Result<SavedConnectionSyncRecord, String> {
    let payload = ConnectionInfo::from(conn);
    let revision = sha256_hex(&payload)?;

    Ok(SavedConnectionSyncRecord {
        id: conn.id.clone(),
        revision,
        updated_at: connection_sync_updated_at(conn),
        deleted: false,
        payload: Some(payload),
    })
}

fn build_saved_connections_sync_snapshot(
    config: &ConfigFile,
) -> Result<SavedConnectionsSyncSnapshot, String> {
    let mut records: Vec<SavedConnectionSyncRecord> = config
        .connections
        .iter()
        .map(build_saved_connection_sync_record)
        .collect::<Result<_, _>>()?;
    records.extend(
        config
            .active_connection_tombstones()
            .into_iter()
            .map(build_saved_connection_tombstone_record)
            .collect::<Result<Vec<_>, _>>()?,
    );
    records.sort_by(|left, right| left.id.cmp(&right.id));

    let revision = sha256_hex(
        &records
            .iter()
            .map(|record| (&record.id, &record.revision, record.deleted))
            .collect::<Vec<_>>(),
    )?;

    Ok(SavedConnectionsSyncSnapshot {
        revision,
        exported_at: chrono::Utc::now().to_rfc3339(),
        records,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SavedConnectionsConflictStrategy {
    Skip,
    Replace,
    Merge,
}

impl SavedConnectionsConflictStrategy {
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("skip") {
            "skip" => Ok(Self::Skip),
            "replace" => Ok(Self::Replace),
            "merge" => Ok(Self::Merge),
            other => Err(format!(
                "Unsupported saved connection conflict strategy: {}",
                other
            )),
        }
    }

    fn preserves_local_auth(self) -> bool {
        matches!(self, Self::Merge)
    }
}

#[derive(Debug, Default)]
struct ApplySavedConnectionsSyncSideEffects {
    deleted_connection_ids: Vec<String>,
    keychain_ids_to_delete: Vec<String>,
}

fn build_synced_proxy_chain(
    proxy_chain: &[ProxyHopInfo],
    existing_proxy_chain: Option<&[ProxyHopConfig]>,
    preserve_auth: bool,
    keychain: &Keychain,
) -> Result<Vec<ProxyHopConfig>, String> {
    proxy_chain
        .iter()
        .map(|hop| {
            let existing_auth = if preserve_auth {
                existing_proxy_chain.and_then(|existing| {
                    existing
                        .iter()
                        .find(|candidate| {
                            candidate.host == hop.host
                                && candidate.port == hop.port
                                && candidate.username == hop.username
                        })
                        .map(|candidate| &candidate.auth)
                })
            } else {
                None
            };

            let auth = if let Some(existing_auth) = existing_auth {
                build_saved_auth_for_update(
                    existing_auth,
                    &hop.auth_type,
                    None,
                    hop.key_path.as_deref(),
                    None,
                    keychain,
                )?
            } else {
                build_saved_auth(
                    &hop.auth_type,
                    None,
                    hop.key_path.as_deref(),
                    None,
                    keychain,
                )?
            };

            Ok(ProxyHopConfig {
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth,
                agent_forwarding: hop.agent_forwarding,
            })
        })
        .collect()
}

fn build_saved_connection_from_sync_payload(
    payload: &ConnectionInfo,
    record_updated_at: chrono::DateTime<chrono::Utc>,
    existing: Option<&SavedConnection>,
    preserve_auth: bool,
    keychain: &Keychain,
) -> Result<SavedConnection, String> {
    let auth = if let Some(existing) = existing.filter(|_| preserve_auth) {
        build_saved_auth_for_update(
            &existing.auth,
            &payload.auth_type,
            None,
            payload.key_path.as_deref(),
            payload.cert_path.as_deref(),
            keychain,
        )?
    } else {
        build_saved_auth(
            &payload.auth_type,
            None,
            payload.key_path.as_deref(),
            payload.cert_path.as_deref(),
            keychain,
        )?
    };

    let proxy_chain = build_synced_proxy_chain(
        &payload.proxy_chain,
        existing.map(|value| value.proxy_chain.as_slice()),
        preserve_auth,
        keychain,
    )?;

    Ok(SavedConnection {
        id: payload.id.clone(),
        version: crate::config::CONFIG_VERSION,
        name: payload.name.clone(),
        group: payload.group.clone(),
        host: payload.host.clone(),
        port: payload.port,
        username: payload.username.clone(),
        auth,
        options: crate::config::ConnectionOptions {
            agent_forwarding: payload.agent_forwarding,
            ..Default::default()
        },
        created_at: chrono::DateTime::parse_from_rfc3339(&payload.created_at)
            .map_err(|e| {
                format!(
                    "Invalid connection created_at '{}': {}",
                    payload.created_at, e
                )
            })?
            .with_timezone(&chrono::Utc),
        last_used_at: payload
            .last_used_at
            .as_deref()
            .map(|value| {
                chrono::DateTime::parse_from_rfc3339(value)
                    .map(|time| time.with_timezone(&chrono::Utc))
                    .map_err(|e| format!("Invalid connection last_used_at '{}': {}", value, e))
            })
            .transpose()?,
        updated_at: Some(record_updated_at),
        color: payload.color.clone(),
        tags: payload.tags.clone(),
        proxy_chain,
    })
}

fn apply_saved_connections_snapshot_to_config(
    config: &mut ConfigFile,
    snapshot: &SavedConnectionsSyncSnapshot,
    strategy: SavedConnectionsConflictStrategy,
    keychain: &Keychain,
) -> Result<
    (
        ApplySavedConnectionsSyncSnapshotResult,
        ApplySavedConnectionsSyncSideEffects,
    ),
    String,
> {
    let mut result = ApplySavedConnectionsSyncSnapshotResult {
        applied: 0,
        skipped: 0,
        conflicts: 0,
    };
    let mut side_effects = ApplySavedConnectionsSyncSideEffects::default();

    for record in &snapshot.records {
        let record_updated_at =
            parse_sync_timestamp(&record.updated_at, "saved connection sync updated_at")?;

        if record.deleted {
            if let Some(existing) = config.get_connection(&record.id) {
                let existing_updated_at = parse_sync_timestamp(
                    &connection_sync_updated_at(existing),
                    "local saved connection updated_at",
                )?;

                if existing_updated_at > record_updated_at {
                    result.skipped += 1;
                    result.conflicts += 1;
                    continue;
                }
            }

            if let Some(removed) =
                config.remove_connection_with_tombstone_at(&record.id, record_updated_at)
            {
                side_effects.deleted_connection_ids.push(removed.id.clone());
                side_effects
                    .keychain_ids_to_delete
                    .extend(collect_connection_keychain_ids(&removed));
                result.applied += 1;
            } else if config.upsert_connection_tombstone(record.id.clone(), record_updated_at) {
                result.applied += 1;
            } else {
                result.skipped += 1;
            }
            continue;
        }

        let Some(payload) = &record.payload else {
            result.skipped += 1;
            result.conflicts += 1;
            continue;
        };

        if let Some(tombstone) = config.get_connection_tombstone(&record.id) {
            if tombstone.deleted_at >= record_updated_at {
                result.skipped += 1;
                result.conflicts += 1;
                continue;
            }
        }

        let existing_by_id = config.get_connection(&record.id).cloned();
        let existing_by_name = if existing_by_id.is_none() {
            config
                .connections
                .iter()
                .find(|candidate| candidate.name == payload.name && candidate.id != record.id)
                .cloned()
        } else {
            None
        };

        if existing_by_id.is_none()
            && existing_by_name.is_some()
            && strategy == SavedConnectionsConflictStrategy::Skip
        {
            result.skipped += 1;
            result.conflicts += 1;
            continue;
        }

        if let Some(existing_same_name) = existing_by_name.as_ref() {
            if let Some(removed) = config.remove_connection(&existing_same_name.id) {
                side_effects.deleted_connection_ids.push(removed.id.clone());
            }
        }

        let baseline = existing_by_id.as_ref().or(existing_by_name.as_ref());
        let connection = build_saved_connection_from_sync_payload(
            payload,
            record_updated_at,
            baseline,
            baseline.is_some() && strategy.preserves_local_auth(),
            keychain,
        )?;

        if let Some(existing) = baseline {
            let existing_keychain_ids: HashSet<String> = collect_connection_keychain_ids(existing)
                .into_iter()
                .collect();
            let next_keychain_ids: HashSet<String> = collect_connection_keychain_ids(&connection)
                .into_iter()
                .collect();

            side_effects.keychain_ids_to_delete.extend(
                existing_keychain_ids
                    .difference(&next_keychain_ids)
                    .cloned(),
            );
        }

        if let Some(group) = connection.group.clone() {
            if !config.groups.contains(&group) {
                config.groups.push(group);
            }
        }

        config.add_connection(connection);
        result.applied += 1;
    }

    Ok((result, side_effects))
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

/// Export a structured snapshot of saved connections for plugin-driven sync.
#[tauri::command]
pub async fn export_saved_connections_snapshot(
    state: State<'_, Arc<ConfigState>>,
) -> Result<SavedConnectionsSyncSnapshot, String> {
    let config = state.config.read();
    build_saved_connections_sync_snapshot(&config)
}

/// Get lightweight local sync metadata for saved connections.
#[tauri::command]
pub async fn get_local_sync_metadata(
    state: State<'_, Arc<ConfigState>>,
) -> Result<LocalSyncMetadata, String> {
    let config = state.config.read();
    let snapshot = build_saved_connections_sync_snapshot(&config)?;
    let saved_connections_updated_at = snapshot
        .records
        .iter()
        .map(|record| record.updated_at.clone())
        .max()
        .unwrap_or_else(|| snapshot.exported_at.clone());

    Ok(LocalSyncMetadata {
        saved_connections_revision: snapshot.revision,
        saved_connections_updated_at,
    })
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
    app_handle: tauri::AppHandle,
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

                app_handle
                    .emit("connection:update", "saved")
                    .map_err(|e| format!("Failed to emit connection:update: {}", e))?;
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

            let now = chrono::Utc::now();
            conn.last_used_at = Some(now);
            conn.updated_at = Some(now);

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
                updated_at: Some(chrono::Utc::now()),
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
            updated_at: Some(chrono::Utc::now()),
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

    #[test]
    fn build_saved_connections_sync_snapshot_includes_agent_forwarding() {
        let mut config = ConfigFile::default();
        let mut connection =
            SavedConnection::new_key("Prod", "prod.example.com", 22, "root", "/tmp/id_ed25519");
        connection.options.agent_forwarding = true;
        connection.proxy_chain.push(ProxyHopConfig {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "jump".to_string(),
            auth: SavedAuth::Agent,
            agent_forwarding: true,
        });
        config.add_connection(connection);

        let snapshot = build_saved_connections_sync_snapshot(&config).unwrap();
        let payload = snapshot.records[0].payload.as_ref().unwrap();

        assert!(!snapshot.revision.is_empty());
        assert!(payload.agent_forwarding);
        assert!(payload.proxy_chain[0].agent_forwarding);
    }

    #[test]
    fn apply_saved_connections_snapshot_merge_preserves_password_keychain() {
        let mut config = ConfigFile::default();
        config.add_connection(SavedConnection {
            id: "conn-1".to_string(),
            version: crate::config::CONFIG_VERSION,
            name: "Prod".to_string(),
            group: Some("Ops".to_string()),
            host: "old.example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SavedAuth::Password {
                keychain_id: Some("kc-pass".to_string()),
            },
            options: crate::config::ConnectionOptions {
                agent_forwarding: false,
                ..Default::default()
            },
            created_at: chrono::Utc::now(),
            last_used_at: None,
            updated_at: Some(chrono::Utc::now()),
            color: None,
            tags: Vec::new(),
            proxy_chain: Vec::new(),
        });

        let snapshot = SavedConnectionsSyncSnapshot {
            revision: "rev-1".to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            records: vec![SavedConnectionSyncRecord {
                id: "conn-1".to_string(),
                revision: "rec-1".to_string(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                deleted: false,
                payload: Some(ConnectionInfo {
                    id: "conn-1".to_string(),
                    name: "Prod".to_string(),
                    group: Some("Ops".to_string()),
                    host: "new.example.com".to_string(),
                    port: 2222,
                    username: "deploy".to_string(),
                    auth_type: "password".to_string(),
                    key_path: None,
                    cert_path: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    last_used_at: None,
                    color: Some("#ff0000".to_string()),
                    tags: vec!["prod".to_string()],
                    agent_forwarding: true,
                    proxy_chain: Vec::new(),
                }),
            }],
        };

        let (result, _side_effects) = apply_saved_connections_snapshot_to_config(
            &mut config,
            &snapshot,
            SavedConnectionsConflictStrategy::Merge,
            &Keychain::with_service("com.oxideterm.test"),
        )
        .unwrap();

        let updated = config.get_connection("conn-1").unwrap();
        assert_eq!(result.applied, 1);
        assert_eq!(updated.host, "new.example.com");
        assert_eq!(updated.port, 2222);
        assert_eq!(updated.username, "deploy");
        assert!(updated.options.agent_forwarding);
        assert_eq!(
            updated.auth,
            SavedAuth::Password {
                keychain_id: Some("kc-pass".to_string()),
            }
        );
    }

    #[test]
    fn apply_saved_connections_snapshot_merge_collects_obsolete_proxy_keychain_ids() {
        let mut config = ConfigFile::default();
        config.add_connection(SavedConnection {
            id: "conn-1".to_string(),
            version: crate::config::CONFIG_VERSION,
            name: "Prod".to_string(),
            group: None,
            host: "prod.example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SavedAuth::Agent,
            options: Default::default(),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            updated_at: Some(chrono::Utc::now()),
            color: None,
            tags: Vec::new(),
            proxy_chain: vec![
                ProxyHopConfig {
                    host: "jump-a.example.com".to_string(),
                    port: 22,
                    username: "jump-a".to_string(),
                    auth: SavedAuth::Password {
                        keychain_id: Some("kc-hop-a".to_string()),
                    },
                    agent_forwarding: false,
                },
                ProxyHopConfig {
                    host: "jump-b.example.com".to_string(),
                    port: 22,
                    username: "jump-b".to_string(),
                    auth: SavedAuth::Password {
                        keychain_id: Some("kc-hop-b".to_string()),
                    },
                    agent_forwarding: false,
                },
            ],
        });

        let snapshot = SavedConnectionsSyncSnapshot {
            revision: "rev-2".to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            records: vec![SavedConnectionSyncRecord {
                id: "conn-1".to_string(),
                revision: "rec-2".to_string(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                deleted: false,
                payload: Some(ConnectionInfo {
                    id: "conn-1".to_string(),
                    name: "Prod".to_string(),
                    group: None,
                    host: "prod.example.com".to_string(),
                    port: 22,
                    username: "root".to_string(),
                    auth_type: "agent".to_string(),
                    key_path: None,
                    cert_path: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    last_used_at: None,
                    color: None,
                    tags: Vec::new(),
                    agent_forwarding: false,
                    proxy_chain: vec![ProxyHopInfo {
                        host: "jump-a.example.com".to_string(),
                        port: 22,
                        username: "jump-a".to_string(),
                        auth_type: "password".to_string(),
                        key_path: None,
                        agent_forwarding: false,
                    }],
                }),
            }],
        };

        let (_result, mut side_effects) = apply_saved_connections_snapshot_to_config(
            &mut config,
            &snapshot,
            SavedConnectionsConflictStrategy::Merge,
            &Keychain::with_service("com.oxideterm.test"),
        )
        .unwrap();

        side_effects.keychain_ids_to_delete.sort();
        assert_eq!(
            side_effects.keychain_ids_to_delete,
            vec!["kc-hop-b".to_string()]
        );
    }

    #[test]
    fn build_saved_connections_sync_snapshot_includes_tombstones() {
        let mut config = ConfigFile::default();
        let deleted_at = chrono::Utc::now();
        config.upsert_connection_tombstone("conn-deleted", deleted_at);

        let snapshot = build_saved_connections_sync_snapshot(&config).unwrap();
        let record = snapshot
            .records
            .iter()
            .find(|candidate| candidate.id == "conn-deleted")
            .unwrap();

        assert!(record.deleted);
        assert!(record.payload.is_none());
        assert_eq!(record.updated_at, deleted_at.to_rfc3339());
    }

    #[test]
    fn apply_saved_connections_snapshot_skips_payload_older_than_local_tombstone() {
        let mut config = ConfigFile::default();
        let deleted_at = chrono::Utc::now();
        config.upsert_connection_tombstone("conn-1", deleted_at);

        let snapshot = SavedConnectionsSyncSnapshot {
            revision: "rev-3".to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            records: vec![SavedConnectionSyncRecord {
                id: "conn-1".to_string(),
                revision: "rec-3".to_string(),
                updated_at: (deleted_at - chrono::Duration::minutes(1)).to_rfc3339(),
                deleted: false,
                payload: Some(ConnectionInfo {
                    id: "conn-1".to_string(),
                    name: "Prod".to_string(),
                    group: None,
                    host: "prod.example.com".to_string(),
                    port: 22,
                    username: "root".to_string(),
                    auth_type: "agent".to_string(),
                    key_path: None,
                    cert_path: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    last_used_at: None,
                    color: None,
                    tags: Vec::new(),
                    agent_forwarding: false,
                    proxy_chain: Vec::new(),
                }),
            }],
        };

        let (result, _side_effects) = apply_saved_connections_snapshot_to_config(
            &mut config,
            &snapshot,
            SavedConnectionsConflictStrategy::Merge,
            &Keychain::with_service("com.oxideterm.test"),
        )
        .unwrap();

        assert_eq!(result.applied, 0);
        assert_eq!(result.skipped, 1);
        assert_eq!(result.conflicts, 1);
        assert!(config.get_connection("conn-1").is_none());
        assert_eq!(
            config
                .get_connection_tombstone("conn-1")
                .unwrap()
                .deleted_at,
            deleted_at
        );
    }

    #[test]
    fn apply_saved_connections_snapshot_records_remote_delete_tombstone_when_local_missing() {
        let mut config = ConfigFile::default();
        let deleted_at = chrono::Utc::now();

        let snapshot = SavedConnectionsSyncSnapshot {
            revision: "rev-4".to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            records: vec![SavedConnectionSyncRecord {
                id: "conn-1".to_string(),
                revision: "rec-4".to_string(),
                updated_at: deleted_at.to_rfc3339(),
                deleted: true,
                payload: None,
            }],
        };

        let (result, _side_effects) = apply_saved_connections_snapshot_to_config(
            &mut config,
            &snapshot,
            SavedConnectionsConflictStrategy::Merge,
            &Keychain::with_service("com.oxideterm.test"),
        )
        .unwrap();

        assert_eq!(result.applied, 1);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.conflicts, 0);
        assert!(config.get_connection("conn-1").is_none());
        assert_eq!(
            config
                .get_connection_tombstone("conn-1")
                .unwrap()
                .deleted_at,
            deleted_at
        );
    }
}

/// Delete a connection
#[tauri::command]
pub async fn delete_connection(
    app_handle: tauri::AppHandle,
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

    app_handle
        .emit("connection:update", "deleted")
        .map_err(|e| format!("Failed to emit connection:update: {}", e))?;

    Ok(())
}

/// Apply a structured snapshot of saved connections produced by a sync plugin.
#[tauri::command]
pub async fn apply_saved_connections_snapshot(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<ConfigState>>,
    forwarding_registry: State<'_, Arc<ForwardingRegistry>>,
    snapshot: SavedConnectionsSyncSnapshot,
    conflict_strategy: Option<String>,
) -> Result<ApplySavedConnectionsSyncSnapshotResult, String> {
    let strategy = SavedConnectionsConflictStrategy::parse(conflict_strategy.as_deref())?;

    let (result, side_effects) = {
        let mut config = state.config.write();
        apply_saved_connections_snapshot_to_config(
            &mut config,
            &snapshot,
            strategy,
            &state.keychain,
        )?
    };

    for keychain_id in side_effects.keychain_ids_to_delete {
        let _ = state.keychain.delete(&keychain_id);
    }

    let deleted_connection_ids: HashSet<String> =
        side_effects.deleted_connection_ids.into_iter().collect();
    for connection_id in deleted_connection_ids {
        forwarding_registry
            .delete_owned_forwards(&connection_id)
            .await?;
    }

    if result.applied > 0 {
        state.save().await?;
        app_handle
            .emit("connection:update", "saved")
            .map_err(|e| format!("Failed to emit connection:update: {}", e))?;
    }

    Ok(result)
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
        updated_at: Some(chrono::Utc::now()),
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
            updated_at: Some(chrono::Utc::now()),
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
