// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! OxideTerm-owned Mermaid subset model.

#[derive(Clone, Debug, PartialEq)]
pub enum MermaidDiagram {
    Graph(GraphDiagram),
    Sequence(SequenceDiagram),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphDirection {
    TopDown,
    BottomTop,
    LeftRight,
    RightLeft,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphDiagram {
    pub direction: GraphDirection,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub subgraphs: Vec<GraphSubgraph>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphSubgraph {
    pub id: String,
    pub label: String,
    pub node_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub shape: GraphNodeShape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphNodeShape {
    Rectangle,
    Rounded,
    Decision,
    Circle,
    Stadium,
    Subroutine,
    Database,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub kind: GraphEdgeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphEdgeKind {
    Arrow,
    Line,
    DottedArrow,
    ThickArrow,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SequenceDiagram {
    pub participants: Vec<SequenceParticipant>,
    pub messages: Vec<SequenceMessage>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SequenceParticipant {
    pub id: String,
    pub label: String,
    pub kind: SequenceParticipantKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SequenceParticipantKind {
    Participant,
    Actor,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SequenceMessage {
    pub from: String,
    pub to: String,
    pub label: String,
    pub kind: SequenceMessageKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SequenceMessageKind {
    SolidArrow,
    DashedArrow,
    SolidLine,
}
