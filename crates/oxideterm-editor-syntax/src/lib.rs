// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Syntax data layer for OxideTerm's native editor.
//!
//! This crate owns tree-sitter parsers and returns byte-range metadata. It does
//! not paint GPUI elements and does not mutate editor buffers.

use std::path::Path;

use oxideterm_editor_core::{BufferOffset, LineCol, TextRange};
use thiserror::Error;
use tree_sitter::{
    InputEdit, Language, Node, Parser, Point, Query, QueryCursor, StreamingIterator, Tree,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum LanguageId {
    Rust,
}

impl LanguageId {
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        match path.as_ref().extension().and_then(|ext| ext.to_str()) {
            Some("rs") => Some(Self::Rust),
            _ => None,
        }
    }

    pub fn detect(path: Option<&Path>, source: &str) -> Option<Self> {
        path.and_then(Self::from_path)
            .or_else(|| language_from_shebang(source))
    }

    fn tree_sitter_language(self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }

    fn highlight_query(self) -> &'static str {
        match self {
            Self::Rust => tree_sitter_rust::HIGHLIGHTS_QUERY,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SyntaxScope {
    Attribute,
    Comment,
    Constant,
    Function,
    Keyword,
    Namespace,
    Number,
    Operator,
    Property,
    Punctuation,
    String,
    Type,
    Variable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HighlightSpan {
    pub range: TextRange,
    pub scope: SyntaxScope,
    pub capture: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BracketPair {
    pub open: BufferOffset,
    pub close: BufferOffset,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoldRange {
    pub range: TextRange,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyntaxEdit {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start_position: LineCol,
    pub old_end_position: LineCol,
    pub new_end_position: LineCol,
}

impl SyntaxEdit {
    pub fn replace(source_before: &str, range: TextRange, replacement: &str) -> Self {
        let start_position = point_for_byte(source_before, range.start.0);
        let old_end_position = point_for_byte(source_before, range.end.0);
        let new_end_position = advance_position(start_position, replacement);
        Self {
            start_byte: range.start.0,
            old_end_byte: range.end.0,
            new_end_byte: range.start.0 + replacement.len(),
            start_position,
            old_end_position,
            new_end_position,
        }
    }

    fn as_input_edit(self) -> InputEdit {
        InputEdit {
            start_byte: self.start_byte,
            old_end_byte: self.old_end_byte,
            new_end_byte: self.new_end_byte,
            start_position: Point {
                row: self.start_position.line,
                column: self.start_position.column,
            },
            old_end_position: Point {
                row: self.old_end_position.line,
                column: self.old_end_position.column,
            },
            new_end_position: Point {
                row: self.new_end_position.line,
                column: self.new_end_position.column,
            },
        }
    }
}

pub struct SyntaxSession {
    language_id: LanguageId,
    language: Language,
    parser: Parser,
    highlight_query: Query,
    tree: Tree,
}

impl SyntaxSession {
    pub fn parse(language_id: LanguageId, source: &str) -> Result<Self, SyntaxError> {
        let language = language_id.tree_sitter_language();
        let mut parser = Parser::new();
        parser.set_language(&language)?;
        let highlight_query = Query::new(&language, language_id.highlight_query())?;
        let tree = parser
            .parse(source, None)
            .ok_or(SyntaxError::ParseCancelled)?;

        Ok(Self {
            language_id,
            language,
            parser,
            highlight_query,
            tree,
        })
    }

    pub fn language_id(&self) -> LanguageId {
        self.language_id
    }

    pub fn root_has_error(&self) -> bool {
        self.tree.root_node().has_error()
    }

    pub fn apply_edit(&mut self, source_after: &str, edit: SyntaxEdit) -> Result<(), SyntaxError> {
        // tree-sitter incremental parsing requires the old tree to be edited
        // with the same byte/point delta before it is passed back as a hint.
        self.tree.edit(&edit.as_input_edit());
        self.tree = self
            .parser
            .parse(source_after, Some(&self.tree))
            .ok_or(SyntaxError::ParseCancelled)?;
        Ok(())
    }

    pub fn reparse(&mut self, source: &str) -> Result<(), SyntaxError> {
        self.parser.set_language(&self.language)?;
        self.tree = self
            .parser
            .parse(source, None)
            .ok_or(SyntaxError::ParseCancelled)?;
        Ok(())
    }

    pub fn highlight_spans(&self, source: &str) -> Vec<HighlightSpan> {
        let mut cursor = QueryCursor::new();
        let mut captures = cursor.captures(
            &self.highlight_query,
            self.tree.root_node(),
            source.as_bytes(),
        );
        let names = self.highlight_query.capture_names();
        let mut spans = Vec::new();

        while {
            captures.advance();
            captures.get().is_some()
        } {
            let Some((query_match, capture_index)) = captures.get() else {
                continue;
            };
            let Some(capture) = query_match.captures.get(*capture_index).copied() else {
                continue;
            };
            let Some(capture_name) = names.get(capture.index as usize).copied() else {
                continue;
            };
            let Some(scope) = scope_for_capture(capture_name, capture.node.kind()) else {
                continue;
            };
            let range = capture.node.byte_range();
            if range.start < range.end && range.end <= source.len() {
                spans.push(HighlightSpan {
                    range: TextRange::new(BufferOffset(range.start), BufferOffset(range.end)),
                    scope,
                    capture: capture_name.to_string(),
                });
            }
        }

        normalize_highlight_spans(spans)
    }

    pub fn bracket_pairs(&self, source: &str) -> Vec<BracketPair> {
        bracket_pairs(source)
    }

    pub fn fold_ranges(&self) -> Vec<FoldRange> {
        let mut ranges = Vec::new();
        collect_fold_ranges(self.tree.root_node(), &mut ranges);
        ranges
    }
}

#[derive(Debug, Error)]
pub enum SyntaxError {
    #[error("tree-sitter language error: {0}")]
    Language(#[from] tree_sitter::LanguageError),
    #[error("tree-sitter query error: {0}")]
    Query(#[from] tree_sitter::QueryError),
    #[error("tree-sitter parse was cancelled")]
    ParseCancelled,
}

fn language_from_shebang(source: &str) -> Option<LanguageId> {
    let first = source.lines().next()?;
    if first.starts_with("#!") && first.contains("rust-script") {
        Some(LanguageId::Rust)
    } else {
        None
    }
}

fn scope_for_capture(capture: &str, node_kind: &str) -> Option<SyntaxScope> {
    if matches!(node_kind, "integer_literal" | "float_literal") {
        return Some(SyntaxScope::Number);
    }

    let root = capture.split('.').next().unwrap_or(capture);
    match root {
        "attribute" => Some(SyntaxScope::Attribute),
        "comment" => Some(SyntaxScope::Comment),
        "constant" => Some(SyntaxScope::Constant),
        "function" => Some(SyntaxScope::Function),
        "keyword" => Some(SyntaxScope::Keyword),
        "module" | "namespace" => Some(SyntaxScope::Namespace),
        "number" => Some(SyntaxScope::Number),
        "operator" => Some(SyntaxScope::Operator),
        "property" | "field" => Some(SyntaxScope::Property),
        "punctuation" => Some(SyntaxScope::Punctuation),
        "string" | "character" => Some(SyntaxScope::String),
        "type" | "constructor" => Some(SyntaxScope::Type),
        "variable" | "parameter" => Some(SyntaxScope::Variable),
        _ => None,
    }
}

fn normalize_highlight_spans(mut spans: Vec<HighlightSpan>) -> Vec<HighlightSpan> {
    spans.sort_by(|left, right| {
        left.range
            .start
            .cmp(&right.range.start)
            .then_with(|| left.range.end.cmp(&right.range.end))
    });
    spans.dedup_by(|left, right| left.range == right.range && left.scope == right.scope);
    spans
}

fn point_for_byte(source: &str, byte: usize) -> LineCol {
    let mut line = 0;
    let mut line_start = 0;
    for (index, ch) in source.char_indices() {
        if index >= byte {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    LineCol::new(line, byte.saturating_sub(line_start))
}

fn advance_position(start: LineCol, text: &str) -> LineCol {
    let mut line = start.line;
    let mut column = start.column;
    for ch in text.chars() {
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += ch.len_utf8();
        }
    }
    LineCol::new(line, column)
}

fn bracket_pairs(source: &str) -> Vec<BracketPair> {
    let mut stack: Vec<(u8, usize)> = Vec::new();
    let mut pairs = Vec::new();

    for (index, byte) in source.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => stack.push((byte, index)),
            b')' | b']' | b'}' => {
                let Some(position) = stack
                    .iter()
                    .rposition(|(open, _)| brackets_match(*open, byte))
                else {
                    continue;
                };
                let (_, open_index) = stack.remove(position);
                pairs.push(BracketPair {
                    open: BufferOffset(open_index),
                    close: BufferOffset(index),
                });
            }
            _ => {}
        }
    }

    pairs.sort_by_key(|pair| pair.open);
    pairs
}

fn brackets_match(open: u8, close: u8) -> bool {
    matches!((open, close), (b'(', b')') | (b'[', b']') | (b'{', b'}'))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_rust_from_extension_and_shebang() {
        assert_eq!(LanguageId::from_path("src/main.rs"), Some(LanguageId::Rust));
        assert_eq!(
            LanguageId::detect(None, "#!/usr/bin/env rust-script\nfn main() {}"),
            Some(LanguageId::Rust)
        );
    }

    #[test]
    fn parses_and_highlights_rust() {
        let source = "fn main() {\n    let message = \"hi\";\n}\n";
        let session = SyntaxSession::parse(LanguageId::Rust, source).unwrap();

        assert!(!session.root_has_error());
        let spans = session.highlight_spans(source);

        assert!(spans.iter().any(|span| span.scope == SyntaxScope::Keyword));
        assert!(spans.iter().any(|span| span.scope == SyntaxScope::Function));
        assert!(spans.iter().any(|span| span.scope == SyntaxScope::String));
        assert!(spans.iter().all(|span| span.range.end.0 <= source.len()));
    }

    #[test]
    fn reparses_incrementally_after_edit() {
        let before = "fn main() {\n    let x = 1;\n}\n";
        let edit_range = TextRange::new(BufferOffset(22), BufferOffset(23));
        let edit = SyntaxEdit::replace(before, edit_range, "10");
        let after = "fn main() {\n    let x = 10;\n}\n";
        let mut session = SyntaxSession::parse(LanguageId::Rust, before).unwrap();

        session.apply_edit(after, edit).unwrap();

        assert!(!session.root_has_error());
        assert!(
            session
                .highlight_spans(after)
                .iter()
                .any(|span| span.scope == SyntaxScope::Number)
        );
    }

    #[test]
    fn computes_bracket_and_fold_ranges() {
        let source = "fn main() {\n    if true {\n        println!(\"x\");\n    }\n}\n";
        let session = SyntaxSession::parse(LanguageId::Rust, source).unwrap();

        assert!(session.bracket_pairs(source).len() >= 3);
        assert!(
            session
                .fold_ranges()
                .iter()
                .any(|range| range.start_line == 0 && range.end_line >= 3)
        );
    }
}
