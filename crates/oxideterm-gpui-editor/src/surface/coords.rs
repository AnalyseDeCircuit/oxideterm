// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

use oxideterm_editor_core::Selection;

pub(super) fn floor_char_boundary(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte = byte.saturating_sub(1);
    }
    byte
}

pub(super) fn selection_columns_for_line(
    selection: Selection,
    line_text: &str,
    line_range: Range<usize>,
) -> Option<(usize, usize)> {
    if selection.is_caret() {
        return None;
    }
    let selected = selection.range();
    let start = selected.start.0.max(line_range.start);
    let end = selected.end.0.min(line_range.end);
    if start >= end {
        return None;
    }
    let local_start = start - line_range.start;
    let local_end = end - line_range.start;
    Some((
        visual_column_for_byte_column(line_text, local_start),
        visual_column_for_byte_column(line_text, local_end),
    ))
}

pub(super) fn visual_column_for_byte_column(text: &str, byte_column: usize) -> usize {
    let byte_column = floor_char_boundary(text, byte_column);
    text[..byte_column].chars().count()
}

pub(super) fn byte_column_for_visual_column(text: &str, visual_column: usize) -> usize {
    text.char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .nth(visual_column)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use oxideterm_editor_core::BufferOffset;

    use super::*;

    #[test]
    fn maps_visual_columns_without_splitting_unicode() {
        let text = "你aé";

        assert_eq!(byte_column_for_visual_column(text, 0), 0);
        assert_eq!(byte_column_for_visual_column(text, 1), 3);
        assert_eq!(byte_column_for_visual_column(text, 2), 4);
        assert_eq!(byte_column_for_visual_column(text, 3), 6);
        assert_eq!(visual_column_for_byte_column(text, 4), 2);
    }

    #[test]
    fn computes_selection_columns_for_unicode_line() {
        let text = "你abc";
        let selection = Selection::new(BufferOffset(3), BufferOffset(5));

        assert_eq!(
            selection_columns_for_line(selection, text, 0..6),
            Some((1, 3))
        );
    }
}
