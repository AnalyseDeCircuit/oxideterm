impl WorkspaceApp {
    pub(in crate::workspace) fn ensure_ai_chat_initialized(&mut self, cx: &mut App) {
        self.ai.state.update(cx, |ai, cx| {
            ai.ensure_chat_initialized(default_ai_conversations_path(), cx);
        });
    }

    pub(in crate::workspace) fn bootstrap_ai_mcp_registry(&self, cx: &App) {
        // Tauri boots the MCP registry from AiChatPanel mount, not from process
        // startup or every settings write. Keep native at the same user-visible
        // boundary so HTTP auth-token/keychain access only happens when the AI
        // surface is actually in use.
        let registry = self.ai.state.read(cx).mcp_registry().clone();
        let configs = self.settings_store.settings().ai.mcp_servers.clone();
        self.forwarding_runtime.spawn(async move {
            registry.connect_all_values(&configs).await;
        });
    }

    pub(in crate::workspace) fn clear_ai_sidebar_keyboard_focus(&mut self, cx: &mut App) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.blur_chat_input(false);
        });
        self.close_ai_model_selector(cx);
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn close_ai_sidebar_popovers(&mut self, cx: &mut App) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.close_chat_popovers();
        });
        self.close_ai_model_selector(cx);
    }

    pub(in crate::workspace) fn close_ai_model_selector(&mut self, cx: &mut App) {
        self.ai.state.update(cx, |state, _| state.dismiss_model_selector());
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn cancel_ai_chat_stream(&mut self, cx: &mut Context<Self>) {
        self.cancel_ai_chat_stream_without_notify(cx);
        self.ai.state.read(cx).persist_chat_state();
        cx.notify();
    }

    pub(in crate::workspace) fn select_ai_conversation(&mut self, id: String, cx: &mut App) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.select_conversation(id);
        });
        self.ai.state.update(cx, |ai, _cx| {
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
        self.ai.state.update(cx, |ai, _cx| {
            ai.begin_conversation_rename(id, title);
        });
        self.ime_marked_text = None;
        self.set_ime_selection_from_anchor(WorkspaceImeTarget::AiConversationRename, 0, title_len);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn save_ai_conversation_rename(&mut self, cx: &mut Context<Self>) {
        let (conversation_id, draft) = {
            let ai = self.ai.state.read(cx);
            (
                ai.chat_ui().renaming_conversation_id.clone(),
                ai.chat_ui().renaming_conversation_draft.trim().to_string(),
            )
        };
        let Some(conversation_id) = conversation_id else {
            return;
        };
        self.ai.state.update(cx, |ai, _cx| {
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
        self.ai.state.update(cx, |ai, _cx| {
            ai.clear_conversation_rename();
        });
        self.clear_ime_selection();
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn set_ai_conversation_archived(
        &mut self,
        id: &str,
        archived: bool,
        cx: &mut App,
    ) {
        self.ai.state
            .update(cx, |ai, _| ai.set_conversation_archived(id, archived));
    }

    pub(in crate::workspace) fn delete_ai_conversation(&mut self, id: &str, cx: &mut App) {
        self.ai.conversations.delete_ai_conversation(id, cx);
    }

    pub(in crate::workspace) fn clear_ai_conversations(&mut self, cx: &mut App) {
        self.ai.conversations.clear_ai_conversations(cx);
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn cancel_ai_chat_stream_without_notify(&mut self, cx: &mut App) {
        self.ai.conversations.cancel_ai_chat_stream_without_notify(cx);
    }

    pub(in crate::workspace) fn retry_ai_chat_initialization(&mut self, cx: &mut Context<Self>) {
        self.ai.state.update(cx, |ai, cx| {
            ai.retry_chat_initialization(default_ai_conversations_path(), cx)
        });
        cx.notify();
    }

    pub(in crate::workspace) fn ai_conversation_turns_label(&self, count: usize) -> String {
        self.i18n
            .t("ai.chat.turns_count")
            .replace("{{count}}", &count.to_string())
    }

    pub(in crate::workspace) fn next_ai_chat_id(&mut self, now_ms: i64, cx: &mut App) -> String {
        self.ai.state.update(cx, |ai, _cx| ai.next_chat_id(now_ms))
    }

    pub(in crate::workspace) fn open_ai_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::Ai, cx)
        });
        self.open_settings(window, cx);
    }
}
