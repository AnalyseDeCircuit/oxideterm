// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::SyntaxError;
use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Cooperative slices for an owned background worker. Lexers and sorts can
/// exceed a slice; publication cancellation must not depend on their latency.
pub struct SyntaxWork {
    generation: Arc<AtomicU64>,
    expected: u64,
    slice: Duration,
    deadline: Cell<Instant>,
}

impl SyntaxWork {
    pub fn new(generation: Arc<AtomicU64>, expected: u64, slice: Duration) -> Self {
        Self {
            generation,
            expected,
            slice,
            deadline: Cell::new(Instant::now() + slice),
        }
    }

    pub fn cancelled(&self) -> bool {
        self.generation.load(Ordering::Acquire) != self.expected
    }
    pub(crate) fn should_pause(&self) -> bool {
        self.cancelled() || Instant::now() >= self.deadline.get()
    }

    pub fn checkpoint(&self) -> Result<(), SyntaxError> {
        if self.cancelled() {
            return Err(SyntaxError::ParseCancelled);
        }
        if Instant::now() >= self.deadline.get() {
            std::thread::yield_now();
            self.deadline.set(Instant::now() + self.slice);
        }
        if self.cancelled() {
            return Err(SyntaxError::ParseCancelled);
        }
        Ok(())
    }
}

pub(crate) fn checkpoint(work: Option<&SyntaxWork>) -> Result<(), SyntaxError> {
    if let Some(work) = work {
        work.checkpoint()?;
    }
    Ok(())
}

pub(crate) fn parse(
    parser: &mut tree_sitter::Parser,
    source: &str,
    old: Option<&tree_sitter::Tree>,
    work: Option<&SyntaxWork>,
) -> Result<tree_sitter::Tree, SyntaxError> {
    loop {
        checkpoint(work)?;
        let mut pause = |_: &tree_sitter::ParseState| work.is_some_and(SyntaxWork::should_pause);
        if let Some(tree) = parser.parse_with_options(
            &mut |offset, _| source.as_bytes().get(offset..).unwrap_or_default(),
            old,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut pause)),
        ) {
            return Ok(tree);
        }
        if work.is_none() {
            return Err(SyntaxError::ParseCancelled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HighlightCache, LanguageId, StructureCache, SyntaxSession};

    #[test]
    fn sliced_work_preserves_results_and_cancelled_work_is_not_reusable() {
        let source = "fn example() {\n    let value = 42;\n}\n".repeat(512);
        let generation = Arc::new(AtomicU64::new(1));
        let work = SyntaxWork::new(generation.clone(), 1, Duration::from_micros(1));
        let session =
            SyntaxSession::parse_controlled(LanguageId::Rust, &source, Some(&work)).unwrap();
        let expected = SyntaxSession::parse(LanguageId::Rust, &source).unwrap();
        let mut highlights = HighlightCache::default();
        let mut structure = StructureCache::default();
        highlights
            .update_controlled(&session, &source, None, Some(&work))
            .unwrap();
        structure
            .update_controlled(&session, &source, 4, None, Some(&work))
            .unwrap();
        assert_eq!(
            highlights
                .spans_in_range(0..source.len())
                .collect::<Vec<_>>(),
            expected.highlight_spans(&source)
        );
        assert_eq!(structure.fold_at_line(0), Some((0, 2)));
        assert_eq!(structure.columns_for_line(1), [0]);
        let markdown = "**中文🙂** and [link](https://example.com)\n\n".repeat(128);
        let md =
            SyntaxSession::parse_controlled(LanguageId::Markdown, &markdown, Some(&work)).unwrap();
        let range = oxideterm_editor_core::TextRange::new(
            oxideterm_editor_core::BufferOffset(0),
            oxideterm_editor_core::BufferOffset(markdown.len()),
        );
        assert_eq!(
            md.highlights_controlled(&markdown, range, Some(&work))
                .unwrap(),
            md.highlight_spans(&markdown)
        );
        generation.store(2, Ordering::Release);
        assert!(matches!(
            SyntaxSession::parse_controlled(LanguageId::Rust, &source, Some(&work)),
            Err(SyntaxError::ParseCancelled)
        ));
        assert!(matches!(
            highlights.update_controlled(&session, &source, None, Some(&work)),
            Err(SyntaxError::ParseCancelled)
        ));
        assert!(matches!(
            structure.update_controlled(&session, &source, 4, None, Some(&work)),
            Err(SyntaxError::ParseCancelled)
        ));
    }
}
