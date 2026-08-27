use std::fmt;

use oxideterm_connections::{
    AuthType, ConnectionInfo, ConnectionTerminalOptions, ConnectionX11ForwardingOptions,
    DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS, MoshIpFamily, MoshPredictionMode, MoshProfile,
    MoshUdpPortSelection, RemoteDesktopProfile, SavedAuth, SavedConnection, SavedProxyHop,
    SavedUpstreamProxyProtocol, SerialProfile, SshAlgorithmPreferences, SshChannelStrategy,
    StandaloneSftpTransferMode, TelnetProfile, TransportUsernameTransition,
    transport_port_replacement, transport_username_transition,
};
pub(in crate::workspace) use oxideterm_connections::{
    ConnectionTransport as NewConnectionTransport, RDP_DEFAULT_PORT_TEXT, SSH_DEFAULT_PORT_TEXT,
    TELNET_DEFAULT_PORT_TEXT, VNC_DEFAULT_PORT_TEXT,
};

/// Shared geometry keeps textarea rendering and IME hit testing aligned.
pub(in crate::workspace) const CONNECTION_NOTES_LINE_HEIGHT: f32 = 20.0;
pub(in crate::workspace) const CONNECTION_NOTES_MIN_HEIGHT: f32 = 84.0;
pub(in crate::workspace) const CONNECTION_NOTES_VERTICAL_PADDING: f32 = 8.0;
use oxideterm_remote_desktop::{
    RemoteDesktopProviderCapabilities, RemoteDesktopSessionOptions, RemoteDesktopVncCompression,
    RemoteDesktopVncImageQuality, RemoteDesktopVncOptions, RemoteDesktopVncSecurityPolicy,
    RemoteDesktopVncSessionMode,
};
use zeroize::Zeroize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SshAuthTab {
    Password,
    DefaultKey,
    SshKey,
    ManagedKey,
    Certificate,
    Agent,
    TwoFactor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SshAuthFamily {
    Password,
    Key,
    Agent,
    TwoFactor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum SshKeyAuthSource {
    DefaultKey,
    SshKey,
    ManagedKey,
    Certificate,
}

pub(in crate::workspace) fn auth_family_from_tab(tab: SshAuthTab) -> SshAuthFamily {
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

pub(in crate::workspace) fn key_source_from_tab(tab: SshAuthTab) -> Option<SshKeyAuthSource> {
    match tab {
        SshAuthTab::DefaultKey => Some(SshKeyAuthSource::DefaultKey),
        SshAuthTab::SshKey => Some(SshKeyAuthSource::SshKey),
        SshAuthTab::ManagedKey => Some(SshKeyAuthSource::ManagedKey),
        SshAuthTab::Certificate => Some(SshKeyAuthSource::Certificate),
        SshAuthTab::Password | SshAuthTab::Agent | SshAuthTab::TwoFactor => None,
    }
}

pub(in crate::workspace) fn auth_tab_from_key_source(source: SshKeyAuthSource) -> SshAuthTab {
    match source {
        SshKeyAuthSource::DefaultKey => SshAuthTab::DefaultKey,
        SshKeyAuthSource::SshKey => SshAuthTab::SshKey,
        SshKeyAuthSource::ManagedKey => SshAuthTab::ManagedKey,
        SshKeyAuthSource::Certificate => SshAuthTab::Certificate,
    }
}

pub(in crate::workspace) fn default_auth_tab_for_family(family: SshAuthFamily) -> SshAuthTab {
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
pub(in crate::workspace) enum SavedConnectionPromptAction {
    Connect,
    Test,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NewConnectionSubmitAction {
    Connect,
    Save,
    SaveAndConnect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NewConnectionFormMode {
    NewConnection,
    SavedConnectionPrompt,
    EditProperties,
    DuplicateTemplate,
}

impl NewConnectionFormMode {
    pub(in crate::workspace) fn submits_saved_connection_properties(self) -> bool {
        matches!(self, Self::EditProperties | Self::DuplicateTemplate)
    }
}

pub(in crate::workspace) fn new_connection_form_mode(
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

pub(in crate::workspace) fn connection_icon_field_visible(
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
pub(in crate::workspace) enum NewConnectionSelect {
    Group,
    KeyAuthSource,
    ManagedKey,
    StandaloneSftpSecondaryKeyAuthSource,
    StandaloneSftpSecondaryManagedKey,
    JumpSavedConnection,
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
    SerialDataBits,
    SerialStopBits,
    SerialParity,
    SerialFlowControl,
    TerminalEncoding,
    TerminalBackspaceSequence,
    TerminalDeleteSequence,
    TerminalSemanticScheme,
    TerminalHighlightRuleSet,
    TerminalSessionLogPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NewConnectionUpstreamProxyPolicy {
    UseGlobal,
    Direct,
    Custom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NewConnectionUpstreamProxyAuth {
    None,
    Password,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::workspace) enum ConnectionRouteTarget {
    #[default]
    Primary,
    StandaloneSftpSecondary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(in crate::workspace) enum NewConnectionField {
    Name,
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
    MoshServerExecutable,
    MoshUdpHost,
    MoshUdpPort,
    MoshLocale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum RemoteDesktopSessionFeature {
    ClipboardText,
    ClipboardImages,
    ClipboardFiles,
    AudioPlayback,
    AudioCapture,
    MultiMonitor,
    DisableRdpGraphicsPipeline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum RemoteDesktopVncPreference {
    Security(RemoteDesktopVncSecurityPolicy),
    SessionMode(RemoteDesktopVncSessionMode),
    ImageQuality(RemoteDesktopVncImageQuality),
    Compression(RemoteDesktopVncCompression),
}

/// Reads one VNC preference for segmented-control rendering.
pub(in crate::workspace) fn remote_desktop_vnc_preference_selected(
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
pub(in crate::workspace) fn apply_remote_desktop_vnc_preference(
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
pub(in crate::workspace) fn remote_desktop_feature_supported(
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
pub(in crate::workspace) fn remote_desktop_feature_selected(
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
pub(in crate::workspace) fn toggle_remote_desktop_feature(
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

pub(in crate::workspace) struct NewConnectionProxyHop {
    pub(in crate::workspace) saved_connection_id: String,
    pub(in crate::workspace) persisted_proxy_hop_index: Option<usize>,
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: String,
    pub(in crate::workspace) username: String,
    pub(in crate::workspace) auth_tab: SshAuthTab,
    pub(in crate::workspace) password: String,
    pub(in crate::workspace) key_path: String,
    pub(in crate::workspace) managed_key_id: String,
    pub(in crate::workspace) cert_path: String,
    pub(in crate::workspace) passphrase: String,
    pub(in crate::workspace) gssapi_enabled: bool,
    pub(in crate::workspace) gssapi_server_identity: String,
    pub(in crate::workspace) gssapi_delegate_credentials: bool,
    pub(in crate::workspace) agent_forwarding: bool,
    pub(in crate::workspace) identity_agent: String,
    pub(in crate::workspace) agent_forwarding_socket: Option<String>,
    pub(in crate::workspace) legacy_ssh_compatibility: bool,
    pub(in crate::workspace) ssh_algorithms: SshAlgorithmPreferences,
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
    pub(in crate::workspace) fn new() -> Self {
        Self {
            saved_connection_id: String::new(),
            persisted_proxy_hop_index: None,
            host: String::new(),
            port: SSH_DEFAULT_PORT_TEXT.to_string(),
            username: String::new(),
            auth_tab: SshAuthTab::SshKey,
            password: String::new(),
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

    pub(in crate::workspace) fn from_saved(
        persisted_proxy_hop_index: usize,
        hop: &SavedProxyHop,
    ) -> Self {
        // Reopen only route metadata; protected credentials stay in their current owner.
        Self {
            saved_connection_id: String::new(),
            persisted_proxy_hop_index: Some(persisted_proxy_hop_index),
            host: hop.host.clone(),
            port: hop.port.to_string(),
            username: hop.username.clone(),
            auth_tab: ssh_auth_tab_from_saved_auth(&hop.auth),
            password: String::new(),
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

    pub(in crate::workspace) fn complete(&self) -> bool {
        !self.host.trim().is_empty() && !self.username.trim().is_empty()
    }

    pub(in crate::workspace) fn has_explicit_secret_draft(&self) -> bool {
        match self.auth_tab {
            SshAuthTab::Password => !self.password.is_empty(),
            SshAuthTab::DefaultKey
            | SshAuthTab::SshKey
            | SshAuthTab::ManagedKey
            | SshAuthTab::Certificate => !self.passphrase.is_empty(),
            SshAuthTab::Agent | SshAuthTab::TwoFactor => false,
        }
    }

    pub(in crate::workspace) fn matches_saved_connection(
        &self,
        connection: &SavedConnection,
    ) -> bool {
        // A saved secret is reusable only while every authentication endpoint field still
        // matches the selected connection, preventing credentials from reaching an edited host.
        self.saved_connection_id == connection.id
            && self.host.trim() == connection.host
            && self.port.trim().parse::<u16>().ok() == Some(connection.port)
            && self.username.trim() == connection.username
            && self.auth_tab == ssh_auth_tab_from_saved_auth(&connection.auth)
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

    pub(in crate::workspace) fn apply_saved_connection(&mut self, connection: &ConnectionInfo) {
        self.saved_connection_id = connection.id.clone();
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

pub(in crate::workspace) fn ssh_auth_tab_from_saved_auth(auth: &SavedAuth) -> SshAuthTab {
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

pub(in crate::workspace) struct StandaloneSftpSecondaryForm {
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: String,
    pub(in crate::workspace) username: String,
    pub(in crate::workspace) auth_tab: SshAuthTab,
    pub(in crate::workspace) password: String,
    pub(in crate::workspace) password_keychain_id: Option<String>,
    pub(in crate::workspace) password_visible: bool,
    pub(in crate::workspace) key_path: String,
    pub(in crate::workspace) managed_key_id: String,
    pub(in crate::workspace) cert_path: String,
    pub(in crate::workspace) passphrase: String,
    pub(in crate::workspace) gssapi_enabled: bool,
    pub(in crate::workspace) gssapi_server_identity: String,
    pub(in crate::workspace) gssapi_delegate_credentials: bool,
    pub(in crate::workspace) passphrase_visible: bool,
    pub(in crate::workspace) save_password: bool,
    pub(in crate::workspace) identity_agent: String,
    pub(in crate::workspace) agent_available: Option<bool>,
    pub(in crate::workspace) legacy_ssh_compatibility: bool,
    pub(in crate::workspace) ssh_algorithms: SshAlgorithmPreferences,
    pub(in crate::workspace) connect_timeout_seconds: u64,
    /// Preserves transient invalid input while the numeric value fails closed at zero.
    pub(in crate::workspace) connect_timeout_seconds_text: String,
    pub(in crate::workspace) initial_remote_path: String,
    pub(in crate::workspace) proxy_hops: Vec<NewConnectionProxyHop>,
    pub(in crate::workspace) proxy_chain_expanded: bool,
    pub(in crate::workspace) proxy_command_enabled: bool,
    pub(in crate::workspace) proxy_command: String,
    pub(in crate::workspace) proxy_command_keychain_id: Option<String>,
    pub(in crate::workspace) upstream_proxy_policy: NewConnectionUpstreamProxyPolicy,
    pub(in crate::workspace) upstream_proxy_protocol: SavedUpstreamProxyProtocol,
    pub(in crate::workspace) upstream_proxy_host: String,
    pub(in crate::workspace) upstream_proxy_port: String,
    pub(in crate::workspace) upstream_proxy_auth: NewConnectionUpstreamProxyAuth,
    pub(in crate::workspace) upstream_proxy_username: String,
    pub(in crate::workspace) upstream_proxy_password: String,
    pub(in crate::workspace) upstream_proxy_password_keychain_id: Option<String>,
    pub(in crate::workspace) upstream_proxy_remote_dns: bool,
    pub(in crate::workspace) upstream_proxy_no_proxy: String,
}

impl Default for StandaloneSftpSecondaryForm {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: SSH_DEFAULT_PORT_TEXT.to_string(),
            username: "root".to_string(),
            auth_tab: SshAuthTab::Password,
            password: String::new(),
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

pub(in crate::workspace) struct NewConnectionForm {
    pub(in crate::workspace) transport: NewConnectionTransport,
    /// Selects one discovered shell for this one-shot local terminal launch.
    pub(in crate::workspace) local_shell_id: Option<String>,
    pub(in crate::workspace) name: String,
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: String,
    pub(in crate::workspace) username: String,
    pub(in crate::workspace) auth_tab: SshAuthTab,
    pub(in crate::workspace) gssapi_enabled: bool,
    pub(in crate::workspace) gssapi_server_identity: String,
    pub(in crate::workspace) gssapi_delegate_credentials: bool,
    pub(in crate::workspace) gssapi_credentials_available: Option<bool>,
    pub(in crate::workspace) gssapi_credentials_check_pending: bool,
    pub(in crate::workspace) password: String,
    pub(in crate::workspace) remote_desktop_session_options: RemoteDesktopSessionOptions,
    /// Identifies an existing RDP/VNC asset without overloading SSH edit state.
    pub(in crate::workspace) remote_desktop_profile_id: Option<String>,
    /// References saved SSH metadata only; credentials remain in the protected store.
    pub(in crate::workspace) remote_desktop_ssh_gateway_connection_id: Option<String>,
    /// Identifies an existing Mosh asset without creating an SSH node edit owner.
    pub(in crate::workspace) mosh_profile_id: Option<String>,
    /// Identifies an existing serial asset without changing a live serial session.
    pub(in crate::workspace) serial_profile_id: Option<String>,
    /// Identifies an existing Telnet asset without changing a live Telnet session.
    pub(in crate::workspace) telnet_profile_id: Option<String>,
    /// Identifies an independent SFTP asset without creating a NodeRouter node.
    pub(in crate::workspace) standalone_sftp_profile_id: Option<String>,
    /// Controls whether the dual-pane SFTP surface has one or two authenticated remotes.
    pub(in crate::workspace) standalone_sftp_transfer_mode: StandaloneSftpTransferMode,
    /// Owns the second endpoint only while editing a remote-to-remote profile.
    pub(in crate::workspace) standalone_sftp_secondary: StandaloneSftpSecondaryForm,
    pub(in crate::workspace) saved_password_keychain_id: Option<String>,
    pub(in crate::workspace) password_loaded: bool,
    pub(in crate::workspace) password_visible: bool,
    pub(in crate::workspace) key_path: String,
    pub(in crate::workspace) managed_key_id: String,
    pub(in crate::workspace) cert_path: String,
    pub(in crate::workspace) passphrase: String,
    pub(in crate::workspace) passphrase_visible: bool,
    pub(in crate::workspace) save_password: bool,
    pub(in crate::workspace) group: String,
    pub(in crate::workspace) notes: String,
    pub(in crate::workspace) sftp_initial_remote_path: String,
    pub(in crate::workspace) post_connect_command: String,
    pub(in crate::workspace) proxy_command_enabled: bool,
    pub(in crate::workspace) proxy_command: String,
    pub(in crate::workspace) proxy_command_keychain_id: Option<String>,
    pub(in crate::workspace) color: String,
    pub(in crate::workspace) icon_background_color: String,
    pub(in crate::workspace) icon: String,
    pub(in crate::workspace) icon_picker_expanded: bool,
    // None uses the default expanded state; user toggles remain transient UI state.
    pub(in crate::workspace) basic_section_expanded: Option<bool>,
    pub(in crate::workspace) authentication_section_expanded: Option<bool>,
    pub(in crate::workspace) route_section_expanded: Option<bool>,
    pub(in crate::workspace) standalone_sftp_secondary_route_section_expanded: Option<bool>,
    pub(in crate::workspace) ssh_options_section_expanded: Option<bool>,
    pub(in crate::workspace) terminal_section_expanded: Option<bool>,
    pub(in crate::workspace) appearance_section_expanded: Option<bool>,
    pub(in crate::workspace) remote_gateway_section_expanded: Option<bool>,
    pub(in crate::workspace) vnc_preferences_section_expanded: Option<bool>,
    pub(in crate::workspace) remote_features_section_expanded: Option<bool>,
    pub(in crate::workspace) serial_parameters_section_expanded: Option<bool>,
    pub(in crate::workspace) mosh_options_section_expanded: Option<bool>,
    pub(in crate::workspace) sftp_options_section_expanded: Option<bool>,
    pub(in crate::workspace) local_shell_section_expanded: Option<bool>,
    /// Keeps the uncommon transport group out of the primary list until requested.
    pub(in crate::workspace) advanced_connections_expanded: bool,
    pub(in crate::workspace) tags: Vec<String>,
    pub(in crate::workspace) proxy_hops: Vec<NewConnectionProxyHop>,
    pub(in crate::workspace) proxy_chain_expanded: bool,
    pub(in crate::workspace) jump_server_form: Option<NewConnectionProxyHop>,
    /// Identifies a hop temporarily moved into the editor so cancel can restore it in place.
    pub(in crate::workspace) jump_server_edit_index: Option<usize>,
    /// Selects which independently owned endpoint receives the pending jump host.
    pub(in crate::workspace) jump_server_target: ConnectionRouteTarget,
    pub(in crate::workspace) upstream_proxy_policy: NewConnectionUpstreamProxyPolicy,
    pub(in crate::workspace) upstream_proxy_protocol: SavedUpstreamProxyProtocol,
    pub(in crate::workspace) upstream_proxy_host: String,
    pub(in crate::workspace) upstream_proxy_port: String,
    pub(in crate::workspace) upstream_proxy_auth: NewConnectionUpstreamProxyAuth,
    pub(in crate::workspace) upstream_proxy_username: String,
    pub(in crate::workspace) upstream_proxy_password: String,
    pub(in crate::workspace) upstream_proxy_password_keychain_id: Option<String>,
    pub(in crate::workspace) upstream_proxy_remote_dns: bool,
    pub(in crate::workspace) upstream_proxy_no_proxy: String,
    pub(in crate::workspace) agent_forwarding: bool,
    pub(in crate::workspace) identity_agent: String,
    pub(in crate::workspace) agent_forwarding_socket: Option<String>,
    pub(in crate::workspace) legacy_ssh_compatibility: bool,
    pub(in crate::workspace) ssh_algorithms: SshAlgorithmPreferences,
    pub(in crate::workspace) ssh_algorithm_editor_open: bool,
    pub(in crate::workspace) ssh_algorithm_editor_category: oxideterm_ssh::SshAlgorithmCategory,
    pub(in crate::workspace) connect_timeout_seconds: u64,
    /// Preserves transient invalid input while the numeric value fails closed at zero.
    pub(in crate::workspace) connect_timeout_seconds_text: String,
    pub(in crate::workspace) dedicated_new_terminal_connection: bool,
    pub(in crate::workspace) ssh_channel_strategy: SshChannelStrategy,
    pub(in crate::workspace) x11_forwarding: ConnectionX11ForwardingOptions,
    pub(in crate::workspace) terminal: ConnectionTerminalOptions,
    pub(in crate::workspace) agent_available: Option<bool>,
    pub(in crate::workspace) save_connection: bool,
    pub(in crate::workspace) field_focused: bool,
    pub(in crate::workspace) focused_field: NewConnectionField,
    pub(in crate::workspace) selected_field: Option<NewConnectionField>,
    pub(in crate::workspace) error: Option<String>,
    // Success styling remains bound to the exact feedback message that produced it.
    pub(in crate::workspace) success_feedback_message: Option<String>,
    pub(in crate::workspace) pending: bool,
    pub(in crate::workspace) serial_ports: Vec<oxideterm_terminal::SerialPortInfo>,
    pub(in crate::workspace) serial_ports_loading: bool,
    pub(in crate::workspace) serial_port_path: String,
    pub(in crate::workspace) serial_baud_rate: String,
    pub(in crate::workspace) serial_data_bits: u8,
    pub(in crate::workspace) serial_stop_bits: u8,
    pub(in crate::workspace) serial_parity: oxideterm_terminal::SerialParity,
    pub(in crate::workspace) serial_flow_control: oxideterm_terminal::SerialFlowControl,
    pub(in crate::workspace) serial_profile_name: String,
    pub(in crate::workspace) telnet_profile_name: String,
    pub(in crate::workspace) mosh_server_executable: String,
    pub(in crate::workspace) mosh_udp_host: String,
    pub(in crate::workspace) mosh_udp_port: String,
    pub(in crate::workspace) mosh_locale: String,
    pub(in crate::workspace) mosh_ip_family: MoshIpFamily,
    pub(in crate::workspace) mosh_prediction: MoshPredictionMode,
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
            transport: NewConnectionTransport::Ssh,
            local_shell_id: None,
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
            remote_desktop_session_options: RemoteDesktopSessionOptions::default(),
            remote_desktop_profile_id: None,
            remote_desktop_ssh_gateway_connection_id: None,
            mosh_profile_id: None,
            serial_profile_id: None,
            telnet_profile_id: None,
            standalone_sftp_profile_id: None,
            standalone_sftp_transfer_mode: StandaloneSftpTransferMode::LocalRemote,
            standalone_sftp_secondary: StandaloneSftpSecondaryForm::default(),
            saved_password_keychain_id: None,
            password_loaded: true,
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
            ssh_algorithm_editor_category: oxideterm_ssh::SshAlgorithmCategory::Kex,
            connect_timeout_seconds: DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS,
            connect_timeout_seconds_text: DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS.to_string(),
            dedicated_new_terminal_connection: false,
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
    pub(in crate::workspace) fn feedback_is_success(&self) -> bool {
        self.error
            .as_ref()
            .is_some_and(|message| self.success_feedback_message.as_ref() == Some(message))
    }

    pub(in crate::workspace) fn set_standalone_sftp_transfer_mode(
        &mut self,
        mode: StandaloneSftpTransferMode,
    ) {
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
        // GPUI inputs require plain String drafts, so scrub them at owner teardown.
        self.zeroize_secret_drafts();
    }
}

pub(in crate::workspace) fn form_from_remote_desktop_profile(
    profile: &RemoteDesktopProfile,
    ungrouped_label: String,
) -> NewConnectionForm {
    // Editing carries only the keychain reference; the credential value is never loaded.
    let mut form = NewConnectionForm::default();
    form.transport = match profile.protocol {
        oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp => NewConnectionTransport::Rdp,
        oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc => NewConnectionTransport::Vnc,
    };
    form.name = profile.name.clone();
    form.host = profile.host.clone();
    form.port = profile.port.to_string();
    form.username = profile.username.clone().unwrap_or_default();
    form.remote_desktop_session_options = profile.session_options;
    form.remote_desktop_profile_id = Some(profile.id.clone());
    form.remote_desktop_ssh_gateway_connection_id = profile.ssh_gateway_connection_id.clone();
    form.saved_password_keychain_id = profile.credential_ref.clone();
    form.save_password = profile.credential_ref.is_some();
    form.group = profile.group.clone().unwrap_or(ungrouped_label);
    form.notes = profile.notes.clone().unwrap_or_default();
    form.icon = profile.icon.clone().unwrap_or_default();
    form.color = profile.color.clone().unwrap_or_default();
    form.icon_background_color = profile.icon_background_color.clone().unwrap_or_default();
    form.focused_field = NewConnectionField::Name;
    form
}

pub(in crate::workspace) fn form_from_mosh_profile(
    profile: &MoshProfile,
    ungrouped_label: String,
) -> NewConnectionForm {
    // Editing retains only protected-store references and never loads the secret value.
    let mut form = NewConnectionForm::default();
    form.transport = NewConnectionTransport::Mosh;
    form.mosh_profile_id = Some(profile.id.clone());
    form.name = profile.name.clone();
    form.host = profile.host.clone();
    form.port = profile.ssh_port.to_string();
    form.username = profile.username.clone();
    form.auth_tab = ssh_auth_tab_from_saved_auth(&profile.auth);
    form.saved_password_keychain_id = match profile.auth.conventional_fallback() {
        SavedAuth::Password { keychain_id, .. } => keychain_id.clone(),
        _ => None,
    };
    // Password-profile editors default to persisting a replacement credential.
    form.save_password = matches!(
        profile.auth.conventional_fallback(),
        SavedAuth::Password { .. }
    );
    form.password_loaded = true;
    form.key_path = profile.auth.key_path().unwrap_or_default().to_string();
    form.managed_key_id = profile
        .auth
        .managed_key_id()
        .unwrap_or_default()
        .to_string();
    form.cert_path = profile.auth.cert_path().unwrap_or_default().to_string();
    form.gssapi_enabled = profile.auth.gssapi_options().is_some();
    form.gssapi_server_identity = profile
        .auth
        .gssapi_options()
        .and_then(|(identity, _)| identity.map(ToOwned::to_owned))
        .unwrap_or_default();
    form.gssapi_delegate_credentials = profile
        .auth
        .gssapi_options()
        .is_some_and(|(_, delegate)| delegate);
    form.group = profile.group.clone().unwrap_or(ungrouped_label);
    form.notes = profile.notes.clone().unwrap_or_default();
    form.icon = profile.icon.clone().unwrap_or_default();
    form.color = profile.color.clone().unwrap_or_default();
    form.icon_background_color = profile.icon_background_color.clone().unwrap_or_default();
    form.identity_agent = profile.identity_agent.clone().unwrap_or_default();
    form.agent_available =
        oxideterm_ssh::ssh_agent_available(identity_agent_selector(&form.identity_agent));
    form.legacy_ssh_compatibility = profile.legacy_ssh_compatibility;
    form.ssh_algorithms = profile.ssh_algorithms.clone();
    form.proxy_hops = profile
        .proxy_chain
        .iter()
        .enumerate()
        .map(|(index, hop)| NewConnectionProxyHop::from_saved(index, hop))
        .collect();
    form.proxy_chain_expanded = !form.proxy_hops.is_empty();
    form.mosh_server_executable = profile.server_executable.clone();
    form.mosh_udp_host = profile.udp_host_override.clone().unwrap_or_default();
    form.mosh_udp_port = match profile.udp_port {
        MoshUdpPortSelection::Automatic => String::new(),
        MoshUdpPortSelection::Fixed { port } => port.to_string(),
        MoshUdpPortSelection::Range { start, end } => format!("{start}:{end}"),
    };
    form.mosh_locale = profile.locale.clone().unwrap_or_default();
    form.mosh_ip_family = profile.ip_family;
    form.mosh_prediction = profile.prediction;
    form.terminal = profile.terminal.clone();
    form.focused_field = NewConnectionField::Name;
    form
}

pub(in crate::workspace) fn terminal_serial_parity_from_profile(
    parity: &oxideterm_connections::SerialParity,
) -> oxideterm_terminal::SerialParity {
    match parity {
        oxideterm_connections::SerialParity::None => oxideterm_terminal::SerialParity::None,
        oxideterm_connections::SerialParity::Odd => oxideterm_terminal::SerialParity::Odd,
        oxideterm_connections::SerialParity::Even => oxideterm_terminal::SerialParity::Even,
    }
}

pub(in crate::workspace) fn terminal_serial_flow_from_profile(
    flow: &oxideterm_connections::SerialFlowControl,
) -> oxideterm_terminal::SerialFlowControl {
    match flow {
        oxideterm_connections::SerialFlowControl::None => {
            oxideterm_terminal::SerialFlowControl::None
        }
        oxideterm_connections::SerialFlowControl::Software => {
            oxideterm_terminal::SerialFlowControl::Software
        }
        oxideterm_connections::SerialFlowControl::Hardware => {
            oxideterm_terminal::SerialFlowControl::Hardware
        }
    }
}

pub(in crate::workspace) fn form_from_serial_profile(
    profile: &SerialProfile,
    ungrouped_label: String,
) -> NewConnectionForm {
    // The edit form owns only persisted settings; live serial sessions keep their current port.
    let mut form = NewConnectionForm::default();
    form.transport = NewConnectionTransport::Serial;
    form.serial_profile_id = Some(profile.id.clone());
    form.serial_profile_name = profile.name.clone();
    form.group = profile.group.clone().unwrap_or(ungrouped_label);
    form.notes = profile.notes.clone().unwrap_or_default();
    form.icon = profile.icon.clone().unwrap_or_default();
    form.color = profile.color.clone().unwrap_or_default();
    form.icon_background_color = profile.icon_background_color.clone().unwrap_or_default();
    form.serial_port_path = profile.port_path.clone();
    form.serial_baud_rate = profile.baud_rate.to_string();
    form.serial_data_bits = profile.data_bits;
    form.serial_stop_bits = profile.stop_bits;
    form.serial_parity = terminal_serial_parity_from_profile(&profile.parity);
    form.serial_flow_control = terminal_serial_flow_from_profile(&profile.flow_control);
    form.terminal = profile.terminal.clone();
    form.focused_field = NewConnectionField::SerialProfileName;
    form
}

pub(in crate::workspace) fn form_from_telnet_profile(
    profile: &TelnetProfile,
    ungrouped_label: String,
) -> NewConnectionForm {
    // The edit form owns only persisted settings; live Telnet sessions keep their current socket.
    let mut form = NewConnectionForm::default();
    form.transport = NewConnectionTransport::Telnet;
    form.telnet_profile_id = Some(profile.id.clone());
    form.telnet_profile_name = profile.name.clone();
    form.group = profile.group.clone().unwrap_or(ungrouped_label);
    form.notes = profile.notes.clone().unwrap_or_default();
    form.icon = profile.icon.clone().unwrap_or_default();
    form.color = profile.color.clone().unwrap_or_default();
    form.icon_background_color = profile.icon_background_color.clone().unwrap_or_default();
    form.host = profile.host.clone();
    form.port = profile.port.to_string();
    form.terminal = profile.terminal.clone();
    form.focused_field = NewConnectionField::TelnetProfileName;
    form
}

/// Returns the presentation-only visibility state for primary credential fields.
pub(in crate::workspace) fn connection_secret_field_visible(
    form: &NewConnectionForm,
    field: NewConnectionField,
) -> Option<bool> {
    match field {
        NewConnectionField::Password => Some(form.password_visible),
        NewConnectionField::Passphrase => Some(form.passphrase_visible),
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            Some(form.standalone_sftp_secondary.password_visible)
        }
        NewConnectionField::StandaloneSftpSecondaryPassphrase => {
            Some(form.standalone_sftp_secondary.passphrase_visible)
        }
        _ => None,
    }
}

/// Toggles visibility without changing or copying the underlying secret draft.
pub(in crate::workspace) fn toggle_connection_secret_field_visibility(
    form: &mut NewConnectionForm,
    field: NewConnectionField,
) -> bool {
    match field {
        NewConnectionField::Password => {
            form.password_visible = !form.password_visible;
            true
        }
        NewConnectionField::Passphrase => {
            form.passphrase_visible = !form.passphrase_visible;
            true
        }
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            form.standalone_sftp_secondary.password_visible =
                !form.standalone_sftp_secondary.password_visible;
            true
        }
        NewConnectionField::StandaloneSftpSecondaryPassphrase => {
            form.standalone_sftp_secondary.passphrase_visible =
                !form.standalone_sftp_secondary.passphrase_visible;
            true
        }
        _ => false,
    }
}

/// Returns the configured endpoint selector while preserving an empty draft as auto-detect.
pub(in crate::workspace) fn identity_agent_selector(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

/// Converts a form endpoint into the persisted optional representation.
pub(in crate::workspace) fn identity_agent_from_form(value: &str) -> Option<String> {
    identity_agent_selector(value).map(str::to_string)
}

/// Refreshes the main connection's status only after its endpoint draft changes.
pub(in crate::workspace) fn refresh_identity_agent_availability(form: &mut NewConnectionForm) {
    form.agent_available =
        oxideterm_ssh::ssh_agent_available(identity_agent_selector(&form.identity_agent));
}

fn refresh_focused_identity_agent_availability(form: &mut NewConnectionForm) {
    if form.focused_field == NewConnectionField::IdentityAgent {
        refresh_identity_agent_availability(form);
    } else if form.focused_field == NewConnectionField::StandaloneSftpSecondaryIdentityAgent {
        form.standalone_sftp_secondary.agent_available = oxideterm_ssh::ssh_agent_available(
            identity_agent_selector(&form.standalone_sftp_secondary.identity_agent),
        );
    }
}

pub(in crate::workspace) fn apply_transport_default_port(
    form: &mut NewConnectionForm,
    previous_transport: NewConnectionTransport,
    next_transport: NewConnectionTransport,
) {
    if let Some(replacement) =
        transport_port_replacement(&form.port, previous_transport, next_transport)
    {
        form.port = replacement.to_string();
    }
}

pub(in crate::workspace) fn apply_transport_default_username(
    form: &mut NewConnectionForm,
    previous_transport: NewConnectionTransport,
    next_transport: NewConnectionTransport,
) {
    match transport_username_transition(&form.username, previous_transport, next_transport) {
        Some(TransportUsernameTransition::Set(username)) => form.username = username.to_string(),
        Some(TransportUsernameTransition::Clear) => form.username.clear(),
        None => {}
    }
}

pub(in crate::workspace) fn next_connection_field(
    field: NewConnectionField,
    auth_tab: SshAuthTab,
    gssapi_enabled: bool,
    transport: NewConnectionTransport,
    upstream_proxy_policy: NewConnectionUpstreamProxyPolicy,
    upstream_proxy_auth: NewConnectionUpstreamProxyAuth,
    forward: bool,
) -> NewConnectionField {
    if transport == NewConnectionTransport::LocalTerminal {
        // A one-shot local terminal has no editable or persistable form fields.
        return field;
    }
    if transport == NewConnectionTransport::WslGraphics {
        return NewConnectionField::Name;
    }
    if transport == NewConnectionTransport::Serial {
        let fields = [
            NewConnectionField::SerialProfileName,
            NewConnectionField::SerialPortPath,
            NewConnectionField::SerialBaudRate,
        ];
        let index = fields
            .iter()
            .position(|candidate| *candidate == field)
            .unwrap_or(0);
        let next = if forward {
            (index + 1) % fields.len()
        } else if index == 0 {
            fields.len() - 1
        } else {
            index - 1
        };
        return fields[next];
    }
    if transport == NewConnectionTransport::Telnet {
        let fields = [
            NewConnectionField::TelnetProfileName,
            NewConnectionField::Host,
            NewConnectionField::Port,
        ];
        let index = fields
            .iter()
            .position(|candidate| *candidate == field)
            .unwrap_or(0);
        let next = if forward {
            (index + 1) % fields.len()
        } else if index == 0 {
            fields.len() - 1
        } else {
            index - 1
        };
        return fields[next];
    }
    if matches!(
        transport,
        NewConnectionTransport::Rdp | NewConnectionTransport::Vnc
    ) {
        let fields = [
            NewConnectionField::Name,
            NewConnectionField::Group,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Password,
        ];
        let index = fields
            .iter()
            .position(|candidate| *candidate == field)
            .unwrap_or(0);
        let next = if forward {
            (index + 1) % fields.len()
        } else if index == 0 {
            fields.len() - 1
        } else {
            index - 1
        };
        return fields[next];
    }

    let mut fields: Vec<NewConnectionField> = match auth_tab {
        SshAuthTab::Password => vec![
            NewConnectionField::Name,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Password,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::DefaultKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Passphrase,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::SshKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::KeyPath,
            NewConnectionField::Passphrase,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::ManagedKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::ManagedKeyId,
            NewConnectionField::Passphrase,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::Certificate => vec![
            NewConnectionField::Name,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::KeyPath,
            NewConnectionField::CertPath,
            NewConnectionField::Passphrase,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::Agent => vec![
            NewConnectionField::Name,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::IdentityAgent,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::TwoFactor => vec![
            NewConnectionField::Name,
            NewConnectionField::Group,
            NewConnectionField::Notes,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::PostConnectCommand,
        ],
    };
    if gssapi_enabled {
        fields.insert(6, NewConnectionField::GssapiServerIdentity);
    }
    if transport == NewConnectionTransport::Mosh {
        fields.retain(|field| {
            !matches!(
                field,
                NewConnectionField::Notes | NewConnectionField::PostConnectCommand
            )
        });
        fields.extend([
            NewConnectionField::MoshServerExecutable,
            NewConnectionField::MoshUdpHost,
            NewConnectionField::MoshUdpPort,
            NewConnectionField::MoshLocale,
        ]);
    } else if transport == NewConnectionTransport::StandaloneSftp {
        // Independent SFTP uses SSH authentication and routing without terminal-only commands.
        fields.retain(|field| *field != NewConnectionField::PostConnectCommand);
        fields.push(NewConnectionField::InitialRemotePath);
    }
    fields.push(NewConnectionField::ConnectTimeoutSeconds);
    if upstream_proxy_policy == NewConnectionUpstreamProxyPolicy::Custom
        && matches!(
            transport,
            NewConnectionTransport::Ssh | NewConnectionTransport::StandaloneSftp
        )
    {
        fields.extend([
            NewConnectionField::UpstreamProxyHost,
            NewConnectionField::UpstreamProxyPort,
            NewConnectionField::UpstreamProxyNoProxy,
        ]);
        if upstream_proxy_auth == NewConnectionUpstreamProxyAuth::Password {
            fields.extend([
                NewConnectionField::UpstreamProxyUsername,
                NewConnectionField::UpstreamProxyPassword,
            ]);
        }
    }
    let index = fields
        .iter()
        .position(|candidate| *candidate == field)
        .unwrap_or(0);
    let next = if forward {
        (index + 1) % fields.len()
    } else if index == 0 {
        fields.len() - 1
    } else {
        index - 1
    };
    fields[next]
}

pub(in crate::workspace) fn next_jump_connection_field(
    field: NewConnectionField,
    auth_tab: SshAuthTab,
    gssapi_enabled: bool,
    forward: bool,
) -> NewConnectionField {
    let mut fields: Vec<NewConnectionField> = match auth_tab {
        SshAuthTab::Password => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpPassword,
        ],
        SshAuthTab::DefaultKey => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
        ],
        SshAuthTab::Agent => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpIdentityAgent,
        ],
        SshAuthTab::SshKey => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpKeyPath,
            NewConnectionField::JumpPassphrase,
        ],
        SshAuthTab::ManagedKey => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpManagedKeyId,
            NewConnectionField::JumpPassphrase,
        ],
        SshAuthTab::Certificate => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpKeyPath,
            NewConnectionField::JumpCertPath,
            NewConnectionField::JumpPassphrase,
        ],
        SshAuthTab::TwoFactor => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
        ],
    };
    if gssapi_enabled {
        fields.insert(3, NewConnectionField::JumpGssapiServerIdentity);
    }
    let index = fields
        .iter()
        .position(|candidate| *candidate == field)
        .unwrap_or(0);
    let next = if forward {
        (index + 1) % fields.len()
    } else if index == 0 {
        fields.len() - 1
    } else {
        index - 1
    };
    fields[next]
}

pub(in crate::workspace) fn next_standalone_sftp_field(
    form: &NewConnectionForm,
    forward: bool,
) -> NewConnectionField {
    fn append_auth_fields(
        fields: &mut Vec<NewConnectionField>,
        auth_tab: SshAuthTab,
        gssapi_enabled: bool,
        secondary: bool,
    ) {
        if gssapi_enabled {
            fields.push(if secondary {
                NewConnectionField::StandaloneSftpSecondaryGssapiServerIdentity
            } else {
                NewConnectionField::GssapiServerIdentity
            });
        }
        match (auth_tab, secondary) {
            (SshAuthTab::Password, false) => fields.push(NewConnectionField::Password),
            (SshAuthTab::Password, true) => {
                fields.push(NewConnectionField::StandaloneSftpSecondaryPassword)
            }
            (SshAuthTab::DefaultKey, false) => fields.push(NewConnectionField::Passphrase),
            (SshAuthTab::DefaultKey, true) => {
                fields.push(NewConnectionField::StandaloneSftpSecondaryPassphrase)
            }
            (SshAuthTab::SshKey, false) => {
                fields.extend([NewConnectionField::KeyPath, NewConnectionField::Passphrase])
            }
            (SshAuthTab::SshKey, true) => fields.extend([
                NewConnectionField::StandaloneSftpSecondaryKeyPath,
                NewConnectionField::StandaloneSftpSecondaryPassphrase,
            ]),
            (SshAuthTab::ManagedKey, false) => fields.extend([
                NewConnectionField::ManagedKeyId,
                NewConnectionField::Passphrase,
            ]),
            (SshAuthTab::ManagedKey, true) => fields.extend([
                NewConnectionField::StandaloneSftpSecondaryManagedKeyId,
                NewConnectionField::StandaloneSftpSecondaryPassphrase,
            ]),
            (SshAuthTab::Certificate, false) => fields.extend([
                NewConnectionField::KeyPath,
                NewConnectionField::CertPath,
                NewConnectionField::Passphrase,
            ]),
            (SshAuthTab::Certificate, true) => fields.extend([
                NewConnectionField::StandaloneSftpSecondaryKeyPath,
                NewConnectionField::StandaloneSftpSecondaryCertPath,
                NewConnectionField::StandaloneSftpSecondaryPassphrase,
            ]),
            (SshAuthTab::Agent, false) => fields.push(NewConnectionField::IdentityAgent),
            (SshAuthTab::Agent, true) => {
                fields.push(NewConnectionField::StandaloneSftpSecondaryIdentityAgent)
            }
            (SshAuthTab::TwoFactor, _) => {}
        }
    }

    let mut fields = vec![
        NewConnectionField::Name,
        NewConnectionField::Group,
        NewConnectionField::Notes,
        NewConnectionField::Host,
        NewConnectionField::Port,
        NewConnectionField::Username,
    ];
    append_auth_fields(&mut fields, form.auth_tab, form.gssapi_enabled, false);
    fields.push(NewConnectionField::InitialRemotePath);
    fields.push(NewConnectionField::ConnectTimeoutSeconds);
    if form.standalone_sftp_transfer_mode == StandaloneSftpTransferMode::RemoteRemote {
        fields.extend([
            NewConnectionField::StandaloneSftpSecondaryHost,
            NewConnectionField::StandaloneSftpSecondaryPort,
            NewConnectionField::StandaloneSftpSecondaryUsername,
        ]);
        append_auth_fields(
            &mut fields,
            form.standalone_sftp_secondary.auth_tab,
            form.standalone_sftp_secondary.gssapi_enabled,
            true,
        );
        fields.push(NewConnectionField::StandaloneSftpSecondaryInitialRemotePath);
        fields.push(NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds);
    }
    let index = fields
        .iter()
        .position(|candidate| *candidate == form.focused_field)
        .unwrap_or(0);
    let next = if forward {
        (index + 1) % fields.len()
    } else if index == 0 {
        fields.len() - 1
    } else {
        index - 1
    };
    fields[next]
}

pub(in crate::workspace) fn current_connection_field_mut(
    form: &mut NewConnectionForm,
) -> &mut String {
    match form.focused_field {
        NewConnectionField::Name => &mut form.name,
        NewConnectionField::Host => &mut form.host,
        NewConnectionField::Port => &mut form.port,
        NewConnectionField::Username => &mut form.username,
        NewConnectionField::Password => &mut form.password,
        NewConnectionField::KeyPath => &mut form.key_path,
        NewConnectionField::ManagedKeyId => &mut form.managed_key_id,
        NewConnectionField::CertPath => &mut form.cert_path,
        NewConnectionField::Passphrase => &mut form.passphrase,
        NewConnectionField::GssapiServerIdentity => &mut form.gssapi_server_identity,
        NewConnectionField::IdentityAgent => &mut form.identity_agent,
        NewConnectionField::Group => &mut form.group,
        NewConnectionField::Notes => &mut form.notes,
        NewConnectionField::InitialRemotePath => &mut form.sftp_initial_remote_path,
        NewConnectionField::ConnectTimeoutSeconds => &mut form.connect_timeout_seconds_text,
        NewConnectionField::StandaloneSftpSecondaryHost => &mut form.standalone_sftp_secondary.host,
        NewConnectionField::StandaloneSftpSecondaryPort => &mut form.standalone_sftp_secondary.port,
        NewConnectionField::StandaloneSftpSecondaryUsername => {
            &mut form.standalone_sftp_secondary.username
        }
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            &mut form.standalone_sftp_secondary.password
        }
        NewConnectionField::StandaloneSftpSecondaryKeyPath => {
            &mut form.standalone_sftp_secondary.key_path
        }
        NewConnectionField::StandaloneSftpSecondaryManagedKeyId => {
            &mut form.standalone_sftp_secondary.managed_key_id
        }
        NewConnectionField::StandaloneSftpSecondaryCertPath => {
            &mut form.standalone_sftp_secondary.cert_path
        }
        NewConnectionField::StandaloneSftpSecondaryPassphrase => {
            &mut form.standalone_sftp_secondary.passphrase
        }
        NewConnectionField::StandaloneSftpSecondaryGssapiServerIdentity => {
            &mut form.standalone_sftp_secondary.gssapi_server_identity
        }
        NewConnectionField::StandaloneSftpSecondaryIdentityAgent => {
            &mut form.standalone_sftp_secondary.identity_agent
        }
        NewConnectionField::StandaloneSftpSecondaryInitialRemotePath => {
            &mut form.standalone_sftp_secondary.initial_remote_path
        }
        NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds => {
            &mut form.standalone_sftp_secondary.connect_timeout_seconds_text
        }
        NewConnectionField::StandaloneSftpSecondaryProxyCommand => {
            &mut form.standalone_sftp_secondary.proxy_command
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyHost => {
            &mut form.standalone_sftp_secondary.upstream_proxy_host
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPort => {
            &mut form.standalone_sftp_secondary.upstream_proxy_port
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyNoProxy => {
            &mut form.standalone_sftp_secondary.upstream_proxy_no_proxy
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyUsername => {
            &mut form.standalone_sftp_secondary.upstream_proxy_username
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPassword => {
            &mut form.standalone_sftp_secondary.upstream_proxy_password
        }
        NewConnectionField::PostConnectCommand => &mut form.post_connect_command,
        NewConnectionField::ProxyCommand => &mut form.proxy_command,
        NewConnectionField::UpstreamProxyHost => &mut form.upstream_proxy_host,
        NewConnectionField::UpstreamProxyPort => &mut form.upstream_proxy_port,
        NewConnectionField::UpstreamProxyNoProxy => &mut form.upstream_proxy_no_proxy,
        NewConnectionField::UpstreamProxyUsername => &mut form.upstream_proxy_username,
        NewConnectionField::UpstreamProxyPassword => &mut form.upstream_proxy_password,
        NewConnectionField::Color => &mut form.color,
        NewConnectionField::IconBackgroundColor => &mut form.icon_background_color,
        NewConnectionField::JumpHost => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump host field without jump form")
                .host
        }
        NewConnectionField::JumpPort => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump port field without jump form")
                .port
        }
        NewConnectionField::JumpUsername => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump username field without jump form")
                .username
        }
        NewConnectionField::JumpPassword => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump password field without jump form")
                .password
        }
        NewConnectionField::JumpKeyPath => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump key path field without jump form")
                .key_path
        }
        NewConnectionField::JumpManagedKeyId => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump managed key field without jump form")
                .managed_key_id
        }
        NewConnectionField::JumpCertPath => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump cert path field without jump form")
                .cert_path
        }
        NewConnectionField::JumpPassphrase => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump passphrase field without jump form")
                .passphrase
        }
        NewConnectionField::JumpGssapiServerIdentity => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump Kerberos server field without jump form")
                .gssapi_server_identity
        }
        NewConnectionField::JumpIdentityAgent => {
            &mut form
                .jump_server_form
                .as_mut()
                .expect("jump identity agent field without jump form")
                .identity_agent
        }
        NewConnectionField::SerialPortPath => &mut form.serial_port_path,
        NewConnectionField::SerialBaudRate => &mut form.serial_baud_rate,
        NewConnectionField::SerialProfileName => &mut form.serial_profile_name,
        NewConnectionField::TelnetProfileName => &mut form.telnet_profile_name,
        NewConnectionField::MoshServerExecutable => &mut form.mosh_server_executable,
        NewConnectionField::MoshUdpHost => &mut form.mosh_udp_host,
        NewConnectionField::MoshUdpPort => &mut form.mosh_udp_port,
        NewConnectionField::MoshLocale => &mut form.mosh_locale,
    }
}

pub(in crate::workspace) fn current_connection_field(form: &NewConnectionForm) -> &str {
    match form.focused_field {
        NewConnectionField::Name => &form.name,
        NewConnectionField::Host => &form.host,
        NewConnectionField::Port => &form.port,
        NewConnectionField::Username => &form.username,
        NewConnectionField::Password => &form.password,
        NewConnectionField::KeyPath => &form.key_path,
        NewConnectionField::ManagedKeyId => &form.managed_key_id,
        NewConnectionField::CertPath => &form.cert_path,
        NewConnectionField::Passphrase => &form.passphrase,
        NewConnectionField::GssapiServerIdentity => &form.gssapi_server_identity,
        NewConnectionField::IdentityAgent => &form.identity_agent,
        NewConnectionField::Group => &form.group,
        NewConnectionField::Notes => &form.notes,
        NewConnectionField::InitialRemotePath => &form.sftp_initial_remote_path,
        NewConnectionField::ConnectTimeoutSeconds => &form.connect_timeout_seconds_text,
        NewConnectionField::StandaloneSftpSecondaryHost => &form.standalone_sftp_secondary.host,
        NewConnectionField::StandaloneSftpSecondaryPort => &form.standalone_sftp_secondary.port,
        NewConnectionField::StandaloneSftpSecondaryUsername => {
            &form.standalone_sftp_secondary.username
        }
        NewConnectionField::StandaloneSftpSecondaryPassword => {
            &form.standalone_sftp_secondary.password
        }
        NewConnectionField::StandaloneSftpSecondaryKeyPath => {
            &form.standalone_sftp_secondary.key_path
        }
        NewConnectionField::StandaloneSftpSecondaryManagedKeyId => {
            &form.standalone_sftp_secondary.managed_key_id
        }
        NewConnectionField::StandaloneSftpSecondaryCertPath => {
            &form.standalone_sftp_secondary.cert_path
        }
        NewConnectionField::StandaloneSftpSecondaryPassphrase => {
            &form.standalone_sftp_secondary.passphrase
        }
        NewConnectionField::StandaloneSftpSecondaryGssapiServerIdentity => {
            &form.standalone_sftp_secondary.gssapi_server_identity
        }
        NewConnectionField::StandaloneSftpSecondaryIdentityAgent => {
            &form.standalone_sftp_secondary.identity_agent
        }
        NewConnectionField::StandaloneSftpSecondaryInitialRemotePath => {
            &form.standalone_sftp_secondary.initial_remote_path
        }
        NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds => {
            &form.standalone_sftp_secondary.connect_timeout_seconds_text
        }
        NewConnectionField::StandaloneSftpSecondaryProxyCommand => {
            &form.standalone_sftp_secondary.proxy_command
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyHost => {
            &form.standalone_sftp_secondary.upstream_proxy_host
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPort => {
            &form.standalone_sftp_secondary.upstream_proxy_port
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyNoProxy => {
            &form.standalone_sftp_secondary.upstream_proxy_no_proxy
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyUsername => {
            &form.standalone_sftp_secondary.upstream_proxy_username
        }
        NewConnectionField::StandaloneSftpSecondaryUpstreamProxyPassword => {
            &form.standalone_sftp_secondary.upstream_proxy_password
        }
        NewConnectionField::PostConnectCommand => &form.post_connect_command,
        NewConnectionField::ProxyCommand => &form.proxy_command,
        NewConnectionField::UpstreamProxyHost => &form.upstream_proxy_host,
        NewConnectionField::UpstreamProxyPort => &form.upstream_proxy_port,
        NewConnectionField::UpstreamProxyNoProxy => &form.upstream_proxy_no_proxy,
        NewConnectionField::UpstreamProxyUsername => &form.upstream_proxy_username,
        NewConnectionField::UpstreamProxyPassword => &form.upstream_proxy_password,
        NewConnectionField::Color => &form.color,
        NewConnectionField::IconBackgroundColor => &form.icon_background_color,
        NewConnectionField::JumpHost => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump host field without jump form")
                .host
        }
        NewConnectionField::JumpPort => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump port field without jump form")
                .port
        }
        NewConnectionField::JumpUsername => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump username field without jump form")
                .username
        }
        NewConnectionField::JumpPassword => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump password field without jump form")
                .password
        }
        NewConnectionField::JumpKeyPath => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump key path field without jump form")
                .key_path
        }
        NewConnectionField::JumpManagedKeyId => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump managed key field without jump form")
                .managed_key_id
        }
        NewConnectionField::JumpCertPath => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump cert path field without jump form")
                .cert_path
        }
        NewConnectionField::JumpPassphrase => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump passphrase field without jump form")
                .passphrase
        }
        NewConnectionField::JumpGssapiServerIdentity => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump Kerberos server field without jump form")
                .gssapi_server_identity
        }
        NewConnectionField::JumpIdentityAgent => {
            &form
                .jump_server_form
                .as_ref()
                .expect("jump identity agent field without jump form")
                .identity_agent
        }
        NewConnectionField::SerialPortPath => &form.serial_port_path,
        NewConnectionField::SerialBaudRate => &form.serial_baud_rate,
        NewConnectionField::SerialProfileName => &form.serial_profile_name,
        NewConnectionField::TelnetProfileName => &form.telnet_profile_name,
        NewConnectionField::MoshServerExecutable => &form.mosh_server_executable,
        NewConnectionField::MoshUdpHost => &form.mosh_udp_host,
        NewConnectionField::MoshUdpPort => &form.mosh_udp_port,
        NewConnectionField::MoshLocale => &form.mosh_locale,
    }
}

pub(in crate::workspace) fn select_current_connection_field(form: &mut NewConnectionForm) {
    if current_connection_field(form).is_empty() {
        form.selected_field = None;
    } else {
        form.selected_field = Some(form.focused_field);
    }
}

pub(in crate::workspace) fn clear_connection_selection(form: &mut NewConnectionForm) {
    form.selected_field = None;
}

pub(in crate::workspace) fn connection_field_is_selected(
    form: &NewConnectionForm,
    field: NewConnectionField,
) -> bool {
    form.selected_field == Some(field)
}

pub(in crate::workspace) fn insert_text_into_current_connection_field(
    form: &mut NewConnectionForm,
    text: &str,
) {
    let focused_field = form.focused_field;
    let replacing_selection = form.selected_field == Some(form.focused_field);
    if replacing_selection {
        current_connection_field_mut(form).clear();
    }
    current_connection_field_mut(form).push_str(text);
    form.selected_field = None;
    refresh_connection_timeout_seconds(form, focused_field);
    refresh_focused_identity_agent_availability(form);
}

pub(in crate::workspace) fn backspace_current_connection_field(
    form: &mut NewConnectionForm,
) -> bool {
    let focused_field = form.focused_field;
    let selection_was_visible = form.selected_field.is_some();
    if form.selected_field == Some(form.focused_field) {
        // Clearing a selected field also clears visible selection state. Track
        // text separately so empty selected fields still report a UI change.
        let field = current_connection_field_mut(form);
        let text_changed = !field.is_empty();
        field.clear();
        form.selected_field = None;
        if text_changed {
            refresh_connection_timeout_seconds(form, focused_field);
            refresh_focused_identity_agent_availability(form);
        }
        text_changed || selection_was_visible
    } else {
        let text_changed = current_connection_field_mut(form).pop().is_some();
        form.selected_field = None;
        if text_changed {
            refresh_connection_timeout_seconds(form, focused_field);
            refresh_focused_identity_agent_availability(form);
        }
        text_changed || selection_was_visible
    }
}

pub(in crate::workspace) fn clear_current_connection_field(form: &mut NewConnectionForm) {
    let focused_field = form.focused_field;
    current_connection_field_mut(form).clear();
    form.selected_field = None;
    refresh_connection_timeout_seconds(form, focused_field);
    refresh_focused_identity_agent_availability(form);
}

pub(in crate::workspace) fn refresh_connection_timeout_seconds(
    form: &mut NewConnectionForm,
    field: NewConnectionField,
) {
    // Invalid drafts map to zero so keyboard submission cannot reuse a stale valid timeout.
    let parsed = match field {
        NewConnectionField::ConnectTimeoutSeconds => form
            .connect_timeout_seconds_text
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|seconds| *seconds > 0),
        NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds => form
            .standalone_sftp_secondary
            .connect_timeout_seconds_text
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|seconds| *seconds > 0),
        _ => return,
    }
    .unwrap_or(0);
    match field {
        NewConnectionField::ConnectTimeoutSeconds => form.connect_timeout_seconds = parsed,
        NewConnectionField::StandaloneSftpSecondaryConnectTimeoutSeconds => {
            form.standalone_sftp_secondary.connect_timeout_seconds = parsed;
        }
        _ => {}
    }
}

pub(in crate::workspace) fn connection_timeout_drafts_valid(form: &NewConnectionForm) -> bool {
    let primary_valid = form
        .connect_timeout_seconds_text
        .trim()
        .parse::<u64>()
        .is_ok_and(|seconds| seconds > 0);
    let secondary_valid = form
        .standalone_sftp_secondary
        .connect_timeout_seconds_text
        .trim()
        .parse::<u64>()
        .is_ok_and(|seconds| seconds > 0);
    primary_valid
        && (form.standalone_sftp_transfer_mode != StandaloneSftpTransferMode::RemoteRemote
            || secondary_valid)
}

pub(in crate::workspace) fn text_from_keystroke(keystroke: &gpui::Keystroke) -> Option<&str> {
    if keystroke.modifiers.platform || keystroke.modifiers.control {
        return None;
    }
    let text = keystroke.key_char.as_deref()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use gpui::{Keystroke, Modifiers};
    use oxideterm_connections::{
        AuthType, ConnectionInfo, MoshIpFamily, MoshPredictionMode, MoshProfile,
        MoshUdpPortSelection, RemoteDesktopProfile, SavedAuth, SavedProxyHop,
        SavedUpstreamProxyPolicy, SerialFlowControl, SerialParity, SerialProfile, TelnetProfile,
    };
    use oxideterm_remote_desktop::{
        RemoteDesktopAudioOptions, RemoteDesktopClipboardOptions, RemoteDesktopDisplayOptions,
        RemoteDesktopProtocol, RemoteDesktopRdpNetworkProfile, RemoteDesktopRdpOptions,
    };

    use super::{
        NewConnectionField, NewConnectionForm, NewConnectionProxyHop, NewConnectionTransport,
        RemoteDesktopSessionOptions, RemoteDesktopVncCompression, RemoteDesktopVncImageQuality,
        RemoteDesktopVncOptions, RemoteDesktopVncSecurityPolicy, RemoteDesktopVncSessionMode,
        SshAuthFamily, SshAuthTab, SshKeyAuthSource, StandaloneSftpTransferMode,
        auth_family_from_tab, backspace_current_connection_field, connection_secret_field_visible,
        form_from_mosh_profile, form_from_remote_desktop_profile, form_from_serial_profile,
        form_from_telnet_profile, insert_text_into_current_connection_field, key_source_from_tab,
        select_current_connection_field, text_from_keystroke,
        toggle_connection_secret_field_visibility,
    };

    fn keystroke(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
        }
    }

    #[test]
    fn primary_secret_visibility_is_hidden_and_toggles_independently() {
        let mut form = NewConnectionForm::default();

        assert_eq!(
            connection_secret_field_visible(&form, NewConnectionField::Password),
            Some(false)
        );
        assert_eq!(
            connection_secret_field_visible(&form, NewConnectionField::Passphrase),
            Some(false)
        );
        assert!(toggle_connection_secret_field_visibility(
            &mut form,
            NewConnectionField::Password
        ));
        assert_eq!(
            connection_secret_field_visible(&form, NewConnectionField::Password),
            Some(true)
        );
        assert_eq!(
            connection_secret_field_visible(&form, NewConnectionField::Passphrase),
            Some(false)
        );
        assert!(toggle_connection_secret_field_visibility(
            &mut form,
            NewConnectionField::Passphrase
        ));
        assert_eq!(
            connection_secret_field_visible(&form, NewConnectionField::Passphrase),
            Some(true)
        );
        assert!(!toggle_connection_secret_field_visibility(
            &mut form,
            NewConnectionField::Host
        ));
    }

    #[test]
    fn visible_secret_drafts_remain_redacted_from_debug_output() {
        let mut form = NewConnectionForm::default();
        form.password = "password-value".to_string();
        form.password_visible = true;
        form.passphrase = "passphrase-value".to_string();
        form.passphrase_visible = true;

        let debug_output = format!("{form:?}");

        assert!(!debug_output.contains("password-value"));
        assert!(!debug_output.contains("passphrase-value"));
        assert!(debug_output.contains("[redacted secret]"));
    }

    #[test]
    fn connection_secret_drafts_are_zeroized() {
        let mut form = NewConnectionForm::default();
        form.password = "password-value".to_string();
        form.passphrase = "passphrase-value".to_string();
        form.upstream_proxy_password = "proxy-password-value".to_string();
        form.standalone_sftp_transfer_mode = StandaloneSftpTransferMode::RemoteRemote;
        form.standalone_sftp_secondary.password = "secondary-password-value".to_string();
        form.standalone_sftp_secondary.passphrase = "secondary-passphrase-value".to_string();
        form.set_standalone_sftp_transfer_mode(StandaloneSftpTransferMode::LocalRemote);
        form.zeroize_secret_drafts();

        let mut proxy_hop = NewConnectionProxyHop::new();
        proxy_hop.password = "hop-password-value".to_string();
        proxy_hop.passphrase = "hop-passphrase-value".to_string();
        proxy_hop.zeroize_secret_drafts();

        assert!(form.password.is_empty());
        assert!(form.passphrase.is_empty());
        assert!(form.upstream_proxy_password.is_empty());
        assert!(form.standalone_sftp_secondary.password.is_empty());
        assert!(form.standalone_sftp_secondary.passphrase.is_empty());
        assert!(proxy_hop.password.is_empty());
        assert!(proxy_hop.passphrase.is_empty());
    }

    #[test]
    fn remote_desktop_edit_form_restores_options_without_loading_secret() {
        let session_options = RemoteDesktopSessionOptions {
            clipboard: RemoteDesktopClipboardOptions {
                text: false,
                images: false,
                files: true,
            },
            audio: RemoteDesktopAudioOptions {
                playback: false,
                capture: true,
            },
            display: RemoteDesktopDisplayOptions {
                use_all_monitors: true,
            },
            rdp: RemoteDesktopRdpOptions {
                network_profile: RemoteDesktopRdpNetworkProfile::Broadband,
                disable_graphics_pipeline: true,
            },
            vnc: RemoteDesktopVncOptions {
                security_policy: RemoteDesktopVncSecurityPolicy::AllowLegacy,
                session_mode: RemoteDesktopVncSessionMode::Exclusive,
                image_quality: RemoteDesktopVncImageQuality::BestQuality,
                compression: RemoteDesktopVncCompression::High,
            },
        };
        let now = Utc::now();
        let profile = RemoteDesktopProfile {
            id: "remote-1".to_string(),
            name: "Lab desktop".to_string(),
            group: Some("Lab".to_string()),
            notes: Some("Shared display".to_string()),
            icon: Some("cloud".to_string()),
            color: Some("#7dd3fc".to_string()),
            icon_background_color: Some("#082f49".to_string()),
            protocol: RemoteDesktopProtocol::Rdp,
            host: "rdp.example.com".to_string(),
            port: 3389,
            username: Some("operator".to_string()),
            domain: Some("EXAMPLE".to_string()),
            credential_ref: Some("remote-desktop:remote-1".to_string()),
            ssh_gateway_connection_id: Some("gateway-1".to_string()),
            read_only: true,
            session_options,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        };

        let form = form_from_remote_desktop_profile(&profile, "Ungrouped".to_string());

        assert_eq!(form.remote_desktop_profile_id.as_deref(), Some("remote-1"));
        assert_eq!(form.transport, NewConnectionTransport::Rdp);
        assert_eq!(form.name, "Lab desktop");
        assert_eq!(form.host, "rdp.example.com");
        assert_eq!(form.port, "3389");
        assert_eq!(form.username, "operator");
        assert_eq!(form.group, "Lab");
        assert_eq!(form.notes, "Shared display");
        assert_eq!(form.icon, "cloud");
        assert_eq!(form.color, "#7dd3fc");
        assert_eq!(form.icon_background_color, "#082f49");
        assert_eq!(
            form.remote_desktop_ssh_gateway_connection_id.as_deref(),
            Some("gateway-1")
        );
        assert_eq!(form.remote_desktop_session_options, session_options);
        assert_eq!(
            form.saved_password_keychain_id.as_deref(),
            Some("remote-desktop:remote-1")
        );
        assert!(form.save_password);
        assert!(form.password.is_empty());
    }

    #[test]
    fn mosh_profile_form_preserves_edit_metadata_without_loading_credentials() {
        let mut profile = MoshProfile::new(
            "Roaming shell",
            "mosh.example.com",
            2222,
            "operator",
            SavedAuth::Password {
                keychain_id: Some("mosh-password-owner".to_string()),
                plaintext_password: None,
            },
        );
        profile.id = "mosh-1".to_string();
        profile.group = Some("Mobile".to_string());
        profile.notes = Some("Intermittent link".to_string());
        profile.icon = Some("wifi".to_string());
        profile.color = Some("#93c5fd".to_string());
        profile.server_executable = "/opt/mosh/bin/mosh-server".to_string();
        profile.udp_host_override = Some("udp.example.com".to_string());
        profile.udp_port = MoshUdpPortSelection::Range {
            start: 60_000,
            end: 60_010,
        };
        profile.ip_family = MoshIpFamily::Ipv4;
        profile.prediction = MoshPredictionMode::Always;
        profile.locale = Some("en_US.UTF-8".to_string());
        profile.proxy_chain.push(SavedProxyHop {
            host: "jump.example.com".to_string(),
            port: 2200,
            username: "jump".to_string(),
            auth: SavedAuth::Agent,
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
        });

        let form = form_from_mosh_profile(&profile, "Ungrouped".to_string());

        assert_eq!(form.mosh_profile_id.as_deref(), Some("mosh-1"));
        assert_eq!(form.transport, NewConnectionTransport::Mosh);
        assert_eq!(form.name, "Roaming shell");
        assert_eq!(form.host, "mosh.example.com");
        assert_eq!(form.port, "2222");
        assert_eq!(form.username, "operator");
        assert_eq!(form.group, "Mobile");
        assert_eq!(form.notes, "Intermittent link");
        assert_eq!(form.icon, "wifi");
        assert_eq!(form.color, "#93c5fd");
        assert_eq!(form.mosh_server_executable, "/opt/mosh/bin/mosh-server");
        assert_eq!(form.mosh_udp_host, "udp.example.com");
        assert_eq!(form.mosh_udp_port, "60000:60010");
        assert_eq!(form.mosh_ip_family, MoshIpFamily::Ipv4);
        assert_eq!(form.mosh_prediction, MoshPredictionMode::Always);
        assert_eq!(form.mosh_locale, "en_US.UTF-8");
        assert_eq!(form.auth_tab, SshAuthTab::Password);
        assert_eq!(
            form.saved_password_keychain_id.as_deref(),
            Some("mosh-password-owner")
        );
        assert!(form.save_password);
        assert!(form.password.is_empty());
        assert!(form.proxy_chain_expanded);
        assert_eq!(form.proxy_hops.len(), 1);
        assert_eq!(form.proxy_hops[0].host, "jump.example.com");
        assert_eq!(form.proxy_hops[0].persisted_proxy_hop_index, Some(0));
    }

    #[test]
    fn serial_profile_form_restores_saved_line_settings_for_editing() {
        let mut profile = SerialProfile::new("Console cable", "/dev/cu.usbserial-10");
        profile.id = "serial-1".to_string();
        profile.group = Some("Lab".to_string());
        profile.notes = Some("Rack B".to_string());
        profile.icon = Some("radio".to_string());
        profile.color = Some("#fbbf24".to_string());
        profile.icon_background_color = Some("#451a03".to_string());
        profile.baud_rate = 57_600;
        profile.data_bits = 7;
        profile.stop_bits = 2;
        profile.parity = SerialParity::Even;
        profile.flow_control = SerialFlowControl::Hardware;

        let form = form_from_serial_profile(&profile, "Ungrouped".to_string());

        assert_eq!(form.serial_profile_id.as_deref(), Some("serial-1"));
        assert_eq!(form.transport, NewConnectionTransport::Serial);
        assert_eq!(form.serial_profile_name, "Console cable");
        assert_eq!(form.group, "Lab");
        assert_eq!(form.notes, "Rack B");
        assert_eq!(form.icon, "radio");
        assert_eq!(form.color, "#fbbf24");
        assert_eq!(form.icon_background_color, "#451a03");
        assert_eq!(form.serial_port_path, "/dev/cu.usbserial-10");
        assert_eq!(form.serial_baud_rate, "57600");
        assert_eq!(form.serial_data_bits, 7);
        assert_eq!(form.serial_stop_bits, 2);
        assert_eq!(form.serial_parity, oxideterm_terminal::SerialParity::Even);
        assert_eq!(
            form.serial_flow_control,
            oxideterm_terminal::SerialFlowControl::Hardware
        );
    }

    #[test]
    fn telnet_profile_form_restores_endpoint_and_terminal_settings_for_editing() {
        let mut profile = TelnetProfile::new("Router console", "router.example.com", 2323);
        profile.id = "telnet-1".to_string();
        profile.group = Some("Lab".to_string());
        profile.notes = Some("Legacy management plane".to_string());
        profile.icon = Some("network".to_string());
        profile.color = Some("#86efac".to_string());
        profile.icon_background_color = Some("#052e16".to_string());
        profile.terminal.encoding = Some(oxideterm_connections::ConnectionTerminalEncoding::Big5);
        profile.terminal.backspace_sequence =
            Some(oxideterm_connections::ConnectionTerminalBackspaceSequence::Delete);

        let form = form_from_telnet_profile(&profile, "Ungrouped".to_string());

        assert_eq!(form.telnet_profile_id.as_deref(), Some("telnet-1"));
        assert_eq!(form.transport, NewConnectionTransport::Telnet);
        assert_eq!(form.telnet_profile_name, "Router console");
        assert_eq!(form.group, "Lab");
        assert_eq!(form.notes, "Legacy management plane");
        assert_eq!(form.icon, "network");
        assert_eq!(form.color, "#86efac");
        assert_eq!(form.icon_background_color, "#052e16");
        assert_eq!(form.host, "router.example.com");
        assert_eq!(form.port, "2323");
        assert_eq!(form.terminal, profile.terminal);
    }

    #[test]
    fn text_input_uses_platform_text_not_binding_key() {
        let shifted = keystroke(
            "1",
            Some("!"),
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        );
        let option_char = keystroke(
            "s",
            Some("ß"),
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        );

        assert_eq!(text_from_keystroke(&shifted), Some("!"));
        assert_eq!(text_from_keystroke(&option_char), Some("ß"));
    }

    #[test]
    fn text_input_ignores_shortcut_keystrokes() {
        let shortcut = keystroke(
            "v",
            None,
            Modifiers {
                platform: true,
                ..Modifiers::default()
            },
        );
        let control = keystroke(
            "a",
            Some("\u{1}"),
            Modifiers {
                control: true,
                ..Modifiers::default()
            },
        );

        assert_eq!(text_from_keystroke(&shortcut), None);
        assert_eq!(text_from_keystroke(&control), None);
    }

    #[test]
    fn selected_text_is_replaced_by_committed_input() {
        let mut form = NewConnectionForm::default();
        form.host = "example.test".to_string();
        form.focused_field = NewConnectionField::Host;
        select_current_connection_field(&mut form);
        insert_text_into_current_connection_field(&mut form, "192.168.1.10");
        assert_eq!(form.host, "192.168.1.10");
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn remote_desktop_form_uses_privacy_preserving_session_defaults() {
        let form = NewConnectionForm::default();

        assert!(form.remote_desktop_session_options.clipboard.text);
        assert!(form.remote_desktop_session_options.clipboard.images);
        assert!(!form.remote_desktop_session_options.clipboard.files);
        assert!(form.remote_desktop_session_options.audio.playback);
        assert!(!form.remote_desktop_session_options.audio.capture);
        assert!(!form.remote_desktop_session_options.display.use_all_monitors);
        assert_eq!(
            form.remote_desktop_session_options.vnc.security_policy,
            RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption
        );
        assert_eq!(
            form.remote_desktop_session_options.vnc.session_mode,
            RemoteDesktopVncSessionMode::Shared
        );
        assert_eq!(
            form.remote_desktop_session_options.vnc.image_quality,
            RemoteDesktopVncImageQuality::Balanced
        );
        assert_eq!(
            form.remote_desktop_session_options.vnc.compression,
            RemoteDesktopVncCompression::Balanced
        );
    }

    #[test]
    fn backspace_handles_selection_and_empty_fields() {
        let mut form = NewConnectionForm::default();
        form.username = "root".to_string();
        form.focused_field = NewConnectionField::Username;
        select_current_connection_field(&mut form);
        assert!(backspace_current_connection_field(&mut form));
        assert!(form.username.is_empty());
        assert_eq!(form.selected_field, None);

        // Unselected text is edited one character at a time.
        form.username = "root".to_string();
        assert!(backspace_current_connection_field(&mut form));
        assert_eq!(form.username, "roo");
        assert_eq!(form.selected_field, None);

        // Empty fields report no change and stale selections are discarded.
        form.username.clear();
        form.focused_field = NewConnectionField::Name;
        assert!(!backspace_current_connection_field(&mut form));
        assert_eq!(form.selected_field, None);

        form.focused_field = NewConnectionField::Username;
        form.selected_field = Some(NewConnectionField::Host);
        assert!(backspace_current_connection_field(&mut form));
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn jump_hop_uses_saved_connection_metadata_without_secrets() {
        let connection = ConnectionInfo {
            id: "conn-1".to_string(),
            name: "Bastion".to_string(),
            group: Some("Prod".to_string()),
            notes: None,
            host: "bastion.example.com".to_string(),
            port: 2222,
            username: "jump".to_string(),
            auth_type: AuthType::Certificate,
            key_path: Some("~/.ssh/id_ed25519".to_string()),
            cert_path: Some("~/.ssh/id_ed25519-cert.pub".to_string()),
            managed_key_id: None,
            managed_key_name: None,
            gssapi_authentication: false,
            gssapi_server_identity: None,
            gssapi_delegate_credentials: false,
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            created_at: "2026-06-15T00:00:00Z".to_string(),
            last_used_at: None,
            color: None,
            icon_background_color: None,
            icon: None,
            tags: Vec::new(),
            agent_forwarding: true,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: true,
            ssh_algorithms: oxideterm_connections::SshAlgorithmPreferences::default(),
            post_connect_command: None,
        };
        let mut hop = NewConnectionProxyHop::new();
        hop.password = "old-password".to_string();
        hop.passphrase = "old-passphrase".to_string();

        hop.apply_saved_connection(&connection);

        assert_eq!(hop.saved_connection_id, "conn-1");
        assert_eq!(hop.host, "bastion.example.com");
        assert_eq!(hop.port, "2222");
        assert_eq!(hop.username, "jump");
        assert_eq!(hop.auth_tab, SshAuthTab::Certificate);
        assert_eq!(auth_family_from_tab(hop.auth_tab), SshAuthFamily::Key);
        assert_eq!(
            key_source_from_tab(hop.auth_tab),
            Some(SshKeyAuthSource::Certificate)
        );
        assert_eq!(hop.key_path, "~/.ssh/id_ed25519");
        assert_eq!(hop.cert_path, "~/.ssh/id_ed25519-cert.pub");
        assert!(hop.password.is_empty());
        assert!(hop.passphrase.is_empty());
        assert!(hop.agent_forwarding);
        assert!(hop.legacy_ssh_compatibility);
    }
}
