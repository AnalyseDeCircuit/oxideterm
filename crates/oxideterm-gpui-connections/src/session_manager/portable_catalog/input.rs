use super::*;
use crate::views::ConnectionImeFrame;
use gpui::{Bounds, ClipboardItem, EntityInputHandler, Pixels, Point, UTF16Selection, px};
use oxideterm_editor_core::utf16::{byte_index_for_utf16, utf16_offset_for_byte_index};
use oxideterm_gpui_ui::text_input::TextInputAnchorId;
use std::ops::Range;
use zeroize::Zeroizing;

impl PortableCatalogView {
    fn edit(&self, field: SessionManagerInput) -> Option<&LineEdit> {
        match field {
            SessionManagerInput::Search => Some(&self.search),
            SessionManagerInput::GroupName => Some(&self.group_name),
            _ => None,
        }
    }
    pub(super) fn edit_mut(&mut self, field: SessionManagerInput) -> Option<&mut LineEdit> {
        match field {
            SessionManagerInput::Search => Some(&mut self.search),
            SessionManagerInput::GroupName => Some(&mut self.group_name),
            _ => None,
        }
    }
    fn active_edit(&self, cx: &App) -> Option<&LineEdit> {
        self.edit(self.manager.read(cx).focused_input?)
    }
    pub(super) fn active_edit_mut(&mut self, cx: &App) -> Option<&mut LineEdit> {
        self.edit_mut(self.manager.read(cx).focused_input?)
    }
    pub(super) fn sync_fields(&mut self, state: &SessionManagerState) {
        for (field, value) in [
            (SessionManagerInput::Search, &state.search_query),
            (SessionManagerInput::GroupName, &state.group_name_draft),
        ] {
            if let Some(edit) = self.edit_mut(field) {
                if edit.value.as_str() != value {
                    edit.marked = None;
                    edit.value = Zeroizing::new(value.clone());
                    let end = value.encode_utf16().count();
                    edit.set_range(end..end);
                }
                if state.focused_input != Some(field) {
                    edit.marked = None;
                }
            }
        }
    }
    pub(super) fn ime_frame(
        &self,
        field: SessionManagerInput,
        cx: &App,
    ) -> Option<ConnectionImeFrame> {
        let edit = self.edit(field)?;
        let focused = self.manager.read(cx).focused_input == Some(field);
        Some(ConnectionImeFrame {
            anchor_id: TextInputAnchorId(field.anchor_key()),
            viewport: edit.viewport.clone(),
            caret_visible: focused,
            selected_range: focused.then(|| edit.selection()),
            active_offset: focused.then_some(edit.caret),
            marked_text: edit.marked.as_ref().map(|(text, _)| text.clone()),
            marked_range: edit.marked_range(),
            notes_with_composition: None,
            update_anchor: Rc::new(|_| {}),
        })
    }
    fn commit_input(&mut self, cx: &mut Context<Self>) {
        let Some(field) = self.manager.read(cx).focused_input else {
            return;
        };
        let Some(edit) = self.edit(field) else { return };
        let value = edit.value.as_str();
        self.manager.update(cx, |s, cx| {
            match field {
                SessionManagerInput::Search => s.search_query = value.to_owned(),
                SessionManagerInput::GroupName => {
                    s.group_editor_error = None;
                    s.group_name_draft = value.to_owned();
                }
                _ => return,
            }
            cx.notify();
        });
        if field == SessionManagerInput::Search {
            self.prune_selection(cx);
        }
        self.refresh(cx);
    }
    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use SessionManagerInput::Search;
        if self
            .active_edit(cx)
            .is_some_and(|edit| edit.marked.is_some())
        {
            return;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let state = self.manager.read(cx);
        if state.delete_confirm.is_some() {
            use oxideterm_gpui_ui::confirm::{
                ConfirmDialogAction as Button, ModalFooterKeyAction as Action,
                modal_footer_key_action,
            };
            if modifiers.platform || modifiers.control {
                return;
            }
            match modal_footer_key_action(
                key,
                modifiers.shift,
                &[Button::Cancel, Button::Confirm],
                state.delete_focus,
                Button::Cancel,
            ) {
                Some(Action::Cancel | Action::Activate(Button::Cancel)) => {
                    self.manager.update(cx, SessionManagerState::cancel_delete)
                }
                Some(Action::Activate(Button::Confirm)) => {
                    self.action(CatalogAction::ConfirmDelete, cx)
                }
                Some(Action::Focus(button)) => self.manager.update(cx, |s, cx| {
                    s.delete_focus = Some(button);
                    cx.notify();
                }),
                None => return,
            }
        } else if key == "escape"
            && (state.row_action_menu.is_some()
                || state.sort_menu_open
                || state.view_mode_menu_open
                || state.show_batch_move)
        {
            self.manager.update(cx, |s, cx| {
                close_session_menu_state(s);
                cx.notify();
            });
        } else if state.show_group_manager && state.group_editor.is_none() && key == "escape" {
            self.manager
                .update(cx, SessionManagerState::close_session_group_manager);
        } else {
            let (handled, request) = self
                .manager
                .update(cx, |s, cx| s.group_editor_key(event, cx));
            if let Some(request) = request {
                self.dispatch(request, Some(window), cx);
            }
            if !handled {
                if (modifiers.platform || modifiers.control)
                    && key == "f"
                    && !self.manager.read(cx).show_group_manager
                {
                    self.manager.update(cx, |s, cx| {
                        s.focused_input = Some(Search);
                        cx.notify();
                    });
                    self.search.edit_key("a", modifiers);
                } else if key == "enter"
                    && self.manager.read(cx).focused_input == Some(SessionManagerInput::GroupName)
                {
                    self.action(CatalogAction::SaveGroup, cx);
                } else if let Some(edit) = self.active_edit_mut(cx) {
                    if (modifiers.platform || modifiers.control) && matches!(key, "c" | "x") {
                        let range = edit.selection();
                        let text = &edit.value[byte_index_for_utf16(&edit.value, range.start)
                            ..byte_index_for_utf16(&edit.value, range.end)];
                        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                        if key == "x" {
                            edit.replace_selection("");
                            self.commit_input(cx);
                        }
                    } else if key == "escape" {
                        self.manager
                            .update(cx, SessionManagerState::clear_input_focus);
                    } else if edit.edit_key(key, modifiers) {
                        if matches!(key, "backspace" | "delete") {
                            self.commit_input(cx);
                        }
                    } else {
                        return;
                    }
                } else if !self.manager.read(cx).show_group_manager {
                    if !self.row_key(key, modifiers, cx) {
                        return;
                    }
                } else {
                    return;
                }
            }
        }
        window.prevent_default();
        window.notify_text_input_changed(&self.focus);
        self.refresh(cx);
        cx.stop_propagation();
    }
    fn row_key(&mut self, key: &str, mods: gpui::Modifiers, cx: &mut Context<Self>) -> bool {
        let state = self.manager.read(cx);
        let visible = &state.visible_selection_targets;
        match key {
            "up" | "down" if !visible.is_empty() => {
                self.manager
                    .update(cx, |s, cx| s.step_selection(key == "down", mods.shift, cx));
            }
            "a" if mods.control || mods.platform => {
                self.manager.update(cx, |s, cx| {
                    s.selected_items = s.visible_selection_targets.iter().cloned().collect();
                    cx.notify();
                });
            }
            "delete" | "backspace" => self
                .manager
                .update(cx, SessionManagerState::request_selected_delete),
            "escape" => self
                .manager
                .update(cx, SessionManagerState::clear_session_manager_selection),
            "enter" if state.selected_items.len() == 1 => {
                let target = self
                    .data
                    .items
                    .iter()
                    .find(|row| {
                        row.selection_target()
                            .is_some_and(|t| state.selected_items.contains(&t))
                    })
                    .map(|row| row.target.clone());
                if let Some(target) = target {
                    self.action(
                        CatalogAction::Open {
                            target,
                            open_sftp: mods.control,
                        },
                        cx,
                    );
                }
            }
            _ => return false,
        }
        true
    }
}

impl EntityInputHandler for PortableCatalogView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let document: String = self.active_edit(cx)?.characters().collect();
        let start = byte_index_for_utf16(&document, range.start);
        let end = byte_index_for_utf16(&document, range.end).max(start);
        *adjusted = Some(
            utf16_offset_for_byte_index(&document, start)
                ..utf16_offset_for_byte_index(&document, end),
        );
        Some(document[start..end].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let draft = self.active_edit(cx)?;
        let mut range = draft.selection();
        if let Some((_, selected)) = &draft.marked {
            range = range.start + selected.start..range.start + selected.end;
        }
        Some(UTF16Selection {
            range,
            reversed: draft.caret < draft.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.active_edit(cx)?.marked_range()
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(draft) = self.active_edit_mut(cx) {
            draft.commit_marked();
            self.commit_input(cx);
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
        let Some(draft) = self.active_edit_mut(cx) else {
            return;
        };
        draft.prepare_replacement(range);
        if text.contains(['\r', '\n']) {
            draft.replace_selection(&Zeroizing::new(text.lines().collect::<Vec<_>>().join(" ")));
        } else {
            draft.replace_selection(text);
        }
        self.commit_input(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.active_edit_mut(cx) else {
            return;
        };
        draft.prepare_replacement(range);
        let end = text.encode_utf16().count();
        let selection = selected.unwrap_or(end..end);
        draft.marked = Some((
            Zeroizing::new(text.to_owned()),
            selection.start.min(end)..selection.end.min(end),
        ));
        self.refresh(cx);
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let draft = self.active_edit(cx)?;
        let byte = draft.visible_byte(range.start, true);
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
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        self.active_edit(cx)?.offset(point, true)
    }

    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = self.active_edit_mut(cx)
            && draft.marked.is_none()
        {
            draft.set_range(range);
            self.refresh(cx);
        }
    }

    fn text_length_utf16(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<usize> {
        Some(
            self.active_edit(cx)?
                .characters()
                .map(char::len_utf16)
                .sum(),
        )
    }

    fn accepts_text_input(&self, _: &mut Window, cx: &mut Context<Self>) -> bool {
        self.active_edit(cx).is_some()
    }
}
