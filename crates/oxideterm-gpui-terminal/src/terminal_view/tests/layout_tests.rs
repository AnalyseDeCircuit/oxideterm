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
fn terminal_element_lays_out_autosuggest_ghost_text_at_cursor() {
    let mut snapshot = selection_snapshot("git");
    snapshot.cursor_row = 0;
    snapshot.cursor_col = 3;
    snapshot.lines[0].cells_mut()[3].cursor = true;
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
    .ghost_text(Some(" status".to_string()))
    .layout();

    let ghost_text = layout.ghost_text.expect("ghost text");
    assert_eq!(ghost_text.text, " status");
    assert_eq!(ghost_text.row, 0);
    assert_eq!(ghost_text.col, 3);
    assert_eq!(ghost_text.cells, 7);
    assert!(
        !layout
            .text_runs
            .iter()
            .any(|run| run.text.contains(" status"))
    );
}

#[test]
fn terminal_element_truncates_autosuggest_ghost_text_to_visible_columns() {
    let mut snapshot = selection_snapshot("git");
    snapshot.cols = 5;
    snapshot.lines[0].cells_mut().truncate(5);
    snapshot.cursor_row = 0;
    snapshot.cursor_col = 3;
    snapshot.lines[0].cells_mut()[3].cursor = true;
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
    .ghost_text(Some(" status".to_string()))
    .layout();

    let ghost_text = layout.ghost_text.expect("ghost text");
    assert_eq!(ghost_text.text, " s");
    assert_eq!(ghost_text.cells, 2);
}

#[test]
fn terminal_element_counts_wide_ghost_text_cells() {
    let mut snapshot = selection_snapshot("Password:");
    snapshot.cursor_row = 0;
    snapshot.cursor_col = 9;
    snapshot.lines[0].cells_mut()[9].cursor = true;
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
    .ghost_text(Some("按Enter 填充保存的密码".to_string()))
    .layout();

    let ghost_text = layout.ghost_text.expect("ghost text");
    assert_eq!(ghost_text.col, 9);
    assert_eq!(ghost_text.cells, 22);
}

#[test]
fn terminal_element_segments_mixed_width_ghost_text_for_grid_painting() {
    let segments = ghost_text_grid_segments("按Enter 填充已保存的提权密码");

    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0].text, "按");
    assert_eq!(segments[0].col_offset, 0);
    assert_eq!(segments[0].cell_stride, 2);
    assert_eq!(segments[1].text, "Enter ");
    assert_eq!(segments[1].col_offset, 2);
    assert_eq!(segments[1].cell_stride, 1);
    assert_eq!(segments[2].text, "填充已保存的提权密码");
    assert_eq!(segments[2].col_offset, 8);
    assert_eq!(segments[2].cell_stride, 2);
    assert_eq!(
        segments.iter().map(|segment| segment.cells).sum::<usize>(),
        28
    );
}

#[test]
fn terminal_element_truncates_wide_ghost_text_by_cells() {
    let mut snapshot = selection_snapshot("Password:");
    snapshot.cols = 12;
    snapshot.lines[0].cells_mut().truncate(12);
    snapshot.cursor_row = 0;
    snapshot.cursor_col = 9;
    snapshot.lines[0].cells_mut()[9].cursor = true;
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
    .ghost_text(Some("按Enter".to_string()))
    .layout();

    let ghost_text = layout.ghost_text.expect("ghost text");
    assert_eq!(ghost_text.text, "按E");
    assert_eq!(ghost_text.cells, 3);
}

#[test]
fn terminal_element_hides_autosuggest_ghost_text_during_ime_composition() {
    let mut snapshot = selection_snapshot("git");
    snapshot.cursor_row = 0;
    snapshot.cursor_col = 3;
    snapshot.lines[0].cells_mut()[3].cursor = true;
    snapshot.lines[0].refresh_signature();

    let layout = TerminalElement::new(
        snapshot,
        None,
        test_metrics(),
        true,
        Some("あ".to_string()),
        None,
        Vec::new(),
        None,
        None,
        None,
    )
    .ghost_text(Some(" status".to_string()))
    .layout();

    assert!(layout.marked_text.is_some());
    assert!(layout.ghost_text.is_none());
}

#[test]
fn terminal_element_shapes_zero_width_marks_with_base_cell() {
    let mut snapshot = selection_snapshot("e");
    snapshot.lines[0].cells_mut()[0].zerowidth = "\u{301}".to_string();
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

    assert_eq!(first_run.text, "e\u{301}");
    assert_eq!(first_run.cells, 1);
    assert_eq!(first_run.style.len, "e\u{301}".len());
}

#[test]
fn terminal_element_keeps_emoji_zwj_cluster_in_one_wide_cell() {
    let mut snapshot = selection_snapshot(" ");
    snapshot.lines[0].cells_mut()[0].ch = '👨';
    snapshot.lines[0].cells_mut()[0].zerowidth = "\u{200d}👩\u{200d}👧\u{200d}👦".to_string();
    snapshot.lines[0].cells_mut()[0].wide = true;
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

    assert_eq!(first_run.text, "👨‍👩‍👧‍👦");
    assert_eq!(first_run.cells, 2);
    assert_eq!(first_run.style.len, "👨‍👩‍👧‍👦".len());
}

#[test]
fn terminal_element_shapes_rtl_row_as_visual_runs() {
    let snapshot = selection_snapshot("السلام عليكم");
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

    assert!(layout.text_runs.len() < "السلام عليكم".chars().count());
    assert_eq!(
        layout.text_runs.iter().map(|run| run.cells).sum::<usize>(),
        "السلام عليكم".chars().filter(|ch| *ch != ' ').count()
    );
    assert!(layout.text_runs.iter().any(|run| run.text.contains("س")));
}

#[test]
fn terminal_element_keeps_rtl_text_at_content_start_with_trailing_blanks() {
    let snapshot = selection_snapshot("שלום");
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
            .map(|run| run.col)
            .min()
            .expect("text run"),
        0
    );
    assert!(layout.text_runs.iter().all(|run| run.col < 4));
}

#[test]
fn terminal_element_maps_rtl_cursor_to_visual_column() {
    let mut snapshot = selection_snapshot("abc שלום def");
    snapshot.cursor_row = 0;
    snapshot.cursor_col = 4;
    snapshot.lines[0].cells_mut()[4].cursor = true;
    snapshot.lines[0].refresh_signature();
    let expected_col = oxideterm_terminal_unicode::visual_line_for_row(&snapshot.lines[0])
        .visual_col_for_logical_col(4);
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

    assert_ne!(expected_col, 4);
    assert_eq!(layout.cursor.expect("cursor").col, expected_col);
}

#[test]
fn terminal_element_maps_rtl_search_highlight_to_visual_rect() {
    let snapshot = selection_snapshot("abc שלום def");
    let expected_rect = oxideterm_terminal_unicode::visual_line_for_row(&snapshot.lines[0])
        .visual_rects_for_logical_range(4..8)
        .next()
        .expect("visual rect");
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

    assert_eq!(layout.search_matches.len(), 1);
    assert_eq!(layout.search_matches[0].col, expected_rect.start);
    assert_eq!(layout.search_matches[0].cells, 4);
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
fn terminal_element_keeps_highlights_across_output_rescans() {
    let rules = vec![TerminalHighlightRule {
        id: "error".to_string(),
        pattern: "ERROR".to_string(),
        is_regex: false,
        case_sensitive: true,
        foreground: None,
        background: Some("#ff0000".to_string()),
        render_mode: TerminalHighlightRenderMode::Background,
        enabled: true,
        priority: 0,
    }];

    let before_output = selection_snapshot("ERROR first pass");
    let before_layout = TerminalElement::new(
        before_output,
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
    .highlight_rules(rules.clone())
    .layout();

    let mut after_output = multirow_snapshot(&["ERROR first pass", "command output"]);
    after_output.rows = 2;
    let after_layout = TerminalElement::new(
        after_output,
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
    .highlight_rules(rules)
    .layout();

    assert_eq!(before_layout.highlight_backgrounds.len(), 1);
    assert_eq!(after_layout.highlight_backgrounds.len(), 1);
    assert_eq!(after_layout.highlight_backgrounds[0].row, 0);
    assert_eq!(after_layout.highlight_backgrounds[0].col, 0);
    assert_eq!(after_layout.highlight_backgrounds[0].cells, 5);
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
