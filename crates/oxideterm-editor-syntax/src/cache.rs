// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashMap, ops::Range, sync::Arc};

const SPANS_PER_INDEX_ENTRY: usize = 64;

use oxideterm_editor_core::{BufferOffset, TextRange};

use crate::{HighlightSpan, LanguageId, SyntaxChange, SyntaxSession};

#[derive(Debug)]
struct HighlightBlock {
    kind_id: u16,
    range: Range<usize>,
    spans: Box<[HighlightSpan]>,
}

/// Relative spans retain their storage across edits; only block positions move.
#[derive(Debug, Default)]
pub struct HighlightCache {
    owner: Option<Arc<()>>,
    revision: u64,
    blocks: Vec<HighlightBlock>,
    span_ends: HashMap<usize, Box<[usize]>>,
}

impl HighlightCache {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(|block| block.spans.is_empty())
    }

    pub fn spans_in_range(&self, range: Range<usize>) -> impl Iterator<Item = HighlightSpan> + '_ {
        let first = self
            .blocks
            .partition_point(|block| block.range.end <= range.start);
        self.blocks[first..]
            .iter()
            .take_while(move |block| range.start < range.end && block.range.start < range.end)
            .flat_map(move |block| {
                let start = range.start.saturating_sub(block.range.start);
                let end = range.end.saturating_sub(block.range.start);
                let first = self.span_ends.get(&block.range.start).map_or(0, |ends| {
                    ends.partition_point(|last| *last <= start) * SPANS_PER_INDEX_ENTRY
                });
                let last = block.spans.partition_point(|span| span.range.start.0 < end);
                block.spans[first.min(last)..last]
                    .iter()
                    .filter(move |span| span.range.end.0 > start)
                    .map(move |span| HighlightSpan {
                        range: TextRange::new(
                            BufferOffset(block.range.start + span.range.start.0),
                            BufferOffset(block.range.start + span.range.end.0),
                        ),
                        scope: span.scope,
                    })
            })
    }

    pub fn update(&mut self, session: &SyntaxSession, source: &str, change: Option<&SyntaxChange>) {
        self.update_controlled(session, source, change, None)
            .expect("uncontrolled cache updates cannot be cancelled");
    }

    pub fn update_controlled(
        &mut self,
        session: &SyntaxSession,
        source: &str,
        change: Option<&SyntaxChange>,
        work: Option<&crate::SyntaxWork>,
    ) -> Result<(), crate::SyntaxError> {
        crate::work::checkpoint(work)?;
        if change.is_none()
            && self.revision == session.revision
            && self
                .owner
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, &session.cache_owner))
        {
            return Ok(());
        }
        // The bundled Rust query has no source_file or cross-root patterns.
        // Other grammars retain full queries until their context rules are verified.
        let partitioned = session.language_id == LanguageId::Rust
            && (0..session.highlight_query.pattern_count()).all(|i| {
                session.highlight_query.is_pattern_rooted(i)
                    && !session.highlight_query.is_pattern_non_local(i)
            });
        let mut reusable = partitioned
            && change.is_some_and(|change| {
                self.owner
                    .as_ref()
                    .is_some_and(|owner| Arc::ptr_eq(owner, &session.cache_owner))
                    && Arc::ptr_eq(&change.owner, &session.cache_owner)
                    && self.revision.checked_add(1) == Some(session.revision)
                    && change.revision == session.revision
            });
        if reusable && let Some(change) = change {
            let changed_bytes = change
                .structural_ranges()
                .map(|range| range.len())
                .sum::<usize>()
                .max(change.edit.new_end_byte - change.edit.start_byte);
            // Broad invalidations are cheaper as one query than thousands of
            // per-root queries. Repartition its result without mixing old spans.
            if changed_bytes > source.len() / 2 {
                reusable = false;
            }
        }
        let mut span_ends = HashMap::new();
        if !partitioned {
            self.blocks = vec![HighlightBlock {
                kind_id: 0,
                range: 0..source.len(),
                spans: session
                    .highlights_controlled(
                        source,
                        TextRange::new(BufferOffset(0), BufferOffset(source.len())),
                        work,
                    )?
                    .into(),
            }];
            if let Some(index) = index_span_ends(&self.blocks[0].spans) {
                span_ends.insert(0, index);
            }
        } else {
            let root = session.tree.root_node();
            let mut cursor = root.walk();
            let mut blocks = Vec::with_capacity(root.child_count());
            // Initial/full refresh uses one query rather than one query per node.
            let full = if reusable {
                None
            } else {
                Some(session.highlights_controlled(
                    source,
                    TextRange::new(BufferOffset(0), BufferOffset(source.len())),
                    work,
                )?)
            };
            let mut full_index = 0;
            for node in root.children(&mut cursor) {
                crate::work::checkpoint(work)?;
                let range = node.byte_range();
                if range.is_empty() {
                    continue;
                }
                let old_block = change.filter(|_| reusable).and_then(|change| {
                    let old_start = change.unchanged_old_range(range.clone())?.start;
                    let index = self
                        .blocks
                        .partition_point(|block| block.range.start < old_start);
                    self.blocks.get_mut(index).filter(|block| {
                        block.range.start == old_start
                            && block.kind_id == node.kind_id()
                            && block.range.len() == range.len()
                    })
                });
                let spans = if let Some(block) = old_block {
                    if let Some(index) = self.span_ends.remove(&block.range.start) {
                        span_ends.insert(range.start, index);
                    }
                    std::mem::take(&mut block.spans)
                } else {
                    let spans = if let Some(full) = &full {
                        let first = full_index;
                        while full_index < full.len() && full[full_index].range.start.0 < range.end
                        {
                            full_index += 1;
                        }
                        full[first..full_index].to_vec()
                    } else {
                        session.highlights_controlled(
                            source,
                            TextRange::new(BufferOffset(range.start), BufferOffset(range.end)),
                            work,
                        )?
                    };
                    // Rust captures are contained in their root child. Preserve
                    // whole captures, including multi-line strings and comments.
                    let spans: Box<[_]> = spans
                        .into_iter()
                        .map(|span| HighlightSpan {
                            range: TextRange::new(
                                BufferOffset(span.range.start.0 - range.start),
                                BufferOffset(span.range.end.0 - range.start),
                            ),
                            scope: span.scope,
                        })
                        .collect::<Vec<_>>()
                        .into();
                    if let Some(index) = index_span_ends(&spans) {
                        span_ends.insert(range.start, index);
                    }
                    spans
                };
                blocks.push(HighlightBlock {
                    kind_id: node.kind_id(),
                    range,
                    spans,
                });
            }
            self.blocks = blocks;
        }
        self.span_ends = span_ends;
        self.owner = Some(session.cache_owner.clone());
        self.revision = session.revision;
        crate::work::checkpoint(work)
    }
}

// Prefix maxima preserve captures enclosing later spans. Index groups rather
// than every token to keep full-query languages compact as well.
fn index_span_ends(spans: &[HighlightSpan]) -> Option<Box<[usize]>> {
    if spans.len() <= SPANS_PER_INDEX_ENTRY {
        return None;
    }
    let mut maximum = 0;
    Some(
        spans
            .chunks(SPANS_PER_INDEX_ENTRY)
            .map(|chunk| {
                maximum = maximum.max(chunk.iter().map(|span| span.range.end.0).max().unwrap_or(0));
                maximum
            })
            .collect::<Vec<_>>()
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SyntaxEdit, SyntaxScope};

    #[test]
    fn large_blocks_and_full_query_languages_keep_range_results() {
        for (language, source) in [
            (
                LanguageId::Rust,
                format!("fn main() {{\n{}}}\n", "let value = foo;\n".repeat(128)),
            ),
            (LanguageId::Python, "value = \"中文🙂\"\n".repeat(128)),
        ] {
            let session = SyntaxSession::parse(language, &source).unwrap();
            let full = session.highlight_spans(&source);
            let mut cache = HighlightCache::default();
            cache.update(&session, &source, None);
            for (start, ch) in source.char_indices().step_by(17) {
                let end = start + ch.len_utf8();
                let expected: Vec<_> = full
                    .iter()
                    .filter(|span| span.range.start.0 < end && span.range.end.0 > start)
                    .cloned()
                    .collect();
                assert_eq!(
                    cache.spans_in_range(start..end).collect::<Vec<_>>(),
                    expected,
                    "{language:?}: {start}..{end}"
                );
                assert_eq!(
                    cache.spans_in_range(start..start).collect::<Vec<_>>(),
                    vec![]
                );
            }
        }
    }

    #[test]
    fn edits_reuse_distant_blocks_without_moving_their_relative_spans() {
        let mut source =
            "fn first() { let value = foo; }\nfn second() {}\nfn third() {}\n".to_string();
        let mut session = SyntaxSession::parse(LanguageId::Rust, &source).unwrap();
        let mut cache = HighlightCache::default();
        cache.update(&session, &source, None);
        assert_eq!(cache.blocks.len(), 3);
        let last_spans = cache.blocks[2].spans.as_ptr();
        let foo = source.find("foo").unwrap();
        for (start, end, replacement) in [(foo, foo + 3, "Foo"), (0, 0, "// 中文🙂\n")] {
            let edit = SyntaxEdit::replace(
                &source,
                TextRange::new(BufferOffset(start), BufferOffset(end)),
                replacement,
            );
            source.replace_range(start..end, replacement);
            let change = session.apply_edit(&source, edit).unwrap();
            cache.update(&session, &source, Some(&change));
            assert!(
                last_spans == cache.blocks.last().unwrap().spans.as_ptr(),
                "unaffected block was regenerated"
            );
            assert_eq!(
                cache.spans_in_range(0..source.len()).collect::<Vec<_>>(),
                session.highlight_spans(&source)
            );
        }
        let first_spans = cache.blocks[0].spans.as_ptr();
        let start = source.find("third").unwrap();
        let edit = SyntaxEdit::replace(
            &source,
            TextRange::new(BufferOffset(start), BufferOffset(start + 5)),
            "最后",
        );
        source.replace_range(start..start + 5, "最后");
        let change = session.apply_edit(&source, edit).unwrap();
        cache.update(&session, &source, Some(&change));
        assert_eq!(first_spans, cache.blocks[0].spans.as_ptr());
        assert_eq!(
            cache.spans_in_range(0..source.len()).collect::<Vec<_>>(),
            session.highlight_spans(&source)
        );
        let foo = source.find("Foo").unwrap();
        assert!(cache.spans_in_range(foo..foo + 3).any(|span| span.range
            == TextRange::new(BufferOffset(foo), BufferOffset(foo + 3))
            && span.scope == SyntaxScope::Type));
    }

    #[test]
    fn boundary_edits_and_stale_changes_match_a_fresh_query() {
        let initial = "fn first() { let value = \"中文🙂\"; }\nfn second() {}\n";
        let mut source = initial.to_string();
        let mut session = SyntaxSession::parse(LanguageId::Rust, &source).unwrap();
        let mut cache = HighlightCache::default();
        cache.update(&session, &source, None);
        let mut stale = None;
        for (start, end, replacement) in [
            (0, 0, "/*"),
            (2, 2, "*/"),
            (0, 4, ""),
            (0, 2, "xx"),
            (0, 2, "fn"),
        ] {
            let edit = SyntaxEdit::replace(
                &source,
                TextRange::new(BufferOffset(start), BufferOffset(end)),
                replacement,
            );
            source.replace_range(start..end, replacement);
            let change = session.apply_edit(&source, edit).unwrap();
            cache.update(&session, &source, Some(&change));
            let expected = SyntaxSession::parse(LanguageId::Rust, &source)
                .unwrap()
                .highlight_spans(&source);
            assert_eq!(
                cache.spans_in_range(0..source.len()).collect::<Vec<_>>(),
                expected,
                "after {edit:?}"
            );
            if let Some(stale) = &stale {
                cache.update(&session, &source, Some(stale));
                assert_eq!(
                    cache.spans_in_range(0..source.len()).collect::<Vec<_>>(),
                    expected
                );
            }
            stale = Some(change);
        }
        assert_eq!(source, initial);
        let markdown = "**中文** and `code`\n";
        let session = SyntaxSession::parse(LanguageId::Markdown, markdown).unwrap();
        cache.update(&session, markdown, stale.as_ref());
        assert_eq!(
            cache.spans_in_range(0..markdown.len()).collect::<Vec<_>>(),
            session.highlight_spans(markdown)
        );
    }
}
