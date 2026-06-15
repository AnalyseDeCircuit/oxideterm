// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeSet;

use tree_sitter::Node;

use crate::IndentGuide;

pub(crate) fn indent_guides(root: Node<'_>, source: &str, tab_size: usize) -> Vec<IndentGuide> {
    let mut guides = BTreeSet::new();
    collect_indent_guides(root, source, tab_size.max(1), &mut guides);
    guides
        .into_iter()
        .map(|(start_line, end_line, column)| IndentGuide {
            start_line,
            end_line,
            column,
        })
        .collect()
}

fn collect_indent_guides(
    node: Node<'_>,
    source: &str,
    tab_size: usize,
    guides: &mut BTreeSet<(usize, usize, usize)>,
) {
    if node.is_named() && is_indent_container_kind(node.kind()) {
        let start = node.start_position();
        let end = node.end_position();
        if end.row > start.row
            && let Some(column) = first_nested_line_indent(source, start.row, end.row, tab_size)
            && column > 0
        {
            // The rendered guide belongs to the syntactic container, not to
            // arbitrary alignment whitespace inside the contained lines.
            guides.insert((start.row, end.row, column));
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_indent_guides(child, source, tab_size, guides);
    }
}

fn is_indent_container_kind(kind: &str) -> bool {
    matches!(
        kind,
        "block"
            | "compound_statement"
            | "statement_block"
            | "declaration_list"
            | "field_declaration_list"
            | "class_body"
            | "enum_body"
            | "interface_body"
            | "object"
            | "array"
            | "switch_body"
            | "match_block"
            | "if_statement"
            | "for_statement"
            | "while_statement"
            | "do_statement"
            | "if_expression"
            | "for_expression"
            | "while_expression"
            | "loop_expression"
            | "case_statement"
            | "try_statement"
            | "catch_clause"
            | "function_definition"
            | "function_declaration"
            | "method_declaration"
            | "class_declaration"
            | "function_item"
            | "impl_item"
            | "mod_item"
            | "struct_item"
            | "trait_item"
    ) || kind.ends_with("_block")
        || kind.ends_with("_body")
}

fn first_nested_line_indent(
    source: &str,
    start_line: usize,
    end_line: usize,
    tab_size: usize,
) -> Option<usize> {
    source
        .lines()
        .enumerate()
        .skip(start_line.saturating_add(1))
        .take(end_line.saturating_sub(start_line))
        .find_map(|(_line, text)| {
            if text.trim().is_empty() {
                return None;
            }
            let column = leading_visual_columns(text, tab_size);
            (column > 0).then_some(column)
        })
}

fn leading_visual_columns(text: &str, tab_size: usize) -> usize {
    let mut column = 0;
    for ch in text.chars() {
        match ch {
            ' ' => column += 1,
            '\t' => column += tab_size,
            _ => break,
        }
    }
    column
}
