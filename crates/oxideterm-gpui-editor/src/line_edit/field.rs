use super::LineEdit;
use gpui::{prelude::*, *};
use oxideterm_editor_core::utf16::{byte_index_for_utf16, utf16_offset_for_byte_index};
use oxideterm_gpui_ui::text_input::{TextInputView, text_input_with_viewport};
use oxideterm_theme::ThemeTokens;
use std::ops::Range;
use zeroize::Zeroizing;

#[derive(Clone, Copy)]
pub enum FieldEvent {
    Changed,
    Submit,
    Cancel,
}

/// A plain single-line control. Its focus handle owns the platform input method.
pub struct LineField {
    draft: LineEdit,
    focus: FocusHandle,
    tokens: ThemeTokens,
    placeholder: String,
    borderless: bool,
    secret: bool,
    focus_events: Option<[Subscription; 2]>,
}

impl EventEmitter<FieldEvent> for LineField {}
impl Focusable for LineField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl LineField {
    pub fn new(
        tokens: ThemeTokens,
        placeholder: String,
        borderless: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            draft: LineEdit::default(),
            focus: cx.focus_handle(),
            tokens,
            placeholder,
            borderless,
            secret: false,
            focus_events: None,
        }
    }
    pub fn environment(
        &mut self,
        tokens: ThemeTokens,
        placeholder: String,
        cx: &mut Context<Self>,
    ) {
        if self.tokens != tokens || self.placeholder != placeholder {
            self.tokens = tokens;
            self.placeholder = placeholder;
            cx.notify();
        }
    }
    pub fn text(&self) -> &str {
        &self.draft.value
    }
    pub fn masked(mut self) -> Self {
        self.secret = true;
        self
    }
    /// Reconcile a host edit without emitting it back as a user edit or discarding an active composition.
    pub fn sync_value(&mut self, value: &str, cx: &mut Context<Self>) {
        if self.text() != value {
            self.set_value(value, false);
            cx.notify();
        }
    }
    pub fn reset(&mut self, value: &str, select_all: bool, cx: &mut Context<Self>) {
        self.set_value(value, select_all);
        self.changed(cx);
    }
    fn set_value(&mut self, value: &str, select_all: bool) {
        self.draft.marked = None;
        self.draft.value = Zeroizing::new(value.to_owned());
        let end = value.encode_utf16().count();
        self.draft
            .set_range(if select_all { 0..end } else { end..end });
    }
    fn changed(&self, cx: &mut Context<Self>) {
        cx.emit(FieldEvent::Changed);
        cx.notify();
    }

    fn key(&mut self, event: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if self.draft.marked.is_some() {
            // The IME owns candidate selection; Escape must not dismiss the enclosing dialog.
            cx.stop_propagation();
            return;
        }
        let mods = event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();
        match key {
            "enter" => cx.emit(FieldEvent::Submit),
            "escape" => cx.emit(FieldEvent::Cancel),
            "c" | "x" if mods.platform || mods.control => {
                let range = self.draft.selection();
                if !self.secret && !range.is_empty() {
                    let value = &self.draft.value;
                    let selected = &value[byte_index_for_utf16(value, range.start)
                        ..byte_index_for_utf16(value, range.end)];
                    cx.write_to_clipboard(ClipboardItem::new_string(selected.to_owned()));
                    if key == "x" {
                        self.draft.replace_selection("");
                        self.changed(cx);
                    }
                }
            }
            key if self.draft.edit_key(key, mods) => {
                if matches!(key, "backspace" | "delete") {
                    self.changed(cx);
                } else {
                    cx.notify();
                }
            }
            _ => return,
        }
        w.prevent_default();
        w.notify_text_input_changed(&self.focus);
        cx.stop_propagation();
    }
}

impl Render for LineField {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_events.is_none() {
            self.focus_events = Some([
                cx.on_focus(&self.focus, w, |_, _, cx| cx.notify()),
                cx.on_blur(&self.focus, w, |v, _, cx| {
                    if v.draft.marked.is_some() {
                        v.draft.commit_marked();
                        v.changed(cx);
                    } else {
                        cx.notify();
                    }
                }),
            ]);
        }
        let focused = self.focus.is_focused(w);
        let field = text_input_with_viewport(
            &self.tokens,
            TextInputView {
                value: self.text(),
                placeholder: self.placeholder.clone(),
                focused,
                caret_visible: focused,
                secret: self.secret,
                selected_all: false,
                selected_range: focused.then(|| self.draft.selection()),
                marked_text: self.draft.marked.as_ref().map(|(text, _)| text.as_str()),
            },
            &self.draft.viewport,
            Some(self.draft.caret),
        );
        field
            .id("single-line-field")
            .track_focus(&self.focus)
            .relative()
            .when(self.borderless, |e| {
                e.border_0().bg(transparent_black()).px_0().h_full()
            })
            .when(focused, |e| {
                let owner = cx.entity();
                let focus = self.focus.clone();
                e.child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, w, cx| {
                            w.handle_input(&focus, ElementInputHandler::new(bounds, owner), cx)
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|v, e: &MouseDownEvent, w, cx| {
                    if v.draft.marked.is_some() {
                        v.draft.commit_marked();
                        v.changed(cx);
                    }
                    w.focus(&v.focus, cx);
                    v.draft.select_at(e, !v.secret);
                    w.notify_text_input_changed(&v.focus);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|v, e: &MouseMoveEvent, w, cx| {
                if e.dragging()
                    && v.focus.is_focused(w)
                    && v.draft.marked.is_none()
                    && let Some(offset) = v.draft.offset(e.position, !v.secret)
                {
                    v.draft.caret = offset;
                    w.notify_text_input_changed(&v.focus);
                    cx.notify();
                }
            }))
            .on_key_down(cx.listener(Self::key))
    }
}

impl EntityInputHandler for LineField {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        // The browser mirrors this document in a DOM input. Mask without changing UTF-16 offsets.
        let doc: String = if self.secret {
            self.draft
                .characters()
                .map(|c| if c.len_utf16() == 1 { '•' } else { '🔒' })
                .collect()
        } else {
            self.draft.characters().collect()
        };
        let start = byte_index_for_utf16(&doc, range.start);
        let end = byte_index_for_utf16(&doc, range.end).max(start);
        *adjusted =
            Some(utf16_offset_for_byte_index(&doc, start)..utf16_offset_for_byte_index(&doc, end));
        Some(doc[start..end].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let mut range = self.draft.selection();
        if let Some((_, sel)) = &self.draft.marked {
            range = range.start + sel.start..range.start + sel.end;
        }
        Some(UTF16Selection {
            range,
            reversed: self.draft.caret < self.draft.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.draft.marked_range()
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.draft.marked.is_some() {
            self.draft.commit_marked();
            self.changed(cx);
        }
    }
    fn paste(&mut self, item: ClipboardItem, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = item.text().map(Zeroizing::new) {
            self.replace_text_in_range(None, &text, w, cx);
        }
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft.prepare_replacement(range);
        if text.contains(['\r', '\n']) {
            let single_line = Zeroizing::new(text.lines().collect::<Vec<_>>().join(" "));
            self.draft.replace_selection(&single_line);
        } else {
            self.draft.replace_selection(text);
        }
        self.changed(cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft.prepare_replacement(range);
        let end = text.encode_utf16().count();
        let sel = selected.unwrap_or(end..end);
        self.draft.marked = Some((
            Zeroizing::new(text.into()),
            sel.start.min(end)..sel.end.min(end),
        ));
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        self.draft
            .viewport
            .caret_bounds_for_byte_index(
                self.draft.visible_byte(r.start, !self.secret),
                px(1.),
                px(self.tokens.metrics.form_text_font_size),
            )
            .or(Some(bounds))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.draft.offset(p, !self.secret)
    }
    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.draft.marked.is_none() {
            self.draft.set_range(range);
            cx.notify();
        }
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.draft.characters().map(char::len_utf16).sum())
    }
    fn accepts_text_input(&self, w: &mut Window, _: &mut Context<Self>) -> bool {
        self.focus.is_focused(w)
    }
}

#[cfg(test)]
mod tests {
    use super::LineField;
    use gpui::{EntityInputHandler, TestAppContext};

    #[gpui::test]
    fn composing_over_a_utf16_selection_updates_the_draft_only_on_commit(cx: &mut TestAppContext) {
        let (field, cx) = cx.add_window_view(|w, cx| {
            let mut f = LineField::new(oxideterm_theme::default_tokens(), String::new(), false, cx);
            f.reset("A🚀B", false, cx);
            w.focus(&f.focus, cx);
            f
        });
        cx.update(|w, app| {
            w.draw(app).clear(app);
            field.update(app, |f, cx| {
                f.set_selected_text_range(1..3, w, cx);
                f.replace_and_mark_text_in_range(None, "中文", Some(2..2), w, cx);
                assert_eq!(f.text(), "A🚀B");
                assert_eq!(
                    f.text_for_range(0..4, &mut None, w, cx).as_deref(),
                    Some("A中文B")
                );
                assert_eq!(f.marked_text_range(w, cx), Some(1..3));
                f.replace_text_in_range(Some(1..3), "界", w, cx);
            });
        });
        cx.simulate_input("!");
        cx.update(|_, app| assert_eq!(field.read(app).text(), "A界!B"));
    }

    #[gpui::test]
    fn masked_composition_keeps_utf16_offsets_without_exposing_text(cx: &mut TestAppContext) {
        let (field, cx) = cx.add_window_view(|w, cx| {
            let mut f = LineField::new(oxideterm_theme::default_tokens(), String::new(), false, cx)
                .masked();
            f.reset("私🚀", false, cx);
            w.focus(&f.focus, cx);
            f
        });
        cx.update(|w, app| {
            field.update(app, |f, cx| {
                f.replace_and_mark_text_in_range(None, "密", Some(1..1), w, cx);
                assert_eq!(
                    f.text_for_range(0..4, &mut None, w, cx).as_deref(),
                    Some("•🔒•")
                );
                assert_eq!(f.marked_text_range(w, cx), Some(3..4));
                f.unmark_text(w, cx);
                assert_eq!(f.text(), "私🚀密");
            })
        });
    }
}
