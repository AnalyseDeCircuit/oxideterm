// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use unicode_segmentation::UnicodeSegmentation;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalSenderInputMode {
    Text,
    Hex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalSenderPacing {
    Line,
    Character,
}

pub enum TerminalSenderFrame {
    TextLine(Zeroizing<String>),
    TextChunk(Zeroizing<String>),
    RawBytes(Zeroizing<Vec<u8>>),
}

impl TerminalSenderFrame {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::TextLine(text) | Self::TextChunk(text) => Some(text),
            Self::RawBytes(_) => None,
        }
    }

    pub fn raw_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::RawBytes(bytes) => Some(bytes),
            Self::TextLine(_) | Self::TextChunk(_) => None,
        }
    }
}

impl fmt::Debug for TerminalSenderFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, byte_len) = match self {
            Self::TextLine(text) => ("text-line", text.len()),
            Self::TextChunk(text) => ("text-chunk", text.len()),
            Self::RawBytes(bytes) => ("raw-bytes", bytes.len()),
        };
        formatter
            .debug_struct("TerminalSenderFrame")
            .field("kind", &kind)
            .field("byte_len", &byte_len)
            .finish()
    }
}

pub struct TerminalSenderPlan {
    frames: Vec<TerminalSenderFrame>,
    repeat_count: u32,
    total_units: u64,
}

impl TerminalSenderPlan {
    pub fn frames(&self) -> &[TerminalSenderFrame] {
        &self.frames
    }

    pub fn repeat_count(&self) -> u32 {
        self.repeat_count
    }

    pub fn total_units(&self) -> u64 {
        self.total_units
    }
}

impl fmt::Debug for TerminalSenderPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TerminalSenderPlan")
            .field("frame_count", &self.frames.len())
            .field("repeat_count", &self.repeat_count)
            .field("total_units", &self.total_units)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalSenderPlanError {
    EmptyInput,
    EmptyHexInput,
    InvalidHexDigit,
    OddHexDigitCount,
    ZeroRepeatCount,
    UnitCountOverflow,
}

impl fmt::Display for TerminalSenderPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyInput => "sender input is empty",
            Self::EmptyHexInput => "hex sender input contains no bytes",
            Self::InvalidHexDigit => "hex sender input contains an invalid digit",
            Self::OddHexDigitCount => "hex sender input contains an odd number of digits",
            Self::ZeroRepeatCount => "sender repeat count must be at least one",
            Self::UnitCountOverflow => "sender unit count exceeds the supported range",
        })
    }
}

impl std::error::Error for TerminalSenderPlanError {}

pub fn build_terminal_sender_plan(
    input: Zeroizing<String>,
    input_mode: TerminalSenderInputMode,
    pacing: TerminalSenderPacing,
    repeat_count: u32,
) -> Result<TerminalSenderPlan, TerminalSenderPlanError> {
    if repeat_count == 0 {
        return Err(TerminalSenderPlanError::ZeroRepeatCount);
    }
    if input.is_empty() {
        return Err(TerminalSenderPlanError::EmptyInput);
    }
    // The normalized copy and every derived frame zeroize independently. This
    // keeps async sender captures bounded even when commands contain secrets.
    let frames = match input_mode {
        TerminalSenderInputMode::Text => build_text_frames(&input, pacing),
        TerminalSenderInputMode::Hex => build_hex_frames(&input, pacing)?,
    };
    if frames.is_empty() {
        return Err(match input_mode {
            TerminalSenderInputMode::Text => TerminalSenderPlanError::EmptyInput,
            TerminalSenderInputMode::Hex => TerminalSenderPlanError::EmptyHexInput,
        });
    }
    let frame_count =
        u64::try_from(frames.len()).map_err(|_| TerminalSenderPlanError::UnitCountOverflow)?;
    let total_units = checked_total_units(frame_count, repeat_count)?;

    Ok(TerminalSenderPlan {
        frames,
        repeat_count,
        total_units,
    })
}

fn checked_total_units(
    frame_count: u64,
    repeat_count: u32,
) -> Result<u64, TerminalSenderPlanError> {
    frame_count
        .checked_mul(u64::from(repeat_count))
        .ok_or(TerminalSenderPlanError::UnitCountOverflow)
}

fn build_text_frames(input: &str, pacing: TerminalSenderPacing) -> Vec<TerminalSenderFrame> {
    let normalized = Zeroizing::new(input.replace("\r\n", "\n").replace('\r', "\n"));
    match pacing {
        TerminalSenderPacing::Line => normalized
            .split_terminator('\n')
            .map(|line| TerminalSenderFrame::TextLine(Zeroizing::new(line.to_string())))
            .collect(),
        TerminalSenderPacing::Character => normalized
            .graphemes(true)
            .map(|grapheme| {
                TerminalSenderFrame::TextChunk(Zeroizing::new(if grapheme == "\n" {
                    "\r".to_string()
                } else {
                    grapheme.to_string()
                }))
            })
            .collect(),
    }
}

fn build_hex_frames(
    input: &str,
    pacing: TerminalSenderPacing,
) -> Result<Vec<TerminalSenderFrame>, TerminalSenderPlanError> {
    match pacing {
        TerminalSenderPacing::Line => {
            let normalized = Zeroizing::new(input.replace("\r\n", "\n").replace('\r', "\n"));
            let mut frames = Vec::new();
            for line in normalized.split('\n') {
                let bytes = parse_hex_bytes(line)?;
                if !bytes.is_empty() {
                    frames.push(TerminalSenderFrame::RawBytes(bytes));
                }
            }
            Ok(frames)
        }
        TerminalSenderPacing::Character => {
            let bytes = parse_hex_bytes(input)?;
            Ok(bytes
                .iter()
                .copied()
                .map(|byte| TerminalSenderFrame::RawBytes(Zeroizing::new(vec![byte])))
                .collect())
        }
    }
}

fn parse_hex_bytes(input: &str) -> Result<Zeroizing<Vec<u8>>, TerminalSenderPlanError> {
    let mut bytes = Zeroizing::new(Vec::new());
    for token in input.split(|character: char| {
        character.is_ascii_whitespace() || matches!(character, ',' | ':' | ';' | '-' | '_')
    }) {
        if token.is_empty() {
            continue;
        }
        let digits = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
            .unwrap_or(token);
        if digits.is_empty() {
            return Err(TerminalSenderPlanError::InvalidHexDigit);
        }
        if !digits.is_ascii() || !digits.as_bytes().iter().all(u8::is_ascii_hexdigit) {
            return Err(TerminalSenderPlanError::InvalidHexDigit);
        }
        if digits.len() % 2 != 0 {
            return Err(TerminalSenderPlanError::OddHexDigitCount);
        }
        for pair_start in (0..digits.len()).step_by(2) {
            let pair = &digits[pair_start..pair_start + 2];
            let byte = u8::from_str_radix(pair, 16)
                .map_err(|_| TerminalSenderPlanError::InvalidHexDigit)?;
            bytes.push(byte);
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(
        input: &str,
        mode: TerminalSenderInputMode,
        pacing: TerminalSenderPacing,
        repeat_count: u32,
    ) -> Result<TerminalSenderPlan, TerminalSenderPlanError> {
        build_terminal_sender_plan(
            Zeroizing::new(input.to_string()),
            mode,
            pacing,
            repeat_count,
        )
    }

    #[test]
    fn text_line_plan_normalizes_endings_and_preserves_empty_lines() {
        let plan = plan(
            "echo one\r\n\r\necho two\n",
            TerminalSenderInputMode::Text,
            TerminalSenderPacing::Line,
            2,
        )
        .unwrap();
        let lines = plan
            .frames()
            .iter()
            .map(|frame| frame.text().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(lines, vec!["echo one", "", "echo two"]);
        assert_eq!(plan.total_units(), 6);
    }

    #[test]
    fn text_character_plan_keeps_grapheme_clusters_together() {
        let plan = plan(
            "a\u{301}👨‍👩‍👧‍👦\n",
            TerminalSenderInputMode::Text,
            TerminalSenderPacing::Character,
            1,
        )
        .unwrap();
        let chunks = plan
            .frames()
            .iter()
            .map(|frame| frame.text().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(chunks, vec!["a\u{301}", "👨‍👩‍👧‍👦", "\r"]);
    }

    #[test]
    fn hex_line_plan_accepts_prefixes_separators_and_contiguous_digits() {
        let plan = plan(
            "0x0d, 0A\ndead-beef",
            TerminalSenderInputMode::Hex,
            TerminalSenderPacing::Line,
            1,
        )
        .unwrap();
        let frames = plan
            .frames()
            .iter()
            .map(|frame| frame.raw_bytes().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            frames,
            vec![&[0x0d, 0x0a][..], &[0xde, 0xad, 0xbe, 0xef][..]]
        );
    }

    #[test]
    fn hex_character_plan_emits_one_raw_byte_per_unit() {
        let plan = plan(
            "00 ff 7f",
            TerminalSenderInputMode::Hex,
            TerminalSenderPacing::Character,
            3,
        )
        .unwrap();
        let frames = plan
            .frames()
            .iter()
            .map(|frame| frame.raw_bytes().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(frames, vec![&[0x00][..], &[0xff][..], &[0x7f][..]]);
        assert_eq!(plan.total_units(), 9);
    }

    #[test]
    fn hex_plan_rejects_odd_and_invalid_digits_without_echoing_input() {
        assert_eq!(
            plan(
                "abc",
                TerminalSenderInputMode::Hex,
                TerminalSenderPacing::Line,
                1,
            )
            .unwrap_err(),
            TerminalSenderPlanError::OddHexDigitCount
        );
        assert_eq!(
            plan(
                "not-secret",
                TerminalSenderInputMode::Hex,
                TerminalSenderPacing::Line,
                1,
            )
            .unwrap_err(),
            TerminalSenderPlanError::InvalidHexDigit
        );
        assert!(
            !TerminalSenderPlanError::InvalidHexDigit
                .to_string()
                .contains("not-secret")
        );
        assert_eq!(
            plan(
                "aa 0x",
                TerminalSenderInputMode::Hex,
                TerminalSenderPacing::Line,
                1,
            )
            .unwrap_err(),
            TerminalSenderPlanError::InvalidHexDigit
        );
        assert_eq!(
            plan(
                " , ; ",
                TerminalSenderInputMode::Hex,
                TerminalSenderPacing::Line,
                1,
            )
            .unwrap_err(),
            TerminalSenderPlanError::EmptyHexInput
        );
    }

    #[test]
    fn line_and_character_pacing_preserve_the_same_logical_line_endings() {
        let input = "a\n\nb\n";
        let line_plan = plan(
            input,
            TerminalSenderInputMode::Text,
            TerminalSenderPacing::Line,
            1,
        )
        .unwrap();
        let line_bytes = line_plan
            .frames()
            .iter()
            .flat_map(|frame| {
                let mut bytes = frame.text().unwrap().as_bytes().to_vec();
                bytes.push(b'\r');
                bytes
            })
            .collect::<Vec<_>>();
        let character_bytes = plan(
            input,
            TerminalSenderInputMode::Text,
            TerminalSenderPacing::Character,
            1,
        )
        .unwrap()
        .frames()
        .iter()
        .flat_map(|frame| frame.text().unwrap().as_bytes().to_vec())
        .collect::<Vec<_>>();

        assert_eq!(line_bytes, b"a\r\rb\r");
        assert_eq!(line_bytes, character_bytes);
    }

    #[test]
    fn plan_rejects_zero_repeats_and_unit_count_overflow() {
        assert_eq!(
            plan(
                "echo one",
                TerminalSenderInputMode::Text,
                TerminalSenderPacing::Line,
                0,
            )
            .unwrap_err(),
            TerminalSenderPlanError::ZeroRepeatCount
        );
        assert_eq!(
            checked_total_units(u64::MAX, 2).unwrap_err(),
            TerminalSenderPlanError::UnitCountOverflow
        );
    }

    #[test]
    fn plan_debug_redacts_frame_contents() {
        let plan = plan(
            "token=super-secret",
            TerminalSenderInputMode::Text,
            TerminalSenderPacing::Line,
            1,
        )
        .unwrap();
        let debug = format!("{plan:?} {:?}", &plan.frames()[0]);

        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("frame_count"));
    }
}
