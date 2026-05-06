use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::keychain::ConnectionKeychain;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    Password,
    Key,
    Certificate,
    Agent,
}

impl AuthType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Key => "key",
            Self::Certificate => "certificate",
            Self::Agent => "agent",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SavedAuth {
    Password {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keychain_id: Option<String>,
        #[serde(default, rename = "password", skip_serializing)]
        plaintext_password: Option<String>,
    },
    Key {
        key_path: String,
        passphrase: Option<String>,
    },
    Certificate {
        key_path: String,
        cert_path: String,
        passphrase: Option<String>,
    },
    Agent,
}

impl SavedAuth {
    pub fn auth_type(&self) -> AuthType {
        match self {
            Self::Password { .. } => AuthType::Password,
            Self::Key { .. } => AuthType::Key,
            Self::Certificate { .. } => AuthType::Certificate,
            Self::Agent => AuthType::Agent,
        }
    }

    pub fn key_path(&self) -> Option<&str> {
        match self {
            Self::Key { key_path, .. } | Self::Certificate { key_path, .. } => Some(key_path),
            _ => None,
        }
    }

    pub fn cert_path(&self) -> Option<&str> {
        match self {
            Self::Certificate { cert_path, .. } => Some(cert_path),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConnectionOptions {
    #[serde(default)]
    pub agent_forwarding: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    pub auth: SavedAuth,
    #[serde(default)]
    pub options: ConnectionOptions,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

fn default_port() -> u16 {
    22
}

impl SavedConnection {
    pub fn touch(&mut self) {
        let now = Utc::now();
        self.last_used_at = Some(now);
        self.updated_at = Some(now);
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionInfo {
    pub id: String,
    pub name: String,
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub agent_forwarding: bool,
}

impl From<&SavedConnection> for ConnectionInfo {
    fn from(conn: &SavedConnection) -> Self {
        Self {
            id: conn.id.clone(),
            name: conn.name.clone(),
            group: conn.group.clone(),
            host: conn.host.clone(),
            port: conn.port,
            username: conn.username.clone(),
            auth_type: conn.auth.auth_type(),
            key_path: conn.auth.key_path().map(ToOwned::to_owned),
            cert_path: conn.auth.cert_path().map(ToOwned::to_owned),
            created_at: conn.created_at.to_rfc3339(),
            last_used_at: conn.last_used_at.map(|time| time.to_rfc3339()),
            color: conn.color.clone(),
            tags: conn.tags.clone(),
            agent_forwarding: conn.options.agent_forwarding,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SaveConnectionRequest {
    pub id: Option<String>,
    pub name: String,
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SavedAuth,
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub agent_forwarding: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConnectionStoreData {
    #[serde(default)]
    pub connections: Vec<SavedConnection>,
    #[serde(default)]
    pub groups: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ConnectionStore {
    path: PathBuf,
    data: ConnectionStoreData,
    keychain: ConnectionKeychain,
}

impl ConnectionStore {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let data = if path.exists() {
            let bytes =
                fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
            serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", path.display()))?
        } else {
            ConnectionStoreData::default()
        };
        let mut store = Self {
            path,
            data,
            keychain: ConnectionKeychain::default(),
        };
        store.normalize();
        if store.migrate_legacy_passwords()? {
            store.save()?;
        }
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn connections(&self) -> &[SavedConnection] {
        &self.data.connections
    }

    pub fn connection_infos(&self) -> Vec<ConnectionInfo> {
        self.data
            .connections
            .iter()
            .map(ConnectionInfo::from)
            .collect()
    }

    pub fn groups(&self) -> &[String] {
        &self.data.groups
    }

    pub fn get(&self, id: &str) -> Option<&SavedConnection> {
        self.data.connections.iter().find(|conn| conn.id == id)
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let data = serde_json::to_vec_pretty(&self.data)?;
        fs::write(&self.path, data)
            .with_context(|| format!("failed to write {}", self.path.display()))
    }

    pub fn upsert(&mut self, request: SaveConnectionRequest) -> Result<ConnectionInfo> {
        let group = normalize_optional_group_name(request.group.as_deref())?;
        let now = Utc::now();
        let id = request.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let existing_auth = self.get(&id).map(|conn| conn.auth.clone());
        let auth = self.materialize_auth(request.auth, existing_auth.as_ref())?;
        let connection = SavedConnection {
            id: id.clone(),
            name: non_empty(request.name.trim(), "Connection name")?.to_string(),
            group: group.clone(),
            host: non_empty(request.host.trim(), "Host")?.to_string(),
            port: request.port.max(1),
            username: non_empty(request.username.trim(), "Username")?.to_string(),
            auth,
            options: ConnectionOptions {
                agent_forwarding: request.agent_forwarding,
            },
            created_at: self.get(&id).map(|conn| conn.created_at).unwrap_or(now),
            last_used_at: self.get(&id).and_then(|conn| conn.last_used_at),
            updated_at: Some(now),
            color: request.color,
            tags: request.tags,
        };
        if let Some(index) = self.data.connections.iter().position(|conn| conn.id == id) {
            self.data.connections[index] = connection;
        } else {
            self.data.connections.push(connection);
        }
        if let Some(group) = group {
            self.ensure_group(group)?;
        }
        self.normalize();
        self.save()?;
        Ok(ConnectionInfo::from(
            self.get(&id).expect("connection saved"),
        ))
    }

    pub fn delete(&mut self, id: &str) -> Result<bool> {
        let keychain_ids = self
            .get(id)
            .map(collect_auth_keychain_ids)
            .unwrap_or_default();
        let before = self.data.connections.len();
        self.data.connections.retain(|conn| conn.id != id);
        let deleted = self.data.connections.len() != before;
        if deleted {
            self.save()?;
            for keychain_id in keychain_ids {
                let _ = self.keychain.delete(&keychain_id);
            }
        }
        Ok(deleted)
    }

    pub fn ensure_group(&mut self, name: String) -> Result<()> {
        let name = validate_group_name(&name)?;
        if !self.data.groups.contains(&name) {
            self.data.groups.push(name);
            self.normalize();
        }
        Ok(())
    }

    pub fn create_group(&mut self, name: String) -> Result<()> {
        self.ensure_group(name)?;
        self.save()
    }

    pub fn delete_group(&mut self, name: &str) -> Result<()> {
        self.data.groups.retain(|group| group != name);
        for conn in &mut self.data.connections {
            if conn.group.as_deref() == Some(name) {
                conn.group = None;
            }
        }
        self.save()
    }

    pub fn move_to_group(&mut self, ids: &[String], group: Option<&str>) -> Result<usize> {
        let group = normalize_optional_group_name(group)?;
        let id_set = ids.iter().collect::<HashSet<_>>();
        let now = Utc::now();
        let mut updated = 0;
        for conn in &mut self.data.connections {
            if id_set.contains(&conn.id) {
                conn.group = group.clone();
                conn.updated_at = Some(now);
                updated += 1;
            }
        }
        if let Some(group) = group {
            self.ensure_group(group)?;
        }
        self.save()?;
        Ok(updated)
    }

    pub fn duplicate(&mut self, id: &str) -> Result<Option<ConnectionInfo>> {
        let Some(mut duplicate) = self.get(id).cloned() else {
            return Ok(None);
        };
        duplicate.id = Uuid::new_v4().to_string();
        duplicate.name = format!("{} (Copy)", duplicate.name);
        duplicate.created_at = Utc::now();
        duplicate.updated_at = Some(Utc::now());
        duplicate.last_used_at = None;
        duplicate.auth = self.clone_auth_secret(&duplicate.auth)?;
        let duplicate_id = duplicate.id.clone();
        self.data.connections.push(duplicate);
        self.normalize();
        self.save()?;
        Ok(self.get(&duplicate_id).map(ConnectionInfo::from))
    }

    pub fn mark_used(&mut self, id: &str) -> Result<bool> {
        let Some(conn) = self.data.connections.iter_mut().find(|conn| conn.id == id) else {
            return Ok(false);
        };
        conn.touch();
        self.save()?;
        Ok(true)
    }

    pub fn import_ssh_connection(
        &mut self,
        mut connection: SavedConnection,
    ) -> Result<ConnectionInfo> {
        connection.id = Uuid::new_v4().to_string();
        connection.created_at = Utc::now();
        connection.updated_at = Some(Utc::now());
        connection.auth = self.materialize_auth(connection.auth, None)?;
        if let Some(group) = connection.group.clone() {
            self.ensure_group(group)?;
        }
        let id = connection.id.clone();
        self.data.connections.push(connection);
        self.normalize();
        self.save()?;
        Ok(self.get(&id).map(ConnectionInfo::from).expect("imported"))
    }

    pub fn get_connection_password(&self, id: &str) -> Result<String> {
        let conn = self
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found"))?;
        match &conn.auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => self.keychain.get(keychain_id),
            SavedAuth::Password {
                plaintext_password: Some(password),
                ..
            } => Ok(password.clone()),
            SavedAuth::Password {
                keychain_id: None, ..
            } => bail!("Password not saved for this connection"),
            _ => bail!("Connection does not use password auth"),
        }
    }

    fn materialize_auth(
        &self,
        auth: SavedAuth,
        existing_auth: Option<&SavedAuth>,
    ) -> Result<SavedAuth> {
        match auth {
            SavedAuth::Password {
                keychain_id,
                plaintext_password,
            } => {
                if let Some(password) = plaintext_password {
                    let keychain_id = existing_password_keychain_id(existing_auth)
                        .or(keychain_id)
                        .unwrap_or_else(new_password_keychain_id);
                    self.keychain.store(&keychain_id, &password)?;
                    Ok(SavedAuth::Password {
                        keychain_id: Some(keychain_id),
                        plaintext_password: None,
                    })
                } else {
                    Ok(SavedAuth::Password {
                        keychain_id,
                        plaintext_password: None,
                    })
                }
            }
            auth => Ok(auth),
        }
    }

    fn clone_auth_secret(&self, auth: &SavedAuth) -> Result<SavedAuth> {
        match auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => {
                let password = self.keychain.get(keychain_id)?;
                let next_keychain_id = new_password_keychain_id();
                self.keychain.store(&next_keychain_id, &password)?;
                Ok(SavedAuth::Password {
                    keychain_id: Some(next_keychain_id),
                    plaintext_password: None,
                })
            }
            SavedAuth::Password {
                keychain_id: None, ..
            } => Ok(SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
            }),
            auth => Ok(auth.clone()),
        }
    }

    fn migrate_legacy_passwords(&mut self) -> Result<bool> {
        let mut migrated = false;
        for conn in &mut self.data.connections {
            if let SavedAuth::Password {
                keychain_id,
                plaintext_password,
            } = &mut conn.auth
                && let Some(password) = plaintext_password.take()
            {
                let next_keychain_id = keychain_id.clone().unwrap_or_else(new_password_keychain_id);
                self.keychain.store(&next_keychain_id, &password)?;
                *keychain_id = Some(next_keychain_id);
                migrated = true;
            }
        }
        Ok(migrated)
    }

    fn normalize(&mut self) {
        self.data
            .groups
            .sort_by(|left, right| left.to_lowercase().cmp(&right.to_lowercase()));
        self.data.groups.dedup();
        let implicit_groups = self
            .data
            .connections
            .iter()
            .filter_map(|conn| conn.group.clone())
            .collect::<Vec<_>>();
        for group in implicit_groups {
            if !self.data.groups.contains(&group) {
                self.data.groups.push(group);
            }
        }
        self.data
            .connections
            .sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    }
}

pub fn validate_group_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("group name cannot be empty");
    }
    if name.split('/').any(|part| part.trim().is_empty()) {
        bail!("group path cannot contain empty segments");
    }
    Ok(name.to_string())
}

fn normalize_optional_group_name(group: Option<&str>) -> Result<Option<String>> {
    let Some(group) = group.map(str::trim).filter(|group| !group.is_empty()) else {
        return Ok(None);
    };
    if matches!(group, "Ungrouped" | "未分组") {
        return Ok(None);
    }
    validate_group_name(group).map(Some)
}

fn non_empty<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    if value.is_empty() {
        bail!("{label} is required");
    }
    Ok(value)
}

fn existing_password_keychain_id(auth: Option<&SavedAuth>) -> Option<String> {
    match auth {
        Some(SavedAuth::Password {
            keychain_id: Some(keychain_id),
            ..
        }) => Some(keychain_id.clone()),
        _ => None,
    }
}

fn collect_auth_keychain_ids(connection: &SavedConnection) -> Vec<String> {
    match &connection.auth {
        SavedAuth::Password {
            keychain_id: Some(keychain_id),
            ..
        } => vec![keychain_id.clone()],
        _ => Vec::new(),
    }
}

fn new_password_keychain_id() -> String {
    format!("oxide_conn_{}", Uuid::new_v4())
}
