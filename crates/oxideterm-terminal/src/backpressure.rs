// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

pub(crate) const LOCAL_PTY_READ_BUFFER_BYTES: usize = 8 * 1024;
pub(crate) const LOCAL_MAX_LOCKED_PARSE_BYTES: usize = 64 * 1024;
pub(crate) const MAGIC_DETECT_OVERLAP_BYTES: usize = 128;
pub(crate) const UTF8_RESIDUAL_MAX_BYTES: usize = 4;

pub const NATIVE_INTERACTIVE_DRAIN_BYTES: usize = 32 * 1024;
pub const NATIVE_NORMAL_DRAIN_BYTES: usize = 128 * 1024;
pub const NATIVE_THROUGHPUT_DRAIN_BYTES: usize = 256 * 1024;

const DEFAULT_MAX_EVENTS_PER_DRAIN: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalDrainBudget {
    pub max_bytes: usize,
    pub max_events: usize,
}

impl TerminalDrainBudget {
    pub const fn new(max_bytes: usize, max_events: usize) -> Self {
        Self {
            max_bytes,
            max_events,
        }
    }

    pub const fn interactive() -> Self {
        Self::new(NATIVE_INTERACTIVE_DRAIN_BYTES, DEFAULT_MAX_EVENTS_PER_DRAIN)
    }

    pub const fn normal() -> Self {
        Self::new(NATIVE_NORMAL_DRAIN_BYTES, DEFAULT_MAX_EVENTS_PER_DRAIN)
    }

    pub const fn throughput() -> Self {
        Self::new(NATIVE_THROUGHPUT_DRAIN_BYTES, DEFAULT_MAX_EVENTS_PER_DRAIN)
    }

    pub const fn unlimited() -> Self {
        Self::new(usize::MAX, usize::MAX)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalDrainReport {
    pub changed: bool,
    pub drained_bytes: usize,
    pub pending_bytes: usize,
    pub events_drained: usize,
    pub drain_duration: Duration,
    pub budget_exhausted: bool,
}

impl TerminalDrainReport {
    pub fn mark_changed(&mut self) {
        self.changed = true;
    }

    pub fn combine(&mut self, other: TerminalDrainReport) {
        self.changed |= other.changed;
        self.drained_bytes = self.drained_bytes.saturating_add(other.drained_bytes);
        self.pending_bytes = self.pending_bytes.saturating_add(other.pending_bytes);
        self.events_drained = self.events_drained.saturating_add(other.events_drained);
        self.drain_duration += other.drain_duration;
        self.budget_exhausted |= other.budget_exhausted;
    }
}

#[derive(Debug, Default)]
pub(crate) struct Utf8ResidualGuard {
    residual: Vec<u8>,
}

impl Utf8ResidualGuard {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Option<Vec<u8>> {
        if bytes.is_empty() && self.residual.is_empty() {
            return None;
        }

        let mut combined = Vec::with_capacity(self.residual.len() + bytes.len());
        combined.extend_from_slice(&self.residual);
        combined.extend_from_slice(bytes);
        self.residual.clear();

        let split = split_before_incomplete_utf8_tail(&combined);
        if split < combined.len() {
            self.residual.extend_from_slice(&combined[split..]);
            combined.truncate(split);
        }

        if self.residual.len() >= UTF8_RESIDUAL_MAX_BYTES {
            combined.extend_from_slice(&self.residual);
            self.residual.clear();
        }

        (!combined.is_empty()).then_some(combined)
    }

    pub(crate) fn flush(&mut self) -> Option<Vec<u8>> {
        (!self.residual.is_empty()).then(|| std::mem::take(&mut self.residual))
    }
}

fn split_before_incomplete_utf8_tail(bytes: &[u8]) -> usize {
    let len = bytes.len();
    let max_tail = len.min(UTF8_RESIDUAL_MAX_BYTES - 1);

    for tail_len in 1..=max_tail {
        let start = len - tail_len;
        let first = bytes[start];
        let width = utf8_char_width(first);
        if width == 0 {
            continue;
        }

        if width > tail_len
            && bytes[start + 1..]
                .iter()
                .all(|byte| is_utf8_continuation(*byte))
        {
            return start;
        }

        break;
    }

    len
}

fn utf8_char_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 0,
    }
}

fn is_utf8_continuation(byte: u8) -> bool {
    (0x80..=0xbf).contains(&byte)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalMagicKind {
    TrzszTransfer,
}

impl TerminalMagicKind {
    const fn marker(self) -> &'static [u8] {
        match self {
            Self::TrzszTransfer => b"::TRZSZ:TRANSFER:",
        }
    }
}

#[derive(Debug)]
pub(crate) struct MagicScanWindow {
    tail: Vec<u8>,
    patterns: Vec<TerminalMagicKind>,
}

impl Default for MagicScanWindow {
    fn default() -> Self {
        Self {
            tail: Vec::new(),
            patterns: vec![TerminalMagicKind::TrzszTransfer],
        }
    }
}

impl MagicScanWindow {
    pub(crate) fn scan(&mut self, chunk: &[u8]) -> Vec<TerminalMagicKind> {
        if chunk.is_empty() {
            return Vec::new();
        }

        let mut window = Vec::with_capacity(self.tail.len() + chunk.len());
        window.extend_from_slice(&self.tail);
        window.extend_from_slice(chunk);
        let current_start = self.tail.len();
        let mut matches = Vec::new();

        for kind in &self.patterns {
            let marker = kind.marker();
            if marker.is_empty() || marker.len() > window.len() {
                continue;
            }

            for index in 0..=window.len() - marker.len() {
                if &window[index..index + marker.len()] == marker
                    && index + marker.len() > current_start
                {
                    matches.push(*kind);
                    break;
                }
            }
        }

        let keep = MAGIC_DETECT_OVERLAP_BYTES.min(window.len());
        self.tail.clear();
        self.tail.extend_from_slice(&window[window.len() - keep..]);
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_guard_keeps_incomplete_tail() {
        let mut guard = Utf8ResidualGuard::default();
        assert_eq!(guard.push(&[0xe4, 0xbd]), None);
        assert_eq!(guard.push(&[0xa0]), Some("你".as_bytes().to_vec()));
    }

    #[test]
    fn utf8_guard_flushes_invalid_bytes_unchanged() {
        let mut guard = Utf8ResidualGuard::default();
        assert_eq!(guard.push(&[0xff, b'a']), Some(vec![0xff, b'a']));
    }

    #[test]
    fn utf8_guard_does_not_split_emoji_tail() {
        let mut guard = Utf8ResidualGuard::default();
        assert_eq!(guard.push(&[0xf0, 0x9f, 0x98]), None);
        assert_eq!(guard.push(&[0x80]), Some("😀".as_bytes().to_vec()));
    }

    #[test]
    fn utf8_guard_flushes_residual_on_stream_end() {
        let mut guard = Utf8ResidualGuard::default();
        assert_eq!(guard.push(&[0xe4, 0xbd]), None);
        assert_eq!(guard.flush(), Some(vec![0xe4, 0xbd]));
        assert_eq!(guard.flush(), None);
    }

    #[test]
    fn magic_scan_detects_split_pattern_once() {
        let mut scan = MagicScanWindow::default();
        assert!(scan.scan(b"abc::TRZSZ:").is_empty());
        assert_eq!(scan.scan(b"TRANSFER:R:1").len(), 1);
        assert!(scan.scan(b"ordinary output").is_empty());
    }
}
