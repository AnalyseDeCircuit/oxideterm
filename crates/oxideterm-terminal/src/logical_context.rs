// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    TerminalLogicalContext, TerminalLogicalContextRow, TerminalLogicalMember, TerminalSnapshot,
};
use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    index::Line,
    term::{Term, cell::Flags},
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

pub(crate) fn capture<T: EventListener>(term: &Term<T>, snapshot: &mut TerminalSnapshot) {
    let history = term.grid().history_size() as i32;
    let screen_end = term.screen_lines() as i32;
    let visible_start = -(snapshot.display_offset as i32);
    let visible_end = (visible_start + snapshot.lines.len() as i32).min(screen_end);
    let wraps = |line: i32| {
        term.grid()[Line(line)]
            .last()
            .is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE))
    };
    let previous = std::mem::take(&mut snapshot.logical_context);
    let mut contexts = Vec::new();
    let mut current = visible_start;
    while current < visible_end {
        if !wraps(current) && (current == -history || !wraps(current - 1)) {
            current += 1;
            continue;
        }
        let mut start = current;
        while start > -history && wraps(start - 1) {
            start -= 1;
        }
        let mut end = current + 1;
        while end < screen_end && wraps(end - 1) {
            end += 1;
        }
        let start_at_history_boundary = start == -history;
        // Validate the current boundaries and every member, including off-screen rows.
        // Display coordinates and cursor roles are intentionally not reused.
        let reusable = previous.iter().find(|context| {
            context.cols == snapshot.cols
                && context.start_at_history_boundary == start_at_history_boundary
                && context.members.len() == (end - start) as usize
                && context
                    .members
                    .iter()
                    .zip(start..end)
                    .all(|(member, line)| {
                        let row = &term.grid()[Line(line)];
                        member.source_id == row.identity() && member.revision == row.revision()
                    })
        });
        let (text, members, signature) = if let Some(context) = reusable {
            (
                context.text.clone(),
                context.members.clone(),
                context.signature,
            )
        } else {
            // Snapshot-owned text is zeroized when the last reader releases it.
            let mut text = zeroize::Zeroizing::new(String::new());
            let mut members = Vec::with_capacity((end - start) as usize);
            for line in start..end {
                let row = &term.grid()[Line(line)];
                members.push(TerminalLogicalMember {
                    source_id: row.identity(),
                    revision: row.revision(),
                    byte_start: text.len(),
                });
                for cell in row.iter().take(snapshot.cols) {
                    if cell
                        .flags
                        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                    {
                        continue;
                    }
                    text.push(if cell.c == '\0' { ' ' } else { cell.c });
                    if let Some(extra) = cell.zerowidth() {
                        text.extend(extra.iter().copied());
                    }
                }
            }
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            start_at_history_boundary.hash(&mut hasher);
            (Arc::new(text), Arc::new(members), hasher.finish())
        };
        let rows = (current..end.min(visible_end))
            .map(|line| {
                let row = &term.grid()[Line(line)];
                let columns = reusable
                    .and_then(|context| {
                        context
                            .rows
                            .iter()
                            .find(|mapped| mapped.source_id == row.identity())
                    })
                    .map(|mapped| mapped.columns.clone())
                    .unwrap_or_else(|| {
                        let mut byte = members[(line - start) as usize].byte_start;
                        let mut columns = Vec::new();
                        for (col, cell) in row.iter().enumerate().take(snapshot.cols) {
                            if cell.flags.intersects(
                                Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER,
                            ) {
                                continue;
                            }
                            columns.push((byte, col));
                            byte += if cell.c == '\0' { 1 } else { cell.c.len_utf8() };
                            if let Some(extra) = cell.zerowidth() {
                                for &ch in extra {
                                    columns.push((byte, col));
                                    byte += ch.len_utf8();
                                }
                            }
                        }
                        Arc::new(columns)
                    });
                TerminalLogicalContextRow {
                    source_id: row.identity(),
                    columns,
                }
            })
            .collect();
        contexts.push(Arc::new(TerminalLogicalContext {
            text,
            members,
            signature,
            cols: snapshot.cols,
            start_line: (history + start) as usize,
            end_line: (history + end - 1) as usize,
            active_input: (start..end).contains(&term.grid().cursor.point.line.0),
            start_at_history_boundary,
            rows,
        }));
        current = end;
    }
    snapshot.logical_context = contexts;
}

#[cfg(test)]
mod tests {
    use crate::TerminalSession;
    use std::sync::Arc;

    #[test]
    fn unchanged_wrapped_context_reuses_text_after_cursor_movement() {
        let mut terminal = TerminalSession::recording_playback(10, 4, Default::default(), 100);
        terminal.feed_recording_output(b"'abcdefghijklmnopqrstuvwx'\r\nDONE");
        let before = terminal.snapshot_consuming_damage();
        let unchanged = terminal.snapshot_incremental(&before);
        assert!(std::sync::Arc::ptr_eq(
            &before.logical_context[0].text,
            &unchanged.logical_context[0].text
        ));
        terminal.feed_recording_output(b"\x1b[2;1H");
        let moved = terminal.snapshot_incremental(&unchanged);
        assert!(std::sync::Arc::ptr_eq(
            &before.logical_context[0].text,
            &moved.logical_context[0].text
        ));
        assert!(moved.logical_context[0].active_input);
        assert_eq!(
            moved.logical_context[0].text.trim_end(),
            "'abcdefghijklmnopqrstuvwx'"
        );
    }

    #[test]
    fn scrolling_reuses_full_text_and_releases_it_with_the_last_snapshot() {
        let mut terminal = TerminalSession::recording_playback(10, 4, Default::default(), 100);
        terminal.feed_recording_output(b"'abcdefghijklmnopqrstuvwx0123456789'\r\nDONE\r\nEND");
        let mut previous = terminal.snapshot_consuming_damage();
        let weak = Arc::downgrade(&previous.logical_context[0].text);
        let scrolled = terminal.scroll_lines_snapshot_incremental(1, &previous);
        assert!(Arc::ptr_eq(
            &previous.logical_context[0].text,
            &scrolled.logical_context[0].text
        ));
        for offset in [0, 1, 2, 0] {
            let next = terminal.snapshot_with_display_offset(offset, 5, Some(&previous));
            assert!(Arc::ptr_eq(
                &previous.logical_context[0].text,
                &next.logical_context[0].text
            ));
            assert_eq!(
                next.logical_context[0].text.trim_end(),
                "'abcdefghijklmnopqrstuvwx0123456789'"
            );
            previous = next;
        }
        drop(previous);
        drop(scrolled);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn wrapped_context_preserves_wide_and_combining_characters_without_spacers() {
        let mut terminal = TerminalSession::recording_playback(10, 4, Default::default(), 100);
        terminal.feed_recording_output("'abcdefgh界e\u{301}'\r\nDONE".as_bytes());
        let snapshot = terminal.snapshot();
        let context = snapshot.logical_context_for_row(0).unwrap();
        assert_eq!(context.text.trim_end(), "'abcdefgh界e\u{301}'");
        assert_eq!(
            *context.rows[0].columns,
            (0..9).map(|i| (i, i)).collect::<Vec<_>>()
        );
        assert_eq!(
            &context.rows[1].columns[..4],
            &[(9, 0), (12, 2), (13, 2), (15, 3)]
        );
        assert_eq!(
            (context.start_line, context.end_line, context.active_input),
            (0, 1, false)
        );
        assert!(!format!("{context:?}").contains("abcdefgh"));
    }

    #[test]
    fn clipped_snapshot_context_observes_hidden_edits_and_resize() {
        let mut terminal = TerminalSession::recording_playback(10, 6, Default::default(), 100);
        terminal.feed_recording_output(b"'abcdefghijklmnopqrstuvwx'\r\nDONE");
        let before = terminal.snapshot_with_display_offset(0, 1, None);
        let before_context = before.logical_context_for_row(0).unwrap();
        assert_eq!(before_context.text.trim_end(), "'abcdefghijklmnopqrstuvwx'");
        terminal.feed_recording_output(b"\x1b[3;1HZ\x1b[4;1H");
        let after = terminal.snapshot_with_display_offset(0, 1, Some(&before));
        assert_eq!(before.lines[0].text(), after.lines[0].text());
        let after_context = after.logical_context_for_row(0).unwrap();
        assert_eq!(after_context.text.trim_end(), "'abcdefghijklmnopqrsZuvwx'");
        assert_ne!(before_context.signature, after_context.signature);
        terminal.resize_with_cell_size(8, 6, 0, 0).unwrap();
        let resized = terminal.snapshot_incremental(&after);
        assert_eq!(
            resized.logical_context_for_row(0).unwrap().text.trim_end(),
            "'abcdefghijklmnopqrsZuvwx'"
        );
    }

    #[test]
    fn history_trimming_drops_unavailable_prefix_instead_of_reusing_old_context() {
        let mut terminal = TerminalSession::recording_playback(10, 2, Default::default(), 1);
        terminal.feed_recording_output(b"'aaaaaaaaabbbbbbbbbb");
        let before = terminal.snapshot();
        terminal.feed_recording_output(b"ccccccccccddddddddd'\r\nDONE");
        let snapshot = terminal.snapshot_with_display_offset(1, 3, Some(&before));
        let context = snapshot.logical_context_for_row(0).unwrap();
        assert_eq!(context.text.trim_end(), "ccccccccccddddddddd'");
        assert!(context.start_at_history_boundary);
        assert_eq!((context.start_line, context.end_line), (0, 1));
    }
}
