use alacritty_terminal::{
    index::{Column, Line, Point, Side},
    selection::{Selection, SelectionType},
    term::Term,
};

/// Inclusive grid coordinates shared by selection painting and clipboard reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSelectionRange {
    pub start_line: i32,
    pub start_col: usize,
    pub end_line: i32,
    pub end_col: usize,
    pub is_block: bool,
}

pub(crate) fn set_term_selection<T>(term: &mut Term<T>, range: Option<TerminalSelectionRange>) {
    term.selection = range.map(|range| {
        // The UI already expands word and line selections. Keep those exact
        // cells while delegating scroll, erase, and resize tracking to the grid.
        let kind = if range.is_block {
            SelectionType::Block
        } else {
            SelectionType::Simple
        };
        let mut selection = Selection::new(
            kind,
            Point::new(Line(range.start_line), Column(range.start_col)),
            Side::Left,
        );
        selection.update(
            Point::new(Line(range.end_line), Column(range.end_col)),
            Side::Right,
        );
        selection
    });
}

pub(crate) fn term_selection<T>(term: &Term<T>) -> Option<TerminalSelectionRange> {
    let range = term.selection.as_ref()?.to_range(term)?;
    Some(TerminalSelectionRange {
        start_line: range.start.line.0,
        start_col: range.start.column.0,
        end_line: range.end.line.0,
        end_col: range.end.column.0,
        is_block: range.is_block,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TerminalSession;

    fn select_line(session: &TerminalSession, line: i32) {
        session.set_selection(Some(TerminalSelectionRange {
            start_line: line,
            start_col: 0,
            end_line: line,
            end_col: 3,
            is_block: false,
        }));
    }

    #[test]
    fn selection_tracks_scrolling_when_history_is_full_and_expires_on_eviction() {
        let mut session = TerminalSession::recording_playback(12, 3, Default::default(), 2);
        session.feed_recording_output(b"zero\r\none\r\ntwo");
        select_line(&session, 1);
        session.feed_recording_output(b"\r\nthree\r\nfour\r\nfive");
        assert_eq!(session.snapshot().scrollback_lines, 2);
        assert_eq!(session.selection().unwrap().start_line, -2);
        session.feed_recording_output(b"\r\nsix");
        assert!(session.selection().is_none());
    }

    #[test]
    fn selection_survives_a_large_output_batch_and_manual_viewport_scrolling() {
        let mut session = TerminalSession::recording_playback(12, 3, Default::default(), 100);
        session.feed_recording_output(b"zero\r\none\r\ntwo");
        select_line(&session, 1);
        session.feed_recording_output(&b"\r\nnext".repeat(40));
        let range = session.selection().unwrap();
        assert_eq!(range.start_line, -39);
        session.scroll_to_display_offset(40);
        assert_eq!(session.selection(), Some(range));
        session.scroll_to_display_offset(0);
        assert_eq!(session.selection(), Some(range));
    }

    #[test]
    fn selection_obeys_partial_scroll_regions_and_terminal_resets() {
        let mut session = TerminalSession::recording_playback(12, 4, Default::default(), 20);
        session.feed_recording_output(b"zero\r\none\r\ntwo\r\nthree");
        select_line(&session, 2);
        session.feed_recording_output(b"\x1b[2;4r\x1b[S");
        assert_eq!(session.selection().unwrap().start_line, 1);
        session.feed_recording_output(b"\x1b[2K");
        session.feed_recording_output(b"\x1b[2J");
        assert!(session.selection().is_none());
        select_line(&session, 0);
        session.feed_recording_output(b"\x1b[?1049h");
        assert!(session.selection().is_none());
        session.feed_recording_output(b"\x1b[?1049l");
        assert!(session.selection().is_none());
    }
}
