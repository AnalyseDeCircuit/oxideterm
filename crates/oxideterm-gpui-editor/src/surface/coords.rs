// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

use oxideterm_editor_core::Selection;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(super) fn floor_char_boundary(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte = byte.saturating_sub(1);
    }
    byte
}

pub(super) fn selection_byte_range_for_line(
    selection: Selection,
    line_text: &str,
    line_range: Range<usize>,
) -> Option<Range<usize>> {
    if selection.is_caret() {
        return None;
    }
    let selected = selection.range();
    let start = selected.start.0.max(line_range.start);
    let end = selected.end.0.min(line_range.end);
    if start > end {
        return None;
    }
    if start == end {
        // A selected newline gives an empty physical line no text bytes, but
        // editors still paint one cell to keep a multi-line selection joined.
        return (line_text.is_empty()
            && selected.start.0 <= line_range.start
            && selected.end.0 > line_range.start)
            .then_some(0..0);
    }
    // Rendering uses shaped text positions, so preserve byte offsets instead
    // of converting through an assumed monospace cell width.
    let local_start = floor_char_boundary(line_text, start - line_range.start);
    let local_end = floor_char_boundary(line_text, end - line_range.start);
    (local_start < local_end).then_some(local_start..local_end)
}

pub(super) fn visual_column_for_byte_column(text: &str, byte_column: usize) -> usize {
    let byte_column = floor_char_boundary(text, byte_column);
    text.grapheme_indices(true)
        .take_while(|(start, grapheme)| start + grapheme.len() <= byte_column)
        .map(|(_, grapheme)| grapheme_visual_width(grapheme))
        .sum()
}

pub(super) fn byte_column_for_visual_column(text: &str, visual_column: usize) -> usize {
    let mut current_column = 0;
    for (start, grapheme) in text.grapheme_indices(true) {
        let end = start + grapheme.len();
        let next_column = current_column + grapheme_visual_width(grapheme);
        if visual_column < next_column {
            // A click inside a wide glyph resolves to its nearest legal caret boundary.
            return if visual_column.saturating_sub(current_column)
                < next_column.saturating_sub(visual_column)
            {
                start
            } else {
                end
            };
        }
        if visual_column == next_column {
            return end;
        }
        current_column = next_column;
    }
    text.len()
}

pub(super) fn grapheme_visual_width(grapheme: &str) -> usize {
    // Keep control-only graphemes addressable while matching conventional
    // monospace widths for CJK and emoji clusters.
    UnicodeWidthStr::width(grapheme).max(1)
}

#[cfg(test)]
mod tests {
    use oxideterm_editor_core::BufferOffset;

    use super::*;

    #[test]
    fn maps_visual_columns_without_splitting_wide_unicode() {
        let text = "你aé";

        assert_eq!(byte_column_for_visual_column(text, 0), 0);
        assert_eq!(byte_column_for_visual_column(text, 1), 3);
        assert_eq!(byte_column_for_visual_column(text, 2), 3);
        assert_eq!(byte_column_for_visual_column(text, 3), 4);
        assert_eq!(byte_column_for_visual_column(text, 4), 6);
        assert_eq!(visual_column_for_byte_column(text, 3), 2);
        assert_eq!(visual_column_for_byte_column(text, 4), 3);
        assert_eq!(visual_column_for_byte_column(text, text.len()), 4);
    }

    #[test]
    fn treats_multi_codepoint_emoji_as_one_wide_grapheme() {
        let text = "a👨‍👩‍👧‍👦b";
        let emoji_end = 1 + "👨‍👩‍👧‍👦".len();

        assert_eq!(visual_column_for_byte_column(text, emoji_end), 3);
        assert_eq!(byte_column_for_visual_column(text, 2), emoji_end);
        assert_eq!(visual_column_for_byte_column(text, text.len()), 4);
    }

    #[test]
    fn combining_marks_share_the_base_character_column() {
        let text = "e\u{301}x";
        let grapheme_end = "e\u{301}".len();

        assert_eq!(visual_column_for_byte_column(text, grapheme_end), 1);
        assert_eq!(byte_column_for_visual_column(text, 1), grapheme_end);
        assert_eq!(visual_column_for_byte_column(text, text.len()), 2);
    }

    #[test]
    fn computes_selection_byte_range_for_unicode_line() {
        let text = "你abc";
        let selection = Selection::new(BufferOffset(3), BufferOffset(5));

        assert_eq!(
            selection_byte_range_for_line(selection, text, 0..6),
            Some(3..5)
        );

        let across_empty_line = Selection::new(BufferOffset(0), BufferOffset(3));
        assert_eq!(
            selection_byte_range_for_line(across_empty_line, "", 2..2),
            Some(0..0)
        );
        let ending_before_empty_line = Selection::new(BufferOffset(0), BufferOffset(2));
        assert_eq!(
            selection_byte_range_for_line(ending_before_empty_line, "", 2..2),
            None
        );
    }

    #[test]
    fn computes_selection_byte_range_for_each_wrapped_segment() {
        let selection = Selection::new(BufferOffset(2), BufferOffset(10));

        assert_eq!(
            selection_byte_range_for_line(selection, "abcdefgh", 0..8),
            Some(2..8)
        );
        assert_eq!(
            selection_byte_range_for_line(selection, "ijklmnop", 8..16),
            Some(0..2)
        );
    }
}
