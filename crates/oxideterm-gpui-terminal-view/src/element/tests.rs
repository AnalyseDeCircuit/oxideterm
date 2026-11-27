use std::sync::Arc;

use gpui::px;
use oxideterm_terminal_emulator::{
    TerminalCell, TerminalColor, TerminalCursorShape, TerminalRow, TerminalSnapshot,
};

use super::*;

use crate::tests::test_metrics;

type TerminalElement = super::TerminalElement<()>;

fn row_with_text_and_cursor(absolute_line: i64, text: &str, cursor_col: usize) -> TerminalRow {
    // Default-colored cells carry the theme defaults that the backend palette resolves.
    let theme = TerminalUiTheme::default();
    let fg = terminal_color_from_hex(theme.foreground);
    let bg = terminal_color_from_hex(theme.background);
    let mut cells = text
        .chars()
        .enumerate()
        .map(|(col, ch)| TerminalCell {
            ch,
            wide: false,
            fg,
            bg,
            style_origin: Default::default(),
            attrs: Default::default(),
            extra: None,
            cursor: col == cursor_col,
        })
        .collect::<Vec<_>>();
    cells.resize_with(cursor_col.saturating_add(1), || TerminalCell {
        ch: ' ',
        wide: false,
        fg,
        bg,
        style_origin: Default::default(),
        attrs: Default::default(),
        extra: None,
        cursor: false,
    });
    if let Some(cursor_cell) = cells.get_mut(cursor_col) {
        cursor_cell.cursor = true;
    }
    let mut row = TerminalRow {
        line_id: absolute_line.max(0) as u64,
        source_id: 0,
        absolute_line,
        cells: Arc::new(cells),
        wrapped: false,
        active_input: true,
        signature: 0,
    };
    row.refresh_signature();
    row
}

fn snapshot(display_offset: usize, lines: Vec<TerminalRow>) -> TerminalSnapshot {
    TerminalSnapshot {
        row_reuse: None,
        logical_context: Vec::new(),
        generation: 1,
        cols: 1,
        rows: lines.len(),
        cursor_col: 0,
        cursor_row: 0,
        cursor_shape: TerminalCursorShape::Block,
        display_offset,
        scrollback_lines: display_offset,
        lines,
        images: Vec::new(),
    }
}

fn element(snapshot: TerminalSnapshot) -> TerminalElement {
    TerminalElement::new(
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
}

#[test]
fn search_overview_survives_thumb_hiding_and_scrolling_until_search_closes() {
    let mut snapshot = snapshot(0, Vec::new());
    snapshot.rows = 10;
    snapshot.scrollback_lines = 90;
    let mut view = element(snapshot);
    view.search_query = Some("hit".into());
    view.search_matches = Arc::from([TerminalSearchMatch {
        line: -40,
        start_col: 0,
        end_col: 1,
        ranges: Vec::new(),
    }]);
    view.selected_search_match = Some(0);
    view.scrollbar_visible = false;
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(200.0), px(100.0)));
    assert_eq!(
        view.layout_for_bounds(bounds).search_scrollbar_markers,
        vec![(50.0, true)]
    );
    view.snapshot.display_offset = 90;
    assert_eq!(
        view.layout_for_bounds(bounds).search_scrollbar_markers,
        vec![(50.0, true)]
    );
    view.search_query = None;
    assert_eq!(
        view.layout_for_bounds(bounds).search_scrollbar_markers,
        vec![]
    );
}

#[test]
fn semantic_budget_accounts_for_both_eviction_limits_and_clear() {
    use oxideterm_terminal_semantic::{SemanticClass, SemanticSpan};
    let mut cache = TerminalLayoutCache::default();
    let key = |id| (id, SemanticLineRole::Output, 0);
    let spans = |count| {
        vec![
            SemanticSpan {
                range: 0..1,
                class: SemanticClass::Number,
                style_variant: None
            };
            count
        ]
    };
    cache.semantic_spans_for(key(1), || spans(40_000));
    cache.semantic_spans_for(key(2), || spans(20_000));
    cache.semantic_spans_for(key(3), || spans(10_000));
    let mut keys = cache
        .semantic_spans
        .entries
        .keys()
        .map(|key| key.0)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(keys, vec![2, 3]);
    assert_eq!(cache.semantic_span_count, 30_000);
    cache.semantic_spans_for(key(2), || panic!("a cache hit must not classify again"));
    assert_eq!(cache.semantic_span_count, 30_000);
    cache.clear();
    assert_eq!(cache.semantic_span_count, 0);
    cache.semantic_spans.capacity = 2;
    for id in 1..=3 {
        cache.semantic_spans_for(key(id), || spans(id as usize * 3));
    }
    let mut keys = cache
        .semantic_spans
        .entries
        .keys()
        .map(|key| key.0)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(keys, vec![2, 3]);
    assert_eq!(cache.semantic_span_count, 15);
}

#[test]
fn layout_hit_does_not_repopulate_evicted_classifications() {
    let mut cache = TerminalLayoutCache::default();
    let mut terminal = crate::tests::emulator(10, 4);
    let _ = terminal.feed_bytes(b"'abcdefghijklmnopqrstuvwx'\r\nDONE");
    let mut view = element(terminal.snapshot());
    view.semantic_coloring = true;
    let lines = view.logical_lines_for_rows(0..4);
    let roles = view.semantic_roles_for_lines(&lines, Some(&mut cache));
    let (_, first) = view.cached_highlight_layout_for_rows(&lines, &roles, &mut cache);
    cache.semantic_spans.clear();
    cache.semantic_span_count = 0;
    let (_, second) = view.cached_highlight_layout_for_rows(&lines, &roles, &mut cache);
    assert!(Arc::ptr_eq(&first[0].1, &second[0].1));
    assert!(cache.semantic_spans.entries.is_empty());
    assert_eq!(cache.semantic_span_count, 0);
}

#[test]
fn oversized_classification_preserves_other_cached_lines_and_reuses_layout() {
    let mut cache = TerminalLayoutCache::default();
    let mut terminal = crate::tests::emulator(120, 4);
    for repetitions in [100, 70_000] {
        let _ = terminal.feed_bytes(("1 ".repeat(repetitions) + "\r\nDONE\r\n").as_bytes());
        let mut view = element(terminal.snapshot());
        view.semantic_coloring = true;
        let lines = view.logical_lines_for_rows(0..4);
        let roles = view.semantic_roles_for_lines(&lines, Some(&mut cache));
        let (_, first) = view.cached_highlight_layout_for_rows(&lines, &roles, &mut cache);
        let (_, second) = view.cached_highlight_layout_for_rows(&lines, &roles, &mut cache);
        assert!(Arc::ptr_eq(&first[0].1, &second[0].1));
        assert!(
            cache
                .semantic_spans
                .entries
                .values()
                .any(|spans| spans.len() == 100)
        );
        assert!(
            cache
                .semantic_spans
                .entries
                .values()
                .all(|spans| spans.len() <= TERMINAL_SEMANTIC_SPAN_CACHE_BUDGET)
        );
    }
}

#[test]
fn semantic_cache_invalidates_when_a_clipped_closing_quote_changes() {
    let mut terminal = crate::tests::emulator(10, 6);
    let _ = terminal.feed_bytes(b"'abcdefghijklmnopqrstuvwx'\r\nDONE");
    let mut cache = TerminalLayoutCache::default();
    let mut view = element(crate::tests::viewport(&mut terminal, 0, 1));
    view.semantic_coloring = true;
    let lines = view.logical_lines_for_rows(0..1);
    let roles = view.semantic_roles_for_lines(&lines, Some(&mut cache));
    let (_, before) = view.cached_highlight_layout_for_rows(&lines, &roles, &mut cache);
    let expected = TerminalHighlightForeground::static_color(to_hsla(terminal_color_from_hex(
        terminal_semantic_color(
            &view.theme,
            oxideterm_terminal_semantic::SemanticClass::String,
            view.semantic_scheme(),
        ),
    )));
    assert_eq!(
        before[0].1.foregrounds.get(&(0, 1)).map(|fg| fg.color),
        Some(expected.color)
    );
    let _ = terminal.feed_bytes(b"\x1b[3;6H \x1b[4;1H");
    view.snapshot = crate::tests::viewport(&mut terminal, 0, 1);
    let lines = view.logical_lines_for_rows(0..1);
    let roles = view.semantic_roles_for_lines(&lines, Some(&mut cache));
    let (_, after) = view.cached_highlight_layout_for_rows(&lines, &roles, &mut cache);
    assert!(
        after
            .iter()
            .all(|(start, colors)| *start != 0 || !colors.foregrounds.contains_key(&(0, 1)))
    );
}

#[test]
fn semantic_classification_is_reused_across_scrolled_fragments() {
    let mut terminal = crate::tests::emulator(10, 2);
    let _ = terminal.feed_bytes(b"'abcdefghijklmnopqrstuvwx'\r\nDONE");
    let mut cache = TerminalLayoutCache::default();
    let mut classified = None;
    for offset in [2, 1, 0, 2] {
        let mut view = element(crate::tests::viewport(&mut terminal, offset, 2));
        view.semantic_coloring = true;
        let lines = view.logical_lines_for_rows(0..2);
        let roles = view.semantic_roles_for_lines(&lines, Some(&mut cache));
        let (_, layout) = view.cached_highlight_layout_for_rows(&lines, &roles, &mut cache);
        let expected = to_hsla(terminal_color_from_hex(terminal_semantic_color(
            &view.theme,
            oxideterm_terminal_semantic::SemanticClass::String,
            view.semantic_scheme(),
        )));
        assert_eq!(
            layout[0].1.foregrounds.get(&(0, 0)).map(|fg| fg.color),
            Some(expected)
        );
        let spans = cache
            .semantic_spans
            .entries
            .values()
            .next()
            .unwrap()
            .clone();
        if let Some(previous) = &classified {
            assert!(Arc::ptr_eq(previous, &spans));
        }
        classified = Some(spans);
    }
}

#[test]
fn stable_line_identity_keeps_layout_keys_across_grid_movement() {
    let mut original_row = row_with_text_and_cursor(4, "stable", 5);
    original_row.line_id = 42;
    let mut moved_row = original_row.clone();
    moved_row.absolute_line = 3;
    let original = element(snapshot(0, vec![original_row]));
    let moved = element(snapshot(0, vec![moved_row]));
    let original_lines = original.logical_lines_for_rows(0..1);
    let moved_lines = moved.logical_lines_for_rows(0..1);
    let original_line = original_lines.line_for_row(0).expect("original line");
    let moved_line = moved_lines.line_for_row(0).expect("moved line");

    assert_eq!(
        original.row_layout_cache_key_with_logical_line(
            0,
            original_line,
            None,
            &original.row_layout_style_hasher()
        ),
        moved.row_layout_cache_key_with_logical_line(
            0,
            moved_line,
            None,
            &moved.row_layout_style_hasher()
        )
    );
    assert_eq!(
        original.logical_highlight_cache_key_with_logical_line(original_line, None),
        moved.logical_highlight_cache_key_with_logical_line(moved_line, None)
    );
    assert_eq!(original.row_link_cache_key(0), moved.row_link_cache_key(0));
}

#[test]
fn cell_drawings_are_batched_separately_from_text_and_combining_marks() {
    let row = row_with_text_and_cursor(0, "a┌──┐b", 5);
    let mut input = snapshot(0, vec![row]);
    input.cols = 6;
    input.cursor_shape = TerminalCursorShape::Bar;
    let layout = element(input).layout();
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| run.text.as_ref())
            .collect::<Vec<_>>(),
        ["a", "┌──┐", "b"]
    );

    let mut row = row_with_text_and_cursor(0, "a─b", 2);
    row.cells_mut()[1].set_extra("\u{301}".into(), None);
    row.refresh_signature();
    let mut input = snapshot(0, vec![row]);
    input.cols = 3;
    input.cursor_shape = TerminalCursorShape::Bar;
    let layout = element(input).layout();
    assert_eq!(
        layout
            .text_runs
            .iter()
            .map(|run| run.text.as_ref())
            .collect::<Vec<_>>(),
        ["a─\u{301}b"]
    );
}

#[test]
fn wide_han_runs_keep_grid_columns_in_logical_and_visual_order() {
    for bidi in [false, true] {
        let mut row = row_with_text_and_cursor(0, "X连 接 成 功 界 🦀 א", 13);
        for i in [1, 3, 5, 7, 9, 11] {
            row.cells_mut()[i].wide = true;
        }
        row.cells_mut()[9].set_zerowidth("\u{fe00}".into());
        row.refresh_signature();
        let mut input = snapshot(0, vec![row]);
        input.cols = 14;
        input.cursor_shape = TerminalCursorShape::Hidden;
        let mut view = element(input);
        view.bidi_enabled = bidi;
        let layout = view.layout();
        let runs: Vec<_> = layout
            .text_runs
            .iter()
            .map(|run| (run.col, run.cells, run.text.as_ref()))
            .collect();
        assert_eq!(
            runs,
            [
                (0, 1, "X"),
                (1, 8, "连接成功"),
                (9, 2, "界\u{fe00}"),
                (11, 2, "🦀"),
                (13, 1, "א")
            ],
            "bidi={bidi}"
        );
    }
}

#[test]
fn edited_row_reuses_only_unchanged_runs_with_matching_metrics() {
    use crate::selection::{TerminalGridPoint, TerminalSelectionMode};

    let mut input = snapshot(0, vec![row_with_text_and_cursor(42, "alpha beta", 9)]);
    input.cols = 10;
    input.cursor_shape = TerminalCursorShape::Hidden;
    let mut view = element(input);
    let mut cache = TerminalLayoutCache::default();
    let first = view.layout_for_rows(0..1, Some(&mut cache));
    let original = &first.cached_rows[0].layout.text_runs;
    assert_eq!(
        original
            .iter()
            .map(|run| run.text.as_ref())
            .collect::<Vec<_>>(),
        ["alpha", "beta"]
    );

    view.snapshot.lines[0].cells_mut()[9].ch = 's';
    view.snapshot.lines[0].refresh_signature();
    let edited = view.layout_for_rows(0..1, Some(&mut cache));
    let runs = &edited.cached_rows[0].layout.text_runs;
    assert_eq!(
        runs.iter()
            .map(|run| (run.col, run.text.as_ref()))
            .collect::<Vec<_>>(),
        [(0, "alpha"), (6, "bets")]
    );
    assert!(Arc::ptr_eq(&original[0].cache, &runs[0].cache));
    assert!(!Arc::ptr_eq(&original[1].cache, &runs[1].cache));

    view.selection = Some(TerminalSelection {
        anchor: TerminalGridPoint { line: 0, col: 0 },
        head: TerminalGridPoint { line: 0, col: 3 },
        mode: TerminalSelectionMode::Simple,
    });
    let selected = view.layout_for_rows(0..1, Some(&mut cache));
    let selected_row = &selected.cached_rows[0].layout;
    assert_eq!(
        selected_row
            .selections
            .iter()
            .map(|rect| (rect.col, rect.cells))
            .collect::<Vec<_>>(),
        [(0, 4)]
    );
    assert!(Arc::ptr_eq(
        &runs[0].cache,
        &selected_row.text_runs[0].cache
    ));
    assert!(Arc::ptr_eq(
        &runs[1].cache,
        &selected_row.text_runs[1].cache
    ));

    view.snapshot.lines[0].cells_mut()[0].fg = TerminalColor::rgb(255, 0, 0);
    view.snapshot.lines[0].refresh_signature();
    let recolored = view.layout_for_rows(0..1, Some(&mut cache));
    assert_eq!(
        recolored.cached_rows[0].layout.text_runs[0].style.color,
        gpui::rgb(0xff0000).into()
    );
    assert!(!Arc::ptr_eq(
        &original[0].cache,
        &recolored.cached_rows[0].layout.text_runs[0].cache
    ));

    let mut previous = recolored;
    for metric in 0..5 {
        match metric {
            0 => view.metrics.font_size += px(1.),
            1 => view.metrics.cell_width += px(1.),
            2 => view.metrics.line_height += px(1.),
            3 => view.metrics.font.weight = gpui::FontWeight::BOLD,
            _ => view.metrics.font.family = "Consolas".into(),
        }
        let resized = view.layout_for_rows(0..1, Some(&mut cache));
        let before = previous.cached_rows[0].layout.text_runs.last().unwrap();
        let after = resized.cached_rows[0].layout.text_runs.last().unwrap();
        assert_eq!(after.text.as_ref(), "bets");
        assert_eq!(after.style.font, view.metrics.font);
        assert!(!Arc::ptr_eq(&before.cache, &after.cache));
        previous = resized;
    }
    let retired = Arc::downgrade(&original[0].cache);
    drop(first);
    drop(edited);
    drop(selected);
    drop(previous);
    cache.clear();
    assert!(
        retired.upgrade().is_none(),
        "hidden panes must release reused glyph resources"
    );
}

#[test]
fn semantic_role_cache_tracks_mark_versions_viewport_and_active_input() {
    let mut row = row_with_text_and_cursor(1, "file", 3);
    row.active_input = false;
    let mut view = element(snapshot(0, vec![row]));
    view.semantic_coloring = true;
    view.snapshot.scrollback_lines = 1;
    let mark = CommandMarkAnnotation {
        command_id: "listing".into(),
        start_line: 0,
        command_line: 0,
        command_line_clipped: false,
        output_start_line: 1,
        has_output_start: false,
        output_role: SemanticLineRole::FileListingOutput,
        foldable_output: None,
        end_line: Some(3),
        is_closed: true,
        stale: false,
        exit_code: Some(0),
        finished_at: Some(2),
    };
    view.command_marks = Arc::from([mark]);
    let mut cache = TerminalLayoutCache::default();
    let lines = view.logical_lines_for_rows(0..1);
    for _ in 0..2 {
        assert_eq!(
            view.semantic_roles_for_lines(&lines, Some(&mut cache))[&(0..1)],
            SemanticLineRole::FileListingOutput
        );
    }
    // The mark version can change without any visible cell changing.
    Arc::make_mut(&mut view.command_marks)[0].output_role = SemanticLineRole::GitStatusOutput;
    assert_eq!(
        view.semantic_roles_for_lines(&lines, Some(&mut cache))[&(0..1)],
        SemanticLineRole::GitStatusOutput
    );
    view.snapshot.lines[0].active_input = true;
    assert_eq!(
        view.semantic_roles_for_lines(&lines, Some(&mut cache))[&(0..1)],
        SemanticLineRole::Command
    );
    view.snapshot.lines[0].active_input = false;
    view.snapshot.scrollback_lines = 4;
    assert_eq!(
        view.semantic_roles_for_lines(&lines, Some(&mut cache))[&(0..1)],
        SemanticLineRole::Unknown
    );
    view.snapshot.display_offset = 3;
    assert_eq!(
        view.semantic_roles_for_lines(&lines, Some(&mut cache))[&(0..1)],
        SemanticLineRole::GitStatusOutput
    );
    let retired = Arc::downgrade(&view.command_marks);
    view.command_marks = Arc::from([]);
    assert!(
        retired.upgrade().is_none(),
        "role cache must not retain command annotations"
    );
    assert_eq!(
        view.semantic_roles_for_lines(&lines, Some(&mut cache))[&(0..1)],
        SemanticLineRole::Unknown
    );
}

#[test]
fn cached_foreground_colors_survive_a_mixed_hit_and_miss_frame() {
    use crate::theme::{TerminalHighlightMatchScope, TerminalHighlightRenderMode};
    let rows = (0..2)
        .map(|line| row_with_text_and_cursor(line, "ERROR output", 11))
        .collect();
    let mut snapshot = snapshot(0, rows);
    snapshot.cols = 12;
    let mut view = element(snapshot).highlight_rules(vec![TerminalHighlightRule {
        id: "error".into(),
        pattern: "ERROR".into(),
        foreground: Some("#ff0000".into()),
        render_mode: TerminalHighlightRenderMode::Background,
        match_scope: TerminalHighlightMatchScope::Match,
        preserve_background: true,
        enabled: true,
        ..Default::default()
    }]);
    let mut cache = TerminalLayoutCache::default();
    let first = view.layout_for_rows(0..2, Some(&mut cache));
    let warm = view.layout_for_rows(0..2, Some(&mut cache));
    assert!(Arc::ptr_eq(
        &first.cached_rows[0].layout,
        &warm.cached_rows[0].layout
    ));
    view.snapshot.lines[1].cells_mut()[6].ch = 'X';
    view.snapshot.lines[1].refresh_signature();
    let changed = view.layout_for_rows(0..2, Some(&mut cache));
    assert!(Arc::ptr_eq(
        &warm.cached_rows[0].layout,
        &changed.cached_rows[0].layout
    ));
    assert!(!Arc::ptr_eq(
        &warm.cached_rows[1].layout,
        &changed.cached_rows[1].layout
    ));
    for frame in [&first, &warm, &changed] {
        for row in &frame.cached_rows {
            let colored = row
                .layout
                .text_runs
                .iter()
                .find(|run| run.text.as_ref() == "ERROR")
                .unwrap();
            assert_eq!(colored.style.color, rgb(0xff0000).into());
        }
    }
    for enabled in [false, true, false] {
        let mut rules = view.highlight_rules.to_vec();
        rules[0].enabled = enabled;
        view = view.highlight_rules(rules);
        let layout = view.layout_for_rows(0..2, Some(&mut cache));
        for row in &layout.cached_rows {
            let first_run = &row.layout.text_runs[0];
            assert_eq!(first_run.text, "ERROR");
            assert_eq!(
                first_run.style.color,
                rgb(if enabled {
                    0xff0000
                } else {
                    view.theme.foreground
                })
                .into()
            );
        }
    }
}

#[test]
fn stable_row_reuses_text_cache_but_style_changes_replace_it() {
    let row = row_with_text_and_cursor(0, "cached", 5);
    let stable = element(snapshot(0, vec![row.clone()]));
    let mut cache = TerminalLayoutCache::default();
    let first = stable.layout_for_rows(0..1, Some(&mut cache));
    let second = stable.layout_for_rows(0..1, Some(&mut cache));
    let first_cache = &first.cached_rows[0].layout.text_runs[0].cache;
    let second_cache = &second.cached_rows[0].layout.text_runs[0].cache;
    assert!(Arc::ptr_eq(first_cache, second_cache));

    let mut changed_row = row;
    let cells = Arc::make_mut(&mut changed_row.cells);
    cells[0].fg = TerminalColor::rgb(0xff, 0x00, 0x00);
    changed_row.refresh_signature();
    let changed = element(snapshot(0, vec![changed_row]));
    let changed_layout = changed.layout_for_rows(0..1, Some(&mut cache));
    let changed_cache = &changed_layout.cached_rows[0].layout.text_runs[0].cache;
    assert!(!Arc::ptr_eq(first_cache, changed_cache));

    let retired = Arc::downgrade(first_cache);
    drop(first);
    drop(second);
    drop(changed_layout);
    cache.clear();
    assert!(
        retired.upgrade().is_none(),
        "hidden rows must release retained glyph data"
    );
    let rebuilt = stable.layout_for_rows(0..1, Some(&mut cache));
    assert_eq!(
        rebuilt.cached_rows[0]
            .layout
            .text_runs
            .iter()
            .map(|run| run.text.as_ref())
            .collect::<String>(),
        "cached"
    );
}

#[test]
fn glyph_edge_enhancement_excludes_selection_in_both_paint_layouts() {
    use crate::selection::{TerminalGridPoint, TerminalSelectionMode};

    let mut view = element(snapshot(0, vec![row_with_text_and_cursor(0, "abcde", 4)]));
    view.snapshot.cols = 5;
    view.snapshot.cursor_shape = TerminalCursorShape::Hidden;
    view.selection = Some(TerminalSelection {
        anchor: TerminalGridPoint { line: 0, col: 1 },
        head: TerminalGridPoint { line: 0, col: 2 },
        mode: TerminalSelectionMode::Simple,
    });
    let mut cache = TerminalLayoutCache::default();
    for layout in [
        view.layout_for_rows(0..1, None),
        view.layout_for_rows(0..1, Some(&mut cache)),
    ] {
        assert_eq!(
            layout.glyph_edge_exclusions(1, 5, false),
            vec![vec![false, true, true, false, false]]
        );
    }
}

#[test]
fn selection_uses_theme_color_and_refreshes_cached_rows() {
    use crate::selection::{TerminalGridPoint, TerminalSelectionMode};

    // Each wide glyph has a second, blank grid cell in the backend snapshot.
    let mut row = row_with_text_and_cursor(0, "a中 文 b", 5);
    row.cells_mut()[1].wide = true;
    row.cells_mut()[3].wide = true;
    row.refresh_signature();
    let mut selected = element(snapshot(0, vec![row]));
    selected.snapshot.cols = 6;
    selected.selection = Some(TerminalSelection {
        anchor: TerminalGridPoint { line: 0, col: 0 },
        head: TerminalGridPoint { line: 0, col: 5 },
        mode: TerminalSelectionMode::Simple,
    });
    let mut tokens =
        oxideterm_theme::ThemeTokens::from_builtin(oxideterm_theme::theme_by_id("paper-oxide"));
    let mut cache = TerminalLayoutCache::default();
    for (color, expected) in [
        (
            tokens.terminal.selection_background,
            rgb(0x8f6251).alpha(0.2),
        ),
        ("#123456", rgb(0x123456)),
        ("#12345680", rgba(0x12345680)),
        ("rgb(17, 34, 51)", rgb(0x112233)),
    ] {
        tokens.terminal.selection_background = color;
        selected.theme = TerminalUiTheme::from_tokens(tokens);
        let layout = selected.layout_for_rows(0..1, Some(&mut cache));
        let rects = &layout.cached_rows[0].layout.selections;
        assert_eq!(
            rects
                .iter()
                .map(|rect| (rect.col, rect.cells))
                .collect::<Vec<_>>(),
            vec![(0, 6)],
            "{color}: each selected grid cell must be painted once"
        );
        assert_eq!(rects[0].color, Hsla::from(expected), "{color}");
    }
    for (start, end, expected) in [(0, 1, (0, 3)), (2, 5, (2, 4))] {
        selected.selection = Some(TerminalSelection {
            anchor: TerminalGridPoint {
                line: 0,
                col: start,
            },
            head: TerminalGridPoint { line: 0, col: end },
            mode: TerminalSelectionMode::Simple,
        });
        let layout = selected.layout_for_rows(0..1, Some(&mut cache));
        assert_eq!(
            layout.cached_rows[0]
                .layout
                .selections
                .iter()
                .map(|rect| (rect.col, rect.cells))
                .collect::<Vec<_>>(),
            vec![expected]
        );
    }
}

#[test]
fn powerline_background_toggle_refreshes_cached_shapes_and_preserves_selection_and_cursor() {
    use crate::selection::{TerminalGridPoint, TerminalSelectionMode};
    use oxideterm_terminal_emulator::TerminalStyleOrigin;

    // Include RTL text to exercise visual ordering as well as the ordinary row path.
    for bidi in [false, true] {
        let mut row = row_with_text_and_cursor(0, "\u{e0b0}\u{e0b0}א", 2);
        for cell in &mut row.cells_mut()[..2] {
            cell.fg = terminal_color_from_hex(TerminalUiTheme::default().background);
            cell.bg = TerminalColor::rgb(0, 128, 128);
            cell.style_origin = TerminalStyleOrigin::new(true, true);
        }
        row.cells_mut()[0].style_origin = row.cells[0]
            .style_origin
            .with_foreground_default_background();
        row.refresh_signature();
        let mut view = element(snapshot(0, vec![row]));
        view.snapshot.cols = 3;
        view.snapshot.cursor_col = 2;
        view.bidi_enabled = bidi;
        view.selection = Some(TerminalSelection {
            anchor: TerminalGridPoint { line: 0, col: 0 },
            head: TerminalGridPoint { line: 0, col: 2 },
            mode: TerminalSelectionMode::Simple,
        });
        let mut cache = TerminalLayoutCache::default();
        for transparent in [false, true, false, true] {
            view.transparent_background = transparent;
            let layout = view.layout_for_rows(0..1, Some(&mut cache));
            let row = &layout.cached_rows[0].layout;
            assert_eq!(
                row.backgrounds
                    .iter()
                    .filter_map(|rect| rect.powerline_cutout)
                    .collect::<Vec<_>>(),
                if transparent {
                    vec!['\u{e0b0}']
                } else {
                    vec![]
                }
            );
            assert_eq!(
                row.text_runs
                    .iter()
                    .flat_map(|run| run.text.chars())
                    .filter(|ch| *ch == '\u{e0b0}')
                    .count(),
                if transparent { 1 } else { 2 },
                "explicit RGB must remain a painted glyph"
            );
            assert_eq!(
                row.selections
                    .iter()
                    .map(|rect| (rect.col, rect.cells, rect.color))
                    .collect::<Vec<_>>(),
                vec![(0, 3, view.theme.selection_background)]
            );
        }

        view.snapshot.lines[0].cells_mut()[2].cursor = false;
        view.snapshot.lines[0].cells_mut()[0].cursor = true;
        view.snapshot.lines[0].refresh_signature();
        view.snapshot.cursor_col = 0;
        let layout = view.layout_for_rows(0..1, Some(&mut cache));
        let row = &layout.cached_rows[0].layout;
        assert!(
            row.backgrounds
                .iter()
                .all(|rect| rect.powerline_cutout.is_none())
        );
        let cursor = row.cursor.expect("block cursor");
        assert_eq!(
            row.backgrounds
                .iter()
                .find(|rect| rect.col == cursor.col)
                .unwrap()
                .color,
            to_hsla(terminal_color_from_hex(view.theme.header_foreground))
        );
    }
}

#[test]
fn transparent_vim_row_contains_only_current_text_and_cursor() {
    let mut snapshot = snapshot(0, vec![row_with_text_and_cursor(0, "846", 2)]);
    snapshot.cols = 3;
    snapshot.cursor_col = 2;
    snapshot.cursor_shape = TerminalCursorShape::Bar;
    let layout = element(snapshot).transparent_background(true).layout();

    assert!(layout.backgrounds.is_empty());
    assert_eq!(layout.text_runs.len(), 1);
    assert_eq!(layout.text_runs[0].text, "846");
    assert_eq!(layout.text_runs[0].col, 0);
    assert_eq!(layout.text_runs[0].cells, 3);
    assert_eq!(
        layout
            .cursor
            .map(|cursor| (cursor.row, cursor.col, cursor.shape)),
        Some((0, 2, TerminalCursorShape::Bar))
    );
}

#[test]
fn theme_transition_interpolates_default_and_ansi_colors_while_reusing_text_run_cache() {
    use oxideterm_terminal_emulator::TerminalStyleOrigin;

    let dark = TerminalUiTheme::from_tokens(oxideterm_theme::ThemeTokens::from_builtin(
        oxideterm_theme::theme_by_id("default"),
    ));
    let light = TerminalUiTheme::from_tokens(oxideterm_theme::ThemeTokens::from_builtin(
        oxideterm_theme::theme_by_id("solarized-light"),
    ));
    let mut row = row_with_text_and_cursor(1, "ab", 1);
    row.cells_mut()[0].fg = terminal_color_from_hex(light.foreground);
    row.cells_mut()[0].bg = terminal_color_from_hex(light.background);
    row.cells_mut()[0].style_origin = TerminalStyleOrigin::new(false, false);
    row.cells_mut()[1].fg = terminal_color_from_hex(light.tokens.terminal.red);
    row.cells_mut()[1].bg = terminal_color_from_hex(light.background);
    row.cells_mut()[1].style_origin = TerminalStyleOrigin::new(true, false);
    row.refresh_signature();
    let snap = snapshot(0, vec![row]);

    let mid_theme = TerminalUiTheme::interpolate(&dark, &light, 0.5);
    let mid_colors = TerminalTransitionColors::new(&dark, &light, &mid_theme, 0.5);
    let quarter_theme = TerminalUiTheme::interpolate(&dark, &light, 0.25);
    let quarter_colors = TerminalTransitionColors::new(&dark, &light, &quarter_theme, 0.25);

    let mid_live_bg = terminal_background(&mid_theme);
    let quarter_live_bg = terminal_background(&quarter_theme);
    let mut cache = TerminalLayoutCache::default();
    let mut view = element(snap.clone());
    view.cursor_visible = false;
    view.theme = mid_theme.clone();
    view.transition_colors = Some(mid_colors);
    let first_layout = view.layout_for_rows(0..1, Some(&mut cache));
    let first_row = &first_layout.cached_rows[0].layout;
    assert!(
        first_row.backgrounds.is_empty(),
        "default background cells must not emit target-colored rects mid-transition"
    );
    assert_eq!(
        mid_colors.resolve_row_color(
            first_row.text_runs[0].transition_key,
            first_row.text_runs[0].style.color,
            mid_live_bg,
        ),
        mid_colors.interpolated_foreground
    );
    assert_eq!(
        mid_colors.resolve_row_color(
            first_row.text_runs[1].transition_key,
            first_row.text_runs[1].style.color,
            mid_live_bg,
        ),
        mid_colors
            .map_ansi_color(terminal_color_from_hex(light.tokens.terminal.red))
            .unwrap()
    );

    let mut next_view = element(snap.clone());
    next_view.cursor_visible = false;
    next_view.theme = quarter_theme.clone();
    next_view.transition_colors = Some(quarter_colors);
    let second_layout = next_view.layout_for_rows(0..1, Some(&mut cache));
    let second_row = &second_layout.cached_rows[0].layout;
    assert!(
        Arc::ptr_eq(first_row, second_row),
        "row layout cache must stay warm across transition progress updates"
    );
    assert!(Arc::ptr_eq(
        &first_row.text_runs[0].cache,
        &second_row.text_runs[0].cache
    ));

    let mut semantic_row = row_with_text_and_cursor(2, "failed", 0);
    semantic_row.line_id = 7;
    semantic_row.cells_mut()[0].cursor = false;
    for cell in semantic_row.cells_mut() {
        cell.fg = terminal_color_from_hex(light.foreground);
        cell.bg = terminal_color_from_hex(light.background);
        cell.style_origin = TerminalStyleOrigin::new(false, false);
    }
    semantic_row.refresh_signature();
    let semantic_snap = snapshot(0, vec![semantic_row]);
    let scheme = Arc::new(compiled_builtin_scheme(SemanticScheme::Balanced).clone());
    let mut sem_first = element(semantic_snap.clone())
        .semantic_configuration(true, scheme.clone(), SemanticShellDialect::Auto)
        .theme_transition_colors(Some(mid_colors));
    sem_first.cursor_visible = false;
    sem_first.theme = mid_theme.clone();
    let sem_layout_first = sem_first.layout_for_rows(0..1, Some(&mut cache));
    let mut sem_second = element(semantic_snap)
        .theme_transition_colors(Some(quarter_colors))
        .semantic_configuration(true, scheme, SemanticShellDialect::Auto);
    sem_second.cursor_visible = false;
    sem_second.theme = quarter_theme;
    let sem_layout_second = sem_second.layout_for_rows(0..1, Some(&mut cache));
    assert!(
        Arc::ptr_eq(
            &sem_layout_first.cached_rows[0].layout,
            &sem_layout_second.cached_rows[0].layout
        ),
        "semantic-colored rows must stay cache-warm across transition progress updates"
    );
    let sem_run = &sem_layout_first.cached_rows[0].layout.text_runs[0];
    let expected_semantic_ansi = terminal_color_from_hex(light.tokens.terminal.green);
    assert_eq!(
        mid_colors.resolve_row_color(sem_run.transition_key, sem_run.style.color, mid_live_bg),
        mid_colors.map_ansi_color(expected_semantic_ansi).unwrap()
    );
    let first_live_fg = mid_colors.resolve_row_color(
        first_row.text_runs[0].transition_key,
        first_row.text_runs[0].style.color,
        mid_live_bg,
    );
    let second_live_fg = quarter_colors.resolve_row_color(
        second_row.text_runs[0].transition_key,
        second_row.text_runs[0].style.color,
        quarter_live_bg,
    );
    assert_ne!(first_live_fg, second_live_fg);

    let mut uncached_view = element(snap);
    uncached_view.cursor_visible = false;
    uncached_view.theme = mid_theme;
    uncached_view.transition_colors = Some(mid_colors);
    let uncached_layout = uncached_view.layout_for_rows(0..1, None);
    assert_eq!(
        uncached_layout.text_runs[0].style.color,
        mid_colors.interpolated_foreground
    );
}

#[test]
fn frame_layout_cache_reuses_unchanged_frames_and_invalidates_on_layout_changes() {
    use crate::selection::{TerminalGridPoint, TerminalSelectionMode};

    let mut row0 = row_with_text_and_cursor(11, "alpha", 4);
    row0.source_id = 101;
    let mut row1 = row_with_text_and_cursor(12, "beta", 0);
    row1.source_id = 102;
    row1.cells_mut()[0].cursor = false;
    row1.refresh_signature();
    let mut snap = snapshot(0, vec![row0, row1]);
    snap.cursor_row = 0;
    snap.cursor_col = 4;
    snap.cursor_shape = TerminalCursorShape::Block;

    let cache = Rc::new(RefCell::new(TerminalLayoutCache::default()));
    let mut timestamps = TerminalRowTimestampStore::default();
    timestamps.insert_label(101, "[12:00:00.000]");
    timestamps.insert_label(102, "[12:00:00.000]");

    let mut view = element(snap)
        .layout_cache(Rc::clone(&cache))
        .row_timestamps(Some(Arc::new(timestamps.clone())))
        .scroll_y_offset(px(3.5))
        .performance_metrics_enabled(true);
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(320.0), px(120.0)));

    let first = view.cached_layout_for_bounds(bounds);
    // Paint-only changes (sub-row scroll animation, horizontal offset, scrollbar visibility)
    // must reuse the exact same whole-frame layout without rebuilding.
    view.scroll_y_offset = px(1.25);
    view.scroll_x_offset = px(12.0);
    view.scrollbar_visible = true;
    view.scrollbar_active = true;
    let second = view.cached_layout_for_bounds(bounds);
    assert!(Arc::ptr_eq(&first, &second));
    assert!(cache.borrow_mut().performance().cache_hit_percent > 0);

    // Cursor blink toggles layout cursor presence and must invalidate the frame cache.
    view.cursor_visible = false;
    let blinked = view.cached_layout_for_bounds(bounds);
    assert!(!Arc::ptr_eq(&second, &blinked));
    assert_eq!(blinked.cursor, None);

    // Selection change must invalidate the frame cache and emit the new selection rect.
    view.selection = Some(TerminalSelection {
        anchor: TerminalGridPoint { line: 0, col: 0 },
        head: TerminalGridPoint { line: 0, col: 3 },
        mode: TerminalSelectionMode::Simple,
    });
    let selected = view.cached_layout_for_bounds(bounds);
    assert!(!Arc::ptr_eq(&blinked, &selected));
    assert!(
        selected
            .cached_rows
            .iter()
            .any(|row| !row.layout.selections.is_empty())
    );

    // Timestamp label change on a visible row must invalidate the frame cache.
    timestamps.insert_label(102, "[12:00:01.250]");
    view.row_timestamps = Some(Arc::new(timestamps));
    let retimestamped = view.cached_layout_for_bounds(bounds);
    assert!(!Arc::ptr_eq(&selected, &retimestamped));
    assert_eq!(
        retimestamped
            .timestamp_runs
            .iter()
            .map(|run| run.text.as_ref())
            .collect::<Vec<_>>(),
        vec!["[12:00:00.000]", "[12:00:01.250]"]
    );

    // Visible row content change must invalidate even when snapshot generation is unchanged.
    view.snapshot.lines[0].cells_mut()[0].ch = 'Z';
    view.snapshot.lines[0].refresh_signature();
    let edited = view.cached_layout_for_bounds(bounds);
    assert!(!Arc::ptr_eq(&retimestamped, &edited));
    assert_eq!(
        edited.cached_rows[0].layout.text_runs[0].text.as_ref(),
        "Zlpha"
    );

    cache.borrow_mut().clear();
    let after_clear = view.cached_layout_for_bounds(bounds);
    assert!(!Arc::ptr_eq(&edited, &after_clear));
}

#[test]
fn timestamp_runs_share_text_run_cache_across_matching_labels_and_frames() {
    let mut row0 = row_with_text_and_cursor(21, "line0", 0);
    row0.source_id = 201;
    row0.cells_mut()[0].cursor = false;
    row0.refresh_signature();
    let mut row1 = row_with_text_and_cursor(22, "line1", 0);
    row1.source_id = 202;
    row1.cells_mut()[0].cursor = false;
    row1.refresh_signature();
    let mut row2 = row_with_text_and_cursor(23, "line2", 0);
    row2.source_id = 203;
    row2.cells_mut()[0].cursor = false;
    row2.refresh_signature();

    let mut timestamps = TerminalRowTimestampStore::default();
    timestamps.insert_label(201, "[12:34:56.789]");
    timestamps.insert_label(202, "[12:34:56.789]");
    timestamps.insert_label(203, "[12:34:57.000]");

    let mut cache = TerminalLayoutCache::default();
    let mut view =
        element(snapshot(0, vec![row0, row1, row2])).row_timestamps(Some(Arc::new(timestamps)));

    let first = view.layout_for_rows(0..3, Some(&mut cache));
    assert_eq!(first.timestamp_runs.len(), 3);
    let cache0 = first.timestamp_runs[0]
        .cache
        .as_ref()
        .expect("cached timestamp layout");
    let cache1 = first.timestamp_runs[1]
        .cache
        .as_ref()
        .expect("cached timestamp layout");
    let cache2 = first.timestamp_runs[2]
        .cache
        .as_ref()
        .expect("cached timestamp layout");
    assert!(
        Arc::ptr_eq(cache0, cache1),
        "rows with the same timestamp label must share shaped LineLayout cache"
    );
    assert!(!Arc::ptr_eq(cache0, cache2));

    let second = view.layout_for_rows(0..3, Some(&mut cache));
    assert!(Arc::ptr_eq(
        cache0,
        second.timestamp_runs[0].cache.as_ref().unwrap()
    ));

    // Changing timestamp color must produce a distinct cache entry rather than reusing stale style.
    view.theme.header_foreground ^= 0x0010_1010;
    let restyled = view.layout_for_rows(0..3, Some(&mut cache));
    assert!(!Arc::ptr_eq(
        cache0,
        restyled.timestamp_runs[0].cache.as_ref().unwrap()
    ));
}
