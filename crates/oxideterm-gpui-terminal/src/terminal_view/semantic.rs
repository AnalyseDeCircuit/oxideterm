// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

use oxideterm_terminal::{TerminalAttrs, TerminalCommandMark, TerminalSnapshot};
use oxideterm_terminal_semantic::{
    CompiledSemanticScheme, SemanticClass, SemanticLineRole, SemanticShellDialect,
    classify_line_with_compiled_scheme_and_shell, semantic_line_emphasis,
    semantic_output_role_for_command,
};
#[cfg(test)]
use oxideterm_terminal_semantic::{
    SemanticScheme, built_in_scheme_document, compile_scheme_document, compiled_builtin_scheme,
};

use crate::terminal_ui::{
    TerminalUiTheme, terminal_color_from_hex, terminal_semantic_color, terminal_semantic_line_band,
    terminal_semantic_variant_color,
};
use crate::terminal_view::element::{TerminalRect, to_hsla};
use crate::terminal_view::highlight::{
    TerminalHighlightLayout, build_logical_line, logical_line_range,
};

pub(super) fn append_terminal_semantics_for_rows(
    snapshot: &TerminalSnapshot,
    command_marks: &[TerminalCommandMark],
    rows: Range<usize>,
    theme: &TerminalUiTheme,
    semantic_scheme: &CompiledSemanticScheme,
    semantic_shell: SemanticShellDialect,
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
        let line = build_logical_line(snapshot, line_range.clone());
        let spans = classify_line_with_compiled_scheme_and_shell(
            &line.text,
            role,
            semantic_scheme,
            semantic_shell,
        );
        if let Some(emphasis) = semantic_line_emphasis(&line.text, role)
            && spans.iter().any(|span| span.class == emphasis)
        {
            append_semantic_line_band(snapshot, line_range, theme, emphasis, layout);
        }
        for span in spans {
            let start = line.text[..span.range.start].chars().count();
            let end = line.text[..span.range.end].chars().count();
            let Some(cells) = line.map.get(start..end) else {
                continue;
            };
            let Some(span_text) = line.text.get(span.range.clone()) else {
                continue;
            };
            let foreground = semantic_foreground_for_variant(
                theme,
                span.class,
                span.style_variant,
                semantic_scheme,
            );
            let operator_foreground =
                matches!(span.class, SemanticClass::Timestamp | SemanticClass::Option)
                    .then(|| semantic_foreground(theme, SemanticClass::Operator, semantic_scheme));
            let mut option_prefix = span.class == SemanticClass::Option;
            for (ch, mapped) in span_text.chars().zip(cells) {
                let class = semantic_component_class(span.class, ch, option_prefix);
                if option_prefix && ch != '-' {
                    option_prefix = false;
                }
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
                if cell.style_origin.foreground_explicit()
                    || cell.style_origin.background_explicit()
                    || cell.attrs != TerminalAttrs::default()
                {
                    continue;
                }
                layout.foregrounds.insert(
                    key,
                    if class == span.class {
                        foreground
                    } else {
                        operator_foreground.unwrap_or(foreground)
                    },
                );
            }
        }
    }
}

fn append_semantic_line_band(
    snapshot: &TerminalSnapshot,
    rows: Range<usize>,
    theme: &TerminalUiTheme,
    class: SemanticClass,
    layout: &mut TerminalHighlightLayout,
) {
    // Bands are intentionally quieter than token colors and remain behind selection/search paint.
    let Some((color, opacity)) = terminal_semantic_line_band(theme, class) else {
        return;
    };
    let existing_background_overlaps = layout
        .backgrounds
        .iter()
        .any(|background| rows.contains(&background.row));
    let has_explicit_background = snapshot.lines.get(rows.clone()).is_none_or(|rows| {
        rows.iter().any(|row| {
            row.cells
                .iter()
                .any(|cell| cell.style_origin.background_explicit())
        })
    });
    if existing_background_overlaps || has_explicit_background {
        return;
    }

    let mut color = to_hsla(terminal_color_from_hex(color));
    color.alpha = opacity;
    layout.backgrounds.extend(rows.map(|row| TerminalRect {
        row,
        col: 0,
        cells: snapshot.cols,
        color,
    }));
}

fn semantic_component_class(class: SemanticClass, ch: char, option_prefix: bool) -> SemanticClass {
    // Structured punctuation gets its own color without changing the stable
    // semantic scheme format or fragmenting public timestamp/option spans.
    if class == SemanticClass::Timestamp && matches!(ch, ':' | '.' | ',' | '-' | '/' | '+') {
        SemanticClass::Operator
    } else if class == SemanticClass::Option && option_prefix && ch == '-' {
        SemanticClass::Operator
    } else {
        class
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
    if command_marks.iter().any(|mark| {
        !mark.command_line_clipped && (start_line..=end_line).contains(&mark.command_line)
    }) {
        return SemanticLineRole::Command;
    }
    if let Some(mark) = command_marks.iter().rev().find(|mark| {
        let output_start = mark.output_start_line();
        let output_end = mark.end_line.unwrap_or(end_line);
        output_start <= end_line && output_end >= start_line
    }) {
        return mark
            .command
            .as_deref()
            .map(semantic_output_role_for_command)
            .unwrap_or(SemanticLineRole::Output);
    }
    SemanticLineRole::Unknown
}

fn semantic_foreground(
    theme: &TerminalUiTheme,
    class: SemanticClass,
    semantic_scheme: &CompiledSemanticScheme,
) -> gpui::Hsla {
    let color = terminal_semantic_color(theme, class, semantic_scheme);
    to_hsla(terminal_color_from_hex(color))
}

fn semantic_foreground_for_variant(
    theme: &TerminalUiTheme,
    class: SemanticClass,
    style_variant: Option<u8>,
    semantic_scheme: &CompiledSemanticScheme,
) -> gpui::Hsla {
    let color = terminal_semantic_variant_color(theme, class, style_variant, semantic_scheme);
    to_hsla(terminal_color_from_hex(color))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use oxideterm_terminal::{
        TerminalCell, TerminalColor, TerminalCommandMarkClosedBy, TerminalCommandMarkConfidence,
        TerminalCommandMarkDetectionSource, TerminalCursorShape, TerminalRow, TerminalStyleOrigin,
    };
    use oxideterm_theme::{ThemeTokens, theme_by_id};

    use super::*;

    fn cell(ch: char, foreground_explicit: bool) -> TerminalCell {
        styled_cell(ch, foreground_explicit, false)
    }

    fn styled_cell(ch: char, foreground_explicit: bool, background_explicit: bool) -> TerminalCell {
        TerminalCell {
            ch,
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            style_origin: TerminalStyleOrigin::new(foreground_explicit, background_explicit),
            attrs: TerminalAttrs::default(),
            extra: None,
            cursor: false,
        }
    }

    fn snapshot(text: &str, explicit_range: Range<usize>) -> TerminalSnapshot {
        let mut row = TerminalRow {
            line_id: 0,
            source_id: 0,
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
            compiled_builtin_scheme(SemanticScheme::Balanced),
            SemanticShellDialect::Auto,
            &mut layout,
        );

        assert!(!layout.foregrounds.contains_key(&(0, 0)));
        assert!(layout.foregrounds.contains_key(&(0, 4)));
    }

    #[test]
    fn explicit_severity_adds_a_theme_band_without_covering_ansi_backgrounds() {
        let theme = TerminalUiTheme::default();
        let scheme = compiled_builtin_scheme(SemanticScheme::Balanced);
        let mut error_snapshot = snapshot("ERROR connection refused", 0..0);
        let mut error_layout = TerminalHighlightLayout::empty();

        append_terminal_semantics_for_rows(
            &error_snapshot,
            &[],
            0..1,
            &theme,
            scheme,
            SemanticShellDialect::Auto,
            &mut error_layout,
        );

        assert_eq!(error_layout.backgrounds.len(), 1);
        assert_eq!(error_layout.backgrounds[0].cells, error_snapshot.cols);
        assert_eq!(
            error_layout.backgrounds[0].color.alpha,
            terminal_semantic_line_band(&theme, SemanticClass::Error)
                .expect("error band")
                .1
        );

        let warning_snapshot = snapshot("WARNING disk space is low", 0..0);
        let mut warning_layout = TerminalHighlightLayout::empty();
        append_terminal_semantics_for_rows(
            &warning_snapshot,
            &[],
            0..1,
            &theme,
            scheme,
            SemanticShellDialect::Auto,
            &mut warning_layout,
        );

        assert_eq!(warning_layout.backgrounds.len(), 1);
        assert_eq!(warning_layout.backgrounds[0].cells, warning_snapshot.cols);
        assert_eq!(
            warning_layout.backgrounds[0].color.alpha,
            terminal_semantic_line_band(&theme, SemanticClass::Warning)
                .expect("warning band")
                .1
        );
        assert_ne!(
            warning_layout.backgrounds[0].color,
            error_layout.backgrounds[0].color
        );

        error_snapshot.lines[0].cells_mut()[0] = styled_cell('E', false, true);
        error_snapshot.lines[0].refresh_signature();
        let mut ansi_layout = TerminalHighlightLayout::empty();
        append_terminal_semantics_for_rows(
            &error_snapshot,
            &[],
            0..1,
            &theme,
            scheme,
            SemanticShellDialect::Auto,
            &mut ansi_layout,
        );

        assert!(ansi_layout.backgrounds.is_empty());
    }

    #[test]
    fn ordinary_mentions_and_active_commands_do_not_add_severity_bands() {
        let theme = TerminalUiTheme::default();
        let scheme = compiled_builtin_scheme(SemanticScheme::Balanced);
        for (text, active_input) in [
            ("the command reports an error when offline", false),
            ("ERROR is still being typed", true),
        ] {
            let mut snapshot = snapshot(text, 0..0);
            snapshot.lines[0].active_input = active_input;
            let mut layout = TerminalHighlightLayout::empty();

            append_terminal_semantics_for_rows(
                &snapshot,
                &[],
                0..1,
                &theme,
                scheme,
                SemanticShellDialect::Auto,
                &mut layout,
            );

            assert!(
                layout.backgrounds.is_empty(),
                "unexpected band for {text:?}"
            );
        }
    }

    #[test]
    fn localized_date_colors_survive_wide_character_spacers() {
        let mut snapshot = snapshot("6月22", 0..0);
        let cells = snapshot.lines[0].cells_mut();
        cells[1].wide = true;
        cells.insert(2, cell(' ', false));
        snapshot.cols = cells.len();
        snapshot.lines[0].refresh_signature();
        let theme = TerminalUiTheme::default();
        let scheme = compiled_builtin_scheme(SemanticScheme::Balanced);
        let mut layout = TerminalHighlightLayout::empty();

        append_terminal_semantics_for_rows(
            &snapshot,
            &[],
            0..1,
            &theme,
            scheme,
            SemanticShellDialect::Auto,
            &mut layout,
        );

        let timestamp = semantic_foreground(&theme, SemanticClass::Timestamp, scheme);
        for column in [0, 1, 3, 4] {
            assert_eq!(layout.foregrounds.get(&(0, column)), Some(&timestamp));
        }
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
    fn ps_command_marks_select_the_structured_output_role() {
        let mut snapshot = snapshot("root 1 0.0 0.0 1 1 ? Ss 6月22 0:00 node", 0..0);
        snapshot.scrollback_lines = 1;
        snapshot.lines[0].absolute_line = 1;
        let mut mark = TerminalCommandMark {
            command_id: "ps-1".to_string(),
            command: Some("ps aux | grep node".to_string()),
            start_line: 0,
            command_line: 0,
            command_line_clipped: false,
            end_line: Some(1),
            is_closed: true,
            closed_by: Some(TerminalCommandMarkClosedBy::ShellIntegration),
            exit_code: Some(0),
            duration_ms: Some(1),
            detection_source: TerminalCommandMarkDetectionSource::ShellIntegration,
            submitted_by: None,
            confidence: TerminalCommandMarkConfidence::High,
            output_confidence: TerminalCommandMarkConfidence::High,
            stale: false,
            started_at: 1,
            finished_at: Some(2),
        };

        assert_eq!(
            semantic_line_role_for_rows(&snapshot, &[mark.clone()], 0..1),
            SemanticLineRole::PsAuxOutput
        );
        assert!(mark.trim_history(1));
        snapshot.scrollback_lines = 0;
        snapshot.lines[0].absolute_line = 0;
        assert_eq!(
            semantic_line_role_for_rows(&snapshot, &[mark], 0..1),
            SemanticLineRole::PsAuxOutput
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
            compiled_builtin_scheme(SemanticScheme::Balanced),
            SemanticShellDialect::Auto,
            &mut layout,
        );

        assert_eq!(layout.foregrounds.get(&(0, 0)), Some(&manual_color));
        assert!(layout.foregrounds.contains_key(&(0, 1)));
    }

    #[test]
    fn conservative_scheme_reaches_the_render_adapter() {
        let snapshot = snapshot("Info 247 failed", 0..0);
        let mut layout = TerminalHighlightLayout::empty();

        append_terminal_semantics_for_rows(
            &snapshot,
            &[],
            0..1,
            &TerminalUiTheme::default(),
            compiled_builtin_scheme(SemanticScheme::Conservative),
            SemanticShellDialect::Auto,
            &mut layout,
        );

        assert!(!layout.foregrounds.contains_key(&(0, 0)));
        assert!(!layout.foregrounds.contains_key(&(0, 5)));
        assert!(layout.foregrounds.contains_key(&(0, 9)));
    }

    #[test]
    fn custom_scheme_color_overrides_the_theme_semantic_color() {
        let mut document = built_in_scheme_document(SemanticScheme::Balanced);
        document.id = "custom:colors".to_string();
        document
            .colors
            .insert(SemanticClass::Error, "#123456".to_string());
        let scheme = compile_scheme_document(&document).expect("compile custom scheme");

        assert_eq!(
            semantic_foreground(&TerminalUiTheme::default(), SemanticClass::Error, &scheme),
            to_hsla(terminal_color_from_hex(0x123456))
        );
    }

    #[test]
    fn structured_punctuation_uses_a_distinct_semantic_color() {
        let snapshot = snapshot("0:00 --color=auto ({[]})", 0..0);
        let theme =
            TerminalUiTheme::from_tokens(ThemeTokens::from_builtin(theme_by_id("solarized-light")));
        let scheme = compiled_builtin_scheme(SemanticScheme::Balanced);
        let mut layout = TerminalHighlightLayout::empty();

        append_terminal_semantics_for_rows(
            &snapshot,
            &[],
            0..1,
            &theme,
            scheme,
            SemanticShellDialect::Auto,
            &mut layout,
        );

        let timestamp = semantic_foreground(&theme, SemanticClass::Timestamp, scheme);
        let operator = semantic_foreground(&theme, SemanticClass::Operator, scheme);
        let number = semantic_foreground(&theme, SemanticClass::Number, scheme);
        let option = semantic_foreground(&theme, SemanticClass::Option, scheme);
        let string = semantic_foreground(&theme, SemanticClass::String, scheme);

        assert_ne!(timestamp, operator);
        assert_ne!(number, operator);
        assert_eq!(layout.foregrounds.get(&(0, 0)), Some(&timestamp));
        assert_eq!(layout.foregrounds.get(&(0, 1)), Some(&operator));
        assert_eq!(layout.foregrounds.get(&(0, 2)), Some(&timestamp));
        assert_eq!(layout.foregrounds.get(&(0, 5)), Some(&operator));
        assert_eq!(layout.foregrounds.get(&(0, 6)), Some(&operator));
        assert_eq!(layout.foregrounds.get(&(0, 7)), Some(&option));
        assert_eq!(layout.foregrounds.get(&(0, 12)), Some(&operator));
        assert_eq!(layout.foregrounds.get(&(0, 13)), Some(&string));

        let outer = layout.foregrounds.get(&(0, 18));
        let middle = layout.foregrounds.get(&(0, 19));
        let inner = layout.foregrounds.get(&(0, 20));
        assert_ne!(outer, middle);
        assert_ne!(middle, inner);
        assert_ne!(outer, inner);
        assert_eq!(layout.foregrounds.get(&(0, 21)), inner);
        assert_eq!(layout.foregrounds.get(&(0, 22)), middle);
        assert_eq!(layout.foregrounds.get(&(0, 23)), outer);
    }
}
