use super::*;
use std::path::Path;

use gpui::{Bounds, FontFeatures, Keystroke, Modifiers, MouseButton, Pixels, point, px, rgb, size};
use oxideterm_terminal::{
    TermMode, TerminalCell, TerminalColor, TerminalCursorShape, TerminalSearchMatch,
    TerminalSnapshot,
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
        cols: 80,
        rows: 10,
        cursor_col: 0,
        cursor_row: 0,
        cursor_shape: TerminalCursorShape::Block,
        display_offset,
        scrollback_lines,
        lines: Vec::new(),
    }
}

fn cursor_snapshot() -> TerminalSnapshot {
    let mut snapshot = test_snapshot(0, 0);
    snapshot.cols = 2;
    snapshot.rows = 1;
    snapshot.cursor_col = 0;
    snapshot.cursor_row = 0;
    snapshot.lines = vec![oxideterm_terminal::TerminalRow {
        wrapped: false,
        active_input: false,
        cells: vec![
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
        ],
    }];
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
    oxideterm_terminal::TerminalRow {
        cells,
        wrapped: false,
        active_input: false,
    }
}

fn selection_snapshot(text: &str) -> TerminalSnapshot {
    let mut snapshot = test_snapshot(0, 0);
    snapshot.cols = text.chars().count().max(40);
    snapshot.rows = 1;
    snapshot.lines = vec![row_from_text(text, snapshot.cols)];
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
    snapshot
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
    snapshot.lines[0].cells[0].ch = '界';
    snapshot.lines[0].cells[0].wide = true;

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
fn double_click_word_selection_uses_terminal_semantic_word_boundaries() {
    let snapshot = selection_snapshot("cargo test ./crates/oxideterm-native");
    let selection = word_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 15 })
        .expect("word selection");

    assert_eq!(
        selection.normalized(),
        (
            TerminalPoint { row: 0, col: 11 },
            TerminalPoint { row: 0, col: 35 }
        )
    );
}

#[test]
fn word_selection_ignores_separator_cells() {
    let snapshot = selection_snapshot("echo (hello)");

    assert!(word_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 5 }).is_none());
}

#[test]
fn triple_click_line_selection_selects_trimmed_visual_line() {
    let snapshot = selection_snapshot("pwd   ");
    let selection = line_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 1 })
        .expect("line selection");

    assert_eq!(
        selection.normalized(),
        (
            TerminalPoint { row: 0, col: 0 },
            TerminalPoint { row: 0, col: 2 }
        )
    );
}

#[test]
fn triple_click_line_selection_expands_across_wrapped_visual_rows() {
    let mut snapshot = multirow_snapshot(&["hello", "world", "next"]);
    snapshot.cols = 5;
    snapshot.lines[0].wrapped = true;

    let selection = line_selection_at_point(&snapshot, TerminalPoint { row: 1, col: 2 })
        .expect("line selection");

    assert_eq!(
        selection.normalized(),
        (
            TerminalPoint { row: 0, col: 0 },
            TerminalPoint { row: 1, col: 4 }
        )
    );
}

#[test]
fn selected_text_joins_soft_wrapped_rows_without_newline() {
    let mut snapshot = multirow_snapshot(&["hello", "world", "next"]);
    snapshot.cols = 5;
    snapshot.lines[0].wrapped = true;
    let selection = TerminalSelection {
        anchor: TerminalPoint { row: 0, col: 0 },
        head: TerminalPoint { row: 1, col: 4 },
        mode: TerminalSelectionMode::Simple,
    };

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("helloworld")
    );
}

#[test]
fn selected_text_keeps_newline_between_hard_wrapped_rows() {
    let snapshot = multirow_snapshot(&["hello", "world"]);
    let selection = TerminalSelection {
        anchor: TerminalPoint { row: 0, col: 0 },
        head: TerminalPoint { row: 1, col: 4 },
        mode: TerminalSelectionMode::Simple,
    };

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("hello\nworld")
    );
}

#[test]
fn line_selection_copy_appends_terminal_line_newline() {
    let snapshot = selection_snapshot("pwd   ");
    let selection = TerminalSelection {
        anchor: TerminalPoint { row: 0, col: 0 },
        head: TerminalPoint { row: 0, col: 2 },
        mode: TerminalSelectionMode::Lines,
    };

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("pwd\n")
    );
}

#[test]
fn block_selection_copies_rectangular_columns() {
    let snapshot = multirow_snapshot(&["abcdef", "ghijkl", "mnopqr"]);
    let selection = TerminalSelection {
        anchor: TerminalPoint { row: 0, col: 1 },
        head: TerminalPoint { row: 2, col: 3 },
        mode: TerminalSelectionMode::Block,
    };

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("bcd\nhij\nnop")
    );
}

#[test]
fn selected_text_preserves_zero_width_marks() {
    let mut snapshot = selection_snapshot("e");
    snapshot.lines[0].cells[0].zerowidth = "\u{301}".to_string();
    let selection = TerminalSelection {
        anchor: TerminalPoint { row: 0, col: 0 },
        head: TerminalPoint { row: 0, col: 0 },
        mode: TerminalSelectionMode::Lines,
    };

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("e\u{301}\n")
    );
}

#[test]
fn semantic_word_selection_crosses_soft_wrapped_rows() {
    let mut snapshot = multirow_snapshot(&["hello", "world"]);
    snapshot.cols = 5;
    snapshot.lines[0].wrapped = true;

    let selection = word_selection_at_point(&snapshot, TerminalPoint { row: 1, col: 1 })
        .expect("semantic selection");

    assert_eq!(
        selection.normalized(),
        (
            TerminalPoint { row: 0, col: 0 },
            TerminalPoint { row: 1, col: 4 }
        )
    );
}

#[test]
fn link_detection_finds_urls_and_trims_trailing_punctuation() {
    let snapshot = selection_snapshot("open https://example.com/docs).");
    let links = detect_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, TerminalLinkKind::Url);
    assert_eq!(links[0].start_col, 5);
    assert_eq!(links[0].end_col, 29);
    assert_eq!(links[0].target, "https://example.com/docs");
}

#[test]
fn link_detection_finds_path_like_targets() {
    let snapshot = selection_snapshot("see ./crates/oxideterm-native/src/main.rs");
    let links = detect_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, TerminalLinkKind::Path);
    assert_eq!(links[0].target, "./crates/oxideterm-native/src/main.rs");
}

#[test]
fn display_links_skip_path_like_text_on_active_input_row() {
    let mut snapshot = selection_snapshot("cd ../");
    snapshot.lines[0].active_input = true;

    let links = detect_link_ranges(&snapshot);
    let display_links = display_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "../");
    assert!(display_links.is_empty());
}

#[test]
fn display_links_skip_path_like_text_on_wrapped_active_input_rows() {
    let mut snapshot = multirow_snapshot(&["echo ./src/", "main.rs"]);
    snapshot.lines[0].active_input = true;
    snapshot.lines[1].active_input = true;

    let links = detect_link_ranges(&snapshot);
    let display_links = display_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert!(display_links.is_empty());
}

#[test]
fn link_detection_prefers_osc8_hyperlink_ranges() {
    let mut snapshot = selection_snapshot("click");
    for cell in &mut snapshot.lines[0].cells[..5] {
        cell.hyperlink = Some("https://example.com/osc8".to_string());
    }

    let links = detect_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kind, TerminalLinkKind::Url);
    assert_eq!(links[0].start_col, 0);
    assert_eq!(links[0].end_col, 5);
    assert_eq!(links[0].target, "https://example.com/osc8");
}

#[test]
fn link_detection_does_not_duplicate_url_inside_osc8_range() {
    let mut snapshot = selection_snapshot("https://example.com");
    for cell in &mut snapshot.lines[0].cells[..19] {
        cell.hyperlink = Some("https://example.com/osc8".to_string());
    }

    let links = detect_link_ranges(&snapshot);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "https://example.com/osc8");
}

#[test]
fn terminal_element_underlines_osc8_links_even_on_colored_cells() {
    let mut snapshot = selection_snapshot("click");
    for cell in &mut snapshot.lines[0].cells[..5] {
        cell.bg = TerminalColor::rgb(0x61, 0xaf, 0xef);
        cell.hyperlink = Some("https://example.com/osc8".to_string());
    }

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
    .layout();

    let link_run = layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("click"))
        .expect("osc8 link run");
    assert!(link_run.style.underline.is_some());
}

#[test]
fn path_links_resolve_relative_paths_to_file_urls() {
    let url = path_link_to_file_url("./src/main.rs", Path::new("/tmp/Oxide Term")).expect("url");

    assert_eq!(url, "file:///tmp/Oxide%20Term/./src/main.rs");
}

#[test]
fn path_links_percent_encode_non_url_safe_characters() {
    let encoded = percent_encode_path(Path::new("/tmp/a b/中文.rs"));

    assert_eq!(encoded, "/tmp/a%20b/%E4%B8%AD%E6%96%87.rs");
}

#[test]
fn terminal_element_underlines_detected_links() {
    let snapshot = selection_snapshot("open https://example.com");
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
    .layout();
    let link_run = layout
        .text_runs
        .iter()
        .find(|run| run.text.contains("https"))
        .expect("link run");

    assert!(link_run.style.underline.is_some());
}

#[test]
fn terminal_element_does_not_recolor_path_like_prompt_segments() {
    let mut snapshot = selection_snapshot("~/Documents/OxideTerm");
    for cell in &mut snapshot.lines[0].cells[..21] {
        cell.bg = TerminalColor::rgb(0x61, 0xaf, 0xef);
        cell.fg = TerminalColor::rgb(0xff, 0xff, 0xff);
    }

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
    .layout();

    assert!(
        layout
            .text_runs
            .iter()
            .filter(|run| run.text.contains("Documents") || run.text.contains("OxideTerm"))
            .all(|run| run.style.underline.is_none() && run.style.color == rgb(0xffffff).into())
    );
}

#[test]
fn terminal_element_batches_adjacent_cells_with_same_style() {
    let snapshot = selection_snapshot("abc");
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
    .layout();
    let first_run = layout.text_runs.first().expect("text run");

    assert_eq!(first_run.text, "abc");
    assert_eq!(first_run.cells, 3);
}

#[test]
fn terminal_element_keeps_powerline_separators_as_cell_painted_runs() {
    let snapshot =
        selection_snapshot("a\u{e0b0}\u{e0b1}\u{e0b2}\u{e0b3}\u{e0b4}\u{e0b5}\u{e0b6}\u{e0b7}b");
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
    .layout();

    let texts = layout
        .text_runs
        .iter()
        .map(|run| (run.text.as_str(), run.col, run.cells))
        .collect::<Vec<_>>();

    assert_eq!(texts[0], ("a", 0, 1));
    assert_eq!(texts[1], ("\u{e0b0}", 1, 1));
    assert_eq!(texts[2], ("\u{e0b1}", 2, 1));
    assert_eq!(texts[3], ("\u{e0b2}", 3, 1));
    assert_eq!(texts[4], ("\u{e0b3}", 4, 1));
    assert_eq!(texts[5], ("\u{e0b4}", 5, 1));
    assert_eq!(texts[6], ("\u{e0b5}", 6, 1));
    assert_eq!(texts[7], ("\u{e0b6}", 7, 1));
    assert_eq!(texts[8], ("\u{e0b7}", 8, 1));
    assert_eq!(texts[9], ("b", 9, 1));
}

#[test]
fn powerline_separator_points_cover_the_terminal_cell() {
    let bounds = Bounds::new(point(px(8.0), px(10.0)), size(px(8.0), px(16.0)));

    let right = powerline_separator_points('\u{e0b0}', bounds).expect("right triangle");
    assert_eq!(right[0], point(px(7.5), px(9.5)));
    assert_eq!(right[1], point(px(7.5), px(26.5)));
    assert_eq!(right[2], point(px(16.5), px(18.0)));

    let left = powerline_separator_points('\u{e0b2}', bounds).expect("left triangle");
    assert_eq!(left[0], point(px(16.5), px(9.5)));
    assert_eq!(left[1], point(px(16.5), px(26.5)));
    assert_eq!(left[2], point(px(7.5), px(18.0)));

    assert!(powerline_separator_points('\u{e0b4}', bounds).is_none());
}

#[test]
fn terminal_element_prepaint_clips_layout_to_visible_rows() {
    let mut snapshot = multirow_snapshot(&[
        "visible zero",
        "visible one https://example.com",
        "hidden two cargo",
        "hidden three",
    ]);
    snapshot.lines[3].cells[0].bg = TerminalColor::rgb(0xff, 0, 0);
    snapshot.cursor_row = 3;
    snapshot.cursor_col = 0;
    snapshot.lines[3].cells[0].cursor = true;

    let layout = TerminalElement::new(
        snapshot,
        Some(TerminalSelection {
            anchor: TerminalPoint { row: 3, col: 0 },
            head: TerminalPoint { row: 3, col: 5 },
            mode: TerminalSelectionMode::Simple,
        }),
        test_metrics(),
        true,
        Some("x".to_string()),
        Some("cargo".to_string()),
        Vec::new(),
        None,
        None,
        None,
    )
    .layout_for_bounds(visible_layout_bounds(2));

    assert!(layout.text_runs.iter().all(|run| run.row < 2));
    assert!(layout.backgrounds.iter().all(|rect| rect.row < 2));
    assert!(layout.selections.iter().all(|rect| rect.row < 2));
    assert!(layout.search_matches.iter().all(|rect| rect.row < 2));
    assert!(layout.cursor.is_none());
    assert!(layout.marked_text.is_none());
    assert!(layout.ime_cursor_bounds.is_none());
    assert!(
        layout
            .text_runs
            .iter()
            .any(|run| run.text.contains("https"))
    );
    assert!(
        !layout
            .text_runs
            .iter()
            .any(|run| run.text.contains("hidden"))
    );
}

#[test]
fn search_match_rects_find_all_visible_row_matches() {
    let snapshot = selection_snapshot("cargo test cargo");
    let matches = search_match_rects(&snapshot, Some("cargo"));

    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].col, 0);
    assert_eq!(matches[0].cells, 5);
    assert_eq!(matches[1].col, 11);
    assert_eq!(matches[1].cells, 5);
}

#[test]
fn terminal_element_lays_out_search_highlights() {
    let snapshot = selection_snapshot("hello search");
    let layout = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        None,
        Some("search".to_string()),
        Vec::new(),
        None,
        None,
        None,
    )
    .layout();

    assert_eq!(layout.search_matches.len(), 1);
    assert_eq!(layout.search_matches[0].col, 6);
    assert_eq!(layout.search_matches[0].cells, 6);
}

#[test]
fn terminal_element_maps_scrollback_search_matches_into_visible_rows() {
    let mut snapshot = multirow_snapshot(&["history cargo", "visible cargo"]);
    snapshot.display_offset = 3;
    let matches = vec![
        TerminalSearchMatch {
            line: -3,
            start_col: 8,
            end_col: 13,
            ranges: vec![oxideterm_terminal::TerminalSearchRange {
                line: -3,
                start_col: 8,
                end_col: 13,
            }],
        },
        TerminalSearchMatch {
            line: -1,
            start_col: 0,
            end_col: 5,
            ranges: vec![oxideterm_terminal::TerminalSearchRange {
                line: -1,
                start_col: 0,
                end_col: 5,
            }],
        },
    ];

    let layout = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        None,
        Some("cargo".to_string()),
        matches,
        None,
        None,
        None,
    )
    .layout();

    assert_eq!(layout.search_matches.len(), 1);
    assert_eq!(layout.search_matches[0].row, 0);
    assert_eq!(layout.search_matches[0].col, 8);
    assert_eq!(layout.search_matches[0].cells, 5);
}

#[test]
fn copy_policy_defaults_keep_current_terminal_behavior() {
    let settings = TerminalUiSettings::default();
    assert!(!settings.copy_on_select);
    assert!(settings.keep_selection_on_copy);
    assert_eq!(settings.blink_mode, TerminalBlinkMode::On);
}

#[test]
fn terminal_theme_defaults_match_current_terminal_palette() {
    let theme = TerminalUiTheme::default();
    assert_eq!(theme.background, OXIDETERM_TERMINAL_BACKGROUND);
    assert_eq!(theme.foreground, 0xe6e8eb);
}

#[test]
fn terminal_font_uses_real_nerd_font_family_and_fallbacks() {
    let font = terminal_font();
    assert_eq!(font.family.as_ref(), "JetBrainsMono Nerd Font");
    assert_eq!(font.features, FontFeatures::disable_ligatures());
    let fallbacks = font.fallbacks.as_ref().unwrap().fallback_list();
    assert!(fallbacks.contains(&"JetBrainsMono Nerd Font Mono".to_string()));
    assert!(fallbacks.contains(&"JetBrains Mono".to_string()));
    assert!(fallbacks.contains(&"MesloLGS Nerd Font Mono".to_string()));
    assert!(fallbacks.contains(&"Maple Mono NF CN".to_string()));
    assert!(fallbacks.contains(&"Apple Color Emoji".to_string()));
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

#[test]
fn app_cursor_navigation_uses_application_sequences() {
    let normal = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "up".to_string(),
            ..Default::default()
        },
        &TermMode::default(),
        false,
        KittyKeyEventType::Press,
    );
    assert_eq!(normal.as_deref(), Some("\x1b[A"));

    let app_cursor = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "up".to_string(),
            ..Default::default()
        },
        &(TermMode::default() | TermMode::APP_CURSOR),
        false,
        KittyKeyEventType::Press,
    );
    assert_eq!(app_cursor.as_deref(), Some("\x1bOA"));
}

#[test]
fn modified_arrows_include_xterm_modifier_code() {
    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            key: "right".to_string(),
            ..Default::default()
        },
        &TermMode::default(),
        false,
        KittyKeyEventType::Press,
    );

    assert_eq!(sequence.as_deref(), Some("\x1b[1;5C"));
}

#[test]
fn plain_printable_keys_are_left_for_gpui_text_input() {
    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "l".to_string(),
            key_char: Some("l".to_string()),
            ..Default::default()
        },
        &TermMode::default(),
        false,
        KittyKeyEventType::Press,
    );

    assert_eq!(sequence, None);
}

#[test]
fn ctrl_alpha_keys_emit_ascii_control_codes() {
    for byte in b'a'..=b'z' {
        let sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: char::from(byte).to_string(),
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        )
        .expect("ctrl alpha key should produce a control code");

        assert_eq!(sequence.as_bytes(), &[byte & 0x1f]);
    }

    for byte in b'A'..=b'Z' {
        let sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: char::from(byte).to_string(),
                modifiers: Modifiers {
                    shift: true,
                    control: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        )
        .expect("ctrl-shift alpha key should produce a control code");

        assert_eq!(sequence.as_bytes(), &[(byte.to_ascii_lowercase()) & 0x1f]);
    }
}

#[test]
fn ctrl_symbol_keys_emit_terminal_control_codes() {
    let cases = [
        ("@", 0x00),
        ("[", 0x1b),
        ("\\", 0x1c),
        ("]", 0x1d),
        ("^", 0x1e),
        ("_", 0x1f),
        ("?", 0x7f),
    ];

    for (key, expected) in cases {
        let sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: key.to_string(),
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        )
        .expect("ctrl symbol key should produce a control code");

        assert_eq!(sequence.as_bytes(), &[expected]);
    }
}

#[test]
fn function_keys_f1_through_f20_emit_plain_and_modified_sequences() {
    let cases = [
        ("f1", "\x1bOP", "\x1b[1;2P"),
        ("f2", "\x1bOQ", "\x1b[1;2Q"),
        ("f3", "\x1bOR", "\x1b[1;2R"),
        ("f4", "\x1bOS", "\x1b[1;2S"),
        ("f5", "\x1b[15~", "\x1b[15;2~"),
        ("f6", "\x1b[17~", "\x1b[17;2~"),
        ("f7", "\x1b[18~", "\x1b[18;2~"),
        ("f8", "\x1b[19~", "\x1b[19;2~"),
        ("f9", "\x1b[20~", "\x1b[20;2~"),
        ("f10", "\x1b[21~", "\x1b[21;2~"),
        ("f11", "\x1b[23~", "\x1b[23;2~"),
        ("f12", "\x1b[24~", "\x1b[24;2~"),
        ("f13", "\x1b[25~", "\x1b[25;2~"),
        ("f14", "\x1b[26~", "\x1b[26;2~"),
        ("f15", "\x1b[28~", "\x1b[28;2~"),
        ("f16", "\x1b[29~", "\x1b[29;2~"),
        ("f17", "\x1b[31~", "\x1b[31;2~"),
        ("f18", "\x1b[32~", "\x1b[32;2~"),
        ("f19", "\x1b[33~", "\x1b[33;2~"),
        ("f20", "\x1b[34~", "\x1b[34;2~"),
    ];

    for (key, plain, shifted) in cases {
        let plain_sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: key.to_string(),
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        );
        assert_eq!(plain_sequence.as_deref(), Some(plain));

        let shifted_sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: key.to_string(),
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        );
        assert_eq!(shifted_sequence.as_deref(), Some(shifted));
    }
}

#[test]
fn alt_meta_printable_keys_emit_escape_prefixed_ascii_when_enabled() {
    let alt_x = Keystroke {
        key: "x".to_string(),
        key_char: Some("x".to_string()),
        modifiers: Modifiers {
            alt: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let meta_enabled =
        oxideterm_key_escape_sequence(&alt_x, &TermMode::default(), true, KittyKeyEventType::Press);
    assert_eq!(meta_enabled.as_deref(), Some("\x1bx"));

    let meta_disabled = oxideterm_key_escape_sequence(
        &alt_x,
        &TermMode::default(),
        false,
        KittyKeyEventType::Press,
    );
    if cfg!(target_os = "macos") {
        assert_eq!(meta_disabled, None);
    } else {
        assert_eq!(meta_disabled.as_deref(), Some("\x1bx"));
    }

    let alt_shift_x = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "x".to_string(),
            key_char: Some("X".to_string()),
            modifiers: Modifiers {
                alt: true,
                shift: true,
                ..Default::default()
            },
            ..Default::default()
        },
        &TermMode::default(),
        true,
        KittyKeyEventType::Press,
    );
    assert_eq!(alt_shift_x.as_deref(), Some("\x1bX"));
}

#[test]
fn app_cursor_navigation_covers_arrows_home_and_end() {
    let cases = [
        ("up", "\x1b[A", "\x1bOA"),
        ("down", "\x1b[B", "\x1bOB"),
        ("right", "\x1b[C", "\x1bOC"),
        ("left", "\x1b[D", "\x1bOD"),
        ("home", "\x1b[H", "\x1bOH"),
        ("end", "\x1b[F", "\x1bOF"),
    ];

    for (key, normal, app_cursor) in cases {
        let normal_sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: key.to_string(),
                ..Default::default()
            },
            &TermMode::default(),
            false,
            KittyKeyEventType::Press,
        );
        assert_eq!(normal_sequence.as_deref(), Some(normal));

        let app_cursor_sequence = oxideterm_key_escape_sequence(
            &Keystroke {
                key: key.to_string(),
                ..Default::default()
            },
            &(TermMode::default() | TermMode::APP_CURSOR),
            false,
            KittyKeyEventType::Press,
        );
        assert_eq!(app_cursor_sequence.as_deref(), Some(app_cursor));
    }
}

#[test]
fn kitty_keyboard_reports_modified_printable_keys_as_csi_u() {
    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "l".to_string(),
            key_char: Some("l".to_string()),
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            ..Default::default()
        },
        &(TermMode::default() | TermMode::DISAMBIGUATE_ESC_CODES),
        false,
        KittyKeyEventType::Press,
    );

    assert_eq!(sequence.as_deref(), Some("\x1b[108;5u"));
}

#[test]
fn kitty_keyboard_can_report_plain_keys_when_requested() {
    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "enter".to_string(),
            ..Default::default()
        },
        &(TermMode::default() | TermMode::REPORT_ALL_KEYS_AS_ESC),
        false,
        KittyKeyEventType::Press,
    );

    assert_eq!(sequence.as_deref(), Some("\x1b[13;1u"));
}

#[test]
fn kitty_keyboard_reports_repeat_and_release_event_types() {
    let mode =
        TermMode::default() | TermMode::REPORT_ALL_KEYS_AS_ESC | TermMode::REPORT_EVENT_TYPES;
    let key = Keystroke {
        key: "a".to_string(),
        key_char: Some("a".to_string()),
        ..Default::default()
    };

    let repeat = oxideterm_key_escape_sequence(&key, &mode, false, KittyKeyEventType::Repeat);
    let release = oxideterm_key_escape_sequence(&key, &mode, false, KittyKeyEventType::Release);

    assert_eq!(repeat.as_deref(), Some("\x1b[97;1:2u"));
    assert_eq!(release.as_deref(), Some("\x1b[97;1:3u"));
}

#[test]
fn kitty_keyboard_reports_function_keys_with_event_types() {
    let sequence = oxideterm_key_escape_sequence(
        &Keystroke {
            key: "f5".to_string(),
            ..Default::default()
        },
        &(TermMode::default() | TermMode::REPORT_EVENT_TYPES),
        false,
        KittyKeyEventType::Release,
    );

    assert_eq!(sequence.as_deref(), Some("\x1b[15;1:3~"));
}

#[test]
fn bracketed_mouse_reports_use_sgr_coordinates() {
    let report = mouse_button_report(
        TerminalPoint { row: 4, col: 7 },
        MouseButton::Left,
        Modifiers::default(),
        true,
        TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE,
    );

    assert_eq!(report.as_deref(), Some(b"\x1b[<0;8;5M".as_slice()));
}

#[test]
fn middle_and_right_mouse_buttons_use_xterm_button_codes() {
    let mode = TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE;

    let middle = mouse_button_report(
        TerminalPoint { row: 0, col: 0 },
        MouseButton::Middle,
        Modifiers::default(),
        true,
        mode,
    );
    assert_eq!(middle.as_deref(), Some(b"\x1b[<1;1;1M".as_slice()));

    let right = mouse_button_report(
        TerminalPoint { row: 0, col: 0 },
        MouseButton::Right,
        Modifiers::default(),
        true,
        mode,
    );
    assert_eq!(right.as_deref(), Some(b"\x1b[<2;1;1M".as_slice()));
}

#[test]
fn normal_mouse_reports_are_one_based_and_release_as_button_three() {
    let report = mouse_button_report(
        TerminalPoint { row: 0, col: 0 },
        MouseButton::Left,
        Modifiers::default(),
        false,
        TermMode::MOUSE_REPORT_CLICK,
    );

    assert_eq!(report.as_deref(), Some(b"\x1b[M#!!".as_slice()));
}
