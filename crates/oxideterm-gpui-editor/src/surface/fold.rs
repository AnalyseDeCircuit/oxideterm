// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Context;
use oxideterm_editor_core::Selection;

use super::{FoldRange, TextEditorView};

impl TextEditorView {
    pub fn toggle_fold_at_line(&mut self, line: usize, cx: &mut Context<Self>) -> bool {
        let Some(range) = self.foldable_range_starting_at(line) else {
            return false;
        };
        if let Some(index) = self
            .folded_ranges
            .iter()
            .position(|folded| folded.start_line == range.start_line)
        {
            self.folded_ranges.remove(index);
        } else {
            // Fold ranges are visual ownership boundaries. Remove nested or
            // overlapping folds so the virtual row model can skip one clear
            // range instead of reconciling competing hidden-line claims.
            self.folded_ranges
                .retain(|folded| !fold_ranges_overlap(*folded, range));
            self.folded_ranges.push(range);
            self.folded_ranges
                .sort_by_key(|folded| (folded.start_line, folded.end_line));
            self.move_caret_to_fold_header_if_hidden(range, cx);
        }
        self.invalidate_display_rows();
        cx.notify();
        true
    }

    pub(super) fn foldable_range_starting_at(&self, line: usize) -> Option<FoldRange> {
        self.structure_cache
            .fold_at_line(line)
            .map(|(start_line, end_line)| FoldRange {
                start_line,
                end_line,
            })
    }

    pub(super) fn folded_range_containing_line(&self, line: usize) -> Option<FoldRange> {
        self.folded_ranges
            .iter()
            .copied()
            .find(|range| line > range.start_line && line <= range.end_line)
    }

    pub(super) fn clear_folds_after_buffer_change(&mut self) {
        if !self.folded_ranges.is_empty() {
            self.folded_ranges.clear();
        }
        self.invalidate_display_rows();
    }

    pub(super) fn refresh_foldable_ranges(&mut self) {
        self.folded_ranges.retain(|folded| {
            self.structure_cache.fold_at_line(folded.start_line)
                == Some((folded.start_line, folded.end_line))
        });
        self.invalidate_display_rows();
    }

    pub(super) fn unfold_line_if_hidden(&mut self, line: usize) -> bool {
        let Some(range) = self.folded_range_containing_line(line) else {
            return false;
        };
        self.folded_ranges
            .retain(|folded| folded.start_line != range.start_line);
        self.invalidate_display_rows();
        true
    }

    fn invalidate_display_rows(&mut self) {
        self.fold_revision = self.fold_revision.wrapping_add(1);
        *self.display_rows_cache.borrow_mut() = None;
    }

    fn move_caret_to_fold_header_if_hidden(&mut self, range: FoldRange, cx: &mut Context<Self>) {
        let Ok(position) = self.buffer.offset_to_line_col(self.cursor.selection().head) else {
            return;
        };
        if position.line <= range.start_line || position.line > range.end_line {
            return;
        }
        if let Some(offset) = self.buffer.line_start_offset(range.start_line) {
            self.cursor.set_selection(Selection::caret(offset));
            self.secondary_selections.clear();
            self.marked_text = None;
            cx.notify();
        }
    }
}

fn fold_ranges_overlap(left: FoldRange, right: FoldRange) -> bool {
    left.start_line <= right.end_line && right.start_line <= left.end_line
}
