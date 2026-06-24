// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use crate::xymodem::{NAK, WANT_CRC};
use crate::zmodem::{ZBIN, ZBIN32, ZDLE, ZHEX, ZPAD};

const DETECTOR_TAIL_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectedModemProtocol {
    Xmodem,
    XymodemNegotiation,
    Ymodem,
    Zmodem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetectedModemStart {
    pub protocol: DetectedModemProtocol,
    pub offset: usize,
}

#[derive(Debug, Default)]
pub struct ModemDetector {
    tail: Vec<u8>,
}

impl ModemDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scan(&mut self, chunk: &[u8]) -> Vec<DetectedModemProtocol> {
        self.scan_first(chunk)
            .map(|start| vec![start.protocol])
            .unwrap_or_default()
    }

    pub fn scan_first(&mut self, chunk: &[u8]) -> Option<DetectedModemStart> {
        if chunk.is_empty() {
            return None;
        }

        let mut window = Vec::with_capacity(self.tail.len() + chunk.len());
        window.extend_from_slice(&self.tail);
        let current_start = window.len();
        window.extend_from_slice(chunk);

        let zmodem_start = find_zmodem_start(&window, current_start);
        let xymodem_start = (zmodem_start.is_none())
            .then(|| detect_xymodem_start(&window, current_start))
            .flatten();

        let keep = DETECTOR_TAIL_BYTES.min(window.len());
        self.tail.clear();
        self.tail.extend_from_slice(&window[window.len() - keep..]);

        zmodem_start.or(xymodem_start).map(|mut start| {
            start.offset = start.offset.saturating_sub(current_start);
            start
        })
    }
}

fn find_zmodem_start(window: &[u8], current_start: usize) -> Option<DetectedModemStart> {
    let patterns: [&[u8]; 3] = [
        &[ZPAD, ZPAD, ZDLE, ZHEX],
        &[ZPAD, ZDLE, ZBIN],
        &[ZPAD, ZDLE, ZBIN32],
    ];
    patterns.iter().find_map(|pattern| {
        find_pattern_crossing_current_chunk(window, current_start, pattern).map(|offset| {
            DetectedModemStart {
                protocol: DetectedModemProtocol::Zmodem,
                offset,
            }
        })
    })
}

fn detect_xymodem_start(window: &[u8], current_start: usize) -> Option<DetectedModemStart> {
    // X/YMODEM data blocks are too small to identify safely without a prior
    // negotiation or explicit user action; serial boot noise can contain the
    // same SOH/STX + complement shape by chance.
    [WANT_CRC, NAK].iter().find_map(|byte| {
        find_pattern_crossing_current_chunk(window, current_start, &[*byte]).map(|offset| {
            DetectedModemStart {
                protocol: DetectedModemProtocol::XymodemNegotiation,
                offset,
            }
        })
    })
}

fn find_pattern_crossing_current_chunk(
    window: &[u8],
    current_start: usize,
    pattern: &[u8],
) -> Option<usize> {
    if pattern.is_empty() || pattern.len() > window.len() {
        return None;
    }

    for index in 0..=window.len() - pattern.len() {
        if &window[index..index + pattern.len()] == pattern && index + pattern.len() > current_start
        {
            return Some(index);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xymodem::{SOH, STX};

    #[test]
    fn detects_zmodem_header_across_chunks() {
        let mut detector = ModemDetector::new();
        assert!(detector.scan(&[b'n', ZPAD, ZPAD]).is_empty());
        assert_eq!(
            detector.scan(&[ZDLE, ZHEX, b'0']),
            vec![DetectedModemProtocol::Zmodem]
        );
    }

    #[test]
    fn detects_binary_zmodem_header() {
        let mut detector = ModemDetector::new();
        assert_eq!(
            detector.scan(&[ZPAD, ZDLE, ZBIN32]),
            vec![DetectedModemProtocol::Zmodem]
        );
    }

    #[test]
    fn detects_xymodem_start_signal() {
        let mut detector = ModemDetector::new();
        assert_eq!(
            detector.scan(&[WANT_CRC]),
            vec![DetectedModemProtocol::XymodemNegotiation]
        );
    }

    #[test]
    fn ignores_ymodem_data_block_without_negotiation() {
        let mut detector = ModemDetector::new();
        assert!(detector.scan(&[SOH, 0, 0xff, b'f']).is_empty());
    }

    #[test]
    fn ignores_xmodem_data_block_without_negotiation() {
        let mut detector = ModemDetector::new();
        assert!(detector.scan(&[SOH, 1, 0xfe, b'f']).is_empty());
        assert!(detector.scan(&[STX, 1, 0xfe, b'f']).is_empty());
    }

    #[test]
    fn does_not_reemit_old_match_on_unrelated_chunk() {
        let mut detector = ModemDetector::new();
        assert_eq!(
            detector.scan(&[ZPAD, ZDLE, ZBIN]),
            vec![DetectedModemProtocol::Zmodem]
        );
        assert!(detector.scan(b"ordinary output").is_empty());
    }
}
