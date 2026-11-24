use oxideterm_gpui_terminal_view::CommandMarkAnnotation;
use oxideterm_terminal::{
    TerminalCommandMark, TerminalCommandMarkClosedBy, TerminalCommandMarkConfidence,
};
use oxideterm_terminal_semantic::{SemanticLineRole, semantic_output_role_for_command};
use std::ops::Range;

pub(crate) fn command_mark_annotation(mark: &TerminalCommandMark) -> CommandMarkAnnotation {
    CommandMarkAnnotation {
        command_id: mark.command_id.clone(),
        start_line: mark.start_line,
        command_line: mark.command_line,
        command_line_clipped: mark.command_line_clipped,
        output_start_line: mark.output_start_line(),
        has_output_start: mark.output_start.is_some(),
        output_role: mark
            .command
            .as_deref()
            .map(semantic_output_role_for_command)
            .unwrap_or(SemanticLineRole::Output),
        foldable_output: foldable_output(mark),
        end_line: mark.end_line,
        is_closed: mark.is_closed,
        stale: mark.stale,
        exit_code: mark.exit_code,
        finished_at: mark.finished_at,
    }
}

pub(crate) fn command_mark_foldable_range(
    mark: &TerminalCommandMark,
    total_lines: usize,
) -> Option<Range<usize>> {
    foldable_output(mark).filter(|range| range.end <= total_lines)
}

fn foldable_output(mark: &TerminalCommandMark) -> Option<Range<usize>> {
    if mark.stale
        || mark.command_line_clipped
        || !mark.is_closed
        || mark.output_confidence != TerminalCommandMarkConfidence::High
        || mark.closed_by != Some(TerminalCommandMarkClosedBy::ShellIntegration)
    {
        return None;
    }
    let start = mark.output_start?;
    let end = mark.output_end?;
    // Partial boundary rows may contain the input or the following prompt.
    // Keep them visible rather than hiding cells outside the output range.
    let first =
        (start.line + usize::from(start.column != 0)).max(mark.command_line.saturating_add(1));
    let last = end.line;
    (last > first).then_some(first..last)
}

/// Compatibility with pane callers that supply native marks rather than a cached annotation slice.
#[allow(dead_code)]
pub(crate) trait TerminalCommandMarksExt {
    fn command_marks(
        self,
        marks: impl Into<std::sync::Arc<[TerminalCommandMark]>>,
        selected: Option<String>,
        hovered: Option<String>,
    ) -> Self;
}
impl TerminalCommandMarksExt for super::TerminalElement {
    fn command_marks(
        self,
        marks: impl Into<std::sync::Arc<[TerminalCommandMark]>>,
        selected: Option<String>,
        hovered: Option<String>,
    ) -> Self {
        self.command_annotations(
            marks
                .into()
                .iter()
                .map(command_mark_annotation)
                .collect::<Vec<_>>(),
            selected,
            hovered,
        )
    }
}
