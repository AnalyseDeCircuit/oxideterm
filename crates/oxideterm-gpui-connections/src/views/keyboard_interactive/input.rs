use std::ops::Range;

use gpui::{
    Bounds, ClipboardItem, Context, EntityInputHandler, Modifiers, MouseDownEvent, Pixels, Point,
    UTF16Selection, Window, px,
};
use oxideterm_editor_core::utf16::{
    byte_index_for_utf16, next_utf16_boundary, previous_utf16_boundary,
    utf16_offset_for_byte_index, word_range_for_utf16_offset,
};
use oxideterm_gpui_ui::text_input::TextInputViewport;
use zeroize::Zeroizing;

use super::PortableKeyboardInteractiveDialog;

#[derive(Default)]
pub(super) struct PromptDraft {
    pub value: Zeroizing<String>,
    pub viewport: TextInputViewport,
    anchor: usize,
    pub caret: usize,
    // Preedit belongs to this prompt, never to the challenge presentation or the host.
    pub marked: Option<(Zeroizing<String>, Range<usize>)>,
}

impl PromptDraft {
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }

    fn set_range(&mut self, range: Range<usize>) {
        let boundary = |offset| {
            utf16_offset_for_byte_index(&self.value, byte_index_for_utf16(&self.value, offset))
        };
        self.anchor = boundary(range.start);
        self.caret = boundary(range.end);
    }

    fn replace_selection(&mut self, text: &str) {
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

    fn marked_range(&self) -> Option<Range<usize>> {
        let start = self.selection().start;
        Some(start..start + self.marked.as_ref()?.0.encode_utf16().count())
    }

    fn prepare_replacement(&mut self, range: Option<Range<usize>>) {
        if let Some(range) = range
            && Some(&range) != self.marked_range().as_ref()
        {
            self.commit_marked();
            self.set_range(range);
        } else {
            self.marked = None;
        }
    }

    fn characters(&self) -> impl Iterator<Item = char> + '_ {
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

    fn visible_byte(&self, offset: usize, echo: bool) -> usize {
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

impl PortableKeyboardInteractiveDialog {
    fn active_draft(&self) -> Option<&PromptDraft> {
        self.drafts.get(self.focused).filter(|_| self.editing())
    }

    fn active_draft_mut(&mut self) -> Option<&mut PromptDraft> {
        if !self.editing() {
            return None;
        }
        self.drafts.get_mut(self.focused)
    }
}

impl EntityInputHandler for PortableKeyboardInteractiveDialog {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        // The browser mirrors this string in a hidden text area. Preserve UTF-16 geometry,
        // including surrogate pairs, without handing it a copy of the response.
        let masked: String = self
            .active_draft()?
            .characters()
            .map(|ch| if ch.len_utf16() == 1 { '•' } else { '🔒' })
            .collect();
        let start = byte_index_for_utf16(&masked, range.start);
        let end = byte_index_for_utf16(&masked, range.end).max(start);
        *adjusted = Some(
            utf16_offset_for_byte_index(&masked, start)..utf16_offset_for_byte_index(&masked, end),
        );
        Some(masked[start..end].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let draft = self.active_draft()?;
        let mut range = draft.selection();
        if let Some((_, selected)) = &draft.marked {
            range = range.start + selected.start..range.start + selected.end;
        }
        Some(UTF16Selection {
            range,
            reversed: draft.caret < draft.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.active_draft()?.marked_range()
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(draft) = self.active_draft_mut() {
            draft.commit_marked();
            cx.notify();
        }
    }

    fn paste(&mut self, item: ClipboardItem, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = item.text().map(Zeroizing::new) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.active_draft_mut() else {
            return;
        };
        draft.prepare_replacement(range);
        if text.contains(['\r', '\n']) {
            draft.replace_selection(&Zeroizing::new(text.lines().collect::<Vec<_>>().join(" ")));
        } else {
            draft.replace_selection(text);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.active_draft_mut() else {
            return;
        };
        draft.prepare_replacement(range);
        let end = text.encode_utf16().count();
        let selection = selected.unwrap_or(end..end);
        draft.marked = Some((
            Zeroizing::new(text.to_owned()),
            selection.start.min(end)..selection.end.min(end),
        ));
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let draft = self.active_draft()?;
        let byte = draft.visible_byte(range.start, self.frame.prompts[self.focused].echo);
        draft
            .viewport
            .caret_bounds_for_byte_index(
                byte,
                px(1.),
                px(self.environment.tokens.metrics.form_text_font_size),
            )
            .or(Some(bounds))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.active_draft()?
            .offset(point, self.frame.prompts[self.focused].echo)
    }

    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = self.active_draft_mut()
            && draft.marked.is_none()
        {
            draft.set_range(range);
            cx.notify();
        }
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.active_draft()?.characters().map(char::len_utf16).sum())
    }

    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        self.active_draft().is_some()
    }
}
