use super::*;

#[test]
fn word_selection_covers_shell_tokens_urls_and_soft_wrapped_rows() {
    let mut wrapped = multirow_snapshot(&["hello", "world"]);
    wrapped.cols = 5;
    wrapped.lines[0].wrapped = true;
    wrapped.lines[0].refresh_signature();
    for (snapshot, (row, col), expected) in [
        (
            selection_snapshot("cargo test ./crates/oxideterm-gpui-app"),
            (0, 15),
            Some(((0, 11), (0, 37))),
        ),
        (selection_snapshot("echo (hello)"), (0, 5), None),
        (
            selection_snapshot("first&&second"),
            (0, 1),
            Some(((0, 0), (0, 4))),
        ),
        (
            selection_snapshot("first&&second"),
            (0, 8),
            Some(((0, 7), (0, 12))),
        ),
        (selection_snapshot("first&&second"), (0, 5), None),
        (
            selection_snapshot("open https://example.com/docs)."),
            (0, 13),
            Some(((0, 5), (0, 28))),
        ),
        (
            selection_snapshot("echo $HOME --color=always"),
            (0, 7),
            Some(((0, 5), (0, 9))),
        ),
        (
            selection_snapshot("echo $HOME --color=always"),
            (0, 15),
            Some(((0, 11), (0, 24))),
        ),
        (wrapped, (1, 1), Some(((0, 0), (1, 4)))),
    ] {
        let actual =
            word_selection_at_point(&snapshot, TerminalPoint { row, col }).map(|selection| {
                let (start, end) = selection.normalized();
                ((start.line, start.col), (end.line, end.col))
            });
        assert_eq!(
            actual, expected,
            "row {row}, col {col}: {:?}",
            snapshot.lines
        );
    }
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
fn selected_text_preserves_selection_modes_wrapping_and_scrollback() {
    let mut soft_wrapped = multirow_snapshot(&["hello", "world", "next"]);
    soft_wrapped.cols = 5;
    soft_wrapped.lines[0].wrapped = true;
    soft_wrapped.lines[0].refresh_signature();
    let mut cross_page_wrapped = soft_wrapped.clone();
    cross_page_wrapped.rows = 2;
    cross_page_wrapped.display_offset = 1;
    cross_page_wrapped.scrollback_lines = 1;
    let mut cross_page = multirow_snapshot(&["old-a", "old-b", "now-a", "now-b"]);
    cross_page.rows = 2;
    cross_page.display_offset = 2;
    cross_page.scrollback_lines = 2;
    let mut cross_page_block = multirow_snapshot(&["abcdef", "ghijkl", "mnopqr", "stuvwx"]);
    cross_page_block.rows = 2;
    cross_page_block.display_offset = 2;
    cross_page_block.scrollback_lines = 2;
    let mut combining = selection_snapshot("e");
    combining.lines[0].cells_mut()[0].set_zerowidth("\u{301}".to_string());
    combining.lines[0].refresh_signature();

    for (case, snapshot, mode, (start_line, start_col), (end_line, end_col), expected) in [
        (
            "soft wrap",
            soft_wrapped,
            TerminalSelectionMode::Simple,
            (0, 0),
            (1, 4),
            "helloworld",
        ),
        (
            "hard wrap",
            multirow_snapshot(&["hello", "world"]),
            TerminalSelectionMode::Simple,
            (0, 0),
            (1, 4),
            "hello\nworld",
        ),
        (
            "line ending",
            selection_snapshot("pwd   "),
            TerminalSelectionMode::Lines,
            (0, 0),
            (0, 2),
            "pwd\n",
        ),
        (
            "rectangle",
            multirow_snapshot(&["abcdef", "ghijkl", "mnopqr"]),
            TerminalSelectionMode::Block,
            (0, 1),
            (2, 3),
            "bcd\nhij\nnop",
        ),
        (
            "reversed cross page",
            cross_page,
            TerminalSelectionMode::Simple,
            (1, 4),
            (-2, 0),
            "old-a\nold-b\nnow-a\nnow-b",
        ),
        (
            "cross page rectangle",
            cross_page_block,
            TerminalSelectionMode::Block,
            (-2, 1),
            (1, 3),
            "bcd\nhij\nnop\ntuv",
        ),
        (
            "cross page soft wrap",
            cross_page_wrapped,
            TerminalSelectionMode::Simple,
            (-1, 0),
            (1, 3),
            "helloworld\nnext",
        ),
        (
            "combining mark",
            combining,
            TerminalSelectionMode::Lines,
            (0, 0),
            (0, 0),
            "e\u{301}\n",
        ),
    ] {
        let selection = TerminalSelection {
            anchor: TerminalGridPoint {
                line: start_line,
                col: start_col,
            },
            head: TerminalGridPoint {
                line: end_line,
                col: end_col,
            },
            mode,
        };
        assert_eq!(
            selected_text_for_selection(&snapshot, selection).as_deref(),
            Some(expected),
            "{case}"
        );
    }
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
