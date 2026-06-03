impl WorkspaceApp {
    pub(super) fn handle_ai_sidebar_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.ai_model_selector_open && self.ai_model_selector_search_focused {
            if event.keystroke.modifiers.platform {
                return false;
            }
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.close_ai_model_selector();
                    cx.notify();
                    true
                }
                "tab" => {
                    // Browser focus leaves the model selector on Tab. Native
                    // does not yet expose all footer/button targets, so close
                    // the Radix-style dropdown rather than trapping focus.
                    self.close_ai_model_selector();
                    cx.notify();
                    true
                }
                "down" | "arrowdown" => {
                    self.move_ai_model_selector_highlight(1);
                    cx.notify();
                    true
                }
                "up" | "arrowup" => {
                    self.move_ai_model_selector_highlight(-1);
                    cx.notify();
                    true
                }
                "home" => {
                    self.set_ai_model_selector_highlight_edge(false);
                    cx.notify();
                    true
                }
                "end" => {
                    self.set_ai_model_selector_highlight_edge(true);
                    cx.notify();
                    true
                }
                "enter" => {
                    self.select_highlighted_ai_model(cx);
                    true
                }
                "backspace" => {
                    let changed = self.ai_model_selector_search_query.pop().is_some()
                        || self.ai_model_selector_highlighted_model.take().is_some()
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
        } else if self.ai_editing_message_id.is_some() && self.ai_editing_message_focused {
            if event.keystroke.modifiers.platform {
                return false;
            }
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.cancel_edit_ai_message(cx);
                    true
                }
                "backspace" => {
                    let changed = self.ai_editing_message_draft.pop().is_some()
                        || self.ime_marked_text.take().is_some();
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
                    self.ai_editing_message_draft.push('\n');
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                "tab" => {
                    // Textareas in the Tauri sidebar release focus on Tab
                    // unless an autocomplete/menu owner consumes it first.
                    self.ai_editing_message_focused = false;
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                _ => true,
            }
        } else if let Some(action) = self.ai_chat_footer_focus {
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
        } else if self.ai_chat_input_focused {
            if event.keystroke.modifiers.platform {
                return false;
            }
            let autocomplete_len = self.ai_chat_autocomplete_items().len();
            if autocomplete_len > 0 {
                match event.keystroke.key.as_str() {
                    "down" | "arrowdown" => {
                        self.ai_chat_autocomplete_index =
                            (self.ai_chat_autocomplete_index + 1) % autocomplete_len;
                        cx.notify();
                        return true;
                    }
                    "up" | "arrowup" => {
                        self.ai_chat_autocomplete_index =
                            (self.ai_chat_autocomplete_index + autocomplete_len - 1)
                                % autocomplete_len;
                        cx.notify();
                        return true;
                    }
                    "tab" | "enter" if !event.keystroke.modifiers.shift => {
                        let index = self.ai_chat_autocomplete_index.min(autocomplete_len - 1);
                        if let Some(candidate) = self.ai_chat_autocomplete_items().get(index).cloned()
                        {
                            self.apply_ai_chat_autocomplete_candidate(&candidate, cx);
                        }
                        return true;
                    }
                    "escape" => {
                        self.ai_chat_autocomplete_suppressed = true;
                        self.ime_marked_text = None;
                        cx.notify();
                        return true;
                    }
                    _ => {}
                }
            }
            let footer_actions = if self.ai_chat_footer_action_enabled() {
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
                    let changed = self.ai_chat_draft.pop().is_some()
                        || self.ai_chat_autocomplete_suppressed
                        || self.ai_chat_autocomplete_index != 0
                        || self.ime_marked_text.take().is_some();
                    self.ai_chat_autocomplete_suppressed = false;
                    self.ai_chat_autocomplete_index = 0;
                    if changed {
                        cx.notify();
                    }
                    true
                }
                "enter" if !event.keystroke.modifiers.shift && !self.ai_chat_loading => {
                    self.send_ai_chat_draft(cx);
                    true
                }
                "enter" => {
                    self.ai_chat_draft.push('\n');
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                _ => true,
            }
        } else {
            false
        }
    }

    fn ai_chat_footer_action_enabled(&self) -> bool {
        self.ai_chat_loading || !self.ai_chat_draft.trim().is_empty()
    }

    fn activate_ai_chat_footer_action(&mut self, action: AiChatFooterAction, cx: &mut Context<Self>) {
        match action {
            AiChatFooterAction::Submit if self.ai_chat_loading => self.cancel_ai_chat_stream(cx),
            AiChatFooterAction::Submit if !self.ai_chat_draft.trim().is_empty() => {
                self.send_ai_chat_draft(cx)
            }
            AiChatFooterAction::Submit => {
                self.ai_chat_footer_focus = None;
                cx.notify();
            }
        }
    }

    fn apply_ai_chat_inline_footer_key_action(
        &mut self,
        action: browser_behavior::InlineFooterInputKeyAction<AiChatFooterAction>,
        cx: &mut Context<Self>,
    ) {
        // The AI composer is an inline browser control rather than a modal
        // dialog. Keep its focus exit/return behavior in the shared browser
        // helper while this method performs the Workspace-specific state writes.
        match action {
            browser_behavior::InlineFooterInputKeyAction::ClearFocus => {
                self.ai_chat_input_focused = false;
                self.ai_chat_footer_focus = None;
                self.ime_marked_text = None;
                cx.notify();
            }
            browser_behavior::InlineFooterInputKeyAction::FocusInput => {
                self.ai_chat_input_focused = true;
                self.ai_chat_footer_focus = None;
                self.ime_marked_text = None;
                cx.notify();
            }
            browser_behavior::InlineFooterInputKeyAction::FocusFooter(action) => {
                self.ai_chat_input_focused = false;
                self.ai_chat_footer_focus = Some(action);
                self.ime_marked_text = None;
                cx.notify();
            }
            browser_behavior::InlineFooterInputKeyAction::Activate(action) => {
                self.activate_ai_chat_footer_action(action, cx);
            }
        }
    }


}
