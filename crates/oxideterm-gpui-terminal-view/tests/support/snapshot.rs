use std::sync::Arc;

use super::{TerminalCell, TerminalColor, TerminalCursorShape, TerminalRow, TerminalSnapshot};

pub(super) fn test_snapshot(display_offset: usize, scrollback_lines: usize) -> TerminalSnapshot {
    TerminalSnapshot {
        row_reuse: None,
        logical_context: Vec::new(),
        generation: 0,
        cols: 80,
        rows: 10,
        cursor_col: 0,
        cursor_row: 0,
        cursor_shape: TerminalCursorShape::Block,
        display_offset,
        scrollback_lines,
        lines: Vec::new(),
        images: Vec::new(),
    }
}

pub(super) fn row_from_text(text: &str, cols: usize) -> TerminalRow {
    let mut cells = Vec::new();
    for ch in text.chars().take(cols) {
        cells.push(TerminalCell {
            ch,
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            style_origin: Default::default(),
            attrs: Default::default(),
            extra: None,
            cursor: false,
        });
    }
    while cells.len() < cols {
        cells.push(TerminalCell {
            ch: ' ',
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            style_origin: Default::default(),
            attrs: Default::default(),
            extra: None,
            cursor: false,
        });
    }
    let mut row = TerminalRow {
        line_id: 0,
        source_id: 0,
        absolute_line: 0,
        cells: Arc::new(cells),
        wrapped: false,
        active_input: false,
        signature: 0,
    };
    row.refresh_signature();
    row
}

pub(super) fn selection_snapshot(text: &str) -> TerminalSnapshot {
    let mut snapshot = test_snapshot(0, 0);
    snapshot.cols = text.chars().count().max(40);
    snapshot.rows = 1;
    snapshot.lines = vec![row_from_text(text, snapshot.cols)];
    snapshot
}

pub(super) fn row_from_text_with_wide_spacers(text: &str) -> TerminalRow {
    let mut cells = Vec::new();
    for ch in text.chars() {
        let wide = matches!(
            ch as u32,
            0x1100..=0x115f
                | 0x2e80..=0xa4cf
                | 0xac00..=0xd7a3
                | 0xf900..=0xfaff
                | 0xfe10..=0xfe19
                | 0xfe30..=0xfe6f
                | 0xff00..=0xff60
                | 0xffe0..=0xffe6
        );
        cells.push(TerminalCell {
            ch,
            wide,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            style_origin: Default::default(),
            attrs: Default::default(),
            extra: None,
            cursor: false,
        });
        if wide {
            cells.push(TerminalCell {
                ch: ' ',
                wide: false,
                fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
                bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
                style_origin: Default::default(),
                attrs: Default::default(),
                extra: None,
                cursor: false,
            });
        }
    }
    let mut row = TerminalRow {
        line_id: 0,
        source_id: 0,
        absolute_line: 0,
        cells: Arc::new(cells),
        wrapped: false,
        active_input: false,
        signature: 0,
    };
    row.refresh_signature();
    row
}

pub(super) fn multirow_snapshot(rows: &[&str]) -> TerminalSnapshot {
    let mut snapshot = test_snapshot(0, 0);
    snapshot.cols = rows
        .iter()
        .map(|row| row.chars().count())
        .max()
        .unwrap_or(1)
        .max(40);
    snapshot.rows = rows.len();
    snapshot.lines = rows
        .iter()
        .map(|row| row_from_text(row, snapshot.cols))
        .collect();
    for (row_index, row) in snapshot.lines.iter_mut().enumerate() {
        row.absolute_line = row_index as i64 - snapshot.display_offset as i64;
        row.refresh_signature();
    }
    snapshot
}
