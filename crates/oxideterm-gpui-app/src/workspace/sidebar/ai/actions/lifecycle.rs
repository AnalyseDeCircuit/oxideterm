impl WorkspaceApp {
    pub(in crate::workspace) fn ensure_ai_chat_initialized(&mut self, cx: &mut App) {
        let outcome = self.ai_entity.update(cx, |ai, _cx| {
            ai.ensure_chat_initialized(default_ai_conversations_path())
        });
        if matches!(outcome, AiChatInitializationOutcome::Loaded) {
            self.reset_ai_message_list(cx);
        }
    }

    fn reset_ai_message_list(&mut self, cx: &mut App) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.reset_chat_message_list();
        });
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
        self.ai_entity.update(cx, |ai, _cx| {
            ai.blur_chat_input(false);
        });
        self.close_ai_model_selector(cx);
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn close_ai_sidebar_popovers(&mut self, cx: &mut App) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.close_chat_popovers();
        });
        self.close_ai_model_selector(cx);
    }

    pub(in crate::workspace) fn close_ai_model_selector(&mut self, cx: &mut App) {
        // The compact model selector behaves like a browser/Radix Select with a
        // searchable input owner. Closing it must clear popup state, keyboard
        // focus origin, highlighted option, and any marked text together so Esc,
        // outside click, Tab, footer navigation, and row activation do not drift.
        let restore_terminal_inline_prompt = self.ai_entity.read(cx).model_selector_scope()
            == Some(AiModelSelectorScope::TerminalInline)
            && self.ai_entity.read(cx).terminal_inline_panel().open;
        self.ai_entity.update(cx, |ai, _cx| {
            ai.close_model_selector();
        });
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
        self.ai_entity.update(cx, |ai, _cx| {
            ai.reset_chat_for_conversation_selection();
        });
    }

    pub(in crate::workspace) fn begin_ai_conversation_rename(
        &mut self,
        id: String,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title_len = title.encode_utf16().count();
        self.ai_entity.update(cx, |ai, _cx| {
            ai.begin_conversation_rename(id, title);
        });
        self.ime_marked_text = None;
        self.set_ime_selection_from_anchor(
            WorkspaceImeTarget::AiConversationRename,
            0,
            title_len,
        );
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn save_ai_conversation_rename(&mut self, cx: &mut Context<Self>) {
        let (conversation_id, draft) = {
            let ai = self.ai_entity.read(cx);
            (
                ai.chat_ui().renaming_conversation_id.clone(),
                ai.chat_ui().renaming_conversation_draft.trim().to_string(),
            )
        };
        let Some(conversation_id) = conversation_id else {
            return;
        };
        self.ai_entity.update(cx, |ai, _cx| {
            if draft.is_empty() {
                ai.clear_conversation_rename();
            } else {
                ai.rename_conversation(&conversation_id, draft, ai_now_ms());
                ai.persist_chat_state();
            }
        });
        self.clear_ime_selection();
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn cancel_ai_conversation_rename(&mut self, cx: &mut Context<Self>) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.clear_conversation_rename();
        });
        self.clear_ime_selection();
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn delete_ai_conversation(&mut self, id: &str, cx: &mut App) {
        self.acp_entity.update(cx, |entity, _cx| {
            entity.close_thread(id, false);
        });
        let has_conversations = self.ai_entity.update(cx, |ai, _cx| {
            let has_conversations = ai.delete_conversation(id);
            ai.persist_chat_state();
            has_conversations
        });
        self.ai_entity.update(cx, |ai, _cx| {
            ai.reset_chat_after_conversation_delete(has_conversations);
        });
    }

    pub(in crate::workspace) fn clear_ai_conversations(&mut self, cx: &mut App) {
        // Cancel the live generation before clearing its routing identifier.
        self.cancel_ai_chat_stream_without_notify(cx);
        self.acp_entity.update(cx, |entity, _cx| {
            entity.close_all_threads(false);
        });
        self.ai_entity.update(cx, |ai, _cx| {
            ai.clear_conversations();
            ai.persist_chat_state();
        });
        self.ai_entity.update(cx, |ai, _cx| {
            ai.clear_chat_expansions();
        });
        self.close_ai_sidebar_popovers(cx);
    }

    pub(in crate::workspace) fn cancel_ai_chat_stream_without_notify(&mut self, cx: &mut App) {
        let cancelled_generation = self.ai_entity.read(cx).chat_stream_generation();
        let active_conversation_id = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone();
        if let Some(conversation_id) = active_conversation_id.as_deref() {
            // The ACP entity is the sole process/session owner. Route Stop to
            // it before invalidating the UI generation so the protocol cancel
            // reaches the still-live session.
            let _ = self
                .acp_entity
                .read(cx)
                .cancel_active_turn(conversation_id);
        }
        let (conversation_id, stopped_turns) = self.ai_entity.update(cx, |ai, _cx| {
            ai.cancel_chat_stream();
            ai.cancel_chat_conversation_state()
        });
        // An abort can leave a UI delivery queued behind the model task. Revoke
        // its lease before that delivery can reach a terminal or other owner.
        self.ai_runtime_context.update(cx, |runtime, _cx| {
            runtime.finish_tool_session(
                cancelled_generation,
                oxideterm_ai::RuntimeRevocationReason::ToolSessionCancelled,
            );
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
            self.reset_ai_message_list(cx);
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
