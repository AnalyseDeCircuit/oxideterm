// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashMap, ops::Range, sync::Arc};

const SPANS_PER_INDEX_ENTRY: usize = 64;

use oxideterm_editor_core::{BufferOffset, TextRange};

use crate::{HighlightSpan, LanguageId, SyntaxChange, SyntaxScope, SyntaxSession};

// Tree-sitter's node byte offsets are uint32_t, including on 64-bit hosts.
// Keep that width in retained spans; expand only at the public range boundary.
#[derive(Debug)]
struct CachedHighlight {
    start: u32,
    end: u32,
    scope: SyntaxScope,
}

impl CachedHighlight {
    fn relative(span: HighlightSpan, base: usize) -> Self {
        Self {
            start: u32::try_from(span.range.start.0 - base).expect("tree-sitter byte offset"),
            end: u32::try_from(span.range.end.0 - base).expect("tree-sitter byte offset"),
            scope: span.scope,
        }
    }
}

#[derive(Debug)]
struct HighlightBlock {
    kind_id: u16,
    range: Range<usize>,
    spans: Box<[CachedHighlight]>,
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
                let last = block
                    .spans
                    .partition_point(|span| (span.start as usize) < end);
                block.spans[first.min(last)..last]
                    .iter()
                    .filter(move |span| (span.end as usize) > start)
                    .map(move |span| HighlightSpan {
                        range: TextRange::new(
                            BufferOffset(block.range.start + span.start as usize),
                            BufferOffset(block.range.start + span.end as usize),
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
            && (0..session.queries.highlight.pattern_count()).all(|i| {
                session.queries.highlight.is_pattern_rooted(i)
                    && !session.queries.highlight.is_pattern_non_local(i)
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
        if !reusable {
            // No old block can contribute to this result. Release it before
            // allocating the full query output and the replacement cache.
            self.clear();
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
                    .into_iter()
                    .map(|span| CachedHighlight::relative(span, 0))
                    .collect::<Vec<_>>()
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
            let mut full = if reusable {
                None
            } else {
                Some(
                    session
                        .highlights_controlled(
                            source,
                            TextRange::new(BufferOffset(0), BufferOffset(source.len())),
                            work,
                        )?
                        .into_iter(),
                )
            };
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
                    let spans: Box<[_]> = if let Some(full) = &mut full {
                        let count = full
                            .as_slice()
                            .partition_point(|span| span.range.start.0 < range.end);
                        full.by_ref()
                            .take(count)
                            .map(|span| CachedHighlight::relative(span, range.start))
                            .collect::<Vec<_>>()
                            .into()
                    } else {
                        session
                            .highlights_controlled(
                                source,
                                TextRange::new(BufferOffset(range.start), BufferOffset(range.end)),
                                work,
                            )?
                            .into_iter()
                            .map(|span| CachedHighlight::relative(span, range.start))
                            .collect::<Vec<_>>()
                            .into()
                    };
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
fn index_span_ends(spans: &[CachedHighlight]) -> Option<Box<[usize]>> {
    if spans.len() <= SPANS_PER_INDEX_ENTRY {
        return None;
    }
    let mut maximum = 0;
    Some(
        spans
            .chunks(SPANS_PER_INDEX_ENTRY)
            .map(|chunk| {
                maximum = maximum.max(
                    chunk
                        .iter()
                        .map(|span| span.end as usize)
                        .max()
                        .unwrap_or(0),
                );
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

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "manual full-highlight rebuild allocation benchmark"]
    fn highlight_rebuild_memory() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        #[repr(C)]
        #[derive(Default)]
        struct Statistics {
            blocks: u32,
            live: usize,
            peak: usize,
            allocated: usize,
        }
        unsafe extern "C" {
            fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut Statistics);
        }
        fn allocated() -> usize {
            let mut stats = Statistics::default();
            // Sample live heap allocation across all zones, not process RSS.
            unsafe { malloc_zone_statistics(std::ptr::null_mut(), &mut stats) };
            stats.live
        }
        for language in [LanguageId::Rust, LanguageId::Python] {
            let source = match language {
                LanguageId::Rust => "fn sample() { let value = 42; }\n".repeat(34000),
                _ => "value = 42 # sample\n".repeat(56000),
            };
            let mut session = SyntaxSession::parse(language, &source).unwrap();
            let mut cache = HighlightCache::default();
            cache.update(&session, &source, None);
            for run in 0..4 {
                session.reparse(&source).unwrap();
                let before = allocated();
                let peak = AtomicUsize::new(before);
                let stop = AtomicBool::new(false);
                let started = std::time::Instant::now();
                std::thread::scope(|scope| {
                    scope.spawn(|| {
                        while !stop.load(Ordering::Acquire) {
                            peak.fetch_max(allocated(), Ordering::Relaxed);
                            std::thread::sleep(std::time::Duration::from_micros(100));
                        }
                    });
                    cache.update(&session, &source, None);
                    peak.fetch_max(allocated(), Ordering::Relaxed);
                    stop.store(true, Ordering::Release);
                });
                let rebuild_ms = started.elapsed().as_secs_f64() * 1000.0;
                eprintln!(
                    "HIGHLIGHT_REBUILD language={language:?} run={run} baseline={before} peak={} rebuild_ms={rebuild_ms:.3}",
                    peak.load(Ordering::Relaxed)
                );
                assert_eq!(
                    cache.spans_in_range(0..source.len()).collect::<Vec<_>>(),
                    session.highlight_spans(&source)
                );
            }
        }
    }

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
