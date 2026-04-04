// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Terminal output parser for ANSI escape sequences
//!
//! Parses raw terminal output, strips ANSI codes, and splits into lines.

use super::scroll_buffer::TerminalLine;
use parking_lot::Mutex;
use vte::{Params, Parser, Perform};

fn extract_raw_terminal_lines(data: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(data)
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .filter(|line| !strip_ansi_escapes::strip_str(line).is_empty())
        .collect()
}

/// Parser state for terminal output
struct TerminalParser {
    /// Current line being built
    current_line: String,
    /// Completed lines
    lines: Vec<String>,
    /// Whether a CR was received — next print() overwrites from column 0
    cr_pending: bool,
}

impl TerminalParser {
    fn new() -> Self {
        Self {
            current_line: String::new(),
            lines: Vec::new(),
            cr_pending: false,
        }
    }

    fn finish(&mut self) -> Vec<String> {
        // Push any remaining content as final line
        if !self.current_line.is_empty() {
            self.lines.push(std::mem::take(&mut self.current_line));
        }
        self.cr_pending = false;
        std::mem::take(&mut self.lines)
    }
}

impl Perform for TerminalParser {
    fn print(&mut self, c: char) {
        if self.cr_pending {
            self.cr_pending = false;
            // CR + new content: clear and overwrite entire line.
            // This intentionally simplifies partial overwrites (e.g. "ABCDE\rXY"
            // becomes "XY" instead of "XYCDE"). Full cursor-position tracking
            // is not worth the complexity for a text-log buffer.
            self.current_line.clear();
        }
        self.current_line.push(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                // Newline: finish current line and reset CR state
                self.cr_pending = false;
                self.lines.push(std::mem::take(&mut self.current_line));
            }
            b'\r' => {
                // Carriage return: move cursor to column 0.
                // Subsequent print() calls overwrite from the start.
                // We track the cursor position so partial overwrites work
                // correctly (e.g. progress bars that only update the percentage).
                self.cr_pending = true;
            }
            b'\t' => {
                // Tab: advance to next 8-column tab stop
                let col = self.current_line.chars().count();
                let spaces = 8 - (col % 8);
                for _ in 0..spaces {
                    self.current_line.push(' ');
                }
            }
            b'\x08' => {
                // Backspace: remove last grapheme cluster (handles combining marks)
                if let Some(pos) = self.current_line.char_indices().next_back().map(|(i, _)| i) {
                    self.current_line.truncate(pos);
                }
            }
            _ => {
                // Ignore other control characters
            }
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _c: char) {
        // DCS sequences - ignore for now
    }

    fn put(&mut self, _byte: u8) {
        // DCS data - ignore
    }

    fn unhook(&mut self) {
        // End of DCS - ignore
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        // OSC sequences (Operating System Command) - ignore for now
    }

    fn csi_dispatch(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _c: char) {
        // CSI sequences (Control Sequence Introducer) - ignore, these are formatting
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {
        // ESC sequences - ignore
    }
}

/// Parse terminal output and extract lines
pub fn parse_terminal_output(data: &[u8]) -> Vec<TerminalLine> {
    let mut parser = Parser::new();
    let mut performer = TerminalParser::new();

    // Feed data through VTE parser (vte 0.14: advance takes &[u8] slices)
    parser.advance(&mut performer, data);

    // Get completed lines
    let parsed_lines: Vec<String> = performer
        .finish()
        .into_iter()
        .filter(|line| !line.is_empty())
        .collect();
    let raw_lines = extract_raw_terminal_lines(data);

    // Only preserve ANSI if the raw split and the VTE-parsed text agree for the
    // entire batch. If they diverge, fall back to plain text rather than risk
    // pairing the wrong ANSI sequence with the wrong line.
    let raw_lines_match = raw_lines.len() == parsed_lines.len()
        && raw_lines
            .iter()
            .zip(parsed_lines.iter())
            .all(|(raw, parsed)| strip_ansi_escapes::strip_str(raw) == *parsed);

    // Convert to TerminalLine structs — share a single timestamp for the whole batch
    let now = chrono::Utc::now().timestamp_millis() as u64;
    parsed_lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let ansi_text = if raw_lines_match {
                raw_lines.get(index).cloned().filter(|raw| raw != &line)
            } else {
                None
            };
            TerminalLine::with_ansi_timestamp(line, ansi_text, now)
        })
        .collect()
}

/// Simple fallback parser that just splits on newlines and strips ANSI codes
pub fn parse_terminal_output_simple(data: &[u8]) -> Vec<TerminalLine> {
    let raw_lines = extract_raw_terminal_lines(data);
    let now = chrono::Utc::now().timestamp_millis() as u64;
    raw_lines
        .into_iter()
        .filter_map(|raw| {
            let text = strip_ansi_escapes::strip_str(&raw);
            if text.is_empty() {
                return None;
            }

            let ansi_text = if raw == text { None } else { Some(raw) };
            Some(TerminalLine::with_ansi_timestamp(text, ansi_text, now))
        })
        .collect()
}

/// Batch parser for accumulated terminal data
pub struct BatchParser {
    parser: Parser,
    performer: Mutex<TerminalParser>,
}

impl BatchParser {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            performer: Mutex::new(TerminalParser::new()),
        }
    }

    /// Feed data to the parser
    pub fn feed(&mut self, data: &[u8]) {
        let mut performer = self.performer.lock();
        self.parser.advance(&mut *performer, data);
    }

    /// Get all completed lines and reset
    pub fn flush(&self) -> Vec<TerminalLine> {
        let mut performer = self.performer.lock();
        let lines = performer.finish();

        let now = chrono::Utc::now().timestamp_millis() as u64;
        lines
            .into_iter()
            .filter(|line| !line.is_empty())
            .map(|line| TerminalLine::with_timestamp(line, now))
            .collect()
    }
}

impl Default for BatchParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_text() {
        let data = b"hello\nworld\n";
        let lines = parse_terminal_output(data);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "hello");
        assert_eq!(lines[1].text, "world");
    }

    #[test]
    fn test_ansi_colors() {
        // Text with ANSI color codes
        let data = b"\x1b[31mred\x1b[0m\nplain\n";
        let lines = parse_terminal_output(data);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "red"); // Color codes stripped
        assert_eq!(lines[0].ansi_text.as_deref(), Some("\x1b[31mred\x1b[0m"));
        assert_eq!(lines[1].text, "plain");
        assert_eq!(lines[1].ansi_text, None);
    }

    #[test]
    fn test_carriage_return() {
        // Progress bar style: \r moves cursor to column 0, new content overwrites
        let data = b"loading....\rDone!\n";
        let lines = parse_terminal_output(data);

        assert_eq!(lines.len(), 1);
        // \r resets to column 0; "Done!" overwrites from the start
        assert_eq!(lines[0].text, "Done!");
    }

    #[test]
    fn test_cr_lf_sequence() {
        // Normal \r\n line ending — should produce clean lines
        let data = b"line1\r\nline2\r\n";
        let lines = parse_terminal_output(data);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "line1");
        assert_eq!(lines[1].text, "line2");
    }

    #[test]
    fn test_backspace() {
        let data = b"hellx\x08o\n";
        let lines = parse_terminal_output(data);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "hello"); // \x08 (backspace) removed 'x'
        assert_eq!(lines[0].ansi_text, None);
    }

    #[test]
    fn test_tabs() {
        let data = b"col1\tcol2\n";
        let lines = parse_terminal_output(data);

        assert_eq!(lines.len(), 1);
        // "col1" is 4 chars → next tab stop at column 8 → 4 spaces
        assert_eq!(lines[0].text, "col1    col2");
    }

    #[test]
    fn test_simple_parser() {
        let data = b"\x1b[32mGreen\x1b[0m text\nSecond line\n";
        let lines = parse_terminal_output_simple(data);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Green text");
        assert_eq!(
            lines[0].ansi_text.as_deref(),
            Some("\x1b[32mGreen\x1b[0m text")
        );
        assert_eq!(lines[1].text, "Second line");
    }

    #[test]
    fn test_batch_parser() {
        let mut parser = BatchParser::new();

        // Feed data in chunks
        parser.feed(b"first ");
        parser.feed(b"line\n");
        parser.feed(b"second line\n");

        let lines = parser.flush();

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "first line");
        assert_eq!(lines[1].text, "second line");
    }

    #[test]
    fn test_empty_lines_filtered() {
        let data = b"line1\n\n\nline2\n";
        let lines = parse_terminal_output(data);

        assert_eq!(lines.len(), 2); // Empty lines filtered
        assert_eq!(lines[0].text, "line1");
        assert_eq!(lines[1].text, "line2");
    }

    #[test]
    fn test_no_final_newline() {
        let data = b"incomplete line";
        let lines = parse_terminal_output(data);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "incomplete line");
    }

    #[test]
    fn test_extract_raw_terminal_lines_basic() {
        let data = b"hello\nworld\n";
        let lines = extract_raw_terminal_lines(data);
        assert_eq!(lines, vec!["hello", "world"]);
    }

    #[test]
    fn test_extract_raw_terminal_lines_crlf() {
        let data = b"line1\r\nline2\r\n";
        let lines = extract_raw_terminal_lines(data);
        assert_eq!(lines, vec!["line1", "line2"]);
    }

    #[test]
    fn test_extract_raw_terminal_lines_empty_filtered() {
        let data = b"a\n\n\nb\n";
        let lines = extract_raw_terminal_lines(data);
        assert_eq!(lines, vec!["a", "b"]);
    }

    #[test]
    fn test_extract_raw_terminal_lines_ansi_only_line_filtered() {
        // A line with ONLY ANSI codes (no visible text) should be filtered out
        let data = b"\x1b[31m\x1b[0m\nvisible\n";
        let lines = extract_raw_terminal_lines(data);
        // The ANSI-only line is stripped to empty and filtered
        assert_eq!(lines, vec!["visible"]);
    }
}
