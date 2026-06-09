// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use gpui::Pixels;

use super::{DisplayRowsCache, FoldRange, TextEditorView};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DisplayRow {
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub is_first: bool,
    pub is_folded_header: bool,
}

impl TextEditorView {
    pub(super) fn display_row_for_window_y(&self, y: Pixels) -> Option<DisplayRow> {
        let bounds = self.content_bounds?;
        let relative_y = f32::from(y - bounds.origin.y) + self.viewport.scroll_y_px;
        let display_index = (relative_y / self.metrics.line_height).floor().max(0.0) as usize;
        self.display_rows().get(display_index).copied()
    }

    pub(super) fn document_row_count(&self) -> usize {
        self.display_rows().len().max(1)
    }

    pub(super) fn display_rows(&self) -> Arc<Vec<DisplayRow>> {
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
        *self.display_rows_cache.borrow_mut() = Some(DisplayRowsCache {
            buffer_version,
            wrap_column,
            fold_revision: self.fold_revision,
            rows: rows.clone(),
        });
        rows
    }

    fn compute_display_rows(&self, wrap_column: Option<usize>) -> Vec<DisplayRow> {
        let line_lengths = self.buffer.line_char_counts();
        compute_display_rows_from_line_lengths(&line_lengths, &self.folded_ranges, wrap_column)
    }

    pub(super) fn display_row_index_for_line(&self, line: usize) -> Option<usize> {
        self.display_rows()
            .iter()
            .position(|display_row| display_row.line == line)
    }

    fn wrap_column(&self) -> Option<usize> {
        if !self.settings.soft_wrap {
            return None;
        }
        let bounds = self.content_bounds?;
        let available_width = f32::from(bounds.size.width)
            - self.metrics.gutter_width
            - self.metrics.content_padding_x * 2.0;
        let measured = (available_width / self.metrics.char_width).floor().max(8.0) as usize;
        Some(measured.min(self.settings.soft_wrap_column.max(8)))
    }
}

fn compute_display_rows_from_line_lengths(
    line_lengths: &[usize],
    folded_ranges: &[FoldRange],
    wrap_column: Option<usize>,
) -> Vec<DisplayRow> {
    let mut rows = Vec::new();
    let mut line = 0;
    while line < line_lengths.len() {
        let visual_len = line_lengths[line];
        if let Some(folded) = folded_ranges
            .iter()
            .find(|range| range.start_line == line)
            .copied()
        {
            rows.push(DisplayRow {
                line,
                start_col: 0,
                end_col: visual_len,
                is_first: true,
                is_folded_header: true,
            });
            line = folded.end_line.saturating_add(1);
            continue;
        }
        let Some(wrap_column) = wrap_column.filter(|column| visual_len > *column) else {
            rows.push(DisplayRow {
                line,
                start_col: 0,
                end_col: visual_len,
                is_first: true,
                is_folded_header: false,
            });
            line += 1;
            continue;
        };
        let mut start_col = 0;
        while start_col < visual_len {
            let end_col = (start_col + wrap_column).min(visual_len);
            rows.push(DisplayRow {
                line,
                start_col,
                end_col,
                is_first: start_col == 0,
                is_folded_header: false,
            });
            start_col = end_col;
        }
        line += 1;
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::{DisplayRow, FoldRange, compute_display_rows_from_line_lengths};

    #[test]
    fn folded_rows_hide_inner_lines() {
        let rows = compute_display_rows_from_line_lengths(
            &[9, 8, 1, 6],
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
}
