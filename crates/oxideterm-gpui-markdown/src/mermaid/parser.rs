// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Line-oriented parser for OxideTerm's Mermaid v0 subset.

use std::borrow::Cow;

use crate::mermaid::model::{
    GraphDiagram, GraphDirection, GraphEdge, GraphEdgeKind, GraphNode, GraphNodeShape,
    GraphSubgraph, MermaidDiagram, SequenceDiagram, SequenceMessage, SequenceMessageKind,
    SequenceParticipant, SequenceParticipantKind,
};

const MAX_MERMAID_STATEMENTS: usize = 240;
const MAX_GRAPH_NODES: usize = 180;
const MAX_GRAPH_EDGES: usize = 320;
const MAX_SEQUENCE_PARTICIPANTS: usize = 80;
const MAX_SEQUENCE_MESSAGES: usize = 240;

pub fn parse(source: &str) -> Result<MermaidDiagram, String> {
    let statements = collect_statements(source)?;
    let Some(header) = statements.first() else {
        return Err("empty Mermaid diagram".to_string());
    };

    let mut words = header.split_whitespace();
    match words.next().unwrap_or_default() {
        "graph" | "flowchart" => parse_graph(header, &statements[1..]),
        "sequenceDiagram" => parse_sequence(&statements[1..]),
        other => Err(format!("unsupported Mermaid diagram type: {other}")),
    }
}

fn collect_statements(source: &str) -> Result<Vec<String>, String> {
    let mut statements = Vec::new();
    for (index, line) in source.lines().enumerate() {
        if index >= MAX_MERMAID_STATEMENTS {
            return Err("Mermaid diagram is too large".to_string());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") && !trimmed.starts_with("%%{") {
            continue;
        }
        if trimmed.starts_with("%%{") {
            return Err("Mermaid directives are not supported".to_string());
        }
        for part in trimmed.split(';') {
            let statement = part.trim();
            if !statement.is_empty() {
                statements.push(statement.to_string());
            }
        }
    }
    Ok(statements)
}

fn parse_graph(header: &str, body: &[String]) -> Result<MermaidDiagram, String> {
    let mut words = header.split_whitespace();
    let _kind = words.next();
    let direction = match words.next().unwrap_or("TD") {
        "TD" | "TB" => GraphDirection::TopDown,
        "BT" => GraphDirection::BottomTop,
        "LR" => GraphDirection::LeftRight,
        "RL" => GraphDirection::RightLeft,
        other => return Err(format!("unsupported graph direction: {other}")),
    };
    if words.next().is_some() {
        return Err("graph header contains unsupported tokens".to_string());
    }

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut subgraphs = Vec::new();
    let mut subgraph_stack = Vec::<usize>::new();

    for statement in body {
        let statement = normalize_graph_statement(statement);
        let statement = statement.as_ref();
        if let Some(header) = statement.strip_prefix("subgraph ") {
            if !subgraph_stack.is_empty() {
                return Err("nested subgraphs are not supported".to_string());
            }
            let subgraph = parse_subgraph_header(header.trim(), subgraphs.len())?;
            subgraphs.push(subgraph);
            subgraph_stack.push(subgraphs.len() - 1);
            continue;
        }
        if statement == "end" {
            if subgraph_stack.pop().is_none() {
                return Err("subgraph end without matching subgraph".to_string());
            }
            continue;
        }

        reject_unsupported_graph_statement(statement)?;
        let parsed_edges = parse_graph_edges(statement, &mut nodes)?;
        if let Some(&subgraph_index) = subgraph_stack.last() {
            for edge in &parsed_edges {
                add_subgraph_node(&mut subgraphs[subgraph_index], &edge.from);
                add_subgraph_node(&mut subgraphs[subgraph_index], &edge.to);
            }
        }
        edges.extend(parsed_edges);
        if nodes.len() > MAX_GRAPH_NODES || edges.len() > MAX_GRAPH_EDGES {
            return Err("Mermaid graph is too large".to_string());
        }
    }

    if !subgraph_stack.is_empty() {
        return Err("unterminated subgraph".to_string());
    }

    if nodes.is_empty() || edges.is_empty() {
        return Err("graph contains no supported edges".to_string());
    }

    Ok(MermaidDiagram::Graph(GraphDiagram {
        direction,
        nodes,
        edges,
        subgraphs,
    }))
}

fn normalize_graph_statement(input: &str) -> Cow<'_, str> {
    // AI answers sometimes emit a typography arrow in otherwise Mermaid-like graph code.
    if input.contains('→') || input.contains('⇒') {
        Cow::Owned(input.replace('⇒', "==>").replace('→', "-->"))
    } else {
        Cow::Borrowed(input)
    }
}

fn reject_unsupported_graph_statement(statement: &str) -> Result<(), String> {
    let keyword = statement
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match keyword.as_str() {
        "click" | "classdef" | "class" | "style" | "linkstyle" => {
            Err(format!("unsupported graph statement: {keyword}"))
        }
        _ => Ok(()),
    }
}

fn parse_subgraph_header(input: &str, fallback_index: usize) -> Result<GraphSubgraph, String> {
    if input.is_empty() {
        return Err("subgraph label is empty".to_string());
    }
    let parsed = parse_graph_node_ref(input).unwrap_or_else(|_| ParsedGraphNode {
        id: format!("subgraph_{fallback_index}"),
        label: input.to_string(),
        shape: GraphNodeShape::Rectangle,
        explicit_label: true,
    });
    Ok(GraphSubgraph {
        id: parsed.id,
        label: parsed.label,
        node_ids: Vec::new(),
    })
}

fn add_subgraph_node(subgraph: &mut GraphSubgraph, id: &str) {
    if !subgraph.node_ids.iter().any(|node_id| node_id == id) {
        subgraph.node_ids.push(id.to_string());
    }
}

fn parse_graph_edges(
    statement: &str,
    nodes: &mut Vec<GraphNode>,
) -> Result<Vec<GraphEdge>, String> {
    let Some((operator, index, kind)) = find_graph_operator(statement) else {
        return Err(format!("unsupported graph edge: {statement}"));
    };

    let mut edges = Vec::new();
    let mut current_sources = parse_graph_node_group(&statement[..index], nodes)?;
    let mut rest = statement[index..].trim();
    let mut current_operator = operator;
    let mut current_kind = kind;

    loop {
        rest = rest[current_operator.len()..].trim_start();
        let (label, after_label) = parse_optional_edge_label(rest)?;
        rest = after_label.trim_start();
        let next = find_graph_operator(rest);
        let (target_segment, next_tail) = if let Some((next_operator, next_index, next_kind)) = next
        {
            (
                rest[..next_index].trim(),
                Some((next_operator, next_kind, rest[next_index..].trim())),
            )
        } else {
            (rest.trim(), None)
        };
        let targets = parse_graph_node_group(target_segment, nodes)?;
        for from in &current_sources {
            for to in &targets {
                edges.push(GraphEdge {
                    from: from.id.clone(),
                    to: to.id.clone(),
                    label: label.clone(),
                    kind: current_kind,
                });
            }
        }

        let Some((next_operator, next_kind, next_rest)) = next_tail else {
            break;
        };
        current_sources = targets;
        current_operator = next_operator;
        current_kind = next_kind;
        rest = next_rest;
    }

    Ok(edges)
}

fn parse_optional_edge_label(input: &str) -> Result<(Option<String>, &str), String> {
    if let Some(rest) = input.strip_prefix('|') {
        let Some(end) = rest.find('|') else {
            return Err("unterminated graph edge label".to_string());
        };
        let label = rest[..end].trim();
        return Ok((
            (!label.is_empty()).then(|| label.to_string()),
            &rest[end + 1..],
        ));
    }
    Ok((None, input))
}

fn find_graph_operator(statement: &str) -> Option<(&'static str, usize, GraphEdgeKind)> {
    [
        ("-.->", GraphEdgeKind::DottedArrow),
        ("-->", GraphEdgeKind::Arrow),
        ("==>", GraphEdgeKind::ThickArrow),
        ("---", GraphEdgeKind::Line),
    ]
    .into_iter()
    .filter_map(|(operator, kind)| {
        statement
            .find(operator)
            .map(|index| (operator, index, kind))
    })
    .min_by_key(|(_, index, _)| *index)
}

fn parse_graph_node_group(
    input: &str,
    nodes: &mut Vec<GraphNode>,
) -> Result<Vec<ParsedGraphNode>, String> {
    let refs = split_graph_node_group(input)?;
    if refs.is_empty() {
        return Err("empty graph node group".to_string());
    }

    let mut parsed_nodes = Vec::new();
    for node_ref in refs {
        let parsed = parse_graph_node_ref(node_ref)?;
        upsert_graph_node(nodes, &parsed);
        parsed_nodes.push(parsed);
    }
    Ok(parsed_nodes)
}

fn split_graph_node_group(input: &str) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    for (index, ch) in input.char_indices() {
        match ch {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            '&' if depth == 0 => {
                let part = input[start..index].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                start = index + ch.len_utf8();
            }
            _ => {}
        }
        if depth < 0 {
            return Err(format!("unbalanced graph node group: {input}"));
        }
    }
    if depth != 0 {
        return Err(format!("unbalanced graph node group: {input}"));
    }
    let part = input[start..].trim();
    if !part.is_empty() {
        parts.push(part);
    }
    Ok(parts)
}

struct ParsedGraphNode {
    id: String,
    label: String,
    shape: GraphNodeShape,
    explicit_label: bool,
}

fn parse_graph_node_ref(input: &str) -> Result<ParsedGraphNode, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("empty graph node reference".to_string());
    }

    if let Some(parsed) = parse_wrapped_graph_node(input, "([", "])", GraphNodeShape::Stadium)? {
        return Ok(parsed);
    }
    if let Some(parsed) = parse_wrapped_graph_node(input, "((", "))", GraphNodeShape::Circle)? {
        return Ok(parsed);
    }
    if let Some(parsed) = parse_wrapped_graph_node(input, "[[", "]]", GraphNodeShape::Subroutine)? {
        return Ok(parsed);
    }
    if let Some(parsed) = parse_wrapped_graph_node(input, "[(", ")]", GraphNodeShape::Database)? {
        return Ok(parsed);
    }

    let shape = input.char_indices().find_map(|(index, ch)| match ch {
        '[' => Some((index, "]", GraphNodeShape::Rectangle)),
        '(' => Some((index, ")", GraphNodeShape::Rounded)),
        '{' => Some((index, "}", GraphNodeShape::Decision)),
        _ => None,
    });

    if let Some((open_index, close, shape)) = shape {
        if !input.ends_with(close) {
            return Err(format!("unterminated graph node label: {input}"));
        }
        let id = input[..open_index].trim();
        validate_identifier(id, "graph node")?;
        let label = input[open_index + 1..input.len() - close.len()].trim();
        return Ok(ParsedGraphNode {
            id: id.to_string(),
            label: if label.is_empty() {
                id.to_string()
            } else {
                label.to_string()
            },
            shape,
            explicit_label: true,
        });
    }

    validate_identifier(input, "graph node")?;
    Ok(ParsedGraphNode {
        id: input.to_string(),
        label: input.to_string(),
        shape: GraphNodeShape::Rectangle,
        explicit_label: false,
    })
}

fn parse_wrapped_graph_node(
    input: &str,
    open: &str,
    close: &str,
    shape: GraphNodeShape,
) -> Result<Option<ParsedGraphNode>, String> {
    let Some(open_index) = input.find(open) else {
        return Ok(None);
    };
    if !input.ends_with(close) {
        return Err(format!("unterminated graph node label: {input}"));
    }
    let id = input[..open_index].trim();
    validate_identifier(id, "graph node")?;
    let label = input[open_index + open.len()..input.len() - close.len()].trim();
    Ok(Some(ParsedGraphNode {
        id: id.to_string(),
        label: if label.is_empty() {
            id.to_string()
        } else {
            label.to_string()
        },
        shape,
        explicit_label: true,
    }))
}

fn upsert_graph_node(nodes: &mut Vec<GraphNode>, parsed: &ParsedGraphNode) {
    if let Some(existing) = nodes.iter_mut().find(|node| node.id == parsed.id) {
        if parsed.explicit_label && existing.label == existing.id {
            existing.label = parsed.label.clone();
            existing.shape = parsed.shape;
        }
        return;
    }

    nodes.push(GraphNode {
        id: parsed.id.clone(),
        label: parsed.label.clone(),
        shape: parsed.shape,
    });
}

fn parse_sequence(body: &[String]) -> Result<MermaidDiagram, String> {
    let mut participants = Vec::new();
    let mut messages = Vec::new();

    for statement in body {
        let statement = normalize_sequence_statement(statement);
        let statement = statement.as_ref();
        if let Some(rest) = statement.strip_prefix("participant ") {
            upsert_sequence_participant(
                &mut participants,
                rest.trim(),
                SequenceParticipantKind::Participant,
            )?;
            continue;
        }
        if let Some(rest) = statement.strip_prefix("actor ") {
            upsert_sequence_participant(
                &mut participants,
                rest.trim(),
                SequenceParticipantKind::Actor,
            )?;
            continue;
        }

        reject_unsupported_sequence_statement(statement)?;
        let message = parse_sequence_message(statement, &mut participants)?;
        messages.push(message);
        if participants.len() > MAX_SEQUENCE_PARTICIPANTS || messages.len() > MAX_SEQUENCE_MESSAGES
        {
            return Err("Mermaid sequence diagram is too large".to_string());
        }
    }

    if participants.is_empty() || messages.is_empty() {
        return Err("sequence diagram contains no supported messages".to_string());
    }

    Ok(MermaidDiagram::Sequence(SequenceDiagram {
        participants,
        messages,
    }))
}

fn normalize_sequence_statement(input: &str) -> Cow<'_, str> {
    // Keep sequence shorthand small: a plain typography arrow maps to the supported line message.
    if input.contains('→') {
        Cow::Owned(input.replace('→', "->"))
    } else {
        Cow::Borrowed(input)
    }
}

fn reject_unsupported_sequence_statement(statement: &str) -> Result<(), String> {
    let keyword = statement
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches(':')
        .to_ascii_lowercase();
    match keyword.as_str() {
        "activate" | "deactivate" | "note" | "alt" | "else" | "opt" | "loop" | "par"
        | "critical" | "break" | "rect" | "end" => {
            Err(format!("unsupported sequence statement: {keyword}"))
        }
        _ => Ok(()),
    }
}

fn parse_sequence_message(
    statement: &str,
    participants: &mut Vec<SequenceParticipant>,
) -> Result<SequenceMessage, String> {
    let Some((operator, index, kind)) = find_sequence_operator(statement) else {
        return Err(format!("unsupported sequence message: {statement}"));
    };
    let from = statement[..index].trim();
    validate_identifier(from, "sequence participant")?;
    let right = statement[index + operator.len()..].trim();
    let Some((to, label)) = right.split_once(':') else {
        return Err("sequence message is missing ':' label separator".to_string());
    };
    let to = to.trim();
    validate_identifier(to, "sequence participant")?;

    upsert_sequence_participant(participants, from, SequenceParticipantKind::Participant)?;
    upsert_sequence_participant(participants, to, SequenceParticipantKind::Participant)?;

    Ok(SequenceMessage {
        from: from.to_string(),
        to: to.to_string(),
        label: label.trim().to_string(),
        kind,
    })
}

fn find_sequence_operator(statement: &str) -> Option<(&'static str, usize, SequenceMessageKind)> {
    [
        ("-->>", SequenceMessageKind::DashedArrow),
        ("->>", SequenceMessageKind::SolidArrow),
        ("->", SequenceMessageKind::SolidLine),
    ]
    .into_iter()
    .filter_map(|(operator, kind)| {
        statement
            .find(operator)
            .map(|index| (operator, index, kind))
    })
    .min_by_key(|(_, index, _)| *index)
}

fn upsert_sequence_participant(
    participants: &mut Vec<SequenceParticipant>,
    id: &str,
    kind: SequenceParticipantKind,
) -> Result<(), String> {
    validate_identifier(id, "sequence participant")?;
    if let Some(existing) = participants
        .iter_mut()
        .find(|participant| participant.id == id)
    {
        if kind == SequenceParticipantKind::Actor {
            existing.kind = kind;
        }
        return Ok(());
    }

    participants.push(SequenceParticipant {
        id: id.to_string(),
        label: id.to_string(),
        kind,
    });
    Ok(())
}

fn validate_identifier(id: &str, role: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err(format!("{role} identifier is empty"));
    }
    if id.chars().any(char::is_whitespace) {
        return Err(format!("{role} identifier contains whitespace: {id}"));
    }
    if id
        .chars()
        .any(|ch| matches!(ch, '[' | ']' | '(' | ')' | '{' | '}' | '|' | ':' | '&'))
    {
        return Err(format!(
            "{role} identifier contains unsupported characters: {id}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_graph_edges_and_labels() {
        let MermaidDiagram::Graph(graph) =
            parse("flowchart TD\nA[Start] -->|yes| B{Decision}\nB -.-> C(Done)")
                .expect("graph should parse")
        else {
            panic!("expected graph");
        };

        assert_eq!(graph.direction, GraphDirection::TopDown);
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.nodes[0].label, "Start");
        assert_eq!(graph.nodes[1].shape, GraphNodeShape::Decision);
        assert_eq!(graph.edges[0].label.as_deref(), Some("yes"));
        assert_eq!(graph.edges[1].kind, GraphEdgeKind::DottedArrow);
    }

    #[test]
    fn parses_graph_direction_variants_chain_edges_and_node_groups() {
        let MermaidDiagram::Graph(graph) =
            parse("graph RL\nA([Start]) & B((Alt)) --> C[[Work]] --> D[(DB)]")
                .expect("graph should parse")
        else {
            panic!("expected graph");
        };

        assert_eq!(graph.direction, GraphDirection::RightLeft);
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.nodes[0].shape, GraphNodeShape::Stadium);
        assert_eq!(graph.nodes[1].shape, GraphNodeShape::Circle);
        assert_eq!(graph.nodes[2].shape, GraphNodeShape::Subroutine);
        assert_eq!(graph.nodes[3].shape, GraphNodeShape::Database);
    }

    #[test]
    fn parses_graph_typography_arrows_from_ai_output() {
        let MermaidDiagram::Graph(graph) =
            parse("graph TD\nA[开始] → B{想喝咖啡?}\nB →|是| C[磨豆子]")
                .expect("graph should parse")
        else {
            panic!("expected graph");
        };

        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.edges[0].from, "A");
        assert_eq!(graph.edges[0].to, "B");
        assert_eq!(graph.edges[1].label.as_deref(), Some("是"));
        assert_eq!(graph.edges[1].to, "C");
    }

    #[test]
    fn parses_one_level_subgraphs() {
        let MermaidDiagram::Graph(graph) =
            parse("flowchart TB\nsubgraph group[Group]\nA --> B\nend\nB --> C")
                .expect("graph should parse")
        else {
            panic!("expected graph");
        };

        assert_eq!(graph.subgraphs.len(), 1);
        assert_eq!(graph.subgraphs[0].label, "Group");
        assert_eq!(graph.subgraphs[0].node_ids, vec!["A", "B"]);
    }

    #[test]
    fn parses_sequence_participants_and_messages() {
        let MermaidDiagram::Sequence(sequence) =
            parse("sequenceDiagram\nparticipant A\nactor B\nA->>B: hello\nB-->>A: ok")
                .expect("sequence should parse")
        else {
            panic!("expected sequence");
        };

        assert_eq!(sequence.participants.len(), 2);
        assert_eq!(
            sequence.participants[1].kind,
            SequenceParticipantKind::Actor
        );
        assert_eq!(sequence.messages[0].label, "hello");
        assert_eq!(sequence.messages[1].kind, SequenceMessageKind::DashedArrow);
    }

    #[test]
    fn parses_sequence_typography_arrow_messages() {
        let MermaidDiagram::Sequence(sequence) =
            parse("sequenceDiagram\nA → B: hello").expect("sequence should parse")
        else {
            panic!("expected sequence");
        };

        assert_eq!(sequence.messages[0].from, "A");
        assert_eq!(sequence.messages[0].to, "B");
        assert_eq!(sequence.messages[0].kind, SequenceMessageKind::SolidLine);
    }

    #[test]
    fn rejects_unsupported_syntax() {
        let error = parse("flowchart TD\nclassDef red fill:#f00").unwrap_err();
        assert!(error.contains("unsupported graph statement"));
    }

    #[test]
    fn rejects_oversized_diagrams() {
        let mut source = String::from("sequenceDiagram\n");
        for index in 0..=MAX_SEQUENCE_MESSAGES {
            source.push_str(&format!("A->>B: {index}\n"));
        }

        assert!(parse(&source).unwrap_err().contains("too large"));
    }
}
