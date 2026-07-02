// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SSH domain model for native OxideTerm.
//!
//! This crate mirrors the Tauri SSH architecture at the model boundary:
//! connection configs, a reference-counted connection registry, node routing,
//! and reconnect orchestration. The actual russh PTY transport plugs into this
//! crate without leaking SSH state into GPUI views.

mod capability;
mod config;
mod connection_registry;
mod host_key;
mod monitor;
mod reconnect;
mod router;
mod transport;
mod upstream_proxy;

pub use capability::{
    SshAlgorithmOffer, SshCapabilityLayer, SshCapabilityLimitation, SshCapabilityReport,
    SshCapabilityStatus, SshIntegrationCapabilities, ssh_capability_report,
};
pub use config::{AuthMethod, ProxyHopConfig, SshConfig};
pub use connection_registry::{
    AcquiredSftpMeta, ConnectionConsumer, ConnectionInfo, ConnectionPoolConfig,
    ConnectionPoolStats, ConnectionState, ConnectionTransportStatus, HEARTBEAT_FAIL_THRESHOLD,
    HEARTBEAT_INTERVAL, KeepaliveProbeResult, ProbeConnectionStatus, RemoteEnvInfo,
    SftpSessionState, SshConnectionHandle, SshConnectionRegistry, WS_BRIDGE_HEARTBEAT_INTERVAL,
    WS_BRIDGE_HEARTBEAT_TIMEOUT,
};
pub use host_key::{
    HostKeyStatus, check_host_key, check_host_key_with_upstream_proxy, remove_host_key,
};
pub use oxideterm_connection_monitor::ConnectionPoolMonitorStats;
pub use oxideterm_sftp::{
    DEFAULT_SFTP_CONCURRENT_TRANSFERS, DEFAULT_SFTP_DIRECTORY_PARALLELISM, FileInfo, FileType,
    ListFilter, MAX_SFTP_CONCURRENT_TRANSFERS, MAX_SFTP_DIRECTORY_PARALLELISM, SftpError,
    SftpSession, SftpTransferManager, SftpTransferPermit, SftpTransferRuntimeSettings, SortOrder,
    TransferDirection, TransferProgress, TransferState,
};
pub use reconnect::{
    MAX_RETAINED_RECONNECT_JOBS, PhaseEvent, PhaseResult, ReconnectForwardRule,
    ReconnectForwardRuleSnapshot, ReconnectIdeSnapshot, ReconnectJob,
    ReconnectNodeConnectionSnapshot, ReconnectNodeTerminalSnapshot, ReconnectNodeTransferSnapshot,
    ReconnectOrchestratorStore, ReconnectPhase, ReconnectSnapshot, ReconnectTiming,
};
pub use router::{
    FlatNode, NodeEventEmitter, NodeEventSequencer, NodeId, NodeOrigin, NodeReadiness, NodeRouter,
    NodeRuntimeStore, NodeState, NodeStateEvent, NodeStateSnapshot, NodeTreeExpansion,
    NodeTreeSnapshot, NodeTreeSnapshotNode, ResolvedConnection, RouteError, SessionTreeSummary,
    TerminalEndpoint,
};
pub use transport::{
    BoxedSshForwardStream, KeyboardInteractivePrompt, KeyboardInteractivePromptRequest,
    KeyboardInteractiveResponses, ManagedKeyResolver, RemoteForwardHandler, RemoteForwardedTcpIp,
    SshCommandOutput, SshForwardStream, SshPromptError, SshPromptHandler, SshPtyHandle,
    SshShellChannel, SshTransportClient, SshTransportCommand, SshTransportError, X11ForwardHandler,
    X11ForwardedChannel,
};
pub use upstream_proxy::{
    UpstreamProxyAuth, UpstreamProxyConfig, UpstreamProxyError, UpstreamProxyProtocol,
    dial_initial_tcp, parse_http_proxy_value, parse_socks5_proxy_value, socks5_proxy_from_env,
    upstream_proxy_from_env,
};
