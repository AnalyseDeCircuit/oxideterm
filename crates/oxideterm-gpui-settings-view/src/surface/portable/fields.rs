use super::*;
use crate::components::SettingsImeFrame;
use gpui::{Bounds, EntityInputHandler, Pixels, Point, UTF16Selection};
use oxideterm_editor_core::utf16::*;
use oxideterm_gpui_ui::text_input::{TextInputAnchorId, TextInputViewport};
use oxideterm_settings_model::{
    SettingsInputDraftApply, apply_persisted_settings_input_draft, persisted_settings_input_value,
};
use std::{cell::RefCell, ops::Range};
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Default)]
struct FieldLayout {
    viewport: TextInputViewport,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

#[derive(Default)]
pub(super) struct Fields {
    pub(super) active: Option<SettingsInput>,
    draft: Zeroizing<String>,
    // The focused editor and the parked probe password exchange ownership on blur.
    password: Zeroizing<String>,
    anchor: usize,
    caret: usize,
    // Composition replaces the current selection visually until the platform commits it.
    marked: Option<(Zeroizing<String>, Range<usize>)>,
    layouts: RefCell<HashMap<SettingsInput, FieldLayout>>,
}

impl Fields {
    pub(super) fn blur(&mut self) {
        if self.active == Some(SettingsInput::NetworkProxyPassword) {
            std::mem::swap(&mut self.password, &mut self.draft);
        }
        self.active = None;
        self.marked = None;
        self.draft.zeroize();
    }
    pub(super) fn value(&self, id: SettingsInput) -> Option<&str> {
        if self.active == Some(id) {
            Some(self.draft.as_str())
        } else if id == SettingsInput::NetworkProxyPassword {
            Some(self.password.as_str())
        } else {
            None
        }
    }
    pub(super) fn take_password(&mut self) -> Zeroizing<String> {
        if self.active == Some(SettingsInput::NetworkProxyPassword) {
            self.blur();
        }
        std::mem::take(&mut self.password)
    }
    fn editing_password(&self) -> bool {
        matches!(
            self.active,
            Some(SettingsInput::NetworkProxyPassword | SettingsInput::AiProviderApiKey(_))
        )
    }
    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }
    fn layout(&self) -> Option<FieldLayout> {
        self.layouts.borrow().get(&self.active?).cloned()
    }
    pub(super) fn hit(&self, point: Point<Pixels>) -> bool {
        self.layout()
            .and_then(|l| l.bounds.get())
            .is_some_and(|b| b.contains(&point))
    }
    fn offset(&self, position: Point<Pixels>) -> Option<usize> {
        let byte = self.layout()?.viewport.byte_index_for_position(position)?;
        let document = self.document();
        Some(if self.editing_password() {
            // The viewport hit-tests the bullets, not the UTF-8 bytes of the password.
            utf16_offset_for_char_index(&document, byte / '•'.len_utf8())
        } else {
            utf16_offset_for_byte_index(&document, byte)
        })
    }
    pub(super) fn select_at(&mut self, position: Point<Pixels>, count: usize, extend: bool) {
        self.marked = None;
        let pos = self
            .offset(position)
            .unwrap_or_else(|| self.draft.encode_utf16().count());
        let selection = match count {
            2 => word_range_for_utf16_offset(&self.draft, pos),
            3.. => 0..self.draft.encode_utf16().count(),
            _ => pos..pos,
        };
        if !extend {
            self.anchor = selection.start;
        }
        self.caret = selection.end;
    }
    pub(super) fn drag_to(&mut self, position: Point<Pixels>) {
        if let Some(offset) = self.offset(position) {
            self.caret = offset;
        }
    }
    fn document(&self) -> String {
        let ime_text = |value: &str| {
            if self.editing_password() {
                // The IME needs UTF-16 geometry, including surrogate pairs, not the credential.
                value
                    .chars()
                    .map(|ch| if ch.len_utf16() == 1 { '•' } else { '🔒' })
                    .collect()
            } else {
                value.to_owned()
            }
        };
        let mut document = ime_text(&self.draft);
        if let Some((text, _)) = &self.marked {
            replace_utf16(&mut document, Some(self.selection()), &ime_text(text));
        }
        document
    }
    fn commit_marked(&mut self) {
        if let Some((text, _)) = self.marked.take() {
            let range = self.selection();
            replace_utf16(&mut self.draft, Some(range), &text);
        }
    }
    fn marked_range(&self) -> Option<Range<usize>> {
        let start = self.selection().start;
        Some(start..start + self.marked.as_ref()?.0.encode_utf16().count())
    }
    fn set_range(&mut self, range: Range<usize>) {
        let len = self.draft.encode_utf16().count();
        self.anchor = range.start.min(len);
        self.caret = range.end.min(len);
    }
}

impl SettingsImeSource for PageState {
    fn frame<'a>(
        &'a self,
        input: SettingsInput,
        value: &'a str,
        _: &'a App,
    ) -> SettingsImeFrame<'a> {
        let layout = self
            .fields
            .layouts
            .borrow_mut()
            .entry(input)
            .or_default()
            .clone();
        let focused = self.fields.active == Some(input);
        let stored_value = self.fields.value(input).or_else(|| match input {
            SettingsInput::AiProviderApiKey(index) => self
                .ai_api_key_drafts
                .get(self.provider_id_at(index)?)
                .map(|k| k.as_str()),
            SettingsInput::AiProviderNewModel(index) => self
                .ai_new_model_drafts
                .get(self.provider_id_at(index)?)
                .map(String::as_str),
            _ => None,
        });
        SettingsImeFrame {
            input,
            value: stored_value.unwrap_or(value),
            focused,
            caret_visible: focused,
            selected_range: focused.then(|| self.fields.selection()),
            active_offset: focused.then_some(self.fields.caret),
            marked_text: if focused {
                self.fields.marked.as_ref().map(|m| m.0.as_str())
            } else {
                None
            },
            anchor_id: TextInputAnchorId(input.anchor_key()),
            viewport: layout.viewport,
            update_anchor: Rc::new(move |anchor| layout.bounds.set(Some(anchor.bounds))),
        }
    }
}

impl PageState {
    pub(super) fn provider_id_at(&self, index: usize) -> Option<&str> {
        self.settings.ai.providers.get(index)?.get("id")?.as_str()
    }

    pub(super) fn focus_input(&mut self, id: SettingsInput) {
        if id == SettingsInput::SettingsSearch {
            self.search_open = true;
        }
        if self.fields.active == Some(id) {
            return;
        }
        if id == SettingsInput::NetworkProxyPassword {
            self.fields.blur();
            std::mem::swap(&mut self.fields.draft, &mut self.fields.password);
            self.fields.active = Some(id);
            self.fields.caret = self.fields.draft.encode_utf16().count();
            self.fields.anchor = self.fields.caret;
            return;
        }
        let value = match id {
            SettingsInput::KnownHostsSearch => Some(self.known_hosts.query.clone()),
            SettingsInput::NetworkProxyTestHost => Some(self.probe.form.test_host.clone()),
            SettingsInput::NetworkProxyTestPort => Some(self.probe.form.test_port.clone()),
            SettingsInput::SettingsSearch => Some(self.query.clone()),
            SettingsInput::KeybindingSearch => {
                Some(self.shortcuts.keybinding_search_query().to_owned())
            }
            SettingsInput::AiProviderApiKey(index) => Some(
                self.provider_id_at(index)
                    .and_then(|id| self.ai_api_key_drafts.get(id))
                    .map(|k| k.as_str().to_owned())
                    .unwrap_or_default(),
            ),
            SettingsInput::AiProviderNewModel(index) => Some(
                self.provider_id_at(index)
                    .and_then(|id| self.ai_new_model_drafts.get(id))
                    .cloned()
                    .unwrap_or_default(),
            ),
            SettingsInput::CustomThemeName => self.editor.as_ref().map(|e| e.name.clone()),
            SettingsInput::CustomThemeTerminalColor(i) => self
                .editor
                .as_ref()
                .and_then(|e| e.terminal_colors.get(i).cloned()),
            SettingsInput::CustomThemeUiColor(i) => self
                .editor
                .as_ref()
                .and_then(|e| e.ui_colors.get(i).cloned()),
            _ => persisted_settings_input_value(&self.settings, id),
        };
        if let Some(value) = value {
            self.fields.blur();
            self.fields.active = Some(id);
            self.fields.caret = value.encode_utf16().count();
            self.fields.anchor = self.fields.caret;
            self.fields.draft = Zeroizing::new(value);
        }
    }

    fn apply_input(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.fields.active else {
            return;
        };
        let draft = &*self.fields.draft;
        match input {
            SettingsInput::NetworkProxyPassword => {
                self.probe
                    .password_profile
                    .clone_from(&self.settings.network.upstream_proxy);
                self.probe.clear_result();
            }
            SettingsInput::KnownHostsSearch => self.known_hosts.query.clone_from(draft),
            SettingsInput::NetworkProxyTestHost | SettingsInput::NetworkProxyTestPort => {
                if input == SettingsInput::NetworkProxyTestHost {
                    self.probe.form.test_host.clone_from(draft);
                } else {
                    self.probe.form.test_port.clone_from(draft);
                }
                self.probe.clear_result();
            }
            SettingsInput::SettingsSearch => {
                self.query.clone_from(draft);
                self.search.apply(draft.trim().to_owned());
                self.search_open = true;
            }
            SettingsInput::KeybindingSearch => {
                self.shortcuts.keybinding_search_query.clone_from(draft)
            }
            SettingsInput::AiProviderApiKey(index) => {
                if let Some(id) = self.provider_id_at(index).map(str::to_owned) {
                    self.ai_api_key_drafts
                        .insert(id, Zeroizing::new(draft.to_owned()));
                }
            }
            SettingsInput::AiProviderNewModel(index) => {
                if let Some(id) = self.provider_id_at(index).map(str::to_owned) {
                    self.ai_new_model_drafts.insert(id, draft.to_owned());
                }
            }
            SettingsInput::CustomThemeName => {
                if let Some(e) = self.editor.as_mut() {
                    e.name.clone_from(draft);
                }
            }
            SettingsInput::CustomThemeTerminalColor(i) | SettingsInput::CustomThemeUiColor(i) => {
                if let Some(e) = self.editor.as_mut() {
                    let colors = if matches!(input, SettingsInput::CustomThemeTerminalColor(_)) {
                        &mut e.terminal_colors
                    } else {
                        &mut e.ui_colors
                    };
                    if let Some(v) = colors.get_mut(i) {
                        v.clone_from(draft);
                    }
                }
            }
            _ => {
                if matches!(
                    apply_persisted_settings_input_draft(&mut self.settings, input, draft),
                    SettingsInputDraftApply::Applied
                ) {
                    self.changed(cx);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn field_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        if self.fields.marked.is_some() {
            return false;
        }
        let m = event.keystroke.modifiers;
        let range = self.fields.selection();
        let length = self.fields.draft.encode_utf16().count();
        match key {
            "enter" | "escape" => {
                if self.fields.active == Some(SettingsInput::SettingsSearch) {
                    if key == "enter"
                        && let Some(first) = self.search.results(&self.i18n).first()
                    {
                        self.navigate(first.tab, Some(first), cx);
                        return true;
                    }
                    self.search_open = false;
                }
                if key == "escape" && self.fields.editing_password() {
                    self.clear_proxy_password();
                } else {
                    self.fields.blur();
                }
            }
            "a" if m.platform || m.control => self.fields.set_range(0..length),
            "c" | "x" if m.platform || m.control => {
                if !self.fields.editing_password() && !range.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(utf16_slice(
                        &self.fields.draft,
                        range,
                    )));
                    if key == "x" {
                        self.replace_text_in_range(None, "", window, cx);
                    }
                }
            }
            "backspace" | "delete" => {
                let range = if !range.is_empty() {
                    range
                } else if key == "backspace" {
                    previous_utf16_boundary(&self.fields.draft, range.start)..range.end
                } else {
                    range.start..next_utf16_boundary(&self.fields.draft, range.end)
                };
                self.replace_text_in_range(Some(range), "", window, cx);
            }
            "left" | "right" | "home" | "end" => {
                let pos = match key {
                    "home" => 0,
                    "end" => length,
                    "left" if !m.shift && !range.is_empty() => range.start,
                    "right" if !m.shift && !range.is_empty() => range.end,
                    "left" => previous_utf16_boundary(&self.fields.draft, self.fields.caret),
                    _ => next_utf16_boundary(&self.fields.draft, self.fields.caret),
                };
                if !m.shift {
                    self.fields.anchor = pos;
                }
                self.fields.caret = pos;
            }
            _ => return false,
        }
        cx.notify();
        true
    }
}

impl EntityInputHandler for PageState {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        self.fields.active?;
        let text = self.fields.document();
        let len = text.encode_utf16().count();
        let range = range.start.min(len)..range.end.min(len);
        *adjusted = Some(range.clone());
        Some(utf16_slice(&text, range))
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.fields.active?;
        let mut range = self.fields.selection();
        if let Some((_, selection)) = &self.fields.marked {
            range = range.start + selection.start..range.start + selection.end;
        }
        Some(UTF16Selection {
            range,
            reversed: self.fields.caret < self.fields.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.fields.marked_range()
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((text, _)) = self.fields.marked.take() {
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
        if self.fields.active.is_none() {
            return;
        }
        let replace = match (range, self.fields.marked_range()) {
            (Some(range), Some(marked)) if range != marked => {
                self.fields.commit_marked();
                range
            }
            (Some(range), None) => range,
            _ => self.fields.selection(),
        };
        self.fields.marked = None;
        let text = Zeroizing::new(text.replace(['\r', '\n'], ""));
        let caret = replace.start.min(self.fields.draft.encode_utf16().count())
            + text.encode_utf16().count();
        replace_utf16(&mut self.fields.draft, Some(replace), &text);
        self.fields.set_range(caret..caret);
        self.apply_input(cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.fields.active.is_none() {
            return;
        }
        if let Some(range) = range
            && Some(&range) != self.fields.marked_range().as_ref()
        {
            self.fields.commit_marked();
            self.fields.set_range(range);
        }
        let end = text.encode_utf16().count();
        self.fields.marked = Some((
            Zeroizing::new(text.to_owned()),
            selected.unwrap_or(end..end),
        ));
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.fields.layout()?;
        let bounds = layout.bounds.get()?;
        let document = self.fields.document();
        let byte = byte_index_for_utf16(&document, range.start);
        let visible_byte = if self.fields.editing_password() {
            document[..byte].chars().count() * '•'.len_utf8()
        } else {
            byte
        };
        layout
            .viewport
            .caret_bounds_for_byte_index(visible_byte, px(1.0), bounds.size.height)
            .or(Some(bounds))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.fields.offset(p)
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        self.fields.active?;
        Some(self.fields.document().encode_utf16().count())
    }
    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.fields.marked.is_none() {
            self.fields.set_range(range);
            cx.notify();
        }
    }
    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        self.fields.active.is_some()
    }
}
