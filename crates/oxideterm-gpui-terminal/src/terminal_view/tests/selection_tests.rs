use super::*;

#[test]
fn word_selection_covers_shell_tokens_and_separators() {
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

    let snapshot = selection_snapshot("echo (hello)");

    assert!(word_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 5 }).is_none());

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
fn free_type_matching_pair_handles_nesting_escaping_width_and_wrapping() {
    let snapshot = selection_snapshot("echo outer(inner[chosen])");
    let selection = matching_pair_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 19 })
        .expect("matching pair selection");

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("chosen")
    );

    let snapshot = selection_snapshot(r#"echo ("ignored )" real\) value)"#);
    let selection = matching_pair_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 25 })
        .expect("outer matching pair selection");

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some(r#""ignored )" real\) value"#)
    );

    let snapshot = wide_snapshot("(你好)");
    let selection = matching_pair_selection_at_point(&snapshot, TerminalPoint { row: 0, col: 3 })
        .expect("wide matching pair selection");

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("你好")
    );

    let mut snapshot = multirow_snapshot(&["(abc", "def)"]);
    snapshot.cols = 4;
    snapshot.lines[0].wrapped = true;
    snapshot.lines[0].refresh_signature();
    let selection = matching_pair_selection_at_point(&snapshot, TerminalPoint { row: 1, col: 1 })
        .expect("wrapped matching pair selection");

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("abcdef")
    );
}

#[test]
fn line_selection_handles_trimmed_and_wrapped_lines() {
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
fn selected_text_distinguishes_soft_and_hard_wrapped_rows() {
    let selection = TerminalSelection {
        anchor: TerminalGridPoint { line: 0, col: 0 },
        head: TerminalGridPoint { line: 1, col: 4 },
        mode: TerminalSelectionMode::Simple,
    };
    let mut soft_wrapped = multirow_snapshot(&["hello", "world", "next"]);
    soft_wrapped.cols = 5;
    soft_wrapped.lines[0].wrapped = true;
    soft_wrapped.lines[0].refresh_signature();

    assert_eq!(
        selected_text_for_selection(&soft_wrapped, selection).as_deref(),
        Some("helloworld")
    );

    assert_eq!(
        selected_text_for_selection(&multirow_snapshot(&["hello", "world"]), selection).as_deref(),
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
fn selection_snapshot_requests_only_ranges_outside_the_viewport() {
    let mut snapshot = multirow_snapshot(&["visible-a", "visible-b"]);
    snapshot.scrollback_lines = 4;
    let selection = TerminalSelection {
        anchor: TerminalGridPoint { line: 1, col: 3 },
        head: TerminalGridPoint { line: -3, col: 1 },
        mode: TerminalSelectionMode::Simple,
    };

    assert_eq!(
        snapshot_request_for_selection(&snapshot, selection),
        Some(TerminalSelectionSnapshotRequest {
            display_offset: 3,
            rows: 5,
        })
    );

    let snapshot = multirow_snapshot(&["visible-a", "visible-b"]);
    let selection = TerminalSelection {
        anchor: TerminalGridPoint { line: 0, col: 0 },
        head: TerminalGridPoint { line: 1, col: 3 },
        mode: TerminalSelectionMode::Simple,
    };

    assert_eq!(snapshot_request_for_selection(&snapshot, selection), None);
}

#[test]
fn reverse_cross_page_selection_reads_rows_beyond_viewport_height() {
    let mut snapshot = multirow_snapshot(&["old-a", "old-b", "now-a", "now-b"]);
    snapshot.rows = 2;
    snapshot.display_offset = 2;
    snapshot.scrollback_lines = 2;
    let selection = TerminalSelection {
        anchor: TerminalGridPoint { line: 1, col: 4 },
        head: TerminalGridPoint { line: -2, col: 0 },
        mode: TerminalSelectionMode::Simple,
    };

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("old-a\nold-b\nnow-a\nnow-b")
    );
}

#[test]
fn cross_page_block_selection_preserves_rectangular_columns() {
    let mut snapshot = multirow_snapshot(&["abcdef", "ghijkl", "mnopqr", "stuvwx"]);
    snapshot.rows = 2;
    snapshot.display_offset = 2;
    snapshot.scrollback_lines = 2;
    let selection = TerminalSelection {
        anchor: TerminalGridPoint { line: -2, col: 1 },
        head: TerminalGridPoint { line: 1, col: 3 },
        mode: TerminalSelectionMode::Block,
    };

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("bcd\nhij\nnop\ntuv")
    );
}

#[test]
fn cross_page_selection_preserves_soft_wrapped_lines() {
    let mut snapshot = multirow_snapshot(&["hello", "world", "next"]);
    snapshot.rows = 2;
    snapshot.display_offset = 1;
    snapshot.scrollback_lines = 1;
    snapshot.cols = 5;
    snapshot.lines[0].wrapped = true;
    snapshot.lines[0].refresh_signature();
    let selection = TerminalSelection {
        anchor: TerminalGridPoint { line: -1, col: 0 },
        head: TerminalGridPoint { line: 1, col: 3 },
        mode: TerminalSelectionMode::Simple,
    };

    assert_eq!(
        selected_text_for_selection(&snapshot, selection).as_deref(),
        Some("helloworld\nnext")
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
    snapshot.lines[0].cells_mut()[0].set_zerowidth("\u{301}".to_string());
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
