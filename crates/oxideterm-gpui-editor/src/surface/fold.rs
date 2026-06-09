// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use gpui::Context;
use oxideterm_editor_core::Selection;
use oxideterm_editor_syntax::{FoldRange as SyntaxFoldRange, SyntaxSession};

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
        self.foldable_ranges
            .iter()
            .copied()
            .find(|range| range.start_line == line)
    }

    pub(super) fn folded_range_containing_line(&self, line: usize) -> Option<FoldRange> {
        self.folded_ranges
            .iter()
            .copied()
            .find(|range| line > range.start_line && line <= range.end_line)
    }

    pub(super) fn clear_folds_after_buffer_change(&mut self) {
        self.foldable_ranges = fold_ranges_from_syntax(self.syntax.as_ref());
        if !self.folded_ranges.is_empty() {
            self.folded_ranges.clear();
        }
        self.invalidate_display_rows();
    }

    pub(super) fn refresh_foldable_ranges(&mut self) {
        self.foldable_ranges = fold_ranges_from_syntax(self.syntax.as_ref());
        self.folded_ranges.retain(|folded| {
            self.foldable_ranges.iter().any(|range| {
                range.start_line == folded.start_line && range.end_line == folded.end_line
            })
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

fn fold_ranges_from_syntax(syntax: Option<&SyntaxSession>) -> Vec<FoldRange> {
    let Some(syntax) = syntax else {
        return Vec::new();
    };
    normalize_syntax_fold_ranges(syntax.fold_ranges())
}

fn normalize_syntax_fold_ranges(syntax_ranges: Vec<SyntaxFoldRange>) -> Vec<FoldRange> {
    let mut ranges = BTreeMap::<usize, FoldRange>::new();
    for range in syntax_ranges {
        if range.end_line <= range.start_line {
            continue;
        }
        // Multiple tree-sitter nodes can start on the same line. The gutter
        // has one control per visual line, so keep the largest visible fold.
        insert_widest_range(
            &mut ranges,
            FoldRange {
                start_line: range.start_line,
                end_line: range.end_line,
            },
        );
    }
    ranges.into_values().collect()
}

fn insert_widest_range(ranges: &mut BTreeMap<usize, FoldRange>, range: FoldRange) {
    match ranges.get(&range.start_line) {
        Some(existing) if existing.end_line >= range.end_line => {}
        _ => {
            ranges.insert(range.start_line, range);
        }
    }
}

fn fold_ranges_overlap(left: FoldRange, right: FoldRange) -> bool {
    left.start_line <= right.end_line && right.start_line <= left.end_line
}

#[cfg(test)]
mod tests {
    use oxideterm_editor_core::{BufferOffset, TextRange};
    use oxideterm_editor_syntax::LanguageId;

    use super::*;

    #[test]
    fn no_syntax_session_has_no_fold_ranges() {
        assert!(fold_ranges_from_syntax(None).is_empty());
    }

    #[test]
    fn syntax_ranges_drive_foldable_ranges() {
        let source = "fn main() {\n    if true {\n        println!(\"x\");\n    }\n}\n";
        let session = SyntaxSession::parse(LanguageId::Rust, source).unwrap();

        assert!(
            fold_ranges_from_syntax(Some(&session))
                .iter()
                .any(|range| range.start_line == 0 && range.end_line >= 3)
        );
    }

    #[test]
    fn syntax_ranges_choose_widest_range_for_same_header() {
        assert_eq!(
            normalize_syntax_fold_ranges(vec![
                syntax_range(0, 2),
                syntax_range(0, 4),
                syntax_range(1, 3),
            ]),
            vec![
                FoldRange {
                    start_line: 0,
                    end_line: 4,
                },
                FoldRange {
                    start_line: 1,
                    end_line: 3,
                },
            ]
        );
    }

    #[test]
    fn syntax_ranges_ignore_single_line_ranges() {
        assert_eq!(
            normalize_syntax_fold_ranges(vec![syntax_range(2, 2), syntax_range(3, 5)]),
            vec![FoldRange {
                start_line: 3,
                end_line: 5,
            }]
        );
    }

    fn syntax_range(start_line: usize, end_line: usize) -> SyntaxFoldRange {
        SyntaxFoldRange {
            range: TextRange::new(BufferOffset::ZERO, BufferOffset::ZERO),
            start_line,
            end_line,
        }
    }
}
