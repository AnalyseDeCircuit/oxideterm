// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, UTF16Selection, Window};
use oxideterm_editor_core::{BufferOffset, TextRange};

use super::{MarkedText, TextEditorView};

impl EntityInputHandler for TextEditorView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let buffer_end = self.buffer.utf16_offset(self.buffer.len());
        if let Some(marked) = &self.marked_text {
            let marked_start = self.buffer.utf16_offset(marked.range.start.0);
            let replaced_end = self.buffer.utf16_offset(marked.range.end.0);
            let marked_end = marked_start + marked.text.encode_utf16().count();
            let end = buffer_end - (replaced_end - marked_start) + marked_end - marked_start;
            let range = range_utf16.start.min(end)..range_utf16.end.min(end);
            *adjusted_range = Some(range.clone());
            let mut text = String::new();
            // Compose only the requested slice, not another copy of the document.
            if range.start < marked_start {
                text.push_str(
                    &self
                        .buffer
                        .slice_utf16(range.start..range.end.min(marked_start)),
                );
            }
            if range.start < marked_end && range.end > marked_start {
                text.push_str(&utf16_slice(
                    &marked.text,
                    range.start.saturating_sub(marked_start)
                        ..range.end.min(marked_end) - marked_start,
                ));
            }
            if range.end > marked_end {
                text.push_str(&self.buffer.slice_utf16(
                    replaced_end + range.start.max(marked_end) - marked_end
                        ..replaced_end + range.end - marked_end,
                ));
            }
            Some(text)
        } else {
            let end = buffer_end;
            let range = range_utf16.start.min(end)..range_utf16.end.min(end);
            *adjusted_range = Some(range.clone());
            Some(self.buffer.slice_utf16(range))
        }
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
            range: self.buffer.utf16_offset(range.start.0)..self.buffer.utf16_offset(range.end.0),
            reversed: selection.anchor > selection.head,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let marked = self.marked_text.as_ref()?;
        let start = self.buffer.utf16_offset(marked.range.start.0);
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
        self.activate_caret_blink(cx);
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
        self.activate_caret_blink(cx);
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
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let index = range_utf16.start;
        let byte = self.buffer.byte_offset_for_utf16(index);
        Some(self.bounds_for_byte_offset(BufferOffset(byte), element_bounds, window))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.offset_for_window_point(point, window)?;
        Some(self.buffer.utf16_offset(offset.0))
    }
}

impl TextEditorView {
    pub(super) fn ime_replacement_range(
        &self,
        range_utf16: Option<Range<usize>>,
    ) -> Option<TextRange> {
        let range_utf16 = range_utf16?;
        if let Some(marked) = self.marked_text.as_ref() {
            let marked_start = self.buffer.utf16_offset(marked.range.start.0);
            let marked_end = marked_start + marked.text.encode_utf16().count();
            if ranges_overlap(&range_utf16, &(marked_start..marked_end)) {
                return Some(marked.range);
            }
        }

        let start = self.buffer.byte_offset_for_utf16(range_utf16.start);
        let end = self.buffer.byte_offset_for_utf16(range_utf16.end);
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

    #[gpui::test]
    fn platform_reads_compose_only_requested_text_and_preserve_commit_ranges(
        cx: &mut gpui::TestAppContext,
    ) {
        let (editor, cx) = cx.add_window_view(|_, cx| {
            TextEditorView::new("a🚀你\nlast", &oxideterm_theme::default_tokens(), cx)
        });
        cx.update(|window, app| {
            editor.update(app, |editor, cx| {
                editor
                    .cursor
                    .set_selection(oxideterm_editor_core::Selection::new(
                        BufferOffset(5),
                        BufferOffset(8),
                    ));
                editor.insert_text("界", cx);
                editor
                    .cursor
                    .set_selection(oxideterm_editor_core::Selection::new(
                        BufferOffset(8),
                        BufferOffset(1),
                    ));
                let selection = editor.selected_text_range(false, window, cx).unwrap();
                assert_eq!(selection.range, 1..4);
                assert!(selection.reversed);
                assert_eq!(
                    editor
                        .text_for_range(1..3, &mut None, window, cx)
                        .as_deref(),
                    Some("🚀")
                );
                editor.replace_and_mark_text_in_range(Some(1..4), "中文🙂", None, window, cx);
                assert_eq!(editor.marked_text_range(window, cx), Some(1..5));
                for (range, expected, adjusted) in [
                    (0..99, "a中文🙂\nlast", 0..10),
                    (1..5, "中文🙂", 1..5),
                    (2..6, "文🙂\n", 2..6),
                    (5..9, "\nlas", 5..9),
                ] {
                    let mut actual_range = None;
                    assert_eq!(
                        editor
                            .text_for_range(range, &mut actual_range, window, cx)
                            .as_deref(),
                        Some(expected)
                    );
                    assert_eq!(actual_range, Some(adjusted));
                }
                editor.replace_text_in_range(Some(1..5), "文", window, cx);
                assert_eq!(editor.buffer.text(), "a文\nlast");
                editor.undo(cx);
                assert_eq!(editor.buffer.text(), "a🚀界\nlast");
            });
        });
    }

    #[test]
    fn converts_utf16_indices_without_splitting_surrogates() {
        let text = "a🚀你";

        assert_eq!(utf16_index_to_byte(text, 2), 1);
        assert_eq!(utf16_index_to_byte(text, 3), 5);
        assert_eq!(utf16_slice(text, 1..3), "🚀");
    }
}
