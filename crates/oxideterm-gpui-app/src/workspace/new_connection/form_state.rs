use std::fmt;

use oxideterm_connections::{
    AuthType, ConnectionInfo, PrivilegeCredentialKind, RawTcpDisplayMode, RawTcpLineEnding,
    RawTcpSendMode, RawTcpTlsMode, RawTcpTlsVerification, RawUdpDisplayMode, RawUdpLineEnding,
    RawUdpSendMode, SavedUpstreamProxyProtocol, TransportUsernameTransition,
    transport_port_replacement, transport_username_transition,
};
pub(in crate::workspace) use oxideterm_connections::{
    ConnectionTransport as NewConnectionTransport, RAW_TCP_DEFAULT_PORT_TEXT,
    RAW_UDP_DEFAULT_PORT_TEXT, RDP_DEFAULT_PORT_TEXT, SSH_DEFAULT_PORT_TEXT,
    TELNET_DEFAULT_PORT_TEXT, VNC_DEFAULT_PORT_TEXT,
};

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

    pub(in crate::workspace) fn stores_connection_on_connect(self) -> bool {
        self == Self::NewConnection
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum NewConnectionSelect {
    Group,
    KeyAuthSource,
    ManagedKey,
    JumpSavedConnection,
    JumpKeyAuthSource,
    JumpManagedKey,
    UpstreamProxyPolicy,
    UpstreamProxyProtocol,
    UpstreamProxyAuth,
    SerialPort,
    SerialDataBits,
    SerialStopBits,
    SerialParity,
    SerialFlowControl,
    RawTcpLineEnding,
    RawTcpDisplayMode,
    RawTcpSendMode,
    RawTcpTlsMode,
    RawTcpTlsVerification,
    RawUdpLineEnding,
    RawUdpDisplayMode,
    RawUdpSendMode,
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
    Group,
    PostConnectCommand,
    Color,
    JumpHost,
    JumpPort,
    JumpUsername,
    JumpPassword,
    JumpKeyPath,
    JumpManagedKeyId,
    JumpCertPath,
    JumpPassphrase,
    UpstreamProxyHost,
    UpstreamProxyPort,
    UpstreamProxyNoProxy,
    UpstreamProxyUsername,
    UpstreamProxyPassword,
    SerialPortPath,
    SerialBaudRate,
    SerialProfileName,
    TelnetProfileName,
    RawTcpProfileName,
    RawTcpTlsServerName,
    RawUdpProfileName,
    RawUdpLocalBindHost,
    RawUdpLocalBindPort,
}

#[derive(Clone)]
pub(in crate::workspace) struct PrivilegeCredentialDraft {
    pub(in crate::workspace) credential_id: Option<String>,
    pub(in crate::workspace) label: String,
    pub(in crate::workspace) kind: PrivilegeCredentialKind,
    pub(in crate::workspace) username_hint: String,
    pub(in crate::workspace) prompt_patterns: String,
    pub(in crate::workspace) secret: String,
    pub(in crate::workspace) enabled: bool,
}

impl fmt::Debug for PrivilegeCredentialDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrivilegeCredentialDraft")
            .field("credential_id", &self.credential_id)
            .field("label", &self.label)
            .field("kind", &self.kind)
            .field("username_hint", &self.username_hint)
            .field("prompt_patterns", &self.prompt_patterns)
            .field("secret", &"[redacted secret]")
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl Default for PrivilegeCredentialDraft {
    fn default() -> Self {
        Self {
            credential_id: None,
            label: String::new(),
            kind: PrivilegeCredentialKind::SudoPassword,
            username_hint: String::new(),
            prompt_patterns: String::new(),
            secret: String::new(),
            enabled: true,
        }
    }
}

#[derive(Clone)]
pub(in crate::workspace) struct NewConnectionProxyHop {
    pub(in crate::workspace) saved_connection_id: String,
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: String,
    pub(in crate::workspace) username: String,
    pub(in crate::workspace) auth_tab: SshAuthTab,
    pub(in crate::workspace) password: String,
    pub(in crate::workspace) key_path: String,
    pub(in crate::workspace) managed_key_id: String,
    pub(in crate::workspace) cert_path: String,
    pub(in crate::workspace) passphrase: String,
    pub(in crate::workspace) agent_forwarding: bool,
    pub(in crate::workspace) legacy_ssh_compatibility: bool,
}

impl fmt::Debug for NewConnectionProxyHop {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewConnectionProxyHop")
            .field("saved_connection_id", &self.saved_connection_id)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth_tab", &self.auth_tab)
            .field("password", &"[redacted secret]")
            .field("key_path", &self.key_path)
            .field("managed_key_id", &self.managed_key_id)
            .field("cert_path", &self.cert_path)
            .field("passphrase", &"[redacted secret]")
            .field("agent_forwarding", &self.agent_forwarding)
            .field("legacy_ssh_compatibility", &self.legacy_ssh_compatibility)
            .finish()
    }
}

impl NewConnectionProxyHop {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            saved_connection_id: String::new(),
            host: String::new(),
            port: SSH_DEFAULT_PORT_TEXT.to_string(),
            username: String::new(),
            auth_tab: SshAuthTab::SshKey,
            password: String::new(),
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            agent_forwarding: false,
            legacy_ssh_compatibility: false,
        }
    }

    pub(in crate::workspace) fn complete(&self) -> bool {
        !self.host.trim().is_empty() && !self.username.trim().is_empty()
    }

    pub(in crate::workspace) fn apply_saved_connection(&mut self, connection: &ConnectionInfo) {
        self.saved_connection_id = connection.id.clone();
        self.host = connection.host.clone();
        self.port = connection.port.to_string();
        self.username = connection.username.clone();
        self.auth_tab = match connection.auth_type {
            AuthType::Password => SshAuthTab::Password,
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
        self.agent_forwarding = connection.agent_forwarding;
        self.legacy_ssh_compatibility = connection.legacy_ssh_compatibility;
    }
}

#[derive(Clone)]
pub(in crate::workspace) struct NewConnectionForm {
    pub(in crate::workspace) transport: NewConnectionTransport,
    pub(in crate::workspace) name: String,
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: String,
    pub(in crate::workspace) username: String,
    pub(in crate::workspace) auth_tab: SshAuthTab,
    pub(in crate::workspace) password: String,
    pub(in crate::workspace) saved_password_keychain_id: Option<String>,
    pub(in crate::workspace) password_loaded: bool,
    pub(in crate::workspace) password_visible: bool,
    pub(in crate::workspace) password_loading: bool,
    pub(in crate::workspace) password_error: Option<String>,
    pub(in crate::workspace) key_path: String,
    pub(in crate::workspace) managed_key_id: String,
    pub(in crate::workspace) cert_path: String,
    pub(in crate::workspace) passphrase: String,
    pub(in crate::workspace) save_password: bool,
    pub(in crate::workspace) group: String,
    pub(in crate::workspace) post_connect_command: String,
    pub(in crate::workspace) color: String,
    pub(in crate::workspace) icon: String,
    pub(in crate::workspace) icon_picker_expanded: bool,
    pub(in crate::workspace) tags: Vec<String>,
    pub(in crate::workspace) proxy_hops: Vec<NewConnectionProxyHop>,
    pub(in crate::workspace) proxy_chain_expanded: bool,
    pub(in crate::workspace) jump_server_form: Option<NewConnectionProxyHop>,
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
    pub(in crate::workspace) legacy_ssh_compatibility: bool,
    pub(in crate::workspace) agent_available: Option<bool>,
    pub(in crate::workspace) save_connection: bool,
    pub(in crate::workspace) field_focused: bool,
    pub(in crate::workspace) focused_field: NewConnectionField,
    pub(in crate::workspace) selected_field: Option<NewConnectionField>,
    pub(in crate::workspace) error: Option<String>,
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
    pub(in crate::workspace) raw_tcp_profile_name: String,
    pub(in crate::workspace) raw_tcp_line_ending: RawTcpLineEnding,
    pub(in crate::workspace) raw_tcp_display_mode: RawTcpDisplayMode,
    pub(in crate::workspace) raw_tcp_send_mode: RawTcpSendMode,
    pub(in crate::workspace) raw_tcp_tls_mode: RawTcpTlsMode,
    pub(in crate::workspace) raw_tcp_tls_verification: RawTcpTlsVerification,
    pub(in crate::workspace) raw_tcp_tls_server_name: String,
    pub(in crate::workspace) raw_udp_profile_name: String,
    pub(in crate::workspace) raw_udp_local_bind_host: String,
    pub(in crate::workspace) raw_udp_local_bind_port: String,
    pub(in crate::workspace) raw_udp_line_ending: RawUdpLineEnding,
    pub(in crate::workspace) raw_udp_display_mode: RawUdpDisplayMode,
    pub(in crate::workspace) raw_udp_send_mode: RawUdpSendMode,
}

impl fmt::Debug for NewConnectionForm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewConnectionForm")
            .field("transport", &self.transport)
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth_tab", &self.auth_tab)
            .field("password", &"[redacted secret]")
            .field(
                "saved_password_keychain_id",
                &self.saved_password_keychain_id,
            )
            .field("password_loaded", &self.password_loaded)
            .field("password_visible", &self.password_visible)
            .field("password_loading", &self.password_loading)
            .field("password_error", &self.password_error)
            .field("key_path", &self.key_path)
            .field("managed_key_id", &self.managed_key_id)
            .field("cert_path", &self.cert_path)
            .field("passphrase", &"[redacted secret]")
            .field("save_password", &self.save_password)
            .field("group", &self.group)
            .field("post_connect_command", &self.post_connect_command)
            .field("color", &self.color)
            .field("icon", &self.icon)
            .field("icon_picker_expanded", &self.icon_picker_expanded)
            .field("tags", &self.tags)
            .field("proxy_hops", &self.proxy_hops)
            .field("proxy_chain_expanded", &self.proxy_chain_expanded)
            .field("jump_server_form", &self.jump_server_form)
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
            .field("legacy_ssh_compatibility", &self.legacy_ssh_compatibility)
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
            .field("raw_tcp_profile_name", &self.raw_tcp_profile_name)
            .field("raw_tcp_line_ending", &self.raw_tcp_line_ending)
            .field("raw_tcp_display_mode", &self.raw_tcp_display_mode)
            .field("raw_tcp_send_mode", &self.raw_tcp_send_mode)
            .field("raw_tcp_tls_mode", &self.raw_tcp_tls_mode)
            .field("raw_tcp_tls_verification", &self.raw_tcp_tls_verification)
            .field("raw_tcp_tls_server_name", &self.raw_tcp_tls_server_name)
            .field("raw_udp_profile_name", &self.raw_udp_profile_name)
            .field("raw_udp_local_bind_host", &self.raw_udp_local_bind_host)
            .field("raw_udp_local_bind_port", &self.raw_udp_local_bind_port)
            .field("raw_udp_line_ending", &self.raw_udp_line_ending)
            .field("raw_udp_display_mode", &self.raw_udp_display_mode)
            .field("raw_udp_send_mode", &self.raw_udp_send_mode)
            .finish()
    }
}

impl Default for NewConnectionForm {
    fn default() -> Self {
        Self {
            transport: NewConnectionTransport::Ssh,
            name: String::new(),
            host: String::new(),
            port: SSH_DEFAULT_PORT_TEXT.to_string(),
            username: "root".to_string(),
            auth_tab: SshAuthTab::Password,
            password: String::new(),
            saved_password_keychain_id: None,
            password_loaded: true,
            password_visible: false,
            password_loading: false,
            password_error: None,
            key_path: String::new(),
            managed_key_id: String::new(),
            cert_path: String::new(),
            passphrase: String::new(),
            save_password: false,
            group: String::new(),
            post_connect_command: String::new(),
            color: String::new(),
            icon: String::new(),
            icon_picker_expanded: false,
            tags: Vec::new(),
            proxy_hops: Vec::new(),
            proxy_chain_expanded: false,
            jump_server_form: None,
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
            legacy_ssh_compatibility: false,
            agent_available: None,
            save_connection: false,
            field_focused: true,
            focused_field: NewConnectionField::Name,
            selected_field: None,
            error: None,
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
            raw_tcp_profile_name: String::new(),
            raw_tcp_line_ending: RawTcpLineEnding::default(),
            raw_tcp_display_mode: RawTcpDisplayMode::default(),
            raw_tcp_send_mode: RawTcpSendMode::default(),
            raw_tcp_tls_mode: RawTcpTlsMode::default(),
            raw_tcp_tls_verification: RawTcpTlsVerification::default(),
            raw_tcp_tls_server_name: String::new(),
            raw_udp_profile_name: String::new(),
            raw_udp_local_bind_host: String::new(),
            raw_udp_local_bind_port: "0".to_string(),
            raw_udp_line_ending: RawUdpLineEnding::default(),
            raw_udp_display_mode: RawUdpDisplayMode::default(),
            raw_udp_send_mode: RawUdpSendMode::default(),
        }
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
    transport: NewConnectionTransport,
    upstream_proxy_policy: NewConnectionUpstreamProxyPolicy,
    upstream_proxy_auth: NewConnectionUpstreamProxyAuth,
    forward: bool,
) -> NewConnectionField {
    if transport == NewConnectionTransport::WslGraphics {
        return NewConnectionField::Name;
    }
    if transport == NewConnectionTransport::Serial {
        let fields = [
            NewConnectionField::SerialPortPath,
            NewConnectionField::SerialBaudRate,
            NewConnectionField::SerialProfileName,
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
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::TelnetProfileName,
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
    if transport == NewConnectionTransport::RawTcp {
        let fields = [
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::RawTcpTlsServerName,
            NewConnectionField::RawTcpProfileName,
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
    if transport == NewConnectionTransport::RawUdp {
        let fields = [
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::RawUdpLocalBindHost,
            NewConnectionField::RawUdpLocalBindPort,
            NewConnectionField::RawUdpProfileName,
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
        let fields = if transport == NewConnectionTransport::Rdp {
            vec![
                NewConnectionField::Name,
                NewConnectionField::Host,
                NewConnectionField::Port,
                NewConnectionField::Username,
                NewConnectionField::Password,
            ]
        } else {
            vec![
                NewConnectionField::Name,
                NewConnectionField::Host,
                NewConnectionField::Port,
                NewConnectionField::Password,
            ]
        };
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
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Password,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::DefaultKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Passphrase,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::SshKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::KeyPath,
            NewConnectionField::Passphrase,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::ManagedKey => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::ManagedKeyId,
            NewConnectionField::Passphrase,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::Certificate => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::KeyPath,
            NewConnectionField::CertPath,
            NewConnectionField::Passphrase,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
        SshAuthTab::Agent | SshAuthTab::TwoFactor => vec![
            NewConnectionField::Name,
            NewConnectionField::Host,
            NewConnectionField::Port,
            NewConnectionField::Username,
            NewConnectionField::Group,
            NewConnectionField::PostConnectCommand,
        ],
    };
    if upstream_proxy_policy == NewConnectionUpstreamProxyPolicy::Custom {
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
    forward: bool,
) -> NewConnectionField {
    let fields: Vec<NewConnectionField> = match auth_tab {
        SshAuthTab::Password => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
            NewConnectionField::JumpPassword,
        ],
        SshAuthTab::DefaultKey | SshAuthTab::Agent => vec![
            NewConnectionField::JumpHost,
            NewConnectionField::JumpPort,
            NewConnectionField::JumpUsername,
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
        NewConnectionField::Group => &mut form.group,
        NewConnectionField::PostConnectCommand => &mut form.post_connect_command,
        NewConnectionField::UpstreamProxyHost => &mut form.upstream_proxy_host,
        NewConnectionField::UpstreamProxyPort => &mut form.upstream_proxy_port,
        NewConnectionField::UpstreamProxyNoProxy => &mut form.upstream_proxy_no_proxy,
        NewConnectionField::UpstreamProxyUsername => &mut form.upstream_proxy_username,
        NewConnectionField::UpstreamProxyPassword => &mut form.upstream_proxy_password,
        NewConnectionField::Color => &mut form.color,
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
        NewConnectionField::SerialPortPath => &mut form.serial_port_path,
        NewConnectionField::SerialBaudRate => &mut form.serial_baud_rate,
        NewConnectionField::SerialProfileName => &mut form.serial_profile_name,
        NewConnectionField::TelnetProfileName => &mut form.telnet_profile_name,
        NewConnectionField::RawTcpProfileName => &mut form.raw_tcp_profile_name,
        NewConnectionField::RawTcpTlsServerName => &mut form.raw_tcp_tls_server_name,
        NewConnectionField::RawUdpProfileName => &mut form.raw_udp_profile_name,
        NewConnectionField::RawUdpLocalBindHost => &mut form.raw_udp_local_bind_host,
        NewConnectionField::RawUdpLocalBindPort => &mut form.raw_udp_local_bind_port,
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
        NewConnectionField::Group => &form.group,
        NewConnectionField::PostConnectCommand => &form.post_connect_command,
        NewConnectionField::UpstreamProxyHost => &form.upstream_proxy_host,
        NewConnectionField::UpstreamProxyPort => &form.upstream_proxy_port,
        NewConnectionField::UpstreamProxyNoProxy => &form.upstream_proxy_no_proxy,
        NewConnectionField::UpstreamProxyUsername => &form.upstream_proxy_username,
        NewConnectionField::UpstreamProxyPassword => &form.upstream_proxy_password,
        NewConnectionField::Color => &form.color,
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
        NewConnectionField::SerialPortPath => &form.serial_port_path,
        NewConnectionField::SerialBaudRate => &form.serial_baud_rate,
        NewConnectionField::SerialProfileName => &form.serial_profile_name,
        NewConnectionField::TelnetProfileName => &form.telnet_profile_name,
        NewConnectionField::RawTcpProfileName => &form.raw_tcp_profile_name,
        NewConnectionField::RawTcpTlsServerName => &form.raw_tcp_tls_server_name,
        NewConnectionField::RawUdpProfileName => &form.raw_udp_profile_name,
        NewConnectionField::RawUdpLocalBindHost => &form.raw_udp_local_bind_host,
        NewConnectionField::RawUdpLocalBindPort => &form.raw_udp_local_bind_port,
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
    let replacing_selection = form.selected_field == Some(form.focused_field);
    if replacing_selection {
        current_connection_field_mut(form).clear();
    }
    current_connection_field_mut(form).push_str(text);
    form.selected_field = None;
}

pub(in crate::workspace) fn backspace_current_connection_field(
    form: &mut NewConnectionForm,
) -> bool {
    let selection_was_visible = form.selected_field.is_some();
    if form.selected_field == Some(form.focused_field) {
        // Clearing a selected field also clears visible selection state. Track
        // text separately so empty selected fields still report a UI change.
        let field = current_connection_field_mut(form);
        let text_changed = !field.is_empty();
        field.clear();
        form.selected_field = None;
        text_changed || selection_was_visible
    } else {
        let text_changed = current_connection_field_mut(form).pop().is_some();
        form.selected_field = None;
        text_changed || selection_was_visible
    }
}

pub(in crate::workspace) fn clear_current_connection_field(form: &mut NewConnectionForm) {
    current_connection_field_mut(form).clear();
    form.selected_field = None;
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
    use gpui::{Keystroke, Modifiers};
    use oxideterm_connections::{
        AuthType, ConnectionInfo, RawTcpDisplayMode, RawTcpLineEnding, RawTcpSendMode,
        RawTcpTlsMode, RawTcpTlsVerification, SavedUpstreamProxyPolicy,
    };

    use super::{
        NewConnectionField, NewConnectionForm, NewConnectionFormMode, NewConnectionProxyHop,
        NewConnectionTransport, RDP_DEFAULT_PORT_TEXT, SSH_DEFAULT_PORT_TEXT,
        SavedConnectionPromptAction, SshAuthFamily, SshAuthTab, SshKeyAuthSource,
        TELNET_DEFAULT_PORT_TEXT, VNC_DEFAULT_PORT_TEXT, apply_transport_default_port,
        apply_transport_default_username, auth_family_from_tab, auth_tab_from_key_source,
        backspace_current_connection_field, default_auth_tab_for_family,
        insert_text_into_current_connection_field, key_source_from_tab, new_connection_form_mode,
        next_connection_field, select_current_connection_field, text_from_keystroke,
    };

    fn keystroke(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
        }
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
        let mut form = NewConnectionForm {
            host: "example.test".to_string(),
            focused_field: NewConnectionField::Host,
            ..NewConnectionForm::default()
        };
        select_current_connection_field(&mut form);
        insert_text_into_current_connection_field(&mut form, "192.168.1.10");
        assert_eq!(form.host, "192.168.1.10");
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn telnet_transport_tabs_between_endpoint_and_profile_name() {
        assert_eq!(
            next_connection_field(
                NewConnectionField::Host,
                super::SshAuthTab::Password,
                NewConnectionTransport::Telnet,
                super::NewConnectionUpstreamProxyPolicy::UseGlobal,
                super::NewConnectionUpstreamProxyAuth::None,
                true,
            ),
            NewConnectionField::Port
        );
        assert_eq!(
            next_connection_field(
                NewConnectionField::Port,
                super::SshAuthTab::Password,
                NewConnectionTransport::Telnet,
                super::NewConnectionUpstreamProxyPolicy::UseGlobal,
                super::NewConnectionUpstreamProxyAuth::None,
                true,
            ),
            NewConnectionField::TelnetProfileName
        );
        assert_eq!(
            next_connection_field(
                NewConnectionField::TelnetProfileName,
                super::SshAuthTab::Password,
                NewConnectionTransport::Telnet,
                super::NewConnectionUpstreamProxyPolicy::UseGlobal,
                super::NewConnectionUpstreamProxyAuth::None,
                true,
            ),
            NewConnectionField::Host
        );
    }

    #[test]
    fn raw_tcp_form_defaults_match_plain_text_socket_mode() {
        let form = NewConnectionForm {
            transport: NewConnectionTransport::RawTcp,
            ..NewConnectionForm::default()
        };

        assert_eq!(form.raw_tcp_line_ending, RawTcpLineEnding::CrLf);
        assert_eq!(form.raw_tcp_display_mode, RawTcpDisplayMode::Text);
        assert_eq!(form.raw_tcp_send_mode, RawTcpSendMode::Text);
        assert_eq!(form.raw_tcp_tls_mode, RawTcpTlsMode::Disabled);
        assert_eq!(form.raw_tcp_tls_verification, RawTcpTlsVerification::System);
        assert!(form.raw_tcp_tls_server_name.is_empty());
    }

    #[test]
    fn raw_tcp_transport_tabs_through_endpoint_tls_name_and_profile_name() {
        assert_eq!(
            next_connection_field(
                NewConnectionField::Host,
                super::SshAuthTab::Password,
                NewConnectionTransport::RawTcp,
                super::NewConnectionUpstreamProxyPolicy::UseGlobal,
                super::NewConnectionUpstreamProxyAuth::None,
                true,
            ),
            NewConnectionField::Port
        );
        assert_eq!(
            next_connection_field(
                NewConnectionField::Port,
                super::SshAuthTab::Password,
                NewConnectionTransport::RawTcp,
                super::NewConnectionUpstreamProxyPolicy::UseGlobal,
                super::NewConnectionUpstreamProxyAuth::None,
                true,
            ),
            NewConnectionField::RawTcpTlsServerName
        );
        assert_eq!(
            next_connection_field(
                NewConnectionField::RawTcpTlsServerName,
                super::SshAuthTab::Password,
                NewConnectionTransport::RawTcp,
                super::NewConnectionUpstreamProxyPolicy::UseGlobal,
                super::NewConnectionUpstreamProxyAuth::None,
                true,
            ),
            NewConnectionField::RawTcpProfileName
        );
    }

    #[test]
    fn remote_desktop_transport_tabs_through_rdp_login_fields() {
        assert_eq!(
            next_connection_field(
                NewConnectionField::Name,
                super::SshAuthTab::Password,
                NewConnectionTransport::Rdp,
                super::NewConnectionUpstreamProxyPolicy::UseGlobal,
                super::NewConnectionUpstreamProxyAuth::None,
                true,
            ),
            NewConnectionField::Host
        );
        assert_eq!(
            next_connection_field(
                NewConnectionField::Port,
                super::SshAuthTab::Password,
                NewConnectionTransport::Rdp,
                super::NewConnectionUpstreamProxyPolicy::UseGlobal,
                super::NewConnectionUpstreamProxyAuth::None,
                true,
            ),
            NewConnectionField::Username
        );
        assert_eq!(
            next_connection_field(
                NewConnectionField::Port,
                super::SshAuthTab::Password,
                NewConnectionTransport::Vnc,
                super::NewConnectionUpstreamProxyPolicy::UseGlobal,
                super::NewConnectionUpstreamProxyAuth::None,
                true,
            ),
            NewConnectionField::Password
        );
        assert_eq!(
            next_connection_field(
                NewConnectionField::Password,
                super::SshAuthTab::Password,
                NewConnectionTransport::Vnc,
                super::NewConnectionUpstreamProxyPolicy::UseGlobal,
                super::NewConnectionUpstreamProxyAuth::None,
                true,
            ),
            NewConnectionField::Name
        );
    }

    #[test]
    fn transport_default_port_changes_only_for_untouched_defaults() {
        let mut form = NewConnectionForm::default();
        apply_transport_default_port(
            &mut form,
            NewConnectionTransport::Ssh,
            NewConnectionTransport::Telnet,
        );
        assert_eq!(form.port, TELNET_DEFAULT_PORT_TEXT);

        apply_transport_default_port(
            &mut form,
            NewConnectionTransport::Telnet,
            NewConnectionTransport::Ssh,
        );
        assert_eq!(form.port, SSH_DEFAULT_PORT_TEXT);

        apply_transport_default_port(
            &mut form,
            NewConnectionTransport::Ssh,
            NewConnectionTransport::Rdp,
        );
        assert_eq!(form.port, RDP_DEFAULT_PORT_TEXT);

        apply_transport_default_port(
            &mut form,
            NewConnectionTransport::Rdp,
            NewConnectionTransport::Vnc,
        );
        assert_eq!(form.port, VNC_DEFAULT_PORT_TEXT);

        form.port = SSH_DEFAULT_PORT_TEXT.to_string();
        apply_transport_default_port(
            &mut form,
            NewConnectionTransport::Serial,
            NewConnectionTransport::Rdp,
        );
        assert_eq!(form.port, RDP_DEFAULT_PORT_TEXT);

        form.port = "2323".to_string();
        apply_transport_default_port(
            &mut form,
            NewConnectionTransport::Vnc,
            NewConnectionTransport::Rdp,
        );
        assert_eq!(form.port, "2323");
    }

    #[test]
    fn transport_default_username_changes_only_for_protocol_defaults() {
        let mut form = NewConnectionForm::default();

        apply_transport_default_username(
            &mut form,
            NewConnectionTransport::Ssh,
            NewConnectionTransport::Rdp,
        );
        assert_eq!(form.username, "Administrator");

        apply_transport_default_username(
            &mut form,
            NewConnectionTransport::Rdp,
            NewConnectionTransport::Vnc,
        );
        assert!(form.username.is_empty());

        apply_transport_default_username(
            &mut form,
            NewConnectionTransport::Vnc,
            NewConnectionTransport::Ssh,
        );
        assert_eq!(form.username, "root");

        form.username = "custom".to_string();
        apply_transport_default_username(
            &mut form,
            NewConnectionTransport::Ssh,
            NewConnectionTransport::Rdp,
        );
        assert_eq!(form.username, "custom");
    }

    #[test]
    fn backspace_clears_selected_field() {
        let mut form = NewConnectionForm {
            username: "root".to_string(),
            focused_field: NewConnectionField::Username,
            ..NewConnectionForm::default()
        };
        select_current_connection_field(&mut form);
        assert!(backspace_current_connection_field(&mut form));
        assert!(form.username.is_empty());
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn backspace_reports_text_changes_without_selection() {
        let mut form = NewConnectionForm {
            username: "root".to_string(),
            focused_field: NewConnectionField::Username,
            ..NewConnectionForm::default()
        };

        assert!(backspace_current_connection_field(&mut form));
        assert_eq!(form.username, "roo");
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn backspace_reports_false_for_empty_unselected_field() {
        let mut form = NewConnectionForm {
            focused_field: NewConnectionField::Name,
            ..NewConnectionForm::default()
        };

        assert!(!backspace_current_connection_field(&mut form));
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn backspace_clears_stale_selection_state() {
        let mut form = NewConnectionForm {
            focused_field: NewConnectionField::Username,
            selected_field: Some(NewConnectionField::Host),
            ..NewConnectionForm::default()
        };

        assert!(backspace_current_connection_field(&mut form));
        assert_eq!(form.selected_field, None);
    }

    #[test]
    fn form_mode_keeps_prompt_edit_and_new_submission_paths_distinct() {
        assert_eq!(
            new_connection_form_mode(None, None, None),
            NewConnectionFormMode::NewConnection
        );
        assert_eq!(
            new_connection_form_mode(Some("conn-1"), None, None),
            NewConnectionFormMode::EditProperties
        );
        assert_eq!(
            new_connection_form_mode(None, Some("conn-1"), None),
            NewConnectionFormMode::DuplicateTemplate
        );
        assert_eq!(
            new_connection_form_mode(
                Some("conn-1"),
                Some("conn-2"),
                Some(SavedConnectionPromptAction::Connect)
            ),
            NewConnectionFormMode::SavedConnectionPrompt
        );

        assert!(NewConnectionFormMode::NewConnection.stores_connection_on_connect());
        assert!(!NewConnectionFormMode::SavedConnectionPrompt.stores_connection_on_connect());
        assert!(NewConnectionFormMode::EditProperties.submits_saved_connection_properties());
        assert!(NewConnectionFormMode::DuplicateTemplate.submits_saved_connection_properties());
        assert!(
            !NewConnectionFormMode::SavedConnectionPrompt.submits_saved_connection_properties()
        );
    }

    #[test]
    fn auth_family_groups_all_key_tabs() {
        for tab in [
            SshAuthTab::DefaultKey,
            SshAuthTab::SshKey,
            SshAuthTab::ManagedKey,
            SshAuthTab::Certificate,
        ] {
            assert_eq!(auth_family_from_tab(tab), SshAuthFamily::Key);
        }
        assert_eq!(
            auth_family_from_tab(SshAuthTab::Password),
            SshAuthFamily::Password
        );
        assert_eq!(
            auth_family_from_tab(SshAuthTab::Agent),
            SshAuthFamily::Agent
        );
        assert_eq!(
            auth_family_from_tab(SshAuthTab::TwoFactor),
            SshAuthFamily::TwoFactor
        );
    }

    #[test]
    fn default_key_family_selection_uses_file_key_tab() {
        assert_eq!(
            default_auth_tab_for_family(SshAuthFamily::Key),
            SshAuthTab::SshKey
        );
    }

    #[test]
    fn key_source_round_trips_to_auth_tab() {
        for (source, tab) in [
            (SshKeyAuthSource::DefaultKey, SshAuthTab::DefaultKey),
            (SshKeyAuthSource::SshKey, SshAuthTab::SshKey),
            (SshKeyAuthSource::ManagedKey, SshAuthTab::ManagedKey),
            (SshKeyAuthSource::Certificate, SshAuthTab::Certificate),
        ] {
            assert_eq!(auth_tab_from_key_source(source), tab);
            assert_eq!(key_source_from_tab(tab), Some(source));
        }
        assert_eq!(key_source_from_tab(SshAuthTab::Password), None);
        assert_eq!(key_source_from_tab(SshAuthTab::Agent), None);
        assert_eq!(key_source_from_tab(SshAuthTab::TwoFactor), None);
    }

    #[test]
    fn jump_hop_uses_saved_connection_metadata_without_secrets() {
        let connection = ConnectionInfo {
            id: "conn-1".to_string(),
            name: "Bastion".to_string(),
            group: Some("Prod".to_string()),
            host: "bastion.example.com".to_string(),
            port: 2222,
            username: "jump".to_string(),
            auth_type: AuthType::Certificate,
            key_path: Some("~/.ssh/id_ed25519".to_string()),
            cert_path: Some("~/.ssh/id_ed25519-cert.pub".to_string()),
            managed_key_id: None,
            managed_key_name: None,
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            created_at: "2026-06-15T00:00:00Z".to_string(),
            last_used_at: None,
            color: None,
            icon: None,
            tags: Vec::new(),
            agent_forwarding: true,
            legacy_ssh_compatibility: true,
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
