// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Pixels;

use super::TextEditorView;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DisplayRow {
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub is_first: bool,
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

    pub(super) fn display_rows(&self) -> Vec<DisplayRow> {
        let wrap_column = self.wrap_column();
        let mut rows = Vec::new();
        for line in 0..self.buffer.line_count() {
            let line_text = self.buffer.line_text(line).unwrap_or_default();
            let visual_len = line_text.chars().count();
            let Some(wrap_column) = wrap_column.filter(|column| visual_len > *column) else {
                rows.push(DisplayRow {
                    line,
                    start_col: 0,
                    end_col: visual_len,
                    is_first: true,
                });
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
                });
                start_col = end_col;
            }
        }
        rows
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
