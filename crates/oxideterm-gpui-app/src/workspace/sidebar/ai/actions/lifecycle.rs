impl WorkspaceApp {
    pub(in crate::workspace) fn ensure_ai_chat_initialized(&mut self) {
        if self.ai.chat.initialized {
            return;
        }
        self.ai.chat.initialized = true;
        match oxideterm_ai::AiChatPersistenceStore::load(default_ai_conversations_path()) {
            Ok((store, state)) => {
                self.ai.chat.persistence_store = Some(store);
                self.ai.chat.conversation_state = state;
                self.ai.chat.initialization_error = None;
                self.ai.chat.message_list_state =
                    tauri_virtual_list_state(0, ListAlignment::Top, ai_chat_virtual_list_spec());
                self.ai
                    .chat
                    .message_list_cache
                    .replace(VirtualListSignatureCache::default());
            }
            Err(error) => {
                eprintln!("failed to load AI chat store: {error}");
                self.ai.chat.conversation_state = oxideterm_ai::AiChatState::default();
                self.ai.chat.persistence_store = None;
                self.ai.chat.initialization_error = Some(ai_chat_initialization_error(&error));
            }
        }
    }

    pub(in crate::workspace) fn bootstrap_ai_mcp_registry(&self) {
        // Tauri boots the MCP registry from AiChatPanel mount, not from process
        // startup or every settings write. Keep native at the same user-visible
        // boundary so HTTP auth-token/keychain access only happens when the AI
        // surface is actually in use.
        let registry = self.ai.runtime.mcp_registry.clone();
        let configs = self.settings_store.settings().ai.mcp_servers.clone();
        self.forwarding_runtime.spawn(async move {
            registry.connect_all_values(&configs).await;
        });
    }

    pub(in crate::workspace) fn clear_ai_sidebar_keyboard_focus(&mut self) {
        self.ai.chat.input_focused = false;
        self.ai.chat.footer_focus = None;
        self.close_ai_model_selector();
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn close_ai_sidebar_popovers(&mut self) {
        self.ai.chat.conversation_list_open = false;
        self.ai.chat.menu_open = false;
        self.ai.chat.reasoning_menu_open = false;
        self.ai.chat.safety_menu_open = false;
        self.ai.chat.context_popover_open = false;
        self.close_ai_model_selector();
    }

    pub(in crate::workspace) fn close_ai_model_selector(&mut self) {
        // The compact model selector behaves like a browser/Radix Select with a
        // searchable input owner. Closing it must clear popup state, keyboard
        // focus origin, highlighted option, and any marked text together so Esc,
        // outside click, Tab, footer navigation, and row activation do not drift.
        let restore_terminal_inline_prompt = self.ai.models.selector_scope
            == Some(AiModelSelectorScope::TerminalInline)
            && self.ai.chat.inline_panel.open;
        self.ai.models.selector_open = false;
        self.ai.models.selector_scope = None;
        self.ai.models.selector_focus_origin = None;
        self.ai.models.selector_search_focused = false;
        self.ai.models.selector_search_query.clear();
        self.ai.models.selector_highlighted_model = None;
        self.ime_marked_text = None;
        if restore_terminal_inline_prompt {
            // Tauri's inline command bar returns focus to its prompt after a
            // nested model picker closes; otherwise the next typed key appears
            // to vanish into the terminal surface.
            self.ai.chat.inline_panel.prompt_focused = true;
        }
    }

    pub(in crate::workspace) fn cancel_ai_chat_stream(&mut self, cx: &mut Context<Self>) {
        if let Some(conversation_id) = self
            .ai
            .chat
            .conversation_state
            .active_conversation_id
            .as_deref()
        {
            let generation_id = self.ai.chat.stream_generation.to_string();
            // ACP Stop must target the live generation before local task abort
            // drops the registered session handle.
            let _ = self
                .ai
                .runtime
                .acp_runtime_registry
                .cancel_generation(conversation_id, &generation_id);
        }
        if let Some(task) = self.ai.chat.stream_task.take() {
            task.abort();
        }
        self.ai.chat.stream_rx = None;
        self.ai.chat.stream_generation = self.ai.chat.stream_generation.saturating_add(1);
        self.ai.chat.loading = false;
        for (_, sender) in self.ai.runtime.pending_tool_approvals.drain() {
            let _ = sender.send(false);
        }
        let conversation_id = self
            .ai
            .chat
            .conversation_state
            .active_conversation_id
            .clone();
        let stopped_turns = self
            .ai
            .chat
            .conversation_state
            .active_conversation_mut()
            .map(finalize_streaming_ai_messages_on_cancel)
            .unwrap_or_default();
        if let Some(conversation_id) = conversation_id.as_deref() {
            self.persist_ai_stopped_assistant_turns(conversation_id, &stopped_turns);
        }
        self.persist_ai_chat_state();
        cx.notify();
    }

    pub(in crate::workspace) fn select_ai_conversation(&mut self, id: String) {
        if let Some(previous) = self
            .ai
            .chat
            .conversation_state
            .active_conversation_id
            .as_ref()
            .filter(|previous| *previous != &id)
            .cloned()
            && let Some(conversation) = self
                .ai
                .chat
                .conversation_state
                .conversations
                .iter_mut()
                .find(|conversation| conversation.id == previous)
        {
            conversation.messages.clear();
            conversation.messages_loaded = false;
        }
        if let Some(conversation) = self
            .ai
            .chat
            .conversation_state
            .conversations
            .iter()
            .find(|conversation| conversation.id == id)
            && !conversation.messages_loaded
            && let Some(store) = self.ai.chat.persistence_store.as_ref()
            && let Ok(Some(loaded)) = store.load_conversation(&id)
            && let Some(slot) = self
                .ai
                .chat
                .conversation_state
                .conversations
                .iter_mut()
                .find(|conversation| conversation.id == id)
        {
            *slot = loaded;
        }
        self.ai.chat.conversation_state.set_active_conversation(id);
        self.ai.chat.conversation_list_open = false;
        self.ai.chat.menu_open = false;
        self.ai.chat.safety_menu_open = false;
        self.ai.chat.editing_message_id = None;
        self.ai.chat.editing_message_draft.clear();
        self.ai.chat.editing_message_focused = false;
        self.ai.chat.thinking_expansion_state.clear();
        self.ai.chat.tool_call_expansion_state.clear();
        self.ai.chat.input_focused = false;
        self.ai.chat.footer_focus = None;
    }

    pub(in crate::workspace) fn delete_ai_conversation(&mut self, id: &str) {
        self.ai.chat.conversation_state.delete_conversation(id);
        self.ai.chat.safety_bypass_conversations.remove(id);
        self.ai.chat.thinking_expansion_state.clear();
        self.ai.chat.tool_call_expansion_state.clear();
        self.ai.chat.conversation_list_open =
            !self.ai.chat.conversation_state.conversations.is_empty();
        self.ai.chat.menu_open = false;
        self.persist_ai_chat_state();
    }

    pub(in crate::workspace) fn clear_ai_conversations(&mut self) {
        self.ai.chat.conversation_state.clear_conversations();
        self.ai.chat.safety_bypass_conversations.clear();
        self.ai.chat.thinking_expansion_state.clear();
        self.ai.chat.tool_call_expansion_state.clear();
        self.close_ai_sidebar_popovers();
        self.ai.chat.clear_all_confirm_open = false;
        self.cancel_ai_chat_stream_without_notify();
        self.persist_ai_chat_state();
    }

    pub(in crate::workspace) fn cancel_ai_chat_stream_without_notify(&mut self) {
        if let Some(conversation_id) = self
            .ai
            .chat
            .conversation_state
            .active_conversation_id
            .as_deref()
        {
            let generation_id = self.ai.chat.stream_generation.to_string();
            // Keep silent cancellation aligned with the visible Stop path.
            let _ = self
                .ai
                .runtime
                .acp_runtime_registry
                .cancel_generation(conversation_id, &generation_id);
        }
        if let Some(task) = self.ai.chat.stream_task.take() {
            task.abort();
        }
        self.ai.chat.stream_rx = None;
        self.ai.chat.stream_generation = self.ai.chat.stream_generation.saturating_add(1);
        self.ai.chat.loading = false;
        for (_, sender) in self.ai.runtime.pending_tool_approvals.drain() {
            let _ = sender.send(false);
        }
        let conversation_id = self
            .ai
            .chat
            .conversation_state
            .active_conversation_id
            .clone();
        let stopped_turns = self
            .ai
            .chat
            .conversation_state
            .active_conversation_mut()
            .map(finalize_streaming_ai_messages_on_cancel)
            .unwrap_or_default();
        if let Some(conversation_id) = conversation_id.as_deref() {
            self.persist_ai_stopped_assistant_turns(conversation_id, &stopped_turns);
        }
    }

    pub(in crate::workspace) fn persist_ai_chat_state(&self) {
        let Some(store) = self.ai.chat.persistence_store.clone() else {
            return;
        };
        let state = self.ai.chat.conversation_state.clone();
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

    pub(in crate::workspace) fn persist_ai_stopped_assistant_turns(
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

    pub(in crate::workspace) fn retry_ai_chat_initialization(&mut self, cx: &mut Context<Self>) {
        self.ai.chat.initialized = true;
        match oxideterm_ai::AiChatPersistenceStore::load(default_ai_conversations_path()) {
            Ok((store, state)) => {
                self.ai.chat.persistence_store = Some(store);
                self.ai.chat.conversation_state = state;
                self.ai.chat.initialization_error = None;
                self.ai.chat.message_list_state =
                    tauri_virtual_list_state(0, ListAlignment::Top, ai_chat_virtual_list_spec());
                self.ai
                    .chat
                    .message_list_cache
                    .replace(VirtualListSignatureCache::default());
            }
            Err(error) => {
                eprintln!("failed to retry AI chat store load: {error}");
                self.ai.chat.conversation_state = oxideterm_ai::AiChatState::default();
                self.ai.chat.persistence_store = None;
                self.ai.chat.initialization_error = Some(ai_chat_initialization_error(&error));
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn ai_messages_count_label(&self, count: usize) -> String {
        self.i18n
            .t("ai.chat.messages_count")
            .replace("{{count}}", &count.to_string())
    }

    pub(in crate::workspace) fn next_ai_chat_id(&mut self, now_ms: i64) -> String {
        self.ai.chat.next_sequence = self.ai.chat.next_sequence.saturating_add(1);
        format!("chat-{now_ms}-{}", self.ai.chat.next_sequence)
    }

    pub(in crate::workspace) fn open_ai_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_page.set_active_tab(SettingsTab::Ai);
        self.open_settings(window, cx);
    }
}
