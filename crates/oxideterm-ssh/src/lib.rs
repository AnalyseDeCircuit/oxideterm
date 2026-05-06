// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SSH domain model for native OxideTerm.
//!
//! This crate mirrors the Tauri SSH architecture at the model boundary:
//! connection configs, a reference-counted connection registry, node routing,
//! and reconnect orchestration. The actual russh PTY transport plugs into this
//! crate without leaking SSH state into GPUI views.

mod config;
mod connection_registry;
mod host_key;
mod reconnect;
mod router;
mod transport;

pub use config::{AuthMethod, ProxyHopConfig, SshConfig};
pub use connection_registry::{
    ConnectionConsumer, ConnectionInfo, ConnectionPoolConfig, ConnectionPoolStats, ConnectionState,
    HEARTBEAT_FAIL_THRESHOLD, HEARTBEAT_INTERVAL, SshConnectionHandle, SshConnectionRegistry,
    WS_BRIDGE_HEARTBEAT_INTERVAL, WS_BRIDGE_HEARTBEAT_TIMEOUT,
};
pub use host_key::{HostKeyStatus, check_host_key, remove_host_key};
pub use reconnect::{
    PhaseEvent, PhaseResult, ReconnectJob, ReconnectOrchestratorStore, ReconnectPhase,
    ReconnectSnapshot, ReconnectTiming,
};
pub use router::{
    NodeId, NodeReadiness, NodeRouter, NodeState, NodeStateEvent, NodeStateSnapshot, RouteError,
    TerminalEndpoint,
};
pub use transport::{
    KeyboardInteractivePrompt, KeyboardInteractivePromptRequest, SshPromptError, SshPromptHandler,
    SshPtyHandle, SshTransportClient, SshTransportCommand, SshTransportError,
};
