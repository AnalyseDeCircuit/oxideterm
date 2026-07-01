use super::*;

#[test]
fn double_click_word_selection_uses_terminal_semantic_word_boundaries() {
    let snapshot = selection_snapshot("cargo test ./crates/oxideterm-gpui-app");
    let selection = word_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 15 })
        .expect("word selection");

    assert_eq!(
        selection.normalized(),
        (
            TerminalGridPoint { line: 0, col: 11 },
            TerminalGridPoint { line: 0, col: 37 }
        )
    );
}

#[test]
fn word_selection_ignores_separator_cells() {
    let snapshot = selection_snapshot("echo (hello)");

    assert!(word_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 5 }).is_none());
}

#[test]
fn word_selection_splits_shell_control_operators() {
    let snapshot = selection_snapshot("first&&second");
    let first = word_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 1 })
        .expect("first token selection");
    let second = word_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 8 })
        .expect("second token selection");

    assert_eq!(
        first.normalized(),
        (
            TerminalGridPoint { line: 0, col: 0 },
            TerminalGridPoint { line: 0, col: 4 }
        )
    );
    assert_eq!(
        second.normalized(),
        (
            TerminalGridPoint { line: 0, col: 7 },
            TerminalGridPoint { line: 0, col: 12 }
        )
    );
    assert!(word_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 5 }).is_none());
}

#[test]
fn word_selection_keeps_url_but_trims_trailing_punctuation() {
    let snapshot = selection_snapshot("open https://example.com/docs).");
    let selection = word_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 13 })
        .expect("url selection");

    assert_eq!(
        selection.normalized(),
        (
            TerminalGridPoint { line: 0, col: 5 },
            TerminalGridPoint { line: 0, col: 28 }
        )
    );
}

#[test]
fn word_selection_keeps_variable_and_flag_values() {
    let snapshot = selection_snapshot("echo $HOME --color=always");
    let variable = word_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 7 })
        .expect("variable selection");
    let flag = word_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 15 })
        .expect("flag selection");

    assert_eq!(
        variable.normalized(),
        (
            TerminalGridPoint { line: 0, col: 5 },
            TerminalGridPoint { line: 0, col: 9 }
        )
    );
    assert_eq!(
        flag.normalized(),
        (
            TerminalGridPoint { line: 0, col: 11 },
            TerminalGridPoint { line: 0, col: 24 }
        )
    );
}

#[test]
fn triple_click_line_selection_selects_trimmed_visual_line() {
    let snapshot = selection_snapshot("pwd   ");
    let selection = line_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 1 })
        .expect("line selection");

    assert_eq!(
        selection.normalized(),
        (
            TerminalGridPoint { line: 0, col: 0 },
            TerminalGridPoint { line: 0, col: 2 }
        )
    );
}

#[test]
fn triple_click_line_selection_expands_across_wrapped_visual_rows() {
    let mut snapshot = multirow_snapshot(&["hello", "world", "next"]);
    snapshot.cols = 5;
    snapshot.lines[0].wrapped = true;
    snapshot.lines[0].refresh_signature();

    let selection = line_selection_at_point(&snapshot, TerminalPoint { row: 1, col: 2 })
        .expect("line selection");

    assert_eq!(
        selection.normalized(),
        (
            TerminalGridPoint { line: 0, col: 0 },
            TerminalGridPoint { line: 1, col: 4 }
        )
    );
}

#[test]
fn selected_text_joins_soft_wrapped_rows_without_newline() {
    let mut snapshot = multirow_snapshot(&["hello", "world", "next"]);
    snapshot.cols = 5;
    snapshot.lines[0].wrapped = true;
    snapshot.lines[0].refresh_signature();
    let selection = TerminalSelection {
        anchor: TerminalGridPoint { line: 0, col: 0 },
        head: TerminalGridPoint { line: 1, col: 4 },
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
        anchor: TerminalGridPoint { line: 0, col: 0 },
        head: TerminalGridPoint { line: 1, col: 4 },
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
        anchor: TerminalGridPoint { line: 0, col: 0 },
        head: TerminalGridPoint { line: 0, col: 2 },
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
        anchor: TerminalGridPoint { line: 0, col: 1 },
        head: TerminalGridPoint { line: 2, col: 3 },
        mode: TerminalSelectionMode::Block,
    };

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("bcd\nhij\nnop")
    );
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

#[test]
fn selected_text_preserves_zero_width_marks() {
    let mut snapshot = selection_snapshot("e");
    snapshot.lines[0].cells_mut()[0].zerowidth = "\u{301}".to_string();
    snapshot.lines[0].refresh_signature();
    let selection = TerminalSelection {
        anchor: TerminalGridPoint { line: 0, col: 0 },
        head: TerminalGridPoint { line: 0, col: 0 },
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
    snapshot.lines[0].refresh_signature();

    let selection = word_selection_at_point(&snapshot, TerminalPoint { row: 1, col: 1 })
        .expect("semantic selection");

    assert_eq!(
        selection.normalized(),
        (
            TerminalGridPoint { line: 0, col: 0 },
            TerminalGridPoint { line: 1, col: 4 }
        )
    );
}
