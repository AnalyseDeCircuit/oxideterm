// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    ops::Range,
    sync::{Arc, OnceLock},
};

use oxideterm_editor_core::{BufferOffset, LineCol, TextRange};
use tree_sitter::{InputEdit, Point, Tree};

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

    pub(crate) fn as_input_edit(self) -> InputEdit {
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

/// Edit coordinates and structural changes are inputs to cache invalidation,
/// not a complete highlight invalidation boundary: query context can extend them.
#[derive(Clone, Debug)]
pub struct SyntaxChange {
    pub edit: SyntaxEdit,
    pub(crate) owner: Arc<()>,
    pub(crate) revision: u64,
    pub(crate) structural_ranges: OnceLock<Vec<TextRange>>,
    pub(crate) old_tree: Tree,
    pub(crate) new_tree: Tree,
}

impl SyntaxChange {
    /// Compute structural changes in the new document's coordinates on demand.
    /// Text-only edits can leave this empty; callers must also account for
    /// `edit` in both versions. Tree comparison is deferred because it can be
    /// substantial work even when incremental parsing reused most of the tree.
    pub fn structural_ranges(&self) -> impl ExactSizeIterator<Item = TextRange> {
        self.changed_ranges().iter().copied()
    }

    fn changed_ranges(&self) -> &[TextRange] {
        self.structural_ranges.get_or_init(|| {
            self.old_tree
                .changed_ranges(&self.new_tree)
                .map(|range| {
                    TextRange::new(BufferOffset(range.start_byte), BufferOffset(range.end_byte))
                })
                .collect()
        })
    }

    pub(crate) fn unchanged_old_range(&self, range: Range<usize>) -> Option<Range<usize>> {
        let changes = self.changed_ranges();
        let first = changes.partition_point(|changed| changed.end.0 < range.start);
        if changes
            .get(first)
            .is_some_and(|changed| changed.start.0 <= range.end)
        {
            return None;
        }
        let start = if range.end < self.edit.start_byte {
            range.start
        } else if range.start > self.edit.new_end_byte {
            range.start - self.edit.new_end_byte + self.edit.old_end_byte
        } else {
            return None;
        };
        Some(start..start + range.len())
    }
}
