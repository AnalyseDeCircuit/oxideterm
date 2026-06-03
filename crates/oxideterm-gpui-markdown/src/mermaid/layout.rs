// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic layout for the supported Mermaid subset.

use std::collections::HashMap;

use crate::mermaid::model::{
    GraphDiagram, GraphDirection, GraphNode, GraphSubgraph, MermaidDiagram, SequenceDiagram,
};
use crate::options::MarkdownOptions;

const GRAPH_MARGIN: f32 = 28.0;
const GRAPH_NODE_MIN_WIDTH: f32 = 96.0;
const GRAPH_NODE_HEIGHT: f32 = 44.0;
const GRAPH_NODE_PADDING_X: f32 = 28.0;
const GRAPH_LAYER_GAP: f32 = 96.0;
const GRAPH_SLOT_GAP: f32 = 44.0;

const SEQUENCE_MARGIN_X: f32 = 32.0;
const SEQUENCE_TOP: f32 = 24.0;
const SEQUENCE_HEADER_HEIGHT: f32 = 38.0;
const SEQUENCE_MESSAGE_GAP: f32 = 58.0;
const SEQUENCE_MIN_SPACING: f32 = 150.0;

#[derive(Clone, Debug)]
pub struct LaidOutDiagram {
    pub width: f32,
    pub height: f32,
    pub kind: LaidOutDiagramKind,
}

#[derive(Clone, Debug)]
pub enum LaidOutDiagramKind {
    Graph(LaidOutGraph),
    Sequence(LaidOutSequence),
}

#[derive(Clone, Debug)]
pub struct LaidOutGraph {
    pub diagram: GraphDiagram,
    pub nodes: HashMap<String, NodeBox>,
    pub subgraphs: Vec<SubgraphBox>,
}

#[derive(Clone, Debug)]
pub struct NodeBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub struct SubgraphBox {
    pub id: String,
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub struct LaidOutSequence {
    pub diagram: SequenceDiagram,
    pub participants: HashMap<String, ParticipantBox>,
    pub message_y: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct ParticipantBox {
    pub center_x: f32,
    pub label_width: f32,
}

pub fn layout(diagram: MermaidDiagram, opts: &MarkdownOptions) -> LaidOutDiagram {
    match diagram {
        MermaidDiagram::Graph(graph) => layout_graph(graph, opts),
        MermaidDiagram::Sequence(sequence) => layout_sequence(sequence, opts),
    }
}

fn layout_graph(diagram: GraphDiagram, opts: &MarkdownOptions) -> LaidOutDiagram {
    let levels = graph_levels(&diagram);
    let max_level = levels.iter().copied().max().unwrap_or(0);
    let mut layers = vec![Vec::<usize>::new(); max_level + 1];
    for (index, level) in levels.iter().enumerate() {
        layers[*level].push(index);
    }
    order_layers_by_neighbor_median(&mut layers, &diagram);

    let node_widths: Vec<f32> = diagram
        .nodes
        .iter()
        .map(|node| graph_node_width(node, opts))
        .collect();
    let max_node_width = node_widths
        .iter()
        .copied()
        .fold(GRAPH_NODE_MIN_WIDTH, f32::max);
    let max_layer_len = layers.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let slot = max_node_width + GRAPH_SLOT_GAP;
    let layer_span = max_layer_len as f32 * slot;
    let primary_step = match diagram.direction {
        GraphDirection::TopDown | GraphDirection::BottomTop => GRAPH_NODE_HEIGHT + GRAPH_LAYER_GAP,
        GraphDirection::LeftRight | GraphDirection::RightLeft => max_node_width + GRAPH_LAYER_GAP,
    };

    let mut nodes = HashMap::new();
    for (level, layer) in layers.iter().enumerate() {
        let row_span = layer.len().max(1) as f32 * slot;
        let offset = (layer_span - row_span) * 0.5;
        for (slot_index, node_index) in layer.iter().enumerate() {
            let node = &diagram.nodes[*node_index];
            let width = node_widths[*node_index];
            let logical_level = match diagram.direction {
                GraphDirection::BottomTop | GraphDirection::RightLeft => max_level - level,
                GraphDirection::TopDown | GraphDirection::LeftRight => level,
            };
            let primary = GRAPH_MARGIN + logical_level as f32 * primary_step;
            let secondary = GRAPH_MARGIN + offset + slot_index as f32 * slot + slot * 0.5;
            let (x, y) = match diagram.direction {
                GraphDirection::TopDown | GraphDirection::BottomTop => {
                    (secondary - width * 0.5, primary)
                }
                GraphDirection::LeftRight | GraphDirection::RightLeft => {
                    (primary, secondary - GRAPH_NODE_HEIGHT * 0.5)
                }
            };
            nodes.insert(
                node.id.clone(),
                NodeBox {
                    x,
                    y,
                    width,
                    height: GRAPH_NODE_HEIGHT,
                },
            );
        }
    }

    let width = match diagram.direction {
        GraphDirection::TopDown | GraphDirection::BottomTop => GRAPH_MARGIN * 2.0 + layer_span,
        GraphDirection::LeftRight | GraphDirection::RightLeft => {
            GRAPH_MARGIN * 2.0 + max_level as f32 * primary_step + max_node_width
        }
    };
    let height = match diagram.direction {
        GraphDirection::TopDown | GraphDirection::BottomTop => {
            GRAPH_MARGIN * 2.0 + max_level as f32 * primary_step + GRAPH_NODE_HEIGHT
        }
        GraphDirection::LeftRight | GraphDirection::RightLeft => GRAPH_MARGIN * 2.0 + layer_span,
    };
    let subgraphs = diagram
        .subgraphs
        .iter()
        .filter_map(|subgraph| layout_subgraph(subgraph, &nodes, opts))
        .collect();

    LaidOutDiagram {
        width,
        height,
        kind: LaidOutDiagramKind::Graph(LaidOutGraph {
            diagram,
            nodes,
            subgraphs,
        }),
    }
}

fn order_layers_by_neighbor_median(layers: &mut [Vec<usize>], diagram: &GraphDiagram) {
    for level in 1..layers.len() {
        let previous_positions: HashMap<usize, usize> = layers[level - 1]
            .iter()
            .enumerate()
            .map(|(position, index)| (*index, position))
            .collect();
        layers[level].sort_by(|a, b| {
            let a_score = incoming_neighbor_score(*a, diagram, &previous_positions);
            let b_score = incoming_neighbor_score(*b, diagram, &previous_positions);
            a_score
                .partial_cmp(&b_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(b))
        });
    }
}

fn incoming_neighbor_score(
    node: usize,
    diagram: &GraphDiagram,
    previous_positions: &HashMap<usize, usize>,
) -> f32 {
    let mut positions = Vec::new();
    for edge in &diagram.edges {
        let Some(to) = node_index(diagram, &edge.to) else {
            continue;
        };
        if to != node {
            continue;
        }
        let Some(from) = node_index(diagram, &edge.from) else {
            continue;
        };
        if let Some(position) = previous_positions.get(&from) {
            positions.push(*position as f32);
        }
    }
    if positions.is_empty() {
        node as f32
    } else {
        positions.iter().sum::<f32>() / positions.len() as f32
    }
}

fn layout_subgraph(
    subgraph: &GraphSubgraph,
    nodes: &HashMap<String, NodeBox>,
    opts: &MarkdownOptions,
) -> Option<SubgraphBox> {
    let mut x1 = f32::MAX;
    let mut y1 = f32::MAX;
    let mut x2 = f32::MIN;
    let mut y2 = f32::MIN;
    for node_id in &subgraph.node_ids {
        let Some(node) = nodes.get(node_id) else {
            continue;
        };
        x1 = x1.min(node.x);
        y1 = y1.min(node.y);
        x2 = x2.max(node.x + node.width);
        y2 = y2.max(node.y + node.height);
    }
    if x1 == f32::MAX {
        return None;
    }

    let label_height = opts.base_font_size + 16.0;
    let padding = 18.0;
    Some(SubgraphBox {
        id: subgraph.id.clone(),
        label: subgraph.label.clone(),
        x: (x1 - padding).max(6.0),
        y: (y1 - padding - label_height).max(6.0),
        width: (x2 - x1) + padding * 2.0,
        height: (y2 - y1) + padding * 2.0 + label_height,
    })
}

fn graph_levels(diagram: &GraphDiagram) -> Vec<usize> {
    let mut indegree = vec![0usize; diagram.nodes.len()];
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); diagram.nodes.len()];
    for edge in &diagram.edges {
        let Some(from) = node_index(diagram, &edge.from) else {
            continue;
        };
        let Some(to) = node_index(diagram, &edge.to) else {
            continue;
        };
        outgoing[from].push(to);
        indegree[to] += 1;
    }

    let mut queue: Vec<usize> = indegree
        .iter()
        .enumerate()
        .filter_map(|(index, degree)| (*degree == 0).then_some(index))
        .collect();
    let mut cursor = 0;
    let mut order = Vec::new();
    while cursor < queue.len() {
        let node = queue[cursor];
        cursor += 1;
        order.push(node);
        for &to in &outgoing[node] {
            indegree[to] = indegree[to].saturating_sub(1);
            if indegree[to] == 0 {
                queue.push(to);
            }
        }
    }

    // Cycle nodes are appended in source order; back edges are kept visually
    // without attempting a full feedback-arc optimization.
    for index in 0..diagram.nodes.len() {
        if !order.contains(&index) {
            order.push(index);
        }
    }

    let position: HashMap<usize, usize> = order
        .iter()
        .enumerate()
        .map(|(position, index)| (*index, position))
        .collect();
    let mut levels = vec![0usize; diagram.nodes.len()];
    for &from in &order {
        for &to in &outgoing[from] {
            if position.get(&from) < position.get(&to) {
                levels[to] = levels[to].max(levels[from] + 1);
            }
        }
    }
    levels
}

fn node_index(diagram: &GraphDiagram, id: &str) -> Option<usize> {
    diagram.nodes.iter().position(|node| node.id == id)
}

fn graph_node_width(node: &GraphNode, opts: &MarkdownOptions) -> f32 {
    let text_width = node.label.chars().count() as f32 * opts.base_font_size * 0.62;
    (text_width + GRAPH_NODE_PADDING_X * 2.0).max(GRAPH_NODE_MIN_WIDTH)
}

fn layout_sequence(diagram: SequenceDiagram, opts: &MarkdownOptions) -> LaidOutDiagram {
    let max_label_width = diagram
        .participants
        .iter()
        .map(|participant| participant.label.chars().count() as f32 * opts.base_font_size * 0.62)
        .fold(0.0, f32::max);
    let spacing = (max_label_width + 80.0).max(SEQUENCE_MIN_SPACING);
    let count = diagram.participants.len().max(1);
    let width = SEQUENCE_MARGIN_X * 2.0 + spacing * count.saturating_sub(1) as f32;
    let height = SEQUENCE_TOP
        + SEQUENCE_HEADER_HEIGHT
        + diagram.messages.len() as f32 * SEQUENCE_MESSAGE_GAP
        + 44.0;

    let mut participants = HashMap::new();
    for (index, participant) in diagram.participants.iter().enumerate() {
        participants.insert(
            participant.id.clone(),
            ParticipantBox {
                center_x: SEQUENCE_MARGIN_X + index as f32 * spacing,
                label_width: (participant.label.chars().count() as f32
                    * opts.base_font_size
                    * 0.62
                    + 32.0)
                    .max(88.0),
            },
        );
    }
    let message_y = (0..diagram.messages.len())
        .map(|index| {
            SEQUENCE_TOP + SEQUENCE_HEADER_HEIGHT + 34.0 + index as f32 * SEQUENCE_MESSAGE_GAP
        })
        .collect();

    LaidOutDiagram {
        width,
        height,
        kind: LaidOutDiagramKind::Sequence(LaidOutSequence {
            diagram,
            participants,
            message_y,
        }),
    }
}

#[cfg(test)]
mod tests {
    use oxideterm_theme::default_tokens;

    use crate::mermaid::model::MermaidDiagram;
    use crate::mermaid::parser;
    use crate::options::MarkdownOptions;

    use super::*;

    #[test]
    fn lays_out_left_right_graph_by_node_width() {
        let tokens = default_tokens();
        let opts = MarkdownOptions::from_theme(&tokens);
        let MermaidDiagram::Graph(graph) =
            parser::parse("flowchart LR\nA[Long starting label] --> B[Long ending label]")
                .expect("graph should parse")
        else {
            panic!("expected graph");
        };

        let layout = layout_graph(graph, &opts);
        let LaidOutDiagramKind::Graph(graph) = layout.kind else {
            panic!("expected laid out graph");
        };
        let a = graph.nodes.get("A").expect("A should be placed");
        let b = graph.nodes.get("B").expect("B should be placed");

        assert!(b.x > a.x + a.width);
    }
}
