//! Frozen text from the backend grid, never from a view's scroll position.
use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    index::Line,
    term::{Term, TermMode, cell::Flags},
};
use std::time::{Instant, SystemTime};
use zeroize::Zeroizing;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalHistoryStamp {
    pub identity: u64,
    pub content: u64,
    pub switches: u64,
    pub cleared: u64,
    pub evicted: usize,
}
pub struct TerminalHistoryRow {
    pub text: Zeroizing<String>,
    pub soft_wrapped: bool,
}
pub struct TerminalHistoryCapture {
    pub stamp: TerminalHistoryStamp,
    pub captured: Instant,
    pub wall_time: SystemTime,
    pub screen_rows: usize,
    pub retained_rows: usize,
    pub alternate_active: bool,
    pub rows: Option<Vec<TerminalHistoryRow>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalHistoryError {
    Unsupported,
    BufferUnavailable,
    TooLarge,
    Cancelled,
}

pub(crate) fn freeze_history<T: EventListener>(
    term: &Term<T>,
    alternate: bool,
    screen: bool,
    previous: Option<&TerminalHistoryStamp>,
    byte_limit: usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<TerminalHistoryCapture, TerminalHistoryError> {
    let active = term.mode().contains(TermMode::ALT_SCREEN);
    if alternate && !active {
        return Err(TerminalHistoryError::BufferUnavailable);
    }
    let grid = if alternate {
        term.grid()
    } else {
        term.primary_grid()
    };
    let stamp = TerminalHistoryStamp {
        identity: term.history_identity(),
        content: grid.history_revision(),
        switches: term.history_switches(),
        cleared: grid.history_cleared(),
        evicted: grid.scrolled_out_lines(),
    };
    let captured = Instant::now();
    let wall_time = SystemTime::now();
    let retained_rows = grid.total_lines();
    let screen_rows = grid.screen_lines();
    let rows = if previous == Some(&stamp) {
        None
    } else {
        let start = if screen {
            0
        } else {
            -(retained_rows.saturating_sub(screen_rows) as i32)
        };
        let count = (screen_rows as i32 - start) as usize;
        let mut bytes = count
            .checked_mul(std::mem::size_of::<TerminalHistoryRow>())
            .ok_or(TerminalHistoryError::TooLarge)?;
        if bytes > byte_limit {
            return Err(TerminalHistoryError::TooLarge);
        }
        let mut rows = Vec::with_capacity(count);
        for line in start..screen_rows as i32 {
            if cancelled() {
                return Err(TerminalHistoryError::Cancelled);
            }
            let row = &grid[Line(line)];
            let wrapped = row
                .last()
                .is_some_and(|cell| cell.flags().contains(Flags::WRAPLINE));
            // Count the exact retained UTF-8 bytes before any sensitive text is copied.
            // Zeroizing<String> cannot erase allocations freed by a later reallocation.
            let mut full_len = 0usize;
            let mut text_len = 0usize;
            for cell in row.iter() {
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                let ch = if cell.c == '\0' { ' ' } else { cell.c };
                for ch in std::iter::once(ch).chain(cell.zerowidth().into_iter().flatten().copied())
                {
                    full_len = full_len
                        .checked_add(ch.len_utf8())
                        .ok_or(TerminalHistoryError::TooLarge)?;
                    if wrapped || ch != ' ' {
                        text_len = full_len;
                    }
                }
            }
            if text_len > byte_limit.saturating_sub(bytes) {
                return Err(TerminalHistoryError::TooLarge);
            }
            let mut text = Zeroizing::new(String::new());
            text.try_reserve_exact(text_len)
                .map_err(|_| TerminalHistoryError::TooLarge)?;
            let capacity = text.capacity();
            if capacity > byte_limit.saturating_sub(bytes) {
                return Err(TerminalHistoryError::TooLarge);
            }
            'cells: for cell in row.iter() {
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                let ch = if cell.c == '\0' { ' ' } else { cell.c };
                for ch in std::iter::once(ch).chain(cell.zerowidth().into_iter().flatten().copied())
                {
                    if text.len() == text_len {
                        break 'cells;
                    }
                    text.push(ch);
                }
            }
            debug_assert_eq!(
                text.capacity(),
                capacity,
                "history text must never reallocate"
            );
            bytes = bytes.saturating_add(text.capacity());
            if bytes > byte_limit {
                return Err(TerminalHistoryError::TooLarge);
            }
            rows.push(TerminalHistoryRow {
                text,
                soft_wrapped: wrapped,
            });
        }
        Some(rows)
    };
    Ok(TerminalHistoryCapture {
        stamp,
        captured,
        wall_time,
        screen_rows,
        retained_rows,
        alternate_active: active,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::{
        event::VoidListener, grid::Scroll, term::Config, vte::ansi::Processor,
    };
    struct Size(usize, usize);
    impl Dimensions for Size {
        fn total_lines(&self) -> usize {
            self.1
        }
        fn screen_lines(&self) -> usize {
            self.1
        }
        fn columns(&self) -> usize {
            self.0
        }
    }
    fn read(term: &Term<VoidListener>, screen: bool) -> TerminalHistoryCapture {
        freeze_history(term, false, screen, None, 16 * 1024 * 1024, &|| false).unwrap()
    }
    #[test]
    fn retained_history_exceeds_500_lines_and_ignores_viewport() {
        let mut term = Term::new(
            Config {
                scrolling_history: 2400,
                ..Default::default()
            },
            &Size(32, 24),
            VoidListener,
        );
        let mut parser: Processor = Processor::new();
        let input = (1..=1200).map(|n| format!("{n}\r\n")).collect::<String>();
        parser.advance(&mut term, input.as_bytes());
        term.scroll_display(Scroll::Top);
        let offset = term.grid().display_offset();
        let first = read(&term, false);
        let expected = (1..=1200)
            .map(|n| n.to_string())
            .chain(std::iter::once(String::new()))
            .collect::<Vec<_>>();
        assert_eq!(
            first
                .rows
                .as_ref()
                .unwrap()
                .iter()
                .map(|r| r.text.to_string())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(term.grid().display_offset(), offset);
        term.scroll_display(Scroll::Bottom);
        let unchanged = freeze_history(
            &term,
            false,
            false,
            Some(&first.stamp),
            16 * 1024 * 1024,
            &|| false,
        )
        .unwrap();
        assert!(unchanged.rows.is_none());
        let more = (1201..=2400)
            .map(|n| format!("{n}\r\n"))
            .collect::<String>();
        parser.advance(&mut term, more.as_bytes());
        parser.advance(&mut term, b"latest");
        term.scroll_display(Scroll::Top);
        let latest = read(&term, true);
        assert_eq!(
            latest.rows.as_ref().unwrap().last().unwrap().text.as_str(),
            "latest"
        );
        assert_eq!(latest.rows.as_ref().unwrap()[22].text.as_str(), "2400");
        assert_ne!(first.stamp, latest.stamp);
        assert_eq!(
            first.rows.as_ref().unwrap().last().unwrap().text.as_str(),
            ""
        );
    }
    #[test]
    fn screen_freezes_first_line_prompt_and_preserves_soft_wrap_spaces() {
        let mut term = Term::new(Config::default(), &Size(4, 24), VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, b"abc def");
        let frozen = read(&term, true);
        let rows = frozen.rows.as_ref().unwrap();
        assert_eq!(rows[0].text.as_str(), "abc ");
        assert!(rows[0].soft_wrapped);
        assert_eq!(rows[1].text.as_str(), "def");
        parser.advance(&mut term, b"\x1b[2J\x1b[H> ");
        let screen = read(&term, true);
        assert_eq!(screen.rows.as_ref().unwrap()[0].text.as_str(), ">");
        assert_eq!(screen.rows.as_ref().unwrap().len(), 24);
        parser.advance(&mut term, b"\x1b[?1049h\x1b[HALT");
        assert_eq!(
            read(&term, true).rows.as_ref().unwrap()[0].text.as_str(),
            ">"
        );
        let alt = freeze_history(&term, true, true, None, 16 * 1024 * 1024, &|| false).unwrap();
        assert_eq!(alt.rows.as_ref().unwrap()[0].text.as_str(), "ALT");
        term.resize(Size(8, 30));
        parser.advance(&mut term, b"\x1b[2J\x1b[?1049l");
        assert_eq!(rows[0].text.as_str(), "abc ");
        assert_eq!(frozen.screen_rows, 24);
        assert_eq!(read(&term, true).screen_rows, 30);
        assert_ne!(screen.stamp, read(&term, true).stamp);
    }
    #[test]
    fn eviction_clear_and_size_limits_are_explicit() {
        let mut term = Term::new(
            Config {
                scrolling_history: 2,
                ..Default::default()
            },
            &Size(8, 2),
            VoidListener,
        );
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, b"a\r\nb\r\nc\r\nd");
        let before = read(&term, false);
        parser.advance(&mut term, b"\r\ne\r\nf");
        let after = read(&term, false);
        assert_eq!(before.retained_rows, after.retained_rows);
        assert!(after.stamp.evicted > before.stamp.evicted);
        parser.advance(&mut term, b"\x1b[3J");
        assert_ne!(after.stamp.cleared, read(&term, false).stamp.cleared);
        assert!(matches!(
            freeze_history(&term, false, false, None, 1, &|| false),
            Err(TerminalHistoryError::TooLarge)
        ));
        assert!(matches!(
            freeze_history(&term, false, false, None, 1000, &|| true),
            Err(TerminalHistoryError::Cancelled)
        ));
    }
    #[test]
    fn retention_markers_follow_discarded_rows_not_preserving_clear() {
        for (control, expected) in [
            (
                &b"\x1b[2;4r\x1b[4;1H\n"[..],
                ["one", "three", "four", "", "five", "six"],
            ),
            (
                &b"\x1b[2;4r\x1b[2;1H\x1bM"[..],
                ["one", "", "two", "three", "five", "six"],
            ),
            (&b"\x1b[6T"[..], ["", "", "", "", "", ""]),
        ] {
            let mut term = Term::new(Config::default(), &Size(20, 6), VoidListener);
            let mut parser: Processor = Processor::new();
            parser.advance(&mut term, b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix");
            let before = read(&term, false);
            parser.advance(&mut term, control);
            let after = read(&term, false);
            assert_eq!(
                after
                    .rows
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_ne!(before.stamp.cleared, after.stamp.cleared);
        }
        let mut term = Term::new(Config::default(), &Size(20, 6), VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, b"one\r\ntwo");
        let before = read(&term, false);
        parser.advance(&mut term, b"\x1b[2J");
        let after = read(&term, false);
        assert_eq!(
            after
                .rows
                .as_ref()
                .unwrap()
                .iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>(),
            ["one", "two", "", "", "", "", "", ""]
        );
        assert_eq!(before.stamp.cleared, after.stamp.cleared);
        assert_eq!(before.stamp.evicted, after.stamp.evicted);
        assert_ne!(before.stamp.content, after.stamp.content);
    }

    #[test]
    fn resize_height_marks_only_discarded_content() {
        for (input, expected, lost) in [
            (
                "one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\x1b[H",
                vec!["one", "two", "three"],
                true,
            ),
            (
                "one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix",
                vec!["one", "two", "three", "four", "five", "six"],
                false,
            ),
            ("one\r\ntwo\x1b[H", vec!["one", "two", ""], false),
        ] {
            let mut term = Term::new(Config::default(), &Size(20, 6), VoidListener);
            let mut parser: Processor = Processor::new();
            parser.advance(&mut term, input.as_bytes());
            let before = read(&term, false);
            term.resize(Size(20, 3));
            let after = read(&term, false);
            assert_eq!(
                after
                    .rows
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                before.stamp.cleared != after.stamp.cleared,
                lost,
                "{input:?}"
            );
            assert_eq!(before.stamp.evicted, after.stamp.evicted);
            assert_ne!(before.stamp.content, after.stamp.content);
        }
    }

    #[test]
    fn resize_alternate_width_marks_only_truncated_content() {
        for (input, expected, lost) in [("abc", "abc", false), ("abcdefghijk", "abcdefghij", true)]
        {
            let mut term = Term::new(Config::default(), &Size(20, 6), VoidListener);
            let mut parser: Processor = Processor::new();
            parser.advance(&mut term, b"\x1b[?1049h\x1b[H");
            parser.advance(&mut term, input.as_bytes());
            let before =
                freeze_history(&term, true, false, None, 16 * 1024 * 1024, &|| false).unwrap();
            term.resize(Size(10, 6));
            let after =
                freeze_history(&term, true, false, None, 16 * 1024 * 1024, &|| false).unwrap();
            assert_eq!(
                after
                    .rows
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<Vec<_>>(),
                [expected, "", "", "", "", ""]
            );
            assert_eq!(
                before.stamp.cleared != after.stamp.cleared,
                lost,
                "{input:?}"
            );
            assert_eq!(before.stamp.evicted, after.stamp.evicted);
            assert_ne!(before.stamp.content, after.stamp.content);
        }
    }

    #[test]
    fn combining_text_is_frozen_with_exact_bounded_allocation() {
        let mut term = Term::new(Config::default(), &Size(20, 6), VoidListener);
        let mut parser: Processor = Processor::new();
        let expected = "secret".to_owned() + &"\u{301}".repeat(100);
        parser.advance(&mut term, expected.as_bytes());
        let limit = 6 * std::mem::size_of::<TerminalHistoryRow>() + expected.len();
        let frozen = freeze_history(&term, false, true, None, limit, &|| false).unwrap();
        assert_eq!(frozen.rows.as_ref().unwrap()[0].text.as_str(), expected);
        assert!(matches!(
            freeze_history(&term, false, true, None, limit - 1, &|| false),
            Err(TerminalHistoryError::TooLarge)
        ));
    }
}
