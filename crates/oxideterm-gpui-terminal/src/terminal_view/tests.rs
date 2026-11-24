use super::*;

use gpui::rgb;
use oxideterm_gpui_terminal_view::metrics;
#[path = "../../../oxideterm-gpui-terminal-view/tests/support/metrics.rs"]
mod geometry;
use geometry::*;
use oxideterm_terminal::{
    TerminalCell, TerminalColor, TerminalCommandMark, TerminalCommandMarkClosedBy,
    TerminalCommandMarkConfidence, TerminalCommandMarkDetectionSource, TerminalCursorShape,
    TerminalRow, TerminalSnapshot,
};

#[cfg(windows)]
use crate::terminal_ui::TerminalUiPreferences;
use oxideterm_gpui_terminal_view::semantic::semantic_line_role_for_rows;
use oxideterm_terminal_semantic::SemanticLineRole;

#[path = "../../../oxideterm-gpui-terminal-view/tests/support/snapshot.rs"]
mod snapshot;
use snapshot::*;

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

    let mut foldable_mark = test_command_mark("cmd-foldable", 0, Some(3), Some(0));
    foldable_mark.output_start =
        Some(oxideterm_terminal::TerminalCommandPosition { line: 1, column: 0 });
    foldable_mark.output_end =
        Some(oxideterm_terminal::TerminalCommandPosition { line: 3, column: 0 });
    let non_foldable_mark = test_command_mark("cmd-single", 4, Some(4), Some(0));
    let foldable_layout = TerminalElement::new(
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
    .command_marks(vec![foldable_mark, non_foldable_mark], None, None)
    .layout();
    assert!(foldable_layout.command_mark_overlays[0].foldable);
    assert!(!foldable_layout.command_mark_overlays[1].foldable);
}

pub(super) fn test_command_mark(
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
        output_start: None,
        output_end: None,
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
fn ps_command_marks_select_the_structured_output_role() {
    let mut snapshot = selection_snapshot("root 1 0.0 0.0 1 1 ? Ss 6月22 0:00 node");
    snapshot.scrollback_lines = 1;
    snapshot.lines[0].absolute_line = 1;
    let mut mark = test_command_mark("ps-1", 0, Some(1), Some(0));
    mark.command = Some("ps aux | grep node".to_string());

    assert_eq!(
        semantic_line_role_for_rows(&snapshot, &[command_mark_annotation(&mark)], 0..1),
        SemanticLineRole::PsAuxOutput
    );
    assert!(mark.trim_history(1));
    snapshot.scrollback_lines = 0;
    snapshot.lines[0].absolute_line = 0;
    assert_eq!(
        semantic_line_role_for_rows(&snapshot, &[command_mark_annotation(&mark)], 0..1),
        SemanticLineRole::PsAuxOutput
    );
}

#[cfg(windows)]
mod input_tests;
mod link_tests;
