//! Terminal output parser for ANSI escape sequences
//!
//! Parses raw terminal output, strips ANSI codes, and splits into lines.

use super::scroll_buffer::TerminalLine;
use parking_lot::Mutex;
use vte::{Params, Parser, Perform};

/// Parser state for terminal output
struct TerminalParser {
    /// Current line being built
    current_line: String,
    /// Completed lines
    lines: Vec<String>,
}

impl TerminalParser {
    fn new() -> Self {
        Self {
            current_line: String::new(),
            lines: Vec::new(),
        }
    }

    fn finish(&mut self) -> Vec<String> {
        // Push any remaining content as final line
        if !self.current_line.is_empty() {
            self.lines.push(std::mem::take(&mut self.current_line));
        }
        std::mem::take(&mut self.lines)
    }
}

impl Perform for TerminalParser {
    fn print(&mut self, c: char) {
        self.current_line.push(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                // Newline: finish current line
                self.lines.push(std::mem::take(&mut self.current_line));
            }
            b'\r' => {
                // Carriage return: usually followed by \n, so just ignore it
                // (Only clear line if it's an actual overwrite, which we can't easily detect)
                // For now, just ignore \r to preserve content
            }
            b'\t' => {
                // Tab: convert to spaces
                self.current_line.push_str("    ");
            }
            b'\x08' => {
                // Backspace: remove last char
                self.current_line.pop();
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

    // Feed data through VTE parser
    for &byte in data {
        parser.advance(&mut performer, byte);
    }

    // Get completed lines
    let lines = performer.finish();

    // Convert to TerminalLine structs
    lines
        .into_iter()
        .filter(|line| !line.is_empty()) // Filter out empty lines
        .map(TerminalLine::new)
        .collect()
}

/// Simple fallback parser that just splits on newlines and strips ANSI codes
pub fn parse_terminal_output_simple(data: &[u8]) -> Vec<TerminalLine> {
    // Convert to UTF-8, replacing invalid sequences
    let text = String::from_utf8_lossy(data);

    // Strip ANSI escape codes
    let stripped = strip_ansi_escapes::strip_str(&text);

    // Split into lines
    stripped
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| TerminalLine::new(line.to_string()))
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
        for &byte in data {
            self.parser.advance(&mut *performer, byte);
        }
    }

    /// Get all completed lines and reset
    pub fn flush(&self) -> Vec<TerminalLine> {
        let mut performer = self.performer.lock();
        let lines = performer.finish();

        lines
            .into_iter()
            .filter(|line| !line.is_empty())
            .map(TerminalLine::new)
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
        assert_eq!(lines[1].text, "plain");
    }

    #[test]
    fn test_carriage_return() {
        // Progress bar style: \r is currently ignored (just moves cursor)
        // Real terminal would overwrite, but we preserve content for simplicity
        let data = b"loading....\rDone!\n";
        let lines = parse_terminal_output(data);

        assert_eq!(lines.len(), 1);
        // \r doesn't clear content in current implementation - it's ignored
        assert_eq!(lines[0].text, "loading....Done!");
    }

    #[test]
    fn test_backspace() {
        let data = b"hellx\x08o\n";
        let lines = parse_terminal_output(data);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "hello"); // \x08 (backspace) removed 'x'
    }

    #[test]
    fn test_tabs() {
        let data = b"col1\tcol2\n";
        let lines = parse_terminal_output(data);

        assert_eq!(lines.len(), 1);
        assert!(lines[0].text.contains("    ")); // Tab converted to spaces
    }

    #[test]
    fn test_simple_parser() {
        let data = b"\x1b[32mGreen\x1b[0m text\nSecond line\n";
        let lines = parse_terminal_output_simple(data);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Green text");
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
}
