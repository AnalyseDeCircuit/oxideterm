//! Single-line selection and IME composition for GPUI fields and authentication prompts.
mod field;
pub use field::{FieldEvent, LineField};
use gpui::{Modifiers, MouseDownEvent, Pixels, Point};
use oxideterm_editor_core::utf16::{
    byte_index_for_utf16, next_utf16_boundary, previous_utf16_boundary,
    utf16_offset_for_byte_index, word_range_for_utf16_offset,
};
use oxideterm_gpui_ui::text_input::TextInputViewport;
use std::ops::Range;
use zeroize::Zeroizing;
#[derive(Default)]
pub struct LineEdit {
    pub value: Zeroizing<String>,
    pub viewport: TextInputViewport,
    pub anchor: usize,
    pub caret: usize,
    // Preedit remains separate until the platform commits it.
    pub marked: Option<(Zeroizing<String>, Range<usize>)>,
}

impl LineEdit {
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }

    pub fn set_range(&mut self, range: Range<usize>) {
        let boundary = |offset| {
            utf16_offset_for_byte_index(&self.value, byte_index_for_utf16(&self.value, offset))
        };
        self.anchor = boundary(range.start);
        self.caret = boundary(range.end);
    }

    pub fn replace_selection(&mut self, text: &str) {
        let range = self.selection();
        let start = byte_index_for_utf16(&self.value, range.start);
        let end = byte_index_for_utf16(&self.value, range.end);
        // Growing a String in place could release its previous allocation without wiping it.
        let mut next = Zeroizing::new(String::with_capacity(
            self.value.len() - (end - start) + text.len(),
        ));
        next.push_str(&self.value[..start]);
        next.push_str(text);
        next.push_str(&self.value[end..]);
        self.value = next;
        self.caret = range.start + text.encode_utf16().count();
        self.anchor = self.caret;
    }

    pub fn commit_marked(&mut self) {
        if let Some((text, _)) = self.marked.take() {
            self.replace_selection(&text);
        }
    }

    pub fn marked_range(&self) -> Option<Range<usize>> {
        let start = self.selection().start;
        Some(start..start + self.marked.as_ref()?.0.encode_utf16().count())
    }

    pub fn prepare_replacement(&mut self, range: Option<Range<usize>>) {
        if let Some(range) = range
            && Some(&range) != self.marked_range().as_ref()
        {
            self.commit_marked();
            self.set_range(range);
        } else {
            self.marked = None;
        }
    }

    pub fn characters(&self) -> impl Iterator<Item = char> + '_ {
        let (before, marked, after) = if let Some((marked, _)) = &self.marked {
            let range = self.selection();
            (
                &self.value[..byte_index_for_utf16(&self.value, range.start)],
                marked.as_str(),
                &self.value[byte_index_for_utf16(&self.value, range.end)..],
            )
        } else {
            (self.value.as_str(), "", "")
        };
        before.chars().chain(marked.chars()).chain(after.chars())
    }

    pub fn visible_byte(&self, offset: usize, echo: bool) -> usize {
        let mut units = 0;
        self.characters()
            .take_while(|ch| {
                let before = units;
                units += ch.len_utf16();
                before < offset
            })
            .map(|ch| {
                if echo {
                    ch.len_utf8()
                } else {
                    '•'.len_utf8()
                }
            })
            .sum()
    }

    pub fn offset(&self, point: Point<Pixels>, echo: bool) -> Option<usize> {
        let byte = self.viewport.byte_index_for_position(point)?;
        let mut bytes = 0;
        Some(
            self.characters()
                .take_while(|ch| {
                    bytes += if echo {
                        ch.len_utf8()
                    } else {
                        '•'.len_utf8()
                    };
                    bytes <= byte
                })
                .map(char::len_utf16)
                .sum(),
        )
    }

    pub fn select_at(&mut self, event: &MouseDownEvent, echo: bool) {
        let len = self.value.encode_utf16().count();
        let offset = self.offset(event.position, echo).unwrap_or(len);
        let range = match event.click_count {
            2 => word_range_for_utf16_offset(&self.value, offset),
            3.. => 0..len,
            _ => offset..offset,
        };
        if !event.modifiers.shift {
            self.anchor = range.start;
        }
        self.caret = range.end;
    }

    pub fn edit_key(&mut self, key: &str, modifiers: Modifiers) -> bool {
        let range = self.selection();
        let len = self.value.encode_utf16().count();
        match key {
            "a" if modifiers.platform || modifiers.control => self.set_range(0..len),
            "backspace" | "delete" => {
                if range.is_empty() {
                    self.anchor = if key == "backspace" {
                        previous_utf16_boundary(&self.value, self.caret)
                    } else {
                        next_utf16_boundary(&self.value, self.caret)
                    };
                }
                self.replace_selection("");
            }
            "left" | "right" | "home" | "end" => {
                self.caret = match key {
                    "home" => 0,
                    "end" => len,
                    "left" if !modifiers.shift && !range.is_empty() => range.start,
                    "right" if !modifiers.shift && !range.is_empty() => range.end,
                    "left" => previous_utf16_boundary(&self.value, self.caret),
                    _ => next_utf16_boundary(&self.value, self.caret),
                };
                if !modifiers.shift {
                    self.anchor = self.caret;
                }
            }
            _ => return false,
        }
        true
    }
}
