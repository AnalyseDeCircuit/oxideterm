use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use alacritty_terminal::{event::VoidListener, index::Column};

use super::*;
use crate::TerminalColor;
use alacritty_terminal::term::Config;

#[test]
fn snapshot_keeps_wrap_on_a_wide_spacer_before_the_default_tail() {
    let size = TerminalSize {
        cols: 8,
        rows: 1,
        cell_width: 8,
        cell_height: 16,
    };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let mut parser = Processor::<StdSyncHandler>::new();
    parser.advance(&mut term, "界".as_bytes());
    term.grid_mut()[Line(0)]
        .cell_mut(Column(1))
        .flags
        .insert(Flags::WRAPLINE);
    let row = snapshot_row_from_term(&term, size, &TerminalPalette::default(), 0, 0);
    assert!(row.wrapped);
    assert!(row.cells[0].wide);
    assert_eq!(row.text(), "界       ");
}

#[test]
fn predecessor_wrap_changes_invalidate_an_unchanged_logical_line() {
    let size = TerminalSize {
        cols: 10,
        rows: 4,
        cell_width: 8,
        cell_height: 16,
    };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let mut parser = Processor::<StdSyncHandler>::new();
    parser.advance(&mut term, b"prefix\r\n'abcdefghijklmno'\r\nend");
    let graphics = TerminalGraphicsState::default();
    let palette = TerminalPalette::default();
    let before = snapshot_from_term(&term, size, &graphics, &palette);
    assert_eq!(
        before.logical_context[0].text.trim_end(),
        "'abcdefghijklmno'"
    );
    term.grid_mut()[Line(0)]
        .cell_mut(Column(9))
        .flags
        .insert(Flags::WRAPLINE);
    let after = snapshot_from_term_with_display_offset(
        &term,
        size,
        &graphics,
        &palette,
        0,
        4,
        Some(&before),
    );
    assert_eq!(
        after.logical_context[0].text.trim_end(),
        "prefix    'abcdefghijklmno'"
    );
    assert!(!Arc::ptr_eq(
        &before.logical_context[0].text,
        &after.logical_context[0].text
    ));
    assert_eq!(
        before.logical_context[0].members[0].revision,
        after.logical_context[0].members[1].revision
    );
}

fn assert_snapshot_content_eq(actual: &TerminalSnapshot, expected: &TerminalSnapshot) {
    assert_eq!(actual.cols, expected.cols);
    assert_eq!(actual.rows, expected.rows);
    assert_eq!(actual.cursor_col, expected.cursor_col);
    assert_eq!(actual.cursor_row, expected.cursor_row);
    assert_eq!(actual.cursor_shape, expected.cursor_shape);
    assert_eq!(actual.display_offset, expected.display_offset);
    assert_eq!(actual.scrollback_lines, expected.scrollback_lines);
    assert_eq!(actual.lines.len(), expected.lines.len());
    for (actual, expected) in actual.lines.iter().zip(&expected.lines) {
        assert_eq!(actual.source_id, expected.source_id);
        assert_eq!(actual.absolute_line, expected.absolute_line);
        assert_eq!(actual.cells, expected.cells);
        assert_eq!(actual.wrapped, expected.wrapped);
        assert_eq!(actual.active_input, expected.active_input);
        assert_eq!(actual.signature, expected.signature);
    }
    assert_eq!(actual.images, expected.images);
}

#[test]
fn snapshot_preserves_powerline_default_background_through_reverse_video() {
    let size = TerminalSize {
        cols: 4,
        rows: 1,
        cell_width: 0,
        cell_height: 0,
    };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let mut parser = Processor::<StdSyncHandler>::new();
    // Oh My Posh's transparent foreground uses SGR 49 followed by SGR 7.
    // The second cell explicitly requests the same RGB and must remain opaque.
    parser.advance(
        &mut term,
        "\x1b[0m\x1b[36;49m\x1b[7m\u{e0b0}\x1b[0m\x1b[36;48;2;13;15;18m\x1b[7m\u{e0b0}\x1b[27m"
            .as_bytes(),
    );
    let snapshot = snapshot_from_term(
        &term,
        size,
        &TerminalGraphicsState::default(),
        &TerminalPalette::default(),
    );
    let cells = &snapshot.lines[0].cells;
    for cell in &cells[..2] {
        assert_eq!(
            (cell.ch, cell.fg, cell.bg),
            (
                '\u{e0b0}',
                TerminalColor::rgb(13, 15, 18),
                TerminalColor::rgb(0x56, 0xc7, 0xda),
            )
        );
    }
    assert!(cells[0].style_origin.foreground_is_default_background());
    assert!(!cells[1].style_origin.foreground_is_default_background());
    assert!(!cells[2].style_origin.foreground_is_default_background());
}

#[test]
fn stamped_snapshot_consumes_scroll_damage_for_next_incremental_render() {
    // The plugin local-display stamp path takes a full snapshot after the
    // grid scrolled and clears term damage under the same lock. The next
    // render pass must treat the stamped snapshot as an exact baseline: an
    // incremental rebuild without new output has to reproduce it instead of
    // applying the stale scroll a second time, which duplicated the freshly
    // rendered bottom rows on screen.
    let size = TerminalSize {
        cols: 12,
        rows: 6,
        cell_width: 0,
        cell_height: 0,
    };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    // A fresh term starts fully damaged; production has render cycles
    // before the feed, so consume the initial damage to reach the same
    // incremental ScrollUp state the bug needs.
    term.reset_damage();
    let mut parser = Processor::<StdSyncHandler>::new();
    // Fill the screen with the cursor walking down every row, then reset
    // damage like a production render cycle would. Text writes do not
    // damage their row; only cursor motion does, so this keeps the later
    // burst's damage limited to the bottom row.
    for index in 0..5 {
        parser.advance(&mut term, format!("row-{index}\r\n").as_bytes());
    }
    term.reset_damage();
    // The burst scrolls three lines in while the cursor never leaves the
    // bottom row, so the pending damage carries a scroll with a narrow
    // dirty set — exactly the state the stamp path must consume.
    for index in 5..8 {
        parser.advance(&mut term, format!("row-{index}\r\n").as_bytes());
    }
    let graphics = TerminalGraphicsState::default();
    let palette = TerminalPalette::default();
    let stamped = snapshot_from_term_with_display_offset(
        &term, size, &graphics, &palette, 0, size.rows, None,
    );
    // Independent content expectations: the burst scrolled three rows into
    // scrollback, leaving the fill lines shifted up and the burst rows at
    // the bottom below the still-empty cursor line.
    let rendered: Vec<String> = stamped
        .lines
        .iter()
        .map(|line| line.text().trim_end().to_owned())
        .collect();
    assert_eq!(
        rendered,
        vec!["row-3", "row-4", "row-5", "row-6", "row-7", ""]
    );
    term.reset_damage();
    let rebuilt = incremental_snapshot_from_term(&mut term, size, &graphics, &palette, &stamped);
    assert_snapshot_content_eq(&rebuilt, &stamped);
}

#[test]
fn snapshot_style_runs_preserve_contrast_attributes_and_link_boundaries() {
    let size = TerminalSize {
        cols: 100,
        rows: 3,
        cell_width: 0,
        cell_height: 0,
    };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let graphics = TerminalGraphicsState::default();
    let mut parser = Processor::<StdSyncHandler>::new();
    parser.advance(&mut term, "\x1b[37;107mab─cd\x1b[2mef\x1b[7mgh\x1b[0mij\x1b[38;2;1;2;3mkl\x1b[38;5;196mmn\x1b[0m\r\n\x1b]8;;https://one.example\x1b\\ABe\u{301}C\x1b]8;;\x1b\\D\x1b]8;;https://two.example\x1b\\EF\x1b]8;;\x1b\\".as_bytes());
    let snapshot = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());
    for (source, actual) in term.grid()[Line(0)]
        .iter()
        .take(16)
        .zip(snapshot.lines[0].cells.iter())
    {
        let attrs = attrs_from_flags(source.flags);
        assert_eq!(
            (actual.fg, actual.bg),
            style_colors_for_cell(
                source.fg,
                source.bg,
                source.c,
                attrs,
                &TerminalPalette::default(),
                term.colors(),
            )
        );
        assert_eq!(actual.attrs, attrs);
        assert_eq!(
            actual.style_origin,
            style_origin_for_cell(source.fg, source.bg, attrs)
        );
    }
    let colors = &snapshot.lines[0].cells;
    assert_ne!(
        colors[0].fg, colors[2].fg,
        "decoration glyphs bypass contrast adjustment"
    );
    assert_eq!(colors[2].fg, TerminalColor::rgb(0xd6, 0xda, 0xdf));
    assert_eq!(colors[12].fg, TerminalColor::rgb(1, 2, 3));
    let linked = &snapshot.lines[1].cells;
    assert_eq!(snapshot.lines[1].text().trim_end(), "ABe\u{301}CDEF");
    assert_eq!(
        linked[..7]
            .iter()
            .map(TerminalCell::hyperlink)
            .collect::<Vec<_>>(),
        vec![
            Some("https://one.example"),
            Some("https://one.example"),
            Some("https://one.example"),
            Some("https://one.example"),
            None,
            Some("https://two.example"),
            Some("https://two.example"),
        ]
    );
    assert_eq!(
        linked[..7]
            .iter()
            .map(TerminalCell::zerowidth)
            .collect::<Vec<_>>(),
        vec!["", "", "\u{301}", "", "", "", ""]
    );
    assert!(Arc::ptr_eq(
        linked[0].extra.as_ref().unwrap(),
        linked[1].extra.as_ref().unwrap()
    ));
    parser.advance(
        &mut term,
        b"\x1b[2;1H\x1b]8;;https://changed.example\x1b\\Z\x1b]8;;\x1b\\",
    );
    let next = incremental_snapshot_from_term(
        &mut term,
        size,
        &graphics,
        &TerminalPalette::default(),
        &snapshot,
    );
    assert_eq!(
        next.lines[1].cells[0].hyperlink(),
        Some("https://changed.example")
    );
    assert_eq!(linked[0].hyperlink(), Some("https://one.example"));
    let mut expected = linked[1].clone();
    expected.cursor = true;
    assert_eq!(next.lines[1].cells[1], expected);

    parser.advance(
        &mut term,
        b"\x1b[2;1H\x1b[4:1m1\x1b[4:2m2\x1b[4:3;58:2::255:80:80m34\x1b[59;4:4m5\x1b[4:5m6\x1b[0m",
    );
    let underlined = incremental_snapshot_from_term(
        &mut term,
        size,
        &graphics,
        &TerminalPalette::default(),
        &next,
    );
    let ul_cells = &underlined.lines[1].cells;
    assert_eq!(
        ul_cells[..6]
            .iter()
            .map(|c| c.attrs.underline_style())
            .collect::<Vec<_>>(),
        vec![
            crate::TerminalUnderlineStyle::Single,
            crate::TerminalUnderlineStyle::Double,
            crate::TerminalUnderlineStyle::Curly,
            crate::TerminalUnderlineStyle::Curly,
            crate::TerminalUnderlineStyle::Dotted,
            crate::TerminalUnderlineStyle::Dashed,
        ]
    );
    assert_eq!(ul_cells[0].underline_color(), None);
    assert_eq!(
        ul_cells[2].underline_color(),
        Some(TerminalColor::rgb(255, 80, 80))
    );
    assert_eq!(
        ul_cells[3].underline_color(),
        Some(TerminalColor::rgb(255, 80, 80))
    );
    assert_eq!(ul_cells[4].underline_color(), None);
    assert!(Arc::ptr_eq(
        ul_cells[2].extra.as_ref().unwrap(),
        ul_cells[3].extra.as_ref().unwrap()
    ));
}

#[test]
fn incremental_snapshot_reuses_undamaged_rows() {
    let size = TerminalSize {
        cols: 8,
        rows: 3,
        cell_width: 0,
        cell_height: 0,
    };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let graphics = TerminalGraphicsState::default();
    let previous = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());
    term.reset_damage();

    let mut parser = Processor::<StdSyncHandler>::new();
    parser.advance(&mut term, b"changed");
    let next = incremental_snapshot_from_term(
        &mut term,
        size,
        &graphics,
        &TerminalPalette::default(),
        &previous,
    );
    let full = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());

    assert!(!Arc::ptr_eq(&previous.lines[0].cells, &next.lines[0].cells));
    assert!(Arc::ptr_eq(&previous.lines[2].cells, &next.lines[2].cells));
    assert_snapshot_content_eq(&next, &full);
}

#[test]
fn snapshot_source_identity_follows_output_rows_during_scroll() {
    let size = TerminalSize {
        cols: 8,
        rows: 3,
        cell_width: 0,
        cell_height: 0,
    };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let graphics = TerminalGraphicsState::default();
    let mut parser = Processor::<StdSyncHandler>::new();
    parser.advance(&mut term, b"one\r\ntwo\r\nthree\r\nfour");
    let previous = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());

    parser.advance(&mut term, b"\r\nfive");
    let next = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());

    assert_eq!(next.lines[0].source_id, previous.lines[1].source_id);
    assert_eq!(next.lines[1].source_id, previous.lines[2].source_id);
}

#[test]
fn incremental_snapshot_reuses_unchanged_rows_after_output_scroll() {
    let size = TerminalSize {
        cols: 16,
        rows: 3,
        cell_width: 0,
        cell_height: 0,
    };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let graphics = TerminalGraphicsState::default();
    let mut parser = Processor::<StdSyncHandler>::new();
    parser.advance(&mut term, b"one\r\ntwo\r\nprompt");
    let initial = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());
    // Consume startup full damage so this assertion covers the steady output path.
    let previous = incremental_snapshot_from_term(
        &mut term,
        size,
        &graphics,
        &TerminalPalette::default(),
        &initial,
    );

    parser.advance(&mut term, b"-completed\r\nnext");
    let next = incremental_snapshot_from_term(
        &mut term,
        size,
        &graphics,
        &TerminalPalette::default(),
        &previous,
    );
    let full = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());

    assert!(Arc::ptr_eq(&previous.lines[1].cells, &next.lines[0].cells));
    assert!(!Arc::ptr_eq(&previous.lines[2].cells, &next.lines[1].cells));
    assert_snapshot_content_eq(&next, &full);
}

#[test]
fn incremental_snapshot_preserves_rows_across_multiple_output_scrolls() {
    let size = TerminalSize {
        cols: 16,
        rows: 5,
        cell_width: 0,
        cell_height: 0,
    };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let graphics = TerminalGraphicsState::default();
    let mut parser = Processor::<StdSyncHandler>::new();
    parser.advance(&mut term, b"zero\r\none\r\ntwo\r\nthree\r\nprompt");
    let initial = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());
    let previous = incremental_snapshot_from_term(
        &mut term,
        size,
        &graphics,
        &TerminalPalette::default(),
        &initial,
    );

    parser.advance(&mut term, b"-completed\r\nnext-a\r\nnext-b");
    let next = incremental_snapshot_from_term(
        &mut term,
        size,
        &graphics,
        &TerminalPalette::default(),
        &previous,
    );
    let full = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());

    assert!(Arc::ptr_eq(&previous.lines[2].cells, &next.lines[0].cells));
    assert!(Arc::ptr_eq(&previous.lines[3].cells, &next.lines[1].cells));
    assert!(next.lines[2].text().starts_with("prompt-completed"));
    assert_snapshot_content_eq(&next, &full);
}

#[test]
fn scroll_snapshot_reuses_overlapping_viewport_rows() {
    let size = TerminalSize {
        cols: 8,
        rows: 3,
        cell_width: 0,
        cell_height: 0,
    };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let graphics = TerminalGraphicsState::default();
    let mut parser = Processor::<StdSyncHandler>::new();
    parser.advance(&mut term, b"one\r\ntwo\r\nthree\r\nfour");
    let previous = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());
    // This fixture represents an already painted frame, with its damage consumed.
    let _ = term.damage();
    term.reset_damage();

    let next = scroll_snapshot_from_term(
        &mut term,
        size,
        &graphics,
        &TerminalPalette::default(),
        1,
        &previous,
    );
    let full = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());

    assert!(Arc::ptr_eq(&previous.lines[0].cells, &next.lines[1].cells));
    assert_snapshot_content_eq(&next, &full);
}

#[test]
fn incremental_snapshot_clears_the_previous_cursor_cell() {
    let size = TerminalSize {
        cols: 8,
        rows: 3,
        cell_width: 0,
        cell_height: 0,
    };
    let mut term = Term::new(Config::default(), &size, VoidListener);
    let graphics = TerminalGraphicsState::default();
    let previous = snapshot_from_term(&term, size, &graphics, &TerminalPalette::default());
    term.reset_damage();

    let mut parser = Processor::<StdSyncHandler>::new();
    parser.advance(&mut term, b"\r\n");
    let next = incremental_snapshot_from_term(
        &mut term,
        size,
        &graphics,
        &TerminalPalette::default(),
        &previous,
    );

    assert!(!next.lines[0].cells[0].cursor);
    assert!(next.lines[1].cells[0].cursor);
    // Publishing the cursor move must neither alter the old frame nor leave a stale cache key.
    assert!(previous.lines[0].cells[0].cursor);
    assert!(!previous.lines[1].cells[0].cursor);
    assert_ne!(next.lines[0].signature, previous.lines[0].signature);
    assert_ne!(next.lines[1].signature, previous.lines[1].signature);
    assert_snapshot_content_eq(
        &next,
        &snapshot_from_term(&term, size, &graphics, &TerminalPalette::default()),
    );
}
