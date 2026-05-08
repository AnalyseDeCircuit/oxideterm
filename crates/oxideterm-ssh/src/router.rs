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

use crate::{
    AcquiredSftpMeta, ConnectionConsumer, ConnectionInfo, ConnectionState, SshConfig,
    SshConnectionHandle, SshConnectionRegistry,
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

#[derive(Clone, Debug, Default)]
pub struct NodeRuntimeStore {
    nodes: Arc<DashMap<NodeId, NodeRuntimeEntry>>,
    root_ids: Arc<parking_lot::RwLock<Vec<NodeId>>>,
    connection_nodes: Arc<DashMap<String, NodeId>>,
}

impl NodeRuntimeStore {
    pub fn upsert_node(&self, node_id: NodeId, config: SshConfig) {
        self.upsert_node_with_origin(node_id, config, NodeOrigin::Direct);
    }

    pub fn upsert_node_with_origin(&self, node_id: NodeId, config: SshConfig, origin: NodeOrigin) {
        let is_new = !self.nodes.contains_key(&node_id);
        self.nodes
            .entry(node_id.clone())
            .and_modify(|route| {
                route.config = config.clone();
                route.origin = origin.clone();
                route.generation += 1;
            })
            .or_insert_with(|| NodeRuntimeEntry {
                config,
                parent_id: None,
                children_ids: Vec::new(),
                depth: 0,
                origin,
                connection_id: None,
                terminal_session_id: None,
                sftp_session_id: None,
                state: NodeState::default(),
                created_at_ms: now_ms(),
                generation: 0,
            });
        if is_new {
            let mut root_ids = self.root_ids.write();
            if !root_ids.contains(&node_id) {
                root_ids.push(node_id);
            }
        }
    }

    pub fn snapshot(&self, node_id: &NodeId) -> Option<NodeRuntimeSnapshot> {
        let route = self.nodes.get(node_id)?;
        Some(NodeRuntimeSnapshot {
            config: route.config.clone(),
            parent_id: route.parent_id.clone(),
            children_ids: route.children_ids.clone(),
            depth: route.depth,
            origin: route.origin.clone(),
            connection_id: route.connection_id.clone(),
            terminal_session_id: route.terminal_session_id.clone(),
            sftp_session_id: route.sftp_session_id.clone(),
            state: route.state.clone(),
            created_at_ms: route.created_at_ms,
            generation: route.generation,
        })
    }

    pub fn upsert_child_node(
        &self,
        parent_id: NodeId,
        node_id: NodeId,
        config: SshConfig,
    ) -> Result<(), RouteError> {
        self.upsert_child_node_with_origin(parent_id, node_id, config, NodeOrigin::Direct)
    }

    pub fn upsert_child_node_with_origin(
        &self,
        parent_id: NodeId,
        node_id: NodeId,
        config: SshConfig,
        origin: NodeOrigin,
    ) -> Result<(), RouteError> {
        let parent_depth = {
            let mut parent = self
                .nodes
                .get_mut(&parent_id)
                .ok_or_else(|| RouteError::NodeNotFound(parent_id.0.clone()))?;
            if !parent.children_ids.contains(&node_id) {
                parent.children_ids.push(node_id.clone());
                parent.generation += 1;
            }
            parent.depth
        };

        self.nodes
            .entry(node_id.clone())
            .and_modify(|route| {
                route.config = config.clone();
                route.parent_id = Some(parent_id.clone());
                route.depth = parent_depth + 1;
                route.origin = origin.clone();
                route.generation += 1;
            })
            .or_insert_with(|| NodeRuntimeEntry {
                config,
                parent_id: Some(parent_id),
                children_ids: Vec::new(),
                depth: parent_depth + 1,
                origin,
                connection_id: None,
                terminal_session_id: None,
                sftp_session_id: None,
                state: NodeState::default(),
                created_at_ms: now_ms(),
                generation: 0,
            });
        self.root_ids.write().retain(|id| id != &node_id);
        Ok(())
    }

    pub fn export_snapshot(&self) -> NodeTreeSnapshot {
        let mut nodes = self
            .nodes
            .iter()
            .map(|entry| {
                let route = entry.value();
                NodeTreeSnapshotNode {
                    id: entry.key().clone(),
                    parent_id: route.parent_id.clone(),
                    children_ids: route.children_ids.clone(),
                    depth: route.depth,
                    config: route.config.clone(),
                    origin: route.origin.clone(),
                    state: route.state.clone(),
                    connection_id: route.connection_id.clone(),
                    terminal_session_id: route.terminal_session_id.clone(),
                    sftp_session_id: route.sftp_session_id.clone(),
                    created_at_ms: route.created_at_ms,
                    generation: route.generation,
                }
            })
            .collect::<Vec<_>>();
        nodes.sort_by_key(|node| (node.depth, node.created_at_ms, node.id.0.clone()));

        NodeTreeSnapshot {
            version: 1,
            exported_at_ms: now_ms(),
            root_ids: self.root_ids.read().clone(),
            nodes,
        }
    }

    pub fn apply_snapshot(&self, snapshot: NodeTreeSnapshot) -> Result<(), RouteError> {
        let node_ids = snapshot
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        for node in &snapshot.nodes {
            if let Some(parent_id) = &node.parent_id
                && !node_ids.contains(parent_id)
            {
                return Err(RouteError::NodeNotFound(parent_id.0.clone()));
            }
        }

        self.nodes.clear();
        self.connection_nodes.clear();
        {
            let mut root_ids = self.root_ids.write();
            root_ids.clear();
            root_ids.extend(snapshot.root_ids);
        }

        for node in snapshot.nodes {
            if let Some(connection_id) = node.connection_id.as_ref() {
                self.connection_nodes
                    .insert(connection_id.clone(), node.id.clone());
            }
            self.nodes.insert(
                node.id,
                NodeRuntimeEntry {
                    config: node.config,
                    parent_id: node.parent_id,
                    children_ids: node.children_ids,
                    depth: node.depth,
                    origin: node.origin,
                    connection_id: node.connection_id,
                    terminal_session_id: node.terminal_session_id,
                    sftp_session_id: node.sftp_session_id,
                    state: node.state,
                    created_at_ms: node.created_at_ms,
                    generation: node.generation,
                },
            );
        }
        self.reconcile_topology();
        Ok(())
    }

    pub fn clear(&self) {
        self.nodes.clear();
        self.connection_nodes.clear();
        self.root_ids.write().clear();
    }

    pub fn flatten(&self) -> Vec<FlatNode> {
        fn collect(store: &NodeRuntimeStore, node_id: &NodeId, output: &mut Vec<FlatNode>) {
            let Some(route) = store.nodes.get(node_id) else {
                return;
            };
            let route = route.value().clone();
            output.push(store.flat_node(node_id, &route));
            for child_id in route.children_ids {
                collect(store, &child_id, output);
            }
        }

        let mut output = Vec::new();
        for root_id in self.root_ids.read().iter() {
            collect(self, root_id, &mut output);
        }
        output
    }

    pub fn summary(&self) -> SessionTreeSummary {
        let nodes = self
            .nodes
            .iter()
            .map(|entry| entry.value().clone())
            .collect::<Vec<_>>();
        SessionTreeSummary {
            total_nodes: nodes.len(),
            root_count: self.root_ids.read().len(),
            connected_count: nodes
                .iter()
                .filter(|node| matches!(node.state.readiness, NodeReadiness::Ready))
                .count(),
            max_depth: nodes.iter().map(|node| node.depth).max().unwrap_or(0),
        }
    }

    pub fn reconcile_with_connections(&self, connections: &HashMap<String, ConnectionState>) {
        for mut route in self.nodes.iter_mut() {
            let Some(connection_id) = route.connection_id.clone() else {
                route.state.readiness = NodeReadiness::Disconnected;
                route.state.error = None;
                continue;
            };
            match connections.get(&connection_id) {
                Some(state) => {
                    route.state.readiness = readiness_for_connection_state(state);
                    route.state.error = match state {
                        ConnectionState::Error(error) => Some(error.clone()),
                        ConnectionState::LinkDown => Some("Link down".to_string()),
                        _ => None,
                    };
                }
                None => {
                    route.connection_id = None;
                    route.terminal_session_id = None;
                    route.sftp_session_id = None;
                    route.state.ws_endpoint = None;
                    route.state.sftp_ready = false;
                    route.state.sftp_cwd = None;
                    route.state.readiness = NodeReadiness::Disconnected;
                    route.state.error = None;
                }
            }
            route.generation += 1;
        }
        self.rebuild_connection_index();
    }

    pub fn subtree_postorder(&self, node_id: &NodeId) -> Vec<NodeId> {
        fn collect(store: &NodeRuntimeStore, node_id: &NodeId, output: &mut Vec<NodeId>) {
            let children = store
                .nodes
                .get(node_id)
                .map(|node| node.children_ids.clone())
                .unwrap_or_default();
            for child_id in children {
                collect(store, &child_id, output);
            }
            output.push(node_id.clone());
        }

        let mut nodes = Vec::new();
        collect(self, node_id, &mut nodes);
        nodes
    }

    fn bind_connection(
        &self,
        node_id: &NodeId,
        connection_id: String,
        connection: &ConnectionInfo,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        if let Some(previous_id) = route.connection_id.as_ref()
            && previous_id != &connection_id
        {
            self.connection_nodes.remove(previous_id);
        }
        self.connection_nodes
            .insert(connection_id.clone(), node_id.clone());
        route.connection_id = Some(connection_id);
        route.generation += 1;
        route.state.readiness = readiness_for_connection(connection);
        route.state.error = match &connection.state {
            ConnectionState::Error(error) => Some(error.clone()),
            ConnectionState::LinkDown => Some("Link down".to_string()),
            _ => None,
        };
        Ok(NodeStateEvent::ConnectionStateChanged {
            node_id: node_id.0.clone(),
            generation: route.generation,
            state: route.state.readiness.clone(),
            reason: "connection bound".to_string(),
        })
    }

    fn bind_terminal_session(
        &self,
        node_id: &NodeId,
        session_id: String,
    ) -> Result<(), RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.terminal_session_id = Some(session_id);
        route.state.ws_endpoint = None;
        route.generation += 1;
        Ok(())
    }

    fn bind_terminal_endpoint(
        &self,
        node_id: &NodeId,
        endpoint: TerminalEndpoint,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.terminal_session_id = Some(endpoint.session_id.clone());
        route.state.ws_endpoint = Some(endpoint.clone());
        route.generation += 1;
        Ok(NodeStateEvent::TerminalEndpointChanged {
            node_id: node_id.0.clone(),
            generation: route.generation,
            ws_port: endpoint.ws_port,
            ws_token: endpoint.ws_token,
        })
    }

    fn unbind_terminal_session(
        &self,
        node_id: &NodeId,
        session_id: &str,
    ) -> Result<(), RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        if route.terminal_session_id.as_deref() == Some(session_id) {
            route.terminal_session_id = None;
            route.state.ws_endpoint = None;
            route.generation += 1;
        }
        Ok(())
    }

    fn bind_sftp_session(
        &self,
        node_id: &NodeId,
        session_id: String,
        cwd: Option<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.sftp_session_id = Some(session_id);
        route.generation += 1;
        route.state.sftp_ready = true;
        route.state.sftp_cwd = cwd;
        Ok(NodeStateEvent::SftpReady {
            node_id: node_id.0.clone(),
            generation: route.generation,
            ready: route.state.sftp_ready,
            cwd: route.state.sftp_cwd.clone(),
        })
    }

    fn set_sftp_ready(
        &self,
        node_id: &NodeId,
        ready: bool,
        cwd: Option<String>,
    ) -> Result<(), RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.state.sftp_ready = ready;
        route.state.sftp_cwd = cwd;
        route.generation += 1;
        Ok(())
    }

    fn update_connection_state(
        &self,
        node_id: &NodeId,
        connection: &ConnectionInfo,
        reason: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.generation += 1;
        route.state.readiness = readiness_for_connection(connection);
        route.state.error = match &connection.state {
            ConnectionState::Error(error) => Some(error.clone()),
            _ => None,
        };
        Ok(NodeStateEvent::ConnectionStateChanged {
            node_id: node_id.0.clone(),
            generation: route.generation,
            state: route.state.readiness.clone(),
            reason: reason.into(),
        })
    }

    pub fn node_id_for_connection(&self, connection_id: &str) -> Option<NodeId> {
        self.connection_nodes
            .get(connection_id)
            .map(|entry| entry.value().clone())
    }

    pub fn connection_id_for_node(&self, node_id: &NodeId) -> Option<String> {
        self.nodes
            .get(node_id)
            .and_then(|route| route.connection_id.clone())
    }

    fn flat_node(&self, node_id: &NodeId, route: &NodeRuntimeEntry) -> FlatNode {
        let is_last_child = if let Some(parent_id) = &route.parent_id {
            self.nodes
                .get(parent_id)
                .is_none_or(|parent| parent.children_ids.last() == Some(node_id))
        } else {
            self.root_ids.read().last() == Some(node_id)
        };
        FlatNode {
            id: node_id.0.clone(),
            parent_id: route.parent_id.as_ref().map(|id| id.0.clone()),
            depth: route.depth,
            host: route.config.host.clone(),
            port: route.config.port,
            username: route.config.username.clone(),
            display_name: None,
            state: route.state.readiness.clone(),
            error: route.state.error.clone(),
            has_children: !route.children_ids.is_empty(),
            is_last_child,
            origin_type: route.origin.origin_type().to_string(),
            terminal_session_id: route.terminal_session_id.clone(),
            sftp_session_id: route.sftp_session_id.clone(),
            ssh_connection_id: route.connection_id.clone(),
        }
    }

    fn reconcile_topology(&self) {
        let node_ids = self
            .nodes
            .iter()
            .map(|entry| entry.key().clone())
            .collect::<HashSet<_>>();
        for mut route in self.nodes.iter_mut() {
            route.children_ids.retain(|id| node_ids.contains(id));
            if route
                .parent_id
                .as_ref()
                .is_some_and(|parent_id| !node_ids.contains(parent_id))
            {
                route.parent_id = None;
                route.depth = 0;
            }
        }

        let mut computed_roots = self
            .nodes
            .iter()
            .filter_map(|entry| {
                entry
                    .value()
                    .parent_id
                    .is_none()
                    .then_some(entry.key().clone())
            })
            .collect::<Vec<_>>();
        computed_roots.sort_by_key(|id| {
            self.nodes
                .get(id)
                .map(|node| node.created_at_ms)
                .unwrap_or_default()
        });

        let mut roots = self.root_ids.write();
        roots.retain(|id| node_ids.contains(id) && computed_roots.contains(id));
        for root_id in computed_roots {
            if !roots.contains(&root_id) {
                roots.push(root_id);
            }
        }
        drop(roots);
        self.rebuild_connection_index();
    }

    fn rebuild_connection_index(&self) {
        self.connection_nodes.clear();
        for entry in self.nodes.iter() {
            if let Some(connection_id) = entry.value().connection_id.as_ref() {
                self.connection_nodes
                    .insert(connection_id.clone(), entry.key().clone());
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NodeEventSequencer {
    generations: Arc<DashMap<NodeId, u64>>,
}

impl NodeEventSequencer {
    pub fn next(&self, node_id: &NodeId) -> u64 {
        let mut generation = self.generations.entry(node_id.clone()).or_insert(0);
        *generation += 1;
        *generation
    }

    pub fn current(&self, node_id: &NodeId) -> u64 {
        self.generations
            .get(node_id)
            .map(|generation| *generation)
            .unwrap_or_default()
    }

    pub fn reset(&self, node_id: &NodeId) {
        self.generations.remove(node_id);
    }
}

#[derive(Clone, Debug, Default)]
pub struct NodeEventEmitter {
    sequencer: NodeEventSequencer,
    connection_nodes: Arc<DashMap<String, NodeId>>,
    listeners: Arc<parking_lot::RwLock<Vec<mpsc::Sender<NodeStateEvent>>>>,
}

impl NodeEventEmitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sequencer(&self) -> &NodeEventSequencer {
        &self.sequencer
    }

    pub fn subscribe(&self, sender: mpsc::Sender<NodeStateEvent>) {
        self.listeners.write().push(sender);
    }

    pub fn register(&self, connection_id: impl Into<String>, node_id: NodeId) {
        self.connection_nodes.insert(connection_id.into(), node_id);
    }

    pub fn unregister(&self, connection_id: &str) -> Option<NodeId> {
        self.connection_nodes
            .remove(connection_id)
            .map(|(_, node_id)| node_id)
    }

    pub fn node_id_for_connection(&self, connection_id: &str) -> Option<NodeId> {
        self.connection_nodes
            .get(connection_id)
            .map(|entry| entry.value().clone())
    }

    pub fn emit_connection_state_changed(
        &self,
        connection_id: &str,
        state: NodeReadiness,
        reason: impl Into<String>,
    ) -> Option<NodeStateEvent> {
        let node_id = self.node_id_for_connection(connection_id)?;
        let generation = self.sequencer.next(&node_id);
        let event = NodeStateEvent::ConnectionStateChanged {
            node_id: node_id.0,
            generation,
            state,
            reason: reason.into(),
        };
        self.dispatch(&event);
        Some(event)
    }

    pub fn emit_state_from_connection(
        &self,
        connection_id: &str,
        connection_state: &ConnectionState,
        reason: impl Into<String>,
    ) -> Option<NodeStateEvent> {
        let reason = reason.into();
        let reason = match connection_state {
            ConnectionState::Error(error) if reason.is_empty() => error.clone(),
            ConnectionState::Error(error) => format!("{reason}: {error}"),
            ConnectionState::LinkDown if reason.is_empty() => "link down".to_string(),
            _ => reason,
        };
        self.emit_connection_state_changed(
            connection_id,
            readiness_for_connection_state(connection_state),
            reason,
        )
    }

    pub fn emit_sftp_ready(
        &self,
        connection_id: &str,
        ready: bool,
        cwd: Option<String>,
    ) -> Option<NodeStateEvent> {
        let node_id = self.node_id_for_connection(connection_id)?;
        let generation = self.sequencer.next(&node_id);
        let event = NodeStateEvent::SftpReady {
            node_id: node_id.0,
            generation,
            ready,
            cwd,
        };
        self.dispatch(&event);
        Some(event)
    }

    pub fn emit_terminal_endpoint_changed(
        &self,
        connection_id: &str,
        ws_port: u16,
        ws_token: impl Into<String>,
    ) -> Option<NodeStateEvent> {
        let node_id = self.node_id_for_connection(connection_id)?;
        let generation = self.sequencer.next(&node_id);
        let event = NodeStateEvent::TerminalEndpointChanged {
            node_id: node_id.0,
            generation,
            ws_port,
            ws_token: ws_token.into(),
        };
        self.dispatch(&event);
        Some(event)
    }

    fn dispatch(&self, event: &NodeStateEvent) {
        let listeners = self.listeners.read().clone();
        for listener in listeners {
            let _ = listener.send(event.clone());
        }
    }
}

#[derive(Clone, Debug)]
pub struct NodeRouter {
    registry: SshConnectionRegistry,
    runtime: NodeRuntimeStore,
    emitter: NodeEventEmitter,
}

impl NodeRouter {
    pub fn new(registry: SshConnectionRegistry) -> Self {
        Self::with_runtime_store(registry, NodeRuntimeStore::default())
    }

    pub fn with_runtime_store(registry: SshConnectionRegistry, runtime: NodeRuntimeStore) -> Self {
        Self::with_runtime_store_and_emitter(registry, runtime, NodeEventEmitter::default())
    }

    pub fn with_runtime_store_and_emitter(
        registry: SshConnectionRegistry,
        runtime: NodeRuntimeStore,
        emitter: NodeEventEmitter,
    ) -> Self {
        registry.set_node_event_emitter(emitter.clone());
        Self {
            registry,
            runtime,
            emitter,
        }
    }

    pub fn runtime_store(&self) -> NodeRuntimeStore {
        self.runtime.clone()
    }

    pub fn emitter(&self) -> &NodeEventEmitter {
        &self.emitter
    }

    pub fn upsert_node(&self, node_id: NodeId, config: SshConfig) {
        self.runtime.upsert_node(node_id, config);
    }

    pub fn upsert_node_with_origin(&self, node_id: NodeId, config: SshConfig, origin: NodeOrigin) {
        self.runtime
            .upsert_node_with_origin(node_id, config, origin);
    }

    pub fn export_tree_snapshot(&self) -> NodeTreeSnapshot {
        self.runtime.export_snapshot()
    }

    pub fn apply_tree_snapshot(&self, snapshot: NodeTreeSnapshot) -> Result<(), RouteError> {
        self.runtime.apply_snapshot(snapshot)
    }

    pub fn flatten_tree(&self) -> Vec<FlatNode> {
        self.runtime.flatten()
    }

    pub fn tree_summary(&self) -> SessionTreeSummary {
        self.runtime.summary()
    }

    pub fn reconcile_runtime_tree(&self) {
        let connections = self
            .registry
            .list()
            .into_iter()
            .map(|info| (info.connection_id, info.state))
            .collect::<HashMap<_, _>>();
        self.runtime.reconcile_with_connections(&connections);
    }

    pub fn resolve_connection(&self, node_id: &NodeId) -> Result<ResolvedConnection, RouteError> {
        let runtime = self
            .runtime
            .snapshot(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        let connection_id = runtime
            .connection_id
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;

        let handle = self
            .registry
            .get(&connection_id)
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        self.require_resolvable_state(node_id, &handle.info())?;
        Ok(ResolvedConnection {
            connection_id,
            handle,
            terminal_session_id: runtime.terminal_session_id,
            sftp_session_id: runtime.sftp_session_id,
        })
    }

    pub fn acquire_connection(
        &self,
        node_id: &NodeId,
        consumer: ConnectionConsumer,
    ) -> Result<ResolvedConnection, RouteError> {
        let runtime = self
            .runtime
            .snapshot(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        let connection_id = runtime
            .connection_id
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        let handle = self
            .registry
            .get(&connection_id)
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        self.require_resolvable_state(node_id, &handle.info())?;
        let handle = self
            .registry
            .acquire_consumer_for_connection(&connection_id, consumer)
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        let _ =
            self.runtime
                .update_connection_state(node_id, &handle.info(), "connection acquired");

        self.require_resolvable_state(node_id, &handle.info())?;
        Ok(ResolvedConnection {
            connection_id,
            handle,
            terminal_session_id: runtime.terminal_session_id,
            sftp_session_id: runtime.sftp_session_id,
        })
    }

    pub async fn acquire_connection_wait(
        &self,
        node_id: &NodeId,
        consumer: ConnectionConsumer,
        max_wait: Duration,
    ) -> Result<ResolvedConnection, RouteError> {
        let runtime = self
            .runtime
            .snapshot(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        let connection_id = runtime
            .connection_id
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        self.wait_for_active(&connection_id, max_wait).await?;
        let handle = self
            .registry
            .acquire_consumer_for_connection(&connection_id, consumer)
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        let _ =
            self.runtime
                .update_connection_state(node_id, &handle.info(), "connection acquired");

        Ok(ResolvedConnection {
            connection_id,
            handle,
            terminal_session_id: runtime.terminal_session_id,
            sftp_session_id: runtime.sftp_session_id,
        })
    }

    pub fn bind_connection(
        &self,
        node_id: &NodeId,
        connection_id: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let connection_id = connection_id.into();
        let handle = self
            .registry
            .get(&connection_id)
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        let connection = handle.info();
        let event = self
            .runtime
            .bind_connection(node_id, connection_id.clone(), &connection)?;
        // Tauri registers connectionId -> nodeId when the runtime tree binds a
        // connection. Native keeps the same translation point so lower-level
        // connection events can be consumed as node events without consulting
        // terminal panes.
        self.emitter
            .register(connection_id.clone(), node_id.clone());
        Ok(self
            .emitter
            .emit_state_from_connection(&connection_id, &connection.state, "connection bound")
            .unwrap_or(event))
    }

    pub fn bind_terminal_session(
        &self,
        node_id: &NodeId,
        session_id: impl Into<String>,
    ) -> Result<(), RouteError> {
        self.runtime
            .bind_terminal_session(node_id, session_id.into())
    }

    pub fn bind_terminal_endpoint(
        &self,
        node_id: &NodeId,
        endpoint: TerminalEndpoint,
    ) -> Result<NodeStateEvent, RouteError> {
        let event = self
            .runtime
            .bind_terminal_endpoint(node_id, endpoint.clone())?;
        self.emitter.dispatch(&event);
        Ok(event)
    }

    pub fn unbind_terminal_session(
        &self,
        node_id: &NodeId,
        session_id: &str,
    ) -> Result<(), RouteError> {
        self.runtime.unbind_terminal_session(node_id, session_id)
    }

    pub fn terminal_url(&self, node_id: &NodeId) -> Result<TerminalEndpoint, RouteError> {
        let runtime = self
            .runtime
            .snapshot(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        runtime.state.ws_endpoint.ok_or_else(|| {
            RouteError::NotConnected(format!("No active terminal session for node {}", node_id.0))
        })
    }

    pub fn node_id_for_connection(&self, connection_id: &str) -> Option<NodeId> {
        self.runtime.node_id_for_connection(connection_id)
    }

    pub fn connection_id_for_node(&self, node_id: &NodeId) -> Option<String> {
        self.runtime.connection_id_for_node(node_id)
    }

    pub fn bind_sftp_session(
        &self,
        node_id: &NodeId,
        session_id: impl Into<String>,
        cwd: Option<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        self.runtime
            .bind_sftp_session(node_id, session_id.into(), cwd)
    }

    pub async fn acquire_sftp(
        &self,
        node_id: &NodeId,
    ) -> Result<Arc<Mutex<SftpSession>>, RouteError> {
        let resolved = self
            .resolve_connection_wait(node_id, Duration::from_secs(15))
            .await?;
        let AcquiredSftpMeta {
            session,
            was_new,
            cwd,
        } = resolved
            .handle
            .acquire_sftp_with_meta()
            .await
            .map_err(|error| sftp_route_error("SFTP init failed", error))?;

        if was_new {
            let _ = self
                .registry
                .mark_sftp_session(&resolved.connection_id, true, cwd.clone());
        }
        self.runtime.set_sftp_ready(node_id, true, cwd)?;
        Ok(session)
    }

    pub async fn acquire_transfer_sftp(&self, node_id: &NodeId) -> Result<SftpSession, RouteError> {
        let resolved = self
            .resolve_connection_wait(node_id, Duration::from_secs(15))
            .await?;
        resolved
            .handle
            .acquire_transfer_sftp()
            .await
            .map_err(|error| sftp_route_error("Transfer SFTP init failed", error))
    }

    pub async fn invalidate_and_reacquire_sftp(
        &self,
        node_id: &NodeId,
    ) -> Result<Arc<Mutex<SftpSession>>, RouteError> {
        let resolved = self
            .resolve_connection_wait(node_id, Duration::from_secs(15))
            .await?;
        let had_sftp = resolved.handle.invalidate_sftp().await;
        if had_sftp {
            let _ = self
                .registry
                .mark_sftp_session(&resolved.connection_id, false, None);
            self.runtime.set_sftp_ready(node_id, false, None)?;
        }

        let AcquiredSftpMeta { session, cwd, .. } = resolved
            .handle
            .acquire_sftp_with_meta()
            .await
            .map_err(|error| sftp_route_error("SFTP rebuild failed", error))?;
        let _ = self
            .registry
            .mark_sftp_session(&resolved.connection_id, true, cwd.clone());
        self.runtime.set_sftp_ready(node_id, true, cwd)?;
        Ok(session)
    }

    pub fn node_state(&self, node_id: &NodeId) -> Result<NodeStateSnapshot, RouteError> {
        let mut runtime = self
            .runtime
            .snapshot(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        if let Some(connection_id) = runtime.connection_id.clone() {
            if let Some(handle) = self.registry.get(&connection_id) {
                let info = handle.info();
                runtime.state.readiness = readiness_for_connection(&info);
                runtime.state.error = match &info.state {
                    ConnectionState::Error(error) => Some(error.clone()),
                    ConnectionState::LinkDown => Some("Link down".to_string()),
                    _ => None,
                };
                if let Some(sftp_state) = self.registry.sftp_session_state(&connection_id) {
                    runtime.state.sftp_ready = sftp_state.ready;
                    runtime.state.sftp_cwd = sftp_state.cwd;
                }
            } else {
                runtime.state.readiness = NodeReadiness::Disconnected;
                runtime.state.error = None;
                runtime.state.sftp_ready = false;
                runtime.state.sftp_cwd = None;
            }
        }
        Ok(NodeStateSnapshot {
            state: runtime.state,
            generation: self
                .emitter
                .sequencer()
                .current(node_id)
                .max(runtime.generation),
        })
    }

    pub fn sync_connection_state(
        &self,
        node_id: &NodeId,
        connection: &ConnectionInfo,
        reason: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let reason = reason.into();
        let event = self
            .runtime
            .update_connection_state(node_id, connection, reason.clone())?;
        Ok(self
            .emitter
            .emit_state_from_connection(&connection.connection_id, &connection.state, reason)
            .unwrap_or(event))
    }

    pub fn sync_connection_state_by_connection_id(
        &self,
        connection: &ConnectionInfo,
        reason: impl Into<String>,
    ) -> Option<NodeStateEvent> {
        let node_id = self
            .emitter
            .node_id_for_connection(&connection.connection_id)
            .or_else(|| self.node_id_for_connection(&connection.connection_id))?;
        self.sync_connection_state(&node_id, connection, reason)
            .ok()
    }

    async fn resolve_connection_wait(
        &self,
        node_id: &NodeId,
        max_wait: Duration,
    ) -> Result<ResolvedConnection, RouteError> {
        let runtime = self
            .runtime
            .snapshot(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        let connection_id = runtime
            .connection_id
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;

        self.wait_for_active(&connection_id, max_wait).await?;
        let handle = self
            .registry
            .get(&connection_id)
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        Ok(ResolvedConnection {
            connection_id,
            handle,
            terminal_session_id: runtime.terminal_session_id,
            sftp_session_id: runtime.sftp_session_id,
        })
    }

    async fn wait_for_active(
        &self,
        connection_id: &str,
        max_wait: Duration,
    ) -> Result<(), RouteError> {
        let result = timeout(max_wait, async {
            loop {
                let Some(handle) = self.registry.get(connection_id) else {
                    return Err(RouteError::NotConnected(connection_id.to_string()));
                };
                match handle.state() {
                    ConnectionState::Active | ConnectionState::Idle => return Ok(()),
                    ConnectionState::Error(error) => {
                        return Err(RouteError::ConnectionError(error));
                    }
                    ConnectionState::Disconnecting | ConnectionState::Disconnected => {
                        return Err(RouteError::NotConnected(connection_id.to_string()));
                    }
                    ConnectionState::LinkDown => {
                        return Err(RouteError::NotConnected(format!(
                            "Connection {connection_id} is link_down"
                        )));
                    }
                    ConnectionState::Connecting | ConnectionState::Reconnecting => {
                        sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => Err(RouteError::ConnectionTimeout(format!(
                "Timed out waiting for connection {connection_id} to become active ({max_wait:?})"
            ))),
        }
    }

    fn require_resolvable_state(
        &self,
        node_id: &NodeId,
        connection: &ConnectionInfo,
    ) -> Result<(), RouteError> {
        match &connection.state {
            ConnectionState::Active | ConnectionState::Idle => Ok(()),
            ConnectionState::Connecting | ConnectionState::Reconnecting => {
                Err(RouteError::ConnectionTimeout(format!(
                    "Connection {} for node {} is still {:?}",
                    connection.connection_id, node_id.0, connection.state
                )))
            }
            ConnectionState::Error(error) => Err(RouteError::ConnectionError(error.clone())),
            ConnectionState::LinkDown => Err(RouteError::NotConnected(format!(
                "Node {} connection {} is link_down",
                node_id.0, connection.connection_id
            ))),
            ConnectionState::Disconnecting | ConnectionState::Disconnected => {
                Err(RouteError::NotConnected(node_id.0.clone()))
            }
        }
    }
}

fn readiness_for_connection(connection: &ConnectionInfo) -> NodeReadiness {
    readiness_for_connection_state(&connection.state)
}

fn readiness_for_connection_state(state: &ConnectionState) -> NodeReadiness {
    match state {
        ConnectionState::Active | ConnectionState::Idle => NodeReadiness::Ready,
        ConnectionState::Connecting | ConnectionState::Reconnecting => NodeReadiness::Connecting,
        ConnectionState::Error(_) | ConnectionState::LinkDown => NodeReadiness::Error,
        ConnectionState::Disconnecting | ConnectionState::Disconnected => {
            NodeReadiness::Disconnected
        }
    }
}

fn sftp_route_error(prefix: &str, error: SftpError) -> RouteError {
    RouteError::CapabilityUnavailable(format!("{prefix}: {error}"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_node_to_shared_connection() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node = NodeId::new("node-a");
        let config = SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node.clone(), config.clone());
        let terminal = registry.acquire(config, ConnectionConsumer::Terminal("term-a".into()));
        router
            .bind_connection(&node, terminal.connection_id().to_string())
            .unwrap();
        router
            .bind_terminal_session(&node, "term-a".to_string())
            .unwrap();

        let resolved = router
            .acquire_connection(&node, ConnectionConsumer::NodeRouter("node-a".into()))
            .unwrap();
        let state = router.node_state(&node).unwrap();

        assert_eq!(state.state.readiness, NodeReadiness::Ready);
        assert_eq!(resolved.terminal_session_id.as_deref(), Some("term-a"));
        assert!(!resolved.connection_id.is_empty());
    }

    #[test]
    fn terminal_url_tracks_bound_endpoint() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry);
        let node = NodeId::new("node-a");
        router.upsert_node(node.clone(), SshConfig::password("host", 22, "me", "pw"));

        let endpoint = TerminalEndpoint {
            ws_port: 0,
            ws_token: "native-terminal-term-a".to_string(),
            session_id: "term-a".to_string(),
        };
        router
            .bind_terminal_endpoint(&node, endpoint.clone())
            .unwrap();

        assert_eq!(router.terminal_url(&node).unwrap(), endpoint);

        router.unbind_terminal_session(&node, "term-a").unwrap();
        assert!(matches!(
            router.terminal_url(&node),
            Err(RouteError::NotConnected(_))
        ));
    }

    #[test]
    fn runtime_tree_snapshot_preserves_origin_and_topology() {
        let store = NodeRuntimeStore::default();
        let root = NodeId::new("root");
        let child = NodeId::new("child");
        store.upsert_node_with_origin(
            root.clone(),
            SshConfig::password("jump", 22, "me", "pw"),
            NodeOrigin::ManualPreset {
                saved_connection_id: "saved-a".to_string(),
                hop_index: 0,
            },
        );
        store
            .upsert_child_node_with_origin(
                root.clone(),
                child.clone(),
                SshConfig::password("target", 22, "me", "pw"),
                NodeOrigin::ManualPreset {
                    saved_connection_id: "saved-a".to_string(),
                    hop_index: 1,
                },
            )
            .unwrap();

        let snapshot = store.export_snapshot();
        let restored = NodeRuntimeStore::default();
        restored.apply_snapshot(snapshot).unwrap();

        let flat = restored.flatten();
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].id, "root");
        assert_eq!(flat[0].origin_type, "manual_preset");
        assert_eq!(flat[1].id, "child");
        assert_eq!(flat[1].parent_id.as_deref(), Some("root"));
        assert_eq!(restored.summary().max_depth, 1);
    }

    #[test]
    fn reconcile_runtime_tree_clears_missing_runtime_connection() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry);
        let node = NodeId::new("node-a");
        router
            .apply_tree_snapshot(NodeTreeSnapshot {
                version: 1,
                exported_at_ms: now_ms(),
                root_ids: vec![node.clone()],
                nodes: vec![NodeTreeSnapshotNode {
                    id: node.clone(),
                    parent_id: None,
                    children_ids: Vec::new(),
                    depth: 0,
                    config: SshConfig::password("host", 22, "me", "pw"),
                    origin: NodeOrigin::Direct,
                    state: NodeState {
                        readiness: NodeReadiness::Ready,
                        error: None,
                        sftp_ready: true,
                        sftp_cwd: Some("/home/me".to_string()),
                        ws_endpoint: Some(TerminalEndpoint {
                            ws_port: 0,
                            ws_token: "token".to_string(),
                            session_id: "term-a".to_string(),
                        }),
                    },
                    connection_id: Some("missing-connection".to_string()),
                    terminal_session_id: Some("term-a".to_string()),
                    sftp_session_id: Some("sftp-a".to_string()),
                    created_at_ms: now_ms(),
                    generation: 1,
                }],
            })
            .unwrap();

        router.reconcile_runtime_tree();
        let state = router.node_state(&node).unwrap();
        let snapshot = router.runtime_store().snapshot(&node).unwrap();

        assert_eq!(state.state.readiness, NodeReadiness::Disconnected);
        assert!(snapshot.connection_id.is_none());
        assert!(snapshot.terminal_session_id.is_none());
        assert!(snapshot.state.ws_endpoint.is_none());
    }

    #[test]
    fn acquiring_consumer_does_not_revive_link_down_connection() {
        let registry = SshConnectionRegistry::default();
        let router = NodeRouter::new(registry.clone());
        let node = NodeId::new("node-a");
        let config = SshConfig::password("host", 22, "me", "pw");
        router.upsert_node(node.clone(), config.clone());
        let terminal = registry.acquire(config, ConnectionConsumer::Terminal("term-a".into()));
        router
            .bind_connection(&node, terminal.connection_id().to_string())
            .unwrap();

        registry.mark_state(terminal.connection_id(), ConnectionState::LinkDown);

        assert!(matches!(
            router.acquire_connection(&node, ConnectionConsumer::PortForward("node:a".into())),
            Err(RouteError::NotConnected(_))
        ));
        assert_eq!(terminal.state(), ConnectionState::LinkDown);
    }
}
