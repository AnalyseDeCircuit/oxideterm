impl WorkspaceApp {
    fn create_ai_sidebar_conversation(
        &mut self,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) -> String {
        let now = ai_now_ms();
        let id = self.next_ai_chat_id(now);
        let profile_id = self
            .settings_store
            .settings()
            .ai
            .execution_profiles
            .get("defaultProfileId")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let id = self
            .ai_chat
            .create_conversation(id, title, now, profile_id);
        self.persist_ai_chat_state();
        self.ai_conversation_list_open = false;
        self.ai_chat_menu_open = false;
        self.ai_chat_draft.clear();
        self.ai_chat_input_focused = false;
        self.ai_chat_autocomplete_index = 0;
        self.ai_chat_autocomplete_suppressed = false;
        cx.notify();
        id
    }

    fn send_ai_chat_draft(&mut self, cx: &mut Context<Self>) {
        let content = self.ai_chat_draft.trim().to_string();
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

        let parsed_input = parse_ai_user_input(&content);
        let sidebar_context = self.resolve_ai_sidebar_context_block(cx);
        let selected_context = self.resolve_ai_selected_terminal_context(cx);
        let reference_context = self.resolve_ai_reference_context(&parsed_input.references, cx);
        let context = ai_chat_message_context([
            selected_context.or(sidebar_context),
            reference_context,
        ]);
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
        let profile_id = self
            .settings_store
            .settings()
            .ai
            .execution_profiles
            .get("defaultProfileId")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let conversation_id = self
            .ai_chat
            .ensure_conversation(id, Some(title), now, profile_id);
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
        };
        self.ai_chat.add_message(&conversation_id, message);
        self.persist_ai_chat_state();
        let request_content = (!parsed_input.clean_text.is_empty())
            .then_some(parsed_input.clean_text.clone());
        let applied_profile = self.resolved_ai_execution_profile();
        let runtime_system_prompt = applied_profile
            .include_runtime_chips
            .then(|| self.resolve_ai_sidebar_system_prompt_segment(cx))
            .flatten();
        let task_system_prompt = ai_chat_message_context([
            ai_input_system_prompt(slash_command, &parsed_input.participants),
            runtime_system_prompt,
        ]);
        self.start_ai_chat_stream(
            conversation_id,
            stream_config,
            request_content,
            task_system_prompt,
            cx,
        );
        self.reset_ai_chat_input_after_submit();
        cx.notify();
    }

    fn add_ai_help_response(&mut self, content: String, cx: &mut Context<Self>) {
        let now = ai_now_ms();
        let title = generate_chat_title(&content);
        let id = self.next_ai_chat_id(now);
        let profile_id = self
            .settings_store
            .settings()
            .ai
            .execution_profiles
            .get("defaultProfileId")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let conversation_id = self
            .ai_chat
            .ensure_conversation(id, Some(title), now, profile_id);
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
        };
        self.ai_chat.add_message(&conversation_id, user_message);
        self.ai_chat.add_message(&conversation_id, assistant_message);
        self.persist_ai_chat_state();
        self.reset_ai_chat_input_after_submit();
        cx.notify();
    }

    fn regenerate_ai_last_response(&mut self, cx: &mut Context<Self>) {
        if self.ai_chat_loading {
            cx.notify();
            return;
        }
        let Some(conversation_id) = self.ai_chat.active_conversation_id.clone() else {
            return;
        };
        let Some(conversation) = self
            .ai_chat
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
        self.start_ai_chat_stream(conversation_id, stream_config, None, None, cx);
        cx.notify();
    }

    fn request_delete_ai_message(&mut self, message_id: String, cx: &mut Context<Self>) {
        if self.ai_chat_loading {
            cx.notify();
            return;
        }
        self.ai_delete_message_confirm = Some(message_id);
        cx.notify();
    }

    fn delete_ai_message(&mut self, message_id: &str, cx: &mut Context<Self>) {
        let Some(conversation) = self.ai_chat.active_conversation_mut() else {
            return;
        };
        let original_len = conversation.messages.len();
        conversation.messages.retain(|message| message.id != message_id);
        if conversation.messages.len() == original_len {
            return;
        }
        self.ai_thinking_expansion_state.remove(message_id);
        conversation.message_count = conversation.messages.len();
        conversation.updated_at_ms = ai_now_ms();
        self.persist_ai_chat_state();
        cx.notify();
    }

    fn set_ai_conversation_profile(
        &mut self,
        conversation_id: &str,
        profile_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(conversation) = self
            .ai_chat
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return;
        };
        let mut metadata = conversation
            .session_metadata
            .as_ref()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        metadata.insert(
            "conversationId".to_string(),
            serde_json::json!(conversation.id.clone()),
        );
        metadata.insert(
            "origin".to_string(),
            serde_json::json!(conversation.origin.clone()),
        );
        metadata.insert("profileId".to_string(), serde_json::json!(&profile_id));
        conversation.profile_id = Some(profile_id);
        conversation.session_metadata = Some(serde_json::Value::Object(metadata));
        conversation.updated_at_ms = ai_now_ms();
        self.ai_profile_selector_open = false;
        self.persist_ai_chat_state();
        cx.notify();
    }

    fn set_ai_safety_mode_default(&mut self, cx: &mut Context<Self>) {
        if let Some(conversation_id) = self.ai_chat.active_conversation_id.as_ref() {
            self.ai_safety_bypass_conversations.remove(conversation_id);
        }
        self.ai_safety_menu_open = false;
        self.ai_safety_confirm_open = false;
        cx.notify();
    }

    fn confirm_ai_safety_bypass(&mut self, cx: &mut Context<Self>) {
        if let Some(conversation_id) = self.ai_chat.active_conversation_id.as_ref() {
            self.ai_safety_bypass_conversations
                .insert(conversation_id.clone());
        }
        self.ai_safety_menu_open = false;
        self.ai_safety_confirm_open = false;
        cx.notify();
    }

    fn start_edit_ai_message(&mut self, message_id: String, content: String, cx: &mut Context<Self>) {
        if self.ai_chat_loading {
            cx.notify();
            return;
        }
        self.ai_editing_message_id = Some(message_id);
        self.ai_editing_message_draft = content;
        self.ai_editing_message_focused = true;
        self.ai_chat_input_focused = false;
        self.ai_model_selector_search_focused = false;
        self.ime_marked_text = None;
        cx.notify();
    }

    fn cancel_edit_ai_message(&mut self, cx: &mut Context<Self>) {
        self.ai_editing_message_id = None;
        self.ai_editing_message_draft.clear();
        self.ai_editing_message_focused = false;
        self.ime_marked_text = None;
        cx.notify();
    }

    fn save_ai_message_edit(&mut self, cx: &mut Context<Self>) {
        if self.ai_chat_loading {
            cx.notify();
            return;
        }
        let edited_content = self.ai_editing_message_draft.trim().to_string();
        if edited_content.is_empty() {
            cx.notify();
            return;
        }
        let Some(message_id) = self.ai_editing_message_id.clone() else {
            return;
        };
        let Some(conversation_id) = self.ai_chat.active_conversation_id.clone() else {
            return;
        };
        let Some(conversation_index) = self
            .ai_chat
            .conversations
            .iter()
            .position(|conversation| conversation.id == conversation_id)
        else {
            return;
        };
        let message_index = {
            let conversation = &self.ai_chat.conversations[conversation_index];
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
            let conversation = &self.ai_chat.conversations[conversation_index];
            let original = &conversation.messages[message_index];
            let current_tail = strip_ai_nested_branches(&conversation.messages[message_index..]);
            let mut branches = original.branches.clone().unwrap_or_else(|| AiMessageBranches {
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
            let conversation = &mut self.ai_chat.conversations[conversation_index];
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
            });
            conversation.message_count = conversation.messages.len();
            conversation.updated_at_ms = now;
        }
        self.ai_editing_message_id = None;
        self.ai_editing_message_draft.clear();
        self.ai_editing_message_focused = false;
        self.ime_marked_text = None;
        self.persist_ai_chat_state();
        self.start_ai_chat_stream(conversation_id, stream_config, request_content, None, cx);
        cx.notify();
    }

    fn switch_ai_message_branch(
        &mut self,
        message_id: String,
        branch_index: usize,
        cx: &mut Context<Self>,
    ) {
        if self.ai_chat_loading {
            cx.notify();
            return;
        }
        let Some(conversation) = self.ai_chat.active_conversation_mut() else {
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
        self.ai_editing_message_id = None;
        self.ai_editing_message_draft.clear();
        self.ai_editing_message_focused = false;
        self.persist_ai_chat_state();
        cx.notify();
    }

    fn reset_ai_chat_input_after_submit(&mut self) {
        self.ai_chat_draft.clear();
        self.ai_chat_autocomplete_index = 0;
        self.ai_chat_autocomplete_suppressed = false;
        self.ai_chat_include_context = false;
        self.ai_chat_include_all_panes = false;
        self.ime_marked_text = None;
    }


}

fn ai_chat_message_context(contexts: [Option<String>; 2]) -> Option<String> {
    let blocks = contexts
        .into_iter()
        .flatten()
        .map(|context| context.trim().to_string())
        .filter(|context| !context.is_empty())
        .collect::<Vec<_>>();
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}

fn strip_ai_nested_branches(messages: &[AiChatMessage]) -> Vec<AiChatMessage> {
    messages
        .iter()
        .cloned()
        .map(|mut message| {
            message.branches = None;
            message
        })
        .collect()
}
