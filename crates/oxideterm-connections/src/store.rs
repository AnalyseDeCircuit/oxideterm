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
        #[serde(default)]
        has_passphrase: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase_keychain_id: Option<String>,
        #[serde(default, rename = "passphrase", skip_serializing)]
        plaintext_passphrase: Option<String>,
    },
    Certificate {
        key_path: String,
        cert_path: String,
        #[serde(default)]
        has_passphrase: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase_keychain_id: Option<String>,
        #[serde(default, rename = "passphrase", skip_serializing)]
        plaintext_passphrase: Option<String>,
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
pub struct SavedProxyHop {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    pub auth: SavedAuth,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_chain: Vec<SavedProxyHop>,
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
    pub proxy_chain: Vec<SavedProxyHop>,
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
            proxy_chain: conn.proxy_chain.clone(),
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
    pub proxy_chain: Vec<SavedProxyHop>,
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
        if store.migrate_legacy_credentials()? {
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
        let old_keychain_ids = self
            .get(&id)
            .map(collect_connection_keychain_ids)
            .unwrap_or_default();
        let existing = self.get(&id).cloned();
        let existing_auth = existing.as_ref().map(|conn| conn.auth.clone());
        let auth = self.materialize_auth(request.auth, existing_auth.as_ref())?;
        let proxy_chain = self.materialize_proxy_chain(
            request.proxy_chain,
            existing
                .as_ref()
                .map(|connection| connection.proxy_chain.as_slice()),
        )?;
        let next_keychain_ids = collect_keychain_ids_for_parts(&auth, &proxy_chain);
        let connection = SavedConnection {
            id: id.clone(),
            name: non_empty(request.name.trim(), "Connection name")?.to_string(),
            group: group.clone(),
            host: non_empty(request.host.trim(), "Host")?.to_string(),
            port: request.port.max(1),
            username: non_empty(request.username.trim(), "Username")?.to_string(),
            auth,
            proxy_chain,
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
        for keychain_id in old_keychain_ids
            .iter()
            .filter(|keychain_id| !next_keychain_ids.contains(*keychain_id))
        {
            let _ = self.keychain.delete(keychain_id);
        }
        Ok(ConnectionInfo::from(
            self.get(&id).expect("connection saved"),
        ))
    }

    pub fn delete(&mut self, id: &str) -> Result<bool> {
        let keychain_ids = self
            .get(id)
            .map(collect_connection_keychain_ids)
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
        for hop in &mut duplicate.proxy_chain {
            hop.auth = self.clone_auth_secret(&hop.auth)?;
        }
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
        connection.proxy_chain = self.materialize_proxy_chain(connection.proxy_chain, None)?;
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
        self.get_saved_auth_password(&conn.auth)
    }

    pub fn get_saved_auth_password(&self, auth: &SavedAuth) -> Result<String> {
        match auth {
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

    pub fn get_connection_passphrase(&self, id: &str) -> Result<Option<String>> {
        let conn = self
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found"))?;
        self.get_saved_auth_passphrase(&conn.auth)
    }

    pub fn get_saved_auth_passphrase(&self, auth: &SavedAuth) -> Result<Option<String>> {
        match auth {
            SavedAuth::Key {
                passphrase_keychain_id: Some(keychain_id),
                ..
            }
            | SavedAuth::Certificate {
                passphrase_keychain_id: Some(keychain_id),
                ..
            } => self.keychain.get(keychain_id).map(Some),
            SavedAuth::Key {
                plaintext_passphrase: Some(passphrase),
                ..
            }
            | SavedAuth::Certificate {
                plaintext_passphrase: Some(passphrase),
                ..
            } => Ok(Some(passphrase.clone())),
            SavedAuth::Key { .. } | SavedAuth::Certificate { .. } => Ok(None),
            _ => bail!("Connection does not use key passphrase auth"),
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
            SavedAuth::Key {
                key_path,
                has_passphrase,
                passphrase_keychain_id,
                plaintext_passphrase,
            } => {
                let retained_id = matching_key_passphrase_id(existing_auth, &key_path);
                if let Some(passphrase) = plaintext_passphrase {
                    let keychain_id = retained_id
                        .or(passphrase_keychain_id)
                        .unwrap_or_else(new_key_passphrase_keychain_id);
                    self.keychain.store(&keychain_id, &passphrase)?;
                    Ok(SavedAuth::Key {
                        key_path,
                        has_passphrase: true,
                        passphrase_keychain_id: Some(keychain_id),
                        plaintext_passphrase: None,
                    })
                } else if let Some((has_passphrase, passphrase_keychain_id)) =
                    matching_key_passphrase(existing_auth, &key_path)
                {
                    Ok(SavedAuth::Key {
                        key_path,
                        has_passphrase,
                        passphrase_keychain_id,
                        plaintext_passphrase: None,
                    })
                } else {
                    let has_passphrase = has_passphrase || passphrase_keychain_id.is_some();
                    Ok(SavedAuth::Key {
                        key_path,
                        has_passphrase,
                        passphrase_keychain_id,
                        plaintext_passphrase: None,
                    })
                }
            }
            SavedAuth::Certificate {
                key_path,
                cert_path,
                has_passphrase,
                passphrase_keychain_id,
                plaintext_passphrase,
            } => {
                let retained_id =
                    matching_certificate_passphrase_id(existing_auth, &key_path, &cert_path);
                if let Some(passphrase) = plaintext_passphrase {
                    let keychain_id = retained_id
                        .or(passphrase_keychain_id)
                        .unwrap_or_else(new_key_passphrase_keychain_id);
                    self.keychain.store(&keychain_id, &passphrase)?;
                    Ok(SavedAuth::Certificate {
                        key_path,
                        cert_path,
                        has_passphrase: true,
                        passphrase_keychain_id: Some(keychain_id),
                        plaintext_passphrase: None,
                    })
                } else if let Some((has_passphrase, passphrase_keychain_id)) =
                    matching_certificate_passphrase(existing_auth, &key_path, &cert_path)
                {
                    Ok(SavedAuth::Certificate {
                        key_path,
                        cert_path,
                        has_passphrase,
                        passphrase_keychain_id,
                        plaintext_passphrase: None,
                    })
                } else {
                    let has_passphrase = has_passphrase || passphrase_keychain_id.is_some();
                    Ok(SavedAuth::Certificate {
                        key_path,
                        cert_path,
                        has_passphrase,
                        passphrase_keychain_id,
                        plaintext_passphrase: None,
                    })
                }
            }
            auth => Ok(auth),
        }
    }

    fn materialize_proxy_chain(
        &self,
        proxy_chain: Vec<SavedProxyHop>,
        existing_proxy_chain: Option<&[SavedProxyHop]>,
    ) -> Result<Vec<SavedProxyHop>> {
        proxy_chain
            .into_iter()
            .enumerate()
            .map(|(index, hop)| {
                let existing_auth = existing_proxy_chain
                    .and_then(|chain| chain.get(index))
                    .filter(|existing| {
                        existing.host == hop.host
                            && existing.port == hop.port
                            && existing.username == hop.username
                    })
                    .map(|existing| &existing.auth);
                Ok(SavedProxyHop {
                    host: non_empty(hop.host.trim(), "Proxy host")?.to_string(),
                    port: hop.port.max(1),
                    username: non_empty(hop.username.trim(), "Proxy username")?.to_string(),
                    auth: self.materialize_auth(hop.auth, existing_auth)?,
                    agent_forwarding: hop.agent_forwarding,
                })
            })
            .collect()
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
            SavedAuth::Key {
                key_path,
                has_passphrase,
                passphrase_keychain_id: Some(passphrase_keychain_id),
                ..
            } => {
                let passphrase = self.keychain.get(passphrase_keychain_id)?;
                let next_keychain_id = new_key_passphrase_keychain_id();
                self.keychain.store(&next_keychain_id, &passphrase)?;
                Ok(SavedAuth::Key {
                    key_path: key_path.clone(),
                    has_passphrase: *has_passphrase,
                    passphrase_keychain_id: Some(next_keychain_id),
                    plaintext_passphrase: None,
                })
            }
            SavedAuth::Certificate {
                key_path,
                cert_path,
                has_passphrase,
                passphrase_keychain_id: Some(passphrase_keychain_id),
                ..
            } => {
                let passphrase = self.keychain.get(passphrase_keychain_id)?;
                let next_keychain_id = new_key_passphrase_keychain_id();
                self.keychain.store(&next_keychain_id, &passphrase)?;
                Ok(SavedAuth::Certificate {
                    key_path: key_path.clone(),
                    cert_path: cert_path.clone(),
                    has_passphrase: *has_passphrase,
                    passphrase_keychain_id: Some(next_keychain_id),
                    plaintext_passphrase: None,
                })
            }
            auth => Ok(auth.clone()),
        }
    }

    fn migrate_legacy_credentials(&mut self) -> Result<bool> {
        let mut migrated = false;
        for conn in &mut self.data.connections {
            migrated |= migrate_legacy_auth_credentials(&mut conn.auth, &self.keychain)?;
            for hop in &mut conn.proxy_chain {
                migrated |= migrate_legacy_auth_credentials(&mut hop.auth, &self.keychain)?;
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

fn migrate_legacy_auth_credentials(
    auth: &mut SavedAuth,
    keychain: &ConnectionKeychain,
) -> Result<bool> {
    match auth {
        SavedAuth::Password {
            keychain_id,
            plaintext_password,
        } => {
            if let Some(password) = plaintext_password.take() {
                let next_keychain_id = keychain_id.clone().unwrap_or_else(new_password_keychain_id);
                keychain.store(&next_keychain_id, &password)?;
                *keychain_id = Some(next_keychain_id);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        SavedAuth::Key {
            has_passphrase,
            passphrase_keychain_id,
            plaintext_passphrase,
            ..
        } => {
            if let Some(passphrase) = plaintext_passphrase.take() {
                let next_keychain_id = passphrase_keychain_id
                    .clone()
                    .unwrap_or_else(new_key_passphrase_keychain_id);
                keychain.store(&next_keychain_id, &passphrase)?;
                *has_passphrase = true;
                *passphrase_keychain_id = Some(next_keychain_id);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        SavedAuth::Certificate {
            has_passphrase,
            passphrase_keychain_id,
            plaintext_passphrase,
            ..
        } => {
            if let Some(passphrase) = plaintext_passphrase.take() {
                let next_keychain_id = passphrase_keychain_id
                    .clone()
                    .unwrap_or_else(new_key_passphrase_keychain_id);
                keychain.store(&next_keychain_id, &passphrase)?;
                *has_passphrase = true;
                *passphrase_keychain_id = Some(next_keychain_id);
                Ok(true)
            } else {
                Ok(false)
            }
        }
        SavedAuth::Agent => Ok(false),
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

fn collect_connection_keychain_ids(connection: &SavedConnection) -> Vec<String> {
    collect_keychain_ids_for_parts(&connection.auth, &connection.proxy_chain)
}

fn collect_keychain_ids_for_parts(auth: &SavedAuth, proxy_chain: &[SavedProxyHop]) -> Vec<String> {
    let mut ids = collect_keychain_ids_for_auth(auth);
    for hop in proxy_chain {
        ids.extend(collect_keychain_ids_for_auth(&hop.auth));
    }
    ids
}

fn collect_keychain_ids_for_auth(auth: &SavedAuth) -> Vec<String> {
    match auth {
        SavedAuth::Password {
            keychain_id: Some(keychain_id),
            ..
        } => vec![keychain_id.clone()],
        SavedAuth::Key {
            passphrase_keychain_id: Some(keychain_id),
            ..
        }
        | SavedAuth::Certificate {
            passphrase_keychain_id: Some(keychain_id),
            ..
        } => vec![keychain_id.clone()],
        _ => Vec::new(),
    }
}

fn new_password_keychain_id() -> String {
    format!("oxide_conn_{}", Uuid::new_v4())
}

fn new_key_passphrase_keychain_id() -> String {
    format!("oxide_conn_key_{}", Uuid::new_v4())
}

fn matching_key_passphrase_id(auth: Option<&SavedAuth>, key_path: &str) -> Option<String> {
    matching_key_passphrase(auth, key_path).and_then(|(_, id)| id)
}

fn matching_key_passphrase(
    auth: Option<&SavedAuth>,
    key_path: &str,
) -> Option<(bool, Option<String>)> {
    match auth {
        Some(SavedAuth::Key {
            key_path: existing_key_path,
            has_passphrase,
            passphrase_keychain_id,
            ..
        }) if existing_key_path == key_path => {
            Some((*has_passphrase, passphrase_keychain_id.clone()))
        }
        _ => None,
    }
}

fn matching_certificate_passphrase_id(
    auth: Option<&SavedAuth>,
    key_path: &str,
    cert_path: &str,
) -> Option<String> {
    matching_certificate_passphrase(auth, key_path, cert_path).and_then(|(_, id)| id)
}

fn matching_certificate_passphrase(
    auth: Option<&SavedAuth>,
    key_path: &str,
    cert_path: &str,
) -> Option<(bool, Option<String>)> {
    match auth {
        Some(SavedAuth::Certificate {
            key_path: existing_key_path,
            cert_path: existing_cert_path,
            has_passphrase,
            passphrase_keychain_id,
            ..
        }) if existing_key_path == key_path && existing_cert_path == cert_path => {
            Some((*has_passphrase, passphrase_keychain_id.clone()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    fn temp_store_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "oxideterm-connection-store-{name}-{}.json",
            Uuid::new_v4()
        ))
    }

    fn request(id: &str, auth: SavedAuth) -> SaveConnectionRequest {
        SaveConnectionRequest {
            id: Some(id.to_string()),
            name: "Home".to_string(),
            group: None,
            host: "192.168.1.2".to_string(),
            port: 22,
            username: "me".to_string(),
            auth,
            proxy_chain: Vec::new(),
            color: None,
            tags: Vec::new(),
            agent_forwarding: false,
        }
    }

    fn load_empty_store(name: &str) -> ConnectionStore {
        ConnectionStore::load(temp_store_path(name)).expect("store should load")
    }

    #[test]
    fn password_is_saved_to_keychain_reference() {
        let mut store = load_empty_store("password-save");

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some("secret".to_string()),
                },
            ))
            .unwrap();

        let conn = store.get("conn-1").unwrap();
        match &conn.auth {
            SavedAuth::Password {
                keychain_id: Some(_),
                plaintext_password: None,
            } => {}
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(store.get_connection_password("conn-1").unwrap(), "secret");
    }

    #[test]
    fn empty_password_is_saved_to_keychain_reference() {
        let mut store = load_empty_store("password-save-empty");

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some(String::new()),
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: Some(_),
                plaintext_password: None,
            } => {}
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(store.get_connection_password("conn-1").unwrap(), "");
    }

    #[test]
    fn password_auth_without_secret_keeps_no_keychain_reference() {
        let mut store = load_empty_store("password-no-save");

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: None,
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
            } => {}
            other => panic!("unexpected auth: {other:?}"),
        }
        assert!(store.get_connection_password("conn-1").is_err());
    }

    #[test]
    fn loaded_empty_password_updates_existing_keychain_entry() {
        let mut store = load_empty_store("password-clear");
        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some("secret".to_string()),
                },
            ))
            .unwrap();
        let previous_keychain_id = match &store.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected auth: {other:?}"),
        };

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: Some(previous_keychain_id.clone()),
                    plaintext_password: Some(String::new()),
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                plaintext_password: None,
            } => assert_eq!(keychain_id, &previous_keychain_id),
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(store.get_connection_password("conn-1").unwrap(), "");
    }

    #[test]
    fn unloaded_password_preserves_saved_keychain_entry() {
        let mut store = load_empty_store("password-preserve");
        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some("secret".to_string()),
                },
            ))
            .unwrap();
        let previous_auth = store.get("conn-1").unwrap().auth.clone();

        store.upsert(request("conn-1", previous_auth)).unwrap();

        assert_eq!(store.get_connection_password("conn-1").unwrap(), "secret");
    }

    #[test]
    fn legacy_plaintext_password_and_passphrase_are_migrated() {
        let path = temp_store_path("legacy-migration");
        fs::write(
            &path,
            r##"{
              "connections": [
                {
                  "id": "conn-1",
                  "name": "Home",
                  "host": "192.168.1.2",
                  "port": 22,
                  "username": "me",
                  "auth": { "type": "password", "password": "secret" },
                  "created_at": "2026-01-01T00:00:00Z"
                },
                {
                  "id": "conn-2",
                  "name": "Key",
                  "host": "192.168.1.3",
                  "port": 22,
                  "username": "me",
                  "auth": { "type": "key", "key_path": "/tmp/id", "passphrase": "key-secret" },
                  "created_at": "2026-01-01T00:00:00Z"
                }
              ],
              "groups": []
            }"##,
        )
        .unwrap();

        let store = ConnectionStore::load(&path).unwrap();

        assert_eq!(store.get_connection_password("conn-1").unwrap(), "secret");
        assert_eq!(
            store.get_connection_passphrase("conn-2").unwrap(),
            Some("key-secret".to_string())
        );
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("\"keychain_id\""));
        assert!(saved.contains("\"passphrase_keychain_id\""));
        assert!(!saved.contains("\"password\": \"secret\""));
        assert!(!saved.contains("\"passphrase\": \"key-secret\""));
    }

    #[test]
    fn unchanged_key_path_preserves_passphrase_keychain_entry() {
        let mut store = load_empty_store("key-preserve");
        store
            .upsert(request(
                "conn-1",
                SavedAuth::Key {
                    key_path: "/tmp/id".to_string(),
                    has_passphrase: true,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: Some("key-secret".to_string()),
                },
            ))
            .unwrap();
        let previous_keychain_id = match &store.get("conn-1").unwrap().auth {
            SavedAuth::Key {
                passphrase_keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected auth: {other:?}"),
        };

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Key {
                    key_path: "/tmp/id".to_string(),
                    has_passphrase: false,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: None,
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Key {
                has_passphrase,
                passphrase_keychain_id: Some(keychain_id),
                plaintext_passphrase: None,
                ..
            } => {
                assert!(*has_passphrase);
                assert_eq!(keychain_id, &previous_keychain_id);
            }
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(
            store.get_connection_passphrase("conn-1").unwrap(),
            Some("key-secret".to_string())
        );
    }

    #[test]
    fn changed_key_path_without_passphrase_clears_passphrase_reference() {
        let mut store = load_empty_store("key-clear");
        store
            .upsert(request(
                "conn-1",
                SavedAuth::Key {
                    key_path: "/tmp/id".to_string(),
                    has_passphrase: true,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: Some("key-secret".to_string()),
                },
            ))
            .unwrap();
        let previous_keychain_id = match &store.get("conn-1").unwrap().auth {
            SavedAuth::Key {
                passphrase_keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected auth: {other:?}"),
        };

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Key {
                    key_path: "/tmp/id-new".to_string(),
                    has_passphrase: false,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: None,
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Key {
                key_path,
                has_passphrase,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            } => {
                assert_eq!(key_path, "/tmp/id-new");
                assert!(!*has_passphrase);
            }
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(store.get_connection_passphrase("conn-1").unwrap(), None);
        assert!(store.keychain.get(&previous_keychain_id).is_err());
    }

    #[test]
    fn unchanged_certificate_paths_preserve_passphrase_keychain_entry() {
        let mut store = load_empty_store("cert-preserve");
        store
            .upsert(request(
                "conn-1",
                SavedAuth::Certificate {
                    key_path: "/tmp/id".to_string(),
                    cert_path: "/tmp/id-cert.pub".to_string(),
                    has_passphrase: true,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: Some("cert-secret".to_string()),
                },
            ))
            .unwrap();

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Certificate {
                    key_path: "/tmp/id".to_string(),
                    cert_path: "/tmp/id-cert.pub".to_string(),
                    has_passphrase: false,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: None,
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Certificate {
                has_passphrase,
                passphrase_keychain_id: Some(_),
                plaintext_passphrase: None,
                ..
            } => assert!(*has_passphrase),
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(
            store.get_connection_passphrase("conn-1").unwrap(),
            Some("cert-secret".to_string())
        );
    }

    #[test]
    fn proxy_hop_password_is_saved_to_keychain_reference() {
        let mut store = load_empty_store("proxy-hop-password");
        let mut req = request("conn-1", SavedAuth::Agent);
        req.proxy_chain = vec![SavedProxyHop {
            host: "jump.example.com".to_string(),
            port: 2222,
            username: "ops".to_string(),
            auth: SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some("jump-secret".to_string()),
            },
            agent_forwarding: true,
        }];

        store.upsert(req).unwrap();

        let hop = &store.get("conn-1").unwrap().proxy_chain[0];
        assert!(hop.agent_forwarding);
        match &hop.auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                plaintext_password: None,
            } => assert_eq!(store.keychain.get(keychain_id).unwrap(), "jump-secret"),
            other => panic!("unexpected proxy auth: {other:?}"),
        }
    }

    #[test]
    fn unchanged_proxy_hop_key_path_preserves_passphrase_keychain_entry() {
        let mut store = load_empty_store("proxy-hop-passphrase");
        let mut req = request("conn-1", SavedAuth::Agent);
        req.proxy_chain = vec![SavedProxyHop {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: SavedAuth::Key {
                key_path: "/tmp/jump-key".to_string(),
                has_passphrase: true,
                passphrase_keychain_id: None,
                plaintext_passphrase: Some("jump-key-secret".to_string()),
            },
            agent_forwarding: false,
        }];
        store.upsert(req).unwrap();
        let previous_keychain_id = match &store.get("conn-1").unwrap().proxy_chain[0].auth {
            SavedAuth::Key {
                passphrase_keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected proxy auth: {other:?}"),
        };

        let mut update = request("conn-1", SavedAuth::Agent);
        update.proxy_chain = vec![SavedProxyHop {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: SavedAuth::Key {
                key_path: "/tmp/jump-key".to_string(),
                has_passphrase: false,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            },
            agent_forwarding: false,
        }];
        store.upsert(update).unwrap();

        match &store.get("conn-1").unwrap().proxy_chain[0].auth {
            SavedAuth::Key {
                has_passphrase,
                passphrase_keychain_id: Some(keychain_id),
                plaintext_passphrase: None,
                ..
            } => {
                assert!(*has_passphrase);
                assert_eq!(keychain_id, &previous_keychain_id);
            }
            other => panic!("unexpected proxy auth: {other:?}"),
        }
    }
}
