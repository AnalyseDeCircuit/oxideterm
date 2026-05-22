impl WorkspaceApp {
    pub(super) fn bootstrap_ai_mcp_registry(&self) {
        // Tauri boots the MCP registry from AiChatPanel mount, not from process
        // startup or every settings write. Keep native at the same user-visible
        // boundary so HTTP auth-token/keychain access only happens when the AI
        // surface is actually in use.
        let registry = self.ai_mcp_registry.clone();
        let configs = self.settings_store.settings().ai.mcp_servers.clone();
        self.forwarding_runtime.spawn(async move {
            registry.connect_all_values(&configs).await;
        });
    }

    pub(super) fn clear_ai_sidebar_keyboard_focus(&mut self) {
        self.ai_chat_input_focused = false;
        self.ai_chat_footer_focus = None;
        self.ai_model_selector_search_focused = false;
        self.ai_model_selector_open = false;
        self.ai_model_selector_focus_origin = None;
        self.ai_model_selector_highlighted_model = None;
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn close_ai_sidebar_popovers(&mut self) {
        self.ai_conversation_list_open = false;
        self.ai_chat_menu_open = false;
        self.ai_profile_selector_open = false;
        self.ai_safety_menu_open = false;
        self.ai_context_popover_open = false;
        self.ai_model_selector_open = false;
        self.ai_model_selector_focus_origin = None;
        self.ai_model_selector_search_focused = false;
        self.ai_model_selector_search_query.clear();
        self.ai_model_selector_highlighted_model = None;
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
        let conversation_id = self.ai_chat.active_conversation_id.clone();
        let stopped_turns = self
            .ai_chat
            .active_conversation_mut()
            .map(finalize_streaming_ai_messages_on_cancel)
            .unwrap_or_default();
        if let Some(conversation_id) = conversation_id.as_deref() {
            self.persist_ai_stopped_assistant_turns(conversation_id, &stopped_turns);
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
            && let Some(store) = self.ai_chat_store.as_ref()
            && let Ok(Some(loaded)) = store.load_conversation(&id)
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
        self.ai_chat_footer_focus = None;
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

    pub(super) fn clear_ai_conversations(&mut self) {
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
        let conversation_id = self.ai_chat.active_conversation_id.clone();
        let stopped_turns = self
            .ai_chat
            .active_conversation_mut()
            .map(finalize_streaming_ai_messages_on_cancel)
            .unwrap_or_default();
        if let Some(conversation_id) = conversation_id.as_deref() {
            self.persist_ai_stopped_assistant_turns(conversation_id, &stopped_turns);
        }
    }

    fn persist_ai_chat_state(&self) {
        let Some(store) = self.ai_chat_store.clone() else {
            return;
        };
        let state = self.ai_chat.clone();
        let projection_updated_at =
            oxideterm_ai::AiChatPersistenceStore::next_projection_persist_at();
        self.forwarding_runtime.spawn_blocking(move || {
            if let Err(error) =
                store.save_state_with_projection_updated_at(&state, projection_updated_at)
            {
                eprintln!("[AiChatStore] Failed to persist conversation: {error}");
            }
        });
    }

    fn persist_ai_stopped_assistant_turns(
        &self,
        conversation_id: &str,
        stopped_turns: &[AiStoppedAssistantTurn],
    ) {
        for stopped in stopped_turns {
            if stopped.retained {
                self.persist_ai_assistant_turn_end(
                    conversation_id,
                    &stopped.message_id,
                    stopped.status,
                );
            } else {
                self.persist_ai_removed_assistant_turn_end(
                    conversation_id,
                    &stopped.message_id,
                    stopped.status,
                );
            }
        }
    }

    fn retry_ai_chat_initialization(&mut self, cx: &mut Context<Self>) {
        match oxideterm_ai::AiChatPersistenceStore::load(default_ai_conversations_path()) {
            Ok((store, state)) => {
                self.ai_chat_store = Some(store);
                self.ai_chat = state;
                self.ai_chat_initialization_error = None;
                self.ai_chat_list_state =
                    ListState::new(0, ListAlignment::Top, px(AI_CHAT_LIST_OVERDRAW_PX));
                self.ai_chat_list_cache
                    .replace(VirtualListSignatureCache::default());
            }
            Err(error) => {
                eprintln!("failed to retry AI chat store load: {error}");
                self.ai_chat = oxideterm_ai::AiChatState::default();
                self.ai_chat_store = None;
                self.ai_chat_initialization_error = Some(ai_chat_initialization_error(&error));
            }
        }
        cx.notify();
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
