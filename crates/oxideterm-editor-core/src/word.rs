// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{BufferOffset, TextBuffer, TextRange};
use unicode_segmentation::UnicodeSegmentation;

fn word_class(grapheme: &str) -> u8 {
    match grapheme.chars().next() {
        Some(ch) if is_word_char(ch) => 1,
        Some(ch) if ch.is_whitespace() => 2,
        _ => 0,
    }
}

impl TextBuffer {
    pub fn word_boundary(&self, offset: BufferOffset, forward: bool) -> BufferOffset {
        let Ok(position) = self.offset_to_line_col(offset) else {
            return offset;
        };
        self.with_line_text(position.line, |line| {
            let column = position.column;
            if forward {
                let mut tail = line[column..].graphemes(true).peekable();
                let Some(first) = tail.peek() else {
                    return self.next_grapheme_offset(offset);
                };
                let class = word_class(first);
                let count: usize = tail
                    .take_while(|g| word_class(g) == class)
                    .map(str::len)
                    .sum();
                BufferOffset(offset.0 + count)
            } else {
                let mut prefix = line[..column].graphemes(true).rev().peekable();
                let Some(first) = prefix.peek() else {
                    return self.previous_grapheme_offset(offset);
                };
                let class = word_class(first);
                let count: usize = prefix
                    .take_while(|g| word_class(g) == class)
                    .map(str::len)
                    .sum();
                BufferOffset(offset.0 - count)
            }
        })
        .unwrap_or(offset)
    }

    pub fn word_selection(&self, offset: BufferOffset) -> TextRange {
        let Ok(pos) = self.offset_to_line_col(offset) else {
            return TextRange::caret(offset);
        };
        if self.line_start_offset(pos.line) == self.line_end_offset(pos.line) {
            return TextRange::caret(offset);
        }
        let end = if Some(offset) == self.line_end_offset(pos.line) {
            offset
        } else {
            self.word_boundary(offset, true)
        };
        TextRange::new(self.word_boundary(end, false), end)
    }
}

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
    fn word_at_preserves_unicode_boundaries_and_dollar_identifiers() {
        for (source, offset, expected) in [
            ("let 名字 = value", 5, "名字"),
            ("const $value = 1", 8, "$value"),
            ("before\n名字\nend", 8, "名字"),
            ("before\n名字\nend", 6, "before"),
        ] {
            assert_eq!(word_at(source, BufferOffset(offset)), expected, "{source}");
            assert_eq!(
                crate::TextBuffer::new(source).word_at(BufferOffset(offset)),
                expected,
                "{source}"
            );
        }
    }
}
