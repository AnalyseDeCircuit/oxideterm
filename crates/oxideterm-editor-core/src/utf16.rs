// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! UTF-16 text transformations used by platform input adapters and editors.

use std::ops::Range;

/// Replaces a UTF-16 range without exposing byte-offset details to UI adapters.
pub fn replace_utf16(value: &mut String, range: Option<Range<usize>>, replacement: &str) {
    let range = range.unwrap_or_else(|| {
        let end = value.encode_utf16().count();
        end..end
    });
    let start = byte_index_for_utf16(value, range.start);
    let end = byte_index_for_utf16(value, range.end);
    value.replace_range(start..end, replacement);
}

pub fn utf16_slice(value: &str, range: Range<usize>) -> String {
    let start = byte_index_for_utf16(value, range.start);
    let end = byte_index_for_utf16(value, range.end);
    value[start..end].to_string()
}

pub fn byte_index_for_utf16(value: &str, offset: usize) -> usize {
    // Offsets inside a surrogate pair resolve to the owning scalar boundary.
    let mut utf16_count = 0;
    for (byte_index, ch) in value.char_indices() {
        if utf16_count >= offset {
            return byte_index;
        }
        utf16_count += ch.len_utf16();
    }
    value.len()
}

pub fn utf16_offset_for_byte_index(value: &str, byte_offset: usize) -> usize {
    let byte_offset = floor_char_boundary(value, byte_offset.min(value.len()));
    value[..byte_offset].encode_utf16().count()
}

pub fn utf16_offset_for_char_index(value: &str, char_offset: usize) -> usize {
    value.chars().take(char_offset).map(char::len_utf16).sum()
}

pub fn char_index_for_utf16(value: &str, offset: usize) -> usize {
    let mut utf16_count = 0;
    for (char_index, ch) in value.chars().enumerate() {
        if utf16_count >= offset {
            return char_index;
        }
        utf16_count += ch.len_utf16();
    }
    value.chars().count()
}

pub fn floor_char_boundary(value: &str, mut byte_offset: usize) -> usize {
    while byte_offset > 0 && !value.is_char_boundary(byte_offset) {
        byte_offset -= 1;
    }
    byte_offset
}

pub fn previous_utf16_boundary(value: &str, offset: usize) -> usize {
    let mut previous = 0;
    let mut utf16_count = 0;
    for ch in value.chars() {
        if utf16_count >= offset {
            break;
        }
        previous = utf16_count;
        utf16_count += ch.len_utf16();
    }
    previous
}

pub fn next_utf16_boundary(value: &str, offset: usize) -> usize {
    let mut utf16_count = 0;
    for ch in value.chars() {
        let next = utf16_count + ch.len_utf16();
        if utf16_count >= offset {
            return next;
        }
        utf16_count = next;
    }
    value.encode_utf16().count()
}

pub fn previous_word_boundary(value: &str, offset: usize) -> usize {
    let current = byte_index_for_utf16(value, offset);
    let prefix = &value[..current];
    let mut saw_word = false;
    for (byte_index, ch) in prefix.char_indices().rev() {
        if ch.is_whitespace() {
            if saw_word {
                return prefix[..byte_index + ch.len_utf8()].encode_utf16().count();
            }
        } else {
            saw_word = true;
        }
    }
    0
}

pub fn next_word_boundary(value: &str, offset: usize) -> usize {
    let current = byte_index_for_utf16(value, offset);
    let suffix = &value[current..];
    let mut saw_word = false;
    for (relative_byte, ch) in suffix.char_indices() {
        if ch.is_whitespace() {
            if saw_word {
                return value[..current + relative_byte].encode_utf16().count();
            }
        } else {
            saw_word = true;
        }
    }
    value.encode_utf16().count()
}

pub fn word_range_for_utf16_offset(value: &str, offset: usize) -> Range<usize> {
    let text_len = value.encode_utf16().count();
    if text_len == 0 {
        return 0..0;
    }
    let mut byte_index = byte_index_for_utf16(value, offset.min(text_len));
    if byte_index == value.len() && byte_index > 0 {
        byte_index = previous_char_start(value, byte_index);
    }
    if value[byte_index..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
        && byte_index > 0
    {
        let previous = previous_char_start(value, byte_index);
        if !value[previous..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            byte_index = previous;
        }
    }

    let mut start = byte_index;
    while start > 0 {
        let previous = previous_char_start(value, start);
        let Some(ch) = value[previous..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            break;
        }
        start = previous;
    }

    let mut end = byte_index;
    while end < value.len() {
        let Some(ch) = value[end..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            break;
        }
        end += ch.len_utf8();
    }

    utf16_offset_for_byte_index(value, start)..utf16_offset_for_byte_index(value, end)
}

pub fn line_range_for_utf16_offset(value: &str, offset: usize) -> Range<usize> {
    let ranges = line_ranges_utf16(value);
    let text_len = value.encode_utf16().count();
    ranges
        .iter()
        .find(|range| offset <= range.end)
        .cloned()
        .unwrap_or(text_len..text_len)
}

pub fn line_start_for_utf16_offset(value: &str, offset: usize) -> usize {
    line_range_for_utf16_offset(value, offset).start
}

pub fn line_end_for_utf16_offset(value: &str, offset: usize) -> usize {
    line_range_for_utf16_offset(value, offset).end
}

pub fn control_k_delete_end(value: &str, offset: usize) -> usize {
    let line_end = line_end_for_utf16_offset(value, offset);
    if line_end > offset {
        return line_end;
    }
    next_utf16_boundary(value, offset)
}

pub fn transpose_text_at_utf16_offset(value: &str, offset: usize) -> Option<(String, usize)> {
    let mut chars: Vec<char> = value.chars().collect();
    if chars.len() < 2 {
        return None;
    }
    let text_len = value.encode_utf16().count();
    let right = if offset >= text_len {
        chars.len() - 1
    } else {
        char_index_for_utf16(value, offset).min(chars.len() - 1)
    };
    if right == 0 {
        return None;
    }
    let left = right - 1;
    chars.swap(left, right);
    let next_caret = if offset >= text_len {
        text_len
    } else {
        utf16_offset_for_char_index(&chars.iter().collect::<String>(), right + 1)
    };
    Some((chars.into_iter().collect(), next_caret))
}

pub fn vertical_line_navigation_destination(value: &str, offset: usize, down: bool) -> usize {
    // Columns are measured in UTF-16 units to match platform text APIs.
    let ranges = line_ranges_utf16(value);
    if ranges.is_empty() {
        return 0;
    }
    let line_index = ranges
        .iter()
        .position(|range| offset <= range.end)
        .unwrap_or_else(|| ranges.len().saturating_sub(1));
    let current = &ranges[line_index];
    let column = offset.saturating_sub(current.start);
    if down {
        let Some(next) = ranges.get(line_index + 1) else {
            return value.encode_utf16().count();
        };
        next.start + column.min(next.end.saturating_sub(next.start))
    } else {
        if line_index == 0 {
            return 0;
        }
        let previous = &ranges[line_index - 1];
        previous.start + column.min(previous.end.saturating_sub(previous.start))
    }
}

pub fn line_ranges_utf16(value: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut offset = 0;
    for ch in value.chars() {
        if ch == '\n' {
            ranges.push(start..offset);
            offset += ch.len_utf16();
            start = offset;
        } else {
            offset += ch.len_utf16();
        }
    }
    ranges.push(start..offset);
    ranges
}

fn previous_char_start(value: &str, byte_index: usize) -> usize {
    value[..byte_index]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_preserves_utf16_surrogate_boundaries() {
        let value = "a😄b";
        assert_eq!(next_utf16_boundary(value, 0), 1);
        assert_eq!(next_utf16_boundary(value, 1), 3);
        assert_eq!(previous_utf16_boundary(value, 3), 1);
        assert_eq!(previous_utf16_boundary(value, 4), 3);
        assert_eq!(utf16_slice(value, 1..3), "😄");
    }

    #[test]
    fn multiline_navigation_preserves_visual_column() {
        let value = "abc\nde\nfghi";
        assert_eq!(vertical_line_navigation_destination(value, 2, true), 6);
        assert_eq!(vertical_line_navigation_destination(value, 6, true), 9);
        assert_eq!(vertical_line_navigation_destination(value, 9, false), 6);
    }

    #[test]
    fn transpose_uses_utf16_caret_offsets() {
        for (text, caret, expected, next_caret) in [
            ("abcd", 2, "acbd", 3),
            ("a😄b", 3, "ab😄", 4),
            ("abcd", 4, "abdc", 4),
        ] {
            assert_eq!(
                transpose_text_at_utf16_offset(text, caret),
                Some((expected.into(), next_caret))
            );
        }
    }

    #[test]
    fn word_navigation_matches_browser_style_runs() {
        let value = "alpha beta  gamma";
        assert_eq!(previous_word_boundary(value, 12), 6);
        assert_eq!(
            previous_word_boundary(value, value.encode_utf16().count()),
            12
        );
        assert_eq!(next_word_boundary(value, 0), 5);
        assert_eq!(next_word_boundary(value, 6), 10);
    }

    #[test]
    fn double_click_word_range_handles_edges() {
        assert_eq!(word_range_for_utf16_offset("root", 1), 0..4);
        assert_eq!(word_range_for_utf16_offset("alpha beta", 7), 6..10);
        assert_eq!(word_range_for_utf16_offset("alpha beta", 5), 0..5);
    }

    #[test]
    fn multiline_line_ranges_match_textarea_navigation() {
        let value = "one\ntwo\nthree";
        assert_eq!(line_range_for_utf16_offset(value, 1), 0..3);
        assert_eq!(line_range_for_utf16_offset(value, 5), 4..7);
        assert_eq!(line_start_for_utf16_offset(value, 10), 8);
        assert_eq!(line_end_for_utf16_offset(value, 10), 13);
    }

    #[test]
    fn control_k_matches_textarea_line_delete() {
        let value = "one\ntwo\nthree";
        assert_eq!(control_k_delete_end(value, 5), 7);
        assert_eq!(control_k_delete_end(value, 7), 8);
    }
}
