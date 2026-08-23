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
    KerberosPreferred {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server_identity: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        delegate_credentials: bool,
        fallback: Box<SavedAuth>,
    },
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
            Self::KerberosPreferred { fallback, .. } => fallback.auth_type(),
        }
    }

    pub fn key_path(&self) -> Option<&str> {
        match self {
            Self::Key { key_path, .. } | Self::Certificate { key_path, .. } => Some(key_path),
            Self::KerberosPreferred { fallback, .. } => fallback.key_path(),
            _ => None,
        }
    }

    pub fn cert_path(&self) -> Option<&str> {
        match self {
            Self::Certificate { cert_path, .. } => Some(cert_path),
            Self::KerberosPreferred { fallback, .. } => fallback.cert_path(),
            _ => None,
        }
    }

    pub fn managed_key_id(&self) -> Option<&str> {
        match self {
            Self::ManagedKey { key_id, .. } => Some(key_id),
            Self::KerberosPreferred { fallback, .. } => fallback.managed_key_id(),
            _ => None,
        }
    }

    pub fn gssapi_options(&self) -> Option<(Option<&str>, bool)> {
        match self {
            Self::KerberosPreferred {
                server_identity,
                delegate_credentials,
                ..
            } => Some((server_identity.as_deref(), *delegate_credentials)),
            _ => None,
        }
    }

    pub fn conventional_fallback(&self) -> &SavedAuth {
        match self {
            Self::KerberosPreferred { fallback, .. } => fallback.conventional_fallback(),
            _ => self,
        }
    }

    pub fn with_kerberos_preferred(
        fallback: SavedAuth,
        server_identity: Option<String>,
        delegate_credentials: bool,
    ) -> Self {
        Self::KerberosPreferred {
            server_identity,
            delegate_credentials,
            fallback: Box::new(match fallback {
                Self::KerberosPreferred { fallback, .. } => *fallback,
                fallback => fallback,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionTerminalEncoding {
    #[serde(rename = "utf-8")]
    Utf8,
    Gbk,
    Gb18030,
    Big5,
    ShiftJis,
    #[serde(rename = "euc-jp")]
    EucJp,
    #[serde(rename = "euc-kr")]
    EucKr,
    #[serde(rename = "windows-1252")]
    Windows1252,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionTerminalBackspaceSequence {
    Delete,
    ControlH,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionTerminalDeleteSequence {
    Csi3Tilde,
    Delete,
    ControlH,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionTerminalSessionLogPolicy {
    #[default]
    Inherit,
    Automatic,
    // Manual keeps the terminal action available without starting a log on connect.
    Manual,
    Disabled,
}

impl ConnectionTerminalSessionLogPolicy {
    fn is_inherit(&self) -> bool {
        *self == Self::Inherit
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTerminalOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<ConnectionTerminalEncoding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backspace_sequence: Option<ConnectionTerminalBackspaceSequence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete_sequence: Option<ConnectionTerminalDeleteSequence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight_rule_set: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "ConnectionTerminalSessionLogPolicy::is_inherit"
    )]
    pub session_log_policy: ConnectionTerminalSessionLogPolicy,
}

impl ConnectionTerminalOptions {
    pub fn inherits_application_defaults(&self) -> bool {
        self.encoding.is_none()
            && self.backspace_sequence.is_none()
            && self.delete_sequence.is_none()
            && self.semantic_scheme.is_none()
            && self.highlight_rule_set.is_none()
            && self.session_log_policy == ConnectionTerminalSessionLogPolicy::Inherit
    }
}

pub const DEFAULT_X11_UNTRUSTED_TIMEOUT_SECONDS: u32 = 20 * 60;
pub const DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS: u64 = 30;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionX11ForwardingMode {
    #[default]
    Untrusted,
    Trusted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectionX11ForwardingOptions {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: ConnectionX11ForwardingMode,
    #[serde(default = "default_x11_untrusted_timeout_seconds")]
    pub untrusted_timeout_seconds: u32,
}

impl ConnectionX11ForwardingOptions {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for ConnectionX11ForwardingOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: ConnectionX11ForwardingMode::Untrusted,
            untrusted_timeout_seconds: DEFAULT_X11_UNTRUSTED_TIMEOUT_SECONDS,
        }
    }
}

fn default_x11_untrusted_timeout_seconds() -> u32 {
    DEFAULT_X11_UNTRUSTED_TIMEOUT_SECONDS
}

pub const MAX_SSH_ALGORITHMS_PER_CATEGORY: usize = 64;
pub const MAX_SSH_ALGORITHM_NAME_BYTES: usize = 128;

/// Ordered SSH algorithm overrides for one endpoint.
///
/// Empty categories inherit the effective OxideTerm preset. Non-empty categories
/// replace that preset category in negotiation order.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SshAlgorithmPreferences {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kex: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_key: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cipher: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mac: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compression: Vec<String>,
}

impl SshAlgorithmPreferences {
    pub fn is_default(&self) -> bool {
        self.kex.is_empty()
            && self.host_key.is_empty()
            && self.cipher.is_empty()
            && self.mac.is_empty()
            && self.compression.is_empty()
    }

    pub fn validate(&self) -> Result<()> {
        for (category, algorithms) in [
            ("KEX", self.kex.as_slice()),
            ("host key", self.host_key.as_slice()),
            ("cipher", self.cipher.as_slice()),
            ("MAC", self.mac.as_slice()),
            ("compression", self.compression.as_slice()),
        ] {
            if algorithms.len() > MAX_SSH_ALGORITHMS_PER_CATEGORY {
                bail!("Too many SSH {category} algorithms");
            }
            let mut unique = std::collections::HashSet::with_capacity(algorithms.len());
            for algorithm in algorithms {
                if algorithm.is_empty()
                    || algorithm.len() > MAX_SSH_ALGORITHM_NAME_BYTES
                    || algorithm.bytes().any(|byte| byte == b',' || byte.is_ascii_whitespace())
                {
                    bail!("Invalid SSH {category} algorithm name");
                }
                if !unique.insert(algorithm) {
                    bail!("Duplicate SSH {category} algorithm");
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConnectionOptions {
    /// Overrides the SSH TCP and protocol-handshake timeout for this host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_timeout_seconds: Option<u64>,
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
    pub identity_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_forwarding_socket: Option<String>,
    #[serde(default)]
    pub legacy_ssh_compatibility: bool,
    #[serde(default)]
    pub ssh_algorithms: SshAlgorithmPreferences,
    /// Some SSH servers require a new authentication exchange for every terminal.
    #[serde(default, skip_serializing_if = "is_false")]
    pub dedicated_new_terminal_connection: bool,
    /// X11 stores only portable policy; local display and cookies are resolved per shell.
    #[serde(
        default,
        skip_serializing_if = "ConnectionX11ForwardingOptions::is_default"
    )]
    pub x11_forwarding: ConnectionX11ForwardingOptions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_connect_command: Option<String>,
    /// Terminal protocol behavior is host-specific; absent values inherit the
    /// application defaults so existing saved connections remain compatible.
    #[serde(
        default,
        skip_serializing_if = "ConnectionTerminalOptions::inherits_application_defaults"
    )]
    pub terminal: ConnectionTerminalOptions,
}

impl ConnectionOptions {
    pub fn effective_connect_timeout_seconds(&self) -> u64 {
        self.connect_timeout_seconds
            .filter(|seconds| *seconds > 0)
            .unwrap_or(DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS)
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_forwarding_socket: Option<String>,
    #[serde(default)]
    pub legacy_ssh_compatibility: bool,
    #[serde(default)]
    pub ssh_algorithms: SshAlgorithmPreferences,
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

#[derive(Clone, Serialize, Deserialize)]
pub struct SavedProxyCommand {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keychain_id: Option<String>,
    #[serde(skip)]
    pub plaintext_command: Option<SecretString>,
}

impl fmt::Debug for SavedProxyCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SavedProxyCommand")
            .field("keychain_id", &self.keychain_id)
            .field(
                "plaintext_command",
                &self
                    .plaintext_command
                    .as_ref()
                    .map(|_| "[redacted secret]"),
            )
            .finish()
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub gssapi_authentication: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gssapi_server_identity: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub gssapi_delegate_credentials: bool,
    pub agent_forwarding: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_forwarding_socket: Option<String>,
    pub legacy_ssh_compatibility: bool,
    #[serde(default)]
    pub ssh_algorithms: SshAlgorithmPreferences,
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
            gssapi_authentication: hop.auth.gssapi_options().is_some(),
            gssapi_server_identity: hop
                .auth
                .gssapi_options()
                .and_then(|(identity, _)| identity.map(ToOwned::to_owned)),
            gssapi_delegate_credentials: hop
                .auth
                .gssapi_options()
                .is_some_and(|(_, delegate)| delegate),
            agent_forwarding: hop.agent_forwarding,
            identity_agent: hop.identity_agent.clone(),
            agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
            legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
            ssh_algorithms: hop.ssh_algorithms.clone(),
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
    /// Free-form user metadata. UI copy warns against storing credentials here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    pub auth: SavedAuth,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_chain: Vec<SavedProxyHop>,
    #[serde(default, skip_serializing_if = "SavedUpstreamProxyPolicy::is_use_global")]
    pub upstream_proxy: SavedUpstreamProxyPolicy,
    /// Manual ProxyCommand text stays in the protected store; metadata keeps only its reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_command: Option<SavedProxyCommand>,
    #[serde(default)]
    pub options: ConnectionOptions,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
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

fn default_ssh_connect_timeout_seconds() -> u64 {
    DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS
}

fn is_default_ssh_connect_timeout_seconds(value: &u64) -> bool {
    *value == DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS
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
    /// Free-form user metadata. It is intentionally excluded from connection search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
    pub managed_key_id: Option<String>,
    pub managed_key_name: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub gssapi_authentication: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gssapi_server_identity: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub gssapi_delegate_credentials: bool,
    pub proxy_chain: Vec<ProxyHopInfo>,
    pub upstream_proxy: SavedUpstreamProxyPolicy,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub tags: Vec<String>,
    pub agent_forwarding: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_forwarding_socket: Option<String>,
    pub legacy_ssh_compatibility: bool,
    #[serde(default)]
    pub ssh_algorithms: SshAlgorithmPreferences,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_connect_command: Option<String>,
}

impl ConnectionInfo {
    pub fn matches_search_query(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }

        // Keep every saved-connection search surface on one non-secret field set.
        self.name.to_lowercase().contains(&query)
            || self.host.to_lowercase().contains(&query)
            || self.port.to_string().contains(&query)
            || self.username.to_lowercase().contains(&query)
            || self
                .group
                .as_deref()
                .is_some_and(|group| group.to_lowercase().contains(&query))
            || self
                .tags
                .iter()
                .any(|tag| tag.to_lowercase().contains(&query))
    }

    pub fn search_text(&self) -> String {
        // Palette filtering consumes one normalized haystack while the other
        // surfaces use matches_search_query directly.
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            self.name,
            self.host,
            self.port,
            self.username,
            self.group.as_deref().unwrap_or_default(),
            self.tags.join(" ")
        )
    }
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
            notes: conn.notes.clone(),
            host: conn.host.clone(),
            port: conn.port,
            username: conn.username.clone(),
            auth_type: conn.auth.auth_type(),
            key_path: conn.auth.key_path().map(ToOwned::to_owned),
            cert_path: conn.auth.cert_path().map(ToOwned::to_owned),
            managed_key_id: conn.auth.managed_key_id().map(ToOwned::to_owned),
            managed_key_name: None,
            gssapi_authentication: conn.auth.gssapi_options().is_some(),
            gssapi_server_identity: conn
                .auth
                .gssapi_options()
                .and_then(|(identity, _)| identity.map(ToOwned::to_owned)),
            gssapi_delegate_credentials: conn
                .auth
                .gssapi_options()
                .is_some_and(|(_, delegate)| delegate),
            proxy_chain: conn.proxy_chain.iter().map(ProxyHopInfo::from).collect(),
            upstream_proxy: conn.upstream_proxy.clone(),
            created_at: conn.created_at.to_rfc3339(),
            last_used_at: conn.last_used_at.map(|time| time.to_rfc3339()),
            color: conn.color.clone(),
            icon_background_color: conn.icon_background_color.clone(),
            icon: conn.icon.clone(),
            tags: conn.tags.clone(),
            agent_forwarding: conn.options.agent_forwarding,
            identity_agent: conn.options.identity_agent.clone(),
            agent_forwarding_socket: conn.options.agent_forwarding_socket.clone(),
            legacy_ssh_compatibility: conn.options.legacy_ssh_compatibility,
            ssh_algorithms: conn.options.ssh_algorithms.clone(),
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
    /// Free-form user metadata. UI copy warns against storing credentials here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_background_color: Option<String>,
    pub port_path: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: SerialParity,
    pub flow_control: SerialFlowControl,
    #[serde(
        default,
        skip_serializing_if = "ConnectionTerminalOptions::inherits_application_defaults"
    )]
    pub terminal: ConnectionTerminalOptions,
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
    pub notes: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub icon_background_color: Option<String>,
    pub port_path: String,
    pub baud_rate: Option<u32>,
    pub data_bits: Option<u8>,
    pub stop_bits: Option<u8>,
    pub parity: Option<SerialParity>,
    pub flow_control: Option<SerialFlowControl>,
    pub terminal: ConnectionTerminalOptions,
    pub connect_on_open: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TelnetProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Free-form user metadata. UI copy warns against storing credentials here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_background_color: Option<String>,
    pub host: String,
    pub port: u16,
    #[serde(
        default,
        skip_serializing_if = "ConnectionTerminalOptions::inherits_application_defaults"
    )]
    pub terminal: ConnectionTerminalOptions,
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
    pub notes: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub icon_background_color: Option<String>,
    pub host: String,
    pub port: u16,
    pub terminal: ConnectionTerminalOptions,
    pub connect_on_open: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoshIpFamily {
    #[default]
    Auto,
    Ipv4,
    Ipv6,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum MoshUdpPortSelection {
    #[default]
    Automatic,
    Fixed { port: u16 },
    Range { start: u16, end: u16 },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoshPredictionMode {
    #[default]
    Adaptive,
    Always,
    Never,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MoshProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Free-form user metadata. UI copy warns against storing credentials here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_background_color: Option<String>,
    pub host: String,
    #[serde(default = "default_port")]
    pub ssh_port: u16,
    pub username: String,
    pub auth: SavedAuth,
    /// The SSH bootstrap may traverse jump hosts; the Mosh UDP session remains direct.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_chain: Vec<SavedProxyHop>,
    #[serde(default = "default_mosh_server_executable")]
    pub server_executable: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub udp_host_override: Option<String>,
    #[serde(default)]
    pub udp_port: MoshUdpPortSelection,
    #[serde(default)]
    pub ip_family: MoshIpFamily,
    #[serde(default)]
    pub prediction: MoshPredictionMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_agent: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub legacy_ssh_compatibility: bool,
    #[serde(default)]
    pub ssh_algorithms: SshAlgorithmPreferences,
    #[serde(
        default,
        skip_serializing_if = "ConnectionTerminalOptions::inherits_application_defaults"
    )]
    pub terminal: ConnectionTerminalOptions,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
}

fn default_mosh_server_executable() -> String {
    "mosh-server".to_string()
}

#[derive(Clone, Debug)]
pub struct SaveMoshProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub group: Option<String>,
    pub notes: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub icon_background_color: Option<String>,
    pub host: String,
    pub ssh_port: u16,
    pub username: String,
    pub auth: SavedAuth,
    pub proxy_chain: Vec<SavedProxyHop>,
    pub server_executable: String,
    pub udp_host_override: Option<String>,
    pub udp_port: MoshUdpPortSelection,
    pub ip_family: MoshIpFamily,
    pub prediction: MoshPredictionMode,
    pub locale: Option<String>,
    pub identity_agent: Option<String>,
    pub legacy_ssh_compatibility: bool,
    pub ssh_algorithms: SshAlgorithmPreferences,
    pub terminal: ConnectionTerminalOptions,
}

/// Carries a newly saved Mosh auth secret directly into one bootstrap attempt.
pub struct SavedMoshProfileRuntimeSecrets {
    pub auth: Option<SecretString>,
    pub proxy_chain: Vec<Option<SecretString>>,
}

impl fmt::Debug for SavedMoshProfileRuntimeSecrets {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SavedMoshProfileRuntimeSecrets")
            .field("auth", &self.auth.as_ref().map(|_| "[redacted secret]"))
            .field(
                "proxy_chain",
                &self
                    .proxy_chain
                    .iter()
                    .map(|secret| secret.as_ref().map(|_| "[redacted secret]"))
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandaloneSftpTransferMode {
    #[default]
    LocalRemote,
    RemoteRemote,
}

impl StandaloneSftpTransferMode {
    fn is_local_remote(value: &Self) -> bool {
        *value == Self::LocalRemote
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StandaloneSftpEndpoint {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    pub auth: SavedAuth,
    #[serde(
        default = "default_ssh_connect_timeout_seconds",
        skip_serializing_if = "is_default_ssh_connect_timeout_seconds"
    )]
    pub connect_timeout_seconds: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_chain: Vec<SavedProxyHop>,
    #[serde(default, skip_serializing_if = "SavedUpstreamProxyPolicy::is_use_global")]
    pub upstream_proxy: SavedUpstreamProxyPolicy,
    /// Manual ProxyCommand text stays in protected storage; metadata keeps only its reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_command: Option<SavedProxyCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_agent: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub legacy_ssh_compatibility: bool,
    #[serde(default)]
    pub ssh_algorithms: SshAlgorithmPreferences,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_remote_path: Option<String>,
}

impl fmt::Debug for StandaloneSftpEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Endpoint metadata may temporarily own plaintext credentials before keychain handoff.
        formatter
            .debug_struct("StandaloneSftpEndpoint")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth_type", &self.auth.auth_type())
            .field("connect_timeout_seconds", &self.connect_timeout_seconds)
            .field("proxy_chain_len", &self.proxy_chain.len())
            .field("has_upstream_proxy", &!self.upstream_proxy.is_use_global())
            .field("has_proxy_command", &self.proxy_command.is_some())
            .field("identity_agent", &self.identity_agent)
            .field("legacy_ssh_compatibility", &self.legacy_ssh_compatibility)
            .field("initial_remote_path", &self.initial_remote_path)
            .finish()
    }
}

impl StandaloneSftpEndpoint {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        auth: SavedAuth,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            username: username.into(),
            auth,
            connect_timeout_seconds: DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS,
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            proxy_command: None,
            identity_agent: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
            initial_remote_path: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.host.trim().is_empty() {
            bail!("Standalone SFTP secondary host is required");
        }
        if self.port == 0 {
            bail!("Standalone SFTP secondary port must be greater than zero");
        }
        if self.username.trim().is_empty() {
            bail!("Standalone SFTP secondary username is required");
        }
        if self.connect_timeout_seconds == 0 {
            bail!("Standalone SFTP secondary connect timeout must be greater than zero");
        }
        self.ssh_algorithms.validate()?;
        for hop in &self.proxy_chain {
            hop.ssh_algorithms.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StandaloneSftpProfile {
    pub id: String,
    #[serde(default = "default_config_version")]
    pub version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Free-form user metadata. UI copy warns against storing credentials here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_background_color: Option<String>,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    pub auth: SavedAuth,
    #[serde(
        default = "default_ssh_connect_timeout_seconds",
        skip_serializing_if = "is_default_ssh_connect_timeout_seconds"
    )]
    pub connect_timeout_seconds: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_chain: Vec<SavedProxyHop>,
    #[serde(default, skip_serializing_if = "SavedUpstreamProxyPolicy::is_use_global")]
    pub upstream_proxy: SavedUpstreamProxyPolicy,
    /// Manual ProxyCommand text stays in protected storage; metadata keeps only its reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_command: Option<SavedProxyCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_agent: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub legacy_ssh_compatibility: bool,
    #[serde(default)]
    pub ssh_algorithms: SshAlgorithmPreferences,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_remote_path: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "StandaloneSftpTransferMode::is_local_remote"
    )]
    pub transfer_mode: StandaloneSftpTransferMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_endpoint: Option<StandaloneSftpEndpoint>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
}

pub struct SaveStandaloneSftpProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub group: Option<String>,
    pub notes: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub icon_background_color: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SavedAuth,
    pub connect_timeout_seconds: u64,
    pub proxy_chain: Vec<SavedProxyHop>,
    pub upstream_proxy: SavedUpstreamProxyPolicy,
    pub proxy_command: Option<SavedProxyCommand>,
    pub identity_agent: Option<String>,
    pub legacy_ssh_compatibility: bool,
    pub ssh_algorithms: SshAlgorithmPreferences,
    pub initial_remote_path: Option<String>,
    pub transfer_mode: StandaloneSftpTransferMode,
    pub secondary_endpoint: Option<StandaloneSftpEndpoint>,
}

impl fmt::Debug for SaveStandaloneSftpProfileRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Requests may own temporary credentials before the store moves them into keychain.
        formatter
            .debug_struct("SaveStandaloneSftpProfileRequest")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("group", &self.group)
            .field("has_notes", &self.notes.is_some())
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth_type", &self.auth.auth_type())
            .field("connect_timeout_seconds", &self.connect_timeout_seconds)
            .field("proxy_chain_len", &self.proxy_chain.len())
            .field("upstream_proxy", &self.upstream_proxy)
            .field("has_proxy_command", &self.proxy_command.is_some())
            .field("identity_agent", &self.identity_agent)
            .field("legacy_ssh_compatibility", &self.legacy_ssh_compatibility)
            .field("initial_remote_path", &self.initial_remote_path)
            .field("transfer_mode", &self.transfer_mode)
            .field("secondary_endpoint", &self.secondary_endpoint)
            .finish()
    }
}

/// Owns protected values for one endpoint during a standalone SFTP connection attempt.
pub struct SavedStandaloneSftpEndpointRuntimeSecrets {
    pub auth: Option<SecretString>,
    pub proxy_chain: Vec<Option<SecretString>>,
    pub upstream_proxy: Option<SecretString>,
    pub proxy_command: Option<SecretString>,
}

impl fmt::Debug for SavedStandaloneSftpEndpointRuntimeSecrets {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SavedStandaloneSftpEndpointRuntimeSecrets")
            .field("auth", &self.auth.as_ref().map(|_| "[redacted secret]"))
            .field(
                "proxy_chain",
                &self
                    .proxy_chain
                    .iter()
                    .map(|secret| secret.as_ref().map(|_| "[redacted secret]"))
                    .collect::<Vec<_>>(),
            )
            .field(
                "upstream_proxy",
                &self.upstream_proxy.as_ref().map(|_| "[redacted secret]"),
            )
            .field(
                "proxy_command",
                &self.proxy_command.as_ref().map(|_| "[redacted secret]"),
            )
            .finish()
    }
}

/// Owns protected values only for the lifetime of one standalone SFTP connection attempt.
pub struct SavedStandaloneSftpProfileRuntimeSecrets {
    pub auth: Option<SecretString>,
    pub proxy_chain: Vec<Option<SecretString>>,
    pub upstream_proxy: Option<SecretString>,
    pub proxy_command: Option<SecretString>,
    pub secondary_endpoint: Option<SavedStandaloneSftpEndpointRuntimeSecrets>,
}

impl fmt::Debug for SavedStandaloneSftpProfileRuntimeSecrets {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SavedStandaloneSftpProfileRuntimeSecrets")
            .field("auth", &self.auth.as_ref().map(|_| "[redacted secret]"))
            .field(
                "proxy_chain",
                &self
                    .proxy_chain
                    .iter()
                    .map(|secret| secret.as_ref().map(|_| "[redacted secret]"))
                    .collect::<Vec<_>>(),
            )
            .field(
                "upstream_proxy",
                &self.upstream_proxy.as_ref().map(|_| "[redacted secret]"),
            )
            .field(
                "proxy_command",
                &self.proxy_command.as_ref().map(|_| "[redacted secret]"),
            )
            .field("secondary_endpoint", &self.secondary_endpoint)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RemoteDesktopProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Free-form user metadata. UI copy warns against storing credentials here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_background_color: Option<String>,
    pub protocol: RemoteDesktopProtocol,
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Saved SSH connection used to reach this endpoint through a local tunnel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_gateway_connection_id: Option<String>,
    /// Stable protected-store reference; the credential value is never serialized here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub read_only: bool,
    #[serde(default)]
    pub session_options: RemoteDesktopSessionOptions,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default)]
pub struct SaveRemoteDesktopProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub group: Option<String>,
    pub notes: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub icon_background_color: Option<String>,
    pub protocol: RemoteDesktopProtocol,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub domain: Option<String>,
    pub ssh_gateway_connection_id: Option<String>,
    /// An explicit reference is primarily used by trusted import and sync paths.
    pub credential_ref: Option<String>,
    /// The store moves this secret into the protected credential backend.
    pub credential: Option<SecretString>,
    /// Explicitly removes the device-local protected credential while updating the profile.
    pub clear_credential: bool,
    pub read_only: bool,
    pub session_options: RemoteDesktopSessionOptions,
}

impl SerialProfile {
    pub fn new(name: impl Into<String>, port_path: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            group: None,
            notes: None,
            icon: None,
            color: None,
            icon_background_color: None,
            port_path: port_path.into(),
            baud_rate: 115_200,
            data_bits: 8,
            stop_bits: 1,
            parity: SerialParity::None,
            flow_control: SerialFlowControl::None,
            terminal: ConnectionTerminalOptions::default(),
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
            notes: None,
            icon: None,
            color: None,
            icon_background_color: None,
            host: host.into(),
            port,
            terminal: ConnectionTerminalOptions::default(),
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

impl MoshProfile {
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        ssh_port: u16,
        username: impl Into<String>,
        auth: SavedAuth,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            group: None,
            notes: None,
            icon: None,
            color: None,
            icon_background_color: None,
            host: host.into(),
            ssh_port,
            username: username.into(),
            auth,
            proxy_chain: Vec::new(),
            server_executable: default_mosh_server_executable(),
            udp_host_override: None,
            udp_port: MoshUdpPortSelection::Automatic,
            ip_family: MoshIpFamily::Auto,
            prediction: MoshPredictionMode::Adaptive,
            locale: None,
            identity_agent: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
            terminal: ConnectionTerminalOptions::default(),
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("Mosh profile id is required");
        }
        if self.name.trim().is_empty() {
            bail!("Mosh profile name is required");
        }
        if self.host.trim().is_empty() {
            bail!("Mosh host is required");
        }
        if self.ssh_port == 0 {
            bail!("Mosh SSH port must be greater than zero");
        }
        if self.username.trim().is_empty() {
            bail!("Mosh username is required");
        }
        if self.server_executable.trim().is_empty() {
            bail!("Mosh server executable is required");
        }
        match self.udp_port {
            MoshUdpPortSelection::Automatic => {}
            MoshUdpPortSelection::Fixed { port: 0 }
            | MoshUdpPortSelection::Range { start: 0, .. }
            | MoshUdpPortSelection::Range { end: 0, .. } => {
                bail!("Mosh UDP port must be greater than zero")
            }
            MoshUdpPortSelection::Range { start, end } if start > end => {
                bail!("Mosh UDP port range is reversed")
            }
            MoshUdpPortSelection::Fixed { .. } | MoshUdpPortSelection::Range { .. } => {}
        }
        if self.locale.as_deref().is_some_and(|locale| {
            locale.trim().is_empty() || locale.contains(['\0', '\r', '\n'])
        }) {
            bail!("Mosh locale is invalid");
        }
        self.ssh_algorithms.validate()?;
        for hop in &self.proxy_chain {
            hop.ssh_algorithms.validate()?;
        }
        Ok(())
    }
}

impl StandaloneSftpProfile {
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        auth: SavedAuth,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            version: CONFIG_VERSION,
            name: name.into(),
            group: None,
            notes: None,
            icon: None,
            color: None,
            icon_background_color: None,
            host: host.into(),
            port,
            username: username.into(),
            auth,
            connect_timeout_seconds: DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS,
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            proxy_command: None,
            identity_agent: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
            initial_remote_path: None,
            transfer_mode: StandaloneSftpTransferMode::LocalRemote,
            secondary_endpoint: None,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("Standalone SFTP profile id is required");
        }
        if self.name.trim().is_empty() {
            bail!("Standalone SFTP profile name is required");
        }
        if self.host.trim().is_empty() {
            bail!("Standalone SFTP host is required");
        }
        if self.port == 0 {
            bail!("Standalone SFTP port must be greater than zero");
        }
        if self.connect_timeout_seconds == 0 {
            bail!("Standalone SFTP connect timeout must be greater than zero");
        }
        if self.username.trim().is_empty() {
            bail!("Standalone SFTP username is required");
        }
        self.ssh_algorithms.validate()?;
        for hop in &self.proxy_chain {
            hop.ssh_algorithms.validate()?;
        }
        if self.transfer_mode == StandaloneSftpTransferMode::RemoteRemote {
            self.secondary_endpoint
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Standalone SFTP secondary endpoint is required"))?
                .validate()?;
        }
        Ok(())
    }
}

impl RemoteDesktopProfile {
    pub fn new(
        name: impl Into<String>,
        protocol: RemoteDesktopProtocol,
        host: impl Into<String>,
        port: u16,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            group: None,
            notes: None,
            icon: None,
            color: None,
            icon_background_color: None,
            protocol,
            host: host.into(),
            port,
            username: None,
            domain: None,
            ssh_gateway_connection_id: None,
            credential_ref: None,
            read_only: false,
            session_options: RemoteDesktopSessionOptions::default(),
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("Remote desktop profile id is required");
        }
        if self.name.trim().is_empty() {
            bail!("Remote desktop profile name is required");
        }
        if self.host.trim().is_empty() {
            bail!("Remote desktop host is required");
        }
        if self.port == 0 {
            bail!("Remote desktop port must be greater than zero");
        }
        if self
            .ssh_gateway_connection_id
            .as_deref()
            .is_some_and(|connection_id| connection_id.trim().is_empty())
        {
            bail!("Remote desktop SSH gateway connection id cannot be empty");
        }
        if self
            .credential_ref
            .as_deref()
            .is_some_and(|reference| reference.trim().is_empty())
        {
            bail!("Remote desktop credential reference cannot be empty");
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
    pub notes: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SavedAuth,
    pub proxy_chain: Vec<SavedProxyHop>,
    pub upstream_proxy: SavedUpstreamProxyPolicy,
    pub proxy_command: Option<SavedProxyCommand>,
    pub color: Option<String>,
    pub icon_background_color: Option<String>,
    pub icon: Option<String>,
    pub tags: Vec<String>,
    pub connect_timeout_seconds: u64,
    pub agent_forwarding: bool,
    pub identity_agent: Option<String>,
    pub agent_forwarding_socket: Option<String>,
    pub legacy_ssh_compatibility: bool,
    pub ssh_algorithms: SshAlgorithmPreferences,
    pub dedicated_new_terminal_connection: bool,
    pub x11_forwarding: ConnectionX11ForwardingOptions,
    pub post_connect_command: Option<String>,
    pub terminal: ConnectionTerminalOptions,
}

/// Returns the original plaintext allocations after persistence for one runtime handoff.
///
/// The saved record never owns these values. Dropping this bundle zeroizes every secret.
pub struct SavedConnectionRuntimeSecrets {
    pub auth: Option<SecretString>,
    pub proxy_chain: Vec<Option<SecretString>>,
    pub upstream_proxy: Option<SecretString>,
    pub proxy_command: Option<SecretString>,
}

/// Identifies one typed secret-bearing slot without exposing its protected-store key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionCredentialSlot {
    Primary,
    ProxyHop { index: usize },
    UpstreamProxy,
}

impl fmt::Debug for SavedConnectionRuntimeSecrets {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SavedConnectionRuntimeSecrets")
            .field("auth", &self.auth.as_ref().map(|_| "[redacted secret]"))
            .field(
                "proxy_chain",
                &self
                    .proxy_chain
                    .iter()
                    .map(|secret| secret.as_ref().map(|_| "[redacted secret]"))
                    .collect::<Vec<_>>(),
            )
            .field(
                "upstream_proxy",
                &self
                    .upstream_proxy
                    .as_ref()
                    .map(|_| "[redacted secret]"),
            )
            .field(
                "proxy_command",
                &self
                    .proxy_command
                    .as_ref()
                    .map(|_| "[redacted secret]"),
            )
            .finish()
    }
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
    pub mosh_profiles: Vec<MoshProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub standalone_sftp_profiles: Vec<StandaloneSftpProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remote_desktop_profiles: Vec<RemoteDesktopProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_privilege_credentials: Vec<SavedPrivilegeCredential>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_keychain_cleanup: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_privilege_keychain_cleanup: Vec<String>,
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
            mosh_profiles: Vec::new(),
            standalone_sftp_profiles: Vec::new(),
            remote_desktop_profiles: Vec::new(),
            local_privilege_credentials: Vec::new(),
            pending_keychain_cleanup: Vec::new(),
            pending_privilege_keychain_cleanup: Vec::new(),
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
pub struct TelnetProfilesSyncSnapshot {
    pub revision: String,
    pub exported_at: String,
    #[serde(default)]
    pub records: Vec<TelnetProfile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoshProfilesSyncSnapshot {
    pub revision: String,
    pub exported_at: String,
    #[serde(default)]
    pub records: Vec<MoshProfile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandaloneSftpProfilesSyncSnapshot {
    pub revision: String,
    pub exported_at: String,
    #[serde(default)]
    pub records: Vec<StandaloneSftpProfile>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopProfilesSyncSnapshot {
    pub revision: String,
    pub exported_at: String,
    #[serde(default)]
    pub records: Vec<RemoteDesktopProfile>,
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
    /// Full connection options were added after the initial sync format. Keep
    /// this optional so existing cloud snapshots remain readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<ConnectionOptions>,
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
