use std::{collections::BTreeMap, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::{
    auth::ToolGroup,
    handles::{
        AddonRef, ArtifactRef, AuditRef, CommandRef, ConnectionRef, DesktopRef, FileSessionRef,
        ForwardRef, NodeRef, OperationRef, QuickCommandRef, RecordingRef, SyncPlanRef, TerminalRef,
        TransferRef, UndoRef, WorkspaceRef,
    },
};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// Requests persistent tool groups without allowing an unattended client to self-grant them.
pub struct RequestAccessArgs {
    pub groups: Vec<ToolGroup>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// Lets a client reduce only its own Public MCP capabilities.
pub struct RevokeAccessArgs {
    pub groups: Vec<ToolGroup>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// Reads a redacted status projection for a client-owned background operation.
pub struct OperationStateArgs {
    pub operation_ref: OperationRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// Requests cancellation without claiming that external side effects were reversed.
pub struct CancelOperationArgs {
    pub operation_ref: OperationRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// Replays only the exact inverse operation retained behind an opaque undo handle.
pub struct RevertArgs {
    pub undo_ref: UndoRef,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct BrowseConnectionsArgs {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub connection_types: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DescribeConnectionArgs {
    pub connection_ref: ConnectionRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum PublicConnectionAuth {
    Password,
    Key {
        key_path: String,
    },
    ManagedKey {
        managed_key_id: String,
    },
    Certificate {
        key_path: String,
        certificate_path: String,
    },
    KeyboardInteractive,
    Agent,
    Gssapi {
        server_identity: Option<String>,
        #[serde(default)]
        delegate_credentials: bool,
    },
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicTerminalEncoding {
    #[default]
    Utf8,
    Gbk,
    Gb18030,
    Big5,
    ShiftJis,
    EucJp,
    EucKr,
    Windows1252,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicTerminalBackspaceSequence {
    Delete,
    ControlH,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicTerminalDeleteSequence {
    Csi3Tilde,
    Delete,
    ControlH,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicTerminalOptions {
    #[serde(default)]
    pub encoding: Option<PublicTerminalEncoding>,
    #[serde(default)]
    pub backspace_sequence: Option<PublicTerminalBackspaceSequence>,
    #[serde(default)]
    pub delete_sequence: Option<PublicTerminalDeleteSequence>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicX11ForwardingMode {
    #[default]
    Untrusted,
    Trusted,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicX11ForwardingOptions {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: PublicX11ForwardingMode,
    #[serde(default)]
    pub untrusted_timeout_seconds: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicUpstreamProxyProtocol {
    Socks5,
    HttpConnect,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, tag = "mode", rename_all = "snake_case")]
pub enum PublicUpstreamProxy {
    #[default]
    UseGlobal,
    Direct,
    Custom {
        protocol: PublicUpstreamProxyProtocol,
        host: String,
        port: u16,
        #[serde(default)]
        username: Option<String>,
        #[serde(default = "default_true")]
        remote_dns: bool,
        #[serde(default)]
        no_proxy: String,
    },
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicProxyHopProfile {
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub username: String,
    pub auth: PublicConnectionAuth,
    #[serde(default)]
    pub agent_forwarding: bool,
    #[serde(default)]
    pub identity_agent: Option<String>,
    #[serde(default)]
    pub agent_forwarding_socket: Option<String>,
    #[serde(default)]
    pub legacy_ssh_compatibility: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicSshProfile {
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub username: String,
    pub auth: PublicConnectionAuth,
    #[serde(default)]
    pub proxy_chain: Vec<PublicProxyHopProfile>,
    #[serde(default)]
    pub upstream_proxy: PublicUpstreamProxy,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon_background_color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub connect_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub agent_forwarding: bool,
    #[serde(default)]
    pub identity_agent: Option<String>,
    #[serde(default)]
    pub agent_forwarding_socket: Option<String>,
    #[serde(default)]
    pub legacy_ssh_compatibility: bool,
    #[serde(default)]
    pub dedicated_new_terminal_connection: bool,
    #[serde(default)]
    pub x11_forwarding: PublicX11ForwardingOptions,
    #[serde(default)]
    pub post_connect_command: Option<String>,
    #[serde(default)]
    pub terminal: PublicTerminalOptions,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicSerialParity {
    None,
    Odd,
    Even,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicSerialFlowControl {
    None,
    Software,
    Hardware,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicSerialProfile {
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub port_path: String,
    #[serde(default)]
    pub baud_rate: Option<u32>,
    #[serde(default)]
    pub data_bits: Option<u8>,
    #[serde(default)]
    pub stop_bits: Option<u8>,
    #[serde(default)]
    pub parity: Option<PublicSerialParity>,
    #[serde(default)]
    pub flow_control: Option<PublicSerialFlowControl>,
    #[serde(default)]
    pub connect_on_open: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon_background_color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicTelnetProfile {
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub host: String,
    #[serde(default = "default_telnet_port")]
    pub port: u16,
    #[serde(default)]
    pub terminal: PublicTerminalOptions,
    #[serde(default)]
    pub connect_on_open: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon_background_color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicMoshIpFamily {
    #[default]
    Auto,
    Ipv4,
    Ipv6,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PublicMoshUdpPortSelection {
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

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicMoshPredictionMode {
    #[default]
    Adaptive,
    Always,
    Never,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicMoshProfile {
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    pub username: String,
    pub auth: PublicConnectionAuth,
    #[serde(default)]
    pub proxy_chain: Vec<PublicProxyHopProfile>,
    #[serde(default = "default_mosh_server")]
    pub server_executable: String,
    #[serde(default)]
    pub udp_host_override: Option<String>,
    #[serde(default)]
    pub udp_port: PublicMoshUdpPortSelection,
    #[serde(default)]
    pub ip_family: PublicMoshIpFamily,
    #[serde(default)]
    pub prediction: PublicMoshPredictionMode,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub identity_agent: Option<String>,
    #[serde(default)]
    pub legacy_ssh_compatibility: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon_background_color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicVncSecurityPolicy {
    #[default]
    RequireVerifiedEncryption,
    AllowUnverifiedEncryption,
    AllowLegacy,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicVncSessionMode {
    #[default]
    Shared,
    Exclusive,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicVncImageQuality {
    Performance,
    #[default]
    Balanced,
    BestQuality,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicVncCompression {
    Low,
    #[default]
    Balanced,
    High,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicRemoteDesktopOptions {
    #[serde(default = "default_true")]
    pub clipboard_text: bool,
    #[serde(default = "default_true")]
    pub clipboard_images: bool,
    #[serde(default)]
    pub clipboard_files: bool,
    #[serde(default = "default_true")]
    pub audio_playback: bool,
    #[serde(default)]
    pub audio_capture: bool,
    #[serde(default)]
    pub use_all_monitors: bool,
    /// Disables the RDP Graphics Pipeline for hosts affected by EGFX rendering corruption.
    #[serde(default)]
    pub disable_rdp_graphics_pipeline: bool,
    #[serde(default)]
    pub vnc_security_policy: PublicVncSecurityPolicy,
    #[serde(default)]
    pub vnc_session_mode: PublicVncSessionMode,
    #[serde(default)]
    pub vnc_image_quality: PublicVncImageQuality,
    #[serde(default)]
    pub vnc_compression: PublicVncCompression,
}

impl Default for PublicRemoteDesktopOptions {
    fn default() -> Self {
        Self {
            clipboard_text: true,
            clipboard_images: true,
            clipboard_files: false,
            audio_playback: true,
            audio_capture: false,
            use_all_monitors: false,
            disable_rdp_graphics_pipeline: false,
            vnc_security_policy: PublicVncSecurityPolicy::default(),
            vnc_session_mode: PublicVncSessionMode::default(),
            vnc_image_quality: PublicVncImageQuality::default(),
            vnc_compression: PublicVncCompression::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicRemoteDesktopProfile {
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub options: PublicRemoteDesktopOptions,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon_background_color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum PublicSavedConnectionProfile {
    Ssh(PublicSshProfile),
    Serial(PublicSerialProfile),
    Telnet(PublicTelnetProfile),
    Mosh(PublicMoshProfile),
    Rdp(PublicRemoteDesktopProfile),
    Vnc(PublicRemoteDesktopProfile),
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SavePublicConnectionArgs {
    #[serde(default)]
    pub connection_ref: Option<ConnectionRef>,
    pub profile: PublicSavedConnectionProfile,
    #[serde(default)]
    pub expected_revision: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemovePublicConnectionArgs {
    pub connection_ref: ConnectionRef,
    #[serde(default)]
    pub forget_credentials: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicCredentialSlot {
    Primary,
    ProxyHop { index: u32 },
    UpstreamProxy,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialStatusArgs {
    pub connection_ref: ConnectionRef,
}

pub struct StoreCredentialArgs {
    pub connection_ref: ConnectionRef,
    pub slot: PublicCredentialSlot,
    pub new_secret: Zeroizing<String>,
}

impl fmt::Debug for StoreCredentialArgs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoreCredentialArgs")
            .field("connection_ref", &self.connection_ref)
            .field("slot", &self.slot)
            .field("new_secret", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgetCredentialArgs {
    pub connection_ref: ConnectionRef,
    pub slot: PublicCredentialSlot,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, JsonSchema, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PublicSyncSection {
    Connections,
    Forwards,
    QuickCommands,
    SerialProfiles,
    TelnetProfiles,
    MoshProfiles,
    RemoteDesktopProfiles,
    SensitiveCredentials,
    AppSettings,
    PluginSettings,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicSyncConflictStrategy {
    #[default]
    Merge,
    Replace,
    Skip,
    Rename,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SyncStatusArgs {}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SyncSelection {
    /// Omit this field to use the app's configured Cloud Sync scope.
    #[serde(default)]
    pub sections: Option<Vec<PublicSyncSection>>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SyncPullPreviewArgs {
    #[serde(default)]
    pub selection: SyncSelection,
    #[serde(default)]
    pub conflict_strategy: PublicSyncConflictStrategy,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SyncPublishPreviewArgs {
    #[serde(default)]
    pub selection: SyncSelection,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SyncApplyPlanArgs {
    pub sync_plan_ref: SyncPlanRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SyncRestoreArgs {
    pub undo_ref: UndoRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConnectNodeArgs {
    pub connection_ref: ConnectionRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct InspectNodeArgs {
    pub node_ref: NodeRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReleaseNodeArgs {
    pub node_ref: NodeRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DisconnectNodeArgs {
    pub node_ref: NodeRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TerminalOpenSource {
    Node { node_ref: NodeRef },
    Connection { connection_ref: ConnectionRef },
    Local,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct OpenTerminalArgs {
    pub source: TerminalOpenSource,
    #[serde(default = "default_terminal_cols")]
    pub cols: u16,
    #[serde(default = "default_terminal_rows")]
    pub rows: u16,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TerminalHandleArgs {
    pub terminal_ref: TerminalRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReadTerminalArgs {
    pub terminal_ref: TerminalRef,
    #[serde(default)]
    pub cursor: Option<u64>,
    #[serde(default = "default_terminal_line_limit")]
    pub line_limit: u32,
    #[serde(default)]
    pub tail: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FindTerminalArgs {
    pub terminal_ref: TerminalRef,
    pub query: String,
    #[serde(default = "default_terminal_match_limit")]
    pub limit: u32,
}

pub struct SubmitTerminalArgs {
    pub terminal_ref: TerminalRef,
    pub input: Zeroizing<Vec<u8>>,
    pub append_enter: bool,
    pub is_text: bool,
}

impl fmt::Debug for SubmitTerminalArgs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubmitTerminalArgs")
            .field("terminal_ref", &self.terminal_ref)
            .field(
                "input",
                &format_args!("[REDACTED; {} bytes]", self.input.len()),
            )
            .field("append_enter", &self.append_enter)
            .field("is_text", &self.is_text)
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ResizeTerminalArgs {
    pub terminal_ref: TerminalRef,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicTelnetControl {
    NoOperation,
    Break,
    InterruptProcess,
    AbortOutput,
    AreYouThere,
    EraseCharacter,
    EraseLine,
    GoAhead,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TerminalControlAction {
    Interrupt,
    Terminate,
    Kill,
    SerialBreak,
    SerialReconnect,
    SerialDataTerminalReady { asserted: bool },
    SerialRequestToSend { asserted: bool },
    Telnet { command: PublicTelnetControl },
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ControlTerminalArgs {
    pub terminal_ref: TerminalRef,
    pub action: TerminalControlAction,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, tag = "action", rename_all = "snake_case")]
pub enum RecordingsControlArgs {
    Start {
        terminal_ref: TerminalRef,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        capture_input: bool,
    },
    Pause {
        recording_ref: RecordingRef,
    },
    Resume {
        recording_ref: RecordingRef,
    },
    Stop {
        recording_ref: RecordingRef,
    },
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum RecordingStatusTarget {
    Recording { recording_ref: RecordingRef },
    Terminal { terminal_ref: TerminalRef },
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingsStatusArgs {
    pub target: RecordingStatusTarget,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingsSearchArgs {
    pub recording_ref: RecordingRef,
    pub query: String,
    #[serde(default = "default_recording_search_limit")]
    pub limit: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingExportFormat {
    AsciicastV2,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingsExportArgs {
    pub recording_ref: RecordingRef,
    pub format: RecordingExportFormat,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenDesktopArgs {
    pub connection_ref: ConnectionRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesktopHandleArgs {
    pub desktop_ref: DesktopRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesktopFrameArgs {
    pub desktop_ref: DesktopRef,
    #[serde(default)]
    pub after_generation: Option<u64>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublicDesktopMouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DesktopButtonState {
    Pressed,
    Released,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum DesktopInputEvent {
    MouseMove {
        x: u32,
        y: u32,
    },
    MouseButton {
        x: u32,
        y: u32,
        button: PublicDesktopMouseButton,
        state: DesktopButtonState,
    },
    Wheel {
        x: u32,
        y: u32,
        delta_x: f32,
        delta_y: f32,
    },
    Key {
        code: String,
        #[serde(default)]
        text: Option<Zeroizing<String>>,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        shift: bool,
        #[serde(default)]
        meta: bool,
        state: DesktopButtonState,
    },
    Text {
        text: Zeroizing<String>,
    },
    ReleaseAll,
}

impl fmt::Debug for DesktopInputEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::MouseMove { .. } => "mouse_move",
            Self::MouseButton { .. } => "mouse_button",
            Self::Wheel { .. } => "wheel",
            Self::Key { .. } => "key",
            Self::Text { .. } => "text",
            Self::ReleaseAll => "release_all",
        };
        formatter
            .debug_struct("DesktopInputEvent")
            .field("kind", &kind)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopInputArgs {
    pub desktop_ref: DesktopRef,
    pub graphics_epoch: u64,
    pub event: DesktopInputEvent,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResizeDesktopArgs {
    pub desktop_ref: DesktopRef,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DesktopClipboardKind {
    Text,
    Image,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadDesktopClipboardArgs {
    pub desktop_ref: DesktopRef,
    #[serde(default = "default_desktop_clipboard_kind")]
    pub kind: DesktopClipboardKind,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DesktopClipboardImageFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Svg,
    Bmp,
    Tiff,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum DesktopClipboardPayload {
    Text {
        text: Zeroizing<String>,
    },
    Image {
        artifact_ref: ArtifactRef,
        format: DesktopClipboardImageFormat,
    },
}

impl fmt::Debug for DesktopClipboardPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text { text } => formatter
                .debug_struct("DesktopClipboardPayload")
                .field("kind", &"text")
                .field("bytes", &text.len())
                .finish(),
            Self::Image {
                artifact_ref,
                format,
            } => formatter
                .debug_struct("DesktopClipboardPayload")
                .field("kind", &"image")
                .field("artifact_ref", artifact_ref)
                .field("format", format)
                .finish(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteDesktopClipboardArgs {
    pub desktop_ref: DesktopRef,
    pub payload: DesktopClipboardPayload,
}

pub struct StartCommandArgs {
    pub node_ref: NodeRef,
    pub command: Zeroizing<String>,
    pub working_directory: Option<Zeroizing<String>>,
}

impl fmt::Debug for StartCommandArgs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartCommandArgs")
            .field("node_ref", &self.node_ref)
            .field("command", &"[REDACTED]")
            .field(
                "working_directory",
                &self.working_directory.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommandStateArgs {
    pub command_ref: CommandRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommandOutputArgs {
    pub command_ref: CommandRef,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_output_limit")]
    pub limit: u32,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CancelCommandArgs {
    pub command_ref: CommandRef,
}

pub struct StageArtifactArgs {
    pub bytes: Zeroizing<Vec<u8>>,
    pub media_type: String,
    pub name: Option<String>,
}

impl fmt::Debug for StageArtifactArgs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StageArtifactArgs")
            .field(
                "bytes",
                &format_args!("[REDACTED; {} bytes]", self.bytes.len()),
            )
            .field("media_type", &self.media_type)
            .field("name", &self.name)
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReadArtifactArgs {
    pub artifact_ref: ArtifactRef,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_artifact_read_length")]
    pub length: u32,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct AuditSearchArgs {
    #[serde(default)]
    pub after_ms: Option<u128>,
    #[serde(default)]
    pub before_ms: Option<u128>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub target_ref: Option<String>,
    #[serde(default)]
    pub cursor: Option<AuditRef>,
    #[serde(default = "default_audit_limit")]
    pub limit: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HostToolResource {
    System,
    Processes,
    Docker,
    Services,
    Logs,
    Tmux,
    Ports,
    Filesystems,
    Packages,
    ScheduledTasks,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HostToolLogPreset {
    #[default]
    All,
    Errors,
    Auth,
    Kernel,
    System,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct HostToolsCatalogArgs {
    pub node_ref: NodeRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct HostToolsCaptureArgs {
    pub node_ref: NodeRef,
    pub resource: HostToolResource,
    #[serde(default)]
    pub log_preset: HostToolLogPreset,
    #[serde(default = "default_host_tool_limit")]
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostToolOperation {
    ProcessStop { pid: String },
    ProcessContinue { pid: String },
    ProcessRenice { pid: String, nice: i32 },
    ProcessTerminate { pid: String },
    ProcessKill { pid: String },
    DockerStart { container_id: String },
    DockerStop { container_id: String },
    DockerRestart { container_id: String },
    ServiceStart { service_id: String },
    ServiceStop { service_id: String },
    ServiceRestart { service_id: String },
    ServiceReload { service_id: String },
    ServiceEnable { service_id: String },
    ServiceDisable { service_id: String },
    TmuxRenameSession { target: String, name: String },
    TmuxRenameWindow { target: String, name: String },
    TmuxKillSession { target: String },
    TmuxKillWindow { target: String },
    TmuxKillPane { target: String },
    ScheduledTaskRun { id: String, unit: String },
    ScheduledTaskEnable { id: String, source: String },
    ScheduledTaskDisable { id: String, source: String },
}

impl HostToolOperation {
    fn target_summary(&self) -> &str {
        match self {
            Self::ProcessStop { pid }
            | Self::ProcessContinue { pid }
            | Self::ProcessRenice { pid, .. }
            | Self::ProcessTerminate { pid }
            | Self::ProcessKill { pid } => pid,
            Self::DockerStart { container_id }
            | Self::DockerStop { container_id }
            | Self::DockerRestart { container_id } => container_id,
            Self::ServiceStart { service_id }
            | Self::ServiceStop { service_id }
            | Self::ServiceRestart { service_id }
            | Self::ServiceReload { service_id }
            | Self::ServiceEnable { service_id }
            | Self::ServiceDisable { service_id } => service_id,
            Self::TmuxRenameSession { target, .. }
            | Self::TmuxRenameWindow { target, .. }
            | Self::TmuxKillSession { target }
            | Self::TmuxKillWindow { target }
            | Self::TmuxKillPane { target } => target,
            Self::ScheduledTaskRun { id, .. }
            | Self::ScheduledTaskEnable { id, .. }
            | Self::ScheduledTaskDisable { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct HostToolsOperateArgs {
    pub node_ref: NodeRef,
    pub operation: HostToolOperation,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct QuickCommandsListArgs {
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct QuickCommandsDescribeArgs {
    pub quickcommand_ref: QuickCommandRef,
}

pub struct QuickCommandsSaveArgs {
    pub quickcommand_ref: Option<QuickCommandRef>,
    pub name: String,
    pub command: Zeroizing<String>,
    pub category: String,
    pub description: Option<String>,
    pub host_pattern: Option<String>,
    pub expected_revision: u64,
}

impl fmt::Debug for QuickCommandsSaveArgs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QuickCommandsSaveArgs")
            .field("quickcommand_ref", &self.quickcommand_ref)
            .field("name", &self.name)
            .field("command", &"[REDACTED]")
            .field("category", &self.category)
            .field("description", &self.description)
            .field("host_pattern", &self.host_pattern)
            .field("expected_revision", &self.expected_revision)
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct QuickCommandsRemoveArgs {
    pub quickcommand_ref: QuickCommandRef,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct QuickCommandsRunArgs {
    pub quickcommand_ref: QuickCommandRef,
    pub node_ref: NodeRef,
    pub expected_revision: u64,
    #[serde(default)]
    pub arguments: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct AddonsListArgs {
    #[serde(default)]
    pub include_disabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AddonsInstallArgs {
    pub artifact_ref: ArtifactRef,
    pub expected_identity: String,
    pub checksum: String,
    #[serde(default)]
    pub replace_existing: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AddonsSetEnabledArgs {
    pub addon_ref: AddonRef,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AddonsRemoveArgs {
    pub addon_ref: AddonRef,
    #[serde(default)]
    pub retain_settings: Option<bool>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardKind {
    Local,
    Remote,
    Dynamic,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ForwardsListArgs {
    #[serde(default)]
    pub node_ref: Option<NodeRef>,
    #[serde(default)]
    pub include_stopped: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ForwardsOpenArgs {
    pub node_ref: NodeRef,
    pub kind: ForwardKind,
    pub bind_address: String,
    pub bind_port: u16,
    #[serde(default)]
    pub target_host: Option<String>,
    #[serde(default)]
    pub target_port: Option<u16>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub persist: bool,
    #[serde(default)]
    pub check_health: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ForwardPatch {
    #[serde(default)]
    pub kind: Option<ForwardKind>,
    #[serde(default)]
    pub bind_address: Option<String>,
    #[serde(default)]
    pub bind_port: Option<u16>,
    #[serde(default)]
    pub target_host: Option<String>,
    #[serde(default)]
    pub target_port: Option<u16>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ForwardsChangeArgs {
    pub forward_ref: ForwardRef,
    pub patch: ForwardPatch,
    pub expected_revision: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ForwardHandleArgs {
    pub forward_ref: ForwardRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ForwardsRemoveArgs {
    pub forward_ref: ForwardRef,
    #[serde(default)]
    pub remove_saved: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ForwardsDiscoverPortsArgs {
    pub node_ref: NodeRef,
}

/// Opens an SFTP capability rooted at one canonical remote directory.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FilesOpenArgs {
    pub node_ref: NodeRef,
    #[serde(default)]
    pub root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FilesCloseArgs {
    pub file_session_ref: FileSessionRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FilesListArgs {
    pub file_session_ref: FileSessionRef,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub cursor: u32,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FilesStatArgs {
    pub file_session_ref: FileSessionRef,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FilesReadArgs {
    pub file_session_ref: FileSessionRef,
    pub path: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default)]
    pub maximum_bytes: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FilesCompareArgs {
    pub file_session_ref: FileSessionRef,
    pub path: String,
    pub artifact_ref: ArtifactRef,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FilesWriteArgs {
    pub file_session_ref: FileSessionRef,
    pub path: String,
    pub artifact_ref: ArtifactRef,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub expected_revision: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FilesMoveArgs {
    pub file_session_ref: FileSessionRef,
    pub source_path: String,
    pub destination_path: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub expected_revision: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FilesRemoveArgs {
    pub file_session_ref: FileSessionRef,
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub expected_revision: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, tag = "direction", rename_all = "snake_case")]
pub enum StartTransferArgs {
    Upload {
        file_session_ref: FileSessionRef,
        remote_path: String,
        artifact_ref: ArtifactRef,
        #[serde(default)]
        overwrite: bool,
        #[serde(default)]
        resume: bool,
    },
    Download {
        file_session_ref: FileSessionRef,
        remote_path: String,
        #[serde(default)]
        resume: bool,
    },
}

impl StartTransferArgs {
    pub fn remote_path(&self) -> &str {
        match self {
            Self::Upload { remote_path, .. } | Self::Download { remote_path, .. } => remote_path,
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransferHandleArgs {
    pub transfer_ref: TransferRef,
}

/// Mounts an IDE workspace beneath an existing authorized SFTP root.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceMountArgs {
    pub file_session_ref: FileSessionRef,
    #[serde(default)]
    pub root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceTreeArgs {
    pub workspace_ref: WorkspaceRef,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub cursor: u32,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceReadArgs {
    pub workspace_ref: WorkspaceRef,
    pub path: String,
}

#[derive(Clone)]
pub struct WorkspaceTextEdit {
    pub start_byte: u32,
    pub end_byte: u32,
    pub replacement: Zeroizing<String>,
}

#[derive(Clone)]
pub struct WorkspaceFileEdits {
    pub path: String,
    pub expected_revision: String,
    pub edits: Vec<WorkspaceTextEdit>,
}

#[derive(Clone)]
pub struct WorkspaceApplyEditsArgs {
    pub workspace_ref: WorkspaceRef,
    pub files: Vec<WorkspaceFileEdits>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSearchArgs {
    pub workspace_ref: WorkspaceRef,
    pub pattern: String,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub maximum_results: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceCloseArgs {
    pub workspace_ref: WorkspaceRef,
}

fn default_output_limit() -> u32 {
    64 * 1024
}

fn default_artifact_read_length() -> u32 {
    256 * 1024
}

fn default_audit_limit() -> u32 {
    50
}

fn default_host_tool_limit() -> u32 {
    200
}

fn sync_selection_summary(selection: &SyncSelection) -> String {
    selection.sections.as_ref().map_or_else(
        || "configured".to_owned(),
        |sections| format!("{} selected", sections.len()),
    )
}

pub enum PublicToolCall {
    RequestAccess(RequestAccessArgs),
    RevokeAccess(RevokeAccessArgs),
    OperationState(OperationStateArgs),
    CancelOperation(CancelOperationArgs),
    Revert(RevertArgs),
    BrowseConnections(BrowseConnectionsArgs),
    DescribeConnection(DescribeConnectionArgs),
    SaveConnection(Box<SavePublicConnectionArgs>),
    RemoveConnection(RemovePublicConnectionArgs),
    CredentialStatus(CredentialStatusArgs),
    StoreCredential(StoreCredentialArgs),
    ForgetCredential(ForgetCredentialArgs),
    SyncStatus(SyncStatusArgs),
    SyncPullPreview(SyncPullPreviewArgs),
    SyncPublishPreview(SyncPublishPreviewArgs),
    SyncApplyPlan(SyncApplyPlanArgs),
    SyncRestore(SyncRestoreArgs),
    ConnectNode(ConnectNodeArgs),
    InspectNode(InspectNodeArgs),
    ReleaseNode(ReleaseNodeArgs),
    DisconnectNode(DisconnectNodeArgs),
    OpenTerminal(OpenTerminalArgs),
    TerminalState(TerminalHandleArgs),
    ReadTerminal(ReadTerminalArgs),
    FindTerminal(FindTerminalArgs),
    SubmitTerminal(SubmitTerminalArgs),
    ResizeTerminal(ResizeTerminalArgs),
    ControlTerminal(ControlTerminalArgs),
    CloseTerminal(TerminalHandleArgs),
    RecordingsControl(RecordingsControlArgs),
    RecordingsStatus(RecordingsStatusArgs),
    RecordingsSearch(RecordingsSearchArgs),
    RecordingsExport(RecordingsExportArgs),
    OpenDesktop(OpenDesktopArgs),
    DesktopState(DesktopHandleArgs),
    DesktopFrame(DesktopFrameArgs),
    DesktopInput(DesktopInputArgs),
    ResizeDesktop(ResizeDesktopArgs),
    ReadDesktopClipboard(ReadDesktopClipboardArgs),
    WriteDesktopClipboard(WriteDesktopClipboardArgs),
    ReconnectDesktop(DesktopHandleArgs),
    CloseDesktop(DesktopHandleArgs),
    StartCommand(StartCommandArgs),
    CommandState(CommandStateArgs),
    CommandOutput(CommandOutputArgs),
    CancelCommand(CancelCommandArgs),
    StageArtifact(StageArtifactArgs),
    ReadArtifact(ReadArtifactArgs),
    AuditSearch(AuditSearchArgs),
    HostToolsCatalog(HostToolsCatalogArgs),
    HostToolsCapture(HostToolsCaptureArgs),
    HostToolsOperate(Box<HostToolsOperateArgs>),
    QuickCommandsList(QuickCommandsListArgs),
    QuickCommandsDescribe(QuickCommandsDescribeArgs),
    QuickCommandsSave(Box<QuickCommandsSaveArgs>),
    QuickCommandsRemove(QuickCommandsRemoveArgs),
    QuickCommandsRun(QuickCommandsRunArgs),
    AddonsList(AddonsListArgs),
    AddonsInstall(AddonsInstallArgs),
    AddonsSetEnabled(AddonsSetEnabledArgs),
    AddonsRemove(AddonsRemoveArgs),
    ForwardsList(ForwardsListArgs),
    ForwardsOpen(ForwardsOpenArgs),
    ForwardsChange(ForwardsChangeArgs),
    ForwardsStop(ForwardHandleArgs),
    ForwardsRestart(ForwardHandleArgs),
    ForwardsRemove(ForwardsRemoveArgs),
    ForwardsMetrics(ForwardHandleArgs),
    ForwardsDiscoverPorts(ForwardsDiscoverPortsArgs),
    FilesOpen(FilesOpenArgs),
    FilesClose(FilesCloseArgs),
    FilesList(FilesListArgs),
    FilesStat(FilesStatArgs),
    FilesRead(FilesReadArgs),
    FilesCompare(FilesCompareArgs),
    FilesWrite(FilesWriteArgs),
    FilesMove(FilesMoveArgs),
    FilesRemove(FilesRemoveArgs),
    TransferStart(StartTransferArgs),
    TransferStatus(TransferHandleArgs),
    TransferCancel(TransferHandleArgs),
    WorkspaceMount(WorkspaceMountArgs),
    WorkspaceTree(WorkspaceTreeArgs),
    WorkspaceRead(WorkspaceReadArgs),
    WorkspaceApplyEdits(WorkspaceApplyEditsArgs),
    WorkspaceSearch(WorkspaceSearchArgs),
    WorkspaceClose(WorkspaceCloseArgs),
}

impl PublicToolCall {
    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::RequestAccess(_) => "mcp_request_access",
            Self::RevokeAccess(_) => "mcp_revoke_access",
            Self::OperationState(_) => "mcp_operation",
            Self::CancelOperation(_) => "mcp_cancel_operation",
            Self::Revert(_) => "mcp_revert",
            Self::BrowseConnections(_) => "connections_browse",
            Self::DescribeConnection(_) => "connections_describe",
            Self::SaveConnection(_) => "connections_save",
            Self::RemoveConnection(_) => "connections_remove",
            Self::CredentialStatus(_) => "credentials_status",
            Self::StoreCredential(_) => "credentials_store",
            Self::ForgetCredential(_) => "credentials_forget",
            Self::SyncStatus(_) => "sync_status",
            Self::SyncPullPreview(_) => "sync_pull_preview",
            Self::SyncPublishPreview(_) => "sync_publish_preview",
            Self::SyncApplyPlan(_) => "sync_apply_plan",
            Self::SyncRestore(_) => "sync_restore",
            Self::ConnectNode(_) => "nodes_connect",
            Self::InspectNode(_) => "nodes_inspect",
            Self::ReleaseNode(_) => "nodes_release",
            Self::DisconnectNode(_) => "nodes_disconnect",
            Self::OpenTerminal(_) => "terminals_open",
            Self::TerminalState(_) => "terminals_state",
            Self::ReadTerminal(_) => "terminals_read",
            Self::FindTerminal(_) => "terminals_find",
            Self::SubmitTerminal(_) => "terminals_submit",
            Self::ResizeTerminal(_) => "terminals_resize",
            Self::ControlTerminal(_) => "terminals_control",
            Self::CloseTerminal(_) => "terminals_close",
            Self::RecordingsControl(_) => "recordings_control",
            Self::RecordingsStatus(_) => "recordings_status",
            Self::RecordingsSearch(_) => "recordings_search",
            Self::RecordingsExport(_) => "recordings_export",
            Self::OpenDesktop(_) => "desktops_open",
            Self::DesktopState(_) => "desktops_state",
            Self::DesktopFrame(_) => "desktops_frame",
            Self::DesktopInput(_) => "desktops_input",
            Self::ResizeDesktop(_) => "desktops_resize",
            Self::ReadDesktopClipboard(_) => "desktops_clipboard_read",
            Self::WriteDesktopClipboard(_) => "desktops_clipboard_write",
            Self::ReconnectDesktop(_) => "desktops_reconnect",
            Self::CloseDesktop(_) => "desktops_close",
            Self::StartCommand(_) => "commands_start",
            Self::CommandState(_) => "commands_state",
            Self::CommandOutput(_) => "commands_output",
            Self::CancelCommand(_) => "commands_cancel",
            Self::StageArtifact(_) => "artifacts_stage",
            Self::ReadArtifact(_) => "artifacts_read",
            Self::AuditSearch(_) => "mcp_audit_search",
            Self::HostToolsCatalog(_) => "hosttools_catalog",
            Self::HostToolsCapture(_) => "hosttools_capture",
            Self::HostToolsOperate(_) => "hosttools_operate",
            Self::QuickCommandsList(_) => "quickcommands_list",
            Self::QuickCommandsDescribe(_) => "quickcommands_describe",
            Self::QuickCommandsSave(_) => "quickcommands_save",
            Self::QuickCommandsRemove(_) => "quickcommands_remove",
            Self::QuickCommandsRun(_) => "quickcommands_run",
            Self::AddonsList(_) => "addons_list",
            Self::AddonsInstall(_) => "addons_install",
            Self::AddonsSetEnabled(_) => "addons_set_enabled",
            Self::AddonsRemove(_) => "addons_remove",
            Self::ForwardsList(_) => "forwards_list",
            Self::ForwardsOpen(_) => "forwards_open",
            Self::ForwardsChange(_) => "forwards_change",
            Self::ForwardsStop(_) => "forwards_stop",
            Self::ForwardsRestart(_) => "forwards_restart",
            Self::ForwardsRemove(_) => "forwards_remove",
            Self::ForwardsMetrics(_) => "forwards_metrics",
            Self::ForwardsDiscoverPorts(_) => "forwards_discover_ports",
            Self::FilesOpen(_) => "files_open",
            Self::FilesClose(_) => "files_close",
            Self::FilesList(_) => "files_list",
            Self::FilesStat(_) => "files_stat",
            Self::FilesRead(_) => "files_read",
            Self::FilesCompare(_) => "files_compare",
            Self::FilesWrite(_) => "files_write",
            Self::FilesMove(_) => "files_move",
            Self::FilesRemove(_) => "files_remove",
            Self::TransferStart(_) => "transfers_start",
            Self::TransferStatus(_) => "transfers_status",
            Self::TransferCancel(_) => "transfers_cancel",
            Self::WorkspaceMount(_) => "workspaces_mount",
            Self::WorkspaceTree(_) => "workspaces_tree",
            Self::WorkspaceRead(_) => "workspaces_read",
            Self::WorkspaceApplyEdits(_) => "workspaces_apply_edits",
            Self::WorkspaceSearch(_) => "workspaces_search",
            Self::WorkspaceClose(_) => "workspaces_close",
        }
    }

    pub fn required_group(&self) -> ToolGroup {
        match self {
            Self::RequestAccess(_)
            | Self::RevokeAccess(_)
            | Self::OperationState(_)
            | Self::CancelOperation(_)
            | Self::Revert(_) => ToolGroup::Basic,
            Self::BrowseConnections(_) => ToolGroup::ConnectionDirectory,
            Self::DescribeConnection(_) => ToolGroup::ConnectionRead,
            Self::SaveConnection(_) | Self::RemoveConnection(_) => ToolGroup::ConnectionManage,
            Self::CredentialStatus(_) | Self::StoreCredential(_) | Self::ForgetCredential(_) => {
                ToolGroup::CredentialManage
            }
            Self::SyncStatus(_)
            | Self::SyncPullPreview(_)
            | Self::SyncPublishPreview(_)
            | Self::SyncApplyPlan(_)
            | Self::SyncRestore(_) => ToolGroup::CloudSync,
            Self::ConnectNode(_)
            | Self::InspectNode(_)
            | Self::ReleaseNode(_)
            | Self::DisconnectNode(_) => ToolGroup::NodeSession,
            Self::OpenTerminal(_) | Self::ResizeTerminal(_) | Self::CloseTerminal(_) => {
                ToolGroup::TerminalSession
            }
            Self::TerminalState(_) | Self::ReadTerminal(_) | Self::FindTerminal(_) => {
                ToolGroup::TerminalObserve
            }
            Self::SubmitTerminal(_) | Self::ControlTerminal(_) => ToolGroup::TerminalInput,
            Self::RecordingsControl(_) | Self::RecordingsStatus(_) => ToolGroup::RecordingControl,
            Self::RecordingsSearch(_) | Self::RecordingsExport(_) => ToolGroup::RecordingContent,
            Self::OpenDesktop(_) | Self::ReconnectDesktop(_) | Self::CloseDesktop(_) => {
                ToolGroup::DesktopSession
            }
            Self::DesktopState(_) | Self::DesktopFrame(_) => ToolGroup::DesktopObserve,
            Self::DesktopInput(_) | Self::ResizeDesktop(_) => ToolGroup::DesktopInput,
            Self::ReadDesktopClipboard(_) | Self::WriteDesktopClipboard(_) => {
                ToolGroup::DesktopClipboard
            }
            Self::StartCommand(_) | Self::CancelCommand(_) => ToolGroup::CommandExecute,
            Self::CommandState(_) | Self::CommandOutput(_) => ToolGroup::CommandObserve,
            Self::StageArtifact(_) | Self::ReadArtifact(_) => ToolGroup::ArtifactTransfer,
            Self::AuditSearch(_) => ToolGroup::AuditRead,
            Self::HostToolsCatalog(_) | Self::HostToolsCapture(_) => ToolGroup::HostToolsObserve,
            Self::HostToolsOperate(_) => ToolGroup::HostToolsOperate,
            Self::QuickCommandsList(_) => ToolGroup::QuickCommandRead,
            Self::QuickCommandsDescribe(_) => ToolGroup::QuickCommandContentRead,
            Self::QuickCommandsSave(_) | Self::QuickCommandsRemove(_) => {
                ToolGroup::QuickCommandManage
            }
            Self::QuickCommandsRun(_) => ToolGroup::QuickCommandExecute,
            Self::AddonsList(_) => ToolGroup::AddonRead,
            Self::AddonsInstall(_) | Self::AddonsSetEnabled(_) | Self::AddonsRemove(_) => {
                ToolGroup::AddonManage
            }
            Self::ForwardsList(_) | Self::ForwardsMetrics(_) | Self::ForwardsDiscoverPorts(_) => {
                ToolGroup::ForwardRead
            }
            Self::ForwardsOpen(_)
            | Self::ForwardsChange(_)
            | Self::ForwardsStop(_)
            | Self::ForwardsRestart(_)
            | Self::ForwardsRemove(_) => ToolGroup::ForwardManage,
            Self::FilesOpen(_)
            | Self::FilesClose(_)
            | Self::FilesList(_)
            | Self::FilesStat(_)
            | Self::FilesRead(_)
            | Self::FilesCompare(_) => ToolGroup::FileRead,
            Self::FilesWrite(_) | Self::FilesMove(_) | Self::FilesRemove(_) => ToolGroup::FileWrite,
            Self::TransferStart(_) | Self::TransferStatus(_) | Self::TransferCancel(_) => {
                ToolGroup::ArtifactTransfer
            }
            Self::WorkspaceMount(_)
            | Self::WorkspaceTree(_)
            | Self::WorkspaceRead(_)
            | Self::WorkspaceSearch(_)
            | Self::WorkspaceClose(_) => ToolGroup::WorkspaceRead,
            Self::WorkspaceApplyEdits(_) => ToolGroup::WorkspaceEdit,
        }
    }

    pub fn additional_required_groups(&self) -> &'static [ToolGroup] {
        match self {
            Self::RecordingsExport(_)
            | Self::DesktopFrame(_)
            | Self::AddonsInstall(_)
            | Self::FilesRead(_)
            | Self::FilesCompare(_)
            | Self::FilesWrite(_) => &[ToolGroup::ArtifactTransfer],
            Self::ReadDesktopClipboard(args)
                if matches!(args.kind, DesktopClipboardKind::Image) =>
            {
                &[ToolGroup::ArtifactTransfer]
            }
            Self::WriteDesktopClipboard(args)
                if matches!(args.payload, DesktopClipboardPayload::Image { .. }) =>
            {
                &[ToolGroup::ArtifactTransfer]
            }
            Self::TransferStart(StartTransferArgs::Upload { .. }) => &[ToolGroup::FileWrite],
            Self::TransferStart(StartTransferArgs::Download { .. }) => &[ToolGroup::FileRead],
            Self::WorkspaceMount(_) => &[ToolGroup::FileRead],
            Self::WorkspaceApplyEdits(_) => &[ToolGroup::FileWrite],
            Self::Revert(_) => &[ToolGroup::CloudSync],
            _ => &[],
        }
    }

    pub fn requires_approval(&self) -> bool {
        matches!(
            self,
            Self::RequestAccess(_)
                | Self::Revert(_)
                | Self::SaveConnection(_)
                | Self::RemoveConnection(_)
                | Self::StoreCredential(_)
                | Self::ForgetCredential(_)
                | Self::SyncApplyPlan(_)
                | Self::SyncRestore(_)
                | Self::ConnectNode(_)
                | Self::DisconnectNode(_)
                | Self::OpenTerminal(_)
                | Self::SubmitTerminal(_)
                | Self::ControlTerminal(_)
                | Self::RecordingsControl(RecordingsControlArgs::Start { .. })
                | Self::RecordingsExport(_)
                | Self::OpenDesktop(_)
                | Self::DesktopInput(_)
                | Self::WriteDesktopClipboard(_)
                | Self::ReconnectDesktop(_)
                | Self::StartCommand(_)
                | Self::HostToolsOperate(_)
                | Self::QuickCommandsSave(_)
                | Self::QuickCommandsRemove(_)
                | Self::QuickCommandsRun(_)
                | Self::AddonsInstall(_)
                | Self::AddonsSetEnabled(_)
                | Self::AddonsRemove(_)
                | Self::ForwardsOpen(_)
                | Self::ForwardsChange(_)
                | Self::ForwardsStop(_)
                | Self::ForwardsRestart(_)
                | Self::ForwardsRemove(_)
                | Self::FilesWrite(_)
                | Self::FilesMove(_)
                | Self::FilesRemove(_)
                | Self::TransferStart(StartTransferArgs::Upload { .. })
                | Self::WorkspaceApplyEdits(_)
        )
    }

    pub fn requires_explicit_app_approval(&self) -> bool {
        matches!(self, Self::RequestAccess(_))
    }

    pub fn target_summary(&self) -> String {
        match self {
            Self::RequestAccess(args) => {
                let groups = args
                    .groups
                    .iter()
                    .map(|group| group.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("enable tool groups: {groups}")
            }
            Self::RevokeAccess(args) => format!(
                "disable tool groups: {}",
                args.groups
                    .iter()
                    .map(|group| group.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::OperationState(args) => args.operation_ref.to_string(),
            Self::CancelOperation(args) => args.operation_ref.to_string(),
            Self::Revert(args) => args.undo_ref.to_string(),
            Self::BrowseConnections(_) => "connection directory".to_owned(),
            Self::DescribeConnection(args) => args.connection_ref.to_string(),
            Self::SaveConnection(args) => args
                .connection_ref
                .as_ref()
                .map_or_else(|| "new saved connection".to_owned(), ToString::to_string),
            Self::RemoveConnection(args) => format!(
                "{} remove forget_credentials={}",
                args.connection_ref, args.forget_credentials
            ),
            Self::CredentialStatus(args) => args.connection_ref.to_string(),
            Self::StoreCredential(args) => {
                format!("{} {:?}", args.connection_ref, args.slot)
            }
            Self::ForgetCredential(args) => {
                format!("{} {:?}", args.connection_ref, args.slot)
            }
            Self::SyncStatus(_) => "cloud sync state".to_owned(),
            Self::SyncPullPreview(args) => format!(
                "cloud sync pull preview sections={}",
                sync_selection_summary(&args.selection)
            ),
            Self::SyncPublishPreview(args) => format!(
                "cloud sync publish preview sections={} force={}",
                sync_selection_summary(&args.selection),
                args.force
            ),
            Self::SyncApplyPlan(args) => args.sync_plan_ref.to_string(),
            Self::SyncRestore(args) => args.undo_ref.to_string(),
            Self::ConnectNode(args) => args.connection_ref.to_string(),
            Self::InspectNode(args) => args.node_ref.to_string(),
            Self::ReleaseNode(args) => args.node_ref.to_string(),
            Self::DisconnectNode(args) => args.node_ref.to_string(),
            Self::OpenTerminal(args) => match &args.source {
                TerminalOpenSource::Node { node_ref } => node_ref.to_string(),
                TerminalOpenSource::Connection { connection_ref } => connection_ref.to_string(),
                TerminalOpenSource::Local => "local terminal".to_owned(),
            },
            Self::TerminalState(args) | Self::CloseTerminal(args) => args.terminal_ref.to_string(),
            Self::ReadTerminal(args) => args.terminal_ref.to_string(),
            Self::FindTerminal(args) => args.terminal_ref.to_string(),
            Self::SubmitTerminal(args) => format!(
                "{} {} {} bytes append_enter={}",
                args.terminal_ref,
                if args.is_text { "text" } else { "binary" },
                args.input.len(),
                args.append_enter
            ),
            Self::ResizeTerminal(args) => {
                format!("{} {}x{}", args.terminal_ref, args.cols, args.rows)
            }
            Self::ControlTerminal(args) => format!("{} {:?}", args.terminal_ref, args.action),
            Self::RecordingsControl(args) => match args {
                RecordingsControlArgs::Start { terminal_ref, .. } => {
                    format!("{terminal_ref} start recording")
                }
                RecordingsControlArgs::Pause { recording_ref } => {
                    format!("{recording_ref} pause")
                }
                RecordingsControlArgs::Resume { recording_ref } => {
                    format!("{recording_ref} resume")
                }
                RecordingsControlArgs::Stop { recording_ref } => {
                    format!("{recording_ref} stop")
                }
            },
            Self::RecordingsStatus(args) => match &args.target {
                RecordingStatusTarget::Recording { recording_ref } => recording_ref.to_string(),
                RecordingStatusTarget::Terminal { terminal_ref } => terminal_ref.to_string(),
            },
            Self::RecordingsSearch(args) => args.recording_ref.to_string(),
            Self::RecordingsExport(args) => args.recording_ref.to_string(),
            Self::OpenDesktop(args) => args.connection_ref.to_string(),
            Self::DesktopState(args) | Self::ReconnectDesktop(args) | Self::CloseDesktop(args) => {
                args.desktop_ref.to_string()
            }
            Self::DesktopFrame(args) => args.desktop_ref.to_string(),
            Self::DesktopInput(args) => {
                format!("{} {:?}", args.desktop_ref, args.event)
            }
            Self::ResizeDesktop(args) => {
                format!("{} {}x{}", args.desktop_ref, args.width, args.height)
            }
            Self::ReadDesktopClipboard(args) => {
                format!("{} {:?}", args.desktop_ref, args.kind)
            }
            Self::WriteDesktopClipboard(args) => {
                format!("{} {:?}", args.desktop_ref, args.payload)
            }
            Self::StartCommand(args) => args.node_ref.to_string(),
            Self::CommandState(args) => args.command_ref.to_string(),
            Self::CommandOutput(args) => args.command_ref.to_string(),
            Self::CancelCommand(args) => args.command_ref.to_string(),
            Self::StageArtifact(_) => "artifact staging".to_owned(),
            Self::ReadArtifact(args) => args.artifact_ref.to_string(),
            Self::AuditSearch(_) => "audit log".to_owned(),
            Self::HostToolsCatalog(args) => args.node_ref.to_string(),
            Self::HostToolsCapture(args) => args.node_ref.to_string(),
            Self::HostToolsOperate(args) => format!(
                "{} host tool target {}",
                args.node_ref,
                args.operation.target_summary()
            ),
            Self::QuickCommandsList(_) => "quick command catalog".to_owned(),
            Self::QuickCommandsDescribe(args) => args.quickcommand_ref.to_string(),
            Self::QuickCommandsSave(args) => args
                .quickcommand_ref
                .as_ref()
                .map_or_else(|| "new quick command".to_owned(), ToString::to_string),
            Self::QuickCommandsRemove(args) => args.quickcommand_ref.to_string(),
            Self::QuickCommandsRun(args) => {
                format!("{} on {}", args.quickcommand_ref, args.node_ref)
            }
            Self::AddonsList(_) => "addon catalog".to_owned(),
            Self::AddonsInstall(args) => format!(
                "{} sha256:{} replace_existing={}",
                args.expected_identity,
                args.checksum
                    .strip_prefix("sha256:")
                    .unwrap_or(&args.checksum),
                args.replace_existing
            ),
            Self::AddonsSetEnabled(args) => format!(
                "{} {}",
                args.addon_ref,
                if args.enabled { "enable" } else { "disable" }
            ),
            Self::AddonsRemove(args) => format!(
                "{} remove retain_settings={}",
                args.addon_ref,
                args.retain_settings.unwrap_or(true)
            ),
            Self::ForwardsList(args) => args
                .node_ref
                .as_ref()
                .map_or_else(|| "forward catalog".to_owned(), ToString::to_string),
            Self::ForwardsOpen(args) => format!(
                "{} {:?} {}:{} -> {}:{} persist={}",
                args.node_ref,
                args.kind,
                args.bind_address,
                args.bind_port,
                args.target_host.as_deref().unwrap_or("dynamic"),
                args.target_port.unwrap_or(0),
                args.persist
            ),
            Self::ForwardsChange(args) => args.forward_ref.to_string(),
            Self::ForwardsStop(args)
            | Self::ForwardsRestart(args)
            | Self::ForwardsMetrics(args) => args.forward_ref.to_string(),
            Self::ForwardsRemove(args) => {
                format!("{} remove_saved={}", args.forward_ref, args.remove_saved)
            }
            Self::ForwardsDiscoverPorts(args) => args.node_ref.to_string(),
            Self::FilesOpen(args) => format!(
                "{} root={}",
                args.node_ref,
                args.root.as_deref().unwrap_or("home")
            ),
            Self::FilesClose(args) => args.file_session_ref.to_string(),
            Self::FilesList(args) => format!(
                "{} {}",
                args.file_session_ref,
                args.path.as_deref().unwrap_or(".")
            ),
            Self::FilesStat(args) => format!("{} {}", args.file_session_ref, args.path),
            Self::FilesRead(args) => format!("{} {}", args.file_session_ref, args.path),
            Self::FilesCompare(args) => format!("{} {}", args.file_session_ref, args.path),
            Self::FilesWrite(args) => format!(
                "{} {} overwrite={}",
                args.file_session_ref, args.path, args.overwrite
            ),
            Self::FilesMove(args) => format!(
                "{} {} -> {} overwrite={}",
                args.file_session_ref, args.source_path, args.destination_path, args.overwrite
            ),
            Self::FilesRemove(args) => format!(
                "{} {} recursive={}",
                args.file_session_ref, args.path, args.recursive
            ),
            Self::WorkspaceMount(args) => format!(
                "{} root={}",
                args.file_session_ref,
                args.root.as_deref().unwrap_or(".")
            ),
            Self::WorkspaceTree(args) => format!(
                "{} {}",
                args.workspace_ref,
                args.path.as_deref().unwrap_or(".")
            ),
            Self::WorkspaceRead(args) => format!("{} {}", args.workspace_ref, args.path),
            Self::WorkspaceApplyEdits(args) => format!(
                "{} {}",
                args.workspace_ref,
                args.files
                    .iter()
                    .map(|file| format!(
                        "{} ({} edits, {} replacement bytes)",
                        file.path,
                        file.edits.len(),
                        file.edits
                            .iter()
                            .map(|edit| edit.replacement.len())
                            .sum::<usize>()
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::WorkspaceSearch(args) => {
                format!("{} search {} bytes", args.workspace_ref, args.pattern.len())
            }
            Self::WorkspaceClose(args) => args.workspace_ref.to_string(),
            Self::TransferStart(args) => match args {
                StartTransferArgs::Upload {
                    file_session_ref,
                    remote_path,
                    artifact_ref,
                    overwrite,
                    resume,
                } => format!(
                    "{file_session_ref} upload {artifact_ref} to {remote_path} overwrite={overwrite} resume={resume}"
                ),
                StartTransferArgs::Download {
                    file_session_ref,
                    remote_path,
                    resume,
                } => format!("{file_session_ref} download {remote_path} resume={resume}"),
            },
            Self::TransferStatus(args) | Self::TransferCancel(args) => {
                args.transfer_ref.to_string()
            }
        }
    }
}

const fn default_terminal_cols() -> u16 {
    80
}

const fn default_true() -> bool {
    true
}

const fn default_ssh_port() -> u16 {
    22
}

const fn default_telnet_port() -> u16 {
    23
}

fn default_mosh_server() -> String {
    "mosh-server".to_owned()
}

const fn default_terminal_rows() -> u16 {
    24
}

const fn default_terminal_line_limit() -> u32 {
    200
}

const fn default_terminal_match_limit() -> u32 {
    100
}

const fn default_recording_search_limit() -> u32 {
    50
}

const fn default_desktop_clipboard_kind() -> DesktopClipboardKind {
    DesktopClipboardKind::Text
}

impl fmt::Debug for PublicToolCall {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PublicToolCall")
            .field("tool", &self.tool_name())
            .field("target", &self.target_summary())
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    Completed,
    Accepted,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolEnvelope {
    pub outcome: ToolOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolEnvelope {
    pub fn completed(data: impl Serialize) -> Result<Self, serde_json::Error> {
        Ok(Self {
            outcome: ToolOutcome::Completed,
            data: Some(serde_json::to_value(data)?),
            error: None,
        })
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            outcome: ToolOutcome::Failed,
            data: None,
            error: Some(error.into()),
        }
    }
}
