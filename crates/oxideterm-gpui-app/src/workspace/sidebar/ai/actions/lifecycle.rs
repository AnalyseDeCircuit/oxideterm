impl WorkspaceApp {
    pub(super) fn clear_ai_sidebar_keyboard_focus(&mut self) {
        self.ai_chat_input_focused = false;
        self.ai_model_selector_search_focused = false;
        self.ai_model_selector_open = false;
        self.ime_marked_text = None;
    }

    fn close_ai_sidebar_popovers(&mut self) {
        self.ai_conversation_list_open = false;
        self.ai_chat_menu_open = false;
        self.ai_profile_selector_open = false;
        self.ai_safety_menu_open = false;
        self.ai_context_popover_open = false;
        self.ai_model_selector_open = false;
        self.ai_model_selector_search_focused = false;
        self.ai_model_selector_search_query.clear();
    }

    fn cancel_ai_chat_stream(&mut self, cx: &mut Context<Self>) {
        if let Some(task) = self.ai_chat_stream_task.take() {
            task.abort();
        }
        self.ai_chat_stream_rx = None;
        self.ai_chat_stream_generation = self.ai_chat_stream_generation.saturating_add(1);
        self.ai_chat_loading = false;
        for (_, sender) in self.ai_pending_tool_approvals.drain() {
            let _ = sender.send(false);
        }
        if let Some(conversation) = self.ai_chat.active_conversation_mut() {
            reject_incomplete_ai_tool_calls_on_cancel(conversation);
            for message in &mut conversation.messages {
                message.is_streaming = false;
            }
        }
        self.persist_ai_chat_state();
        cx.notify();
    }

    fn select_ai_conversation(&mut self, id: String) {
        if let Some(previous) = self
            .ai_chat
            .active_conversation_id
            .as_ref()
            .filter(|previous| *previous != &id)
            .cloned()
            && let Some(conversation) = self
                .ai_chat
                .conversations
                .iter_mut()
                .find(|conversation| conversation.id == previous)
        {
            conversation.messages.clear();
            conversation.messages_loaded = false;
        }
        if let Some(conversation) = self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == id)
            && !conversation.messages_loaded
            && let Ok(Some(loaded)) = self.ai_chat_store.load_conversation(&id)
            && let Some(slot) = self
                .ai_chat
                .conversations
                .iter_mut()
                .find(|conversation| conversation.id == id)
        {
            *slot = loaded;
        }
        self.ai_chat.set_active_conversation(id);
        self.ai_conversation_list_open = false;
        self.ai_chat_menu_open = false;
        self.ai_profile_selector_open = false;
        self.ai_safety_menu_open = false;
        self.ai_editing_message_id = None;
        self.ai_editing_message_draft.clear();
        self.ai_editing_message_focused = false;
        self.ai_thinking_expansion_state.clear();
        self.ai_tool_call_expansion_state.clear();
        self.ai_chat_input_focused = false;
    }

    fn delete_ai_conversation(&mut self, id: &str) {
        self.ai_chat.delete_conversation(id);
        self.ai_safety_bypass_conversations.remove(id);
        self.ai_thinking_expansion_state.clear();
        self.ai_tool_call_expansion_state.clear();
        self.ai_conversation_list_open = !self.ai_chat.conversations.is_empty();
        self.ai_chat_menu_open = false;
        self.persist_ai_chat_state();
    }

    fn clear_ai_conversations(&mut self) {
        self.ai_chat.clear_conversations();
        self.ai_safety_bypass_conversations.clear();
        self.ai_thinking_expansion_state.clear();
        self.ai_tool_call_expansion_state.clear();
        self.close_ai_sidebar_popovers();
        self.ai_clear_all_confirm_open = false;
        self.cancel_ai_chat_stream_without_notify();
        self.persist_ai_chat_state();
    }

    fn cancel_ai_chat_stream_without_notify(&mut self) {
        if let Some(task) = self.ai_chat_stream_task.take() {
            task.abort();
        }
        self.ai_chat_stream_rx = None;
        self.ai_chat_stream_generation = self.ai_chat_stream_generation.saturating_add(1);
        self.ai_chat_loading = false;
        for (_, sender) in self.ai_pending_tool_approvals.drain() {
            let _ = sender.send(false);
        }
        if let Some(conversation) = self.ai_chat.active_conversation_mut() {
            reject_incomplete_ai_tool_calls_on_cancel(conversation);
            for message in &mut conversation.messages {
                message.is_streaming = false;
            }
        }
    }

    fn persist_ai_chat_state(&self) {
        let store = self.ai_chat_store.clone();
        let state = self.ai_chat.clone();
        self.forwarding_runtime.spawn_blocking(move || {
            if let Err(error) = store.save_state(&state) {
                eprintln!("[AiChatStore] Failed to persist conversation: {error}");
            }
        });
    }

    fn ai_messages_count_label(&self, count: usize) -> String {
        self.i18n
            .t("ai.chat.messages_count")
            .replace("{{count}}", &count.to_string())
    }

    fn next_ai_chat_id(&mut self, now_ms: i64) -> String {
        self.next_ai_chat_sequence = self.next_ai_chat_sequence.saturating_add(1);
        format!("chat-{now_ms}-{}", self.next_ai_chat_sequence)
    }

    fn open_ai_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active_settings_tab = SettingsTab::Ai;
        self.open_settings(window, cx);
    }

}
