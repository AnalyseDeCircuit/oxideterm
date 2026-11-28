//! Browser-side form choices. Desktop keeps its published field types via re-exports.

pub const SSH_DEFAULT_PORT_TEXT: &str = "22";
pub const MOSH_DEFAULT_PORT_TEXT: &str = "22";
pub const TELNET_DEFAULT_PORT_TEXT: &str = "23";
pub const RDP_DEFAULT_PORT_TEXT: &str = "3389";
pub const VNC_DEFAULT_PORT_TEXT: &str = "5900";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTransport {
    LocalTerminal,
    Ssh,
    StandaloneSftp,
    Ftp,
    Mosh,
    Telnet,
    Serial,
    Ble,
    Rdp,
    Vnc,
    WslGraphics,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportUsernameTransition {
    Set(&'static str),
    Clear,
}

pub fn transport_default_port(transport: ConnectionTransport) -> Option<&'static str> {
    match transport {
        ConnectionTransport::LocalTerminal => None,
        ConnectionTransport::Ssh => Some(SSH_DEFAULT_PORT_TEXT),
        ConnectionTransport::StandaloneSftp => Some(SSH_DEFAULT_PORT_TEXT),
        ConnectionTransport::Ftp => Some("21"),
        ConnectionTransport::Mosh => Some(MOSH_DEFAULT_PORT_TEXT),
        ConnectionTransport::Telnet => Some(TELNET_DEFAULT_PORT_TEXT),
        ConnectionTransport::Rdp => Some(RDP_DEFAULT_PORT_TEXT),
        ConnectionTransport::Vnc => Some(VNC_DEFAULT_PORT_TEXT),
        ConnectionTransport::Serial
        | ConnectionTransport::Ble
        | ConnectionTransport::WslGraphics => None,
    }
}

pub fn transport_port_replacement(
    current_port: &str,
    previous_transport: ConnectionTransport,
    next_transport: ConnectionTransport,
) -> Option<&'static str> {
    let current_port = current_port.trim();
    let next_default_port = transport_default_port(next_transport)?;
    let current_matches_previous_default = transport_default_port(previous_transport).map_or_else(
        || is_known_transport_default_port(current_port),
        |previous_default| current_port == previous_default,
    );

    (current_port.is_empty() || current_matches_previous_default).then_some(next_default_port)
}

pub fn transport_username_transition(
    current_username: &str,
    previous_transport: ConnectionTransport,
    next_transport: ConnectionTransport,
) -> Option<TransportUsernameTransition> {
    let username = current_username.trim();
    match next_transport {
        ConnectionTransport::Rdp
            if username.is_empty()
                || (matches!(
                    previous_transport,
                    ConnectionTransport::Ssh | ConnectionTransport::StandaloneSftp
                ) && username == "root") =>
        {
            Some(TransportUsernameTransition::Set("Administrator"))
        }
        ConnectionTransport::Vnc
            if matches!(
                previous_transport,
                ConnectionTransport::Ssh
                    | ConnectionTransport::StandaloneSftp
                    | ConnectionTransport::Rdp
            ) && matches!(username, "root" | "Administrator") =>
        {
            Some(TransportUsernameTransition::Clear)
        }
        ConnectionTransport::Ssh
        | ConnectionTransport::StandaloneSftp
        | ConnectionTransport::Mosh
            if previous_transport == ConnectionTransport::Rdp
                && (username == "Administrator" || username.is_empty()) =>
        {
            Some(TransportUsernameTransition::Set("root"))
        }
        ConnectionTransport::Ssh
        | ConnectionTransport::StandaloneSftp
        | ConnectionTransport::Mosh
            if previous_transport == ConnectionTransport::Vnc && username.is_empty() =>
        {
            Some(TransportUsernameTransition::Set("root"))
        }
        _ => None,
    }
}

pub fn transport_is_persistable(transport: ConnectionTransport) -> bool {
    // WSL graphics and BLE discovery is a launch surface without saved connection metadata.
    !matches!(
        transport,
        ConnectionTransport::WslGraphics | ConnectionTransport::Ble
    )
}

fn is_known_transport_default_port(port: &str) -> bool {
    [
        "21",
        SSH_DEFAULT_PORT_TEXT,
        MOSH_DEFAULT_PORT_TEXT,
        TELNET_DEFAULT_PORT_TEXT,
        RDP_DEFAULT_PORT_TEXT,
        VNC_DEFAULT_PORT_TEXT,
    ]
    .contains(&port)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthType {
    Password,
    Key,
    ManagedKey,
    Certificate,
    KeyboardInteractive,
    Agent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTerminalEncoding {
    Utf8,
    Gbk,
    Gb18030,
    Big5,
    ShiftJis,
    EucJp,
    EucKr,
    Windows1252,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTerminalBackspaceSequence {
    Delete,
    ControlH,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTerminalDeleteSequence {
    Csi3Tilde,
    Delete,
    ControlH,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionTerminalSessionLogPolicy {
    #[default]
    Inherit,
    Automatic,
    // Manual keeps the terminal action available without starting a log on connect.
    Manual,
    Disabled,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionTerminalBellPolicy {
    #[default]
    Inherit,
    Silent,
    TabHighlight,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnectionTerminalOptions {
    pub encoding: Option<ConnectionTerminalEncoding>,
    pub backspace_sequence: Option<ConnectionTerminalBackspaceSequence>,
    pub delete_sequence: Option<ConnectionTerminalDeleteSequence>,
    pub semantic_scheme: Option<String>,
    pub highlight_rule_set: Option<String>,
    pub session_log_policy: ConnectionTerminalSessionLogPolicy,
    pub bell_policy: ConnectionTerminalBellPolicy,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionX11ForwardingMode {
    #[default]
    Untrusted,
    Trusted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectionX11ForwardingOptions {
    pub enabled: bool,
    pub mode: ConnectionX11ForwardingMode,
    pub untrusted_timeout_seconds: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SshAlgorithmPreferences {
    pub kex: Vec<String>,
    pub host_key: Vec<String>,
    pub cipher: Vec<String>,
    pub mac: Vec<String>,
    pub compression: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SshChannelStrategy {
    #[default]
    Multiplexed,
    DedicatedPerConsumer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SavedUpstreamProxyProtocol {
    Socks5,
    HttpConnect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialParity {
    None,
    Odd,
    Even,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialFlowControl {
    None,
    Software,
    Hardware,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialLineEnding {
    Lf,
    CrLf,
    Cr,
    #[default]
    None,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SerialSendMode {
    #[default]
    Text,
    Hex,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SerialDisplayMode {
    #[default]
    Text,
    Hex,
    Mixed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MoshIpFamily {
    #[default]
    Auto,
    Ipv4,
    Ipv6,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MoshPredictionMode {
    #[default]
    Adaptive,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StandaloneSftpTransferMode {
    #[default]
    LocalRemote,
    RemoteRemote,
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

impl SshChannelStrategy {
    pub fn is_default(value: &Self) -> bool {
        matches!(value, Self::Multiplexed)
    }

    pub fn requires_dedicated_consumers(self) -> bool {
        matches!(self, Self::DedicatedPerConsumer)
    }
}

impl ConnectionTerminalOptions {
    pub fn inherits_application_defaults(&self) -> bool {
        self.encoding.is_none()
            && self.backspace_sequence.is_none()
            && self.delete_sequence.is_none()
            && self.semantic_scheme.is_none()
            && self.highlight_rule_set.is_none()
            && self.session_log_policy == ConnectionTerminalSessionLogPolicy::Inherit
            && self.bell_policy == ConnectionTerminalBellPolicy::Inherit
    }
}

pub const DEFAULT_X11_UNTRUSTED_TIMEOUT_SECONDS: u32 = 20 * 60;
pub const DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS: u64 = 30;
pub const DEFAULT_SSH_KEEP_ALIVE_INTERVAL_SECONDS: u32 = 30;
pub const DEFAULT_SSH_KEEP_ALIVE_COUNT_MAX: u32 = 3;

#[derive(Clone, Debug)]
pub struct ConnectionInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub empty_password: bool,
    pub totp_credential_id: Option<String>,
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
    pub managed_key_id: Option<String>,
    pub gssapi_authentication: bool,
    pub gssapi_server_identity: Option<String>,
    pub gssapi_delegate_credentials: bool,
    pub agent_forwarding: bool,
    pub identity_agent: Option<String>,
    pub agent_forwarding_socket: Option<String>,
    pub legacy_ssh_compatibility: bool,
    pub ssh_algorithms: SshAlgorithmPreferences,
}
#[derive(Clone)]
pub struct ManagedSshKeyInfo {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
}
#[derive(Clone, Debug)]
pub struct SerialPortInfo {
    pub port_path: String,
    pub display_name: String,
}
#[derive(Clone)]
pub struct ShellInfo {
    pub id: String,
    pub label: String,
    pub path: std::path::PathBuf,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SshAlgorithmCategory {
    Kex,
    HostKey,
    Cipher,
    Mac,
    Compression,
}
pub type NodeId = String;

impl SshAlgorithmCategory {
    pub const ALL: [Self; 5] = [
        Self::Kex,
        Self::HostKey,
        Self::Cipher,
        Self::Mac,
        Self::Compression,
    ];
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MoshUdpPortSelection {
    #[default]
    Automatic,
    Fixed {
        port: u16,
    },
    Range {
        start: u16,
        end: u16,
    },
}
