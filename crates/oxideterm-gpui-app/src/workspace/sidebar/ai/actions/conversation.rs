use crate::workspace::ai_state::AiStandardConfirmKind;

impl WorkspaceApp {
    pub(in crate::workspace) fn open_ai_safety_confirm(&mut self, cx: &mut Context<Self>) {
        self.ai.chat.safety_confirm_open = true;
        self.ai.chat.safety_confirm_presence.reopen();
        // Pointer-opened confirmations do not show keyboard focus until navigation starts.
        self.clear_standard_confirm_focus();
        cx.notify();
    }

    pub(in crate::workspace) fn begin_ai_safety_confirm_exit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self.ai.chat.safety_confirm_presence.begin_exit() else {
            return false;
        };
        self.clear_standard_confirm_focus();
        self.schedule_ai_standard_confirm_exit(AiStandardConfirmKind::Safety, generation, cx);
        true
    }

    pub(in crate::workspace) fn open_ai_summarize_confirm(&mut self, cx: &mut Context<Self>) {
        self.ai.chat.summarize_confirm_open = true;
        self.ai.chat.summarize_confirm_presence.reopen();
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(in crate::workspace) fn begin_ai_summarize_confirm_exit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self.ai.chat.summarize_confirm_presence.begin_exit() else {
            return false;
        };
        self.clear_standard_confirm_focus();
        self.schedule_ai_standard_confirm_exit(AiStandardConfirmKind::Summarize, generation, cx);
        true
    }

    fn schedule_ai_standard_confirm_exit(
        &mut self,
        kind: AiStandardConfirmKind,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        if delay.is_zero() {
            self.finish_ai_standard_confirm_exit(kind, generation);
            return;
        }
        // The open flag remains set until this generation's exit frame completes.
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.finish_ai_standard_confirm_exit(kind, generation) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn finish_ai_standard_confirm_exit(
        &mut self,
        kind: AiStandardConfirmKind,
        generation: u64,
    ) -> bool {
        let finished = match kind {
            AiStandardConfirmKind::Safety => {
                self.ai.chat.safety_confirm_presence.finish_exit(generation)
            }
            AiStandardConfirmKind::Summarize => self
                .ai
                .chat
                .summarize_confirm_presence
                .finish_exit(generation),
        };
        if !finished {
            return false;
        }
        match kind {
            AiStandardConfirmKind::Safety => self.ai.chat.safety_confirm_open = false,
            AiStandardConfirmKind::Summarize => self.ai.chat.summarize_confirm_open = false,
        }
        true
    }

    pub(in crate::workspace) fn create_ai_sidebar_conversation(
        &mut self,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) -> String {
        self.ensure_ai_chat_initialized();
        let now = ai_now_ms();
        let id = self.next_ai_chat_id(now);
        let id = self
            .ai
            .chat
            .conversation_state
            .create_conversation(id, title, now, None);
        self.persist_ai_chat_state();
        self.ai.chat.conversation_list_open = false;
        self.ai.chat.menu_open = false;
        self.ai.chat.draft.clear();
        self.ai.chat.input_focused = false;
        self.ai.chat.autocomplete_index = 0;
        self.ai.chat.autocomplete_suppressed = false;
        cx.notify();
        id
    }

    pub(in crate::workspace) fn send_ai_chat_draft(&mut self, cx: &mut Context<Self>) {
        self.ensure_ai_chat_initialized();
        let content = self.ai.chat.draft.trim().to_string();
        if content.is_empty() {
            cx.notify();
            return;
        }
        if !self.settings_store.settings().ai.enabled {
            self.push_ai_settings_toast(
                self.i18n.t("ai.chat.disabled_message"),
                TerminalNoticeVariant::Warning,
            );
            cx.notify();
            return;
        }
        self.bootstrap_ai_mcp_registry();

        let parsed_input = parse_ai_user_input(&content);
        let detected_intent = detect_ai_intent(&parsed_input);
        let sidebar_context = self.resolve_ai_sidebar_context_block(cx);
        let selected_context = self.resolve_ai_selected_terminal_context(cx);
        let reference_context = self.resolve_ai_reference_context(&parsed_input.references, cx);
        let context =
            ai_chat_message_context([selected_context.or(sidebar_context), reference_context]);
        let slash_command = parsed_input
            .slash_command
            .as_deref()
            .and_then(resolve_ai_slash_command);
        if let Some(command) = slash_command.filter(|command| command.client_only) {
            match command.name {
                "clear" => {
                    self.create_ai_sidebar_conversation(None, cx);
                    self.reset_ai_chat_input_after_submit();
                    cx.notify();
                    return;
                }
                "help" => {
                    self.add_ai_help_response(content, cx);
                    return;
                }
                "compact" => {
                    self.start_ai_compact_conversation(cx);
                    self.reset_ai_chat_input_after_submit();
                    cx.notify();
                    return;
                }
                _ => return,
            }
        }

        let stream_config = match self.resolve_ai_stream_config() {
            Ok(config) => config,
            Err(error) => {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        let now = ai_now_ms();
        let title = generate_chat_title(&content);
        let id = self.next_ai_chat_id(now);
        let conversation_id =
            self.ai
                .chat
                .conversation_state
                .ensure_conversation(id, Some(title), now, None);
        let message = AiChatMessage {
            id: self.next_ai_chat_id(now),
            role: AiChatRole::User,
            content,
            timestamp_ms: now,
            model: Some(stream_config.model.clone()),
            context,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };
        self.ai
            .chat
            .conversation_state
            .add_message(&conversation_id, message);
        self.persist_ai_chat_state();
        let request_content =
            (!parsed_input.clean_text.is_empty()).then_some(parsed_input.clean_text.clone());
        let runtime_system_prompt = self.resolve_ai_sidebar_system_prompt_segment(cx);
        let task_system_prompt = ai_chat_message_context([
            ai_input_system_prompt(slash_command, &parsed_input.participants),
            runtime_system_prompt,
            ai_detected_intent_system_prompt(&detected_intent),
        ]);
        if let Some(conversation) = self
            .ai
            .chat
            .conversation_state
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        {
            let metadata = conversation
                .session_metadata
                .get_or_insert_with(|| serde_json::json!({ "conversationId": conversation_id }));
            if let Some(object) = metadata.as_object_mut() {
                object.insert(
                    "conversationId".to_string(),
                    serde_json::json!(conversation_id),
                );
                object.insert("origin".to_string(), serde_json::json!("sidebar"));
                object
                    .entry("firstUserMessage".to_string())
                    .or_insert_with(|| serde_json::json!(parsed_input.raw_text.clone()));
                object.insert(
                    "providerId".to_string(),
                    serde_json::json!(stream_config.provider_id),
                );
                object.insert(
                    "providerModel".to_string(),
                    serde_json::json!(stream_config.model),
                );
                if let Some(participant) = parsed_input.participants.first() {
                    object.insert(
                        "activeParticipant".to_string(),
                        serde_json::json!(participant.name.clone()),
                    );
                }
            }
        }
        self.start_ai_chat_stream_after_api_key_lookup(
            conversation_id,
            stream_config,
            request_content,
            task_system_prompt,
            cx,
        );
        self.reset_ai_chat_input_after_submit();
        cx.notify();
    }

    pub(in crate::workspace) fn start_ai_chat_stream_after_api_key_lookup(
        &mut self,
        conversation_id: String,
        mut stream_config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let requires_key = ai_provider_chat_requires_key(&stream_config.provider_type);
        let Some(provider_id) = stream_config.provider_id.clone() else {
            self.start_ai_chat_stream_after_rag_lookup(
                conversation_id,
                stream_config,
                request_content,
                task_system_prompt,
                cx,
            );
            return;
        };
        let key_store = self.ai.models.key_store.clone();
        let runtime = self.forwarding_runtime.clone();
        let failed_to_get_key = self.i18n.t("ai.model_selector.failed_to_get_api_key");
        let api_key_not_found = self.i18n.t("ai.model_selector.api_key_not_found");
        self.ai.chat.loading = true;
        cx.spawn(async move |weak, cx| {
            let key_result = runtime
                .spawn_blocking(move || key_store.get_provider_key(&provider_id))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| match key_result {
                Ok(api_key) => {
                    if requires_key && api_key.is_none() {
                        this.ai.chat.loading = false;
                        this.push_ai_settings_toast(
                            api_key_not_found,
                            TerminalNoticeVariant::Error,
                        );
                        cx.notify();
                        return;
                    }
                    stream_config.api_key = api_key;
                    this.start_ai_chat_stream_after_rag_lookup(
                        conversation_id,
                        stream_config,
                        request_content,
                        task_system_prompt,
                        cx,
                    );
                }
                Err(_) if requires_key => {
                    this.ai.chat.loading = false;
                    this.push_ai_settings_toast(failed_to_get_key, TerminalNoticeVariant::Error);
                    cx.notify();
                }
                Err(_) => {
                    stream_config.api_key = None;
                    this.start_ai_chat_stream_after_rag_lookup(
                        conversation_id,
                        stream_config,
                        request_content,
                        task_system_prompt,
                        cx,
                    );
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn add_ai_help_response(
        &mut self,
        content: String,
        cx: &mut Context<Self>,
    ) {
        let now = ai_now_ms();
        let title = generate_chat_title(&content);
        let id = self.next_ai_chat_id(now);
        let conversation_id =
            self.ai
                .chat
                .conversation_state
                .ensure_conversation(id, Some(title), now, None);
        let user_message = AiChatMessage {
            id: self.next_ai_chat_id(now),
            role: AiChatRole::User,
            content,
            timestamp_ms: now,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };
        let assistant_message = AiChatMessage {
            id: self.next_ai_chat_id(now),
            role: AiChatRole::Assistant,
            content: self.ai_help_markdown(),
            timestamp_ms: now,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        };
        self.ai
            .chat
            .conversation_state
            .add_message(&conversation_id, user_message);
        self.ai
            .chat
            .conversation_state
            .add_message(&conversation_id, assistant_message);
        self.persist_ai_chat_state();
        self.reset_ai_chat_input_after_submit();
        cx.notify();
    }

    pub(in crate::workspace) fn send_ai_follow_up_suggestion(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.ai.chat.draft = text;
        self.send_ai_chat_draft(cx);
    }

    pub(in crate::workspace) fn regenerate_ai_last_response(&mut self, cx: &mut Context<Self>) {
        if self.ai.chat.loading {
            cx.notify();
            return;
        }
        let Some(conversation_id) = self
            .ai
            .chat
            .conversation_state
            .active_conversation_id
            .clone()
        else {
            return;
        };
        let Some(conversation) = self
            .ai
            .chat
            .conversation_state
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return;
        };
        let Some(last_user_index) = conversation
            .messages
            .iter()
            .rposition(|message| message.role == AiChatRole::User)
        else {
            return;
        };
        conversation.messages.truncate(last_user_index + 1);
        conversation.message_count = conversation.messages.len();
        conversation.updated_at_ms = ai_now_ms();
        let stream_config = match self.resolve_ai_stream_config() {
            Ok(config) => config,
            Err(error) => {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        self.persist_ai_chat_state();
        self.start_ai_chat_stream_after_api_key_lookup(
            conversation_id,
            stream_config,
            None,
            None,
            cx,
        );
        cx.notify();
    }

    pub(in crate::workspace) fn request_delete_ai_message(
        &mut self,
        message_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.ai.chat.loading {
            cx.notify();
            return;
        }
        self.ai.chat.delete_message_confirm = Some(message_id);
        self.ai_delete_message_confirm_presence.reopen();
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(in crate::workspace) fn delete_ai_message(
        &mut self,
        message_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(conversation) = self.ai.chat.conversation_state.active_conversation_mut() else {
            return;
        };
        let original_len = conversation.messages.len();
        conversation
            .messages
            .retain(|message| message.id != message_id);
        if conversation.messages.len() == original_len {
            return;
        }
        self.ai.chat.thinking_expansion_state.remove(message_id);
        conversation.message_count = conversation.messages.len();
        conversation.updated_at_ms = ai_now_ms();
        self.persist_ai_chat_state();
        cx.notify();
    }

    pub(in crate::workspace) fn set_ai_safety_mode_default(&mut self, cx: &mut Context<Self>) {
        if let Some(conversation_id) = self
            .ai
            .chat
            .conversation_state
            .active_conversation_id
            .as_ref()
        {
            self.ai
                .chat
                .safety_bypass_conversations
                .remove(conversation_id);
        }
        self.ai.chat.safety_menu_open = false;
        self.restore_ai_chat_input_focus_after_safety_mode_change();
        cx.notify();
    }

    pub(in crate::workspace) fn confirm_ai_safety_bypass(&mut self, cx: &mut Context<Self>) {
        if let Some(conversation_id) = self
            .ai
            .chat
            .conversation_state
            .active_conversation_id
            .as_ref()
        {
            self.ai
                .chat
                .safety_bypass_conversations
                .insert(conversation_id.clone());
        }
        self.ai.chat.safety_menu_open = false;
        self.restore_ai_chat_input_focus_after_safety_mode_change();
        cx.notify();
    }

    pub(in crate::workspace) fn restore_ai_chat_input_focus_after_safety_mode_change(&mut self) {
        // Closing the safety menu returns keyboard ownership to the composer so
        // Enter/Space continue the conversation instead of falling through.
        self.ai.chat.input_focused = true;
        self.ai.chat.footer_focus = None;
        self.ai.models.selector_search_focused = false;
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn start_edit_ai_message(
        &mut self,
        message_id: String,
        content: String,
        cx: &mut Context<Self>,
    ) {
        if self.ai.chat.loading {
            cx.notify();
            return;
        }
        self.ai.chat.editing_message_id = Some(message_id);
        self.ai.chat.editing_message_draft = content;
        self.ai.chat.editing_message_focused = true;
        self.ai.chat.input_focused = false;
        self.ai.models.selector_search_focused = false;
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn cancel_edit_ai_message(&mut self, cx: &mut Context<Self>) {
        self.ai.chat.editing_message_id = None;
        self.ai.chat.editing_message_draft.clear();
        self.ai.chat.editing_message_focused = false;
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn save_ai_message_edit(&mut self, cx: &mut Context<Self>) {
        if self.ai.chat.loading {
            cx.notify();
            return;
        }
        let edited_content = self.ai.chat.editing_message_draft.trim().to_string();
        if edited_content.is_empty() {
            cx.notify();
            return;
        }
        let Some(message_id) = self.ai.chat.editing_message_id.clone() else {
            return;
        };
        let Some(conversation_id) = self
            .ai
            .chat
            .conversation_state
            .active_conversation_id
            .clone()
        else {
            return;
        };
        let Some(conversation_index) = self
            .ai
            .chat
            .conversation_state
            .conversations
            .iter()
            .position(|conversation| conversation.id == conversation_id)
        else {
            return;
        };
        let message_index = {
            let conversation = &self.ai.chat.conversation_state.conversations[conversation_index];
            let Some(index) = conversation
                .messages
                .iter()
                .position(|message| message.id == message_id)
            else {
                return;
            };
            if conversation.messages[index].role != AiChatRole::User {
                return;
            }
            index
        };
        let stream_config = match self.resolve_ai_stream_config() {
            Ok(config) => config,
            Err(error) => {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };

        let now = ai_now_ms();
        let new_user_id = self.next_ai_chat_id(now);
        let (context, branch_data) = {
            let conversation = &self.ai.chat.conversation_state.conversations[conversation_index];
            let original = &conversation.messages[message_index];
            let current_tail = strip_ai_nested_branches(&conversation.messages[message_index..]);
            let mut branches = original
                .branches
                .clone()
                .unwrap_or_else(|| AiMessageBranches {
                    total: 2,
                    active_index: 1,
                    tails: HashMap::from([(0, current_tail.clone())]),
                });
            if original.branches.is_some() {
                branches.tails.insert(branches.active_index, current_tail);
                branches.total = branches.total.saturating_add(1);
                branches.active_index = branches.total.saturating_sub(1);
            }
            (original.context.clone(), branches)
        };
        let request_content = Some(edited_content.clone());
        {
            let conversation =
                &mut self.ai.chat.conversation_state.conversations[conversation_index];
            conversation.messages.truncate(message_index);
            conversation.messages.push(AiChatMessage {
                id: new_user_id,
                role: AiChatRole::User,
                content: edited_content,
                timestamp_ms: now,
                model: Some(stream_config.model.clone()),
                context,
                is_streaming: false,
                thinking_content: None,
                metadata: None,
                tool_call_id: None,
                tool_calls: Vec::new(),
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: Some(branch_data),
                suggestions: Vec::new(),
            });
            conversation.message_count = conversation.messages.len();
            conversation.updated_at_ms = now;
        }
        self.ai.chat.editing_message_id = None;
        self.ai.chat.editing_message_draft.clear();
        self.ai.chat.editing_message_focused = false;
        self.ime_marked_text = None;
        self.persist_ai_chat_state();
        self.start_ai_chat_stream_after_api_key_lookup(
            conversation_id,
            stream_config,
            request_content,
            None,
            cx,
        );
        cx.notify();
    }

    pub(in crate::workspace) fn switch_ai_message_branch(
        &mut self,
        message_id: String,
        branch_index: usize,
        cx: &mut Context<Self>,
    ) {
        if self.ai.chat.loading {
            cx.notify();
            return;
        }
        let Some(conversation) = self.ai.chat.conversation_state.active_conversation_mut() else {
            return;
        };
        let Some(message_index) = conversation
            .messages
            .iter()
            .position(|message| message.id == message_id)
        else {
            return;
        };
        let Some(branch_point) = conversation.messages.get(message_index) else {
            return;
        };
        let Some(mut branches) = branch_point.branches.clone() else {
            return;
        };
        if branch_index >= branches.total || branch_index == branches.active_index {
            return;
        }
        let live_tail = strip_ai_nested_branches(&conversation.messages[message_index..]);
        let Some(target_tail) = branches.tails.get(&branch_index).cloned() else {
            return;
        };
        if target_tail.is_empty() {
            return;
        }
        branches.tails.insert(branches.active_index, live_tail);
        branches.active_index = branch_index;
        let mut new_messages = conversation.messages[..message_index].to_vec();
        for (index, mut message) in target_tail.into_iter().enumerate() {
            if index == 0 {
                message.branches = Some(branches.clone());
            } else {
                message.branches = None;
            }
            new_messages.push(message);
        }
        conversation.messages = new_messages;
        conversation.message_count = conversation.messages.len();
        conversation.updated_at_ms = ai_now_ms();
        self.ai.chat.editing_message_id = None;
        self.ai.chat.editing_message_draft.clear();
        self.ai.chat.editing_message_focused = false;
        self.persist_ai_chat_state();
        cx.notify();
    }

    pub(in crate::workspace) fn reset_ai_chat_input_after_submit(&mut self) {
        self.ai.chat.draft.clear();
        self.ai.chat.autocomplete_index = 0;
        self.ai.chat.autocomplete_suppressed = false;
        self.ai.chat.include_context = false;
        self.ai.chat.include_all_panes = false;
        self.ime_marked_text = None;
    }
}

pub(in crate::workspace) fn ai_chat_message_context(
    contexts: impl IntoIterator<Item = Option<String>>,
) -> Option<String> {
    let blocks = contexts
        .into_iter()
        .flatten()
        .map(|context| context.trim().to_string())
        .filter(|context| !context.is_empty())
        .collect::<Vec<_>>();
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}

pub(in crate::workspace) fn strip_ai_nested_branches(
    messages: &[AiChatMessage],
) -> Vec<AiChatMessage> {
    messages
        .iter()
        .cloned()
        .map(|mut message| {
            message.branches = None;
            message
        })
        .collect()
}
