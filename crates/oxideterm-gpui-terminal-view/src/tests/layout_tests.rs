use crate::metrics::TERMINAL_CONTENT_PADDING;
use gpui::{Bounds, Pixels, point, px, rgb, size};
use std::sync::Arc;
type TerminalElement = crate::TerminalElement<()>;
use super::snapshot::*;
use super::{TerminalCell, TerminalColor, TerminalCursorShape, TerminalSnapshot, test_metrics};
use crate::{TransientCommandHighlight, element::*, selection::*, theme::*};
use oxideterm_terminal_emulator::TerminalSearchMatch;

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
fn terminal_ghost_text_respects_cursor_width_and_ime_composition() {
    for (text, cols, cursor_col, marked, suggestion, expected) in [
        ("git", 40, 3, None, " status", Some((" status", 0, 3, 7))),
        ("Password:", 12, 9, None, "按Enter", Some(("按E", 0, 9, 3))),
        ("git", 40, 3, Some("あ"), " status", None),
        ("git", 6, 3, None, "👨‍👩‍👧‍👦a", Some(("👨‍👩‍👧‍👦a", 0, 3, 3))),
        ("git", 5, 3, None, "👨‍👩‍👧‍👦a", Some(("👨‍👩‍👧‍👦", 0, 3, 2))),
        ("git", 4, 3, None, "👨‍👩‍👧‍👦a", None),
    ] {
        let mut snapshot = selection_snapshot(text);
        snapshot.cols = cols;
        snapshot.lines[0].cells_mut().truncate(cols);
        snapshot.cursor_row = 0;
        snapshot.cursor_col = cursor_col;
        snapshot.lines[0].cells_mut()[cursor_col].cursor = true;
        snapshot.lines[0].refresh_signature();
        let layout = TerminalElement::new(
            snapshot,
            None,
            test_metrics(),
            true,
            marked.map(str::to_owned),
            None,
            Vec::new(),
            None,
            None,
            None,
        )
        .ghost_text(Some(suggestion.to_owned()))
        .layout();
        assert_eq!(
            layout
                .ghost_text
                .as_ref()
                .map(|run| (run.text.as_str(), run.row, run.col, run.cells)),
            expected,
            "text={text}, cols={cols}, marked={marked:?}",
        );
        assert_eq!(
            layout.marked_text.as_ref().map(|run| run.text.as_str()),
            marked
        );
        assert!(
            !layout
                .text_runs
                .iter()
                .any(|run| run.text.contains(suggestion))
        );
    }
}

#[test]
fn terminal_element_segments_mixed_width_ghost_text_for_grid_painting() {
    for (text, expected) in [
        (
            "按Enter 填充已保存的提权密码",
            vec![
                ("按", 0, 2, 2),
                ("Enter ", 2, 1, 6),
                ("填充已保存的提权密码", 8, 2, 20),
            ],
        ),
        ("🦀a", vec![("🦀", 0, 2, 2), ("a", 2, 1, 1)]),
        ("👨‍👩‍👧‍👦a", vec![("👨‍👩‍👧‍👦", 0, 2, 2), ("a", 2, 1, 1)]),
        ("👩🏽‍💻a", vec![("👩🏽‍💻", 0, 2, 2), ("a", 2, 1, 1)]),
        ("🇨🇳a", vec![("🇨🇳", 0, 2, 2), ("a", 2, 1, 1)]),
        ("e\u{301}a", vec![("e\u{301}a", 0, 1, 2)]),
    ] {
        let segments = ghost_text_grid_segments(text);
        assert_eq!(
            segments
                .iter()
                .map(|segment| (
                    segment.text.as_str(),
                    segment.col_offset,
                    segment.cell_stride,
                    segment.cells
                ))
                .collect::<Vec<_>>(),
            expected,
            "{text}"
        );
    }
}

#[test]
fn terminal_element_moves_cursor_to_ime_caret_during_composition() {
    let mut snapshot = selection_snapshot("git");
    snapshot.cursor_row = 0;
    snapshot.cursor_col = 3;
    snapshot.cursor_shape = TerminalCursorShape::Block;
    snapshot.lines[0].cells_mut()[3].cursor = true;
    snapshot.lines[0].refresh_signature();
    let element = |marked_text: Option<&str>, caret_utf16: Option<usize>| {
        TerminalElement::new(
            snapshot.clone(),
            None,
            test_metrics(),
            true,
            marked_text.map(str::to_string),
            None,
            Vec::new(),
            None,
            None,
            None,
        )
        .marked_text_caret(caret_utf16)
        .layout()
    };
    let block_cursor_col = |layout: &TerminalElementLayout| {
        layout
            .backgrounds
            .iter()
            .any(|rect| rect.row == 0 && rect.col == 3)
    };
    assert!(block_cursor_col(&element(None, None)));

    for (text, caret_utf16, cells, expected_col) in [
        ("你hao", Some(0), 5, 3),
        ("你hao", Some(1), 5, 5),
        ("你hao", Some(4), 5, 8),
        ("你hao", None, 5, 8),
        ("🦀a", Some(1), 3, 5),
        ("🦀a", Some(2), 3, 5),
        ("👨‍👩‍👧‍👦a", Some(0), 3, 3),
        ("👨‍👩‍👧‍👦a", Some(2), 3, 5),
        ("👨‍👩‍👧‍👦a", Some(11), 3, 5),
        ("👨‍👩‍👧‍👦a", None, 3, 6),
        ("👩🏽‍💻a", Some(7), 3, 5),
        ("👩🏽‍💻a", Some(8), 3, 6),
        ("🇨🇳a", Some(4), 3, 5),
        ("e\u{301}a", Some(1), 2, 4),
    ] {
        let layout = element(Some(text), caret_utf16);
        let marked_text = layout.marked_text.as_ref().expect("marked text");
        assert_eq!((marked_text.col, marked_text.cells), (3, cells), "{text}");
        let cursor = layout.cursor.expect("composition caret");
        assert_eq!(
            (cursor.row, cursor.col, cursor.shape),
            (0, expected_col, TerminalCursorShape::Bar),
            "text={text}, caret_utf16={caret_utf16:?}"
        );
        assert!(
            !block_cursor_col(&layout),
            "the grid block cursor must not stay at the composition start"
        );
    }
}

#[test]
fn terminal_element_shapes_combining_marks_and_wide_grapheme_clusters() {
    for (base, marks, wide, expected, cells) in [
        ('e', "\u{301}", false, "e\u{301}", 1),
        ('👨', "\u{200d}👩\u{200d}👧\u{200d}👦", true, "👨‍👩‍👧‍👦", 2),
    ] {
        let mut snapshot = selection_snapshot(" ");
        let cell = &mut snapshot.lines[0].cells_mut()[0];
        cell.ch = base;
        cell.set_zerowidth(marks.to_string());
        cell.wide = wide;
        snapshot.lines[0].refresh_signature();
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
        assert_eq!(
            (
                first_run.text.as_str(),
                first_run.cells,
                first_run.style.len
            ),
            (expected, cells, expected.len()),
            "{expected}"
        );
    }
}

#[test]
fn bidi_text_runs_preserve_content_start_and_wide_cell_geometry() {
    let mut mixed = selection_snapshot("");
    mixed.lines = vec![row_from_text_with_wide_spacers("中文a\u{05fc}")];
    for (case, snapshot, expected) in [
        (
            "Arabic",
            selection_snapshot("السلام عليكم"),
            vec![(0, "مكيلع", 5), (6, "مالسلا", 6)],
        ),
        (
            "wide mixed row",
            mixed,
            vec![(0, "中", 2), (2, "文", 2), (4, "a\u{05fc}", 2)],
        ),
        (
            "Hebrew with trailing blanks",
            selection_snapshot("שלום"),
            vec![(0, "םולש", 4)],
        ),
    ] {
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
        assert_eq!(
            layout
                .text_runs
                .iter()
                .map(|run| (run.col, run.text.as_ref(), run.cells,))
                .collect::<Vec<_>>(),
            expected,
            "{case}"
        );
    }
}

#[test]
fn terminal_element_maps_rtl_cursor_and_search_to_visual_columns() {
    let mut snapshot = selection_snapshot("abc שלום def");
    snapshot.cursor_row = 0;
    snapshot.cursor_col = 4;
    snapshot.lines[0].cells_mut()[4].cursor = true;
    snapshot.lines[0].refresh_signature();
    let layout = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        None,
        Some("שלום".to_string()),
        Vec::new(),
        None,
        None,
        None,
    )
    .layout();

    assert_eq!(layout.cursor.expect("cursor").col, 7);
    assert_eq!(
        layout
            .search_matches
            .iter()
            .map(|rect| (rect.row, rect.col, rect.cells))
            .collect::<Vec<_>>(),
        [(0, 4, 4)]
    );
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
fn transparent_powerline_default_background_is_not_painted_as_a_solid_glyph() {
    let mut snapshot = selection_snapshot("\u{e0b0}\u{e0b2}\u{e0b4}\u{e0b6}");
    snapshot.cols = 4;
    snapshot.lines[0].cells_mut().truncate(4);
    for cell in snapshot.lines[0].cells_mut().iter_mut().take(4) {
        cell.fg = terminal_color_from_hex(TerminalUiTheme::default().background);
        cell.bg = TerminalColor::rgb(0, 128, 128);
        cell.style_origin = oxideterm_terminal_emulator::TerminalStyleOrigin::new(true, true)
            .with_foreground_default_background();
    }
    snapshot.lines[0].refresh_signature();
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
    .transparent_background(true)
    .layout();

    assert!(
        layout.text_runs.is_empty(),
        "transparent separators must leave the image visible"
    );
    assert_eq!(
        layout
            .backgrounds
            .iter()
            .map(|rect| (rect.col, rect.cells, rect.powerline_cutout, rect.color))
            .collect::<Vec<_>>(),
        ['\u{e0b0}', '\u{e0b2}', '\u{e0b4}', '\u{e0b6}']
            .into_iter()
            .enumerate()
            .map(|(col, ch)| (col, 1, Some(ch), gpui::Hsla::from(rgb(0x008080))))
            .collect::<Vec<_>>(),
        "each separator needs its own shaped background rather than one solid rectangle"
    );
}

#[test]
fn terminal_element_prepaint_clips_layout_to_visible_rows() {
    let mut snapshot = multirow_snapshot(&[
        "visible zero",
        "visible one https://example.com",
        "hidden two cargo",
        "hidden three",
    ]);
    snapshot.lines[3].cells_mut()[0].bg = TerminalColor::rgb(0xff, 0, 0);
    snapshot.cursor_row = 3;
    snapshot.cursor_col = 0;
    snapshot.lines[3].cells_mut()[0].cursor = true;
    snapshot.lines[3].refresh_signature();

    let layout = TerminalElement::new(
        snapshot,
        Some(TerminalSelection {
            anchor: TerminalGridPoint { line: 3, col: 0 },
            head: TerminalGridPoint { line: 3, col: 5 },
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
    assert!(layout.ghost_text.is_none());
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
fn transient_command_highlight_stays_inside_latest_command_output() {
    let snapshot = multirow_snapshot(&[
        "$ grep dbx",
        "dbx first result",
        "$ printf dbx",
        "dbx later output",
    ]);
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
    .transient_command_highlight(Some(TransientCommandHighlight {
        command_id: Arc::from("cmd-1"),
        query: Arc::from("dbx"),
        case_sensitive: true,
        output_start_global_line: 1,
        output_end_global_line: Some(1),
    }))
    .layout();

    assert_eq!(layout.highlight_backgrounds.len(), 1);
    assert_eq!(layout.highlight_backgrounds[0].row, 1);
    assert_eq!(layout.highlight_backgrounds[0].col, 0);
    assert_eq!(layout.highlight_backgrounds[0].cells, 3);
}

#[test]
fn terminal_highlight_can_preserve_existing_background() {
    let layout = TerminalElement::new(
        selection_snapshot("ERROR output"),
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
    .highlight_rules(vec![TerminalHighlightRule {
        id: "error".to_string(),
        pattern: "ERROR".to_string(),
        foreground: Some("#ff0000".to_string()),
        render_mode: TerminalHighlightRenderMode::Background,
        match_scope: TerminalHighlightMatchScope::Match,
        preserve_background: true,
        enabled: true,
        ..TerminalHighlightRule::default()
    }])
    .layout();

    assert!(layout.highlight_backgrounds.is_empty());
    assert!(
        layout
            .text_runs
            .iter()
            .any(|run| run.text == "ERROR" && run.style.color == rgb(0xff0000).into())
    );
}

#[test]
fn terminal_highlight_scopes_preserve_match_and_logical_line_geometry() {
    let mut snapshot = multirow_snapshot(&["prefix ER", "ROR suffix", "next line"]);
    snapshot.cols = 9;
    for row in &mut snapshot.lines {
        row.cells_mut().truncate(snapshot.cols);
        row.refresh_signature();
    }
    snapshot.lines[0].wrapped = true;
    snapshot.lines[0].refresh_signature();
    for (scope, expected) in [
        (TerminalHighlightMatchScope::Match, [(0, 7, 2), (1, 0, 3)]),
        (
            TerminalHighlightMatchScope::LogicalLine,
            [(0, 0, 9), (1, 0, 9)],
        ),
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
        .highlight_rules(vec![TerminalHighlightRule {
            id: "error".to_string(),
            pattern: "ERROR".to_string(),
            background: Some("#ff0000".to_string()),
            render_mode: TerminalHighlightRenderMode::Background,
            match_scope: scope,
            preserve_background: false,
            enabled: true,
            ..TerminalHighlightRule::default()
        }])
        .layout();
        assert_eq!(
            layout
                .highlight_backgrounds
                .iter()
                .map(|rect| (rect.row, rect.col, rect.cells))
                .collect::<Vec<_>>(),
            expected,
            "{scope:?}"
        );
    }
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
            ranges: vec![oxideterm_terminal_emulator::TerminalSearchRange {
                line: -3,
                start_col: 8,
                end_col: 13,
            }],
        },
        TerminalSearchMatch {
            line: -1,
            start_col: 0,
            end_col: 5,
            ranges: vec![oxideterm_terminal_emulator::TerminalSearchRange {
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
fn selection_matches_use_literal_case_sensitive_text_without_changing_search() {
    let snapshot = selection_snapshot("share SHARE shares [x] [x] SHARE");
    let element = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        None,
        Some("SHARE".into()),
        Vec::new(),
        None,
        None,
        None,
    );
    assert!(element.layout().selection_matches.is_empty());
    let layout = element
        .selection_highlight_query(Some(Arc::new(zeroize::Zeroizing::new("share".into()))))
        .layout();
    assert_eq!(
        layout
            .selection_matches
            .iter()
            .map(|rect| (rect.row, rect.col, rect.cells))
            .collect::<Vec<_>>(),
        vec![(0, 0, 5), (0, 12, 5)]
    );
    assert_eq!(
        layout
            .search_matches
            .iter()
            .map(|rect| (rect.row, rect.col, rect.cells))
            .collect::<Vec<_>>(),
        vec![(0, 6, 5), (0, 27, 5)]
    );
    let literal = TerminalElement::new(
        selection_snapshot("[x] x [x]"),
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
    .selection_highlight_query(Some(Arc::new(zeroize::Zeroizing::new("[x]".into()))))
    .layout();
    assert_eq!(
        literal
            .selection_matches
            .iter()
            .map(|rect| (rect.col, rect.cells))
            .collect::<Vec<_>>(),
        vec![(0, 3), (6, 3)]
    );
}

#[test]
fn selection_matches_preserve_wide_cells_and_soft_wraps() {
    let mut snapshot = selection_snapshot("你好 你好");
    snapshot.lines[0] = row_from_text_with_wide_spacers("你好 你好");
    snapshot.cols = snapshot.lines[0].cells.len();
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
    .selection_highlight_query(Some(Arc::new(zeroize::Zeroizing::new("你好".into()))))
    .layout();
    assert_eq!(
        layout
            .selection_matches
            .iter()
            .map(|rect| (rect.col, rect.cells))
            .collect::<Vec<_>>(),
        vec![(0, 4), (5, 4)]
    );

    let mut snapshot = multirow_snapshot(&["sha", "res", "har", "e!!"]);
    snapshot.cols = 3;
    for row in &mut snapshot.lines[..3] {
        row.wrapped = true;
        row.refresh_signature();
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
    .selection_highlight_query(Some(Arc::new(zeroize::Zeroizing::new("share".into()))))
    .viewport_rows(2)
    .layout();
    assert_eq!(
        layout
            .selection_matches
            .iter()
            .map(|rect| (rect.row, rect.col, rect.cells))
            .collect::<Vec<_>>(),
        vec![(0, 0, 3), (1, 0, 2), (1, 2, 1)]
    );
}

fn cursor_snapshot() -> TerminalSnapshot {
    let mut snapshot = test_snapshot(0, 0);
    snapshot.cols = 2;
    snapshot.rows = 1;
    snapshot.cursor_col = 0;
    snapshot.cursor_row = 0;
    snapshot.lines = vec![oxideterm_terminal_emulator::TerminalRow {
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
fn selection_rects_track_grid_lines_when_scrollback_offset_changes() {
    let mut snapshot = multirow_snapshot(&["row0", "row1", "row2", "row3"]);
    snapshot.display_offset = 2;
    snapshot.scrollback_lines = 4;
    let layout = TerminalElement::new(
        snapshot,
        Some(TerminalSelection {
            anchor: TerminalGridPoint { line: 1, col: 0 },
            head: TerminalGridPoint { line: 1, col: 3 },
            mode: TerminalSelectionMode::Simple,
        }),
        test_metrics(),
        true,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .layout_for_bounds(visible_layout_bounds(4));

    assert_eq!(layout.selections.len(), 1);
    assert_eq!(layout.selections[0].row, 3);
}

pub(crate) fn visible_layout_bounds(rows: usize) -> Bounds<Pixels> {
    Bounds::new(
        point(px(0.0), px(0.0)),
        size(
            px(400.0),
            px(TERMINAL_CONTENT_PADDING * 2.0 + rows as f32 * test_metrics().line_height_f32()),
        ),
    )
}
