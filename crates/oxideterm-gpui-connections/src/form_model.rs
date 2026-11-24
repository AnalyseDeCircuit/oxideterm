//! Values consumed by the shared form, independent of a transport runtime.
#[cfg(target_family = "wasm")]
#[path = "form_state/choices.rs"]
mod choices;
#[cfg(target_family = "wasm")]
pub use choices::*;
#[cfg(not(target_family = "wasm"))]
pub use oxideterm_connections::{
    AuthType, ConnectionInfo, ConnectionTerminalBackspaceSequence, ConnectionTerminalBellPolicy,
    ConnectionTerminalDeleteSequence, ConnectionTerminalEncoding, ConnectionTerminalOptions,
    ConnectionTerminalSessionLogPolicy, ConnectionTransport, ConnectionX11ForwardingMode,
    ConnectionX11ForwardingOptions, DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS, ManagedSshKeyInfo,
    MoshIpFamily, MoshPredictionMode, MoshUdpPortSelection, RDP_DEFAULT_PORT_TEXT,
    SSH_DEFAULT_PORT_TEXT, SavedUpstreamProxyProtocol, SerialDisplayMode, SerialLineEnding,
    SerialSendMode, SshAlgorithmPreferences, SshChannelStrategy, StandaloneSftpTransferMode,
    TELNET_DEFAULT_PORT_TEXT, TransportUsernameTransition, VNC_DEFAULT_PORT_TEXT,
    transport_is_persistable, transport_port_replacement, transport_username_transition,
};
#[cfg(not(target_family = "wasm"))]
pub use oxideterm_ssh::{NodeId, SshAlgorithmCategory};
#[cfg(not(target_family = "wasm"))]
pub use oxideterm_terminal::{SerialFlowControl, SerialParity, SerialPortInfo, ShellInfo};
