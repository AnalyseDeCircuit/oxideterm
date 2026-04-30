// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Session Management Module
//!
//! Provides a global session registry with:
//! - State machine for session lifecycle
//! - Concurrent connection limiting
//! - Thread-safe session access via DashMap
//! - Silent reconnection with exponential backoff
//! - Connection health monitoring
//! - Tauri event emission for frontend state sync
//! - **Session Tree** for dynamic jump host support (三种跳板机模式)
//! - **Topology Graph** for auto-route calculation (静态自动路由)

pub mod auth;
pub mod auto_reconnect;
pub mod command_facts;
pub mod env_detector;
pub mod events;
pub mod health;
pub mod history_archive;
pub mod parser;
pub mod profiler;
mod reconnect;
mod registry;
pub mod scroll_buffer;
pub mod search;
mod state;
pub mod topology_graph;
pub mod tree;
pub mod types;

pub use auth::{KeyAuth, load_private_key};
pub use auto_reconnect::AutoReconnectService;
pub use command_facts::{
    CloseCommandFactPatch, CommandFact, CommandFactAuthorityMode, CommandFactClosedBy,
    CommandFactConfidence, CommandFactLedgerCandidate, CommandFactLedgerDiagnostic,
    CommandFactLedgerDiagnosticReason, CommandFactLedgerDiagnosticsSnapshot,
    CommandFactOutputResponse, CommandFactSource, CommandFactStatus, CommandFactStore,
    CreateCommandFactRequest, CreateCommandFactResponse,
};
pub use env_detector::RemoteEnvInfo;
pub use events::{NetworkStatusPayload, event_names};
pub use health::{
    HealthMetrics, HealthStatus, HealthThresholds, HealthTracker, MetricsSource, QuickHealthCheck,
    ResourceMetrics,
};
pub use history_archive::{
    ArchiveHealthSnapshot, ArchivedHistoryExcerpt, TerminalHistoryArchive,
    TerminalHistoryArchiveError, cleanup_stale_terminal_history_archives,
};
pub use parser::{BatchParser, parse_terminal_output, parse_terminal_output_simple};
pub use profiler::{ProfilerState, ResourceProfiler};
pub use reconnect::{
    ReconnectConfig, ReconnectError, ReconnectEvent, ReconnectState, SessionReconnector,
};
pub use registry::{RegistryError, SessionRegistry};
pub use scroll_buffer::{
    BufferLineIdentity, BufferStats, ScrollBuffer, SerializedBuffer, TerminalLine,
};
pub use search::{SearchMatch, SearchOptions, SearchResult, search_lines};
pub use state::{SessionState, SessionStateMachine};
pub use topology_graph::{
    NetworkTopology, RouteResult, TopologyEdge, TopologyNodeConfig, TopologyNodeInfo,
};
pub use tree::{FlatNode, NodeConnection, NodeOrigin, NodeState, SessionTree, TreeError};
pub use types::{AuthMethod, SessionConfig, SessionEntry, SessionInfo, SessionStats};
