// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Syntax data layer for OxideTerm's native editor.
//!
//! This crate owns tree-sitter parsers and returns byte-range metadata. It does
//! not paint GPUI elements and does not mutate editor buffers.

mod brackets;
mod cache;
mod edit;
mod error;
mod folding;
mod highlight;
mod indent;
mod indent_index;
mod language;
mod queries;
mod session;
mod structure;
mod types;

#[cfg(test)]
mod tests;

pub use cache::HighlightCache;
pub use edit::{SyntaxChange, SyntaxEdit};
pub use error::SyntaxError;
pub use language::{LanguageId, SUPPORTED_LANGUAGES};
pub use session::SyntaxSession;
pub use structure::StructureCache;
pub use types::{BracketPair, FoldRange, HighlightSpan, IndentGuide, SyntaxScope};

fn visit_multiline_nodes<'tree>(
    root: tree_sitter::Node<'tree>,
    mut visit: impl FnMut(tree_sitter::Node<'tree>),
) {
    // Reuse one cursor for the traversal instead of allocating one at every node.
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        // A single-line subtree cannot contain a multiline fold or guide.
        if node.end_position().row > node.start_position().row {
            visit(node);
            if cursor.goto_first_child() {
                continue;
            }
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}
