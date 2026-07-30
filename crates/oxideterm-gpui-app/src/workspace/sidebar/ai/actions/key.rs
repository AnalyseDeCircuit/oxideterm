impl WorkspaceApp {
    pub(in crate::workspace) fn handle_ai_sidebar_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(selection) = self
            .ai_entity
            .read(cx)
            .chat_ui()
            .tool_candidate_selection
            .clone()
        {
            if event.keystroke.modifiers.platform {
                return false;
            }
            match event.keystroke.key.as_str() {
                "down" | "arrowdown" => {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.move_tool_candidate_selection(1);
                    });
                    cx.notify();
                    true
                }
                "up" | "arrowup" => {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.move_tool_candidate_selection(-1);
                    });
                    cx.notify();
                    true
                }
                "home" => {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.move_tool_candidate_selection(-(selection.candidate_count as isize));
                    });
                    cx.notify();
                    true
                }
                "end" => {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.move_tool_candidate_selection(selection.candidate_count as isize);
                    });
                    cx.notify();
                    true
                }
                "enter" => {
                    self.resolve_ai_tool_candidate_selection(
                        selection.tool_call_id,
                        Some(selection.selected_index),
                        cx,
                    );
                    true
                }
                "escape" => {
                    self.resolve_ai_tool_candidate_selection(
                        selection.tool_call_id,
                        None,
                        cx,
                    );
                    true
                }
                _ => true,
            }
        } else if self.ai_entity.read(cx).model_selector_open()
            && self.ai_entity.read(cx).model_selector_search_focused()
        {
            if event.keystroke.modifiers.platform {
                return false;
            }
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.close_ai_model_selector(cx);
                    cx.notify();
                    true
                }
                "tab" => {
                    // Browser focus leaves the model selector on Tab. Native
                    // does not yet expose all footer/button targets, so close
                    // the Radix-style dropdown rather than trapping focus.
                    self.close_ai_model_selector(cx);
                    cx.notify();
                    true
                }
                "down" | "arrowdown" => {
                    self.move_ai_model_selector_highlight(1, cx);
                    cx.notify();
                    true
                }
                "up" | "arrowup" => {
                    self.move_ai_model_selector_highlight(-1, cx);
                    cx.notify();
                    true
                }
                "home" => {
                    self.set_ai_model_selector_highlight_edge(false, cx);
                    cx.notify();
                    true
                }
                "end" => {
                    self.set_ai_model_selector_highlight_edge(true, cx);
                    cx.notify();
                    true
                }
                "enter" => {
                    self.select_highlighted_ai_model(cx);
                    true
                }
                "backspace" => {
                    let changed = self
                        .ai_entity
                        .update(cx, |ai, _cx| ai.pop_model_selector_search())
                        || self.ime_marked_text.take().is_some();
                    if changed {
                        // Empty Backspace should not repaint the selector when
                        // query, highlight, and IME state are already unchanged.
                        cx.notify();
                    }
                    true
                }
                _ => true,
            }
        } else if self
            .ai_entity
            .read(cx)
            .chat_ui()
            .renaming_conversation_id
            .is_some()
            && self
                .ai_entity
                .read(cx)
                .chat_ui()
                .renaming_conversation_focused
        {
            if event.keystroke.modifiers.platform {
                return false;
            }
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.cancel_ai_conversation_rename(cx);
                    true
                }
                "backspace" => {
                    let changed = self
                        .ai_entity
                        .update(cx, |ai, _cx| ai.pop_conversation_rename())
                        || self.ime_marked_text.take().is_some();
                    if changed {
                        cx.notify();
                    }
                    true
                }
                "enter" | "tab" => {
                    self.save_ai_conversation_rename(cx);
                    true
                }
                "space" | " "
                    if ai_text_input_space_inserts_literal(
                        event.keystroke.modifiers.platform,
                        event.keystroke.modifiers.control,
                        event.keystroke.modifiers.alt,
                    ) =>
                {
                    self.insert_ai_text_input_literal_space(
                        WorkspaceImeTarget::AiConversationRename,
                        cx,
                    );
                    true
                }
                _ => true,
            }
        } else if self.ai_entity.read(cx).chat_ui().editing_message_id.is_some() && self.ai_entity.read(cx).chat_ui().editing_message_focused
        {
            if event.keystroke.modifiers.platform {
                return false;
            }
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.cancel_edit_ai_message(cx);
                    true
                }
                "backspace" => {
                    let changed = self.ai_entity.update(cx, |ai, _cx| {
                        ai.pop_message_edit()
                    }) || self.ime_marked_text.take().is_some();
                    if changed {
                        cx.notify();
                    }
                    true
                }
                "enter" if !event.keystroke.modifiers.shift => {
                    self.save_ai_message_edit(cx);
                    true
                }
                "enter" => {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.push_message_edit_newline();
                    });
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                "tab" => {
                    // Textareas in the Tauri sidebar release focus on Tab
                    // unless an autocomplete/menu owner consumes it first.
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.blur_message_edit();
                    });
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                "space" | " "
                    if ai_text_input_space_inserts_literal(
                        event.keystroke.modifiers.platform,
                        event.keystroke.modifiers.control,
                        event.keystroke.modifiers.alt,
                    ) =>
                {
                    self.insert_ai_text_input_literal_space(WorkspaceImeTarget::AiMessageEdit, cx);
                    true
                }
                _ => true,
            }
        } else if let Some(action) = self.ai_entity.read(cx).chat_ui().footer_focus {
            if event.keystroke.modifiers.platform {
                return false;
            }
            if let Some(action) = browser_behavior::inline_footer_input_key_action(
                event.keystroke.key.as_str(),
                event.keystroke.modifiers.shift,
                &AI_CHAT_FOOTER_ACTIONS,
                false,
                Some(action),
                AiChatFooterAction::Submit,
            ) {
                self.apply_ai_chat_inline_footer_key_action(action, cx);
            }
            true
        } else if self.ai_entity.read(cx).chat_ui().input_focused {
            if event.keystroke.modifiers.platform {
                return false;
            }
            let autocomplete_len = self.ai_chat_autocomplete_items(cx).len();
            if autocomplete_len > 0 {
                match event.keystroke.key.as_str() {
                    "down" | "arrowdown" => {
                        self.ai_entity.update(cx, |ai, _cx| {
                            ai.move_chat_autocomplete(1, autocomplete_len);
                        });
                        cx.notify();
                        return true;
                    }
                    "up" | "arrowup" => {
                        self.ai_entity.update(cx, |ai, _cx| {
                            ai.move_chat_autocomplete(-1, autocomplete_len);
                        });
                        cx.notify();
                        return true;
                    }
                    "tab" | "enter" if !event.keystroke.modifiers.shift => {
                        let index = self.ai_entity.read(cx).chat_ui().autocomplete_index.min(autocomplete_len - 1);
                        if let Some(candidate) =
                            self.ai_chat_autocomplete_items(cx).get(index).cloned()
                        {
                            self.apply_ai_chat_autocomplete_candidate(&candidate, cx);
                        }
                        return true;
                    }
                    "escape" => {
                        self.ai_entity.update(cx, |ai, _cx| {
                            ai.suppress_chat_autocomplete();
                        });
                        self.ime_marked_text = None;
                        cx.notify();
                        return true;
                    }
                    _ => {}
                }
            }
            let footer_actions = if self.ai_chat_footer_action_enabled(cx) {
                &AI_CHAT_FOOTER_ACTIONS[..]
            } else {
                &[]
            };
            if let Some(action) = browser_behavior::inline_footer_input_key_action(
                event.keystroke.key.as_str(),
                event.keystroke.modifiers.shift,
                footer_actions,
                true,
                None,
                AiChatFooterAction::Submit,
            ) {
                self.apply_ai_chat_inline_footer_key_action(action, cx);
                return true;
            }
            match event.keystroke.key.as_str() {
                "backspace" => {
                    let changed = self.ai_entity.update(cx, |ai, _cx| {
                        ai.pop_chat_draft()
                    })
                        || self.ime_marked_text.take().is_some();
                    if changed {
                        cx.notify();
                    }
                    true
                }
                "enter" if !event.keystroke.modifiers.shift && !self.ai_entity.read(cx).chat_is_loading() => {
                    self.send_ai_chat_draft(cx);
                    true
                }
                "enter" => {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.push_chat_draft_newline();
                    });
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                "space" | " "
                    if ai_text_input_space_inserts_literal(
                        event.keystroke.modifiers.platform,
                        event.keystroke.modifiers.control,
                        event.keystroke.modifiers.alt,
                    ) =>
                {
                    self.insert_ai_text_input_literal_space(WorkspaceImeTarget::AiChatInput, cx);
                    true
                }
                _ => true,
            }
        } else {
            false
        }
    }

    pub(in crate::workspace) fn ai_chat_footer_action_enabled(&self, cx: &App) -> bool {
        self.ai_entity.read(cx).chat_is_loading() || !self.ai_entity.read(cx).chat_ui().draft.trim().is_empty()
    }

    pub(in crate::workspace) fn activate_ai_chat_footer_action(
        &mut self,
        action: AiChatFooterAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            AiChatFooterAction::Submit if self.ai_entity.read(cx).chat_is_loading() => self.cancel_ai_chat_stream(cx),
            AiChatFooterAction::Submit if !self.ai_entity.read(cx).chat_ui().draft.trim().is_empty() => {
                self.send_ai_chat_draft(cx)
            }
            AiChatFooterAction::Submit => {
                self.ai_entity.update(cx, |ai, _cx| {
                    ai.clear_chat_footer_focus();
                });
                cx.notify();
            }
        }
    }

    pub(in crate::workspace) fn apply_ai_chat_inline_footer_key_action(
        &mut self,
        action: browser_behavior::InlineFooterInputKeyAction<AiChatFooterAction>,
        cx: &mut Context<Self>,
    ) {
        // The AI composer is an inline browser control rather than a modal
        // dialog. Keep its focus exit/return behavior in the shared browser
        // helper while this method performs the Workspace-specific state writes.
        match action {
            browser_behavior::InlineFooterInputKeyAction::ClearFocus => {
                self.ai_entity.update(cx, |ai, _cx| {
                    ai.blur_chat_input(false);
                });
                self.ime_marked_text = None;
                cx.notify();
            }
            browser_behavior::InlineFooterInputKeyAction::FocusInput => {
                self.ai_entity.update(cx, |ai, _cx| {
                    ai.focus_chat_input();
                });
                self.ime_marked_text = None;
                cx.notify();
            }
            browser_behavior::InlineFooterInputKeyAction::FocusFooter(action) => {
                self.ai_entity.update(cx, |ai, _cx| {
                    ai.set_chat_footer_focus(Some(action));
                });
                self.ime_marked_text = None;
                cx.notify();
            }
            browser_behavior::InlineFooterInputKeyAction::Activate(action) => {
                self.activate_ai_chat_footer_action(action, cx);
            }
        }
    }

    pub(in crate::workspace) fn insert_ai_text_input_literal_space(
        &mut self,
        target: WorkspaceImeTarget,
        cx: &mut Context<Self>,
    ) {
        // Some GPUI platforms deliver Space without key_char, so write it
        // through the IME owner just like a browser textarea would.
        let replacement_range = self.ime_selection_range_for_target(target, cx);
        let caret = replacement_range
            .as_ref()
            .map(|range| range.start + " ".encode_utf16().count());
        self.clear_ime_selection();
        self.replace_ime_target_text(target, replacement_range, " ", cx);
        if let Some(caret) = caret {
            self.set_ime_selection_from_anchor(target, caret, caret);
        }
    }
}

pub(in crate::workspace) fn ai_text_input_space_inserts_literal(
    platform: bool,
    control: bool,
    alt: bool,
) -> bool {
    !platform && !control && !alt
}

#[cfg(test)]
mod key_tests {
    use super::ai_text_input_space_inserts_literal;

    #[test]
    pub(in crate::workspace) fn ai_text_input_plain_space_inserts_literal() {
        assert!(ai_text_input_space_inserts_literal(false, false, false));
    }

    #[test]
    pub(in crate::workspace) fn ai_text_input_modified_space_falls_through() {
        assert!(!ai_text_input_space_inserts_literal(true, false, false));
        assert!(!ai_text_input_space_inserts_literal(false, true, false));
        assert!(!ai_text_input_space_inserts_literal(false, false, true));
    }
}
