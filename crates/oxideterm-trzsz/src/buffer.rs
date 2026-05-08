// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

use crate::TrzszError;

#[derive(Debug, Clone, Default)]
pub struct TrzszBuffer {
    inner: Arc<BufferInner>,
}

#[derive(Debug, Default)]
struct BufferInner {
    state: Mutex<BufferState>,
    notify: Condvar,
}

#[derive(Debug, Default)]
struct BufferState {
    queue: VecDeque<Vec<u8>>,
    next_buffer: Vec<u8>,
    next_index: usize,
    stopped: bool,
}

impl TrzszBuffer {
    pub fn add_buffer(&self, buffer: impl AsRef<[u8]>) {
        let mut state = self.inner.state.lock().expect("trzsz buffer mutex");
        state.queue.push_back(buffer.as_ref().to_vec());
        self.inner.notify.notify_all();
    }

    pub fn stop_buffer(&self) {
        let mut state = self.inner.state.lock().expect("trzsz buffer mutex");
        state.stopped = true;
        self.inner.notify.notify_all();
    }

    pub fn drain_buffer(&self) {
        let mut state = self.inner.state.lock().expect("trzsz buffer mutex");
        state.queue.clear();
        state.next_buffer.clear();
        state.next_index = 0;
    }

    pub fn read_line(&self) -> Result<String, TrzszError> {
        let mut buffer = Vec::new();
        loop {
            let mut next = self.next_buffer()?;
            if let Some(newline_index) = next.iter().position(|value| *value == b'\n') {
                self.consume(newline_index + 1);
                next.truncate(newline_index);
            } else {
                let len = next.len();
                self.consume(len);
            }

            if next.contains(&0x03) {
                return Err(TrzszError::InvalidState("Interrupted".to_string()));
            }
            buffer.extend_from_slice(&next);
            if self.previous_chunk_had_newline() {
                return Ok(String::from_utf8_lossy(&buffer).into_owned());
            }
        }
    }

    pub fn read_binary(&self, length: usize) -> Result<Vec<u8>, TrzszError> {
        let mut buffer = Vec::with_capacity(length);
        while buffer.len() < length {
            let remaining = length - buffer.len();
            let mut next = self.next_buffer()?;
            if next.len() > remaining {
                next.truncate(remaining);
            }
            self.consume(next.len());
            buffer.extend_from_slice(&next);
        }
        Ok(buffer)
    }

    pub fn read_line_on_windows(&self) -> Result<String, TrzszError> {
        let mut buffer = Vec::new();
        let mut last_byte: u8 = 0x1b;
        let mut skip_vt100 = false;
        let mut has_newline = false;
        let mut may_duplicate = false;
        let mut has_cursor_home = false;
        let mut previous_has_cursor_home = false;

        loop {
            let mut next = self.next_buffer()?;
            let had_bang = if let Some(newline_index) = next.iter().position(|value| *value == b'!')
            {
                self.consume(newline_index + 1);
                next.truncate(newline_index);
                true
            } else {
                let len = next.len();
                self.consume(len);
                false
            };

            for char_code in next {
                if char_code == 0x03 {
                    return Err(TrzszError::InvalidState("Interrupted".to_string()));
                }
                if char_code == b'\n' {
                    has_newline = true;
                }

                if skip_vt100 {
                    if is_vt100_end(char_code) {
                        skip_vt100 = false;
                        if char_code == b'H' && last_byte.is_ascii_digit() {
                            may_duplicate = true;
                        }
                    }
                    if last_byte == b'[' && char_code == b'H' {
                        has_cursor_home = true;
                    }
                    last_byte = char_code;
                    continue;
                }

                if char_code == 0x1b {
                    skip_vt100 = true;
                    last_byte = char_code;
                    continue;
                }

                if !is_trzsz_letter(char_code) {
                    continue;
                }

                if may_duplicate {
                    may_duplicate = false;
                    if has_newline
                        && !buffer.is_empty()
                        && (char_code == *buffer.last().expect("non-empty")
                            || previous_has_cursor_home)
                    {
                        *buffer.last_mut().expect("non-empty") = char_code;
                        continue;
                    }
                }

                buffer.push(char_code);
                previous_has_cursor_home = has_cursor_home;
                has_cursor_home = false;
                has_newline = false;
            }

            if had_bang && !buffer.is_empty() && !skip_vt100 {
                return Ok(String::from_utf8_lossy(&buffer).into_owned());
            }
        }
    }

    fn next_buffer(&self) -> Result<Vec<u8>, TrzszError> {
        let mut state = self.inner.state.lock().expect("trzsz buffer mutex");
        loop {
            if state.next_index < state.next_buffer.len() {
                return Ok(state.next_buffer[state.next_index..].to_vec());
            }

            if state.stopped {
                return Err(TrzszError::InvalidState("Stopped".to_string()));
            }

            if let Some(next) = state.queue.pop_front() {
                state.next_buffer = next;
                state.next_index = 0;
                continue;
            }

            state = self.inner.notify.wait(state).expect("trzsz buffer condvar");
        }
    }

    fn consume(&self, length: usize) {
        let mut state = self.inner.state.lock().expect("trzsz buffer mutex");
        state.next_index = state.next_index.saturating_add(length);
    }

    fn previous_chunk_had_newline(&self) -> bool {
        let state = self.inner.state.lock().expect("trzsz buffer mutex");
        state.next_index > 0
            && state
                .next_buffer
                .get(state.next_index.saturating_sub(1))
                .is_some_and(|value| *value == b'\n')
    }
}

fn is_trzsz_letter(char_code: u8) -> bool {
    char_code.is_ascii_alphanumeric() || matches!(char_code, b'#' | b':' | b'+' | b'/' | b'=')
}

fn is_vt100_end(char_code: u8) -> bool {
    char_code.is_ascii_alphabetic()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_line_across_chunks_and_keeps_remainder() {
        let buffer = TrzszBuffer::default();
        buffer.add_buffer(b"#NUM:1");
        buffer.add_buffer(b"2\n#SUCC:12\n");
        assert_eq!(buffer.read_line().unwrap(), "#NUM:12");
        assert_eq!(buffer.read_line().unwrap(), "#SUCC:12");
    }

    #[test]
    fn reads_binary_without_losing_remainder() {
        let buffer = TrzszBuffer::default();
        buffer.add_buffer(b"abcdef");
        assert_eq!(buffer.read_binary(4).unwrap(), b"abcd");
        assert_eq!(buffer.read_binary(2).unwrap(), b"ef");
    }

    #[test]
    fn stop_unblocks_waiters() {
        let buffer = TrzszBuffer::default();
        buffer.stop_buffer();
        assert!(buffer.read_line().is_err());
    }
}
