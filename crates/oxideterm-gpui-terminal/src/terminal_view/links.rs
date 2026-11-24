use std::ops::Range;

use oxideterm_environment::archive::is_archive_path;
use oxideterm_gpui_terminal_view::links;
use oxideterm_terminal::TerminalSnapshot;

pub(crate) use links::*;

pub(crate) fn display_link_ranges_for_rows_with_path_detection(
    snapshot: &TerminalSnapshot,
    rows: Range<usize>,
    detect_file_paths: bool,
) -> Vec<TerminalLinkRange> {
    links::display_link_ranges_for_rows(
        snapshot,
        rows,
        detect_file_paths.then_some(is_archive_path),
    )
}

#[cfg(test)]
pub(crate) fn display_link_ranges_with_path_detection(
    snapshot: &TerminalSnapshot,
    detect_file_paths: bool,
) -> Vec<TerminalLinkRange> {
    display_link_ranges_for_rows_with_path_detection(
        snapshot,
        0..snapshot.lines.len(),
        detect_file_paths,
    )
}

// Preserve the desktop helpers for callers that use its archive policy implicitly.
#[allow(dead_code)]
pub(super) fn detect_link_ranges_for_rows_with_path_detection(
    snapshot: &TerminalSnapshot,
    rows: Range<usize>,
    detect_file_paths: bool,
) -> Vec<TerminalLinkRange> {
    links::detect_link_ranges_for_rows(snapshot, rows, detect_file_paths.then_some(is_archive_path))
}

#[allow(dead_code)]
pub(crate) fn is_path_like(token: &str) -> bool {
    links::is_path_like(token, is_archive_path)
}
