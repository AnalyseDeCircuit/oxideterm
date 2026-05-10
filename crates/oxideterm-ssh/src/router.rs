// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use dashmap::DashMap;
use oxideterm_sftp::{SftpError, SftpSession};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, mpsc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

use crate::{
    AcquiredSftpMeta, ConnectionConsumer, ConnectionInfo, ConnectionState,
    ConnectionTransportStatus, SshConfig, SshConnectionHandle, SshConnectionRegistry,
};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Error, Serialize)]
pub enum RouteError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("No active connection for node: {0}")]
    NotConnected(String),
    #[error("Connection in error state: {0}")]
    ConnectionError(String),
    #[error("Capability unavailable: {0}")]
    CapabilityUnavailable(String),
    #[error("Connection timeout: {0}")]
    ConnectionTimeout(String),
    #[error("Parent node is not connected: {0}")]
    ParentNotConnected(String),
    #[error("Maximum session tree depth exceeded: {0}")]
    MaxDepthExceeded(u32),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeReadiness {
    Ready,
    Connecting,
    Error,
    Disconnected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeOrigin {
    ManualPreset {
        saved_connection_id: String,
        hop_index: u32,
    },
    AutoRoute {
        target_host: String,
        route_id: String,
        hop_index: u32,
    },
    DrillDown {
        timestamp: i64,
    },
    Direct,
    Restored {
        saved_connection_id: String,
    },
}

impl Default for NodeOrigin {
    fn default() -> Self {
        Self::Direct
    }
}

impl NodeOrigin {
    pub fn origin_type(&self) -> &'static str {
        match self {
            Self::ManualPreset { .. } => "manual_preset",
            Self::AutoRoute { .. } => "auto_route",
            Self::DrillDown { .. } => "drill_down",
            Self::Direct => "direct",
            Self::Restored { .. } => "restored",
        }
    }

    pub fn saved_connection_id(&self) -> Option<&str> {
        match self {
            Self::ManualPreset {
                saved_connection_id,
                ..
            }
            | Self::Restored {
                saved_connection_id,
            } => Some(saved_connection_id),
            Self::AutoRoute { .. } | Self::DrillDown { .. } | Self::Direct => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalEndpoint {
    pub ws_port: u16,
    pub ws_token: String,
    pub session_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeState {
    pub readiness: NodeReadiness,
    pub error: Option<String>,
    pub sftp_ready: bool,
    pub sftp_cwd: Option<String>,
    pub ws_endpoint: Option<TerminalEndpoint>,
}

impl Default for NodeState {
    fn default() -> Self {
        Self {
            readiness: NodeReadiness::Disconnected,
            error: None,
            sftp_ready: false,
            sftp_cwd: None,
            ws_endpoint: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeStateSnapshot {
    pub state: NodeState,
    pub generation: u64,
}

#[derive(Clone, Debug)]
pub struct ResolvedConnection {
    pub connection_id: String,
    pub handle: SshConnectionHandle,
    pub terminal_session_id: Option<String>,
    pub sftp_session_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NodeStateEvent {
    ConnectionStateChanged {
        node_id: String,
        generation: u64,
        state: NodeReadiness,
        reason: String,
    },
    SftpReady {
        node_id: String,
        generation: u64,
        ready: bool,
        cwd: Option<String>,
    },
    TerminalEndpointChanged {
        node_id: String,
        generation: u64,
        ws_port: u16,
        ws_token: String,
    },
}

#[derive(Clone, Debug)]
pub struct NodeRuntimeSnapshot {
    pub config: SshConfig,
    pub parent_id: Option<NodeId>,
    pub children_ids: Vec<NodeId>,
    pub depth: u32,
    pub origin: NodeOrigin,
    pub connection_id: Option<String>,
    pub terminal_session_id: Option<String>,
    pub sftp_session_id: Option<String>,
    pub state: NodeState,
    pub created_at_ms: u64,
    pub generation: u64,
}

#[derive(Clone, Debug)]
struct NodeRuntimeEntry {
    config: SshConfig,
    parent_id: Option<NodeId>,
    children_ids: Vec<NodeId>,
    depth: u32,
    origin: NodeOrigin,
    connection_id: Option<String>,
    terminal_session_id: Option<String>,
    sftp_session_id: Option<String>,
    state: NodeState,
    created_at_ms: u64,
    generation: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeTreeSnapshot {
    pub version: u32,
    pub exported_at_ms: u64,
    pub root_ids: Vec<NodeId>,
    pub nodes: Vec<NodeTreeSnapshotNode>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeTreeSnapshotNode {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    pub children_ids: Vec<NodeId>,
    pub depth: u32,
    pub config: SshConfig,
    pub origin: NodeOrigin,
    pub state: NodeState,
    pub connection_id: Option<String>,
    pub terminal_session_id: Option<String>,
    pub sftp_session_id: Option<String>,
    pub created_at_ms: u64,
    pub generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeTreeExpansion {
    pub target_node_id: NodeId,
    pub path_node_ids: Vec<NodeId>,
    pub chain_depth: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub depth: u32,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub display_name: Option<String>,
    pub state: NodeReadiness,
    pub error: Option<String>,
    pub has_children: bool,
    pub is_last_child: bool,
    pub origin_type: String,
    pub terminal_session_id: Option<String>,
    pub sftp_session_id: Option<String>,
    pub ssh_connection_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTreeSummary {
    pub total_nodes: usize,
    pub root_count: usize,
    pub connected_count: usize,
    pub max_depth: u32,
}

const MAX_SESSION_TREE_DEPTH: u32 = 10;

include!("router/runtime_store.rs");
include!("router/events.rs");
include!("router/node_router.rs");
include!("router/helpers.rs");
include!("router/tests.rs");
