use super::*;
use crate::form_state::connection_secret_field_visible;
use crate::input::{field_value, protected, replace_text};
use crate::views::ConnectionImeFrame;
use gpui::{
    Bounds, ClipboardItem, EntityInputHandler, MouseDownEvent, Pixels, Point, UTF16Selection, px,
};
use oxideterm_editor_core::utf16::{
    byte_index_for_utf16 as byte, next_utf16_boundary, previous_utf16_boundary,
    utf16_offset_for_byte_index as units, word_range_for_utf16_offset,
};
use oxideterm_gpui_ui::text_input::{TextInputAnchor, TextInputAnchorId, TextInputViewport};
use std::{collections::HashMap, ops::Range};
use zeroize::Zeroizing;

#[derive(Default)]
pub(super) struct FormInput {
    field: Option<NewConnectionField>,
    anchor: usize,
    caret: usize,
    pub(super) marked: Option<(Zeroizing<String>, Range<usize>)>,
    viewports: RefCell<HashMap<NewConnectionField, TextInputViewport>>,
    bounds: Rc<RefCell<HashMap<NewConnectionField, TextInputAnchor>>>,
}

fn active(state: &ConnectionFormState) -> Option<NewConnectionField> {
    let f = state.form.as_ref()?;
    f.field_focused.then_some(f.focused_field)
}
fn notes_line(
    text: &str,
    tokens: &oxideterm_theme::ThemeTokens,
    window: &mut Window,
) -> gpui::ShapedLine {
    let run = gpui::TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color: gpui::rgb(tokens.ui.text).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
        letter_spacing: None,
    };
    window.text_system().shape_line(
        text.to_owned().into(),
        px(tokens.metrics.ui_text_sm),
        &[run],
        None,
    )
}

impl FormInput {
    pub(super) fn leave_other_field(&mut self, active: Option<NewConnectionField>) {
        if self.field != active {
            self.clear();
        }
    }
    pub(super) fn clear(&mut self) {
        self.field = None;
        self.marked = None;
    }
    fn selection(&self, field: NewConnectionField, text: &str) -> Range<usize> {
        let len = text.encode_utf16().count();
        if self.field != Some(field) {
            return len..len;
        }
        self.anchor.min(self.caret).min(len)..self.anchor.max(self.caret).min(len)
    }
    fn viewport(&self, field: NewConnectionField) -> TextInputViewport {
        self.viewports
            .borrow_mut()
            .entry(field)
            .or_default()
            .clone()
    }
    fn offset(
        &self,
        field: NewConnectionField,
        point: Point<Pixels>,
        state: &ConnectionFormState,
        tokens: &oxideterm_theme::ThemeTokens,
        window: &mut Window,
    ) -> Option<usize> {
        let f = state.form.as_ref()?;
        let text = field_value(f, field)?;
        if field == NewConnectionField::Notes {
            use crate::form_state::{
                CONNECTION_NOTES_LINE_HEIGHT, CONNECTION_NOTES_VERTICAL_PADDING,
            };
            let bounds = self.bounds.borrow().get(&field)?.bounds;
            let y = (point.y - bounds.top() - px(CONNECTION_NOTES_VERTICAL_PADDING)).max(px(0.));
            let row = (y / px(CONNECTION_NOTES_LINE_HEIGHT)).floor() as usize;
            let lines = oxideterm_settings_model::settings_multiline_line_ranges(text);
            let (range, line) = lines.get(row.min(lines.len().saturating_sub(1)))?;
            let layout = notes_line(line, tokens, window);
            let x = point.x - bounds.left() - px(tokens.metrics.ui_control_padding_x);
            return Some(
                range.start
                    + units(
                        line,
                        layout.closest_index_for_x(x.clamp(px(0.), layout.width)),
                    ),
            );
        }
        let pos = self.viewport(field).byte_index_for_position(point)?;
        Some(
            if protected(field)
                && connection_secret_field_visible(f, field) != Some(true)
                && !matches!(
                    field,
                    NewConnectionField::ProxyCommand
                        | NewConnectionField::StandaloneSftpSecondaryProxyCommand
                )
            {
                text.chars()
                    .take(pos / '•'.len_utf8())
                    .map(char::len_utf16)
                    .sum()
            } else {
                units(text, pos)
            },
        )
    }
    pub(super) fn click(
        &mut self,
        field: NewConnectionField,
        event: &MouseDownEvent,
        state: &ConnectionFormState,
        tokens: &oxideterm_theme::ThemeTokens,
        window: &mut Window,
    ) {
        let Some(text) = state.form.as_ref().and_then(|f| field_value(f, field)) else {
            return;
        };
        let end = text.encode_utf16().count();
        let pos = self
            .offset(field, event.position, state, tokens, window)
            .unwrap_or(end);
        let range = match event.click_count {
            2 => word_range_for_utf16_offset(text, pos),
            3.. => 0..end,
            _ => pos..pos,
        };
        if self.field != Some(field) || !event.modifiers.shift {
            self.anchor = range.start
        }
        self.field = Some(field);
        self.caret = range.end;
        self.marked = None;
    }
    pub(super) fn drag(
        &mut self,
        point: Point<Pixels>,
        state: &ConnectionFormState,
        tokens: &oxideterm_theme::ThemeTokens,
        window: &mut Window,
    ) {
        if let Some(field) = active(state)
            && self.field == Some(field)
            && let Some(pos) = self.offset(field, point, state, tokens, window)
        {
            self.caret = pos;
        }
    }
    pub(super) fn frame(
        &self,
        target: ConnectionsInput,
        state: &ConnectionFormState,
    ) -> Option<ConnectionImeFrame> {
        let ConnectionsInput::Form(field) = target else {
            return None;
        };
        let text = field_value(state.form.as_ref()?, field)?;
        let focused = active(state) == Some(field);
        let selection = self.selection(field, text);
        let marked = focused
            .then_some(())
            .and_then(|_| self.marked.as_ref())
            .filter(|_| self.field == Some(field));
        let marked_range =
            marked.map(|(m, _)| selection.start..selection.start + m.encode_utf16().count());
        let notes_with_composition = if field == NewConnectionField::Notes {
            marked.map(|(m, _)| {
                let mut notes = text.to_owned();
                notes.replace_range(byte(text, selection.start)..byte(text, selection.end), m);
                notes
            })
        } else {
            None
        };
        let bounds = self.bounds.clone();
        Some(ConnectionImeFrame {
            anchor_id: TextInputAnchorId(field as u64),
            viewport: self.viewport(field),
            caret_visible: focused,
            selected_range: focused.then_some(selection.clone()),
            active_offset: focused.then_some(if self.field == Some(field) {
                self.caret
            } else {
                selection.end
            }),
            marked_text: marked.map(|(m, _)| m.clone()),
            marked_range,
            notes_with_composition,
            update_anchor: Rc::new(move |a| {
                bounds.borrow_mut().insert(field, a);
            }),
        })
    }
}

impl PortableConnectionFormView {
    pub(super) fn active_field(&self, cx: &App) -> Option<NewConnectionField> {
        active(self.state.read(cx))
    }
    fn value<'a>(&self, field: NewConnectionField, cx: &'a App) -> Option<&'a str> {
        field_value(self.form(cx)?, field)
    }
    fn range(&self, field: NewConnectionField, cx: &App) -> Range<usize> {
        self.input
            .selection(field, self.value(field, cx).unwrap_or_default())
    }
    fn replace(
        &mut self,
        field: NewConnectionField,
        range: Range<usize>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        self.input.marked = None;
        let single_line;
        let text = if field != NewConnectionField::Notes && text.contains(['\r', '\n']) {
            single_line = Zeroizing::new(text.lines().collect::<Vec<_>>().join(" "));
            single_line.as_str()
        } else {
            text
        };
        self.state.update(cx, |s, cx| {
            replace_text(s, field, Some(range.clone()), text);
            cx.notify();
        });
        self.input.field = Some(field);
        self.input.caret = range.start + text.encode_utf16().count();
        self.input.anchor = self.input.caret;
        self.refresh(cx);
    }
    pub(super) fn input_key(
        &mut self,
        e: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(field) = self.active_field(cx) else {
            return false;
        };
        let text = self.value(field, cx).unwrap_or_default();
        let range = self.input.selection(field, text);
        let mods = e.keystroke.modifiers;
        let shortcut = mods.platform || mods.control;
        let key = e.keystroke.key.as_str();
        let end = text.encode_utf16().count();
        match key {
            "a" if shortcut => {
                self.input.anchor = 0;
                self.input.caret = end;
            }
            "c" | "x" if shortcut => {
                if protected(field) {
                    return true;
                }
                let selected = text[byte(text, range.start)..byte(text, range.end)].to_owned();
                cx.write_to_clipboard(ClipboardItem::new_string(selected));
                if key == "x" {
                    self.replace(field, range, "", cx);
                }
            }
            "enter" if field == NewConnectionField::Notes && mods.shift => {
                self.replace(field, range, "\n", cx)
            }
            "backspace" | "delete" => {
                let range = if range.is_empty() {
                    if key == "backspace" {
                        previous_utf16_boundary(text, range.start)..range.end
                    } else {
                        range.start..next_utf16_boundary(text, range.end)
                    }
                } else {
                    range
                };
                self.replace(field, range, "", cx);
            }
            "left" | "right" | "home" | "end" => {
                let caret = if self.input.field == Some(field) {
                    self.input.caret.min(end)
                } else {
                    end
                };
                let dest = match key {
                    "home" => 0,
                    "end" => end,
                    "left" if !mods.shift && !range.is_empty() => range.start,
                    "right" if !mods.shift && !range.is_empty() => range.end,
                    "left" => previous_utf16_boundary(text, caret),
                    _ => next_utf16_boundary(text, caret),
                };
                if !mods.shift {
                    self.input.anchor = dest
                } else if self.input.field != Some(field) {
                    self.input.anchor = caret
                }
                self.input.caret = dest;
            }
            _ => return false,
        }
        self.input.field = Some(field);
        window.notify_text_input_changed(&self.focus);
        self.refresh(cx);
        true
    }
}
impl EntityInputHandler for PortableConnectionFormView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let field = self.active_field(cx)?;
        let text = self.value(field, cx)?;
        let selected = self.range(field, cx);
        let marked = self
            .input
            .marked
            .as_ref()
            .filter(|_| self.input.field == Some(field));
        let (prefix, preedit, suffix) = if let Some((m, _)) = marked {
            (
                &text[..byte(text, selected.start)],
                m.as_str(),
                &text[byte(text, selected.end)..],
            )
        } else {
            (text, "", "")
        };
        // A browser input mirror needs character geometry but never the credential itself.
        let mirror: String = prefix
            .chars()
            .chain(preedit.chars())
            .chain(suffix.chars())
            .map(|c| {
                if protected(field) {
                    if c.len_utf16() == 1 { '•' } else { '🔒' }
                } else {
                    c
                }
            })
            .collect();
        let a = byte(&mirror, range.start);
        let b = byte(&mirror, range.end).max(a);
        *adjusted = Some(units(&mirror, a)..units(&mirror, b));
        Some(mirror[a..b].to_owned())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let field = self.active_field(cx)?;
        let mut range = self.range(field, cx);
        if self.input.field == Some(field)
            && let Some((_, sel)) = &self.input.marked
        {
            range = range.start + sel.start..range.start + sel.end
        }
        Some(UTF16Selection {
            range,
            reversed: self.input.caret < self.input.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        let field = self.active_field(cx)?;
        if self.input.field != Some(field) {
            return None;
        }
        let start = self.range(field, cx).start;
        Some(start..start + self.input.marked.as_ref()?.0.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some((text, _)) = self.input.marked.take()
            && let Some(field) = self.active_field(cx)
            && self.input.field == Some(field)
        {
            self.replace(field, self.range(field, cx), &text, cx)
        }
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.active_field(cx) else {
            return;
        };
        let range = range
            .filter(|r| Some(r) != self.marked_text_range(window, cx).as_ref())
            .unwrap_or_else(|| self.range(field, cx));
        self.replace(field, range, text, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.active_field(cx) else {
            return;
        };
        if let Some(r) = range.filter(|r| Some(r) != self.marked_text_range(window, cx).as_ref()) {
            self.input.anchor = r.start;
            self.input.caret = r.end
        } else if self.input.field != Some(field) {
            self.input.caret = self.range(field, cx).end;
            self.input.anchor = self.input.caret;
        }
        self.input.field = Some(field);
        let len = text.encode_utf16().count();
        let r = selection.unwrap_or(len..len);
        self.input.marked = Some((
            Zeroizing::new(text.to_owned()),
            r.start.min(len)..r.end.min(len),
        ));
        self.refresh(cx);
    }
    fn paste(&mut self, item: ClipboardItem, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(field), Some(text)) = (self.active_field(cx), item.text().map(Zeroizing::new))
        else {
            return;
        };
        let range = self.range(field, cx);
        if field == NewConnectionField::Host
            && range.start == 0
            && range.end
                == self
                    .value(field, cx)
                    .unwrap_or_default()
                    .encode_utf16()
                    .count()
        {
            let parsed = self.state.update(cx, |s, cx| {
                let Some(f) = s.form.as_mut() else {
                    return false;
                };
                let previous = f.selected_field;
                f.selected_field = Some(field);
                if f.apply_address_paste(&text) {
                    f.error = None;
                    cx.notify();
                    true
                } else {
                    f.selected_field = previous;
                    false
                }
            });
            if parsed {
                self.input.clear();
                self.refresh(cx);
                return;
            }
        }
        self.replace_text_in_range(Some(range), &text, window, cx)
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let field = self.active_field(cx)?;
        let text = self.value(field, cx)?;
        if field == NewConnectionField::Notes {
            use crate::form_state::{
                CONNECTION_NOTES_LINE_HEIGHT, CONNECTION_NOTES_VERTICAL_PADDING,
            };
            let anchor = self.input.bounds.borrow().get(&field)?.bounds;
            let tokens = &self.theme.tokens;
            for (row, (line_range, line)) in
                oxideterm_settings_model::settings_multiline_line_ranges(text)
                    .into_iter()
                    .enumerate()
            {
                if range.start <= line_range.end {
                    let x = notes_line(&line, tokens, window)
                        .x_for_index(byte(&line, range.start.saturating_sub(line_range.start)));
                    return Some(Bounds::new(
                        gpui::point(
                            anchor.left() + px(tokens.metrics.ui_control_padding_x) + x,
                            anchor.top()
                                + px(CONNECTION_NOTES_VERTICAL_PADDING
                                    + row as f32 * CONNECTION_NOTES_LINE_HEIGHT),
                        ),
                        gpui::size(px(1.), px(CONNECTION_NOTES_LINE_HEIGHT)),
                    ));
                }
            }
        }
        let pos = byte(text, range.start);
        let visual = if protected(field)
            && connection_secret_field_visible(self.form(cx)?, field) == Some(false)
        {
            text[..pos].chars().count() * '•'.len_utf8()
        } else {
            pos
        };
        self.input
            .viewport(field)
            .caret_bounds_for_byte_index(
                visual,
                px(1.),
                px(self.theme.tokens.metrics.form_text_font_size),
            )
            .or_else(|| self.input.bounds.borrow().get(&field).map(|a| a.bounds))
            .or(Some(bounds))
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        self.input.offset(
            self.active_field(cx)?,
            point,
            self.state.read(cx),
            &self.theme.tokens,
            window,
        )
    }
    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(field) = self.active_field(cx)
            && self.input.marked.is_none()
        {
            let text = self.value(field, cx).unwrap_or_default();
            self.input.anchor = units(text, byte(text, range.start));
            self.input.caret = units(text, byte(text, range.end));
            self.input.field = Some(field);
            self.refresh(cx);
        }
    }
    fn text_length_utf16(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<usize> {
        let field = self.active_field(cx)?;
        let length = self.value(field, cx)?.encode_utf16().count();
        if self.input.field == Some(field)
            && let Some((marked, _)) = &self.input.marked
        {
            Some(length - self.range(field, cx).len() + marked.encode_utf16().count())
        } else {
            Some(length)
        }
    }
    fn accepts_text_input(&self, _: &mut Window, cx: &mut Context<Self>) -> bool {
        self.active_field(cx).is_some()
    }
}
