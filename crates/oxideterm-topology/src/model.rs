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
    pub other: usize,
}

impl ConnectionTopologyConsumerSummary {
    pub fn total(&self) -> usize {
        self.terminals
            .saturating_add(self.sftp)
            .saturating_add(self.port_forwards)
            .saturating_add(self.ide)
            .saturating_add(self.node_router)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_counts_roots_and_children_like_tauri_tree_projection() {
        let snapshot = ConnectionTopologySnapshot::new(
            vec![
                node("root", None),
                node("child", Some("root")),
                node("orphan", Some("missing")),
            ],
            vec![ConnectionTopologyEdge {
                parent_connection_id: "root".into(),
                child_connection_id: "child".into(),
            }],
        );

        assert_eq!(snapshot.root_count, 1);
        assert_eq!(snapshot.child_count, 2);
    }

    fn node(id: &str, parent: Option<&str>) -> ConnectionTopologyNode {
        ConnectionTopologyNode {
            connection_id: id.into(),
            parent_connection_id: parent.map(str::to_string),
            host: id.into(),
            port: 22,
            username: "me".into(),
            status: ConnectionTopologyStatus::Active,
            depth: usize::from(parent.is_some()),
            ref_count: 1,
            consumers: ConnectionTopologyConsumerSummary::default(),
        }
    }
}
