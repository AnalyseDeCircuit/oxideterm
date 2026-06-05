// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! OxideTerm-owned Mermaid subset model.

#[derive(Clone, Debug, PartialEq)]
pub enum MermaidDiagram {
    Gantt(GanttDiagram),
    Graph(GraphDiagram),
    Pie(PieDiagram),
    Sequence(SequenceDiagram),
}

#[derive(Clone, Debug, PartialEq)]
pub struct GanttDiagram {
    pub title: Option<String>,
    pub sections: Vec<GanttSection>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GanttSection {
    pub label: String,
    pub tasks: Vec<GanttTask>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GanttTask {
    pub label: String,
    pub id: Option<String>,
    pub start_day: i32,
    pub end_day: i32,
    pub status: GanttTaskStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GanttTaskStatus {
    Normal,
    Active,
    Done,
    Critical,
    Milestone,
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
pub struct PieDiagram {
    pub title: Option<String>,
    pub show_data: bool,
    pub slices: Vec<PieSlice>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PieSlice {
    pub label: String,
    pub value: f64,
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
