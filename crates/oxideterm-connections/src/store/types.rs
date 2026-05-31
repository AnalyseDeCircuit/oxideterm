use crate::{SecretString, keychain::ConnectionKeychain};

pub const CONFIG_VERSION: u32 = 1;
pub const CONNECTION_TOMBSTONE_RETENTION_DAYS: i64 = 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    Password,
    Key,
    ManagedKey,
    Certificate,
    Agent,
}

impl AuthType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Key => "key",
            Self::ManagedKey => "managed_key",
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
        plaintext_password: Option<SecretString>,
    },
    Key {
        key_path: String,
        #[serde(default)]
        has_passphrase: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase_keychain_id: Option<String>,
        #[serde(default, rename = "passphrase", skip_serializing)]
        plaintext_passphrase: Option<SecretString>,
    },
    Certificate {
        key_path: String,
        cert_path: String,
        #[serde(default)]
        has_passphrase: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase_keychain_id: Option<String>,
        #[serde(default, rename = "passphrase", skip_serializing)]
        plaintext_passphrase: Option<SecretString>,
    },
    ManagedKey {
        key_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase_keychain_id: Option<String>,
        #[serde(default, rename = "passphrase", skip_serializing)]
        plaintext_passphrase: Option<SecretString>,
    },
    Agent,
}

impl SavedAuth {
    pub fn auth_type(&self) -> AuthType {
        match self {
            Self::Password { .. } => AuthType::Password,
            Self::Key { .. } => AuthType::Key,
            Self::ManagedKey { .. } => AuthType::ManagedKey,
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

    pub fn managed_key_id(&self) -> Option<&str> {
        match self {
            Self::ManagedKey { key_id, .. } => Some(key_id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConnectionOptions {
    #[serde(default)]
    pub keep_alive_interval: u32,
    #[serde(default)]
    pub compression: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term_type: Option<String>,
    #[serde(default)]
    pub agent_forwarding: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_connect_command: Option<String>,
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
pub struct ProxyHopInfo {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
    pub managed_key_id: Option<String>,
    pub managed_key_name: Option<String>,
    pub agent_forwarding: bool,
}

impl From<&SavedProxyHop> for ProxyHopInfo {
    fn from(hop: &SavedProxyHop) -> Self {
        Self {
            host: hop.host.clone(),
            port: hop.port,
            username: hop.username.clone(),
            auth_type: hop.auth.auth_type(),
            key_path: hop.auth.key_path().map(ToOwned::to_owned),
            cert_path: hop.auth.cert_path().map(ToOwned::to_owned),
            managed_key_id: hop.auth.managed_key_id().map(ToOwned::to_owned),
            managed_key_name: None,
            agent_forwarding: hop.agent_forwarding,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedConnection {
    pub id: String,
    #[serde(default = "default_config_version")]
    pub version: u32,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_connect_command: Option<String>,
}

fn default_port() -> u16 {
    22
}

fn default_config_version() -> u32 {
    CONFIG_VERSION
}

impl SavedConnection {
    pub fn touch(&mut self) {
        let now = Utc::now();
        self.last_used_at = Some(now);
        self.updated_at = Some(now);
    }

    pub fn post_connect_command(&self) -> Option<&str> {
        self.post_connect_command
            .as_deref()
            .or(self.options.post_connect_command.as_deref())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub managed_key_id: Option<String>,
    pub managed_key_name: Option<String>,
    pub proxy_chain: Vec<ProxyHopInfo>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub agent_forwarding: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_connect_command: Option<String>,
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
            managed_key_id: conn.auth.managed_key_id().map(ToOwned::to_owned),
            managed_key_name: None,
            proxy_chain: conn.proxy_chain.iter().map(ProxyHopInfo::from).collect(),
            created_at: conn.created_at.to_rfc3339(),
            last_used_at: conn.last_used_at.map(|time| time.to_rfc3339()),
            color: conn.color.clone(),
            tags: conn.tags.clone(),
            agent_forwarding: conn.options.agent_forwarding,
            post_connect_command: conn.post_connect_command().map(ToOwned::to_owned),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialParity {
    None,
    Odd,
    Even,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialFlowControl {
    None,
    Software,
    Hardware,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SerialProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub port_path: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: SerialParity,
    pub flow_control: SerialFlowControl,
    #[serde(default, skip_serializing_if = "is_false")]
    pub connect_on_open: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
}

impl SerialProfile {
    pub fn new(name: impl Into<String>, port_path: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            group: None,
            port_path: port_path.into(),
            baud_rate: 115_200,
            data_bits: 8,
            stop_bits: 1,
            parity: SerialParity::None,
            flow_control: SerialFlowControl::None,
            connect_on_open: false,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("Serial profile id is required");
        }
        if self.name.trim().is_empty() {
            bail!("Serial profile name is required");
        }
        if self.port_path.trim().is_empty() {
            bail!("Serial port path is required");
        }
        if self.baud_rate == 0 {
            bail!("Serial baud rate must be greater than zero");
        }
        if !(5..=8).contains(&self.data_bits) {
            bail!("Serial data bits must be between 5 and 8");
        }
        if !matches!(self.stop_bits, 1 | 2) {
            bail!("Serial stop bits must be 1 or 2");
        }
        Ok(())
    }
}

fn is_false(value: &bool) -> bool {
    !*value
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
    pub post_connect_command: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionStoreData {
    #[serde(default = "default_config_version")]
    pub version: u32,
    #[serde(default)]
    pub connections: Vec<SavedConnection>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connection_tombstones: Vec<DeletedConnectionTombstone>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub managed_ssh_keys: Vec<ManagedSshKey>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub serial_profiles: Vec<SerialProfile>,
}

impl Default for ConnectionStoreData {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            connections: Vec::new(),
            groups: Vec::new(),
            recent: Vec::new(),
            connection_tombstones: Vec::new(),
            managed_ssh_keys: Vec::new(),
            serial_profiles: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedSshKeyOrigin {
    ImportedFile,
    PastedText,
    OxideImport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManagedSshKey {
    pub id: String,
    pub secret_id: String,
    pub name: String,
    pub fingerprint: String,
    pub public_key: String,
    pub requires_passphrase: bool,
    pub origin: ManagedSshKeyOrigin,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(crate) struct ImportedManagedSshKey {
    pub key: ManagedSshKey,
    pub secret: SecretString,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeletedConnectionTombstone {
    pub id: String,
    pub deleted_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnectionSyncRecord {
    pub id: String,
    pub revision: String,
    pub updated_at: String,
    pub deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<ConnectionInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnectionsSyncSnapshot {
    pub revision: String,
    pub exported_at: String,
    pub records: Vec<SavedConnectionSyncRecord>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySavedConnectionsSyncSnapshotResult {
    pub applied: usize,
    pub skipped: usize,
    pub conflicts: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ApplySavedConnectionsSyncOutcome {
    pub result: ApplySavedConnectionsSyncSnapshotResult,
    pub deleted_connection_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSyncMetadata {
    pub saved_connections_revision: String,
    pub saved_connections_updated_at: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SavedConnectionsConflictStrategy {
    Skip,
    Replace,
    Merge,
}

impl SavedConnectionsConflictStrategy {
    pub fn parse(value: Option<&str>) -> Result<Self> {
        match value.unwrap_or("skip") {
            "skip" => Ok(Self::Skip),
            "replace" => Ok(Self::Replace),
            "merge" => Ok(Self::Merge),
            other => bail!("Unsupported saved connection conflict strategy: {other}"),
        }
    }

    fn preserves_local_auth(self) -> bool {
        matches!(self, Self::Merge)
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionStore {
    path: PathBuf,
    data: ConnectionStoreData,
    storage_format: ConnectionStoreStorageFormat,
    keychain: ConnectionKeychain,
    managed_keychain: ConnectionKeychain,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManagedSshKeyInfo {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub public_key: String,
    pub requires_passphrase: bool,
    pub origin: ManagedSshKeyOrigin,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&ManagedSshKey> for ManagedSshKeyInfo {
    fn from(key: &ManagedSshKey) -> Self {
        Self {
            id: key.id.clone(),
            name: key.name.clone(),
            fingerprint: key.fingerprint.clone(),
            public_key: key.public_key.clone(),
            requires_passphrase: key.requires_passphrase,
            origin: key.origin.clone(),
            created_at: key.created_at.to_rfc3339(),
            updated_at: key.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManagedSshKeyUsageItem {
    pub connection_id: String,
    pub connection_name: String,
    pub location: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManagedSshKeyUsage {
    pub key_id: String,
    pub count: usize,
    pub items: Vec<ManagedSshKeyUsageItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManagedSshKeyDeleteResult {
    pub deleted: bool,
    pub key_id: String,
    pub usage: ManagedSshKeyUsage,
}

#[derive(Debug)]
struct StagedImportedConnection {
    id: String,
    touched_keychain_ids: Vec<String>,
    stale_old_keychain_ids: Vec<String>,
}
