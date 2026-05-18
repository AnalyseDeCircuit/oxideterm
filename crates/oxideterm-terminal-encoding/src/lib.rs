// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::{borrow::Cow, fmt};

use encoding_rs::{BIG5, EUC_JP, EUC_KR, Encoding, GB18030, GBK, SHIFT_JIS, UTF_8, WINDOWS_1252};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TerminalEncoding {
    #[default]
    Utf8,
    Gbk,
    Gb18030,
    Big5,
    ShiftJis,
    EucJp,
    EucKr,
    Windows1252,
}

pub const TERMINAL_ENCODINGS: &[TerminalEncoding] = &[
    TerminalEncoding::Utf8,
    TerminalEncoding::Gbk,
    TerminalEncoding::Gb18030,
    TerminalEncoding::Big5,
    TerminalEncoding::ShiftJis,
    TerminalEncoding::EucJp,
    TerminalEncoding::EucKr,
    TerminalEncoding::Windows1252,
];

impl TerminalEncoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Gbk => "gbk",
            Self::Gb18030 => "gb18030",
            Self::Big5 => "big5",
            Self::ShiftJis => "shift_jis",
            Self::EucJp => "euc-jp",
            Self::EucKr => "euc-kr",
            Self::Windows1252 => "windows-1252",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Gbk => "GBK",
            Self::Gb18030 => "GB18030",
            Self::Big5 => "Big5",
            Self::ShiftJis => "Shift_JIS",
            Self::EucJp => "EUC-JP",
            Self::EucKr => "EUC-KR",
            Self::Windows1252 => "Windows-1252",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        let normalized = label.trim().to_ascii_lowercase().replace('_', "-");
        match normalized.as_str() {
            "utf-8" | "utf8" => Some(Self::Utf8),
            "gbk" => Some(Self::Gbk),
            "gb18030" => Some(Self::Gb18030),
            "big5" | "big-5" => Some(Self::Big5),
            "shift-jis" | "sjis" | "ms-kanji" => Some(Self::ShiftJis),
            "euc-jp" => Some(Self::EucJp),
            "euc-kr" => Some(Self::EucKr),
            "windows-1252" | "cp1252" => Some(Self::Windows1252),
            _ => None,
        }
    }

    pub fn encoding_rs(self) -> &'static Encoding {
        match self {
            Self::Utf8 => UTF_8,
            Self::Gbk => GBK,
            Self::Gb18030 => GB18030,
            Self::Big5 => BIG5,
            Self::ShiftJis => SHIFT_JIS,
            Self::EucJp => EUC_JP,
            Self::EucKr => EUC_KR,
            Self::Windows1252 => WINDOWS_1252,
        }
    }

    pub fn is_utf8(self) -> bool {
        self == Self::Utf8
    }
}

impl fmt::Display for TerminalEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

pub struct TerminalOutputDecoder {
    encoding: TerminalEncoding,
    decoder: Option<encoding_rs::Decoder>,
}

impl TerminalOutputDecoder {
    pub fn new(encoding: TerminalEncoding) -> Self {
        let decoder = (!encoding.is_utf8()).then(|| encoding.encoding_rs().new_decoder());
        Self { encoding, decoder }
    }

    pub fn encoding(&self) -> TerminalEncoding {
        self.encoding
    }

    pub fn set_encoding(&mut self, encoding: TerminalEncoding) {
        if self.encoding != encoding {
            *self = Self::new(encoding);
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.encoding);
    }

    pub fn decode_to_utf8_bytes<'a>(&mut self, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        if bytes.is_empty() || self.encoding.is_utf8() {
            return Cow::Borrowed(bytes);
        }

        let decoder = self
            .decoder
            .get_or_insert_with(|| self.encoding.encoding_rs().new_decoder());
        let mut output = String::new();
        let reserve = decoder
            .max_utf8_buffer_length(bytes.len())
            .unwrap_or(bytes.len().saturating_mul(3).saturating_add(16));
        output.reserve(reserve);
        let (_result, _read, _replaced) = decoder.decode_to_string(bytes, &mut output, false);
        Cow::Owned(output.into_bytes())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TerminalInputEncoder {
    encoding: TerminalEncoding,
}

impl TerminalInputEncoder {
    pub fn new(encoding: TerminalEncoding) -> Self {
        Self { encoding }
    }

    pub fn encoding(&self) -> TerminalEncoding {
        self.encoding
    }

    pub fn set_encoding(&mut self, encoding: TerminalEncoding) {
        self.encoding = encoding;
    }

    pub fn encode_text(self, text: &str) -> Cow<'_, [u8]> {
        if self.encoding.is_utf8() || text.is_ascii() {
            return Cow::Borrowed(text.as_bytes());
        }

        let (bytes, _encoding, _had_errors) = self.encoding.encoding_rs().encode(text);
        bytes
    }

    pub fn encode_paste(self, text: &str, bracketed: bool) -> Vec<u8> {
        let prepared = normalize_paste_line_endings(text);

        if !bracketed || !prepared.contains('\r') {
            let encoded = self.encode_text(&prepared);
            return encoded.into_owned();
        }

        let sanitized = prepared.replace('\x1b', "");
        let encoded = self.encode_text(&sanitized);
        let mut bytes = Vec::with_capacity(encoded.len() + 12);
        bytes.extend_from_slice(b"\x1b[200~");
        bytes.extend_from_slice(&encoded);
        bytes.extend_from_slice(b"\x1b[201~");
        bytes
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EncodingHint {
    pub suggestions: Vec<TerminalEncoding>,
    pub invalid_bytes: usize,
    pub sample_bytes: usize,
    pub invalid_ratio: f32,
}

#[derive(Debug)]
pub struct EncodingMismatchDetector {
    enabled: bool,
    observed_bytes: usize,
    sample: Vec<u8>,
    invalid_bytes: usize,
    emitted: bool,
}

impl EncodingMismatchDetector {
    pub const MAX_SAMPLE_BYTES: usize = 2048;
    const MIN_OBSERVED_BYTES: usize = 128;
    const MIN_SAMPLE_BYTES: usize = 64;
    const MIN_INVALID_BYTES: usize = 4;
    const MIN_INVALID_RATIO: f32 = 0.015;

    pub fn new(encoding: TerminalEncoding) -> Self {
        Self {
            enabled: encoding.is_utf8(),
            observed_bytes: 0,
            sample: Vec::new(),
            invalid_bytes: 0,
            emitted: false,
        }
    }

    pub fn set_encoding(&mut self, encoding: TerminalEncoding) {
        *self = Self::new(encoding);
    }

    pub fn observe(&mut self, bytes: &[u8]) -> Option<EncodingHint> {
        if !self.enabled || self.emitted || bytes.is_empty() {
            return None;
        }

        self.observed_bytes = self.observed_bytes.saturating_add(bytes.len());
        self.invalid_bytes = self.invalid_bytes.saturating_add(invalid_utf8_bytes(bytes));
        let remaining = Self::MAX_SAMPLE_BYTES.saturating_sub(self.sample.len());
        self.sample
            .extend_from_slice(&bytes[..bytes.len().min(remaining)]);

        if self.observed_bytes < Self::MIN_OBSERVED_BYTES
            || self.sample.len() < Self::MIN_SAMPLE_BYTES
            || self.invalid_bytes < Self::MIN_INVALID_BYTES
        {
            return None;
        }

        let ratio = self.invalid_bytes as f32 / self.observed_bytes.max(1) as f32;
        if ratio < Self::MIN_INVALID_RATIO {
            return None;
        }

        let suggestions = ranked_legacy_suggestions(&self.sample);
        if suggestions.is_empty() {
            return None;
        }

        self.emitted = true;
        Some(EncodingHint {
            suggestions,
            invalid_bytes: self.invalid_bytes,
            sample_bytes: self.sample.len(),
            invalid_ratio: ratio,
        })
    }
}

fn ranked_legacy_suggestions(sample: &[u8]) -> Vec<TerminalEncoding> {
    let mut scored = TERMINAL_ENCODINGS
        .iter()
        .copied()
        .filter(|encoding| !encoding.is_utf8())
        .map(|encoding| (score_decoded_text(encoding, sample), encoding))
        .filter(|(score, _encoding)| *score > 0)
        .collect::<Vec<_>>();
    scored.sort_by(|(left, _), (right, _)| right.cmp(left));
    scored
        .into_iter()
        .take(3)
        .map(|(_, encoding)| encoding)
        .collect()
}

fn score_decoded_text(encoding: TerminalEncoding, sample: &[u8]) -> i32 {
    let (decoded, _used_encoding, had_errors) = encoding.encoding_rs().decode(sample);
    let mut score = if had_errors { -20 } else { 0 };
    for ch in decoded.chars() {
        if ch == '\u{fffd}' {
            score -= 10;
        } else if ch.is_control() && !matches!(ch, '\n' | '\r' | '\t' | '\x1b') {
            score -= 2;
        } else if ch.is_alphanumeric() || ch.is_whitespace() {
            score += 2;
        } else if !ch.is_ascii() {
            score += 3;
        } else {
            score += 1;
        }
    }
    score
}

fn invalid_utf8_bytes(bytes: &[u8]) -> usize {
    let mut index = 0;
    let mut invalid = 0;
    while index < bytes.len() {
        match std::str::from_utf8(&bytes[index..]) {
            Ok(_) => break,
            Err(error) => {
                index += error.valid_up_to();
                match error.error_len() {
                    Some(len) => {
                        invalid += len;
                        index += len;
                    }
                    None => {
                        invalid += bytes.len().saturating_sub(index);
                        break;
                    }
                }
            }
        }
    }
    invalid
}

fn normalize_paste_line_endings(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                normalized.push('\r');
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            '\n' => normalized.push('\r'),
            _ => normalized.push(ch),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_and_ascii_fast_paths_do_not_allocate() {
        let mut decoder = TerminalOutputDecoder::new(TerminalEncoding::Utf8);
        assert!(matches!(
            decoder.decode_to_utf8_bytes(b"\x1b[Ahello"),
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            TerminalInputEncoder::new(TerminalEncoding::Gbk).encode_text("ascii"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn legacy_encodings_round_trip_sample_text() {
        let samples = [
            (TerminalEncoding::Gbk, "你好"),
            (TerminalEncoding::Gb18030, "你好𠀀"),
            (TerminalEncoding::Big5, "繁體中文"),
            (TerminalEncoding::ShiftJis, "こんにちは"),
            (TerminalEncoding::EucJp, "こんにちは"),
            (TerminalEncoding::EucKr, "한국어"),
            (TerminalEncoding::Windows1252, "café"),
        ];

        for (encoding, text) in samples {
            let encoded = TerminalInputEncoder::new(encoding)
                .encode_text(text)
                .into_owned();
            let mut decoder = TerminalOutputDecoder::new(encoding);
            let decoded = decoder.decode_to_utf8_bytes(&encoded).into_owned();
            assert_eq!(String::from_utf8(decoded).unwrap(), text, "{encoding}");
        }
    }

    #[test]
    fn streaming_decoder_preserves_split_multibyte_legacy_character() {
        let encoded = TerminalInputEncoder::new(TerminalEncoding::Gbk)
            .encode_text("你好")
            .into_owned();
        let mut decoder = TerminalOutputDecoder::new(TerminalEncoding::Gbk);
        let first = decoder.decode_to_utf8_bytes(&encoded[..1]).into_owned();
        let second = decoder.decode_to_utf8_bytes(&encoded[1..]).into_owned();
        assert_eq!(String::from_utf8([first, second].concat()).unwrap(), "你好");
    }

    #[test]
    fn ascii_control_bytes_survive_legacy_decode() {
        let raw = b"\x1b[31mred\x07\x1b[0m\r\n";
        let mut decoder = TerminalOutputDecoder::new(TerminalEncoding::ShiftJis);
        assert_eq!(decoder.decode_to_utf8_bytes(raw).as_ref(), raw);
    }

    #[test]
    fn bracketed_paste_wraps_raw_protocol_and_encodes_only_content() {
        let encoded =
            TerminalInputEncoder::new(TerminalEncoding::Gbk).encode_paste("你好\n世界\x1b", true);
        assert!(encoded.starts_with(b"\x1b[200~"));
        assert!(encoded.ends_with(b"\x1b[201~"));
        assert!(!encoded[6..encoded.len() - 6].contains(&0x1b));

        let mut decoder = TerminalOutputDecoder::new(TerminalEncoding::Gbk);
        let body = &encoded[6..encoded.len() - 6];
        assert_eq!(
            String::from_utf8(decoder.decode_to_utf8_bytes(body).into_owned()).unwrap(),
            "你好\r世界"
        );
    }

    #[test]
    fn paste_converts_line_endings_to_terminal_cr_without_bracketed_mode() {
        let encoded =
            TerminalInputEncoder::new(TerminalEncoding::Utf8).encode_paste("line 1\nline 2", false);

        assert_eq!(encoded, b"line 1\rline 2");
    }

    #[test]
    fn bracketed_paste_converts_line_endings_to_terminal_cr() {
        let encoded = TerminalInputEncoder::new(TerminalEncoding::Utf8)
            .encode_paste("line 1\r\nline 2\nline 3", true);

        assert_eq!(encoded, b"\x1b[200~line 1\rline 2\rline 3\x1b[201~");
    }

    #[test]
    fn bracketed_paste_leaves_single_line_unwrapped_like_tauri() {
        let encoded = TerminalInputEncoder::new(TerminalEncoding::Utf8).encode_paste("pwd", true);

        assert_eq!(encoded, b"pwd");
    }

    #[test]
    fn mismatch_detector_suggests_legacy_encoding_for_invalid_utf8() {
        let encoded = TerminalInputEncoder::new(TerminalEncoding::Gbk)
            .encode_text("你好世界你好世界你好世界")
            .into_owned();
        let mut detector = EncodingMismatchDetector::new(TerminalEncoding::Utf8);
        let hint = detector.observe(&encoded.repeat(8)).unwrap();
        assert!(hint.suggestions.contains(&TerminalEncoding::Gbk));
        assert!(hint.invalid_bytes >= 4);
    }

    #[test]
    fn mismatch_detector_disabled_for_non_utf8_mode() {
        let encoded = TerminalInputEncoder::new(TerminalEncoding::Gbk)
            .encode_text("你好世界")
            .into_owned();
        let mut detector = EncodingMismatchDetector::new(TerminalEncoding::Gbk);
        assert!(detector.observe(&encoded.repeat(16)).is_none());
    }
}
