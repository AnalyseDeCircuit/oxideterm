// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

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
