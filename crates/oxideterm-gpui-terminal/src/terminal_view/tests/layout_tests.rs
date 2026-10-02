use super::*;

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
            .any(|run| run.text == "ERROR" && run.style.color == rgb(0xff0000).into_color())
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
    snapshot.lines[1].wrapped = true;
    snapshot.lines[1].refresh_signature();
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
