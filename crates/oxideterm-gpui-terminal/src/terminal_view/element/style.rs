use gpui::{Hsla, TextRun};
use oxideterm_gpui_terminal_view::style;
use oxideterm_terminal::TerminalCell;

use crate::terminal_ui::{TerminalMetrics, TerminalUiTheme, terminal_color_from_hex};

pub(crate) use style::*;

pub(crate) fn text_run_for_cell(
    cell: &TerminalCell,
    color: Hsla,
    link: bool,
    metrics: &TerminalMetrics,
) -> TextRun {
    style::text_run_for_cell(cell, color, link, &metrics.font, metrics.letter_spacing)
}

pub(crate) fn marked_text_run(
    text: &str,
    theme: &TerminalUiTheme,
    metrics: &TerminalMetrics,
) -> TextRun {
    style::marked_text_run(
        text,
        theme.foreground,
        theme.background,
        &metrics.font,
        metrics.letter_spacing,
    )
}

pub(crate) fn ghost_text_run(
    text: &str,
    theme: &TerminalUiTheme,
    metrics: &TerminalMetrics,
) -> TextRun {
    style::ghost_text_run(
        text,
        theme.foreground,
        &metrics.font,
        metrics.letter_spacing,
    )
}

pub(crate) fn timestamp_text_run(
    text: &str,
    theme: &TerminalUiTheme,
    metrics: &TerminalMetrics,
) -> TextRun {
    style::timestamp_text_run(text, theme.header_foreground, &metrics.font)
}

pub(crate) fn terminal_background(theme: &TerminalUiTheme) -> Hsla {
    to_hsla(terminal_color_from_hex(theme.background))
}
