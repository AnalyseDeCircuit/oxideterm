// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_editor_core::{BufferOffset, TextRange};
use tree_sitter::Node;

use crate::FoldRange;

pub(crate) fn fold_ranges(root: Node<'_>) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    collect_fold_ranges(root, &mut ranges);
    ranges
}

fn collect_fold_ranges(node: Node<'_>, ranges: &mut Vec<FoldRange>) {
    if is_foldable_node(node) {
        let start = node.start_position();
        let end = node.end_position();
        if end.row > start.row {
            ranges.push(FoldRange {
                range: TextRange::new(
                    BufferOffset(node.start_byte()),
                    BufferOffset(node.end_byte()),
                ),
                start_line: start.row,
                end_line: end.row,
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_fold_ranges(child, ranges);
    }
}

fn is_foldable_node(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "block"
            | "declaration_list"
            | "enum_item"
            | "function_item"
            | "impl_item"
            | "match_block"
            | "mod_item"
            | "struct_item"
            | "trait_item"
    )
}
