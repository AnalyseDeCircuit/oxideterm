// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

use oxideterm_terminal::{TerminalAttrs, TerminalCommandMark, TerminalSnapshot};
use oxideterm_terminal_semantic::{SemanticClass, SemanticLineRole, classify_line};

use crate::terminal_ui::{TerminalUiTheme, terminal_color_from_hex};
use crate::terminal_view::element::to_hsla;
use crate::terminal_view::highlight::{
    TerminalHighlightLayout, build_logical_line, logical_line_range,
};

pub(super) fn append_terminal_semantics_for_rows(
    snapshot: &TerminalSnapshot,
    command_marks: &[TerminalCommandMark],
    rows: Range<usize>,
    theme: &TerminalUiTheme,
    layout: &mut TerminalHighlightLayout,
) {
    let mut seen_lines = std::collections::HashSet::new();
    for row in rows {
        let Some(line_range) = logical_line_range(snapshot, row) else {
            continue;
        };
        if !seen_lines.insert(line_range.clone()) {
            continue;
        }
        let role = semantic_line_role_for_rows(snapshot, command_marks, line_range.clone());
        let line = build_logical_line(snapshot, line_range);
        for span in classify_line(&line.text, role) {
            let start = line.text[..span.range.start].chars().count();
            let end = line.text[..span.range.end].chars().count();
            let Some(cells) = line.map.get(start..end) else {
                continue;
            };
            let foreground = semantic_foreground(theme, span.class);
            for mapped in cells {
                let key = (mapped.row, mapped.col);
                if layout.foregrounds.contains_key(&key) {
                    continue;
                }
                let Some(cell) = snapshot
                    .lines
                    .get(mapped.row)
                    .and_then(|row| row.cells.get(mapped.col))
                else {
                    continue;
                };
                // Semantic colors fill only genuinely unstyled terminal text.
                if cell.style_origin.foreground_explicit
                    || cell.style_origin.background_explicit
                    || cell.attrs != TerminalAttrs::default()
                {
                    continue;
                }
                layout.foregrounds.insert(key, foreground);
            }
        }
    }
}

pub(super) fn semantic_line_role_for_rows(
    snapshot: &TerminalSnapshot,
    command_marks: &[TerminalCommandMark],
    rows: Range<usize>,
) -> SemanticLineRole {
    if snapshot
        .lines
        .get(rows.clone())
        .is_some_and(|lines| lines.iter().any(|row| row.active_input))
    {
        return SemanticLineRole::Command;
    }

    let viewport_start = snapshot
        .scrollback_lines
        .saturating_sub(snapshot.display_offset);
    let start_line = viewport_start.saturating_add(rows.start);
    let end_line = viewport_start.saturating_add(rows.end.saturating_sub(1));
    if command_marks
        .iter()
        .any(|mark| (start_line..=end_line).contains(&mark.command_line))
    {
        return SemanticLineRole::Command;
    }
    if command_marks.iter().any(|mark| {
        let output_start = mark.command_line.saturating_add(1);
        let output_end = mark.end_line.unwrap_or(end_line);
        output_start <= end_line && output_end >= start_line
    }) {
        return SemanticLineRole::Output;
    }
    SemanticLineRole::Unknown
}

fn semantic_foreground(theme: &TerminalUiTheme, class: SemanticClass) -> gpui::Hsla {
    let terminal = theme.tokens.terminal;
    let color = match class {
        SemanticClass::Command | SemanticClass::Link => terminal.bright_cyan,
        SemanticClass::Option | SemanticClass::Timestamp => terminal.cyan,
        SemanticClass::String | SemanticClass::Path => terminal.yellow,
        SemanticClass::Address | SemanticClass::Info => terminal.bright_blue,
        SemanticClass::Number => terminal.bright_magenta,
        SemanticClass::Error => terminal.bright_red,
        SemanticClass::Warning => terminal.bright_yellow,
        SemanticClass::Success => terminal.bright_green,
    };
    to_hsla(terminal_color_from_hex(color))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use oxideterm_terminal::{
        TerminalCell, TerminalColor, TerminalCursorShape, TerminalRow, TerminalStyleOrigin,
    };

    use super::*;

    fn cell(ch: char, foreground_explicit: bool) -> TerminalCell {
        TerminalCell {
            ch,
            zerowidth: String::new(),
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            style_origin: TerminalStyleOrigin {
                foreground_explicit,
                background_explicit: false,
            },
            attrs: TerminalAttrs::default(),
            hyperlink: None,
            cursor: false,
        }
    }

    fn snapshot(text: &str, explicit_range: Range<usize>) -> TerminalSnapshot {
        let mut row = TerminalRow {
            absolute_line: 0,
            cells: Arc::new(
                text.chars()
                    .enumerate()
                    .map(|(index, ch)| cell(ch, explicit_range.contains(&index)))
                    .collect(),
            ),
            wrapped: false,
            active_input: false,
            signature: 0,
        };
        row.refresh_signature();
        TerminalSnapshot {
            generation: 0,
            cols: text.chars().count(),
            rows: 1,
            cursor_col: 0,
            cursor_row: 0,
            cursor_shape: TerminalCursorShape::Block,
            display_offset: 0,
            scrollback_lines: 0,
            lines: vec![row],
            images: Vec::new(),
        }
    }

    #[test]
    fn semantic_colors_do_not_replace_explicit_ansi_foregrounds() {
        let snapshot = snapshot("not enabled", 0..3);
        let mut layout = TerminalHighlightLayout::empty();

        append_terminal_semantics_for_rows(
            &snapshot,
            &[],
            0..1,
            &TerminalUiTheme::default(),
            &mut layout,
        );

        assert!(!layout.foregrounds.contains_key(&(0, 0)));
        assert!(layout.foregrounds.contains_key(&(0, 4)));
    }

    #[test]
    fn active_input_rows_use_command_context() {
        let mut snapshot = snapshot("sudo apt update", 0..0);
        snapshot.lines[0].active_input = true;
        assert_eq!(
            semantic_line_role_for_rows(&snapshot, &[], 0..1),
            SemanticLineRole::Command
        );
    }

    #[test]
    fn semantic_colors_do_not_replace_manual_foregrounds() {
        let snapshot = snapshot("failed", 0..0);
        let mut layout = TerminalHighlightLayout::empty();
        let manual_color = to_hsla(terminal_color_from_hex(0x123456));
        layout.foregrounds.insert((0, 0), manual_color);

        append_terminal_semantics_for_rows(
            &snapshot,
            &[],
            0..1,
            &TerminalUiTheme::default(),
            &mut layout,
        );

        assert_eq!(layout.foregrounds.get(&(0, 0)), Some(&manual_color));
        assert!(layout.foregrounds.contains_key(&(0, 1)));
    }
}
