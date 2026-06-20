use super::*;
use std::{path::Path, sync::Arc};

use gpui::{Bounds, FontFeatures, Keystroke, Modifiers, MouseButton, Pixels, point, px, rgb, size};
use oxideterm_terminal::{
    TermMode, TerminalCell, TerminalColor, TerminalCommandMark, TerminalCommandMarkConfidence,
    TerminalCommandMarkDetectionSource, TerminalCursorShape, TerminalSearchMatch, TerminalSnapshot,
};

use crate::terminal_ui::*;

fn test_metrics() -> TerminalMetrics {
    TerminalMetrics {
        font: terminal_font(),
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
        absolute_line: 0,
        wrapped: false,
        active_input: false,
        signature: 0,
        cells: Arc::new(vec![
            TerminalCell {
                ch: ' ',
                zerowidth: String::new(),
                wide: false,
                fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
                bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
                attrs: Default::default(),
                hyperlink: None,
                cursor: true,
            },
            TerminalCell {
                ch: 'x',
                zerowidth: String::new(),
                wide: false,
                fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
                bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
                attrs: Default::default(),
                hyperlink: None,
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
            zerowidth: String::new(),
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            attrs: Default::default(),
            hyperlink: None,
            cursor: false,
        });
    }
    while cells.len() < cols {
        cells.push(TerminalCell {
            ch: ' ',
            zerowidth: String::new(),
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            attrs: Default::default(),
            hyperlink: None,
            cursor: false,
        });
    }
    let mut row = oxideterm_terminal::TerminalRow {
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
            zerowidth: String::new(),
            wide,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            attrs: Default::default(),
            hyperlink: None,
            cursor: false,
        });
        if wide {
            cells.push(TerminalCell {
                ch: ' ',
                zerowidth: String::new(),
                wide: false,
                fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
                bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
                attrs: Default::default(),
                hyperlink: None,
                cursor: false,
            });
        }
    }
    let mut row = oxideterm_terminal::TerminalRow {
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
fn smooth_scroll_layout_includes_one_overscan_row() {
    let snapshot = multirow_snapshot(&["alpha", "bravo", "charlie", "delta"]);
    let bounds = visible_layout_bounds(3);

    let snapped = TerminalElement::new(
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
    .viewport_rows(3)
    .layout_for_bounds(bounds);
    assert!(!snapped.text_runs.iter().any(|run| run.row == 3));

    let smooth = TerminalElement::new(
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
    .viewport_rows(3)
    .scroll_y_offset(px(-1.0))
    .layout_for_bounds(bounds);
    assert!(smooth.text_runs.iter().any(|run| run.row == 3));
}

#[test]
fn scrollbar_thumb_tracks_display_offset_direction() {
    let metrics = test_metrics();
    let bottom = terminal_scrollbar(&test_snapshot(0, 90), &metrics).unwrap();
    let top = terminal_scrollbar(&test_snapshot(90, 90), &metrics).unwrap();

    assert!(bottom.top > top.top);
    assert_eq!(top.top, 0.0);
    assert_eq!(bottom.top, 76.0);
    assert_eq!(bottom.height, 24.0);
}

#[test]
fn scrollbar_thumb_tracks_fractional_smooth_offset() {
    let metrics = test_metrics();
    let snapshot = test_snapshot(10, 90);
    let current =
        terminal_scrollbar_for_viewport_display_offset(&snapshot, &metrics, 10, 10.0).unwrap();
    let halfway =
        terminal_scrollbar_for_viewport_display_offset(&snapshot, &metrics, 10, 10.5).unwrap();
    let next =
        terminal_scrollbar_for_viewport_display_offset(&snapshot, &metrics, 10, 11.0).unwrap();

    assert!(current.top > halfway.top);
    assert!(halfway.top > next.top);
}

#[test]
fn terminal_element_scrollbar_uses_fractional_display_offset() {
    let metrics = test_metrics();
    let mut snapshot = multirow_snapshot(&[
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ]);
    snapshot.display_offset = 11;
    snapshot.scrollback_lines = 90;
    snapshot.rows = 10;

    let layout = TerminalElement::new(
        snapshot.clone(),
        None,
        metrics.clone(),
        true,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .viewport_rows(10)
    .scrollbar_display_offset(10.5)
    .layout_for_bounds(visible_layout_bounds(10));
    let scrollbar = layout.scrollbar.unwrap();
    let current =
        terminal_scrollbar_for_viewport_display_offset(&snapshot, &metrics, 10, 10.0).unwrap();
    let next =
        terminal_scrollbar_for_viewport_display_offset(&snapshot, &metrics, 10, 11.0).unwrap();

    assert!(current.top > scrollbar.top);
    assert!(scrollbar.top > next.top);
}

#[test]
fn scrollbar_is_hidden_without_scrollback() {
    assert!(terminal_scrollbar(&test_snapshot(0, 0), &test_metrics()).is_none());
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
fn copy_policy_defaults_keep_current_terminal_behavior() {
    let settings = TerminalUiSettings::default();
    assert!(!settings.copy_on_select);
    assert!(settings.keep_selection_on_copy);
    assert_eq!(settings.blink_mode, TerminalBlinkMode::On);

    let preferences = TerminalUiPreferences::default();
    assert_eq!(preferences.scrollback_lines, DEFAULT_SCROLLBACK_LINES);
}

#[test]
fn terminal_theme_defaults_match_current_terminal_palette() {
    let theme = TerminalUiTheme::default();
    let tokens = oxideterm_theme::default_tokens();
    assert_eq!(theme.background, tokens.terminal.background);
    assert_eq!(theme.foreground, tokens.terminal.foreground);
    assert_eq!(theme.header_foreground, tokens.terminal.cursor);
}

#[test]
fn terminal_font_uses_real_nerd_font_family_and_fallbacks() {
    let font = terminal_font();
    assert_eq!(
        font.family.as_ref(),
        oxideterm_settings::JETBRAINS_MONO_SUBSET_FAMILY
    );
    assert_eq!(font.features, FontFeatures::disable_ligatures());
    let fallbacks = font.fallbacks.as_ref().unwrap().fallback_list();
    assert!(fallbacks.contains(&oxideterm_settings::JETBRAINS_MONO_SUBSET_FAMILY.to_string()));
    assert!(fallbacks.contains(&oxideterm_settings::MESLO_SUBSET_FAMILY.to_string()));
    assert!(fallbacks.contains(&oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY.to_string()));
    assert!(fallbacks.contains(&"JetBrainsMono Nerd Font".to_string()));
    assert!(fallbacks.contains(&"JetBrains Mono".to_string()));
    assert!(fallbacks.contains(&"MesloLGS Nerd Font Mono".to_string()));
    assert!(fallbacks.contains(&"Maple Mono NF CN".to_string()));
    assert!(fallbacks.contains(&"Apple Color Emoji".to_string()));
}

#[test]
fn terminal_font_places_explicit_cjk_fallback_before_defaults() {
    let font = terminal_font_with_family_and_cjk(
        oxideterm_settings::JETBRAINS_MONO_SUBSET_FAMILY,
        Some("Noto Sans Mono CJK SC"),
    );
    let fallbacks = font.fallbacks.as_ref().unwrap().fallback_list();
    let explicit_cjk = fallbacks
        .iter()
        .position(|family| family == "Noto Sans Mono CJK SC")
        .expect("explicit CJK fallback should be registered");
    let bundled_cjk = fallbacks
        .iter()
        .position(|family| family == oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY)
        .expect("bundled CJK fallback should remain available");

    assert!(explicit_cjk < bundled_cjk);
}

#[test]
fn oxideterm_terminal_scroll_actions_match_terminal_keymap() {
    let shift_pageup = oxideterm_terminal_scroll_action(&Keystroke {
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        key: "pageup".to_string(),
        ..Default::default()
    });
    assert_eq!(shift_pageup, Some(TerminalScrollAction::PageUp));

    let plain_pageup = oxideterm_terminal_scroll_action(&Keystroke {
        key: "pageup".to_string(),
        ..Default::default()
    });
    assert_eq!(plain_pageup, None);

    let shift_pagedown = oxideterm_terminal_scroll_action(&Keystroke {
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        key: "pagedown".to_string(),
        ..Default::default()
    });
    assert_eq!(shift_pagedown, Some(TerminalScrollAction::PageDown));
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
    .command_marks(vec![mark], Some("cmd-1".to_string()))
    .layout();

    assert_eq!(layout.command_mark_overlays.len(), 1);
    assert_eq!(layout.command_mark_overlays[0].start_row, 0);
    assert_eq!(layout.command_mark_overlays[0].end_row, 2);
}

#[test]
fn wheel_scroll_delta_uses_oxideterm_direction() {
    assert_eq!(terminal_scroll_delta(3), 3);
    assert_eq!(terminal_scroll_delta(-3), -3);
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
