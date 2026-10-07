// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{HighlightSpan, PluginGrammar, SyntaxEdit, SyntaxError, SyntaxSession, SyntaxWork};
use oxideterm_editor_core::{BufferOffset, TextRange};
use std::{cell::Cell, ops::ControlFlow, sync::Arc};
use tree_sitter::{Query, QueryCursor, StreamingIterator};

pub(crate) struct PluginInjection {
    pub query: Query,
    pub grammar: Arc<PluginGrammar>,
}

pub(crate) struct InjectionSyntax {
    definition: usize,
    pub syntax: Box<SyntaxSession>,
    ranges: Vec<tree_sitter::Range>,
}

impl SyntaxSession {
    pub(crate) fn refresh_injections(
        &mut self,
        source: &str,
        edit: Option<SyntaxEdit>,
        work: Option<&SyntaxWork>,
    ) -> Result<(), SyntaxError> {
        let mut previous = std::mem::take(&mut self.injections);
        for (index, definition) in self.queries.injections.iter().enumerate() {
            // One parser per embedded language keeps global byte coordinates and
            // avoids a separate WASM execution store for every template expression.
            let ranges = injection_ranges(&definition.query, &self.tree, source, work)?;
            if ranges.is_empty() {
                continue;
            }
            let syntax =
                if let Some(position) = previous.iter().position(|item| item.definition == index) {
                    let mut syntax = previous.remove(position).syntax;
                    syntax
                        .parser
                        .set_included_ranges(&ranges)
                        .map_err(|error| SyntaxError::Plugin(error.to_string()))?;
                    if let Some(edit) = edit {
                        syntax.apply_edit_controlled(source, edit, work)?;
                    } else {
                        syntax.reparse_controlled(source, work)?;
                    }
                    syntax
                } else {
                    Box::new(Self::parse_plugin_in_ranges(
                        &definition.grammar,
                        source,
                        &ranges,
                        work,
                    )?)
                };
            self.injections.push(InjectionSyntax {
                definition: index,
                syntax,
                ranges,
            });
        }
        Ok(())
    }
}

impl InjectionSyntax {
    pub(crate) fn overlay_highlights(
        &self,
        source: &str,
        range: TextRange,
        spans: Vec<HighlightSpan>,
        work: Option<&SyntaxWork>,
    ) -> Result<Vec<HighlightSpan>, SyntaxError> {
        let mut combined = Vec::with_capacity(spans.len());
        for span in spans {
            crate::work::checkpoint(work)?;
            let mut start = span.range.start.0;
            let end = span.range.end.0;
            for included in self.overlapping_ranges(start, end) {
                if start < included.start_byte {
                    combined.push(HighlightSpan {
                        range: TextRange::new(
                            BufferOffset(start),
                            BufferOffset(included.start_byte.min(end)),
                        ),
                        scope: span.scope,
                    });
                }
                start = start.max(included.end_byte).min(end);
            }
            if start < end {
                combined.push(HighlightSpan {
                    range: TextRange::new(BufferOffset(start), BufferOffset(end)),
                    scope: span.scope,
                });
            }
        }
        // Embedded captures own only their included regions. Parent captures
        // keep delimiters; a child node spanning separate regions cannot color tags.
        for span in self.syntax.highlights_controlled(source, range, work)? {
            crate::work::checkpoint(work)?;
            for included in self.overlapping_ranges(span.range.start.0, span.range.end.0) {
                combined.push(HighlightSpan {
                    range: TextRange::new(
                        BufferOffset(span.range.start.0.max(included.start_byte)),
                        BufferOffset(span.range.end.0.min(included.end_byte)),
                    ),
                    scope: span.scope,
                });
            }
        }
        Ok(combined)
    }

    fn overlapping_ranges(
        &self,
        start: usize,
        end: usize,
    ) -> impl Iterator<Item = &tree_sitter::Range> {
        let first = self.ranges.partition_point(|range| range.end_byte <= start);
        self.ranges[first..]
            .iter()
            .take_while(move |range| range.start_byte < end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HighlightCache, LanguageId, PluginGrammarInjectionSource, PluginGrammarSource, SyntaxScope,
    };
    use oxideterm_editor_core::{BufferOffset, TextRange};

    fn fixture() -> (tempfile::TempDir, PluginGrammarSource) {
        let directory = tempfile::tempdir().unwrap();
        zip::ZipArchive::new(std::io::Cursor::new(include_bytes!(
            "../tests/fixtures/vue.zip"
        )))
        .unwrap()
        .extract(directory.path())
        .unwrap();
        let manifest: oxideterm_plugin_manifest::NativePluginManifest =
            serde_json::from_slice(&std::fs::read(directory.path().join("plugin.json")).unwrap())
                .unwrap();
        let language = manifest.contributes.unwrap().language.unwrap();
        let source = PluginGrammarSource {
            language: LanguageId::from_plugin_key("vue").unwrap(),
            grammar_name: "vue".into(),
            parser: directory.path().join("parser.wasm"),
            highlights: directory.path().join(language.highlights),
            parser_sha256: language.parser_sha256,
            highlights_sha256: language.highlights_sha256,
            injections: language
                .injections
                .into_iter()
                .map(|injection| PluginGrammarInjectionSource {
                    query: directory.path().join(injection.query),
                    query_sha256: injection.query_sha256,
                    grammar: Box::new(PluginGrammarSource {
                        language: LanguageId::from_plugin_key(&injection.id).unwrap(),
                        grammar_name: injection.grammar_name.unwrap_or(injection.id),
                        parser: directory.path().join(injection.parser),
                        highlights: directory.path().join(injection.highlights),
                        parser_sha256: injection.parser_sha256,
                        highlights_sha256: injection.highlights_sha256,
                        injections: Vec::new(),
                    }),
                })
                .collect(),
        };
        (directory, source)
    }

    #[test]
    fn embedded_highlights_follow_global_offsets_edits_and_removed_regions() {
        let (_directory, grammar) = fixture();
        let grammar = PluginGrammar::new(grammar);
        let mut source = "<template><div>🙂 {{ count + 1 }}</div></template>\n<script lang=\"ts\">\nconst count: number = 42;\n</script>\n<style>\n.box { color: red; }\n</style>\n".to_string();
        let generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let work = SyntaxWork::new(generation.clone(), 1, std::time::Duration::from_micros(1));
        let mut session = SyntaxSession::parse_plugin(&grammar, &source, Some(&work)).unwrap();
        let mut cache = HighlightCache::default();
        cache
            .update_controlled(&session, &source, None, Some(&work))
            .unwrap();
        let check = |cache: &HighlightCache, source: &str, word: &str, scope: SyntaxScope| {
            let start = source.find(word).unwrap();
            let range = TextRange::new(BufferOffset(start), BufferOffset(start + word.len()));
            assert!(
                cache
                    .spans_in_range(start..range.end.0)
                    .any(|span| span.range == range && span.scope == scope),
                "{word} at {start}"
            );
        };
        for (word, scope) in [
            ("42", SyntaxScope::Number),
            ("1", SyntaxScope::Number),
            ("const", SyntaxScope::Keyword),
            ("number", SyntaxScope::Type),
            ("color", SyntaxScope::Property),
        ] {
            check(&cache, &source, word, scope);
        }
        // The renderer consumes spans in order; an enclosing template capture
        // must not hide the embedded token even when both queries matched it.
        let number = source.find("1").unwrap();
        assert_eq!(
            cache
                .spans_in_range(number..number + 1)
                .next()
                .unwrap()
                .scope,
            SyntaxScope::Number
        );
        for (range, replacement) in [
            (0..0, "<!-- 你好 -->\n"),
            (
                source.find("42").unwrap() + "<!-- 你好 -->\n".len()
                    ..source.find("42").unwrap() + "<!-- 你好 -->\n".len() + 2,
                "100",
            ),
        ] {
            let edit = SyntaxEdit::replace(
                &source,
                TextRange::new(BufferOffset(range.start), BufferOffset(range.end)),
                replacement,
            );
            source.replace_range(range, replacement);
            let change = session
                .apply_edit_controlled(&source, edit, Some(&work))
                .unwrap();
            cache
                .update_controlled(&session, &source, Some(&change), Some(&work))
                .unwrap();
            check(&cache, &source, "const", SyntaxScope::Keyword);
            check(&cache, &source, "color", SyntaxScope::Property);
        }
        check(&cache, &source, "100", SyntaxScope::Number);
        let start = source.find("<style>").unwrap();
        let end = source.find("</style>").unwrap() + "</style>".len();
        // Keep "color" at the former CSS capture offset to expose a retained stale tree.
        let replacement = "<p>            color</p>";
        let edit = SyntaxEdit::replace(
            &source,
            TextRange::new(BufferOffset(start), BufferOffset(end)),
            replacement,
        );
        source.replace_range(start..end, replacement);
        let change = session
            .apply_edit_controlled(&source, edit, Some(&work))
            .unwrap();
        cache
            .update_controlled(&session, &source, Some(&change), Some(&work))
            .unwrap();
        check(&cache, &source, "100", SyntaxScope::Number);
        assert!(
            !cache
                .spans_in_range(0..source.len())
                .any(|span| span.scope == SyntaxScope::Property
                    && &source[span.range.start.0..span.range.end.0] == "color")
        );
        generation.store(2, std::sync::atomic::Ordering::Release);
        assert!(matches!(
            injection_ranges(
                &session.queries.injections[0].query,
                &session.tree,
                &source,
                Some(&work)
            ),
            Err(SyntaxError::ParseCancelled)
        ));
    }

    #[test]
    #[ignore = "manual same-environment embedded grammar benchmark"]
    fn injection_performance() {
        let (_directory, configured) = fixture();
        let source = "<script lang=\"ts\">const value: number = 42;</script><template><div>{{ value + 1 }}</div></template><style>.x { color: red; }</style>\n".repeat(1000);
        for embedded in [false, true] {
            let mut grammar = configured.clone();
            if !embedded {
                grammar.injections.clear();
            }
            let grammar = PluginGrammar::new(grammar);
            let session = SyntaxSession::parse_plugin(&grammar, &source, None).unwrap();
            std::hint::black_box(session.highlight_spans(&source));
            let started = std::time::Instant::now();
            for _ in 0..5 {
                let session = SyntaxSession::parse_plugin(&grammar, &source, None).unwrap();
                std::hint::black_box(session.highlight_spans(&source));
            }
            eprintln!(
                "embedded={embedded} bytes={} parse_highlight_ms={:.2}",
                source.len(),
                started.elapsed().as_secs_f64() * 1000.0 / 5.0
            );
        }
    }
}

fn injection_ranges(
    query: &Query,
    tree: &tree_sitter::Tree,
    source: &str,
    work: Option<&SyntaxWork>,
) -> Result<Vec<tree_sitter::Range>, SyntaxError> {
    let mut cursor = QueryCursor::new();
    let paused = Cell::new(false);
    let mut pause = |_: &tree_sitter::QueryCursorState| {
        let stop = work.is_some_and(SyntaxWork::should_pause);
        paused.set(stop);
        if stop {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let mut captures = cursor.captures_with_options(
        query,
        tree.root_node(),
        source.as_bytes(),
        tree_sitter::QueryCursorOptions::new().progress_callback(&mut pause),
    );
    let mut ranges = Vec::new();
    loop {
        crate::work::checkpoint(work)?;
        captures.advance();
        let Some((query_match, capture_index)) = captures.get() else {
            if paused.replace(false) {
                continue;
            }
            break;
        };
        let capture = query_match.captures()[*capture_index];
        if query.capture_names()[capture.index as usize] == "injection.content"
            && !capture.node.byte_range().is_empty()
        {
            ranges.push(capture.node.range());
        }
    }
    ranges.sort_by_key(|range| (range.start_byte, range.end_byte));
    let mut merged: Vec<tree_sitter::Range> = Vec::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start_byte <= previous.end_byte
        {
            if range.end_byte > previous.end_byte {
                previous.end_byte = range.end_byte;
                previous.end_point = range.end_point;
            }
        } else {
            merged.push(range);
        }
    }
    Ok(merged)
}
