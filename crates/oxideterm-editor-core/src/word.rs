// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::BufferOffset;

pub fn word_at(source: &str, offset: BufferOffset) -> String {
    let offset = floor_char_boundary(source, offset.0.min(source.len()));
    let start = source[..offset]
        .char_indices()
        .rev()
        .find(|(_, ch)| !is_word_char(*ch))
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(0);
    let end = source[offset..]
        .char_indices()
        .find(|(_, ch)| !is_word_char(*ch))
        .map(|(index, _)| offset + index)
        .unwrap_or(source.len());
    source[start..end].to_string()
}

fn floor_char_boundary(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

fn is_word_char(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_at_respects_unicode_boundaries() {
        assert_eq!(word_at("let 名字 = value", BufferOffset(5)), "名字");
    }

    #[test]
    fn word_at_matches_codemirror_dollar_identifiers() {
        assert_eq!(word_at("const $value = 1", BufferOffset(8)), "$value");
    }
}
