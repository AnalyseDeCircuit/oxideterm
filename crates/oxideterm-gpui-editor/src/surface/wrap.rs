// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use gpui::Pixels;
use oxideterm_editor_core::TextRange;
use unicode_segmentation::UnicodeSegmentation;

#[cfg(test)]
use super::FoldRange;
use super::{DisplayRowsCache, TextEditorView, coords::grapheme_visual_width};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DisplayRow {
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub is_first: bool,
    pub is_folded_header: bool,
}

/// Ordinary rows have implicit line numbers and flags; retain only their widths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum DisplayRows {
    Unwrapped(Vec<usize>),
    Explicit(Vec<DisplayRow>),
}

impl DisplayRows {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Unwrapped(widths) => widths.len(),
            Self::Explicit(rows) => rows.len(),
        }
    }

    pub(super) fn get(&self, index: usize) -> Option<DisplayRow> {
        match self {
            Self::Unwrapped(widths) => widths.get(index).map(|&width| DisplayRow {
                line: index,
                start_col: 0,
                end_col: width,
                is_first: true,
                is_folded_header: false,
            }),
            Self::Explicit(rows) => rows.get(index).copied(),
        }
    }

    pub(super) fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = DisplayRow> + ExactSizeIterator + '_ {
        (0..self.len()).map(|index| self.get(index).expect("row index is in bounds"))
    }

    fn max_width(&self) -> usize {
        match self {
            Self::Unwrapped(widths) => widths.iter().copied().max().unwrap_or(0),
            Self::Explicit(rows) => rows
                .iter()
                .map(|row| row.end_col.saturating_sub(row.start_col))
                .max()
                .unwrap_or(0),
        }
    }
}

pub(super) struct UnwrappedRowEdit {
    cache: DisplayRowsCache,
    first_line: usize,
    last_line: usize,
    newlines: usize,
}

impl TextEditorView {
    pub(super) fn unwrapped_row_edit(
        &self,
        range: TextRange,
        replacement: &str,
    ) -> Option<UnwrappedRowEdit> {
        let first_line = self.buffer.offset_to_line_col(range.start).ok()?.line;
        let last_line = self.buffer.offset_to_line_col(range.end).ok()?.line;
        let mut cached = self.display_rows_cache.borrow_mut();
        let cache = cached.as_ref()?;
        if cache.wrap_column.is_some()
            || !self.folded_ranges.is_empty()
            || cache.buffer_version != self.buffer.version()
        {
            return None;
        }
        Some(UnwrappedRowEdit {
            cache: cached.take()?,
            first_line,
            last_line,
            newlines: replacement.bytes().filter(|b| *b == b'\n').count(),
        })
    }

    pub(super) fn restore_unwrapped_rows_after_edit(&self, edit: Option<UnwrappedRowEdit>) {
        let Some(mut edit) = edit else {
            return;
        };
        if self.wrap_column().is_some() {
            return;
        }
        let DisplayRows::Unwrapped(widths) = Arc::make_mut(&mut edit.cache.rows) else {
            return;
        };
        let new_widths = (edit.first_line..edit.first_line + edit.newlines + 1)
            .map(|line| {
                self.buffer
                    .with_line_text(line, unwrapped_line_width)
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>();
        let removed_widest =
            widths[edit.first_line..=edit.last_line].contains(&edit.cache.max_width_columns);
        let new_max = new_widths.iter().copied().max().unwrap_or(0);
        widths.splice(edit.first_line..=edit.last_line, new_widths);
        edit.cache.max_width_columns = if removed_widest && new_max < edit.cache.max_width_columns {
            widths.iter().copied().max().unwrap_or(0)
        } else {
            edit.cache.max_width_columns.max(new_max)
        };
        edit.cache.buffer_version = self.buffer.version();
        edit.cache.fold_revision = self.fold_revision;
        *self.display_rows_cache.borrow_mut() = Some(edit.cache);
    }

    pub(super) fn display_row_for_window_y(&self, y: Pixels) -> Option<DisplayRow> {
        let bounds = self.content_bounds?;
        let relative_y = f32::from(y - bounds.origin.y) + self.vertical_scroll_y_px();
        let display_index = (relative_y / self.metrics.line_height).floor().max(0.0) as usize;
        self.display_rows().get(display_index)
    }

    pub(super) fn document_row_count(&self) -> usize {
        self.display_rows().len().max(1)
    }

    pub(super) fn display_rows(&self) -> Arc<DisplayRows> {
        let wrap_column = self.wrap_column();
        let buffer_version = self.buffer.version();
        if let Some(cache) = self.display_rows_cache.borrow().as_ref()
            && cache.buffer_version == buffer_version
            && cache.wrap_column == wrap_column
            && cache.fold_revision == self.fold_revision
        {
            return cache.rows.clone();
        }

        let rows = Arc::new(self.compute_display_rows(wrap_column));
        let max_width_columns = rows.max_width();
        *self.display_rows_cache.borrow_mut() = Some(DisplayRowsCache {
            buffer_version,
            wrap_column,
            fold_revision: self.fold_revision,
            max_width_columns,
            rows: rows.clone(),
        });
        rows
    }

    pub(super) fn document_width_columns(&self) -> usize {
        // Populate the shared row cache once, then reuse its width summary on every scroll frame.
        let _ = self.display_rows();
        self.display_rows_cache
            .borrow()
            .as_ref()
            .map(|cache| cache.max_width_columns)
            .unwrap_or(0)
    }

    fn compute_display_rows(&self, wrap_column: Option<usize>) -> DisplayRows {
        if wrap_column.is_none() && self.folded_ranges.is_empty() {
            return DisplayRows::Unwrapped(
                (0..self.buffer.line_count())
                    .map(|line| {
                        self.buffer
                            .with_line_text(line, unwrapped_line_width)
                            .unwrap_or(0)
                    })
                    .collect(),
            );
        }
        let mut rows = Vec::new();
        let mut line = 0;
        while line < self.buffer.line_count() {
            let folded = self
                .folded_ranges
                .iter()
                .find(|range| range.start_line == line)
                .copied();
            let line_wrap_column = if folded.is_some() { None } else { wrap_column };
            self.buffer
                .with_line_text(line, |text| {
                    if line_wrap_column.is_none() {
                        rows.push(DisplayRow {
                            line,
                            start_col: 0,
                            end_col: unwrapped_line_width(text),
                            is_first: true,
                            is_folded_header: folded.is_some(),
                        });
                    } else {
                        append_display_rows_for_line(
                            &mut rows,
                            line,
                            text.graphemes(true).map(grapheme_visual_width),
                            line_wrap_column,
                            folded.is_some(),
                        );
                    }
                })
                .unwrap_or_else(|| {
                    append_display_rows_for_line(
                        &mut rows,
                        line,
                        std::iter::empty(),
                        line_wrap_column,
                        folded.is_some(),
                    );
                });
            line = folded
                .map(|range| range.end_line.saturating_add(1))
                .unwrap_or_else(|| line + 1);
        }
        DisplayRows::Explicit(rows)
    }

    fn wrap_column(&self) -> Option<usize> {
        if self.is_large_file() || !self.settings.soft_wrap {
            return None;
        }
        let bounds = self.content_bounds?;
        let available_width = f32::from(bounds.size.width)
            - self.visible_gutter_width()
            - self.visible_content_padding_x() * 2.0;
        let measured = (available_width / self.metrics.char_width).floor().max(8.0) as usize;
        Some(measured.min(self.settings.soft_wrap_column.max(8)))
    }
}

fn unwrapped_line_width(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.graphemes(true).map(grapheme_visual_width).sum()
    }
}

pub(super) fn display_row_for_visual_column(
    rows: &DisplayRows,
    line: usize,
    visual_column: usize,
) -> Option<(usize, DisplayRow, usize)> {
    if matches!(rows, DisplayRows::Unwrapped(_)) {
        return rows.get(line).map(|row| (line, row, visual_column));
    }
    // Wrapped segments share their boundary column. Assign that caret slot to
    // the later segment, while the physical line ending remains on its last row.
    let index = rows
        .iter()
        .enumerate()
        .rfind(|(_, row)| {
            row.line == line && visual_column >= row.start_col && visual_column <= row.end_col
        })
        .map(|(index, _)| index)
        .or_else(|| rows.iter().rposition(|row| row.line == line))?;
    let row = rows.get(index)?;
    Some((index, row, visual_column.saturating_sub(row.start_col)))
}

#[cfg(test)]
fn compute_display_rows_from_grapheme_widths(
    line_grapheme_widths: &[Vec<usize>],
    folded_ranges: &[FoldRange],
    wrap_column: Option<usize>,
) -> Vec<DisplayRow> {
    let mut rows = Vec::new();
    let mut line = 0;
    while line < line_grapheme_widths.len() {
        let folded = folded_ranges
            .iter()
            .find(|range| range.start_line == line)
            .copied();
        append_display_rows_for_line(
            &mut rows,
            line,
            line_grapheme_widths[line].iter().copied(),
            if folded.is_some() { None } else { wrap_column },
            folded.is_some(),
        );
        line = folded
            .map(|range| range.end_line.saturating_add(1))
            .unwrap_or_else(|| line + 1);
    }
    rows
}

fn append_display_rows_for_line(
    rows: &mut Vec<DisplayRow>,
    line: usize,
    grapheme_widths: impl IntoIterator<Item = usize>,
    wrap_column: Option<usize>,
    is_folded_header: bool,
) {
    let mut start_col = 0;
    let mut end_col = 0;
    for grapheme_width in grapheme_widths {
        if wrap_column.is_some_and(|column| {
            end_col > start_col && end_col + grapheme_width > start_col + column
        }) {
            rows.push(DisplayRow {
                line,
                start_col,
                end_col,
                is_first: start_col == 0,
                is_folded_header: false,
            });
            start_col = end_col;
        }
        end_col += grapheme_width;
    }
    // Every physical line owns at least one display row, including empty lines.
    rows.push(DisplayRow {
        line,
        start_col,
        end_col,
        is_first: start_col == 0,
        is_folded_header,
    });
}

#[cfg(test)]
mod tests {
    use super::{
        DisplayRow, DisplayRows, FoldRange, compute_display_rows_from_grapheme_widths,
        display_row_for_visual_column,
    };

    fn ascii_line_widths(lengths: &[usize]) -> Vec<Vec<usize>> {
        lengths.iter().map(|length| vec![1; *length]).collect()
    }

    #[test]
    fn folded_rows_hide_inner_lines() {
        let rows = compute_display_rows_from_grapheme_widths(
            &ascii_line_widths(&[9, 8, 1, 6]),
            &[FoldRange {
                start_line: 0,
                end_line: 2,
            }],
            None,
        );

        assert_eq!(
            rows,
            vec![
                DisplayRow {
                    line: 0,
                    start_col: 0,
                    end_col: 9,
                    is_first: true,
                    is_folded_header: true,
                },
                DisplayRow {
                    line: 3,
                    start_col: 0,
                    end_col: 6,
                    is_first: true,
                    is_folded_header: false,
                },
            ]
        );
    }

    #[test]
    fn wrapped_boundary_belongs_to_the_later_display_row() {
        let rows =
            compute_display_rows_from_grapheme_widths(&ascii_line_widths(&[16]), &[], Some(8));

        let rows = DisplayRows::Explicit(rows);
        assert_eq!(display_row_for_visual_column(&rows, 0, 7).unwrap().0, 0);
        assert_eq!(display_row_for_visual_column(&rows, 0, 8).unwrap().0, 1);
        assert_eq!(display_row_for_visual_column(&rows, 0, 16).unwrap().0, 1);
    }

    #[test]
    fn wrapping_never_splits_a_wide_grapheme() {
        let rows = compute_display_rows_from_grapheme_widths(&[vec![1, 2, 2, 1]], &[], Some(4));

        assert_eq!(
            rows,
            vec![
                DisplayRow {
                    line: 0,
                    start_col: 0,
                    end_col: 3,
                    is_first: true,
                    is_folded_header: false,
                },
                DisplayRow {
                    line: 0,
                    start_col: 3,
                    end_col: 6,
                    is_first: false,
                    is_folded_header: false,
                },
            ]
        );
    }
}

#[cfg(test)]
mod edit_layout_tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};
    use oxideterm_editor_core::{BufferOffset, Selection};
    use oxideterm_theme::default_tokens;

    #[gpui::test]
    fn compact_rows_transition_to_folding_and_wrapping(cx: &mut TestAppContext) {
        use gpui::{Bounds, point, px, size};
        use oxideterm_editor_syntax::LanguageId;
        let editor = cx.new(|cx| {
            TextEditorView::new("fn sample() {\n    call();\n}\nlast", &default_tokens(), cx)
        });
        editor.update(cx, |editor, cx| {
            editor.set_language(Some(LanguageId::Rust), cx)
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            editor.settings.soft_wrap = false;
            let original = editor.display_rows();
            assert_eq!(
                original
                    .iter()
                    .map(|row| (row.line, row.end_col))
                    .collect::<Vec<_>>(),
                [(0, 13), (1, 11), (2, 1), (3, 4)]
            );
            let (index, row, column) = display_row_for_visual_column(&original, 0, 50).unwrap();
            assert_eq!((index, row.line, column), (0, 0, 50));
            assert!(editor.toggle_fold_at_line(0, cx));
            let folded = editor.display_rows();
            assert_eq!(
                folded
                    .iter()
                    .map(|row| (row.line, row.is_folded_header))
                    .collect::<Vec<_>>(),
                [(0, true), (3, false)]
            );
            assert!(editor.toggle_fold_at_line(0, cx));
            assert_eq!(*editor.display_rows(), *original);
            editor.content_bounds = Some(Bounds::new(
                point(px(0.0), px(0.0)),
                size(px(1000.0), px(500.0)),
            ));
            editor.settings.soft_wrap = true;
            editor.settings.soft_wrap_column = 8;
            let wrapped = editor.display_rows();
            let (index, row, column) = display_row_for_visual_column(&wrapped, 0, 8).unwrap();
            assert_eq!((index, row.start_col, row.end_col, column), (1, 8, 13, 0));
        });
    }

    #[gpui::test]
    fn unwrapped_edits_update_rows_without_changing_retained_layout(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| TextEditorView::new("a\n中🙂\nend", &default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            editor.settings.soft_wrap = false;
            let old_rows = editor.display_rows();
            editor.insert_text("long\n", cx);
            let widths = editor
                .display_rows()
                .iter()
                .map(|r| (r.line, r.end_col))
                .collect::<Vec<_>>();
            assert_eq!(widths, vec![(0, 4), (1, 1), (2, 4), (3, 3)]);
            assert_eq!(old_rows.len(), 3);
            assert_eq!(old_rows.get(1).unwrap().end_col, 4);
            editor
                .cursor
                .set_selection(Selection::new(BufferOffset(0), BufferOffset(5)));
            editor.insert_text("", cx);
            assert_eq!(*editor.display_rows(), *old_rows);
            editor
                .cursor
                .set_selection(Selection::new(BufferOffset(2), BufferOffset(9)));
            editor.insert_text("x", cx);
            assert_eq!(editor.document_width_columns(), 3);
        });
    }
}
