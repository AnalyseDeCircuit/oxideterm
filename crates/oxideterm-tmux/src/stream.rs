// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::{borrow::Cow, error::Error, fmt};

use crate::{ControlEvent, ControlParser, Notification};

pub const CONTROL_MODE_ENTER: &[u8] = b"\x1bP1000p";
pub const CONTROL_MODE_EXIT: &[u8] = b"\x1b\\";
const DEFAULT_MAX_CONTROL_LINE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ControlStreamMode {
    #[default]
    Terminal,
    Control,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StreamEvent<'a> {
    TerminalOutput(Cow<'a, [u8]>),
    ControlModeEntered,
    Control(ControlEvent<'a>),
    ControlModeExited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlStreamError {
    pub line_bytes: usize,
    pub maximum_line_bytes: usize,
}

impl fmt::Display for ControlStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "tmux control line has {} bytes, exceeding the {} byte limit",
            self.line_bytes, self.maximum_line_bytes
        )
    }
}

impl Error for ControlStreamError {}

/// Separates ordinary terminal output from an in-band `tmux -CC` session.
///
/// Complete terminal spans and control lines borrow the caller's read buffer.
/// Only a marker or line split across reads is retained internally.
pub struct ControlStream {
    mode: ControlStreamMode,
    enter_match_bytes: usize,
    pending_line: Vec<u8>,
    pending_control_escape: bool,
    exit_announced: bool,
    maximum_line_bytes: usize,
    parser: ControlParser,
}

impl Default for ControlStream {
    fn default() -> Self {
        Self::with_maximum_line_bytes(DEFAULT_MAX_CONTROL_LINE_BYTES)
    }
}

impl ControlStream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_maximum_line_bytes(maximum_line_bytes: usize) -> Self {
        Self {
            mode: ControlStreamMode::Terminal,
            enter_match_bytes: 0,
            pending_line: Vec::new(),
            pending_control_escape: false,
            exit_announced: false,
            maximum_line_bytes: maximum_line_bytes.max(1),
            parser: ControlParser::new(),
        }
    }

    pub fn mode(&self) -> ControlStreamMode {
        self.mode
    }

    pub fn advance(
        &mut self,
        input: &[u8],
        mut emit: impl FnMut(StreamEvent<'_>),
    ) -> Result<(), ControlStreamError> {
        let mut cursor = 0;

        while cursor < input.len() {
            match self.mode {
                ControlStreamMode::Terminal => {
                    self.advance_terminal(input, &mut cursor, &mut emit);
                }
                ControlStreamMode::Control => {
                    self.advance_control(input, &mut cursor, &mut emit)?;
                }
            }
        }

        Ok(())
    }

    fn advance_terminal(
        &mut self,
        input: &[u8],
        cursor: &mut usize,
        emit: &mut impl FnMut(StreamEvent<'_>),
    ) {
        while *cursor < input.len() && self.mode == ControlStreamMode::Terminal {
            if self.enter_match_bytes != 0 {
                let candidate_start = self.enter_match_bytes;
                let available = input.len() - *cursor;
                let compare_len = available.min(CONTROL_MODE_ENTER.len() - candidate_start);
                let continuation =
                    &CONTROL_MODE_ENTER[candidate_start..candidate_start + compare_len];
                let input_prefix = &input[*cursor..*cursor + compare_len];
                let matched = continuation
                    .iter()
                    .zip(input_prefix)
                    .take_while(|(expected, actual)| expected == actual)
                    .count();
                self.enter_match_bytes += matched;
                *cursor += matched;

                if self.enter_match_bytes == CONTROL_MODE_ENTER.len() {
                    self.enter_match_bytes = 0;
                    self.mode = ControlStreamMode::Control;
                    self.parser.reset();
                    self.exit_announced = false;
                    emit(StreamEvent::ControlModeEntered);
                } else if matched < compare_len {
                    // The marker is static and no prefix allocation is needed
                    // when an ANSI escape merely resembles the tmux marker.
                    emit(StreamEvent::TerminalOutput(Cow::Borrowed(
                        &CONTROL_MODE_ENTER[..self.enter_match_bytes],
                    )));
                    self.enter_match_bytes = 0;
                }
                continue;
            }

            let remaining = &input[*cursor..];
            if let Some(marker_offset) = memchr::memmem::find(remaining, CONTROL_MODE_ENTER) {
                if marker_offset != 0 {
                    emit(StreamEvent::TerminalOutput(Cow::Borrowed(
                        &remaining[..marker_offset],
                    )));
                }
                *cursor += marker_offset + CONTROL_MODE_ENTER.len();
                self.mode = ControlStreamMode::Control;
                self.parser.reset();
                self.exit_announced = false;
                emit(StreamEvent::ControlModeEntered);
                continue;
            }

            let partial_len = trailing_entry_prefix_len(remaining);
            let terminal_len = remaining.len() - partial_len;
            if terminal_len != 0 {
                emit(StreamEvent::TerminalOutput(Cow::Borrowed(
                    &remaining[..terminal_len],
                )));
            }
            self.enter_match_bytes = partial_len;
            *cursor = input.len();
        }
    }

    fn advance_control(
        &mut self,
        input: &[u8],
        cursor: &mut usize,
        emit: &mut impl FnMut(StreamEvent<'_>),
    ) -> Result<(), ControlStreamError> {
        while *cursor < input.len() && self.mode == ControlStreamMode::Control {
            if self.pending_control_escape {
                if input[*cursor] == b'\\' && self.exit_announced {
                    self.emit_pending_line(emit);
                    self.pending_control_escape = false;
                    self.exit_announced = false;
                    self.mode = ControlStreamMode::Terminal;
                    self.parser.reset();
                    *cursor += 1;
                    emit(StreamEvent::ControlModeExited);
                    continue;
                }

                self.append_pending(&[0x1b])?;
                self.pending_control_escape = false;
                continue;
            }

            let remaining = &input[*cursor..];
            let delimiter = memchr::memchr2(b'\n', 0x1b, remaining);
            let Some(delimiter_offset) = delimiter else {
                self.append_pending(remaining)?;
                *cursor = input.len();
                return Ok(());
            };

            let delimiter_index = *cursor + delimiter_offset;
            match input[delimiter_index] {
                b'\n' => {
                    let line = &input[*cursor..delimiter_index];
                    if self.pending_line.is_empty() {
                        self.check_line_length(line.len())?;
                        self.emit_line(trim_carriage_return(line), emit);
                    } else {
                        self.append_pending(line)?;
                        self.emit_pending_line(emit);
                    }
                    *cursor = delimiter_index + 1;
                }
                0x1b => {
                    self.append_pending(&input[*cursor..delimiter_index])?;
                    self.pending_control_escape = true;
                    *cursor = delimiter_index + 1;
                }
                _ => unreachable!("delimiter search only returns newline or escape"),
            }
        }

        Ok(())
    }

    fn append_pending(&mut self, bytes: &[u8]) -> Result<(), ControlStreamError> {
        self.check_line_length(self.pending_line.len().saturating_add(bytes.len()))?;
        self.pending_line.extend_from_slice(bytes);
        Ok(())
    }

    fn check_line_length(&self, line_bytes: usize) -> Result<(), ControlStreamError> {
        if line_bytes <= self.maximum_line_bytes {
            Ok(())
        } else {
            Err(ControlStreamError {
                line_bytes,
                maximum_line_bytes: self.maximum_line_bytes,
            })
        }
    }

    fn emit_pending_line(&mut self, emit: &mut impl FnMut(StreamEvent<'_>)) {
        if self.pending_line.is_empty() {
            return;
        }
        if self.pending_line.last() == Some(&b'\r') {
            self.pending_line.pop();
        }
        let event = self.parser.parse_line(&self.pending_line);
        self.exit_announced |= matches!(
            &event,
            ControlEvent::Notification(Notification::Exit { .. })
        );
        emit(StreamEvent::Control(event));
        self.pending_line.clear();
    }

    fn emit_line(&mut self, line: &[u8], emit: &mut impl FnMut(StreamEvent<'_>)) {
        let event = self.parser.parse_line(line);
        self.exit_announced |= matches!(
            &event,
            ControlEvent::Notification(Notification::Exit { .. })
        );
        emit(StreamEvent::Control(event));
    }
}

fn trim_carriage_return(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn trailing_entry_prefix_len(bytes: &[u8]) -> usize {
    let maximum = bytes.len().min(CONTROL_MODE_ENTER.len() - 1);
    (1..=maximum)
        .rev()
        .find(|length| bytes.ends_with(&CONTROL_MODE_ENTER[..*length]))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandGuard, Notification, PaneId};

    #[derive(Debug, Eq, PartialEq)]
    enum Observed {
        Terminal(Vec<u8>),
        Entered,
        Begin(u64),
        Output(PaneId, Vec<u8>),
        Other(Vec<u8>, Vec<u8>),
        Exited,
    }

    fn observe(event: StreamEvent<'_>, events: &mut Vec<Observed>) {
        match event {
            StreamEvent::TerminalOutput(bytes) => {
                events.push(Observed::Terminal(bytes.into_owned()))
            }
            StreamEvent::ControlModeEntered => events.push(Observed::Entered),
            StreamEvent::Control(ControlEvent::Begin(CommandGuard { number, .. })) => {
                events.push(Observed::Begin(number));
            }
            StreamEvent::Control(ControlEvent::Notification(Notification::Output {
                pane,
                bytes,
            })) => events.push(Observed::Output(pane, bytes.into_owned())),
            StreamEvent::Control(ControlEvent::Notification(Notification::Other {
                name,
                arguments,
            })) => events.push(Observed::Other(name.to_vec(), arguments.to_vec())),
            StreamEvent::ControlModeExited => events.push(Observed::Exited),
            _ => {}
        }
    }

    #[test]
    fn entry_marker_can_span_reads_and_keeps_same_read_control_data() {
        let mut stream = ControlStream::new();
        let mut events = Vec::new();
        stream
            .advance(b"prompt\x1bP10", |event| observe(event, &mut events))
            .unwrap();
        stream
            .advance(b"00p%begin 1 9 1\n%output %2 hi\\015\n", |event| {
                observe(event, &mut events)
            })
            .unwrap();

        assert_eq!(
            events,
            vec![
                Observed::Terminal(b"prompt".to_vec()),
                Observed::Entered,
                Observed::Begin(9),
            ]
        );
        assert_eq!(stream.mode(), ControlStreamMode::Control);
    }

    #[test]
    fn failed_entry_candidate_returns_every_terminal_byte() {
        let mut stream = ControlStream::new();
        let mut events = Vec::new();
        stream
            .advance(b"a\x1bP10", |event| observe(event, &mut events))
            .unwrap();
        stream
            .advance(b"Xb", |event| observe(event, &mut events))
            .unwrap();
        let joined = events
            .into_iter()
            .filter_map(|event| match event {
                Observed::Terminal(bytes) => Some(bytes),
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(joined, b"a\x1bP10Xb");
    }

    #[test]
    fn lines_and_exit_marker_can_span_reads() {
        let mut stream = ControlStream::new();
        let mut events = Vec::new();
        stream
            .advance(b"\x1bP1000p%output %5 hel", |event| {
                observe(event, &mut events)
            })
            .unwrap();
        stream
            .advance(
                b"lo\\033\nopaque\x1b\\text\n%future arg\n%exit\n\x1b",
                |event| observe(event, &mut events),
            )
            .unwrap();
        stream
            .advance(b"\\shell prompt", |event| observe(event, &mut events))
            .unwrap();

        assert_eq!(
            events,
            vec![
                Observed::Entered,
                Observed::Output(PaneId(5), b"hello\x1b".to_vec()),
                Observed::Other(b"future".to_vec(), b"arg".to_vec()),
                Observed::Exited,
                Observed::Terminal(b"shell prompt".to_vec()),
            ]
        );
        assert_eq!(stream.mode(), ControlStreamMode::Terminal);
    }
}
