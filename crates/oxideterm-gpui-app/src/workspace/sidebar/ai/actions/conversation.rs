use crate::workspace::ai_state::AiStandardConfirmKind;

impl AiWorkspaceEntity {
    fn create_chat_conversation(
        &mut self,
        id: String,
        title: Option<String>,
        now_ms: i64,
    ) -> String {
        let id = self
            .conversation_state_mut()
            .create_conversation(id, title, now_ms, None);
        self.persist_chat_state();
        id
    }

    fn begin_sidebar_user_turn(
        &mut self,
        candidate_id: String,
        title: String,
        now_ms: i64,
        message: AiChatMessage,
        stream_config: &AiChatStreamConfig,
        active_participant: Option<String>,
    ) -> String {
        let message_id = message.id.clone();
        let backend = ai_message_backend_for_stream(stream_config);
        let first_user_message = message.content.clone();
        let conversation_id = self.conversation_state_mut().ensure_conversation(
            candidate_id,
            Some(title),
            now_ms,
            None,
        );
        self.conversation_state_mut()
            .add_message(&conversation_id, message);
        if let Some(conversation) = self
            .conversation_state_mut()
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
                    .or_insert_with(|| serde_json::json!(first_user_message));
                object.insert(
                    "providerId".to_string(),
                    serde_json::json!(stream_config.provider_id),
                );
                object.insert(
                    "providerModel".to_string(),
                    serde_json::json!(stream_config.model),
                );
                if let Some(participant) = active_participant {
                    object.insert(
                        "activeParticipant".to_string(),
                        serde_json::json!(participant),
                    );
                }
            }
            // Provenance is safe structural metadata used to avoid replaying
            // messages that the selected ACP session already owns.
            oxideterm_ai::store_ai_message_backend_provenance(
                conversation,
                &message_id,
                backend,
            );
        }
        // Save the message and session metadata as one projection.
        self.persist_chat_state();
        conversation_id
    }

    fn add_help_exchange(
        &mut self,
        candidate_id: String,
        title: String,
        now_ms: i64,
        user_message: AiChatMessage,
        assistant_message: AiChatMessage,
    ) {
        let conversation_id = self.conversation_state_mut().ensure_conversation(
            candidate_id,
            Some(title),
            now_ms,
            None,
        );
        self.conversation_state_mut()
            .add_message(&conversation_id, user_message);
        self.conversation_state_mut()
            .add_message(&conversation_id, assistant_message);
        self.persist_chat_state();
    }

    fn truncate_active_conversation_after_last_user(&mut self, now_ms: i64) -> Option<String> {
        let conversation = self.conversation_state_mut().active_conversation_mut()?;
        let last_user_index = conversation
            .messages
            .iter()
            .rposition(|message| message.role == AiChatRole::User)?;
        conversation.messages.truncate(last_user_index + 1);
        conversation.message_count = conversation.messages.len();
        conversation.updated_at_ms = now_ms;
        let conversation_id = conversation.id.clone();
        self.persist_chat_state();
        Some(conversation_id)
    }

    fn delete_active_message(&mut self, message_id: &str, now_ms: i64) -> bool {
        let Some(conversation) = self.conversation_state_mut().active_conversation_mut() else {
            return false;
        };
        let original_len = conversation.messages.len();
        conversation
            .messages
            .retain(|message| message.id != message_id);
        if conversation.messages.len() == original_len {
            return false;
        }
        conversation.message_count = conversation.messages.len();
        conversation.updated_at_ms = now_ms;
        self.persist_chat_state();
        true
    }

    fn replace_active_user_message(
        &mut self,
        message_id: &str,
        new_user_id: String,
        edited_content: String,
        model: String,
        backend: oxideterm_ai::AiMessageBackendProvenance,
        now_ms: i64,
    ) -> Option<String> {
        let conversation = self.conversation_state_mut().active_conversation_mut()?;
        let message_index = conversation
            .messages
            .iter()
            .position(|message| message.id == message_id)?;
        if conversation.messages.get(message_index)?.role != AiChatRole::User {
            return None;
        }
        let current_tail = strip_ai_nested_branches(&conversation.messages[message_index..]);
        let original = conversation.messages.get_mut(message_index)?;
        let branches = match original.branches.take() {
            Some(mut branches) => {
                branches.tails.insert(branches.active_index, current_tail);
                branches.total = branches.total.saturating_add(1);
                branches.active_index = branches.total.saturating_sub(1);
                branches
            }
            None => AiMessageBranches {
                total: 2,
                active_index: 1,
                tails: HashMap::from([(0, current_tail)]),
            },
        };
        let context = original.context.take();
        conversation.messages.truncate(message_index);
        conversation.messages.push(AiChatMessage {
            id: new_user_id.clone(),
            role: AiChatRole::User,
            content: edited_content,
            timestamp_ms: now_ms,
            model: Some(model),
            context,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: Some(branches),
            suggestions: Vec::new(),
        });
        // Editing starts a new turn, so the replacement must carry the backend
        // owner selected for that turn instead of inheriting the removed branch.
        oxideterm_ai::store_ai_message_backend_provenance(
            conversation,
            &new_user_id,
            backend,
        );
        conversation.message_count = conversation.messages.len();
        conversation.updated_at_ms = now_ms;
        let conversation_id = conversation.id.clone();
        self.persist_chat_state();
        Some(conversation_id)
    }

    fn switch_active_message_branch(
        &mut self,
        message_id: &str,
        branch_index: usize,
        now_ms: i64,
    ) -> bool {
        let Some(conversation) = self.conversation_state_mut().active_conversation_mut() else {
            return false;
        };
        let Some(message_index) = conversation
            .messages
            .iter()
            .position(|message| message.id == message_id)
        else {
            return false;
        };
        let Some(branches) = conversation.messages[message_index].branches.as_ref() else {
            return false;
        };
        if branch_index >= branches.total || branch_index == branches.active_index {
            return false;
        }
        if !branches
            .tails
            .get(&branch_index)
            .is_some_and(|target_tail| !target_tail.is_empty())
        {
            return false;
        }
        let live_tail = strip_ai_nested_branches(&conversation.messages[message_index..]);
        // This branch payload belongs to the message being replaced, so move it
        // instead of cloning every stored branch and message tail.
        let mut branches = conversation.messages[message_index]
            .branches
            .take()
            .expect("validated branch metadata must remain present");
        let target_tail = branches
            .tails
            .remove(&branch_index)
            .expect("validated branch tail must remain present");
        branches.tails.insert(branches.active_index, live_tail);
        branches.active_index = branch_index;
        let mut new_messages = conversation.messages[..message_index].to_vec();
        let mut updated_branches = Some(branches);
        for (index, mut message) in target_tail.into_iter().enumerate() {
            message.branches = if index == 0 {
                updated_branches.take()
            } else {
                None
            };
            new_messages.push(message);
        }
        conversation.messages = new_messages;
        conversation.message_count = conversation.messages.len();
        conversation.updated_at_ms = now_ms;
        self.persist_chat_state();
        true
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn open_ai_safety_confirm(&mut self, cx: &mut Context<Self>) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.open_standard_chat_confirm(AiStandardConfirmKind::Safety);
        });
        // Pointer-opened confirmations do not show keyboard focus until navigation starts.
        self.clear_standard_confirm_focus();
        cx.notify();
    }

    pub(in crate::workspace) fn begin_ai_safety_confirm_exit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self
            .ai_entity
            .update(cx, |ai, _cx| {
                ai.begin_standard_chat_confirm_exit(AiStandardConfirmKind::Safety)
            })
        else {
            return false;
        };
        self.clear_standard_confirm_focus();
        self.schedule_ai_standard_confirm_exit(AiStandardConfirmKind::Safety, generation, cx);
        true
    }

    pub(in crate::workspace) fn open_ai_summarize_confirm(&mut self, cx: &mut Context<Self>) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.open_standard_chat_confirm(AiStandardConfirmKind::Summarize);
        });
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(in crate::workspace) fn begin_ai_summarize_confirm_exit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(generation) = self
            .ai_entity
            .update(cx, |ai, _cx| {
                ai.begin_standard_chat_confirm_exit(AiStandardConfirmKind::Summarize)
            })
        else {
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
            self.finish_ai_standard_confirm_exit(kind, generation, cx);
            return;
        }
        // The open flag remains set until this generation's exit frame completes.
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.finish_ai_standard_confirm_exit(kind, generation, cx) {
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
        cx: &mut Context<Self>,
    ) -> bool {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.finish_standard_chat_confirm_exit(kind, generation)
        })
    }

    pub(in crate::workspace) fn create_ai_sidebar_conversation(
        &mut self,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) -> String {
        self.ensure_ai_chat_initialized(cx);
        let now = ai_now_ms();
        let id = self.next_ai_chat_id(now, cx);
        let id = self
            .ai_entity
            .update(cx, |ai, _cx| ai.create_chat_conversation(id, title, now));
        self.ai_entity.update(cx, |ai, _cx| {
            ai.reset_chat_for_new_conversation();
        });
        cx.notify();
        id
    }

    pub(in crate::workspace) fn send_ai_chat_draft(&mut self, cx: &mut Context<Self>) {
        self.ensure_ai_chat_initialized(cx);
        let content = self.ai_entity.read(cx).chat_ui().draft.trim().to_string();
        if content.is_empty() {
            cx.notify();
            return;
        }
        if !self.settings_store.settings().ai.enabled {
            self.push_ai_settings_toast(
                self.i18n.t("ai.chat.disabled_message"),
                TerminalNoticeVariant::Warning,
                cx,
            );
            cx.notify();
            return;
        }
        self.bootstrap_ai_mcp_registry(cx);

        let parsed_input = parse_ai_user_input(&content);
        let detected_intent = detect_ai_intent(&parsed_input);
        let sidebar_context = self.resolve_ai_sidebar_context_block(cx);
        let selected_context = self.resolve_ai_selected_terminal_context(cx);
        let reference_context = self.resolve_ai_reference_context(&parsed_input.references, cx);
        let context = ai_chat_message_context([
            selected_context,
            sidebar_context,
            reference_context,
        ]);
        let slash_command = parsed_input
            .slash_command
            .as_deref()
            .and_then(resolve_ai_slash_command);
        let explicit_skill = if slash_command.is_none()
            && self.settings_store.settings().ai.skills.enabled
        {
            parsed_input.slash_command.as_deref().and_then(|skill_id| {
                let registry = self.skill_registry.read();
                let record = registry.enabled_record(skill_id)?;
                let instructions = registry.load(skill_id).ok()?;
                Some((
                    skill_id.to_string(),
                    record.content_hash.clone(),
                    instructions,
                ))
            })
        } else {
            None
        };
        if let Some(command) = slash_command.filter(|command| command.client_only) {
            match command.name {
                "clear" => {
                    self.create_ai_sidebar_conversation(None, cx);
                    self.reset_ai_chat_input_after_submit(cx);
                    cx.notify();
                    return;
                }
                "help" => {
                    self.add_ai_help_response(content, cx);
                    return;
                }
                "compact" => {
                    self.start_ai_compact_conversation(cx);
                    self.reset_ai_chat_input_after_submit(cx);
                    cx.notify();
                    return;
                }
                _ => return,
            }
        }

        let stream_config = match self.resolve_ai_stream_config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                cx.notify();
                return;
            }
        };
        self.record_ai_memory_usage(&stream_config.memory_entry_ids, cx);
        let now = ai_now_ms();
        let title = generate_chat_title(&content);
        let id = self.next_ai_chat_id(now, cx);
        let message = AiChatMessage {
            id: self.next_ai_chat_id(now, cx),
            role: AiChatRole::User,
            content: content.clone(),
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
        let runtime_system_prompt = self.resolve_ai_sidebar_system_prompt_segment(cx);
        let explicit_skill_prompt = explicit_skill.as_ref().map(
            |(skill_id, _content_hash, instructions)| {
                format!(
                    "## Explicit Agent Skill\nThe user explicitly selected `{skill_id}`. Follow these instructions for this request, subject to the current tool permissions and safety mode.\n\n<skill_instructions id=\"{skill_id}\">\n{}\n</skill_instructions>",
                    oxideterm_ai::sanitize_for_ai(instructions)
                )
            },
        );
        let task_system_prompt = ai_chat_message_context([
            ai_input_system_prompt(slash_command, &parsed_input.participants),
            explicit_skill_prompt,
            runtime_system_prompt,
            ai_detected_intent_system_prompt(&detected_intent),
        ]);
        let active_participant = parsed_input
            .participants
            .first()
            .map(|participant| participant.name.clone());
        let request_text = if stream_config.execution_backend == AiExecutionBackend::Acp
            && parsed_input.slash_command.is_some()
            && slash_command.is_none()
            && explicit_skill.is_none()
        {
            // Agent-advertised ACP commands are protocol input, not OxideTerm
            // system prompts. Preserve the slash prefix for the agent.
            content
        } else {
            parsed_input.clean_text
        };
        let request_content = if request_text.is_empty() && explicit_skill.is_some() {
            Some("Apply the explicitly selected Agent Skill to the current task.".to_string())
        } else {
            (!request_text.is_empty()).then_some(request_text)
        };
        let conversation_id = self.ai_entity.update(cx, |ai, _cx| {
            ai.begin_sidebar_user_turn(
                id,
                title,
                now,
                message,
                &stream_config,
                active_participant,
            )
        });
        if let Some((skill_id, content_hash, _instructions)) = explicit_skill {
            self.record_ai_loaded_skill(&conversation_id, &skill_id, &content_hash, cx);
        }
        // Sending a new turn is an explicit request to resume following the
        // conversation tail; manual upward scrolling can pause it again.
        self.ai_entity.read(cx).chat_ui().message_list_state
            .set_follow_mode(FollowMode::Tail);
        self.start_ai_chat_stream_after_api_key_lookup(
            conversation_id,
            stream_config,
            request_content,
            task_system_prompt,
            cx,
        );
        self.reset_ai_chat_input_after_submit(cx);
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
        if stream_config.execution_backend == AiExecutionBackend::Acp {
            // ACP is a session protocol, not a provider completion backend.
            // It owns history and authentication after connection negotiation.
            self.start_acp_chat_thread(
                conversation_id,
                stream_config,
                request_content,
                task_system_prompt,
                cx,
            );
            return;
        }
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
        let key_store = self.ai_entity.read(cx).key_store().clone();
        let runtime = self.forwarding_runtime.clone();
        let failed_to_get_key = self.i18n.t("ai.model_selector.failed_to_get_api_key");
        let api_key_not_found = self.i18n.t("ai.model_selector.api_key_not_found");
        self.ai_entity.update(cx, |ai, _cx| ai.set_chat_loading(true));
        cx.spawn(async move |weak, cx| {
            let key_result = runtime
                .spawn_blocking(move || key_store.get_provider_key(&provider_id))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| match key_result {
                Ok(api_key) => {
                    if requires_key && api_key.is_none() {
                        this.ai_entity.update(cx, |ai, _cx| ai.set_chat_loading(false));
                        this.push_ai_settings_toast(
                            api_key_not_found,
                            TerminalNoticeVariant::Error,
                            cx,
                        );
                        cx.notify();
                        return;
                    }
                    stream_config.api_key =
                        api_key.map(oxideterm_ai::SharedAiProviderKey::new);
                    this.start_ai_chat_stream_after_rag_lookup(
                        conversation_id,
                        stream_config,
                        request_content,
                        task_system_prompt,
                        cx,
                    );
                }
                Err(_) if requires_key => {
                    this.ai_entity.update(cx, |ai, _cx| ai.set_chat_loading(false));
                    this.push_ai_settings_toast(failed_to_get_key, TerminalNoticeVariant::Error, cx);
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
        let id = self.next_ai_chat_id(now, cx);
        let user_message = AiChatMessage {
            id: self.next_ai_chat_id(now, cx),
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
            id: self.next_ai_chat_id(now, cx),
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
        self.ai_entity.update(cx, |ai, _cx| {
            ai.add_help_exchange(id, title, now, user_message, assistant_message);
        });
        self.reset_ai_chat_input_after_submit(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn send_ai_follow_up_suggestion(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_chat_draft(text);
        });
        self.send_ai_chat_draft(cx);
    }

    pub(in crate::workspace) fn regenerate_ai_last_response(&mut self, cx: &mut Context<Self>) {
        if self.ai_entity.read(cx).chat_is_loading() {
            cx.notify();
            return;
        }
        let stream_config = match self.resolve_ai_stream_config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                cx.notify();
                return;
            }
        };
        self.record_ai_memory_usage(&stream_config.memory_entry_ids, cx);
        let conversation_id = self.ai_entity.update(cx, |ai, _cx| {
            ai.truncate_active_conversation_after_last_user(ai_now_ms())
        });
        let Some(conversation_id) = conversation_id else {
            return;
        };
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
        if self.ai_entity.read(cx).chat_is_loading() {
            cx.notify();
            return;
        }
        self.ai_entity.update(cx, |ai, cx| {
            ai.open_chat_confirm(
                ai_state::AiChatConfirmKind::DeleteMessage {
                    message_id: Arc::from(message_id),
                },
                cx,
            );
        });
    }

    pub(in crate::workspace) fn delete_ai_message(
        &mut self,
        message_id: &str,
        cx: &mut Context<Self>,
    ) {
        let deleted = self
            .ai_entity
            .update(cx, |ai, _cx| ai.delete_active_message(message_id, ai_now_ms()));
        if !deleted {
            return;
        }
        self.ai_entity.update(cx, |ai, _cx| {
            ai.remove_thinking_expansion(message_id);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn set_ai_safety_mode_default(&mut self, cx: &mut Context<Self>) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_active_conversation_safety_mode(AiSafetyMode::Default);
        });
        self.close_ai_safety_mode_menu(cx);
    }

    pub(in crate::workspace) fn set_ai_safety_mode_read_only(&mut self, cx: &mut Context<Self>) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_active_conversation_safety_mode(AiSafetyMode::ReadOnly);
        });
        self.close_ai_safety_mode_menu(cx);
    }

    fn close_ai_safety_mode_menu(&mut self, cx: &mut Context<Self>) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_chat_popover_open(AiChatPopover::Safety, false);
        });
        self.restore_ai_chat_input_focus_after_safety_mode_change(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn confirm_ai_safety_bypass(&mut self, cx: &mut Context<Self>) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_active_conversation_safety_mode(AiSafetyMode::Bypass);
        });
        self.close_ai_safety_mode_menu(cx);
    }

    pub(in crate::workspace) fn restore_ai_chat_input_focus_after_safety_mode_change(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        // Closing the safety menu returns keyboard ownership to the composer so
        // Enter/Space continue the conversation instead of falling through.
        self.ai_entity.update(cx, |ai, _cx| {
            ai.focus_chat_input();
        });
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_model_selector_search_focused(false);
        });
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn start_edit_ai_message(
        &mut self,
        message_id: String,
        content: String,
        cx: &mut Context<Self>,
    ) {
        if self.ai_entity.read(cx).chat_is_loading() {
            cx.notify();
            return;
        }
        self.ai_entity.update(cx, |ai, _cx| {
            ai.begin_message_edit(message_id, content);
        });
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_model_selector_search_focused(false);
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn cancel_edit_ai_message(&mut self, cx: &mut Context<Self>) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.clear_message_edit();
        });
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn save_ai_message_edit(&mut self, cx: &mut Context<Self>) {
        if self.ai_entity.read(cx).chat_is_loading() {
            cx.notify();
            return;
        }
        let edited_content = self.ai_entity.read(cx).chat_ui().editing_message_draft.trim().to_string();
        if edited_content.is_empty() {
            cx.notify();
            return;
        }
        let Some(message_id) = self.ai_entity.read(cx).chat_ui().editing_message_id.clone() else {
            return;
        };
        let editable = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .active_conversation()
            .and_then(|conversation| {
                conversation
                    .messages
                    .iter()
                    .find(|message| message.id == message_id)
            })
            .is_some_and(|message| message.role == AiChatRole::User);
        if !editable {
            return;
        }
        let stream_config = match self.resolve_ai_stream_config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error, cx);
                cx.notify();
                return;
            }
        };
        self.record_ai_memory_usage(&stream_config.memory_entry_ids, cx);

        let now = ai_now_ms();
        let new_user_id = self.next_ai_chat_id(now, cx);
        let request_content = Some(edited_content.clone());
        let backend = ai_message_backend_for_stream(&stream_config);
        let conversation_id = self.ai_entity.update(cx, |ai, _cx| {
            ai.replace_active_user_message(
                &message_id,
                new_user_id,
                edited_content,
                stream_config.model.clone(),
                backend,
                now,
            )
        });
        let Some(conversation_id) = conversation_id else {
            return;
        };
        self.ai_entity.update(cx, |ai, _cx| {
            ai.clear_message_edit();
        });
        self.ime_marked_text = None;
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
        if self.ai_entity.read(cx).chat_is_loading() {
            cx.notify();
            return;
        }
        let switched = self.ai_entity.update(cx, |ai, _cx| {
            ai.switch_active_message_branch(&message_id, branch_index, ai_now_ms())
        });
        if !switched {
            return;
        }
        self.ai_entity.update(cx, |ai, _cx| {
            ai.clear_message_edit();
        });
        cx.notify();
    }

    pub(in crate::workspace) fn reset_ai_chat_input_after_submit(&mut self, cx: &mut App) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.clear_chat_input_after_submit();
        });
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
