use super::saved_auth::SavedPasswordLoadError;
use std::fmt;

use oxideterm_connections::{
    AuthType, ConnectionInfo, ConnectionTerminalOptions, ConnectionX11ForwardingOptions,
    DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS, MoshIpFamily, MoshPredictionMode, MoshProfile,
    MoshUdpPortSelection, RemoteDesktopProfile, SavedAuth, SavedConnection, SavedProxyHop,
    SavedUpstreamProxyProtocol, SerialProfile, SshAlgorithmPreferences, SshChannelStrategy,
    StandaloneSftpTransferMode, TelnetProfile, TransportUsernameTransition,
    transport_port_replacement, transport_username_transition,
};
pub use oxideterm_connections::{
    ConnectionTransport as NewConnectionTransport, RDP_DEFAULT_PORT_TEXT, SSH_DEFAULT_PORT_TEXT,
    TELNET_DEFAULT_PORT_TEXT, VNC_DEFAULT_PORT_TEXT,
};

/// Shared geometry keeps textarea rendering and IME hit testing aligned.
pub const CONNECTION_NOTES_LINE_HEIGHT: f32 = 20.0;
pub const CONNECTION_NOTES_MIN_HEIGHT: f32 = 84.0;
pub const CONNECTION_NOTES_VERTICAL_PADDING: f32 = 8.0;
use oxideterm_remote_desktop::{
    RemoteDesktopProviderCapabilities, RemoteDesktopSessionOptions, RemoteDesktopVncCompression,
    RemoteDesktopVncImageQuality, RemoteDesktopVncOptions, RemoteDesktopVncSecurityPolicy,
    RemoteDesktopVncSessionMode,
};
use zeroize::Zeroize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SshAuthTab {
    Password,
    DefaultKey,
    SshKey,
    ManagedKey,
    Certificate,
    Agent,
    TwoFactor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SshAuthFamily {
    Password,
    Key,
    Agent,
    TwoFactor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SshKeyAuthSource {
    DefaultKey,
    SshKey,
    ManagedKey,
    Certificate,
}

pub fn auth_family_from_tab(tab: SshAuthTab) -> SshAuthFamily {
    match tab {
        SshAuthTab::Password => SshAuthFamily::Password,
        SshAuthTab::DefaultKey
        | SshAuthTab::SshKey
        | SshAuthTab::ManagedKey
        | SshAuthTab::Certificate => SshAuthFamily::Key,
        SshAuthTab::Agent => SshAuthFamily::Agent,
        SshAuthTab::TwoFactor => SshAuthFamily::TwoFactor,
    }
}

pub fn key_source_from_tab(tab: SshAuthTab) -> Option<SshKeyAuthSource> {
    match tab {
        SshAuthTab::DefaultKey => Some(SshKeyAuthSource::DefaultKey),
        SshAuthTab::SshKey => Some(SshKeyAuthSource::SshKey),
        SshAuthTab::ManagedKey => Some(SshKeyAuthSource::ManagedKey),
        SshAuthTab::Certificate => Some(SshKeyAuthSource::Certificate),
        SshAuthTab::Password | SshAuthTab::Agent | SshAuthTab::TwoFactor => None,
    }
}

pub fn auth_tab_from_key_source(source: SshKeyAuthSource) -> SshAuthTab {
    match source {
        SshKeyAuthSource::DefaultKey => SshAuthTab::DefaultKey,
        SshKeyAuthSource::SshKey => SshAuthTab::SshKey,
        SshKeyAuthSource::ManagedKey => SshAuthTab::ManagedKey,
        SshKeyAuthSource::Certificate => SshAuthTab::Certificate,
    }
}

pub fn default_auth_tab_for_family(family: SshAuthFamily) -> SshAuthTab {
    match family {
        SshAuthFamily::Password => SshAuthTab::Password,
        // The grouped Key entry opens the file-key form first. Other key
        // sources are explicit choices inside the secondary selector.
        SshAuthFamily::Key => SshAuthTab::SshKey,
        SshAuthFamily::Agent => SshAuthTab::Agent,
        SshAuthFamily::TwoFactor => SshAuthTab::TwoFactor,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SavedConnectionPromptAction {
    Connect,
    Tunnel,
    Test,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewConnectionSubmitAction {
    Connect,
    Save,
    SaveAndConnect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewConnectionFormMode {
    NewConnection,
    SavedConnectionPrompt,
    EditProperties,
    DuplicateTemplate,
}

impl NewConnectionFormMode {
    pub fn submits_saved_connection_properties(self) -> bool {
        matches!(self, Self::EditProperties | Self::DuplicateTemplate)
    }
}

pub fn new_connection_form_mode(
    editing_saved_connection_id: Option<&str>,
    duplicating_saved_connection_id: Option<&str>,
    prompt_action: Option<SavedConnectionPromptAction>,
) -> NewConnectionFormMode {
    if prompt_action.is_some() {
        NewConnectionFormMode::SavedConnectionPrompt
    } else if duplicating_saved_connection_id.is_some() {
        NewConnectionFormMode::DuplicateTemplate
    } else if editing_saved_connection_id.is_some() {
        NewConnectionFormMode::EditProperties
    } else {
        NewConnectionFormMode::NewConnection
    }
}

pub fn connection_icon_field_visible(
    mode: NewConnectionFormMode,
    drill_down_mode: bool,
    transport: NewConnectionTransport,
) -> bool {
    // Only persisted session assets expose custom icons in this shared form.
    mode != NewConnectionFormMode::SavedConnectionPrompt
        && !drill_down_mode
        && oxideterm_connections::transport_is_persistable(transport)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewConnectionSelect {
    Totp,
    JumpTotp,
    Group,
    KeyAuthSource,
    ManagedKey,
    StandaloneSftpSecondaryKeyAuthSource,
    StandaloneSftpSecondaryManagedKey,
    JumpSavedConnection,
    TunnelHost,
    JumpKeyAuthSource,
    JumpManagedKey,
    UpstreamProxyPolicy,
    UpstreamProxyProtocol,
    UpstreamProxyAuth,
    StandaloneSftpSecondaryUpstreamProxyPolicy,
    StandaloneSftpSecondaryUpstreamProxyProtocol,
    StandaloneSftpSecondaryUpstreamProxyAuth,
    RemoteDesktopSshGateway,
    LocalShell,
    SerialPort,
    BleAdapter,
    SerialDataBits,
    SerialStopBits,
    SerialParity,
    SerialFlowControl,
    TerminalEncoding,
    TerminalBackspaceSequence,
    TerminalDeleteSequence,
    TelnetInputLineEnding,
    TelnetOutputLineEnding,
    BleInputLineEnding,
    BleOutputLineEnding,
    BleSendMode,
    BleDisplayMode,
    TerminalSemanticScheme,
    TerminalHighlightRuleSet,
    TerminalSessionLogPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewConnectionUpstreamProxyPolicy {
    UseGlobal,
    Direct,
    Custom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewConnectionUpstreamProxyAuth {
    None,
    Password,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionRouteTarget {
    #[default]
    Primary,
    StandaloneSftpSecondary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum NewConnectionField {
    Name,
    LocalCwd,
    Host,
    Port,
    Username,
    Password,
    KeyPath,
    ManagedKeyId,
    CertPath,
    Passphrase,
    GssapiServerIdentity,
    IdentityAgent,
    Group,
    Notes,
    InitialRemotePath,
    ConnectTimeoutSeconds,
    StandaloneSftpSecondaryHost,
    StandaloneSftpSecondaryPort,
    StandaloneSftpSecondaryUsername,
    StandaloneSftpSecondaryPassword,
    StandaloneSftpSecondaryKeyPath,
    StandaloneSftpSecondaryManagedKeyId,
    StandaloneSftpSecondaryCertPath,
    StandaloneSftpSecondaryPassphrase,
    StandaloneSftpSecondaryGssapiServerIdentity,
    StandaloneSftpSecondaryIdentityAgent,
    StandaloneSftpSecondaryInitialRemotePath,
    StandaloneSftpSecondaryConnectTimeoutSeconds,
    StandaloneSftpSecondaryProxyCommand,
    StandaloneSftpSecondaryUpstreamProxyHost,
    StandaloneSftpSecondaryUpstreamProxyPort,
    StandaloneSftpSecondaryUpstreamProxyNoProxy,
    StandaloneSftpSecondaryUpstreamProxyUsername,
    StandaloneSftpSecondaryUpstreamProxyPassword,
    PostConnectCommand,
    PreConnectCommand,
    ProxyCommand,
    Color,
    IconBackgroundColor,
    JumpHost,
    JumpPort,
    JumpUsername,
    JumpPassword,
    JumpKeyPath,
    JumpManagedKeyId,
    JumpCertPath,
    JumpPassphrase,
    JumpGssapiServerIdentity,
    JumpIdentityAgent,
    UpstreamProxyHost,
    UpstreamProxyPort,
    UpstreamProxyNoProxy,
    UpstreamProxyUsername,
    UpstreamProxyPassword,
    SerialPortPath,
    SerialBaudRate,
    SerialProfileName,
    TelnetProfileName,
    BleProfileName,
    BleMtu,
    MoshServerExecutable,
    MoshUdpHost,
    MoshUdpPort,
    MoshLocale,
    TunnelTargetHost,
    TunnelTargetPort,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteDesktopSessionFeature {
    ClipboardText,
    ClipboardImages,
    ClipboardFiles,
    AudioPlayback,
    AudioCapture,
    MultiMonitor,
    DisableRdpGraphicsPipeline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteDesktopVncPreference {
    Security(RemoteDesktopVncSecurityPolicy),
    SessionMode(RemoteDesktopVncSessionMode),
    ImageQuality(RemoteDesktopVncImageQuality),
    Compression(RemoteDesktopVncCompression),
}

/// Reads one VNC preference for segmented-control rendering.
pub fn remote_desktop_vnc_preference_selected(
    options: &RemoteDesktopVncOptions,
    preference: RemoteDesktopVncPreference,
) -> bool {
    match preference {
        RemoteDesktopVncPreference::Security(value) => options.security_policy == value,
        RemoteDesktopVncPreference::SessionMode(value) => options.session_mode == value,
        RemoteDesktopVncPreference::ImageQuality(value) => options.image_quality == value,
        RemoteDesktopVncPreference::Compression(value) => options.compression == value,
    }
}

/// Applies only the VNC preference represented by the selected segment.
pub fn apply_remote_desktop_vnc_preference(
    options: &mut RemoteDesktopVncOptions,
    preference: RemoteDesktopVncPreference,
) {
    match preference {
        RemoteDesktopVncPreference::Security(value) => options.security_policy = value,
        RemoteDesktopVncPreference::SessionMode(value) => options.session_mode = value,
        RemoteDesktopVncPreference::ImageQuality(value) => options.image_quality = value,
        RemoteDesktopVncPreference::Compression(value) => options.compression = value,
    }
}

/// Keeps provider support separate from the user's per-session selection.
pub fn remote_desktop_feature_supported(
    capabilities: &RemoteDesktopProviderCapabilities,
    feature: RemoteDesktopSessionFeature,
) -> bool {
    match feature {
        RemoteDesktopSessionFeature::ClipboardText => capabilities.clipboard_text,
        RemoteDesktopSessionFeature::ClipboardImages => capabilities.clipboard_data,
        RemoteDesktopSessionFeature::ClipboardFiles => capabilities.clipboard_files,
        RemoteDesktopSessionFeature::AudioPlayback => capabilities.audio_playback,
        RemoteDesktopSessionFeature::AudioCapture => capabilities.audio_capture,
        RemoteDesktopSessionFeature::MultiMonitor => capabilities.multi_monitor,
        // Compatibility controls are client policy rather than provider capabilities.
        RemoteDesktopSessionFeature::DisableRdpGraphicsPipeline => true,
    }
}

/// Reads one feature without duplicating the nested options layout in the view.
pub fn remote_desktop_feature_selected(
    options: &RemoteDesktopSessionOptions,
    feature: RemoteDesktopSessionFeature,
) -> bool {
    match feature {
        RemoteDesktopSessionFeature::ClipboardText => options.clipboard.text,
        RemoteDesktopSessionFeature::ClipboardImages => options.clipboard.images,
        RemoteDesktopSessionFeature::ClipboardFiles => options.clipboard.files,
        RemoteDesktopSessionFeature::AudioPlayback => options.audio.playback,
        RemoteDesktopSessionFeature::AudioCapture => options.audio.capture,
        RemoteDesktopSessionFeature::MultiMonitor => options.display.use_all_monitors,
        RemoteDesktopSessionFeature::DisableRdpGraphicsPipeline => {
            options.rdp.disable_graphics_pipeline
        }
    }
}

/// Mutates only the option represented by the clicked feature row.
pub fn toggle_remote_desktop_feature(
    options: &mut RemoteDesktopSessionOptions,
    feature: RemoteDesktopSessionFeature,
) {
    let selected = remote_desktop_feature_selected(options, feature);
    match feature {
        RemoteDesktopSessionFeature::ClipboardText => options.clipboard.text = !selected,
        RemoteDesktopSessionFeature::ClipboardImages => options.clipboard.images = !selected,
        RemoteDesktopSessionFeature::ClipboardFiles => options.clipboard.files = !selected,
        RemoteDesktopSessionFeature::AudioPlayback => options.audio.playback = !selected,
        RemoteDesktopSessionFeature::AudioCapture => options.audio.capture = !selected,
        RemoteDesktopSessionFeature::MultiMonitor => {
            options.display.use_all_monitors = !selected;
        }
        RemoteDesktopSessionFeature::DisableRdpGraphicsPipeline => {
            options.rdp.disable_graphics_pipeline = !selected;
        }
    }
}

pub struct NewConnectionProxyHop {
    pub totp_credential_id: Option<String>,
    pub saved_connection_id: String,
    pub persisted_proxy_hop_index: Option<usize>,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_tab: SshAuthTab,
    pub password: String,
    pub empty_password: bool,
    pub key_path: String,
    pub managed_key_id: String,
    pub cert_path: String,
    pub passphrase: String,
    pub gssapi_enabled: bool,
    pub gssapi_server_identity: String,
    pub gssapi_delegate_credentials: bool,
    pub agent_forwarding: bool,
    pub identity_agent: String,
    pub agent_forwarding_socket: Option<String>,
    pub legacy_ssh_compatibility: bool,
    pub ssh_algorithms: SshAlgorithmPreferences,
}

impl fmt::Debug for NewConnectionProxyHop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewConnectionProxyHop")
            .field("saved_connection_id", &self.saved_connection_id)
            .field("persisted_proxy_hop_index", &self.persisted_proxy_hop_index)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth_tab", &self.auth_tab)
            .field("password", &"[redacted secret]")
            .field("key_path", &self.key_path)
            .field("managed_key_id", &self.managed_key_id)
            .field("cert_path", &self.cert_path)
            .field("passphrase", &"[redacted secret]")
            .field("gssapi_enabled", &self.gssapi_enabled)
            .field(
                "gssapi_server_identity_configured",
                &!self.gssapi_server_identity.trim().is_empty(),
            )
            .field(
                "gssapi_delegate_credentials",
                &self.gssapi_delegate_credentials,
            )
            .field("agent_forwarding", &self.agent_forwarding)
            .field(
                "identity_agent_configured",
                &identity_agent_selector(&self.identity_agent).is_some(),
            )
            .field(
                "agent_forwarding_socket_configured",
                &self.agent_forwarding_socket.is_some(),
            )
            .field("legacy_ssh_compatibility", &self.legacy_ssh_compatibility)
            .finish()
    }
}

impl NewConnectionProxyHop {
    pub fn new() -> Self {
        Self {
            totp_credential_id: None,
            saved_connection_id: String::new(),
            persisted_proxy_hop_index: None,
            host: String::new(),
            port: SSH_DEFAULT_PORT_TEXT.to_string(),
            username: String::new(),
            auth_tab: SshAuthTab::SshKey,
            password: String::new(),
            empty_password: false,
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
            agent_forwarding: false,
            identity_agent: String::new(),
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
        }
    }

    pub fn from_saved(persisted_proxy_hop_index: usize, hop: &SavedProxyHop) -> Self {
        // Reopen only route metadata; protected credentials stay in their current owner.
        Self {
            totp_credential_id: hop.totp_credential_id.clone(),
            saved_connection_id: String::new(),
            persisted_proxy_hop_index: Some(persisted_proxy_hop_index),
            host: hop.host.clone(),
            port: hop.port.to_string(),
            username: hop.username.clone(),
            auth_tab: ssh_auth_tab_from_saved_auth(&hop.auth),
            password: String::new(),
            empty_password: hop.auth.uses_empty_password(),
            key_path: hop.auth.key_path().unwrap_or_default().to_string(),
            managed_key_id: hop.auth.managed_key_id().unwrap_or_default().to_string(),
            cert_path: hop.auth.cert_path().unwrap_or_default().to_string(),
            passphrase: String::new(),
            gssapi_enabled: hop.auth.gssapi_options().is_some(),
            gssapi_server_identity: hop
                .auth
                .gssapi_options()
                .and_then(|(identity, _)| identity.map(ToOwned::to_owned))
                .unwrap_or_default(),
            gssapi_delegate_credentials: hop
                .auth
                .gssapi_options()
                .is_some_and(|(_, delegate)| delegate),
            agent_forwarding: hop.agent_forwarding,
            identity_agent: hop.identity_agent.clone().unwrap_or_default(),
            agent_forwarding_socket: hop.agent_forwarding_socket.clone(),
            legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
            ssh_algorithms: hop.ssh_algorithms.clone(),
        }
    }

    pub fn complete(&self) -> bool {
        !self.host.trim().is_empty() && !self.username.trim().is_empty()
    }

    pub fn has_explicit_secret_draft(&self) -> bool {
        match self.auth_tab {
            SshAuthTab::Password => self.empty_password || !self.password.is_empty(),
            SshAuthTab::DefaultKey
            | SshAuthTab::SshKey
            | SshAuthTab::ManagedKey
            | SshAuthTab::Certificate => !self.passphrase.is_empty(),
            SshAuthTab::Agent | SshAuthTab::TwoFactor => false,
        }
    }

    pub fn matches_saved_connection(&self, connection: &SavedConnection) -> bool {
        // A saved secret is reusable only while every authentication endpoint field still
        // matches the selected connection, preventing credentials from reaching an edited host.
        self.saved_connection_id == connection.id
            && self.totp_credential_id == connection.options.totp_credential_id
            && self.host.trim() == connection.host
            && self.port.trim().parse::<u16>().ok() == Some(connection.port)
            && self.username.trim() == connection.username
            && self.auth_tab == ssh_auth_tab_from_saved_auth(&connection.auth)
            && self.empty_password == connection.auth.uses_empty_password()
            && self.key_path.trim() == connection.auth.key_path().unwrap_or_default()
            && self.cert_path.trim() == connection.auth.cert_path().unwrap_or_default()
            && self.managed_key_id.trim() == connection.auth.managed_key_id().unwrap_or_default()
            && self.gssapi_server_identity.trim()
                == connection
                    .auth
                    .gssapi_options()
                    .and_then(|(identity, _)| identity)
                    .unwrap_or_default()
            && self.gssapi_delegate_credentials
                == connection
                    .auth
                    .gssapi_options()
                    .is_some_and(|(_, delegate)| delegate)
    }

    fn zeroize_secret_drafts(&mut self) {
        self.password.zeroize();
        self.passphrase.zeroize();
    }

    pub fn apply_saved_connection(&mut self, connection: &ConnectionInfo) {
        self.totp_credential_id = connection.totp_credential_id.clone();
        self.saved_connection_id = connection.id.clone();
        self.empty_password = connection.empty_password;
        self.persisted_proxy_hop_index = None;
        self.host = connection.host.clone();
        self.port = connection.port.to_string();
        self.username = connection.username.clone();
        self.auth_tab = match connection.auth_type {
            AuthType::Password => SshAuthTab::Password,
            AuthType::Key
                if connection
                    .key_path
                    .as_deref()
                    .unwrap_or_default()
                    .is_empty() =>
            {
                SshAuthTab::DefaultKey
            }
            AuthType::Key => SshAuthTab::SshKey,
            AuthType::ManagedKey => SshAuthTab::ManagedKey,
            AuthType::Certificate => SshAuthTab::Certificate,
            AuthType::KeyboardInteractive => SshAuthTab::TwoFactor,
            AuthType::Agent => SshAuthTab::Agent,
        };
        // ConnectionInfo is metadata-only. Keep keychain-backed passwords and
        // passphrases out of the form when reusing a saved connection as a hop.
        self.password.clear();
        self.passphrase.clear();
        self.key_path = connection.key_path.clone().unwrap_or_default();
        self.cert_path = connection.cert_path.clone().unwrap_or_default();
        self.managed_key_id = connection.managed_key_id.clone().unwrap_or_default();
        self.gssapi_enabled = connection.gssapi_authentication;
        self.gssapi_server_identity = connection
            .gssapi_server_identity
            .clone()
            .unwrap_or_default();
        self.gssapi_delegate_credentials = connection.gssapi_delegate_credentials;
        self.agent_forwarding = connection.agent_forwarding;
        self.identity_agent = connection.identity_agent.clone().unwrap_or_default();
        self.agent_forwarding_socket = connection.agent_forwarding_socket.clone();
        self.legacy_ssh_compatibility = connection.legacy_ssh_compatibility;
        self.ssh_algorithms = connection.ssh_algorithms.clone();
    }
}

pub fn ssh_auth_tab_from_saved_auth(auth: &SavedAuth) -> SshAuthTab {
    match auth.conventional_fallback() {
        SavedAuth::Password { .. } => SshAuthTab::Password,
        SavedAuth::Key { key_path, .. } if key_path.is_empty() => SshAuthTab::DefaultKey,
        SavedAuth::Key { .. } => SshAuthTab::SshKey,
        SavedAuth::ManagedKey { .. } => SshAuthTab::ManagedKey,
        SavedAuth::Certificate { .. } => SshAuthTab::Certificate,
        SavedAuth::KeyboardInteractive => SshAuthTab::TwoFactor,
        SavedAuth::Agent => SshAuthTab::Agent,
        SavedAuth::KerberosPreferred { .. } => unreachable!("fallback authentication is flattened"),
    }
}

impl Drop for NewConnectionProxyHop {
    fn drop(&mut self) {
        // Proxy credentials remain in one form owner and are scrubbed on removal.
        self.zeroize_secret_drafts();
    }
}

pub struct StandaloneSftpSecondaryForm {
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_tab: SshAuthTab,
    pub password: String,
    pub empty_password: bool,
    pub password_keychain_id: Option<String>,
    pub password_visible: bool,
    pub key_path: String,
    pub managed_key_id: String,
    pub cert_path: String,
    pub passphrase: String,
    pub gssapi_enabled: bool,
    pub gssapi_server_identity: String,
    pub gssapi_delegate_credentials: bool,
    pub passphrase_visible: bool,
    pub save_password: bool,
    pub identity_agent: String,
    pub agent_available: Option<bool>,
    pub legacy_ssh_compatibility: bool,
    pub ssh_algorithms: SshAlgorithmPreferences,
    pub connect_timeout_seconds: u64,
    /// Preserves transient invalid input while the numeric value fails closed at zero.
    pub connect_timeout_seconds_text: String,
    pub initial_remote_path: String,
    pub proxy_hops: Vec<NewConnectionProxyHop>,
    pub proxy_chain_expanded: bool,
    pub proxy_command_enabled: bool,
    pub proxy_command: String,
    pub proxy_command_keychain_id: Option<String>,
    pub upstream_proxy_policy: NewConnectionUpstreamProxyPolicy,
    pub upstream_proxy_protocol: SavedUpstreamProxyProtocol,
    pub upstream_proxy_host: String,
    pub upstream_proxy_port: String,
    pub upstream_proxy_auth: NewConnectionUpstreamProxyAuth,
    pub upstream_proxy_username: String,
    pub upstream_proxy_password: String,
    pub upstream_proxy_password_keychain_id: Option<String>,
    pub upstream_proxy_remote_dns: bool,
    pub upstream_proxy_no_proxy: String,
}

impl Default for StandaloneSftpSecondaryForm {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: SSH_DEFAULT_PORT_TEXT.to_string(),
            username: "root".to_string(),
            auth_tab: SshAuthTab::Password,
            password: String::new(),
            empty_password: false,
            password_keychain_id: None,
            password_visible: false,
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
            passphrase_visible: false,
            save_password: false,
            identity_agent: String::new(),
            agent_available: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
            connect_timeout_seconds: DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS,
            connect_timeout_seconds_text: DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS.to_string(),
            initial_remote_path: String::new(),
            proxy_hops: Vec::new(),
            proxy_chain_expanded: false,
            proxy_command_enabled: false,
            proxy_command: String::new(),
            proxy_command_keychain_id: None,
            upstream_proxy_policy: NewConnectionUpstreamProxyPolicy::UseGlobal,
            upstream_proxy_protocol: SavedUpstreamProxyProtocol::Socks5,
            upstream_proxy_host: "127.0.0.1".to_string(),
            upstream_proxy_port: "1080".to_string(),
            upstream_proxy_auth: NewConnectionUpstreamProxyAuth::None,
            upstream_proxy_username: String::new(),
            upstream_proxy_password: String::new(),
            upstream_proxy_password_keychain_id: None,
            upstream_proxy_remote_dns: true,
            upstream_proxy_no_proxy: String::new(),
        }
    }
}

impl fmt::Debug for StandaloneSftpSecondaryForm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StandaloneSftpSecondaryForm")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth_tab", &self.auth_tab)
            .field("password", &"[redacted secret]")
            .field("password_keychain_id", &self.password_keychain_id)
            .field("key_path", &self.key_path)
            .field("managed_key_id", &self.managed_key_id)
            .field("cert_path", &self.cert_path)
            .field("passphrase", &"[redacted secret]")
            .field("gssapi_enabled", &self.gssapi_enabled)
            .field(
                "gssapi_server_identity_configured",
                &!self.gssapi_server_identity.trim().is_empty(),
            )
            .field(
                "gssapi_delegate_credentials",
                &self.gssapi_delegate_credentials,
            )
            .field("save_password", &self.save_password)
            .field(
                "identity_agent_configured",
                &identity_agent_selector(&self.identity_agent).is_some(),
            )
            .field("legacy_ssh_compatibility", &self.legacy_ssh_compatibility)
            .field("connect_timeout_seconds", &self.connect_timeout_seconds)
            .field("initial_remote_path", &self.initial_remote_path)
            .field("proxy_hops", &self.proxy_hops)
            .field("proxy_chain_expanded", &self.proxy_chain_expanded)
            .field("proxy_command_enabled", &self.proxy_command_enabled)
            .field("proxy_command", &"[redacted secret]")
            .field("proxy_command_keychain_id", &self.proxy_command_keychain_id)
            .field("upstream_proxy_policy", &self.upstream_proxy_policy)
            .field("upstream_proxy_protocol", &self.upstream_proxy_protocol)
            .field("upstream_proxy_host", &self.upstream_proxy_host)
            .field("upstream_proxy_port", &self.upstream_proxy_port)
            .field("upstream_proxy_auth", &self.upstream_proxy_auth)
            .field("upstream_proxy_username", &self.upstream_proxy_username)
            .field("upstream_proxy_password", &"[redacted secret]")
            .field(
                "upstream_proxy_password_keychain_id",
                &self.upstream_proxy_password_keychain_id,
            )
            .field("upstream_proxy_remote_dns", &self.upstream_proxy_remote_dns)
            .field("upstream_proxy_no_proxy", &self.upstream_proxy_no_proxy)
            .finish()
    }
}

impl StandaloneSftpSecondaryForm {
    fn zeroize_secret_drafts(&mut self) {
        self.password.zeroize();
        self.passphrase.zeroize();
        self.proxy_command.zeroize();
        self.upstream_proxy_password.zeroize();
        for hop in &mut self.proxy_hops {
            hop.zeroize_secret_drafts();
        }
    }
}

impl Drop for StandaloneSftpSecondaryForm {
    fn drop(&mut self) {
        // The second endpoint owns its drafts independently and scrubs them on form teardown.
        self.zeroize_secret_drafts();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionFormDrawer {
    Route,
    SshSecurity,
    Terminal,
    Notes,
    MoshOptions,
    RemoteFeatures,
    VncPreferences,
    SerialParameters,
    BleTransportOptions,
    FtpOptions,
}

pub struct NewConnectionForm {
    pub tunnel_target_host: String,
    pub tunnel_target_port: String,
    pub totp_credential_id: Option<String>,
    // Reauthentication submits into the existing logical session, never a second sidebar row.
    pub standalone_connection_id: Option<String>,
    pub transport: NewConnectionTransport,
    pub local_profile_id: Option<String>,
    pub local_cwd: String,
    /// Selects a discovered shell without retaining environment credentials.
    pub local_shell_id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_tab: SshAuthTab,
    pub gssapi_enabled: bool,
    pub gssapi_server_identity: String,
    pub gssapi_delegate_credentials: bool,
    pub gssapi_credentials_available: Option<bool>,
    pub gssapi_credentials_check_pending: bool,
    pub password: String,
    pub empty_password: bool,
    pub remote_desktop_session_options: RemoteDesktopSessionOptions,
    /// Identifies an existing RDP/VNC asset without overloading SSH edit state.
    pub remote_desktop_profile_id: Option<String>,
    /// References saved SSH metadata only; credentials remain in the protected store.
    pub remote_desktop_ssh_gateway_connection_id: Option<String>,
    /// Identifies an existing Mosh asset without creating an SSH node edit owner.
    pub mosh_profile_id: Option<String>,
    /// Identifies an existing serial asset without changing a live serial session.
    pub serial_profile_id: Option<String>,
    /// Identifies an existing Telnet asset without changing a live Telnet session.
    pub telnet_profile_id: Option<String>,
    pub telnet_input_line_ending: oxideterm_connections::SerialLineEnding,
    pub telnet_output_line_ending: oxideterm_connections::SerialLineEnding,
    pub ftp_profile_id: Option<String>,
    pub ftp_tls: bool,
    pub ftp_attempt: Option<(uuid::Uuid, tokio_util::sync::CancellationToken)>,
    /// Identifies an independent SFTP asset without creating a NodeRouter node.
    pub standalone_sftp_profile_id: Option<String>,
    /// Controls whether the dual-pane SFTP surface has one or two authenticated remotes.
    pub standalone_sftp_transfer_mode: StandaloneSftpTransferMode,
    /// Owns the second endpoint only while editing a remote-to-remote profile.
    pub standalone_sftp_secondary: StandaloneSftpSecondaryForm,
    pub saved_password_keychain_id: Option<String>,
    pub password_loaded: bool,
    // A revealed value is presentation state until the user edits it.
    pub password_from_store: bool,
    pub password_load_id: Option<u64>,
    pub password_load_error: Option<SavedPasswordLoadError>,
    pub password_visible: bool,
    pub key_path: String,
    pub managed_key_id: String,
    pub cert_path: String,
    pub passphrase: String,
    pub passphrase_visible: bool,
    pub save_password: bool,
    pub group: String,
    pub notes: String,
    pub sftp_initial_remote_path: String,
    pub post_connect_command: String,
    pub pre_connect_command: String,
    pub proxy_command_enabled: bool,
    pub proxy_command: String,
    pub proxy_command_keychain_id: Option<String>,
    pub color: String,
    pub icon_background_color: String,
    pub icon: String,
    pub icon_picker_expanded: bool,
    // None uses the default expanded state; user toggles remain transient UI state.
    pub basic_section_expanded: Option<bool>,
    pub authentication_section_expanded: Option<bool>,
    pub route_section_expanded: Option<bool>,
    pub standalone_sftp_secondary_route_section_expanded: Option<bool>,
    pub ssh_options_section_expanded: Option<bool>,
    pub terminal_section_expanded: Option<bool>,
    pub appearance_section_expanded: Option<bool>,
    pub remote_gateway_section_expanded: Option<bool>,
    pub vnc_preferences_section_expanded: Option<bool>,
    pub remote_features_section_expanded: Option<bool>,
    pub serial_parameters_section_expanded: Option<bool>,
    pub mosh_options_section_expanded: Option<bool>,
    pub sftp_options_section_expanded: Option<bool>,
    pub local_shell_section_expanded: Option<bool>,
    /// Keeps the uncommon transport group out of the primary list until requested.
    pub advanced_connections_expanded: bool,
    /// Single-open in-place capability drawer shown below the composer chip rail.
    pub active_drawer: Option<ConnectionFormDrawer>,
    /// Floating categorized protocol picker opened from the header transport pill.
    pub transport_picker_open: bool,
    /// Floating icon and accent color picker opened from the header session avatar.
    pub appearance_popover_open: bool,
    /// Expands the optional TOTP selector inside the authentication card on demand.
    pub totp_picker_expanded: bool,
    pub tags: Vec<String>,
    pub proxy_hops: Vec<NewConnectionProxyHop>,
    pub proxy_chain_expanded: bool,
    pub jump_server_form: Option<NewConnectionProxyHop>,
    /// Identifies a hop temporarily moved into the editor so cancel can restore it in place.
    pub jump_server_edit_index: Option<usize>,
    /// Selects which independently owned endpoint receives the pending jump host.
    pub jump_server_target: ConnectionRouteTarget,
    pub upstream_proxy_policy: NewConnectionUpstreamProxyPolicy,
    pub upstream_proxy_protocol: SavedUpstreamProxyProtocol,
    pub upstream_proxy_host: String,
    pub upstream_proxy_port: String,
    pub upstream_proxy_auth: NewConnectionUpstreamProxyAuth,
    pub upstream_proxy_username: String,
    pub upstream_proxy_password: String,
    pub upstream_proxy_password_keychain_id: Option<String>,
    pub upstream_proxy_remote_dns: bool,
    pub upstream_proxy_no_proxy: String,
    pub agent_forwarding: bool,
    pub identity_agent: String,
    pub agent_forwarding_socket: Option<String>,
    pub legacy_ssh_compatibility: bool,
    pub ssh_algorithms: SshAlgorithmPreferences,
    pub ssh_algorithm_editor_open: bool,
    pub ssh_algorithm_selected: Option<String>,
    pub ssh_algorithm_menu: Option<(String, gpui::Point<gpui::Pixels>)>,
    pub ssh_algorithm_editor_category: oxideterm_ssh::SshAlgorithmCategory,
    pub connect_timeout_seconds: u64,
    /// Preserves transient invalid input while the numeric value fails closed at zero.
    pub connect_timeout_seconds_text: String,
    pub dedicated_new_terminal_connection: bool,
    pub open_sftp_on_connect: bool,
    pub ssh_channel_strategy: SshChannelStrategy,
    pub x11_forwarding: ConnectionX11ForwardingOptions,
    pub terminal: ConnectionTerminalOptions,
    pub agent_available: Option<bool>,
    pub save_connection: bool,
    pub field_focused: bool,
    pub focused_field: NewConnectionField,
    pub selected_field: Option<NewConnectionField>,
    pub error: Option<String>,
    // Success styling remains bound to the exact feedback message that produced it.
    pub success_feedback_message: Option<String>,
    pub pending: bool,
    pub serial_ports: Vec<oxideterm_terminal::SerialPortInfo>,
    pub serial_ports_loading: bool,
    pub serial_port_path: String,
    pub serial_baud_rate: String,
    pub serial_data_bits: u8,
    pub serial_stop_bits: u8,
    pub serial_parity: oxideterm_terminal::SerialParity,
    pub serial_flow_control: oxideterm_terminal::SerialFlowControl,
    pub serial_profile_name: String,
    pub ble: BleFormState,
    pub telnet_profile_name: String,
    pub mosh_server_executable: String,
    pub mosh_udp_host: String,
    pub mosh_udp_port: String,
    pub mosh_locale: String,
    pub mosh_ip_family: MoshIpFamily,
    pub mosh_prediction: MoshPredictionMode,
}

/// BLE-specific draft and discovery view state of the new-connection form.
/// Grouped so the shared form draft keeps other transports' fields flat while
/// the BLE surface stays in one resettable unit; nothing here is persisted —
/// saving still materializes a `BleProfile` into the connection store.
pub struct BleFormState {
    pub instance_id: uuid::Uuid,
    pub open_cancel: Option<tokio_util::sync::DropGuard>,
    pub discovery_section_expanded: Option<bool>,
    pub profile_name: String,
    // Set only when the form edits an existing BLE profile; the save request
    // reuses the stored device identity when no live scan selection is made.
    pub profile_id: Option<String>,
    pub adapters: Vec<oxideterm_ble::BleAdapterInfo>,
    pub adapters_loading: bool,
    pub selected_adapter_index: usize,
    pub devices: Vec<oxideterm_ble::BleDeviceInfo>,
    pub scanning: bool,
    pub scan_started: bool,
    // Holds the adapter that produced `devices` so inspection and the
    // terminal session resolve peripherals against the same BlueZ object
    // (Linux) or peripheral cache (macOS/Windows); the scan it owns stays
    // running until the terminal link is up or the form is dismissed.
    pub adapter_guard: Option<oxideterm_ble::BleAdapterGuard>,
    // Monotonic token: in-flight discovery writes back only when the generation
    // still matches, so stale adapter or scan results never reach the form.
    pub scan_generation: u64,
    pub selected_device_id: Option<oxideterm_ble::BlePeripheralId>,
    pub device_details: Option<oxideterm_ble::BleDeviceDetails>,
    pub inspecting: bool,
    // Inspection failure is scoped to the selected device's expanded row, not
    // the whole form, so switching devices drops it instead of leaking across rows.
    pub inspect_error: Option<String>,
    // The service selected inside the expanded details table; drives whether the
    // open button can start a read/write (notify + write) channel.
    pub selected_service_uuid: Option<uuid::Uuid>,
    // Byte-stream transport options. Shared with SerialRuntimeOptions so the
    // terminal panel and the saved-profile store consume the same shape.
    pub input_line_ending: oxideterm_connections::SerialLineEnding,
    pub output_line_ending: oxideterm_connections::SerialLineEnding,
    pub send_mode: oxideterm_connections::SerialSendMode,
    pub display_mode: oxideterm_connections::SerialDisplayMode,
    pub local_echo: bool,
    /// Draft text for the optional ATT MTU override; parsed at submit time.
    pub mtu: String,
    /// Prefer the write-without-response characteristic. Only editable while
    /// the selected service exposes one; the enabled state is derived from the
    /// inspected device details at render time, not stored here.
    pub write_without_response: bool,
    pub transport_section_expanded: Option<bool>,
}

impl Default for BleFormState {
    fn default() -> Self {
        Self {
            instance_id: uuid::Uuid::new_v4(),
            open_cancel: None,
            discovery_section_expanded: None,
            profile_name: String::new(),
            profile_id: None,
            adapters: Vec::new(),
            adapters_loading: false,
            selected_adapter_index: 0,
            devices: Vec::new(),
            scanning: false,
            scan_started: false,
            adapter_guard: None,
            scan_generation: 0,
            selected_device_id: None,
            device_details: None,
            inspecting: false,
            inspect_error: None,
            selected_service_uuid: None,
            input_line_ending: oxideterm_connections::SerialLineEnding::default(),
            output_line_ending: oxideterm_connections::SerialLineEnding::default(),
            send_mode: oxideterm_connections::SerialSendMode::default(),
            display_mode: oxideterm_connections::SerialDisplayMode::default(),
            local_echo: false,
            mtu: String::new(),
            write_without_response: true,
            transport_section_expanded: None,
        }
    }
}

impl fmt::Debug for BleFormState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BleFormState")
            .field(
                "discovery_section_expanded",
                &self.discovery_section_expanded,
            )
            .field("adapters", &self.adapters)
            .field("adapters_loading", &self.adapters_loading)
            .field("selected_adapter_index", &self.selected_adapter_index)
            .field("devices", &self.devices)
            .field("scanning", &self.scanning)
            .field("scan_started", &self.scan_started)
            .field("adapter_guard", &self.adapter_guard.is_some())
            .field("scan_generation", &self.scan_generation)
            .field("selected_device", &self.selected_device_id.is_some())
            .field(
                "device_details_services",
                &self
                    .device_details
                    .as_ref()
                    .map(|details| details.services.len()),
            )
            .field("inspecting", &self.inspecting)
            .field("inspect_error", &self.inspect_error.is_some())
            .field(
                "selected_service_uuid",
                &self.selected_service_uuid.is_some(),
            )
            .finish()
    }
}

impl fmt::Debug for NewConnectionForm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewConnectionForm")
            .field("transport", &self.transport)
            .field("local_shell_id", &self.local_shell_id)
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth_tab", &self.auth_tab)
            .field("gssapi_enabled", &self.gssapi_enabled)
            .field(
                "gssapi_credentials_available",
                &self.gssapi_credentials_available,
            )
            .field("password", &"[redacted secret]")
            .field(
                "remote_desktop_session_options",
                &self.remote_desktop_session_options,
            )
            .field("remote_desktop_profile_id", &self.remote_desktop_profile_id)
            .field(
                "remote_desktop_ssh_gateway_connection_id",
                &self.remote_desktop_ssh_gateway_connection_id,
            )
            .field("mosh_profile_id", &self.mosh_profile_id)
            .field("serial_profile_id", &self.serial_profile_id)
            .field("telnet_profile_id", &self.telnet_profile_id)
            .field(
                "standalone_sftp_profile_id",
                &self.standalone_sftp_profile_id,
            )
            .field(
                "standalone_sftp_transfer_mode",
                &self.standalone_sftp_transfer_mode,
            )
            .field("standalone_sftp_secondary", &self.standalone_sftp_secondary)
            .field(
                "saved_password_keychain_id",
                &self.saved_password_keychain_id,
            )
            .field("password_loaded", &self.password_loaded)
            .field("password_visible", &self.password_visible)
            .field("key_path", &self.key_path)
            .field("managed_key_id", &self.managed_key_id)
            .field("cert_path", &self.cert_path)
            .field("passphrase", &"[redacted secret]")
            .field("passphrase_visible", &self.passphrase_visible)
            .field("save_password", &self.save_password)
            .field("group", &self.group)
            // Notes are user-authored free text and may contain sensitive context.
            .field("notes_present", &!self.notes.is_empty())
            .field("sftp_initial_remote_path", &self.sftp_initial_remote_path)
            .field("post_connect_command", &self.post_connect_command)
            .field(
                "pre_connect_command_configured",
                &!self.pre_connect_command.is_empty(),
            )
            .field("proxy_command_enabled", &self.proxy_command_enabled)
            .field("proxy_command", &"[redacted secret]")
            .field("proxy_command_keychain_id", &self.proxy_command_keychain_id)
            .field("color", &self.color)
            .field("icon_background_color", &self.icon_background_color)
            .field("icon", &self.icon)
            .field("icon_picker_expanded", &self.icon_picker_expanded)
            .field("basic_section_expanded", &self.basic_section_expanded)
            .field(
                "authentication_section_expanded",
                &self.authentication_section_expanded,
            )
            .field("route_section_expanded", &self.route_section_expanded)
            .field(
                "ssh_options_section_expanded",
                &self.ssh_options_section_expanded,
            )
            .field("terminal_section_expanded", &self.terminal_section_expanded)
            .field(
                "appearance_section_expanded",
                &self.appearance_section_expanded,
            )
            .field(
                "remote_gateway_section_expanded",
                &self.remote_gateway_section_expanded,
            )
            .field(
                "vnc_preferences_section_expanded",
                &self.vnc_preferences_section_expanded,
            )
            .field(
                "remote_features_section_expanded",
                &self.remote_features_section_expanded,
            )
            .field(
                "serial_parameters_section_expanded",
                &self.serial_parameters_section_expanded,
            )
            .field(
                "mosh_options_section_expanded",
                &self.mosh_options_section_expanded,
            )
            .field(
                "sftp_options_section_expanded",
                &self.sftp_options_section_expanded,
            )
            .field(
                "local_shell_section_expanded",
                &self.local_shell_section_expanded,
            )
            .field(
                "advanced_connections_expanded",
                &self.advanced_connections_expanded,
            )
            .field("tags", &self.tags)
            .field("proxy_hops", &self.proxy_hops)
            .field("proxy_chain_expanded", &self.proxy_chain_expanded)
            .field("jump_server_form", &self.jump_server_form)
            .field("jump_server_edit_index", &self.jump_server_edit_index)
            .field("upstream_proxy_policy", &self.upstream_proxy_policy)
            .field("upstream_proxy_protocol", &self.upstream_proxy_protocol)
            .field("upstream_proxy_host", &self.upstream_proxy_host)
            .field("upstream_proxy_port", &self.upstream_proxy_port)
            .field("upstream_proxy_auth", &self.upstream_proxy_auth)
            .field("upstream_proxy_username", &self.upstream_proxy_username)
            .field("upstream_proxy_password", &"[redacted secret]")
            .field(
                "upstream_proxy_password_keychain_id",
                &self.upstream_proxy_password_keychain_id,
            )
            .field("upstream_proxy_remote_dns", &self.upstream_proxy_remote_dns)
            .field("upstream_proxy_no_proxy", &self.upstream_proxy_no_proxy)
            .field("agent_forwarding", &self.agent_forwarding)
            .field(
                "identity_agent_configured",
                &identity_agent_selector(&self.identity_agent).is_some(),
            )
            .field(
                "agent_forwarding_socket_configured",
                &self.agent_forwarding_socket.is_some(),
            )
            .field("legacy_ssh_compatibility", &self.legacy_ssh_compatibility)
            .field("connect_timeout_seconds", &self.connect_timeout_seconds)
            .field(
                "dedicated_new_terminal_connection",
                &self.dedicated_new_terminal_connection,
            )
            .field("ssh_channel_strategy", &self.ssh_channel_strategy)
            .field("x11_forwarding", &self.x11_forwarding)
            .field("terminal", &self.terminal)
            .field("agent_available", &self.agent_available)
            .field("save_connection", &self.save_connection)
            .field("field_focused", &self.field_focused)
            .field("focused_field", &self.focused_field)
            .field("selected_field", &self.selected_field)
            .field("error", &self.error)
            .field("pending", &self.pending)
            .field("serial_ports", &self.serial_ports)
            .field("serial_ports_loading", &self.serial_ports_loading)
            .field("serial_port_path", &self.serial_port_path)
            .field("serial_baud_rate", &self.serial_baud_rate)
            .field("serial_data_bits", &self.serial_data_bits)
            .field("serial_stop_bits", &self.serial_stop_bits)
            .field("serial_parity", &self.serial_parity)
            .field("serial_flow_control", &self.serial_flow_control)
            .field("serial_profile_name", &self.serial_profile_name)
            .field("ble", &self.ble)
            .field("telnet_profile_name", &self.telnet_profile_name)
            .field("mosh_server_executable", &self.mosh_server_executable)
            .field("mosh_udp_host", &self.mosh_udp_host)
            .field("mosh_udp_port", &self.mosh_udp_port)
            .field("mosh_locale", &self.mosh_locale)
            .field("mosh_ip_family", &self.mosh_ip_family)
            .field("mosh_prediction", &self.mosh_prediction)
            .finish()
    }
}

impl Default for NewConnectionForm {
    fn default() -> Self {
        Self {
            tunnel_target_host: String::new(),
            tunnel_target_port: String::new(),
            totp_credential_id: None,
            standalone_connection_id: None,
            transport: NewConnectionTransport::Ssh,
            local_shell_id: None,
            local_profile_id: None,
            local_cwd: String::new(),
            name: String::new(),
            host: String::new(),
            port: SSH_DEFAULT_PORT_TEXT.to_string(),
            username: "root".to_string(),
            auth_tab: SshAuthTab::Password,
            gssapi_enabled: false,
            gssapi_server_identity: String::new(),
            gssapi_delegate_credentials: false,
            gssapi_credentials_available: None,
            gssapi_credentials_check_pending: false,
            password: String::new(),
            empty_password: false,
            remote_desktop_session_options: RemoteDesktopSessionOptions::default(),
            remote_desktop_profile_id: None,
            remote_desktop_ssh_gateway_connection_id: None,
            mosh_profile_id: None,
            serial_profile_id: None,
            telnet_profile_id: None,
            telnet_input_line_ending: Default::default(),
            telnet_output_line_ending: Default::default(),
            ftp_profile_id: None,
            ftp_tls: true,
            ftp_attempt: None,
            standalone_sftp_profile_id: None,
            standalone_sftp_transfer_mode: StandaloneSftpTransferMode::LocalRemote,
            standalone_sftp_secondary: StandaloneSftpSecondaryForm::default(),
            saved_password_keychain_id: None,
            password_loaded: true,
            password_from_store: false,
            password_load_id: None,
            password_load_error: None,
            password_visible: false,
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            passphrase_visible: false,
            save_password: false,
            group: String::new(),
            notes: String::new(),
            sftp_initial_remote_path: String::new(),
            post_connect_command: String::new(),
            pre_connect_command: String::new(),
            proxy_command_enabled: false,
            proxy_command: String::new(),
            proxy_command_keychain_id: None,
            color: String::new(),
            icon_background_color: String::new(),
            icon: String::new(),
            icon_picker_expanded: false,
            basic_section_expanded: None,
            authentication_section_expanded: None,
            route_section_expanded: None,
            standalone_sftp_secondary_route_section_expanded: None,
            ssh_options_section_expanded: None,
            terminal_section_expanded: None,
            appearance_section_expanded: None,
            remote_gateway_section_expanded: None,
            vnc_preferences_section_expanded: None,
            remote_features_section_expanded: None,
            serial_parameters_section_expanded: None,
            mosh_options_section_expanded: None,
            sftp_options_section_expanded: None,
            local_shell_section_expanded: None,
            advanced_connections_expanded: false,
            active_drawer: None,
            transport_picker_open: false,
            appearance_popover_open: false,
            totp_picker_expanded: false,
            tags: Vec::new(),
            proxy_hops: Vec::new(),
            proxy_chain_expanded: false,
            jump_server_form: None,
            jump_server_edit_index: None,
            jump_server_target: ConnectionRouteTarget::Primary,
            upstream_proxy_policy: NewConnectionUpstreamProxyPolicy::UseGlobal,
            upstream_proxy_protocol: SavedUpstreamProxyProtocol::Socks5,
            upstream_proxy_host: "127.0.0.1".to_string(),
            upstream_proxy_port: "1080".to_string(),
            upstream_proxy_auth: NewConnectionUpstreamProxyAuth::None,
            upstream_proxy_username: String::new(),
            upstream_proxy_password: String::new(),
            upstream_proxy_password_keychain_id: None,
            upstream_proxy_remote_dns: true,
            upstream_proxy_no_proxy: String::new(),
            agent_forwarding: false,
            identity_agent: String::new(),
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
            ssh_algorithm_editor_open: false,
            ssh_algorithm_selected: None,
            ssh_algorithm_menu: None,
            ssh_algorithm_editor_category: oxideterm_ssh::SshAlgorithmCategory::Kex,
            connect_timeout_seconds: DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS,
            connect_timeout_seconds_text: DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS.to_string(),
            dedicated_new_terminal_connection: false,
            open_sftp_on_connect: false,
            ssh_channel_strategy: SshChannelStrategy::default(),
            x11_forwarding: ConnectionX11ForwardingOptions::default(),
            terminal: ConnectionTerminalOptions::default(),
            agent_available: None,
            save_connection: false,
            field_focused: true,
            focused_field: NewConnectionField::Name,
            selected_field: None,
            error: None,
            success_feedback_message: None,
            pending: false,
            serial_ports: Vec::new(),
            serial_ports_loading: false,
            serial_port_path: String::new(),
            serial_baud_rate: "115200".to_string(),
            serial_data_bits: 8,
            serial_stop_bits: 1,
            serial_parity: oxideterm_terminal::SerialParity::None,
            serial_flow_control: oxideterm_terminal::SerialFlowControl::None,
            serial_profile_name: String::new(),
            ble: BleFormState::default(),
            telnet_profile_name: String::new(),
            mosh_server_executable: "mosh-server".to_string(),
            mosh_udp_host: String::new(),
            mosh_udp_port: String::new(),
            mosh_locale: String::new(),
            mosh_ip_family: MoshIpFamily::Auto,
            mosh_prediction: MoshPredictionMode::Adaptive,
        }
    }
}

impl NewConnectionForm {
    pub fn feedback_is_success(&self) -> bool {
        self.error
            .as_ref()
            .is_some_and(|message| self.success_feedback_message.as_ref() == Some(message))
    }

    pub fn set_standalone_sftp_transfer_mode(&mut self, mode: StandaloneSftpTransferMode) {
        if mode == StandaloneSftpTransferMode::LocalRemote
            && self.standalone_sftp_transfer_mode == StandaloneSftpTransferMode::RemoteRemote
        {
            // Hidden second-endpoint credentials must not outlive the selected topology.
            self.standalone_sftp_secondary.zeroize_secret_drafts();
        }
        self.standalone_sftp_transfer_mode = mode;
    }

    fn zeroize_secret_drafts(&mut self) {
        self.password.zeroize();
        self.passphrase.zeroize();
        self.upstream_proxy_password.zeroize();
        self.proxy_command.zeroize();
    }
}

impl Drop for NewConnectionForm {
    fn drop(&mut self) {
        if let Some((_, cancel)) = &self.ftp_attempt {
            cancel.cancel();
        }
        // GPUI inputs require plain String drafts, so scrub them at owner teardown.
        self.zeroize_secret_drafts();
    }
}

mod editing;
mod navigation;
mod profiles;

pub use editing::*;
pub use navigation::*;
pub use profiles::*;

#[cfg(test)]
mod tests;
