use std::ops::Range;

use super::TerminalPoint;
use chrono::{DateTime, Local};
use gpui::{Context, Pixels, Point};
use oxideterm_terminal::{TerminalCell, TerminalSnapshot};

use super::{TerminalPane, TerminalPaneEvent};
use crate::terminal_ui::TERMINAL_CONTENT_PADDING;

pub(super) struct TerminalTextHover {
    row: usize,
    columns: Range<usize>,
    pub(super) generation: u64,
    label: String,
    position: Point<Pixels>,
}

impl TerminalPane {
    pub fn hover_tooltip(&self) -> Option<(&str, Point<Pixels>)> {
        if let Some(hover) = &self.interactive_trigger_hover {
            return hover
                .tooltip
                .as_ref()
                .map(|text| (text.as_str(), hover.position));
        }
        self.archive_hover
            .as_ref()
            .or(self.timestamp_hover.as_ref())
            .map(|hover| (hover.label.as_str(), hover.position))
    }

    pub fn clear_terminal_hover(&mut self, cx: &mut Context<Self>) {
        let custom = self.interactive_trigger_hover.take().is_some();
        if custom {
            self.hovered_link = None;
        }
        let timestamp = self.timestamp_hover.take().is_some();
        let archive = self.archive_hover.take().is_some();
        if timestamp || archive || custom {
            cx.emit(TerminalPaneEvent::HoverTooltipChanged);
        }
    }

    pub(super) fn update_archive_hover(
        &mut self,
        link: &super::TerminalLinkRange,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.archive_hover.as_ref().is_some_and(|hover| {
            hover.generation == self.snapshot_generation
                && hover.row == link.row
                && hover.columns == (link.start_col..link.end_col)
        }) {
            return;
        }
        self.clear_terminal_hover(cx);
        let label = if self.settings.open_links_with_modifier {
            self.preferences.archive_link_modifier_hint.replace(
                "{{modifier}}",
                if cfg!(target_os = "macos") {
                    "⌘"
                } else {
                    "Ctrl"
                },
            )
        } else {
            self.preferences.archive_link_hint.clone()
        };
        self.archive_hover = Some(TerminalTextHover {
            row: link.row,
            columns: link.start_col..link.end_col,
            generation: self.snapshot_generation,
            label,
            position,
        });
        cx.emit(TerminalPaneEvent::HoverTooltipChanged);
    }

    pub(super) fn update_timestamp_hover(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(point) = self.hovered_text_point(position) else {
            self.clear_terminal_hover(cx);
            return;
        };
        if self.timestamp_hover.as_ref().is_some_and(|hover| {
            hover.generation == self.snapshot_generation
                && hover.row == point.row
                && hover.columns.contains(&point.col)
        }) {
            return;
        }
        let Some((columns, label)) =
            timestamp_at_cell(self.displayed_snapshot(), point.row, point.col)
        else {
            self.clear_terminal_hover(cx);
            return;
        };
        self.clear_terminal_hover(cx);
        self.timestamp_hover = Some(TerminalTextHover {
            row: point.row,
            columns,
            generation: self.snapshot_generation,
            label,
            position,
        });
        cx.emit(TerminalPaneEvent::HoverTooltipChanged);
    }

    pub(super) fn hovered_text_point(&self, position: Point<Pixels>) -> Option<TerminalPoint> {
        let bounds = self.bounds.filter(|bounds| bounds.contains(&position))?;
        let x = f32::from(position.x - bounds.origin.x) - self.terminal_content_padding_x()
            + f32::from(self.horizontal_scroll_offset_px);
        let y = f32::from(position.y - bounds.origin.y)
            - TERMINAL_CONTENT_PADDING
            - f32::from(self.smooth_scroll_offset_px);
        // The shared hit test clamps outside positions; gutters and padding are not text.
        if x < 0.0
            || y < 0.0
            || x >= self.snapshot.cols as f32 * self.metrics.cell_width_f32()
            || y >= self.snapshot.rows as f32 * self.metrics.line_height_f32()
        {
            return None;
        }
        Some(self.terminal_point_for_position(position))
    }
}

fn timestamp_at_cell(
    snapshot: &TerminalSnapshot,
    row_index: usize,
    col: usize,
) -> Option<(Range<usize>, String)> {
    let row = snapshot.lines.get(row_index)?;
    let cells = &row.cells;
    let is_digit =
        |cell: &TerminalCell| cell.ch.is_ascii_digit() && !cell.wide && cell.zerowidth().is_empty();
    if !is_digit(cells.get(col)?) {
        return None;
    }
    let mut start = col;
    let mut end = col + 1;
    // Bound work even when the pointer is over a very long numeric identifier.
    while start > 0 && is_digit(&cells[start - 1]) && end - start <= 13 {
        start -= 1;
    }
    while end < cells.len() && is_digit(&cells[end]) && end - start <= 13 {
        end += 1;
    }
    let digits = end - start;
    if !matches!(digits, 10 | 13) {
        return None;
    }
    let before = if start > 0 {
        cells.get(start - 1)
    } else {
        row_index
            .checked_sub(1)
            .and_then(|index| snapshot.lines.get(index))
            .filter(|previous| previous.wrapped)
            .and_then(|previous| previous.cells.last())
    };
    let after = cells.get(end).or_else(|| {
        row.wrapped
            .then(|| snapshot.lines.get(row_index + 1)?.cells.first())
            .flatten()
    });
    // Do not interpret pieces of identifiers, signed values or decimals as timestamps.
    let is_identifier = |cell: &TerminalCell| cell.ch.is_alphanumeric() || cell.ch == '_';
    if before.is_some_and(|cell| is_identifier(cell) || matches!(cell.ch, '-' | '+' | '.'))
        || after.is_some_and(|cell| is_identifier(cell) || cell.ch == '.')
        || (end == cells.len() && row.wrapped && after.is_none())
    {
        return None;
    }
    let value = cells[start..end].iter().fold(0_i64, |value, cell| {
        value * 10 + i64::from(cell.ch as u8 - b'0')
    });
    let timestamp = if digits == 13 {
        DateTime::from_timestamp_millis(value)?
    } else {
        DateTime::from_timestamp(value, 0)?
    };
    let format = if digits == 13 {
        "%Y-%m-%d %H:%M:%S%.3f %:z"
    } else {
        "%Y-%m-%d %H:%M:%S %:z"
    };
    Some((
        start..end,
        timestamp.with_timezone(&Local).format(format).to_string(),
    ))
}
