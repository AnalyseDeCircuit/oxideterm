// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{HashMap, HashSet};

use crate::{
    ConnectionTopologySnapshot, ConnectionTopologyStatus, TopologyViewStatus, matrix_view_status,
    matrix_visible,
};

// Tauri TopologyViewEnhanced THEME.node and force-layout defaults translated
// into a deterministic native layout so the graph shape is stable in GPUI tests.
pub const TOPOLOGY_NODE_WIDTH: f32 = 140.0;
pub const TOPOLOGY_NODE_HEIGHT: f32 = 50.0;
pub const TOPOLOGY_CANVAS_MIN_WIDTH: f32 = 800.0;
pub const TOPOLOGY_CANVAS_MIN_HEIGHT: f32 = 600.0;
pub const TOPOLOGY_ROOT_Y: f32 = 80.0;
pub const TOPOLOGY_DEPTH_GAP: f32 = 150.0;
pub const TOPOLOGY_LEAF_GAP: f32 = 180.0;
pub const TOPOLOGY_PADDING_X: f32 = 80.0;

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionTopologyLayout {
    pub nodes: Vec<TopologyLayoutNode>,
    pub edges: Vec<TopologyLayoutEdge>,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopologyLayoutNode {
    pub connection_id: String,
    pub name: String,
    pub host: String,
    pub status: ConnectionTopologyStatus,
    pub view_status: TopologyViewStatus,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TopologyLayoutEdge {
    pub parent_connection_id: String,
    pub child_connection_id: String,
    pub source_x: f32,
    pub source_y: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub source_status: TopologyViewStatus,
    pub target_status: TopologyViewStatus,
    pub active: bool,
}

impl ConnectionTopologyLayout {
    pub fn from_snapshot(snapshot: &ConnectionTopologySnapshot) -> Self {
        let visible_ids = snapshot
            .nodes
            .iter()
            .filter(|node| matrix_visible(node.status))
            .map(|node| node.connection_id.clone())
            .collect::<HashSet<_>>();
        let node_by_id = snapshot
            .nodes
            .iter()
            .filter(|node| visible_ids.contains(&node.connection_id))
            .map(|node| (node.connection_id.clone(), node))
            .collect::<HashMap<_, _>>();

        let mut children = HashMap::<String, Vec<String>>::new();
        let mut roots = Vec::<String>::new();
        for node in node_by_id.values() {
            if let Some(parent_id) = node.parent_connection_id.as_ref()
                && visible_ids.contains(parent_id)
            {
                children
                    .entry(parent_id.clone())
                    .or_default()
                    .push(node.connection_id.clone());
            } else {
                roots.push(node.connection_id.clone());
            }
        }
        roots.sort();
        for child_ids in children.values_mut() {
            child_ids.sort();
        }

        let mut positions = HashMap::<String, (f32, f32)>::new();
        let mut next_leaf = 0usize;
        let mut max_depth = 0usize;
        for root_id in &roots {
            place_node(
                root_id,
                0,
                &children,
                &mut positions,
                &mut next_leaf,
                &mut max_depth,
            );
        }

        let leaf_count = next_leaf.max(1);
        let content_width =
            TOPOLOGY_PADDING_X * 2.0 + leaf_count.saturating_sub(1) as f32 * TOPOLOGY_LEAF_GAP;
        let width = content_width.max(TOPOLOGY_CANVAS_MIN_WIDTH);
        let height = (TOPOLOGY_ROOT_Y + max_depth as f32 * TOPOLOGY_DEPTH_GAP + 180.0)
            .max(TOPOLOGY_CANVAS_MIN_HEIGHT);
        let x_offset = (width - content_width) / 2.0;

        let mut nodes = node_by_id
            .values()
            .filter_map(|node| {
                let (x, y) = *positions.get(&node.connection_id)?;
                Some(TopologyLayoutNode {
                    connection_id: node.connection_id.clone(),
                    name: format!("{}@{}", node.username, node.host),
                    host: node.host.clone(),
                    status: node.status,
                    view_status: matrix_view_status(node.status),
                    x: x + x_offset,
                    y,
                })
            })
            .collect::<Vec<_>>();
        nodes.sort_by(|a, b| a.y.total_cmp(&b.y).then_with(|| a.x.total_cmp(&b.x)));

        let mut edges = snapshot
            .edges
            .iter()
            .filter(|edge| {
                visible_ids.contains(&edge.parent_connection_id)
                    && visible_ids.contains(&edge.child_connection_id)
            })
            .filter_map(|edge| {
                let parent = node_by_id.get(&edge.parent_connection_id)?;
                let child = node_by_id.get(&edge.child_connection_id)?;
                let (source_x, source_y) = *positions.get(&edge.parent_connection_id)?;
                let (target_x, target_y) = *positions.get(&edge.child_connection_id)?;
                let source_status = matrix_view_status(parent.status);
                let target_status = matrix_view_status(child.status);
                Some(TopologyLayoutEdge {
                    parent_connection_id: edge.parent_connection_id.clone(),
                    child_connection_id: edge.child_connection_id.clone(),
                    source_x: source_x + x_offset,
                    source_y,
                    target_x: target_x + x_offset,
                    target_y,
                    source_status,
                    target_status,
                    active: source_status.is_connected() && target_status.is_connected(),
                })
            })
            .collect::<Vec<_>>();
        edges.sort_by(|a, b| {
            a.parent_connection_id
                .cmp(&b.parent_connection_id)
                .then_with(|| a.child_connection_id.cmp(&b.child_connection_id))
        });

        Self {
            nodes,
            edges,
            width,
            height,
        }
    }
}

fn place_node(
    node_id: &str,
    depth: usize,
    children: &HashMap<String, Vec<String>>,
    positions: &mut HashMap<String, (f32, f32)>,
    next_leaf: &mut usize,
    max_depth: &mut usize,
) -> f32 {
    *max_depth = (*max_depth).max(depth);
    let y = TOPOLOGY_ROOT_Y + depth as f32 * TOPOLOGY_DEPTH_GAP;
    let x = if let Some(child_ids) = children.get(node_id) {
        if child_ids.is_empty() {
            next_leaf_x(next_leaf)
        } else {
            let child_x = child_ids
                .iter()
                .map(|child_id| {
                    place_node(
                        child_id,
                        depth + 1,
                        children,
                        positions,
                        next_leaf,
                        max_depth,
                    )
                })
                .collect::<Vec<_>>();
            child_x.iter().copied().sum::<f32>() / child_x.len() as f32
        }
    } else {
        next_leaf_x(next_leaf)
    };
    positions.insert(node_id.to_string(), (x, y));
    x
}

fn next_leaf_x(next_leaf: &mut usize) -> f32 {
    let x = TOPOLOGY_PADDING_X + *next_leaf as f32 * TOPOLOGY_LEAF_GAP;
    *next_leaf += 1;
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ConnectionTopologyConsumerSummary, ConnectionTopologyEdge, ConnectionTopologyNode,
        ConnectionTopologySnapshot,
    };

    #[test]
    fn layout_builds_parent_child_edges_from_snapshot() {
        let snapshot = ConnectionTopologySnapshot::new(
            vec![
                node("root", None, ConnectionTopologyStatus::Active),
                node("child", Some("root"), ConnectionTopologyStatus::Idle),
            ],
            vec![ConnectionTopologyEdge {
                parent_connection_id: "root".into(),
                child_connection_id: "child".into(),
            }],
        );

        let layout = ConnectionTopologyLayout::from_snapshot(&snapshot);

        assert_eq!(layout.nodes.len(), 2);
        assert_eq!(layout.edges.len(), 1);
        assert!(layout.edges[0].active);
        assert_eq!(layout.nodes[0].connection_id, "root");
        assert_eq!(layout.nodes[1].connection_id, "child");
    }

    #[test]
    fn layout_filters_to_tauri_connected_and_connecting_matrix_nodes() {
        let snapshot = ConnectionTopologySnapshot::new(
            vec![
                node("connected", None, ConnectionTopologyStatus::Active),
                node("down", None, ConnectionTopologyStatus::LinkDown),
            ],
            Vec::new(),
        );

        let layout = ConnectionTopologyLayout::from_snapshot(&snapshot);

        assert_eq!(layout.nodes.len(), 1);
        assert_eq!(layout.nodes[0].connection_id, "connected");
    }

    fn node(
        id: &str,
        parent: Option<&str>,
        status: ConnectionTopologyStatus,
    ) -> ConnectionTopologyNode {
        ConnectionTopologyNode {
            connection_id: id.into(),
            parent_connection_id: parent.map(str::to_string),
            host: format!("{id}.internal"),
            port: 22,
            username: "me".into(),
            status,
            depth: usize::from(parent.is_some()),
            ref_count: 1,
            consumers: ConnectionTopologyConsumerSummary::default(),
        }
    }
}
