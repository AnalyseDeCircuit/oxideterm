// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

/// Visible line window used by the GPUI surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleRows {
    pub range: Range<usize>,
    pub top_spacer_px: usize,
    pub bottom_spacer_px: usize,
}

/// Scroll state for a virtualized editor viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorViewport {
    pub scroll_x_px: f32,
    pub scroll_y_px: f32,
    pub height_px: f32,
    pub overscan_rows: usize,
}

impl EditorViewport {
    pub fn new(overscan_rows: usize) -> Self {
        Self {
            scroll_x_px: 0.0,
            scroll_y_px: 0.0,
            height_px: 0.0,
            overscan_rows,
        }
    }

    pub fn set_height(&mut self, height_px: f32) -> bool {
        let height_px = height_px.max(0.0);
        if (self.height_px - height_px).abs() < f32::EPSILON {
            return false;
        }
        self.height_px = height_px;
        true
    }

    pub fn scroll_by(&mut self, dx_px: f32, dy_px: f32, line_count: usize, line_height: f32) {
        self.scroll_x_px = (self.scroll_x_px + dx_px).max(0.0);
        self.scroll_y_px = (self.scroll_y_px + dy_px)
            .clamp(0.0, max_scroll_y(line_count, line_height, self.height_px));
    }

    pub fn clamp(&mut self, line_count: usize, line_height: f32) {
        self.scroll_y_px = self
            .scroll_y_px
            .clamp(0.0, max_scroll_y(line_count, line_height, self.height_px));
    }

    pub fn visible_rows(&self, line_count: usize, line_height: f32) -> VisibleRows {
        if line_count == 0 {
            return VisibleRows {
                range: 0..0,
                top_spacer_px: 0,
                bottom_spacer_px: 0,
            };
        }

        let first_visible = (self.scroll_y_px / line_height).floor().max(0.0) as usize;
        let start = first_visible.saturating_sub(self.overscan_rows);
        let viewport_rows = if self.height_px <= 0.0 {
            80
        } else {
            (self.height_px / line_height).ceil().max(1.0) as usize
        };
        let end = (first_visible + viewport_rows + self.overscan_rows * 2).min(line_count);
        let top_spacer_px = (start as f32 * line_height).round() as usize;
        let rendered_px = (end.saturating_sub(start) as f32 * line_height).round() as usize;
        let total_px = (line_count as f32 * line_height).round() as usize;
        let bottom_spacer_px = total_px.saturating_sub(top_spacer_px + rendered_px);

        VisibleRows {
            range: start..end,
            top_spacer_px,
            bottom_spacer_px,
        }
    }
}

fn max_scroll_y(line_count: usize, line_height: f32, viewport_height: f32) -> f32 {
    (line_count as f32 * line_height - viewport_height.max(0.0)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_overscanned_visible_rows() {
        let mut viewport = EditorViewport::new(2);
        viewport.height_px = 60.0;
        viewport.scroll_y_px = 100.0;

        let rows = viewport.visible_rows(100, 20.0);

        assert_eq!(rows.range, 3..12);
        assert_eq!(rows.top_spacer_px, 60);
        assert_eq!(rows.bottom_spacer_px, 1760);
    }

    #[test]
    fn clamps_scroll_to_document_height() {
        let mut viewport = EditorViewport::new(0);
        viewport.height_px = 100.0;

        viewport.scroll_by(12.0, 10_000.0, 8, 20.0);

        assert_eq!(viewport.scroll_x_px, 12.0);
        assert_eq!(viewport.scroll_y_px, 60.0);
    }
}
