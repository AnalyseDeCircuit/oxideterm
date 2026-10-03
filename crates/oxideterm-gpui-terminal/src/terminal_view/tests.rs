use super::*;
use std::{path::Path, sync::Arc};

use gpui::{Bounds, Keystroke, Modifiers, MouseButton, Pixels, point, px, rgb, size};
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

    let bounds = hidden.ime_cursor_bounds.unwrap();
    assert_eq!(bounds.origin.x, px(0.0));
    assert_eq!(bounds.origin.y, px(0.0));
    assert_eq!(bounds.size.width, px(8.0));
    assert_eq!(bounds.size.height, px(10.0));
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
    for (foreground, background) in [(0x102030, 0xfdf6e3), (0xe6e8eb, 0x0d0f12)] {
        let theme = TerminalUiTheme {
            foreground,
            background,
            ..Default::default()
        };
        let layout = TerminalElement::new_with_images(
            cursor_snapshot(),
            Vec::new(),
            None,
            test_metrics(),
            theme,
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
        assert_eq!(marked_text.style.color, rgb(foreground).into());
        assert_eq!(
            marked_text.style.background_color,
            Some(rgb(background).into())
        );
        assert!(layout.ime_cursor_bounds.is_some());
    }
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
    let mut mark = test_command_mark("cmd-1", 0, None, None);
    mark.command = Some("ls".to_string());
    mark.duration_ms = None;
    mark.detection_source = TerminalCommandMarkDetectionSource::CommandBar;
    mark.output_confidence = TerminalCommandMarkConfidence::Unknown;

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
fn command_mark_overlays_preserve_boundaries_and_distinguish_selection_from_hover() {
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

    for (selected, hovered, first_selected, second_hovered) in [
        (None, None, false, false),
        (Some("cmd-success"), Some("cmd-failure"), true, true),
    ] {
        let layout = TerminalElement::new(
            snapshot.clone(),
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
            vec![success.clone(), failure.clone()],
            selected.map(str::to_string),
            hovered.map(str::to_string),
        )
        .layout();

        let mut overlays = layout
            .command_mark_overlays
            .iter()
            .map(|overlay| {
                (
                    overlay.start_row,
                    overlay.end_row,
                    overlay.exit_code,
                    overlay.selected,
                    overlay.hovered,
                    overlay.running,
                )
            })
            .collect::<Vec<_>>();
        overlays.sort_by_key(|overlay| overlay.0);
        assert_eq!(
            overlays,
            [
                (0, 1, Some(0), first_selected, false, false),
                (2, 4, Some(1), false, second_hovered, false),
            ],
            "selected={selected:?}, hovered={hovered:?}"
        );
    }
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
        command_line_clipped: false,
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
fn cursor_blink_respects_preference_terminal_state_and_surface_ownership() {
    use TerminalBlinkMode::{Off, On, TerminalControlled};
    use TerminalCursorShape::{Block, Hidden};

    for (mode, focused, terminal_blinking, alt_screen, shape, expected) in [
        (On, true, false, false, Block, true),
        (TerminalControlled, true, false, false, Block, false),
        (TerminalControlled, true, true, false, Block, true),
        (On, false, true, false, Block, false),
        (On, true, true, true, Block, false),
        (On, true, true, false, Hidden, false),
        (Off, true, true, false, Block, false),
    ] {
        assert_eq!(
            should_blink_cursor_for_mode(mode, focused, terminal_blinking, alt_screen, shape),
            expected,
            "{mode:?}, focused={focused}, terminal={terminal_blinking}, alt={alt_screen}, {shape:?}"
        );
    }
}

mod input_tests;
mod layout_tests;
mod link_tests;
mod selection_tests;
