// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::{borrow::Cow, fmt};

use encoding_rs::{
    BIG5, EUC_JP, EUC_KR, EncoderResult, Encoding, GB18030, GBK, SHIFT_JIS, UTF_8, WINDOWS_1252,
};
use zeroize::{Zeroize, Zeroizing};

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

        Cow::Owned(encode_without_html_references(
            self.encoding.encoding_rs(),
            text,
        ))
    }

    pub fn encode_paste(self, text: &str, bracketed: bool) -> Vec<u8> {
        let prepared = prepare_paste_text(text, bracketed);
        let mut encoded = self.encode_text(&prepared);

        if !bracketed {
            return encoded.into_owned();
        }

        // Every paste needs boundaries so applications can distinguish text from key events.
        let mut bytes = Vec::with_capacity(encoded.len() + 12);
        bytes.extend_from_slice(b"\x1b[200~");
        bytes.extend_from_slice(&encoded);
        bytes.extend_from_slice(b"\x1b[201~");
        // Legacy encodings may allocate a second temporary copy of clipboard text.
        if let Cow::Owned(encoded) = &mut encoded {
            encoded.zeroize();
        }
        bytes
    }
}

/// Encodes `text`, sending `?` for a character that `encoding` can't represent.
///
/// `Encoding::encode` follows HTML form submission and replaces such a
/// character with a numeric character reference like `&#128512;`, which a
/// remote shell would receive as typed text. The buffer is sized for the
/// worst case up front so pasted secrets are not left behind by reallocation.
fn encode_without_html_references(encoding: &'static Encoding, text: &str) -> Vec<u8> {
    let mut encoder = encoding.new_encoder();
    let worst_case = encoder
        .max_buffer_length_from_utf8_without_replacement(text.len())
        .unwrap_or(text.len().saturating_mul(4));
    let mut bytes = Vec::with_capacity(worst_case);
    let mut remaining = text;
    loop {
        let (result, read) =
            encoder.encode_from_utf8_to_vec_without_replacement(remaining, &mut bytes, true);
        remaining = &remaining[read..];
        match result {
            EncoderResult::InputEmpty => return bytes,
            EncoderResult::Unmappable(_) => bytes.push(b'?'),
            EncoderResult::OutputFull => bytes.reserve(remaining.len().saturating_mul(4)),
        }
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
    // The start of a UTF-8 sequence that the last read cut off, completed by
    // the next one before it is judged.
    incomplete: [u8; 3],
    incomplete_len: usize,
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
            incomplete: [0; 3],
            incomplete_len: 0,
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
        let invalid = self.invalid_utf8_bytes(bytes);
        self.invalid_bytes = self.invalid_bytes.saturating_add(invalid);
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

impl EncodingMismatchDetector {
    /// Counts the bytes of `bytes` that are not UTF-8, treating a sequence
    /// split across reads as the whole sequence.
    fn invalid_utf8_bytes(&mut self, mut bytes: &[u8]) -> usize {
        let mut invalid = 0;
        if self.incomplete_len != 0 {
            let pending = self.incomplete_len;
            let width = match self.incomplete[0] {
                0xf0..=0xff => 4,
                0xe0..=0xef => 3,
                _ => 2,
            };
            let taken = (width - pending).min(bytes.len());
            let mut joined = [0; 4];
            joined[..pending].copy_from_slice(&self.incomplete[..pending]);
            joined[pending..pending + taken].copy_from_slice(&bytes[..taken]);
            let joined = &joined[..pending + taken];
            self.incomplete_len = 0;
            match std::str::from_utf8(joined) {
                Ok(_) => bytes = &bytes[taken..],
                Err(error) => match error.error_len() {
                    // Still cut off: `taken` used up this read.
                    None => {
                        self.incomplete[..joined.len()].copy_from_slice(joined);
                        self.incomplete_len = joined.len();
                        return 0;
                    }
                    Some(len) => {
                        invalid += len;
                        bytes = &bytes[len.saturating_sub(pending)..];
                    }
                },
            }
        }

        let mut index = 0;
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
                            let tail = &bytes[index..];
                            self.incomplete[..tail.len()].copy_from_slice(tail);
                            self.incomplete_len = tail.len();
                            break;
                        }
                    }
                }
            }
        }
        invalid
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

fn prepare_paste_text(text: &str, bracketed: bool) -> Zeroizing<String> {
    let mut normalized = Zeroizing::new(String::with_capacity(text.len()));
    // Bracketed content is text; plain terminal input retains shell submission semantics.
    let newline = if bracketed { '\n' } else { '\r' };
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                normalized.push(newline);
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            '\n' => normalized.push(newline),
            // ESC can terminate the paste, and Ctrl-C can interrupt its application-side reader.
            '\x1b' | '\x03' if bracketed => {}
            _ => normalized.push(ch),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_controls_and_fast_path_borrowing_are_preserved() {
        let raw = b"\x1b[31mred\x07\x1b[0m\r\n";
        for encoding in [
            TerminalEncoding::Utf8,
            TerminalEncoding::Gbk,
            TerminalEncoding::ShiftJis,
        ] {
            let mut decoder = TerminalOutputDecoder::new(encoding);
            let decoded = decoder.decode_to_utf8_bytes(raw);
            if encoding == TerminalEncoding::Utf8 {
                assert!(matches!(decoded, Cow::Borrowed(_)));
            }
            assert_eq!(decoded.as_ref(), raw);
        }
        let encoded = TerminalInputEncoder::new(TerminalEncoding::Gbk).encode_text("ascii");
        assert!(matches!(encoded, Cow::Borrowed(_)));
        assert_eq!(encoded.as_ref(), b"ascii");
    }

    #[test]
    fn legacy_encodings_match_wire_bytes_and_decode_every_chunk_split() {
        let samples: &[(TerminalEncoding, &str, &[u8])] = &[
            (TerminalEncoding::Gbk, "你好", b"\xc4\xe3\xba\xc3"),
            (
                TerminalEncoding::Gb18030,
                "你好𠀀",
                b"\xc4\xe3\xba\xc3\x95\x32\x82\x36",
            ),
            (
                TerminalEncoding::Big5,
                "繁體中文",
                b"\xc1\x63\xc5\xe9\xa4\xa4\xa4\xe5",
            ),
            (
                TerminalEncoding::ShiftJis,
                "こんにちは",
                b"\x82\xb1\x82\xf1\x82\xc9\x82\xbf\x82\xcd",
            ),
            (
                TerminalEncoding::EucJp,
                "こんにちは",
                b"\xa4\xb3\xa4\xf3\xa4\xcb\xa4\xc1\xa4\xcf",
            ),
            (
                TerminalEncoding::EucKr,
                "한국어",
                b"\xc7\xd1\xb1\xb9\xbe\xee",
            ),
            (TerminalEncoding::Windows1252, "café", b"caf\xe9"),
        ];
        for &(encoding, text, wire) in samples {
            assert_eq!(
                TerminalInputEncoder::new(encoding)
                    .encode_text(text)
                    .as_ref(),
                wire,
                "{encoding}"
            );
            for split in 0..=wire.len() {
                let mut decoder = TerminalOutputDecoder::new(encoding);
                let mut decoded = decoder.decode_to_utf8_bytes(&wire[..split]).into_owned();
                decoded.extend_from_slice(&decoder.decode_to_utf8_bytes(&wire[split..]));
                assert_eq!(decoded, text.as_bytes(), "{encoding}, split {split}");
            }
        }
    }

    #[test]
    fn unmappable_input_becomes_a_question_mark_instead_of_an_html_reference() {
        // encoding_rs's `Encoding::encode` follows HTML form submission and
        // would send `&#128512;` to the remote shell.
        let samples: &[(TerminalEncoding, &str, &[u8])] = &[
            (TerminalEncoding::Gbk, "a😀你", b"a?\xc4\xe3"),
            (TerminalEncoding::Big5, "é體", b"?\xc5\xe9"),
            (TerminalEncoding::ShiftJis, "한こ", b"?\x82\xb1"),
            (TerminalEncoding::EucKr, "😀한", b"?\xc7\xd1"),
            (TerminalEncoding::Windows1252, "中é€", b"?\xe9\x80"),
            // GB18030 maps every character, so nothing is replaced.
            (TerminalEncoding::Gb18030, "😀", b"\x94\x39\xfc\x36"),
        ];
        for &(encoding, text, wire) in samples {
            let encoder = TerminalInputEncoder::new(encoding);
            assert_eq!(encoder.encode_text(text).as_ref(), wire, "{encoding}");
            let mut pasted = b"\x1b[200~".to_vec();
            pasted.extend_from_slice(wire);
            pasted.extend_from_slice(b"\x1b[201~");
            assert_eq!(encoder.encode_paste(text, true), pasted, "{encoding}");
        }
    }

    #[test]
    fn bracketed_paste_wraps_raw_protocol_and_encodes_only_content() {
        let encoded =
            TerminalInputEncoder::new(TerminalEncoding::Gbk).encode_paste("你好\n世界\x1b", true);
        assert_eq!(
            encoded,
            b"\x1b[200~\xc4\xe3\xba\xc3\n\xca\xc0\xbd\xe7\x1b[201~"
        );
    }

    #[test]
    fn paste_preserves_text_newlines_inside_bracketed_boundaries() {
        let cases: [(&str, bool, &[u8]); 7] = [
            ("line 1\nline 2", false, b"line 1\rline 2"),
            ("line 1\r\nline 2\rline 3", false, b"line 1\rline 2\rline 3"),
            (
                "line 1\r\nline 2\nline 3\rline 4",
                true,
                b"\x1b[200~line 1\nline 2\nline 3\nline 4\x1b[201~",
            ),
            (
                "line 1\n\nline 2\n",
                true,
                b"\x1b[200~line 1\n\nline 2\n\x1b[201~",
            ),
            ("pwd", true, b"\x1b[200~pwd\x1b[201~"),
            ("literal\\n", true, b"\x1b[200~literal\\n\x1b[201~"),
            (
                "text\x1b[201~\x03\nnext",
                true,
                b"\x1b[200~text[201~\nnext\x1b[201~",
            ),
        ];

        for (text, bracketed, expected) in cases {
            let encoded =
                TerminalInputEncoder::new(TerminalEncoding::Utf8).encode_paste(text, bracketed);
            assert_eq!(encoded, expected);
        }
    }

    #[test]
    fn mismatch_detector_suggests_legacy_encoding_only_in_utf8_mode() {
        let encoded = TerminalInputEncoder::new(TerminalEncoding::Gbk)
            .encode_text("你好世界你好世界你好世界")
            .into_owned();
        for mode in [TerminalEncoding::Utf8, TerminalEncoding::Gbk] {
            let mut detector = EncodingMismatchDetector::new(mode);
            let hint = detector.observe(&encoded.repeat(8));
            if mode == TerminalEncoding::Utf8 {
                let hint = hint.expect("invalid UTF-8 should produce an encoding hint");
                assert!(hint.suggestions.contains(&TerminalEncoding::Gbk));
                assert!(hint.invalid_bytes >= 4);
            } else {
                assert!(hint.is_none());
            }
        }
    }

    #[test]
    fn mismatch_detector_counts_utf8_split_across_reads_as_valid() {
        let utf8 = "你好世界，终端输出".repeat(16);
        let gbk = TerminalInputEncoder::new(TerminalEncoding::Gbk)
            .encode_text(&utf8)
            .into_owned();
        // Small reads, as a serial line delivers them, split most characters.
        for chunk_len in [1, 2, 5, 7] {
            let mut detector = EncodingMismatchDetector::new(TerminalEncoding::Utf8);
            let hints = utf8
                .as_bytes()
                .chunks(chunk_len)
                .filter_map(|chunk| detector.observe(chunk))
                .count();
            assert_eq!(hints, 0, "UTF-8 in {chunk_len}-byte reads");
            assert_eq!(detector.invalid_bytes, 0, "UTF-8 in {chunk_len}-byte reads");

            let mut detector = EncodingMismatchDetector::new(TerminalEncoding::Utf8);
            let hint = gbk
                .chunks(chunk_len)
                .find_map(|chunk| detector.observe(chunk))
                .unwrap_or_else(|| panic!("GBK in {chunk_len}-byte reads"));
            assert!(hint.suggestions.contains(&TerminalEncoding::Gbk));
        }

        // A cut-off sequence that the next read doesn't complete is still invalid.
        let mut detector = EncodingMismatchDetector::new(TerminalEncoding::Utf8);
        detector.observe(b"a\xe4\xbd");
        assert_eq!(detector.invalid_bytes, 0);
        detector.observe(b"A\xe4");
        assert_eq!(detector.invalid_bytes, 2);
        detector.observe(b"\xbd\xa0");
        assert_eq!(detector.invalid_bytes, 2);
    }
}
