use std::fmt;

use crate::{SecretString, keychain::ConnectionKeychain};

pub const CONFIG_VERSION: u32 = 1;
pub const CONNECTION_TOMBSTONE_RETENTION_DAYS: i64 = 30;
pub const LOCAL_SHELL_PRIVILEGE_CONNECTION_ID: &str = "local-shell:default";
pub const GLOBAL_UPSTREAM_PROXY_PASSWORD_KEYCHAIN_ID: &str = "oxide_global_upstream_proxy_password";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    Password,
    Key,
    ManagedKey,
    Certificate,
    KeyboardInteractive,
    Agent,
}

impl AuthType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Key => "key",
            Self::ManagedKey => "managed_key",
            Self::Certificate => "certificate",
            Self::KeyboardInteractive => "keyboard_interactive",
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
    // Keyboard-interactive carries no persisted secret; prompts are collected during connect.
    KeyboardInteractive,
    Agent,
}

impl SavedAuth {
    pub fn auth_type(&self) -> AuthType {
        match self {
            Self::Password { .. } => AuthType::Password,
            Self::Key { .. } => AuthType::Key,
            Self::ManagedKey { .. } => AuthType::ManagedKey,
            Self::Certificate { .. } => AuthType::Certificate,
            Self::KeyboardInteractive => AuthType::KeyboardInteractive,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedUpstreamProxyProtocol {
    Socks5,
    HttpConnect,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SavedUpstreamProxyAuth {
    None,
    Password {
        username: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keychain_id: Option<String>,
        #[serde(default, rename = "password", skip_serializing)]
        plaintext_password: Option<SecretString>,
    },
}

impl Default for SavedUpstreamProxyAuth {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedUpstreamProxyConfig {
    pub protocol: SavedUpstreamProxyProtocol,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub auth: SavedUpstreamProxyAuth,
    #[serde(default = "default_proxy_remote_dns")]
    pub remote_dns: bool,
    #[serde(default)]
    pub no_proxy: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SavedUpstreamProxyPolicy {
    UseGlobal,
    Direct,
    Custom { proxy: SavedUpstreamProxyConfig },
}

impl SavedUpstreamProxyPolicy {
    pub fn is_use_global(&self) -> bool {
        matches!(self, Self::UseGlobal)
    }
}

impl Default for SavedUpstreamProxyPolicy {
    fn default() -> Self {
        Self::UseGlobal
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivilegeCredentialKind {
    SudoPassword,
    SuPassword,
    CustomPrompt,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SavedPrivilegeCredential {
    pub id: String,
    pub connection_id: String,
    pub label: String,
    pub kind: PrivilegeCredentialKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompt_patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keychain_id: Option<String>,
    #[serde(default, skip)]
    pub plaintext_secret: Option<SecretString>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub require_click_to_send: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    #[serde(default, skip_serializing_if = "SavedUpstreamProxyPolicy::is_use_global")]
    pub upstream_proxy: SavedUpstreamProxyPolicy,
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
    /// Privilege helper metadata is persisted with the connection, but the
    /// secret value lives only in the dedicated keychain namespace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub privilege_credentials: Vec<SavedPrivilegeCredential>,
}

fn default_port() -> u16 {
    22
}

fn default_true() -> bool {
    true
}

fn default_proxy_remote_dns() -> bool {
    true
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
    pub upstream_proxy: SavedUpstreamProxyPolicy,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub agent_forwarding: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_connect_command: Option<String>,
}

#[derive(Clone)]
pub struct SavePrivilegeCredentialRequest {
    pub connection_id: String,
    pub credential_id: Option<String>,
    pub label: String,
    pub kind: PrivilegeCredentialKind,
    pub username_hint: Option<String>,
    pub prompt_patterns: Vec<String>,
    /// UI drafts become SecretString at the store boundary. The value is stored
    /// in keychain and never serialized into SavedConnection.
    pub secret: Option<SecretString>,
    pub enabled: bool,
    pub require_click_to_send: bool,
}

impl fmt::Debug for SavePrivilegeCredentialRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // This request crosses the UI-to-store secret boundary. Keep Debug
        // useful for metadata while never depending on SecretString internals
        // to redact the cleartext privilege credential.
        formatter
            .debug_struct("SavePrivilegeCredentialRequest")
            .field("connection_id", &self.connection_id)
            .field("credential_id", &self.credential_id)
            .field("label", &self.label)
            .field("kind", &self.kind)
            .field("username_hint", &self.username_hint)
            .field("prompt_patterns", &self.prompt_patterns)
            .field("secret", &self.secret.as_ref().map(|_| "[redacted secret]"))
            .field("enabled", &self.enabled)
            .field("require_click_to_send", &self.require_click_to_send)
            .finish()
    }
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
            upstream_proxy: conn.upstream_proxy.clone(),
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

#[derive(Clone, Debug, Default)]
pub struct SaveSerialProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub group: Option<String>,
    pub port_path: String,
    pub baud_rate: Option<u32>,
    pub data_bits: Option<u8>,
    pub stop_bits: Option<u8>,
    pub parity: Option<SerialParity>,
    pub flow_control: Option<SerialFlowControl>,
    pub connect_on_open: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TelnetProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "is_false")]
    pub connect_on_open: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default)]
pub struct SaveTelnetProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub connect_on_open: Option<bool>,
}

/// Controls which bytes are appended when Enter is pressed in text send mode.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawTcpLineEnding {
    Lf,
    CrLf,
    Cr,
    None,
}

impl Default for RawTcpLineEnding {
    fn default() -> Self {
        // CRLF is the safest default for interactive network protocols such as
        // HTTP, SMTP, and many device consoles.
        Self::CrLf
    }
}

/// Describes how received Raw TCP bytes should be rendered to the terminal UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawTcpDisplayMode {
    Text,
    Hex,
    Mixed,
}

impl Default for RawTcpDisplayMode {
    fn default() -> Self {
        Self::Text
    }
}

/// Describes how typed Raw TCP input should be interpreted before writing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawTcpSendMode {
    Text,
    Hex,
}

impl Default for RawTcpSendMode {
    fn default() -> Self {
        Self::Text
    }
}

/// Stores whether a Raw TCP profile upgrades the socket with TLS.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawTcpTlsMode {
    Disabled,
    Enabled,
}

impl Default for RawTcpTlsMode {
    fn default() -> Self {
        Self::Disabled
    }
}

/// Stores certificate verification policy for TLS Raw TCP profiles.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawTcpTlsVerification {
    System,
    AllowInvalidCertificates,
}

impl Default for RawTcpTlsVerification {
    fn default() -> Self {
        Self::System
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawTcpProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "is_raw_tcp_default_line_ending")]
    pub line_ending: RawTcpLineEnding,
    #[serde(default, skip_serializing_if = "is_raw_tcp_default_display_mode")]
    pub display_mode: RawTcpDisplayMode,
    #[serde(default, skip_serializing_if = "is_raw_tcp_default_send_mode")]
    pub send_mode: RawTcpSendMode,
    #[serde(default, skip_serializing_if = "is_raw_tcp_tls_disabled")]
    pub tls_mode: RawTcpTlsMode,
    #[serde(default, skip_serializing_if = "is_raw_tcp_default_tls_verification")]
    pub tls_verification: RawTcpTlsVerification,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_server_name: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub connect_on_open: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default)]
pub struct SaveRawTcpProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub line_ending: Option<RawTcpLineEnding>,
    pub display_mode: Option<RawTcpDisplayMode>,
    pub send_mode: Option<RawTcpSendMode>,
    pub tls_mode: Option<RawTcpTlsMode>,
    pub tls_verification: Option<RawTcpTlsVerification>,
    pub tls_server_name: Option<String>,
    pub connect_on_open: Option<bool>,
}

/// Controls which bytes are appended when Enter sends a Raw UDP text datagram.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawUdpLineEnding {
    Lf,
    CrLf,
    Cr,
    None,
}

impl Default for RawUdpLineEnding {
    fn default() -> Self {
        // UDP datagrams are packet-shaped already, so avoid adding bytes unless
        // the user chooses protocol-specific line endings.
        Self::None
    }
}

/// Describes how received Raw UDP datagrams should be rendered to the terminal UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawUdpDisplayMode {
    Text,
    Hex,
    Mixed,
}

impl Default for RawUdpDisplayMode {
    fn default() -> Self {
        Self::Text
    }
}

/// Describes how typed Raw UDP input should be interpreted before sending.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawUdpSendMode {
    Text,
    Hex,
}

impl Default for RawUdpSendMode {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawUdpProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub remote_host: String,
    pub remote_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_bind_host: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero_u16")]
    pub local_bind_port: u16,
    #[serde(default, skip_serializing_if = "is_raw_udp_default_line_ending")]
    pub line_ending: RawUdpLineEnding,
    #[serde(default, skip_serializing_if = "is_raw_udp_default_display_mode")]
    pub display_mode: RawUdpDisplayMode,
    #[serde(default, skip_serializing_if = "is_raw_udp_default_send_mode")]
    pub send_mode: RawUdpSendMode,
    #[serde(default, skip_serializing_if = "is_false")]
    pub connect_on_open: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default)]
pub struct SaveRawUdpProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub group: Option<String>,
    pub remote_host: String,
    pub remote_port: u16,
    pub local_bind_host: Option<String>,
    pub local_bind_port: Option<u16>,
    pub line_ending: Option<RawUdpLineEnding>,
    pub display_mode: Option<RawUdpDisplayMode>,
    pub send_mode: Option<RawUdpSendMode>,
    pub connect_on_open: Option<bool>,
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

impl TelnetProfile {
    pub fn new(name: impl Into<String>, host: impl Into<String>, port: u16) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            group: None,
            host: host.into(),
            port,
            connect_on_open: false,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("Telnet profile id is required");
        }
        if self.name.trim().is_empty() {
            bail!("Telnet profile name is required");
        }
        if self.host.trim().is_empty() {
            bail!("Telnet host is required");
        }
        Ok(())
    }
}

impl RawTcpProfile {
    pub fn new(name: impl Into<String>, host: impl Into<String>, port: u16) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            group: None,
            host: host.into(),
            port,
            line_ending: RawTcpLineEnding::default(),
            display_mode: RawTcpDisplayMode::default(),
            send_mode: RawTcpSendMode::default(),
            tls_mode: RawTcpTlsMode::default(),
            tls_verification: RawTcpTlsVerification::default(),
            tls_server_name: None,
            connect_on_open: false,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("Raw TCP profile id is required");
        }
        if self.name.trim().is_empty() {
            bail!("Raw TCP profile name is required");
        }
        if self.host.trim().is_empty() {
            bail!("Raw TCP host is required");
        }
        if self.port == 0 {
            bail!("Raw TCP port must be greater than zero");
        }
        if self
            .tls_server_name
            .as_deref()
            .is_some_and(|server_name| server_name.trim().is_empty())
        {
            bail!("Raw TCP TLS server name must not be empty");
        }
        Ok(())
    }
}

impl RawUdpProfile {
    pub fn new(
        name: impl Into<String>,
        remote_host: impl Into<String>,
        remote_port: u16,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            group: None,
            remote_host: remote_host.into(),
            remote_port,
            local_bind_host: None,
            local_bind_port: 0,
            line_ending: RawUdpLineEnding::default(),
            display_mode: RawUdpDisplayMode::default(),
            send_mode: RawUdpSendMode::default(),
            connect_on_open: false,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("Raw UDP profile id is required");
        }
        if self.name.trim().is_empty() {
            bail!("Raw UDP profile name is required");
        }
        if self.remote_host.trim().is_empty() {
            bail!("Raw UDP remote host is required");
        }
        if self.remote_port == 0 {
            bail!("Raw UDP remote port must be greater than zero");
        }
        if self
            .local_bind_host
            .as_deref()
            .is_some_and(|host| host.trim().is_empty())
        {
            bail!("Raw UDP local bind host must not be empty");
        }
        Ok(())
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero_u16(value: &u16) -> bool {
    *value == 0
}

fn is_raw_tcp_default_line_ending(value: &RawTcpLineEnding) -> bool {
    *value == RawTcpLineEnding::default()
}

fn is_raw_tcp_default_display_mode(value: &RawTcpDisplayMode) -> bool {
    *value == RawTcpDisplayMode::default()
}

fn is_raw_tcp_default_send_mode(value: &RawTcpSendMode) -> bool {
    *value == RawTcpSendMode::default()
}

fn is_raw_tcp_tls_disabled(value: &RawTcpTlsMode) -> bool {
    *value == RawTcpTlsMode::Disabled
}

fn is_raw_tcp_default_tls_verification(value: &RawTcpTlsVerification) -> bool {
    *value == RawTcpTlsVerification::default()
}

fn is_raw_udp_default_line_ending(value: &RawUdpLineEnding) -> bool {
    *value == RawUdpLineEnding::default()
}

fn is_raw_udp_default_display_mode(value: &RawUdpDisplayMode) -> bool {
    *value == RawUdpDisplayMode::default()
}

fn is_raw_udp_default_send_mode(value: &RawUdpSendMode) -> bool {
    *value == RawUdpSendMode::default()
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
    pub upstream_proxy: SavedUpstreamProxyPolicy,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub telnet_profiles: Vec<TelnetProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_tcp_profiles: Vec<RawTcpProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_udp_profiles: Vec<RawUdpProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_privilege_credentials: Vec<SavedPrivilegeCredential>,
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
            telnet_profiles: Vec::new(),
            raw_tcp_profiles: Vec::new(),
            raw_udp_profiles: Vec::new(),
            local_privilege_credentials: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialProfilesSyncSnapshot {
    pub revision: String,
    pub exported_at: String,
    #[serde(default)]
    pub records: Vec<SerialProfile>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawTcpProfilesSyncSnapshot {
    pub revision: String,
    pub exported_at: String,
    #[serde(default)]
    pub records: Vec<RawTcpProfile>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawUdpProfilesSyncSnapshot {
    pub revision: String,
    pub exported_at: String,
    #[serde(default)]
    pub records: Vec<RawUdpProfile>,
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
    /// Managed secret ID containing the private key material.
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
    privilege_keychain: ConnectionKeychain,
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
    touched_privilege_keychain_ids: Vec<String>,
    stale_old_keychain_ids: Vec<String>,
}
