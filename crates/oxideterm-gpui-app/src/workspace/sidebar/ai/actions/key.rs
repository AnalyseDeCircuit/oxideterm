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
                    self.ai_model_selector_open = false;
                    self.ai_model_selector_search_focused = false;
                    self.ai_model_selector_search_query.clear();
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                "backspace" => {
                    self.ai_model_selector_search_query.pop();
                    self.ime_marked_text = None;
                    cx.notify();
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
                    self.ai_editing_message_draft.pop();
                    self.ime_marked_text = None;
                    cx.notify();
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
                _ => true,
            }
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
            match event.keystroke.key.as_str() {
                "escape" => {
                    self.ai_chat_input_focused = false;
                    self.ime_marked_text = None;
                    cx.notify();
                    true
                }
                "backspace" => {
                    self.ai_chat_draft.pop();
                    self.ai_chat_autocomplete_suppressed = false;
                    self.ai_chat_autocomplete_index = 0;
                    self.ime_marked_text = None;
                    cx.notify();
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


}
