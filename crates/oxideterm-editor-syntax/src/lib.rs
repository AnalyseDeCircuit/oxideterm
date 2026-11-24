// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Syntax data layer for OxideTerm's native editor.
//!
//! This crate owns tree-sitter parsers and returns byte-range metadata. It does
//! not paint GPUI elements and does not mutate editor buffers.

mod brackets;
#[cfg(not(target_family = "wasm"))]
mod cache;
mod edit;
mod error;
#[cfg(not(target_family = "wasm"))]
mod folding;
#[cfg(not(target_family = "wasm"))]
mod highlight;
#[cfg(not(target_family = "wasm"))]
mod indent;
#[cfg(not(target_family = "wasm"))]
mod indent_index;
#[cfg(not(target_family = "wasm"))]
mod injections;
mod language;
#[cfg(not(target_family = "wasm"))]
mod plugin;
#[cfg(not(target_family = "wasm"))]
mod queries;
#[cfg(not(target_family = "wasm"))]
mod session;
mod source;
#[cfg(not(target_family = "wasm"))]
mod structure;
mod types;
#[cfg(target_family = "wasm")]
mod wasm_stub;
mod work;

#[cfg(all(test, not(target_family = "wasm")))]
mod tests;

#[cfg(all(test, not(target_family = "wasm")))]
mod grammar_fixture {
    use crate::{
        LanguageId, PluginGrammar, PluginGrammarSource, SyntaxError, SyntaxSession, SyntaxWork,
    };
    include!("../tests/support/grammars.rs");
}

pub use brackets::BracketIndex;
#[cfg(not(target_family = "wasm"))]
pub use cache::HighlightCache;
pub use edit::{SyntaxChange, SyntaxEdit};
pub use error::SyntaxError;
pub use language::{LanguageId, SUPPORTED_LANGUAGES};
#[cfg(not(target_family = "wasm"))]
pub use plugin::{PluginGrammar, PluginGrammarInjectionSource, PluginGrammarSource};
#[cfg(not(target_family = "wasm"))]
pub use session::SyntaxSession;
pub use source::{ChunkedText, TextSource};
#[cfg(not(target_family = "wasm"))]
pub use structure::StructureCache;
pub use types::{BracketPair, FoldRange, HighlightSpan, IndentGuide, SyntaxScope};
#[cfg(target_family = "wasm")]
pub use wasm_stub::{
    HighlightCache, PluginGrammar, PluginGrammarInjectionSource, PluginGrammarSource,
    StructureCache, SyntaxSession,
};
pub use work::SyntaxWork;

#[cfg(not(target_family = "wasm"))]
fn visit_multiline_nodes_controlled<'tree>(
    root: tree_sitter::Node<'tree>,
    mut visit: impl FnMut(tree_sitter::Node<'tree>),
    work: Option<&SyntaxWork>,
) -> Result<(), SyntaxError> {
    // Reuse one cursor for the traversal instead of allocating one at every node.
    let mut cursor = root.walk();
    loop {
        work::checkpoint(work)?;
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
                return Ok(());
            }
        }
    }
}
