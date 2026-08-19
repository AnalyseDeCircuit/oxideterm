// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionTopologyStatus {
    Connecting,
    Active,
    Idle,
    LinkDown,
    Reconnecting,
    Disconnecting,
    Disconnected,
    Error,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTopologyConsumerSummary {
    pub terminals: usize,
    pub sftp: usize,
    pub port_forwards: usize,
    pub ide: usize,
    pub node_router: usize,
    #[serde(default)]
    pub public_mcp: usize,
    pub other: usize,
}

impl ConnectionTopologyConsumerSummary {
    pub fn total(&self) -> usize {
        self.terminals
            .saturating_add(self.sftp)
            .saturating_add(self.port_forwards)
            .saturating_add(self.ide)
            .saturating_add(self.node_router)
            .saturating_add(self.public_mcp)
            .saturating_add(self.other)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTopologyNode {
    pub connection_id: String,
    pub parent_connection_id: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub status: ConnectionTopologyStatus,
    pub depth: usize,
    pub ref_count: u64,
    pub consumers: ConnectionTopologyConsumerSummary,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTopologyEdge {
    pub parent_connection_id: String,
    pub child_connection_id: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTopologySnapshot {
    pub nodes: Vec<ConnectionTopologyNode>,
    pub edges: Vec<ConnectionTopologyEdge>,
    pub root_count: usize,
    pub child_count: usize,
}

impl ConnectionTopologySnapshot {
    pub fn new(nodes: Vec<ConnectionTopologyNode>, edges: Vec<ConnectionTopologyEdge>) -> Self {
        let root_count = nodes
            .iter()
            .filter(|node| node.parent_connection_id.is_none())
            .count();
        let child_count = nodes.len().saturating_sub(root_count);
        Self {
            nodes,
            edges,
            root_count,
            child_count,
        }
    }
}
