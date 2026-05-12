// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_editor_core::{BufferOffset, TextRange};
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator, Tree};

use crate::{HighlightSpan, LanguageId, SyntaxScope};

pub(crate) fn highlight_spans(
    language_id: LanguageId,
    tree: &Tree,
    highlight_query: &Query,
    markdown_inline_query: Option<&Query>,
    source: &str,
) -> Vec<HighlightSpan> {
    let mut cursor = QueryCursor::new();
    let mut captures = cursor.captures(highlight_query, tree.root_node(), source.as_bytes());
    let names = highlight_query.capture_names();
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

    if language_id == LanguageId::Markdown
        && let Some(inline_query) = markdown_inline_query
    {
        collect_markdown_inline_highlights(tree.root_node(), source, inline_query, &mut spans);
    }

    normalize_highlight_spans(spans)
}

fn scope_for_capture(capture: &str, node_kind: &str) -> Option<SyntaxScope> {
    if matches!(node_kind, "integer_literal" | "float_literal") {
        return Some(SyntaxScope::Number);
    }
    match capture {
        // Tauri loads `@codemirror/lang-markdown`, whose Lezer tags include
        // heading/link/literal punctuation. The native editor maps those
        // Markdown-specific captures onto the existing syntax palette so `.md`
        // files are highlighted without adding a parallel color system.
        "text.title" => return Some(SyntaxScope::Keyword),
        "text.uri" => return Some(SyntaxScope::Function),
        "text.literal" => return Some(SyntaxScope::String),
        "text.reference" => return Some(SyntaxScope::Type),
        "text.emphasis" | "text.strong" => return Some(SyntaxScope::Variable),
        _ => {}
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

fn collect_markdown_inline_highlights(
    node: Node<'_>,
    source: &str,
    inline_query: &Query,
    spans: &mut Vec<HighlightSpan>,
) {
    if node.kind() == "inline" {
        collect_markdown_inline_node_highlights(node, source, inline_query, spans);
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_markdown_inline_highlights(child, source, inline_query, spans);
    }
}

fn collect_markdown_inline_node_highlights(
    node: Node<'_>,
    source: &str,
    inline_query: &Query,
    spans: &mut Vec<HighlightSpan>,
) {
    let range = node.byte_range();
    if range.start >= range.end || range.end > source.len() {
        return;
    }
    let inline_language: Language = tree_sitter_md::INLINE_LANGUAGE.into();
    let mut parser = Parser::new();
    if parser.set_language(&inline_language).is_err() {
        return;
    }
    let Some(tree) = parser.parse(&source[range.clone()], None) else {
        return;
    };

    let mut query_cursor = QueryCursor::new();
    let mut captures = query_cursor.captures(
        inline_query,
        tree.root_node(),
        source[range.clone()].as_bytes(),
    );
    let names = inline_query.capture_names();

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
        let capture_range = capture.node.byte_range();
        let start = range.start + capture_range.start;
        let end = range.start + capture_range.end;
        if start < end && end <= source.len() {
            spans.push(HighlightSpan {
                range: TextRange::new(BufferOffset(start), BufferOffset(end)),
                scope,
                capture: capture_name.to_string(),
            });
        }
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
