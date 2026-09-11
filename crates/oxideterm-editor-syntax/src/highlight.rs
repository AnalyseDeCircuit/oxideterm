// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{cell::Cell, ops::Range};

use oxideterm_editor_core::{BufferOffset, TextRange};
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator, Tree};

use crate::{HighlightSpan, LanguageId, SyntaxScope};

pub(crate) fn highlight_spans(
    language_id: LanguageId,
    tree: &Tree,
    highlight_query: &Query,
    markdown_inline_query: Option<&Query>,
    source: &str,
    requested: Range<usize>,
    work: Option<&crate::SyntaxWork>,
) -> Result<Vec<HighlightSpan>, crate::SyntaxError> {
    let requested = requested.start..requested.end.min(source.len());
    if requested.start >= requested.end {
        return Ok(Vec::new());
    }
    let mut cursor = QueryCursor::new();
    // Query from the root so predicates and captures keep their ancestor context.
    cursor.set_byte_range(requested.clone());
    let paused = Cell::new(false);
    let mut pause = |_: &tree_sitter::QueryCursorState| {
        let stop = work.is_some_and(crate::SyntaxWork::should_pause);
        paused.set(stop);
        stop
    };
    let mut captures = cursor.captures_with_options(
        highlight_query,
        tree.root_node(),
        source.as_bytes(),
        tree_sitter::QueryCursorOptions::new().progress_callback(&mut pause),
    );
    let names = highlight_query.capture_names();
    let mut spans = Vec::new();

    loop {
        crate::work::checkpoint(work)?;
        captures.advance();
        if captures.get().is_none() {
            if paused.replace(false) {
                continue;
            }
            break;
        }
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
        if range.start < range.end
            && range.end <= source.len()
            && range.start < requested.end
            && requested.start < range.end
        {
            spans.push(HighlightSpan {
                range: TextRange::new(BufferOffset(range.start), BufferOffset(range.end)),
                scope,
            });
        }
    }

    if language_id == LanguageId::Markdown
        && let Some(inline_query) = markdown_inline_query
    {
        collect_markdown_inline_highlights(
            tree.root_node(),
            source,
            inline_query,
            &requested,
            &mut spans,
            work,
        )?;
    }

    crate::work::checkpoint(work)?;
    Ok(normalize_highlight_spans(spans))
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
    requested: &Range<usize>,
    spans: &mut Vec<HighlightSpan>,
    work: Option<&crate::SyntaxWork>,
) -> Result<(), crate::SyntaxError> {
    crate::work::checkpoint(work)?;
    if node.start_byte() >= requested.end || node.end_byte() <= requested.start {
        return Ok(());
    }
    if node.kind() == "inline" {
        collect_markdown_inline_node_highlights(
            node,
            source,
            inline_query,
            requested,
            spans,
            work,
        )?;
        return Ok(());
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_markdown_inline_highlights(child, source, inline_query, requested, spans, work)?;
    }
    Ok(())
}

fn collect_markdown_inline_node_highlights(
    node: Node<'_>,
    source: &str,
    inline_query: &Query,
    requested: &Range<usize>,
    spans: &mut Vec<HighlightSpan>,
    work: Option<&crate::SyntaxWork>,
) -> Result<(), crate::SyntaxError> {
    crate::work::checkpoint(work)?;
    let range = node.byte_range();
    if range.start >= range.end || range.end > source.len() {
        return Ok(());
    }
    let inline_language: Language = tree_sitter_md::INLINE_LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&inline_language)?;
    let tree = crate::work::parse(&mut parser, &source[range.clone()], None, work)?;

    let mut query_cursor = QueryCursor::new();
    query_cursor.set_byte_range(
        requested.start.saturating_sub(range.start)..requested.end.min(range.end) - range.start,
    );
    let paused = Cell::new(false);
    let mut pause = |_: &tree_sitter::QueryCursorState| {
        let stop = work.is_some_and(crate::SyntaxWork::should_pause);
        paused.set(stop);
        stop
    };
    let mut captures = query_cursor.captures_with_options(
        inline_query,
        tree.root_node(),
        source[range.clone()].as_bytes(),
        tree_sitter::QueryCursorOptions::new().progress_callback(&mut pause),
    );
    let names = inline_query.capture_names();

    loop {
        crate::work::checkpoint(work)?;
        captures.advance();
        if captures.get().is_none() {
            if paused.replace(false) {
                continue;
            }
            break;
        }
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
        if start < end && end <= source.len() && start < requested.end && requested.start < end {
            spans.push(HighlightSpan {
                range: TextRange::new(BufferOffset(start), BufferOffset(end)),
                scope,
            });
        }
    }
    Ok(())
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
