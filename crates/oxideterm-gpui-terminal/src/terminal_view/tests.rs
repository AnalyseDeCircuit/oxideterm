use super::*;
use std::{path::Path, sync::Arc};

use gpui::{Bounds, IntoColor, Keystroke, Modifiers, MouseButton, Pixels, point, px, rgb, size};
use oxideterm_terminal::{
    TermMode, TerminalCell, TerminalColor, TerminalCommandMark, TerminalCommandMarkClosedBy,
    TerminalCommandMarkConfidence, TerminalCommandMarkDetectionSource, TerminalCursorShape,
    TerminalSearchMatch, TerminalSnapshot,
};

use crate::command_facts::TransientCommandHighlight;
use crate::terminal_ui::*;

fn test_metrics() -> TerminalMetrics {
    TerminalMetrics {
        font: terminal_font_with_family_and_cjk(
            TERMINAL_FONT,
            None,
            TERMINAL_FONT_LIGATURES,
            TERMINAL_FONT_WEIGHT,
        ),
        font_size: px(14.0),
        cell_width: px(8.0),
        line_height: px(10.0),
    }
}

fn test_snapshot(display_offset: usize, scrollback_lines: usize) -> TerminalSnapshot {
    TerminalSnapshot {
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

fn cursor_snapshot() -> TerminalSnapshot {
    let mut snapshot = test_snapshot(0, 0);
    snapshot.cols = 2;
    snapshot.rows = 1;
    snapshot.cursor_col = 0;
    snapshot.cursor_row = 0;
    snapshot.lines = vec![oxideterm_terminal::TerminalRow {
        line_id: 0,
        source_id: 0,
        absolute_line: 0,
        wrapped: false,
        active_input: false,
        signature: 0,
        cells: Arc::new(vec![
            TerminalCell {
                ch: ' ',
                wide: false,
                fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
                bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
                style_origin: Default::default(),
                attrs: Default::default(),
                extra: None,
                cursor: true,
            },
            TerminalCell {
                ch: 'x',
                wide: false,
                fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
                bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
                style_origin: Default::default(),
                attrs: Default::default(),
                extra: None,
                cursor: false,
            },
        ]),
    }];
    for row in &mut snapshot.lines {
        row.refresh_signature();
    }
    snapshot
}

fn row_from_text(text: &str, cols: usize) -> oxideterm_terminal::TerminalRow {
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
    let mut row = oxideterm_terminal::TerminalRow {
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

fn selection_snapshot(text: &str) -> TerminalSnapshot {
    let mut snapshot = test_snapshot(0, 0);
    snapshot.cols = text.chars().count().max(40);
    snapshot.rows = 1;
    snapshot.lines = vec![row_from_text(text, snapshot.cols)];
    snapshot
}

fn row_from_text_with_wide_spacers(text: &str) -> oxideterm_terminal::TerminalRow {
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
    let mut row = oxideterm_terminal::TerminalRow {
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

fn wide_snapshot(text: &str) -> TerminalSnapshot {
    let row = row_from_text_with_wide_spacers(text);
    let mut snapshot = test_snapshot(0, 0);
    snapshot.cols = row.cells.len().max(40);
    snapshot.rows = 1;
    snapshot.lines = vec![row];
    snapshot
}

fn visible_layout_bounds(rows: usize) -> Bounds<Pixels> {
    Bounds::new(
        point(px(0.0), px(0.0)),
        size(
            px(400.0),
            px(TERMINAL_CONTENT_PADDING * 2.0 + rows as f32 * test_metrics().line_height_f32()),
        ),
    )
}

fn multirow_snapshot(rows: &[&str]) -> TerminalSnapshot {
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

#[test]
fn terminal_element_hides_cursor_when_blink_cycle_is_invisible() {
    let visible = TerminalElement::new(
        cursor_snapshot(),
        None,
        test_metrics(),
        true,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .layout();
    assert!(visible.cursor.is_some());
    assert_eq!(visible.text_runs.first().unwrap().text, " ");
    assert_eq!(visible.text_runs.get(1).unwrap().text, "x");

    let hidden = TerminalElement::new(
        cursor_snapshot(),
        None,
        test_metrics(),
        false,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .layout();
    assert!(hidden.cursor.is_none());
    assert_eq!(hidden.text_runs.first().unwrap().text, "x");
    assert_eq!(hidden.text_runs.first().unwrap().col, 1);
}

#[test]
fn ime_cursor_bounds_track_terminal_cursor_even_when_cursor_blink_is_hidden() {
    let layout = TerminalElement::new(
        cursor_snapshot(),
        None,
        test_metrics(),
        false,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .layout();

    let bounds = layout.ime_cursor_bounds.unwrap();
    assert_eq!(bounds.origin.x, px(0.0));
    assert_eq!(bounds.origin.y, px(0.0));
    assert_eq!(bounds.size.width, px(8.0));
    assert_eq!(bounds.size.height, px(10.0));
    assert!(layout.cursor.is_none());
}

#[test]
fn ime_cursor_bounds_expand_for_wide_cursor_cell() {
    let mut snapshot = cursor_snapshot();
    snapshot.lines[0].cells_mut()[0].ch = '界';
    snapshot.lines[0].cells_mut()[0].wide = true;
    snapshot.lines[0].refresh_signature();

    let bounds = ime_cursor_bounds_for_snapshot(&snapshot, &test_metrics()).unwrap();

    assert_eq!(bounds.size.width, px(16.0));
    assert_eq!(bounds.size.height, px(10.0));
}

#[test]
fn marked_text_is_laid_out_at_terminal_cursor() {
    let layout = TerminalElement::new(
        cursor_snapshot(),
        None,
        test_metrics(),
        true,
        Some("拼".to_string()),
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .layout();

    let marked_text = layout.marked_text.unwrap();
    assert_eq!(marked_text.row, 0);
    assert_eq!(marked_text.col, 0);
    assert_eq!(marked_text.text, "拼");
    assert!(layout.ime_cursor_bounds.is_some());
}

#[test]
fn open_command_mark_overlay_uses_transient_prompt_boundary() {
    let mut snapshot = test_snapshot(0, 0);
    snapshot.rows = 5;
    snapshot.cols = 80;
    snapshot.cursor_row = 4;
    snapshot.lines = vec![
        row_from_text("❯ ls", snapshot.cols),
        row_from_text("file-a", snapshot.cols),
        row_from_text("file-b", snapshot.cols),
        row_from_text("   ~ ··············· lips@host 15:16:05", snapshot.cols),
        row_from_text("❯", snapshot.cols),
    ];
    let mark = TerminalCommandMark {
        command_id: "cmd-1".to_string(),
        command: Some("ls".to_string()),
        start_line: 0,
        command_line: 0,
        end_line: None,
        is_closed: false,
        closed_by: None,
        exit_code: None,
        duration_ms: None,
        detection_source: TerminalCommandMarkDetectionSource::CommandBar,
        submitted_by: None,
        confidence: TerminalCommandMarkConfidence::High,
        output_confidence: TerminalCommandMarkConfidence::Unknown,
        stale: false,
        started_at: 1,
        finished_at: None,
    };

    let layout = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .command_marks(vec![mark], Some("cmd-1".to_string()), None)
    .layout();

    assert_eq!(layout.command_mark_overlays.len(), 1);
    assert_eq!(layout.command_mark_overlays[0].start_row, 0);
    assert_eq!(layout.command_mark_overlays[0].end_row, 2);
    assert!(layout.command_mark_overlays[0].selected);
    assert!(!layout.command_mark_overlays[0].hovered);
}

#[test]
fn command_mark_overlays_include_visible_unselected_blocks() {
    let mut snapshot = test_snapshot(0, 0);
    snapshot.rows = 6;
    snapshot.cols = 80;
    snapshot.lines = vec![
        row_from_text("❯ true", snapshot.cols),
        row_from_text("ok", snapshot.cols),
        row_from_text("❯ false", snapshot.cols),
        row_from_text("err", snapshot.cols),
        row_from_text("more err", snapshot.cols),
        row_from_text("❯", snapshot.cols),
    ];
    let success = test_command_mark("cmd-success", 0, Some(1), Some(0));
    let failure = test_command_mark("cmd-failure", 2, Some(4), Some(1));

    let layout = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .command_marks(vec![success, failure], None, None)
    .layout();

    assert_eq!(layout.command_mark_overlays.len(), 2);
    assert!(
        layout
            .command_mark_overlays
            .iter()
            .all(|overlay| { !overlay.selected && !overlay.hovered && !overlay.running })
    );
    assert!(layout.command_mark_overlays.iter().any(|overlay| {
        overlay.start_row == 0 && overlay.end_row == 1 && overlay.exit_code == Some(0)
    }));
    assert!(layout.command_mark_overlays.iter().any(|overlay| {
        overlay.start_row == 2 && overlay.end_row == 4 && overlay.exit_code == Some(1)
    }));
}

#[test]
fn command_mark_overlay_distinguishes_hovered_and_selected_blocks() {
    let mut snapshot = test_snapshot(0, 0);
    snapshot.rows = 4;
    snapshot.cols = 80;
    snapshot.lines = vec![
        row_from_text("❯ pwd", snapshot.cols),
        row_from_text("/tmp", snapshot.cols),
        row_from_text("❯ ls", snapshot.cols),
        row_from_text("file", snapshot.cols),
    ];
    let selected = test_command_mark("cmd-selected", 0, Some(1), Some(0));
    let hovered = test_command_mark("cmd-hovered", 2, Some(3), Some(0));

    let layout = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .command_marks(
        vec![selected, hovered],
        Some("cmd-selected".to_string()),
        Some("cmd-hovered".to_string()),
    )
    .layout();

    let selected_overlay = layout
        .command_mark_overlays
        .iter()
        .find(|overlay| overlay.start_row == 0)
        .expect("selected overlay");
    let hovered_overlay = layout
        .command_mark_overlays
        .iter()
        .find(|overlay| overlay.start_row == 2)
        .expect("hovered overlay");
    assert!(selected_overlay.selected);
    assert!(!selected_overlay.hovered);
    assert!(!hovered_overlay.selected);
    assert!(hovered_overlay.hovered);
}

fn test_command_mark(
    command_id: &str,
    start_line: usize,
    end_line: Option<usize>,
    exit_code: Option<i32>,
) -> TerminalCommandMark {
    TerminalCommandMark {
        command_id: command_id.to_string(),
        command: Some("test".to_string()),
        start_line,
        command_line: start_line,
        end_line,
        is_closed: end_line.is_some(),
        closed_by: end_line.map(|_| TerminalCommandMarkClosedBy::ShellIntegration),
        exit_code,
        duration_ms: Some(10),
        detection_source: TerminalCommandMarkDetectionSource::ShellIntegration,
        submitted_by: None,
        confidence: TerminalCommandMarkConfidence::High,
        output_confidence: TerminalCommandMarkConfidence::High,
        stale: false,
        started_at: 1,
        finished_at: end_line.map(|_| 2),
    }
}

#[test]
fn cursor_blink_mode_on_does_not_wait_for_terminal_control_sequence() {
    assert!(should_blink_cursor_for_mode(
        TerminalBlinkMode::On,
        true,
        false,
        false,
        TerminalCursorShape::Block,
    ));
}

#[test]
fn terminal_controlled_cursor_blink_still_respects_terminal_state() {
    assert!(!should_blink_cursor_for_mode(
        TerminalBlinkMode::TerminalControlled,
        true,
        false,
        false,
        TerminalCursorShape::Block,
    ));
    assert!(should_blink_cursor_for_mode(
        TerminalBlinkMode::TerminalControlled,
        true,
        true,
        false,
        TerminalCursorShape::Block,
    ));
}

#[test]
fn cursor_blink_is_disabled_when_unfocused_alt_screen_hidden_or_off() {
    assert!(!should_blink_cursor_for_mode(
        TerminalBlinkMode::On,
        false,
        true,
        false,
        TerminalCursorShape::Block,
    ));
    assert!(!should_blink_cursor_for_mode(
        TerminalBlinkMode::On,
        true,
        true,
        true,
        TerminalCursorShape::Block,
    ));
    assert!(!should_blink_cursor_for_mode(
        TerminalBlinkMode::On,
        true,
        true,
        false,
        TerminalCursorShape::Hidden,
    ));
    assert!(!should_blink_cursor_for_mode(
        TerminalBlinkMode::Off,
        true,
        true,
        false,
        TerminalCursorShape::Block,
    ));
}

mod input_tests;
mod layout_tests;
mod link_tests;
mod selection_tests;
