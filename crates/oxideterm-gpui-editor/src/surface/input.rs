// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, UTF16Selection, Window};
use oxideterm_editor_core::{BufferOffset, LineCol, TextRange};

use super::{MarkedText, TextEditorView};

impl EntityInputHandler for TextEditorView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.text_with_marked();
        let end = text.encode_utf16().count();
        let range = range_utf16.start.min(end)..range_utf16.end.min(end);
        *adjusted_range = Some(range.clone());
        Some(utf16_slice(&text, range))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let selection = self.cursor.selection();
        let range = selection.range();
        Some(UTF16Selection {
            range: byte_range_to_utf16_range(self.buffer.text(), range.start.0..range.end.0),
            reversed: selection.anchor > selection.head,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let marked = self.marked_text.as_ref()?;
        let start = byte_to_utf16_index(self.buffer.text(), marked.range.start.0);
        let end = start + marked.text.encode_utf16().count();
        Some(start..end)
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.marked_text.take().is_some() {
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = self
            .ime_replacement_range(range_utf16)
            .unwrap_or_else(|| self.cursor.selection().range());
        self.replace_range_with_caret(range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = self
            .ime_replacement_range(range_utf16)
            .unwrap_or_else(|| self.cursor.selection().range());
        // Keep the original replacement range separate from the committed
        // buffer so IME updates can replace the same composing segment without
        // corrupting the underlying text before commit.
        self.marked_text = (!new_text.is_empty()).then(|| MarkedText {
            text: new_text.to_string(),
            range,
        });
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let index = range_utf16.start;
        let byte = utf16_index_to_byte(self.buffer.text(), index);
        Some(self.bounds_for_byte_offset(BufferOffset(byte), element_bounds))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let display_row = self.display_row_for_window_y(point.y)?;
        let column = display_row.start_col + self.visual_column_for_window_x(point.x);
        let line_text = self.buffer.line_text(display_row.line).unwrap_or_default();
        let byte_column = super::byte_column_for_visual_column(line_text, column);
        let offset = self
            .buffer
            .line_col_to_offset(LineCol::new(display_row.line, byte_column))
            .ok()?;
        Some(byte_to_utf16_index(self.buffer.text(), offset.0))
    }
}

impl TextEditorView {
    pub(super) fn text_with_marked(&self) -> String {
        let Some(marked) = self.marked_text.as_ref() else {
            return self.buffer.text().to_string();
        };
        let mut text = self.buffer.text().to_string();
        let start = marked.range.start.0;
        let end = marked.range.end.0;
        if start <= end
            && end <= text.len()
            && text.is_char_boundary(start)
            && text.is_char_boundary(end)
        {
            text.replace_range(start..end, &marked.text);
        }
        text
    }

    pub(super) fn ime_replacement_range(
        &self,
        range_utf16: Option<Range<usize>>,
    ) -> Option<TextRange> {
        let range_utf16 = range_utf16?;
        if let Some(marked) = self.marked_text.as_ref() {
            let marked_start = byte_to_utf16_index(self.buffer.text(), marked.range.start.0);
            let marked_end = marked_start + marked.text.encode_utf16().count();
            if ranges_overlap(&range_utf16, &(marked_start..marked_end)) {
                return Some(marked.range);
            }
        }

        let start = utf16_index_to_byte(self.buffer.text(), range_utf16.start);
        let end = utf16_index_to_byte(self.buffer.text(), range_utf16.end);
        Some(TextRange::new(BufferOffset(start), BufferOffset(end)))
    }
}

pub(super) fn keystroke_commits_platform_text(keystroke: &gpui::Keystroke) -> bool {
    if keystroke.modifiers.platform || keystroke.modifiers.control {
        return false;
    }

    keystroke
        .key_char
        .as_deref()
        .is_some_and(|text| !text.is_empty() && !text.chars().any(char::is_control))
}

fn byte_to_utf16_index(text: &str, byte: usize) -> usize {
    let byte = byte.min(text.len());
    let byte = super::coords::floor_char_boundary(text, byte);
    text[..byte].encode_utf16().count()
}

fn utf16_index_to_byte(text: &str, utf16_index: usize) -> usize {
    if utf16_index == 0 {
        return 0;
    }

    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        let next = units + ch.len_utf16();
        if next > utf16_index {
            return byte;
        }
        if next == utf16_index {
            return byte + ch.len_utf8();
        }
        units = next;
    }
    text.len()
}

fn byte_range_to_utf16_range(text: &str, range: Range<usize>) -> Range<usize> {
    byte_to_utf16_index(text, range.start)..byte_to_utf16_index(text, range.end)
}

fn utf16_slice(text: &str, range: Range<usize>) -> String {
    let start = utf16_index_to_byte(text, range.start);
    let end = utf16_index_to_byte(text, range.end);
    text[start..end].to_string()
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf16_indices_without_splitting_surrogates() {
        let text = "a🚀你";

        assert_eq!(byte_to_utf16_index(text, 0), 0);
        assert_eq!(byte_to_utf16_index(text, 1), 1);
        assert_eq!(byte_to_utf16_index(text, 5), 3);
        assert_eq!(utf16_index_to_byte(text, 2), 1);
        assert_eq!(utf16_index_to_byte(text, 3), 5);
        assert_eq!(utf16_slice(text, 1..3), "🚀");
    }

    #[test]
    fn ranges_overlap_only_when_boundaries_cross() {
        assert!(ranges_overlap(&(1..3), &(2..4)));
        assert!(!ranges_overlap(&(1..2), &(2..4)));
    }
}
