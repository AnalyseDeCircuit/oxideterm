impl WorkspaceApp {
    pub(in crate::workspace) fn ensure_ai_chat_initialized(&mut self, cx: &mut App) {
        let outcome = self.ai_entity.update(cx, |ai, _cx| {
            ai.ensure_chat_initialized(default_ai_conversations_path())
        });
        if matches!(outcome, AiChatInitializationOutcome::Loaded) {
            self.reset_ai_message_list();
        }
    }

    fn reset_ai_message_list(&mut self) {
        self.ai.chat.message_list_state =
            tauri_virtual_list_state(0, ListAlignment::Top, ai_chat_virtual_list_spec());
        self.ai
            .chat
            .message_list_cache
            .replace(VirtualListSignatureCache::default());
    }

    pub(in crate::workspace) fn bootstrap_ai_mcp_registry(&self, cx: &App) {
        // Tauri boots the MCP registry from AiChatPanel mount, not from process
        // startup or every settings write. Keep native at the same user-visible
        // boundary so HTTP auth-token/keychain access only happens when the AI
        // surface is actually in use.
        let registry = self.ai_entity.read(cx).mcp_registry().clone();
        let configs = self.settings_store.settings().ai.mcp_servers.clone();
        self.forwarding_runtime.spawn(async move {
            registry.connect_all_values(&configs).await;
        });
    }

    pub(in crate::workspace) fn clear_ai_sidebar_keyboard_focus(&mut self, cx: &mut App) {
        self.ai.chat.input_focused = false;
        self.ai.chat.footer_focus = None;
        self.close_ai_model_selector(cx);
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn close_ai_sidebar_popovers(&mut self, cx: &mut App) {
        self.ai.chat.conversation_list_open = false;
        self.ai.chat.menu_open = false;
        self.ai.chat.reasoning_menu_open = false;
        self.ai.chat.safety_menu_open = false;
        self.ai.chat.context_popover_open = false;
        self.close_ai_model_selector(cx);
    }

    pub(in crate::workspace) fn close_ai_model_selector(&mut self, cx: &mut App) {
        // The compact model selector behaves like a browser/Radix Select with a
        // searchable input owner. Closing it must clear popup state, keyboard
        // focus origin, highlighted option, and any marked text together so Esc,
        // outside click, Tab, footer navigation, and row activation do not drift.
        let restore_terminal_inline_prompt = self.ai.models.selector_scope
            == Some(AiModelSelectorScope::TerminalInline)
            && self.ai_entity.read(cx).terminal_inline_panel().open;
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
            self.ai_entity.update(cx, |ai, _cx| {
                ai.terminal_inline_panel_mut().prompt_focused = true;
            });
        }
    }

    pub(in crate::workspace) fn cancel_ai_chat_stream(&mut self, cx: &mut Context<Self>) {
        self.cancel_ai_chat_stream_without_notify(cx);
        self.ai_entity.read(cx).persist_chat_state();
        cx.notify();
    }

    pub(in crate::workspace) fn select_ai_conversation(&mut self, id: String, cx: &mut App) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.select_conversation(id);
        });
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

    pub(in crate::workspace) fn delete_ai_conversation(&mut self, id: &str, cx: &mut App) {
        let has_conversations = self.ai_entity.update(cx, |ai, _cx| {
            let has_conversations = ai.delete_conversation(id);
            ai.persist_chat_state();
            has_conversations
        });
        self.ai.chat.thinking_expansion_state.clear();
        self.ai.chat.tool_call_expansion_state.clear();
        self.ai.chat.conversation_list_open = has_conversations;
        self.ai.chat.menu_open = false;
    }

    pub(in crate::workspace) fn clear_ai_conversations(&mut self, cx: &mut App) {
        // Cancel the live generation before clearing its routing identifier.
        self.cancel_ai_chat_stream_without_notify(cx);
        self.ai_entity.update(cx, |ai, _cx| {
            ai.clear_conversations();
            ai.persist_chat_state();
        });
        self.ai.chat.thinking_expansion_state.clear();
        self.ai.chat.tool_call_expansion_state.clear();
        self.close_ai_sidebar_popovers(cx);
    }

    pub(in crate::workspace) fn cancel_ai_chat_stream_without_notify(&mut self, cx: &mut App) {
        let active_conversation_id = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone();
        if let Some(conversation_id) = active_conversation_id.as_deref() {
            let generation_id = self.ai_entity.read(cx).chat_stream_generation().to_string();
            // ACP Stop must target the live generation before local task abort
            // drops the registered session handle.
            let _ = self
                .ai_entity
                .read(cx)
                .acp_runtime_registry()
                .cancel_generation(conversation_id, &generation_id);
        }
        let (conversation_id, stopped_turns) = self.ai_entity.update(cx, |ai, _cx| {
            ai.cancel_chat_stream();
            ai.cancel_chat_conversation_state()
        });
        if let Some(conversation_id) = conversation_id.as_deref() {
            self.persist_ai_stopped_assistant_turns(conversation_id, &stopped_turns, cx);
        }
    }

    pub(in crate::workspace) fn persist_ai_stopped_assistant_turns(
        &self,
        conversation_id: &str,
        stopped_turns: &[AiStoppedAssistantTurn],
        cx: &App,
    ) {
        for stopped in stopped_turns {
            if stopped.retained {
                self.persist_ai_assistant_turn_end(
                    conversation_id,
                    &stopped.message_id,
                    stopped.status,
                    cx,
                );
            } else {
                self.persist_ai_removed_assistant_turn_end(
                    conversation_id,
                    &stopped.message_id,
                    stopped.status,
                    cx,
                );
            }
        }
    }

    pub(in crate::workspace) fn retry_ai_chat_initialization(&mut self, cx: &mut Context<Self>) {
        let outcome = self.ai_entity.update(cx, |ai, _cx| {
            ai.retry_chat_initialization(default_ai_conversations_path())
        });
        if matches!(outcome, AiChatInitializationOutcome::Loaded) {
            self.reset_ai_message_list();
        }
        cx.notify();
    }

    pub(in crate::workspace) fn ai_messages_count_label(&self, count: usize) -> String {
        self.i18n
            .t("ai.chat.messages_count")
            .replace("{{count}}", &count.to_string())
    }

    pub(in crate::workspace) fn next_ai_chat_id(&mut self, now_ms: i64, cx: &mut App) -> String {
        self.ai_entity
            .update(cx, |ai, _cx| ai.next_chat_id(now_ms))
    }

    pub(in crate::workspace) fn open_ai_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace
            .update(cx, |settings, cx| settings.set_active_tab(SettingsTab::Ai, cx));
        self.open_settings(window, cx);
    }
}
