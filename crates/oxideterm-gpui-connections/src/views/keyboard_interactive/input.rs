use super::{PortableKeyboardInteractiveDialog, PromptDraft};
use gpui::{
    Bounds, ClipboardItem, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window, px,
};
use oxideterm_editor_core::utf16::{byte_index_for_utf16, utf16_offset_for_byte_index};
use std::ops::Range;
use zeroize::Zeroizing;
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
