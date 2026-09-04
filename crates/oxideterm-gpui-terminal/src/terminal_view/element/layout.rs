use std::ops::Range;

use gpui::{Bounds, IntoColor, Pixels, point, px, rgba, size};
use oxideterm_terminal::{TerminalSearchMatch, TerminalSnapshot};
use oxideterm_terminal_unicode::visual_line_for_row_if_bidi;

use crate::terminal_ui::*;
use crate::terminal_view::element::{TerminalHorizontalScrollbar, TerminalRect, TerminalScrollbar};

pub(crate) fn terminal_scrollbar_for_viewport_display_offset(
    snapshot: &TerminalSnapshot,
    metrics: &TerminalMetrics,
    viewport_rows: usize,
    display_offset_rows: f32,
) -> Option<TerminalScrollbar> {
    let history = snapshot.scrollback_lines;
    if history == 0 {
        return None;
    }

    let viewport_height = viewport_rows as f32 * metrics.line_height_f32();
    let total_lines = viewport_rows + history;
    let thumb_height = (viewport_height * viewport_rows as f32 / total_lines as f32)
        .max(SCROLLBAR_MIN_THUMB)
        .min(viewport_height);
    let display_offset = display_offset_rows.clamp(0.0, history as f32);
    let scroll_fraction = (history as f32 - display_offset) / history as f32;
    let top = (viewport_height - thumb_height) * scroll_fraction;

    Some(TerminalScrollbar {
        top,
        height: thumb_height,
    })
}

pub(crate) fn terminal_horizontal_scrollbar_for_viewport(
    viewport_width: f32,
    max_scroll: f32,
    scroll_offset: f32,
) -> Option<TerminalHorizontalScrollbar> {
    // Only the timestamp overlay width is scrollable; the PTY grid remains
    // fixed while this geometry exposes the otherwise clipped columns.
    if viewport_width <= 0.0 || max_scroll <= 0.0 {
        return None;
    }
    let content_width = viewport_width + max_scroll;
    let width = (viewport_width / content_width * viewport_width)
        .clamp(SCROLLBAR_MIN_THUMB.min(viewport_width), viewport_width);
    let thumb_travel = (viewport_width - width).max(0.0);
    let left = scroll_offset.clamp(0.0, max_scroll) / max_scroll * thumb_travel;
    Some(TerminalHorizontalScrollbar {
        left,
        width,
        max_scroll,
    })
}

#[cfg(test)]
mod horizontal_scrollbar_tests {
    use super::*;

    #[test]
    fn horizontal_thumb_reaches_both_ends_of_the_overflow_range() {
        let start = terminal_horizontal_scrollbar_for_viewport(800.0, 120.0, 0.0)
            .expect("timestamp gutter overflow");
        let end = terminal_horizontal_scrollbar_for_viewport(800.0, 120.0, 120.0)
            .expect("timestamp gutter overflow");

        assert_eq!(start.left, 0.0);
        assert!((end.left - (800.0 - end.width)).abs() <= f32::EPSILON);
    }
}

pub(crate) fn terminal_visible_rows_for_limit(
    bounds: Bounds<Pixels>,
    metrics: &TerminalMetrics,
    row_limit: usize,
) -> Range<usize> {
    let visible_height = (f32::from(bounds.size.height) - TERMINAL_CONTENT_PADDING * 2.0).max(0.0);
    if visible_height <= 0.0 {
        return 0..0;
    }

    let visible_rows = (visible_height / metrics.line_height_f32()).ceil() as usize;
    0..visible_rows.min(row_limit)
}

#[allow(dead_code)]
pub(crate) fn search_match_rects(
    snapshot: &TerminalSnapshot,
    query: Option<&str>,
) -> Vec<TerminalRect> {
    search_match_rects_for_rows(snapshot, query, 0..snapshot.lines.len())
}

pub(crate) fn search_match_rects_for_rows(
    snapshot: &TerminalSnapshot,
    query: Option<&str>,
    rows: Range<usize>,
) -> Vec<TerminalRect> {
    let Some(query) = query.filter(|query| !query.is_empty()) else {
        return Vec::new();
    };
    let query_len = query.chars().count();
    if query_len == 0 {
        return Vec::new();
    }

    let mut rects = Vec::new();
    for row_index in rows {
        let Some(row) = snapshot.lines.get(row_index) else {
            continue;
        };
        let text = row.text();
        for start_byte in text.match_indices(query).map(|(index, _)| index) {
            let start_col = text[..start_byte].chars().count();
            if start_col >= snapshot.cols {
                continue;
            }
            let cells = query_len.min(snapshot.cols.saturating_sub(start_col));
            rects.push(TerminalRect {
                row: row_index,
                col: start_col,
                cells,
                color: rgba(0xffcc6644).into_color(),
            });
        }
    }
    rects
}

pub(crate) fn visible_search_match_rects(
    matches: &[TerminalSearchMatch],
    display_offset: usize,
    rows: Range<usize>,
    selected_match: Option<usize>,
) -> Vec<TerminalRect> {
    matches
        .iter()
        .enumerate()
        .flat_map(|(index, search_match)| {
            let rows = rows.clone();
            search_match.ranges.iter().filter_map(move |range| {
                let row = (range.line + display_offset as i32).try_into().ok()?;
                if !rows.contains(&row) {
                    return None;
                }

                Some(TerminalRect {
                    row,
                    col: range.start_col,
                    cells: range.end_col.saturating_sub(range.start_col),
                    color: if selected_match == Some(index) {
                        rgba(0xffdd8899).into_color()
                    } else {
                        rgba(0xffcc6644).into_color()
                    },
                })
            })
        })
        .collect()
}

pub(crate) fn terminal_content_bounds_for_rows(
    origin: gpui::Point<Pixels>,
    rows: usize,
    cols: usize,
    metrics: &TerminalMetrics,
) -> Bounds<Pixels> {
    Bounds::new(
        origin,
        size(
            px(cols as f32 * metrics.cell_width_f32()),
            px(rows as f32 * metrics.line_height_f32()),
        ),
    )
}

pub(crate) fn ime_cursor_bounds_for_snapshot(
    snapshot: &TerminalSnapshot,
    metrics: &TerminalMetrics,
) -> Option<Bounds<Pixels>> {
    if snapshot.cursor_row >= snapshot.rows || snapshot.cursor_col >= snapshot.cols {
        return None;
    }

    let cell_width = snapshot
        .lines
        .get(snapshot.cursor_row)
        .and_then(|row| row.cells.get(snapshot.cursor_col))
        .map(|cell| {
            if cell.ch.is_whitespace() {
                1
            } else if cell.wide {
                2
            } else {
                1
            }
        })
        .unwrap_or(1);

    Some(Bounds::new(
        point(
            px(cursor_visual_col(snapshot) as f32 * metrics.cell_width_f32()),
            px(snapshot.cursor_row as f32 * metrics.line_height_f32()),
        ),
        size(
            px(cell_width as f32 * metrics.cell_width_f32()),
            metrics.line_height,
        ),
    ))
}

fn cursor_visual_col(snapshot: &TerminalSnapshot) -> usize {
    snapshot
        .lines
        .get(snapshot.cursor_row)
        // Ordinary cursor rows do not need an allocated visual mapping.
        .and_then(visual_line_for_row_if_bidi)
        .map(|line| line.visual_col_for_logical_col(snapshot.cursor_col))
        .unwrap_or(snapshot.cursor_col)
}
