use std::ops::Range;

use gpui::{Bounds, Hsla, Pixels, point, px, rgba, size};
use oxideterm_terminal::{TerminalSearchMatch, TerminalSnapshot};
use oxideterm_terminal_unicode::visual_line_for_row;

use crate::terminal_ui::*;
use crate::terminal_view::element::{TerminalRect, TerminalScrollbar};

pub(crate) fn extend_or_push_rect(
    current: &mut Option<TerminalRect>,
    rects: &mut Vec<TerminalRect>,
    row: usize,
    col: usize,
    cells: usize,
    color: Hsla,
) {
    if let Some(rect) = current
        && rect.row == row
        && rect.col + rect.cells == col
        && rect.color == color
    {
        rect.cells += cells;
        return;
    }

    if let Some(rect) = current.take() {
        rects.push(rect);
    }
    *current = Some(TerminalRect {
        row,
        col,
        cells,
        color,
    });
}

pub(crate) fn terminal_scrollbar(
    snapshot: &TerminalSnapshot,
    metrics: &TerminalMetrics,
) -> Option<TerminalScrollbar> {
    let history = snapshot.scrollback_lines;
    if history == 0 {
        return None;
    }

    let viewport_height = snapshot.rows as f32 * metrics.line_height_f32();
    let total_lines = snapshot.rows + history;
    let thumb_height = (viewport_height * snapshot.rows as f32 / total_lines as f32)
        .max(SCROLLBAR_MIN_THUMB)
        .min(viewport_height);
    let scroll_fraction = (history.saturating_sub(snapshot.display_offset)) as f32 / history as f32;
    let top = (viewport_height - thumb_height) * scroll_fraction;

    Some(TerminalScrollbar {
        top,
        height: thumb_height,
    })
}

pub(crate) fn terminal_visible_rows(
    bounds: Bounds<Pixels>,
    snapshot: &TerminalSnapshot,
    metrics: &TerminalMetrics,
) -> Range<usize> {
    let visible_height = (f32::from(bounds.size.height) - TERMINAL_CONTENT_PADDING * 2.0).max(0.0);
    if visible_height <= 0.0 {
        return 0..0;
    }

    let visible_rows = (visible_height / metrics.line_height_f32()).ceil() as usize;
    0..visible_rows.min(snapshot.rows).min(snapshot.lines.len())
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
                color: rgba(0xffcc6644).into(),
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
                        rgba(0xffdd8899).into()
                    } else {
                        rgba(0xffcc6644).into()
                    },
                })
            })
        })
        .collect()
}

pub(crate) fn terminal_content_bounds(
    origin: gpui::Point<Pixels>,
    snapshot: &TerminalSnapshot,
    metrics: &TerminalMetrics,
) -> Bounds<Pixels> {
    Bounds::new(
        origin,
        size(
            px(snapshot.cols as f32 * metrics.cell_width_f32()),
            px(snapshot.rows as f32 * metrics.line_height_f32()),
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
        .map(visual_line_for_row)
        .filter(|line| line.has_bidi)
        .map(|line| line.visual_col_for_logical_col(snapshot.cursor_col))
        .unwrap_or(snapshot.cursor_col)
}
