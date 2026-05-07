// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use dashmap::DashMap;
use oxideterm_sftp::{SftpError, SftpSession};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

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
struct NodeRoute {
    config: SshConfig,
    connection_id: Option<String>,
    terminal_session_id: Option<String>,
    sftp_session_id: Option<String>,
    state: NodeState,
    generation: u64,
}

#[derive(Clone, Debug)]
pub struct NodeRouter {
    registry: SshConnectionRegistry,
    nodes: DashMap<NodeId, NodeRoute>,
    connection_nodes: DashMap<String, NodeId>,
}

impl NodeRouter {
    pub fn new(registry: SshConnectionRegistry) -> Self {
        Self {
            registry,
            nodes: DashMap::new(),
            connection_nodes: DashMap::new(),
        }
    }

    pub fn upsert_node(&self, node_id: NodeId, config: SshConfig) {
        self.nodes
            .entry(node_id)
            .and_modify(|route| {
                route.config = config.clone();
                route.generation += 1;
            })
            .or_insert_with(|| NodeRoute {
                config,
                connection_id: None,
                terminal_session_id: None,
                sftp_session_id: None,
                state: NodeState::default(),
                generation: 0,
            });
    }

    pub fn resolve_connection(&self, node_id: &NodeId) -> Result<ResolvedConnection, RouteError> {
        let route = self
            .nodes
            .get(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        let connection_id = route
            .connection_id
            .clone()
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        let terminal_session_id = route.terminal_session_id.clone();
        let sftp_session_id = route.sftp_session_id.clone();
        drop(route);

        let handle = self
            .registry
            .get(&connection_id)
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        self.require_resolvable_state(node_id, &handle.info())?;
        Ok(ResolvedConnection {
            connection_id,
            handle,
            terminal_session_id,
            sftp_session_id,
        })
    }

    pub fn acquire_connection(
        &self,
        node_id: &NodeId,
        consumer: ConnectionConsumer,
    ) -> Result<ResolvedConnection, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        let connection_id = route
            .connection_id
            .clone()
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
        route.generation += 1;
        route.state.readiness = readiness_for_connection(&handle.info());
        route.state.error = None;
        let terminal_session_id = route.terminal_session_id.clone();
        let sftp_session_id = route.sftp_session_id.clone();
        drop(route);

        self.connection_nodes
            .insert(connection_id.clone(), node_id.clone());
        self.require_resolvable_state(node_id, &handle.info())?;
        Ok(ResolvedConnection {
            connection_id,
            handle,
            terminal_session_id,
            sftp_session_id,
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
        route.state.readiness = readiness_for_connection(&connection);
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

    pub fn bind_terminal_session(
        &self,
        node_id: &NodeId,
        session_id: impl Into<String>,
    ) -> Result<(), RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.terminal_session_id = Some(session_id.into());
        route.generation += 1;
        Ok(())
    }

    pub fn unbind_terminal_session(
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
            route.generation += 1;
        }
        Ok(())
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

    pub fn bind_sftp_session(
        &self,
        node_id: &NodeId,
        session_id: impl Into<String>,
        cwd: Option<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.sftp_session_id = Some(session_id.into());
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

    pub async fn acquire_sftp(
        &self,
        node_id: &NodeId,
    ) -> Result<Arc<Mutex<SftpSession>>, RouteError> {
        let resolved = self.resolve_connection(node_id)?;
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
        self.set_sftp_ready(node_id, true, cwd)?;
        Ok(session)
    }

    pub async fn acquire_transfer_sftp(&self, node_id: &NodeId) -> Result<SftpSession, RouteError> {
        let resolved = self.resolve_connection(node_id)?;
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
        let resolved = self.resolve_connection(node_id)?;
        let had_sftp = resolved.handle.invalidate_sftp().await;
        if had_sftp {
            let _ = self
                .registry
                .mark_sftp_session(&resolved.connection_id, false, None);
            self.set_sftp_ready(node_id, false, None)?;
        }

        let AcquiredSftpMeta { session, cwd, .. } = resolved
            .handle
            .acquire_sftp_with_meta()
            .await
            .map_err(|error| sftp_route_error("SFTP rebuild failed", error))?;
        let _ = self
            .registry
            .mark_sftp_session(&resolved.connection_id, true, cwd.clone());
        self.set_sftp_ready(node_id, true, cwd)?;
        Ok(session)
    }

    pub fn node_state(&self, node_id: &NodeId) -> Result<NodeStateSnapshot, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        if let Some(connection_id) = route.connection_id.clone() {
            if let Some(handle) = self.registry.get(&connection_id) {
                let info = handle.info();
                route.state.readiness = readiness_for_connection(&info);
                route.state.error = match &info.state {
                    ConnectionState::Error(error) => Some(error.clone()),
                    ConnectionState::LinkDown => Some("Link down".to_string()),
                    _ => None,
                };
                if let Some(sftp_state) = self.registry.sftp_session_state(&connection_id) {
                    route.state.sftp_ready = sftp_state.ready;
                    route.state.sftp_cwd = sftp_state.cwd;
                }
            } else {
                route.state.readiness = NodeReadiness::Disconnected;
                route.state.error = None;
                route.state.sftp_ready = false;
                route.state.sftp_cwd = None;
            }
        }
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

    pub fn sync_connection_state_by_connection_id(
        &self,
        connection: &ConnectionInfo,
        reason: impl Into<String>,
    ) -> Option<NodeStateEvent> {
        let node_id = self.node_id_for_connection(&connection.connection_id)?;
        self.sync_connection_state(&node_id, connection, reason)
            .ok()
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
}

fn readiness_for_connection(connection: &ConnectionInfo) -> NodeReadiness {
    match &connection.state {
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
