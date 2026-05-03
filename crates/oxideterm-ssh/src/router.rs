// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ConnectionConsumer, ConnectionInfo, ConnectionState, SshConfig, SshConnectionHandle,
    SshConnectionRegistry,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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
struct NodeRoute {
    config: SshConfig,
    connection_id: Option<String>,
    state: NodeState,
    generation: u64,
}

#[derive(Clone, Debug)]
pub struct NodeRouter {
    registry: SshConnectionRegistry,
    nodes: DashMap<NodeId, NodeRoute>,
}

impl NodeRouter {
    pub fn new(registry: SshConnectionRegistry) -> Self {
        Self {
            registry,
            nodes: DashMap::new(),
        }
    }

    pub fn upsert_node(&self, node_id: NodeId, config: SshConfig) {
        self.nodes.insert(
            node_id,
            NodeRoute {
                config,
                connection_id: None,
                state: NodeState::default(),
                generation: 0,
            },
        );
    }

    pub fn resolve_connection(
        &self,
        node_id: &NodeId,
        consumer: ConnectionConsumer,
    ) -> Result<SshConnectionHandle, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        let handle = self.registry.acquire(route.config.clone(), consumer);
        route.connection_id = Some(handle.connection_id().to_string());
        route.generation += 1;
        route.state.readiness = readiness_for_connection(&handle.info());
        route.state.error = None;
        Ok(handle)
    }

    pub fn node_state(&self, node_id: &NodeId) -> Result<NodeStateSnapshot, RouteError> {
        let route = self
            .nodes
            .get(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        Ok(NodeStateSnapshot {
            state: route.state.clone(),
            generation: route.generation,
        })
    }

    pub fn sync_connection_state(
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
}

fn readiness_for_connection(connection: &ConnectionInfo) -> NodeReadiness {
    match &connection.state {
        ConnectionState::Active | ConnectionState::Idle => NodeReadiness::Ready,
        ConnectionState::Connecting | ConnectionState::Reconnecting => NodeReadiness::Connecting,
        ConnectionState::Error(_) => NodeReadiness::Error,
        ConnectionState::LinkDown
        | ConnectionState::Disconnecting
        | ConnectionState::Disconnected => NodeReadiness::Disconnected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_node_to_shared_connection() {
        let router = NodeRouter::new(SshConnectionRegistry::default());
        let node = NodeId::new("node-a");
        router.upsert_node(node.clone(), SshConfig::password("host", 22, "me", "pw"));

        let handle = router
            .resolve_connection(&node, ConnectionConsumer::NodeRouter("node-a".into()))
            .unwrap();
        let state = router.node_state(&node).unwrap();

        assert_eq!(state.state.readiness, NodeReadiness::Ready);
        assert!(!handle.connection_id().is_empty());
    }
}
