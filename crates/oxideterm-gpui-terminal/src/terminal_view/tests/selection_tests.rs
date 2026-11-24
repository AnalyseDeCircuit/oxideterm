use super::*;

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
