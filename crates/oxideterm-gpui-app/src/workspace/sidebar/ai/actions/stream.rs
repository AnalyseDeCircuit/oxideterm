impl WorkspaceApp {
    fn start_ai_chat_stream(
        &mut self,
        conversation_id: String,
        config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.start_ai_chat_stream_after_budget_preflight(
            conversation_id,
            config,
            request_content,
            task_system_prompt,
            true,
            cx,
        );
    }

    fn start_ai_chat_stream_after_budget_preflight(
        &mut self,
        conversation_id: String,
        config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        allow_pre_send_compaction: bool,
        cx: &mut Context<Self>,
    ) {
        if allow_pre_send_compaction
            && self.should_force_ai_pre_send_compaction(&conversation_id, &config)
        {
            let pending = AiPendingChatStream {
                conversation_id: conversation_id.clone(),
                config,
                request_content,
                task_system_prompt,
            };
            if self.start_ai_compact_conversation_for(
                conversation_id,
                true,
                true,
                Some(pending.clone()),
                cx,
            )
            {
                return;
            }

            return self.start_ai_chat_stream_after_budget_preflight(
                pending.conversation_id,
                pending.config,
                pending.request_content,
                pending.task_system_prompt,
                false,
                cx,
            );
        }

        let rag_query = if self.resolved_ai_execution_profile().include_rag {
            request_content.clone()
        } else {
            None
        };
        let Some((history, trimmed_count)) = self.build_ai_stream_history(
            &conversation_id,
            &config,
            request_content.clone(),
            task_system_prompt,
        ) else {
            return;
        };
        if trimmed_count > 0 {
            self.show_ai_trim_notice(trimmed_count, cx);
        }
        let now = ai_now_ms();
        let assistant_id = self.next_ai_chat_id(now);
        let request_message = self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(|conversation| {
                conversation
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == AiChatRole::User)
                    .cloned()
            });
        let request_message_id = request_message
            .as_ref()
            .map(|message| message.id.clone())
            .unwrap_or_else(|| format!("{assistant_id}-request"));
        let budget_decision = self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(|conversation| self.ai_send_budget_decision(conversation, &config));
        self.ai_chat.add_message(
            &conversation_id,
            AiChatMessage {
                id: assistant_id.clone(),
                role: AiChatRole::Assistant,
                content: String::new(),
                timestamp_ms: now,
                model: Some(config.model.clone()),
                context: None,
                is_streaming: true,
                thinking_content: None,
                metadata: None,
                tool_call_id: None,
                tool_calls: Vec::new(),
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            },
        );
        if let Some(conversation) = self
            .ai_chat
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        {
            let metadata = conversation
                .session_metadata
                .get_or_insert_with(|| serde_json::json!({ "conversationId": conversation_id }));
            if let Some(object) = metadata.as_object_mut() {
                object.insert("conversationId".to_string(), serde_json::json!(conversation_id));
                object.insert("origin".to_string(), serde_json::json!("sidebar"));
                object.insert(
                    "lastBudgetLevel".to_string(),
                    serde_json::json!(budget_decision.map(|decision| decision.level).unwrap_or(0)),
                );
            }
        }
        let mut transcript_entries = Vec::new();
        let mut diagnostic_events = Vec::new();
        if let Some(request_message) = request_message.as_ref() {
            transcript_entries.push(ai_transcript_entry(
                format!("transcript-user-{}", request_message.id),
                &conversation_id,
                "user_message",
                serde_json::json!({
                    "messageId": request_message.id,
                    "role": "user",
                    "content": request_content.as_deref().unwrap_or(&request_message.content),
                    "hasContext": request_message.context.as_ref().is_some_and(|context| !context.is_empty()),
                }),
                None,
                None,
                request_message.timestamp_ms,
            ));
            diagnostic_events.push(ai_diagnostic_event(
                format!("diagnostic-user-{}", request_message.id),
                &conversation_id,
                "user_message",
                None,
                None,
                request_message.timestamp_ms,
                self.ai_diagnostic_base(serde_json::json!({
                    "messageId": request_message.id,
                    "role": "user",
                    "contentLength": request_content.as_deref().unwrap_or(&request_message.content).len(),
                    "hasContext": request_message.context.as_ref().is_some_and(|context| !context.is_empty()),
                })),
            ));
        }
        transcript_entries.push(ai_transcript_entry(
            format!("transcript-assistant-start-{assistant_id}"),
            &conversation_id,
            "assistant_turn_start",
            serde_json::json!({
                "messageId": assistant_id,
                "requestMessageId": request_message_id,
                "conversationTurnId": assistant_id,
            }),
            Some(assistant_id.clone()),
            Some(request_message_id.clone()),
            now,
        ));
        diagnostic_events.push(ai_diagnostic_event(
            format!("diagnostic-budget-{assistant_id}"),
            &conversation_id,
            "budget_level_changed",
            Some(assistant_id.clone()),
            None,
            now,
            self.ai_diagnostic_base(serde_json::json!({
                "nextLevel": budget_decision.map(|decision| decision.level).unwrap_or(0),
                "contextWindow": self.ai_active_model_context_window(&config),
                "responseReserve": config.max_response_tokens,
                "trimmedCount": trimmed_count,
            })),
        ));
        self.persist_ai_transcript_entries(conversation_id.clone(), transcript_entries);
        self.persist_ai_diagnostic_events(conversation_id.clone(), diagnostic_events);
        self.ai_chat_loading = true;
        self.ai_chat_stream_generation = self.ai_chat_stream_generation.saturating_add(1);
        let generation = self.ai_chat_stream_generation;
        let (ui_tx, ui_rx) = std::sync::mpsc::channel();
        if let Some(task) = self.ai_chat_stream_task.take() {
            task.abort();
        }
        let snapshot = self.ai_chat_orchestrator_snapshot(&config, cx);
        self.ai_chat_stream_rx = Some(ui_rx);
        self.ai_chat_stream_task = Some(
            self.forwarding_runtime
                .spawn(run_ai_chat_tool_loop(
                    config,
                    history,
                    snapshot,
                    rag_query,
                    generation,
                    conversation_id,
                    assistant_id,
                    ui_tx,
                )),
        );
        self.schedule_ai_chat_stream_poll(cx);
    }

    fn should_force_ai_pre_send_compaction(
        &self,
        conversation_id: &str,
        config: &AiChatStreamConfig,
    ) -> bool {
        let Some(conversation) = self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return false;
        };
        let Some(decision) = self.ai_send_budget_decision(conversation, config) else {
            return false;
        };
        decision.level >= 2 && ai_find_prompt_transcript_lookup_reference(&conversation.messages).is_none()
    }

    fn resolve_ai_stream_config(&self) -> Result<AiChatStreamConfig, String> {
        let settings = self.settings_store.settings();
        let providers = ai_provider_views(&settings.ai.providers);
        let applied_profile = self.resolved_ai_execution_profile();
        let provider = active_provider_view(&providers, applied_profile.provider_id.as_deref())
            .cloned()
            .ok_or_else(|| self.i18n.t("ai.model_selector.no_provider"))?;
        let model = active_model_or_provider_default(applied_profile.model.as_deref(), &provider)
            .ok_or_else(|| "No model selected. Please refresh models or select one in Settings > AI.".to_string())?;
        let requires_key = ai_provider_chat_requires_key(&provider.provider_type);
        let api_key = match self.ai_key_store.get_provider_key(&provider.id) {
            Ok(key) => key,
            Err(_) if requires_key => {
                return Err(self.i18n.t("ai.model_selector.failed_to_get_api_key"));
            }
            Err(_) => None,
        };
        if requires_key && api_key.is_none() {
            return Err(self.i18n.t("ai.model_selector.api_key_not_found"));
        }
        let max_response_tokens =
            ai_model_max_response_tokens(&settings.ai.model_max_response_tokens, &provider.id, &model);
        let reasoning_effort = oxideterm_ai::resolve_ai_reasoning_effort(
            applied_profile.reasoning_effort.as_deref(),
            &settings.ai.reasoning_provider_overrides,
            &settings.ai.reasoning_model_overrides,
            Some(&provider.id),
            Some(&model),
        );
        let tool_use_enabled = applied_profile.tool_policy.enabled;
        let tools = if tool_use_enabled {
            let mut tools = oxideterm_ai::orchestrator_tool_definitions();
            tools.extend(self.ai_mcp_registry.tool_definitions());
            tools.retain(|tool| !applied_profile.tool_policy.disabled_tools.contains(&tool.name));
            tools
        } else {
            Vec::new()
        };
        Ok(AiChatStreamConfig {
            provider_id: Some(provider.id),
            provider_type: provider.provider_type,
            base_url: provider.base_url,
            model,
            api_key,
            max_response_tokens,
            reasoning_effort: Some(reasoning_effort),
            safety_mode: match self.active_ai_safety_mode() {
                AiSafetyMode::Bypass => AiPolicySafetyMode::Bypass,
                AiSafetyMode::Default => AiPolicySafetyMode::Default,
            },
            profile_id: applied_profile.profile_id,
            tool_policy: applied_profile.tool_policy,
            tools,
            tool_choice: oxideterm_ai::AiToolChoice::Auto,
        })
    }

    fn resolved_ai_execution_profile(&self) -> ResolvedAiExecutionProfile {
        let settings = self.settings_store.settings();
        resolve_ai_execution_profile(
            &settings.ai.execution_profiles,
            self.active_ai_conversation_profile_id().as_deref(),
            settings.ai.active_provider_id.as_deref(),
            settings.ai.active_model.as_deref(),
            ai_reasoning_effort_value(settings.ai.reasoning_effort).as_deref(),
            ai_tool_use_policy_from_settings(&settings.ai.tool_use),
        )
    }

    fn active_ai_conversation_profile_id(&self) -> Option<String> {
        self.ai_chat.active_conversation().and_then(|conversation| {
            conversation.profile_id.clone().or_else(|| {
                conversation
                    .session_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("profileId"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
        })
    }

    fn build_ai_stream_history(
        &self,
        conversation_id: &str,
        config: &AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
    ) -> Option<(Vec<AiChatMessage>, usize)> {
        let transcript_lookup_prompt =
            self.ai_transcript_lookup_prompt_for_conversation(conversation_id, config);
        let mut history = self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .map(|conversation| conversation.messages.clone())?;
        apply_chat_request_overrides(&mut history, request_content, task_system_prompt);
        normalize_ai_stream_history_for_provider(&mut history);
        let base_system_prompt = self.build_ai_base_system_prompt(config);
        history.insert(
            0,
            AiChatMessage {
                id: "base-system".to_string(),
                role: AiChatRole::System,
                content: base_system_prompt,
                timestamp_ms: 0,
                model: None,
                context: None,
                thinking_content: None,
                is_streaming: false,
                metadata: None,
                tool_call_id: None,
                tool_calls: Vec::new(),
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            },
        );
        let context_window = self.ai_active_model_context_window(config);
        if let Some(transcript_lookup_prompt) = transcript_lookup_prompt {
            history.insert(
                1,
                AiChatMessage {
                    id: "transcript-lookup-reference".to_string(),
                    role: AiChatRole::System,
                    content: transcript_lookup_prompt,
                    timestamp_ms: 0,
                    model: None,
                    context: None,
                    thinking_content: None,
                    is_streaming: false,
                    metadata: None,
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    turn: None,
                    transcript_ref: None,
                    summary_ref: None,
                    branches: None,
                },
            );
        }
        let trimmed_count = trim_ai_stream_history_to_budget(
            &mut history,
            context_window,
            config.max_response_tokens
                .and_then(|tokens| usize::try_from(tokens).ok())
                .filter(|tokens| *tokens > 0)
                .unwrap_or_else(|| ai_response_reserve(context_window)),
        );
        Some((history, trimmed_count))
    }

    fn ai_send_budget_decision(
        &self,
        conversation: &AiConversation,
        config: &AiChatStreamConfig,
    ) -> Option<AiPromptBudgetDecision> {
        let context_window = self.ai_active_model_context_window(config);
        let response_reserve = config
            .max_response_tokens
            .and_then(|tokens| usize::try_from(tokens).ok())
            .filter(|tokens| *tokens > 0)
            .unwrap_or_else(|| ai_response_reserve(context_window));
        let base_system_tokens = ai_estimated_tokens(&self.build_ai_base_system_prompt(config))
            .saturating_add(ai_tool_definitions_estimated_tokens(&config.tools));
        let anchor_tokens = conversation
            .messages
            .iter()
            .filter(|message| is_ai_compaction_anchor(message))
            .map(ai_message_estimated_tokens)
            .sum::<usize>();
        let regular_messages = conversation
            .messages
            .iter()
            .filter(|message| !is_ai_compaction_anchor(message))
            .collect::<Vec<_>>();
        let history_tokens = regular_messages
            .iter()
            .map(|message| ai_message_estimated_tokens(message))
            .sum::<usize>();
        let summary_eligible_tokens = ai_summary_eligible_tokens(&regular_messages);
        Some(determine_ai_compression_level(AiPromptBudgetInput {
            context_window,
            response_reserve,
            system_budget: base_system_tokens.saturating_add(anchor_tokens),
            history_tokens,
            trimmable_history_tokens: Some(history_tokens),
            summary_eligible_tokens: Some(summary_eligible_tokens),
            can_summarize: summary_eligible_tokens > 0,
            can_lookup_transcript: ai_find_prompt_transcript_lookup_reference(&conversation.messages)
                .is_some(),
            in_tool_loop: false,
            auto_compact_threshold: None,
            transcript_lookup_threshold: None,
            tool_loop_stop_threshold: None,
            safety_margin: None,
        }))
    }

    fn ai_transcript_lookup_prompt_for_conversation(
        &self,
        conversation_id: &str,
        config: &AiChatStreamConfig,
    ) -> Option<String> {
        let conversation = self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)?;
        let decision = self.ai_send_budget_decision(&conversation, config)?;
        (decision.level >= 3)
            .then(|| ai_find_prompt_transcript_lookup_reference(&conversation.messages))
            .flatten()
            .map(ai_build_transcript_lookup_prompt_reference)
    }

    fn show_ai_trim_notice(&mut self, count: usize, cx: &mut Context<Self>) {
        self.ai_context_trim_notice_count = Some(count);
        self.ai_context_trim_notice_sequence =
            self.ai_context_trim_notice_sequence.saturating_add(1);
        let sequence = self.ai_context_trim_notice_sequence;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_secs(5)).await;
            let _ = weak.update(cx, |this, cx| {
                if this.ai_context_trim_notice_sequence == sequence {
                    this.ai_context_trim_notice_count = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn persist_ai_transcript_entries(
        &self,
        conversation_id: String,
        entries: Vec<oxideterm_ai::PersistedTranscriptEntry>,
    ) {
        if entries.is_empty() {
            return;
        }
        let store = self.ai_chat_store.clone();
        self.forwarding_runtime.spawn_blocking(move || {
            if let Err(error) = store.append_transcript_entries(&conversation_id, &entries) {
                eprintln!("[AiChatStore] Failed to persist transcript entries: {error}");
            }
        });
    }

    fn persist_ai_diagnostic_events(
        &self,
        conversation_id: String,
        events: Vec<oxideterm_ai::PersistedDiagnosticEvent>,
    ) {
        if events.is_empty() {
            return;
        }
        let store = self.ai_chat_store.clone();
        self.forwarding_runtime.spawn_blocking(move || {
            if let Err(error) = store.append_diagnostic_events(&conversation_id, &events) {
                eprintln!("[AiChatStore] Failed to persist diagnostic events: {error}");
            }
        });
    }

    fn ai_diagnostic_base(&self, data: serde_json::Value) -> serde_json::Value {
        let mut object = match data {
            serde_json::Value::Object(object) => object,
            other => {
                let mut object = serde_json::Map::new();
                object.insert("value".to_string(), other);
                object
            }
        };
        object.insert("source".to_string(), serde_json::json!("sidebar"));
        object.insert(
            "toolUseEnabled".to_string(),
            serde_json::json!(self.resolved_ai_execution_profile().tool_policy.enabled),
        );
        if let Some(provider_id) = self.settings_store.settings().ai.active_provider_id.as_ref() {
            object.insert("providerId".to_string(), serde_json::json!(provider_id));
        }
        if let Some(model) = self.settings_store.settings().ai.active_model.as_ref() {
            object.insert("model".to_string(), serde_json::json!(model));
        }
        serde_json::Value::Object(object)
    }

    fn build_ai_base_system_prompt(&self, config: &AiChatStreamConfig) -> String {
        let settings = self.settings_store.settings();
        let providers = ai_provider_views(&settings.ai.providers);
        let provider = active_provider_view(&providers, config.provider_id.as_deref());
        let provider_label = provider
            .map(|provider| provider.name.as_str())
            .filter(|label| !label.trim().is_empty())
            .unwrap_or(config.provider_type.as_str());
        let mut prompt = settings.ai.custom_system_prompt.trim().to_string();
        if prompt.is_empty() {
            prompt = DEFAULT_AI_SYSTEM_PROMPT.to_string();
        }
        prompt.push_str(&format!(
            "\nYou are currently the model \"{}\", provided by {}.",
            config.model, provider_label
        ));
        let applied_profile = self.resolved_ai_execution_profile();
        if let Some(memory) = ai_user_memory_prompt(
            &settings.ai.memory.content,
            settings.ai.memory.enabled && applied_profile.include_memory,
        ) {
            prompt.push_str("\n\n");
            prompt.push_str(&memory);
        }
        if self.ai_active_model_context_window(config) >= 8192 {
            prompt.push_str(AI_SUGGESTIONS_INSTRUCTION);
        }
        prompt.push_str("\n\n");
        prompt.push_str(&ai_orchestrator_system_prompt(config.tool_policy.enabled));
        prompt
    }

    fn apply_ai_stream_event(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        event: AiStreamEvent,
        cx: &mut Context<Self>,
    ) {
        if self.ai_chat_stream_generation != generation {
            return;
        }
        match event {
            AiStreamEvent::Content(chunk) => {
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        message.content.push_str(&chunk);
                        append_ai_turn_text_part(message, "text", &chunk, false);
                    });
            }
            AiStreamEvent::Thinking(chunk) => {
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        message
                            .thinking_content
                            .get_or_insert_with(String::new)
                            .push_str(&chunk);
                        append_ai_turn_text_part(message, "thinking", &chunk, true);
                    });
            }
            AiStreamEvent::ToolCall {
                id,
                name,
                arguments,
            } => {
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        upsert_ai_tool_call(message, &id, &name, &arguments, "running");
                        upsert_ai_turn_tool_call(message, &id, &name, &arguments, "partial");
                    });
            }
            AiStreamEvent::ToolCallComplete {
                id,
                name,
                arguments,
            } => {
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        upsert_ai_tool_call(message, &id, &name, &arguments, "pending");
                        upsert_ai_turn_tool_call(message, &id, &name, &arguments, "complete");
                    });
            }
            AiStreamEvent::Done => {
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        message.is_streaming = false;
                        set_ai_turn_status(message, "complete");
                    });
                self.persist_ai_assistant_turn_end(conversation_id, message_id, "complete");
                self.ai_chat_stream_task = None;
                self.ai_chat_loading = false;
                self.persist_ai_chat_state();
                self.maybe_start_ai_auto_compaction(conversation_id, cx);
            }
            AiStreamEvent::Error(error) => {
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        message.is_streaming = false;
                        if message.content.is_empty() {
                            message.content = error.clone();
                        } else {
                            message.content.push_str("\n\n");
                            message.content.push_str(&error);
                        }
                        append_ai_turn_error_part(message, &error);
                        set_ai_turn_status(message, "error");
                    });
                self.persist_ai_assistant_turn_end(conversation_id, message_id, "error");
                self.persist_ai_diagnostic_events(
                    conversation_id.to_string(),
                    vec![ai_diagnostic_event(
                        format!("diagnostic-error-{message_id}-{}", ai_now_ms()),
                        conversation_id,
                        "error",
                        Some(message_id.to_string()),
                        None,
                        ai_now_ms(),
                        self.ai_diagnostic_base(serde_json::json!({
                            "requestKind": "chat",
                            "message": error,
                        })),
                    )],
                );
                self.ai_chat_stream_task = None;
                self.ai_chat_loading = false;
                self.persist_ai_chat_state();
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
            }
        }
        cx.notify();
    }

    fn apply_ai_round_summary(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        round_id: &str,
        text: &str,
        metadata: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        if self.ai_chat_stream_generation != generation {
            return;
        }
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        self.ai_chat
            .update_message(conversation_id, message_id, |message| {
                upsert_ai_round_summary(message, round_id, text, metadata.clone());
            });

        let now = ai_now_ms();
        let mut payload = serde_json::json!({
            "messageId": message_id,
            "summaryText": text,
            "summaryKind": "round",
            "roundId": round_id,
        });
        if let Some(payload_object) = payload.as_object_mut()
            && let Some(metadata_object) = metadata.as_object()
        {
            for key in [
                "source",
                "model",
                "summarizationMode",
                "durationMs",
                "contextLengthBefore",
                "numRounds",
                "numRoundsSinceLastSummarization",
                "usage",
            ] {
                if let Some(value) = metadata_object.get(key) {
                    payload_object.insert(key.to_string(), value.clone());
                }
            }
        }

        self.persist_ai_transcript_entries(
            conversation_id.to_string(),
            vec![ai_transcript_entry(
                format!("transcript-summary-created-{message_id}-{round_id}"),
                conversation_id,
                "summary_created",
                payload,
                Some(message_id.to_string()),
                Some(round_id.to_string()),
                now,
            )],
        );
        self.persist_ai_chat_state();
        cx.notify();
    }

    fn apply_ai_round_stateful_marker(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        round_id: &str,
        marker: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.ai_chat_stream_generation != generation {
            return;
        }
        self.ai_chat
            .update_message(conversation_id, message_id, |message| {
                set_ai_turn_round_stateful_marker(message, round_id, marker.as_deref());
            });
        self.persist_ai_chat_state();
        cx.notify();
    }

    fn persist_ai_stream_diagnostic(
        &self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        event_type: &str,
        round_id: Option<String>,
        data: serde_json::Value,
    ) {
        if self.ai_chat_stream_generation != generation {
            return;
        }
        let now = ai_now_ms();
        self.persist_ai_diagnostic_events(
            conversation_id.to_string(),
            vec![ai_diagnostic_event(
                format!("diagnostic-{event_type}-{message_id}-{now}"),
                conversation_id,
                event_type,
                Some(message_id.to_string()),
                round_id,
                now,
                self.ai_diagnostic_base(data),
            )],
        );
    }

    fn apply_ai_tool_status(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        tool_call_id: &str,
        name: &str,
        arguments: &str,
        status: &str,
        result: Option<serde_json::Value>,
        risk: Option<String>,
        summary: Option<String>,
        synthetic_denied: bool,
        raw_text: Option<String>,
        round_id_override: Option<String>,
        round_number_override: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        if self.ai_chat_stream_generation != generation {
            return;
        }
        let should_persist = result.is_some()
            || matches!(
                status,
                "pending_user_approval" | "rejected" | "completed" | "error"
            );
        let mut round_id = None;
        let mut round_number = None;
        self.ai_chat
            .update_message(conversation_id, message_id, |message| {
                update_ai_tool_call_status(
                    message,
                    tool_call_id,
                    name,
                    arguments,
                    status,
                    result.clone(),
                    risk,
                    summary,
                    round_id_override.as_deref(),
                    round_number_override,
                );
                let (id, number) =
                    ai_turn_round_for_tool_call_with_override(message, tool_call_id, round_id_override.as_deref(), round_number_override);
                round_id = Some(id);
                round_number = Some(number);
            });
        if should_persist {
            let now = ai_now_ms();
            let round_id_value = round_id.clone();
            let round_number_value = round_number.unwrap_or(1);
            let mut transcript_entries = Vec::new();
            let mut diagnostic_events = Vec::new();
            if synthetic_denied || matches!(status, "pending" | "running" | "pending_user_approval") {
                let mut call_payload = serde_json::json!({
                    "id": tool_call_id,
                    "name": name,
                    "argumentsText": arguments,
                    "roundId": round_id_value,
                });
                if let Some(object) = call_payload.as_object_mut()
                    && synthetic_denied
                {
                    object.insert("syntheticDenied".to_string(), serde_json::json!(true));
                }
                transcript_entries.push(ai_transcript_entry(
                    format!("transcript-tool-call-{tool_call_id}"),
                    conversation_id,
                    "tool_call",
                    call_payload,
                    Some(message_id.to_string()),
                    round_id.clone(),
                    now,
                ));
                diagnostic_events.push(ai_diagnostic_event(
                    format!("diagnostic-tool-call-{tool_call_id}"),
                    conversation_id,
                    "tool_call",
                    Some(message_id.to_string()),
                    round_id.clone(),
                    now,
                    self.ai_diagnostic_base(serde_json::json!({
                        "logicalRound": round_number_value,
                        "toolCallId": tool_call_id,
                        "toolName": name,
                        "arguments": arguments,
                        "syntheticDenied": synthetic_denied,
                    })),
                ));
            }
            if matches!(status, "rejected" | "completed" | "error") {
                let success = status == "completed";
                let output = result
                    .as_ref()
                    .and_then(|value| value.get("output"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let error = result
                    .as_ref()
                    .and_then(|value| value.get("error"))
                    .cloned();
                let mut result_payload = serde_json::json!({
                    "toolCallId": tool_call_id,
                    "toolName": name,
                    "success": success,
                    "output": output,
                    "error": error,
                    "roundId": round_id_value,
                });
                if let Some(object) = result_payload.as_object_mut() {
                    if synthetic_denied {
                        object.insert("syntheticDenied".to_string(), serde_json::json!(true));
                    }
                    if let Some(raw_text) = raw_text.as_deref() {
                        object.insert("rawText".to_string(), serde_json::json!(raw_text));
                    }
                }
                transcript_entries.push(ai_transcript_entry(
                    format!("transcript-tool-result-{tool_call_id}"),
                    conversation_id,
                    "tool_result",
                    result_payload,
                    Some(message_id.to_string()),
                    Some(tool_call_id.to_string()),
                    now,
                ));
                diagnostic_events.push(ai_diagnostic_event(
                    format!("diagnostic-tool-result-{tool_call_id}"),
                    conversation_id,
                    "tool_result",
                    Some(message_id.to_string()),
                    round_id,
                    now,
                    self.ai_diagnostic_base(serde_json::json!({
                        "logicalRound": round_number_value,
                        "toolCallId": tool_call_id,
                        "toolName": name,
                        "success": success,
                        "error": error,
                        "syntheticDenied": synthetic_denied,
                    })),
                ));
            }
            self.persist_ai_transcript_entries(conversation_id.to_string(), transcript_entries);
            self.persist_ai_diagnostic_events(conversation_id.to_string(), diagnostic_events);
            self.persist_ai_chat_state();
        }
        cx.notify();
    }

    fn apply_ai_guardrail(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        code: &str,
        message: &str,
        raw_text: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.ai_chat_stream_generation != generation {
            return;
        }
        self.ai_chat
            .update_message(conversation_id, message_id, |message_value| {
                append_ai_turn_guardrail_part(message_value, code, message, raw_text.as_deref());
            });
        let now = ai_now_ms();
        self.persist_ai_transcript_entries(
            conversation_id.to_string(),
            vec![ai_transcript_entry(
                format!("transcript-guardrail-{message_id}-{code}-{now}"),
                conversation_id,
                "guardrail",
                serde_json::json!({
                    "code": code,
                    "message": message,
                    "rawText": raw_text,
                }),
                Some(message_id.to_string()),
                Some(message_id.to_string()),
                now,
            )],
        );
        self.persist_ai_diagnostic_events(
            conversation_id.to_string(),
            vec![ai_diagnostic_event(
                format!("diagnostic-guardrail-{message_id}-{code}-{now}"),
                conversation_id,
                "guardrail",
                Some(message_id.to_string()),
                None,
                now,
                self.ai_diagnostic_base(serde_json::json!({
                    "requestKind": "chat",
                    "code": code,
                    "message": message,
                    "rawTextLength": raw_text.as_ref().map(|text| text.len()).unwrap_or(0),
                })),
            )],
        );
        self.persist_ai_chat_state();
        cx.notify();
    }

    fn persist_ai_assistant_turn_end(
        &self,
        conversation_id: &str,
        message_id: &str,
        status: &str,
    ) {
        let Some(message) = self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(|conversation| {
                conversation
                    .messages
                    .iter()
                    .find(|message| message.id == message_id)
            })
        else {
            return;
        };
        let parts = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        let has_parts = parts
            .as_array()
            .is_some_and(|parts| !parts.is_empty());
        let tool_round_count = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("toolRounds"))
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        let plain_text_summary =
            ai_turn_plain_text_summary(message).unwrap_or_else(|| message.content.clone());
        let now = ai_now_ms();
        let mut entries = Vec::new();
        if has_parts {
            entries.push(ai_transcript_entry(
                    format!("transcript-assistant-parts-{message_id}"),
                    conversation_id,
                    "assistant_part",
                    serde_json::json!({
                        "parts": parts,
                        "completeTurnParts": true,
                    }),
                    Some(message_id.to_string()),
                    Some(message_id.to_string()),
                    now,
                ));
        }
        entries.push(ai_transcript_entry(
            format!("transcript-assistant-end-{message_id}"),
            conversation_id,
            "assistant_turn_end",
            serde_json::json!({
                "status": status,
                "messageId": message_id,
                "plainTextSummary": plain_text_summary,
                "toolRoundCount": tool_round_count,
            }),
            Some(message_id.to_string()),
            Some(message_id.to_string()),
            now,
        ));
        self.persist_ai_transcript_entries(conversation_id.to_string(), entries);
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_ai_assistant_round(
        &self,
        conversation_id: &str,
        message_id: &str,
        round_id: String,
        round_number: i64,
        response_length: usize,
        tool_call_ids: Vec<String>,
        synthetic: bool,
        retry_attempt: Option<usize>,
        hard_deny_triggered: bool,
    ) {
        let now = ai_now_ms();
        let mut transcript_entries = Vec::new();
        if !tool_call_ids.is_empty() || synthetic {
            transcript_entries.push(ai_transcript_entry(
                format!("transcript-assistant-round-{round_id}"),
                conversation_id,
                "assistant_round",
                serde_json::json!({
                    "round": round_number,
                    "roundId": round_id,
                    "synthetic": synthetic,
                    "retryAttempt": retry_attempt,
                    "toolCallIds": tool_call_ids,
                }),
                Some(message_id.to_string()),
                Some(message_id.to_string()),
                now,
            ));
        }
        self.persist_ai_transcript_entries(conversation_id.to_string(), transcript_entries);
        self.persist_ai_diagnostic_events(
            conversation_id.to_string(),
            vec![ai_diagnostic_event(
                format!("diagnostic-assistant-round-{round_id}"),
                conversation_id,
                "assistant_round",
                Some(message_id.to_string()),
                Some(round_id.clone()),
                now,
                self.ai_diagnostic_base(serde_json::json!({
                    "logicalRound": round_number,
                    "responseLength": response_length,
                    "toolCallCount": tool_call_ids.len(),
                    "toolRoundIds": [round_id],
                    "synthetic": synthetic,
                    "retryAttempt": retry_attempt,
                    "hardDenyTriggered": hard_deny_triggered,
                })),
            )],
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_ai_summary_created(
        &self,
        conversation_id: &str,
        message_id: &str,
        summary_kind: &str,
        summary_text: &str,
        transcript_ref: Option<serde_json::Value>,
        compacted_message_count: Option<usize>,
        source: Option<&str>,
        timestamp: i64,
    ) {
        self.persist_ai_transcript_entries(
            conversation_id.to_string(),
            vec![ai_transcript_entry(
                format!("transcript-summary-created-{message_id}"),
                conversation_id,
                "summary_created",
                serde_json::json!({
                    "messageId": message_id,
                    "summaryText": summary_text,
                    "summaryKind": summary_kind,
                    "sourceStartEntryId": transcript_ref
                        .as_ref()
                        .and_then(|value| value.get("startEntryId"))
                        .and_then(serde_json::Value::as_str),
                    "sourceEndEntryId": transcript_ref
                        .as_ref()
                        .and_then(|value| value.get("endEntryId"))
                        .and_then(serde_json::Value::as_str),
                    "source": source,
                    "summarizationMode": source,
                    "compactedMessageCount": compacted_message_count,
                }),
                Some(message_id.to_string()),
                Some(message_id.to_string()),
                timestamp,
            )],
        );
        self.persist_ai_diagnostic_events(
            conversation_id.to_string(),
            vec![ai_diagnostic_event(
                format!("diagnostic-summary-created-{message_id}"),
                conversation_id,
                "compaction_completed",
                Some(message_id.to_string()),
                None,
                timestamp,
                self.ai_diagnostic_base(serde_json::json!({
                    "summaryKind": summary_kind,
                    "summaryLength": summary_text.len(),
                    "compactedMessageCount": compacted_message_count,
                    "source": source,
                })),
            )],
        );
    }

    fn start_ai_compact_conversation(&mut self, cx: &mut Context<Self>) {
        let Some(conversation_id) = self
            .ai_chat
            .active_conversation()
            .map(|conversation| conversation.id.clone())
        else {
            return;
        };
        self.start_ai_compact_conversation_for(conversation_id, false, true, None, cx);
    }

    fn maybe_start_ai_auto_compaction(&mut self, conversation_id: &str, cx: &mut Context<Self>) {
        if self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .is_none_or(|conversation| conversation.messages.len() < 6)
        {
            return;
        }
        self.start_ai_compact_conversation_for(conversation_id.to_string(), true, false, None, cx);
    }

    fn start_ai_compact_conversation_for(
        &mut self,
        conversation_id: String,
        silent: bool,
        force: bool,
        resume_after: Option<AiPendingChatStream>,
        cx: &mut Context<Self>,
    ) -> bool {
        let conversation = match self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        {
            Some(conversation) if conversation.messages.len() >= 4 => conversation.clone(),
            _ => return false,
        };
        if !self
            .ai_compacting_conversations
            .insert(conversation.id.clone())
        {
            return false;
        }

        let config = match self.resolve_ai_stream_config() {
            Ok(config) => config,
            Err(error) => {
                self.ai_compacting_conversations.remove(&conversation.id);
                if !silent {
                    self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                }
                return false;
            }
        };
        let context_window = self.ai_active_model_context_window(&config);
        if silent && !force {
            let total_tokens = conversation
                .messages
                .iter()
                .map(ai_message_estimated_tokens)
                .sum::<usize>();
            let reserve = ai_response_reserve(context_window);
            let prompt_budget = compute_ai_prompt_budget(context_window, reserve, 0, None);
            let auto_compact_threshold = if prompt_budget.usable_prompt_budget > 0 {
                (context_window as f32 * AI_COMPACTION_TRIGGER_THRESHOLD)
                    / prompt_budget.usable_prompt_budget as f32
            } else {
                AI_COMPACTION_TRIGGER_THRESHOLD
            };
            let decision = determine_ai_compression_level(AiPromptBudgetInput {
                context_window,
                response_reserve: reserve,
                system_budget: 0,
                history_tokens: total_tokens,
                trimmable_history_tokens: None,
                summary_eligible_tokens: Some(total_tokens),
                can_summarize: true,
                can_lookup_transcript: false,
                in_tool_loop: false,
                auto_compact_threshold: Some(auto_compact_threshold),
                transcript_lookup_threshold: None,
                tool_loop_stop_threshold: None,
                safety_margin: None,
            });
            if decision.level < 2 {
                self.ai_compacting_conversations.remove(&conversation.id);
                return false;
            }
        }
        let Some(plan) = ai_compaction_plan(&conversation.messages, context_window) else {
            self.ai_compacting_conversations.remove(&conversation.id);
            return false;
        };
        let summary_messages = ai_compaction_summary_messages(&plan.compact_messages);
        let conversation_id = conversation.id.clone();
        let base_ids = conversation
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (ui_tx, ui_rx) = std::sync::mpsc::channel();
        if resume_after.is_some() {
            self.ai_pending_chat_after_compaction = resume_after.clone();
        }
        self.ai_compaction_rx = Some(ui_rx);
        self.forwarding_runtime
            .spawn(stream_chat_completion(config, summary_messages, tx));
        self.forwarding_runtime.spawn(async move {
            let mut summary = String::new();
            let mut stream_error = None;
            while let Some(event) = rx.recv().await {
                match event {
                    AiStreamEvent::Content(chunk) => {
                        summary.push_str(&chunk);
                    }
                    AiStreamEvent::Thinking(_)
                    | AiStreamEvent::ToolCall { .. }
                    | AiStreamEvent::ToolCallComplete { .. } => {}
                    AiStreamEvent::Done => break,
                    AiStreamEvent::Error(error) => {
                        stream_error = Some(error);
                        break;
                    }
                }
            }
            let _ = ui_tx.send(AiCompactionDelivery {
                kind: AiCompactionDeliveryKind::Compact,
                conversation_id,
                base_ids,
                plan: Some(plan),
                summary,
                stream_error,
                resume_after,
            });
        });
        self.schedule_ai_compaction_poll(cx);
        true
    }

    fn start_ai_summarize_conversation(&mut self, cx: &mut Context<Self>) {
        let conversation = match self.ai_chat.active_conversation() {
            Some(conversation) if conversation.messages.len() >= 4 => conversation.clone(),
            _ => return,
        };
        if !self
            .ai_compacting_conversations
            .insert(conversation.id.clone())
        {
            return;
        }

        let config = match self.resolve_ai_stream_config() {
            Ok(config) => config,
            Err(error) => {
                self.ai_compacting_conversations.remove(&conversation.id);
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                return;
            }
        };
        let summary_messages = ai_conversation_summary_messages(&conversation.messages);
        let conversation_id = conversation.id.clone();
        let base_ids = conversation
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (ui_tx, ui_rx) = std::sync::mpsc::channel();
        self.ai_chat_loading = true;
        self.ai_compaction_rx = Some(ui_rx);
        self.forwarding_runtime
            .spawn(stream_chat_completion(config, summary_messages, tx));
        self.forwarding_runtime.spawn(async move {
            let mut summary = String::new();
            let mut stream_error = None;
            while let Some(event) = rx.recv().await {
                match event {
                    AiStreamEvent::Content(chunk) => summary.push_str(&chunk),
                    AiStreamEvent::Thinking(_)
                    | AiStreamEvent::ToolCall { .. }
                    | AiStreamEvent::ToolCallComplete { .. } => {}
                    AiStreamEvent::Done => break,
                    AiStreamEvent::Error(error) => {
                        stream_error = Some(error);
                        break;
                    }
                }
            }
            let _ = ui_tx.send(AiCompactionDelivery {
                kind: AiCompactionDeliveryKind::Summary,
                conversation_id,
                base_ids,
                plan: None,
                summary,
                stream_error,
                resume_after: None,
            });
        });
        self.schedule_ai_compaction_poll(cx);
        cx.notify();
    }

    pub(super) fn poll_ai_chat_stream_events(
        &mut self,
        mut window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let Some(rx) = self.ai_chat_stream_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        while let Ok(delivery) = rx.try_recv() {
            let done = matches!(
                delivery.event,
                AiStreamDeliveryEvent::Stream(AiStreamEvent::Done | AiStreamEvent::Error(_))
            );
            match delivery.event {
                AiStreamDeliveryEvent::Stream(event) => {
                    self.apply_ai_stream_event(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        event,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::TrimNotice(count) => {
                    self.show_ai_trim_notice(count, cx);
                }
                AiStreamDeliveryEvent::Guardrail {
                    code,
                    message,
                    raw_text,
                } => {
                    self.apply_ai_guardrail(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &code,
                        &message,
                        raw_text,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::AssistantRound {
                    round_id,
                    round_number,
                    response_length,
                    tool_call_ids,
                    synthetic,
                    retry_attempt,
                    hard_deny_triggered,
                } => {
                    self.persist_ai_assistant_round(
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        round_id,
                        round_number,
                        response_length,
                        tool_call_ids,
                        synthetic,
                        retry_attempt,
                        hard_deny_triggered,
                    );
                }
                AiStreamDeliveryEvent::RoundSummary {
                    round_id,
                    text,
                    metadata,
                } => {
                    self.apply_ai_round_summary(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &round_id,
                        &text,
                        metadata,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::RoundStatefulMarker { round_id, marker } => {
                    self.apply_ai_round_stateful_marker(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &round_id,
                        marker,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::Diagnostic {
                    event_type,
                    round_id,
                    data,
                } => {
                    self.persist_ai_stream_diagnostic(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &event_type,
                        round_id,
                        data,
                    );
                }
                AiStreamDeliveryEvent::ToolStatus {
                    tool_call_id,
                    name,
                    arguments,
                    status,
                    result,
                    risk,
                    summary,
                    synthetic_denied,
                    raw_text,
                    round_id,
                    round_number,
                } => {
                    self.apply_ai_tool_status(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &tool_call_id,
                        &name,
                        &arguments,
                        &status,
                        result,
                        risk,
                        summary,
                        synthetic_denied,
                        raw_text,
                        round_id,
                        round_number,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::ToolApprovalRequested {
                    tool_call_id,
                    name,
                    arguments,
                    risk,
                    summary,
                    sender,
                } => {
                    self.ai_pending_tool_approvals
                        .insert(tool_call_id.clone(), sender);
                    self.apply_ai_tool_status(
                        delivery.generation,
                        &delivery.conversation_id,
                        &delivery.assistant_id,
                        &tool_call_id,
                        &name,
                        &arguments,
                        "pending_user_approval",
                        None,
                        Some(risk),
                        Some(summary),
                        false,
                        None,
                        None,
                        None,
                        cx,
                    );
                }
                AiStreamDeliveryEvent::ToolExecutionRequested {
                    tool_call_id,
                    name,
                    args,
                    sender,
                } => {
                    let Some(window) = window.as_deref_mut() else {
                        self.ai_chat_stream_rx = Some(rx);
                        self.schedule_ai_chat_stream_poll(cx);
                        cx.notify();
                        return;
                    };
                    self.start_ai_ui_orchestrator_tool_execution(
                        tool_call_id,
                        name,
                        args,
                        sender,
                        window,
                        cx,
                    );
                }
            }
            if done {
                keep_rx = false;
                break;
            }
        }
        if keep_rx {
            self.ai_chat_stream_rx = Some(rx);
        }
    }

    fn schedule_ai_chat_stream_poll(&mut self, cx: &mut Context<Self>) {
        if self.ai_chat_stream_polling {
            return;
        }
        self.ai_chat_stream_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(16)).await;
            let _ = weak.update(cx, |this, cx| {
                this.ai_chat_stream_polling = false;
                if this.ai_chat_stream_rx.is_some() {
                    cx.notify();
                    this.schedule_ai_chat_stream_poll(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn poll_ai_compaction_results(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.ai_compaction_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        while let Ok(delivery) = rx.try_recv() {
            keep_rx = false;
            match delivery.kind {
                AiCompactionDeliveryKind::Compact => {
                    if let Some(plan) = delivery.plan {
                        self.finish_ai_compaction(
                            delivery.conversation_id,
                            delivery.base_ids,
                            plan,
                            delivery.summary,
                            delivery.stream_error,
                            delivery.resume_after,
                            cx,
                        );
                    }
                }
                AiCompactionDeliveryKind::Summary => {
                    self.finish_ai_summary(
                        delivery.conversation_id,
                        delivery.base_ids,
                        delivery.summary,
                        delivery.stream_error,
                        cx,
                    );
                }
            }
        }
        if keep_rx {
            self.ai_compaction_rx = Some(rx);
        }
    }

    fn schedule_ai_compaction_poll(&mut self, cx: &mut Context<Self>) {
        if self.ai_compaction_polling {
            return;
        }
        self.ai_compaction_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(50)).await;
            let _ = weak.update(cx, |this, cx| {
                this.ai_compaction_polling = false;
                this.poll_ai_compaction_results(cx);
                if this.ai_compaction_rx.is_some() {
                    this.schedule_ai_compaction_poll(cx);
                }
            });
        })
        .detach();
    }

    fn ai_active_model_context_window(&self, config: &AiChatStreamConfig) -> usize {
        let settings = self.settings_store.settings();
        config
            .provider_id
            .as_deref()
            .and_then(|provider_id| {
                ai_context_window_from_maps(
                    &settings.ai.user_context_windows,
                    &settings.ai.model_context_windows,
                    provider_id,
                    &config.model,
                )
            })
            .unwrap_or(AI_COMPACTION_DEFAULT_CONTEXT_WINDOW)
    }

    fn finish_ai_compaction(
        &mut self,
        conversation_id: String,
        base_ids: Vec<String>,
        plan: AiCompactionPlan,
        summary: String,
        stream_error: Option<String>,
        resume_after: Option<AiPendingChatStream>,
        cx: &mut Context<Self>,
    ) {
        self.ai_compacting_conversations.remove(&conversation_id);
        if let Some(error) = stream_error {
            if resume_after.is_none() {
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
            }
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        }
        let summary = summary.trim();
        if summary.is_empty() {
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        }
        let now = ai_now_ms();
        let anchor_id = self.next_ai_chat_id(now);
        let Some(conversation) = self
            .ai_chat
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        else {
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        };
        let latest_ids = conversation
            .messages
            .iter()
            .take(base_ids.len())
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>();
        let stale = latest_ids.len() != base_ids.len()
            || latest_ids
                .iter()
                .zip(base_ids.iter())
                .any(|(latest, expected)| *latest != expected);
        if stale {
            self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
            cx.notify();
            return;
        }
        let appended = conversation
            .messages
            .iter()
            .skip(base_ids.len())
            .cloned()
            .collect::<Vec<_>>();
        let summary_source_transcript_ref =
            ai_summary_source_transcript_ref(&plan.compact_messages, &conversation_id);
        let transcript_ref = serde_json::json!({
            "conversationId": conversation_id,
            "endEntryId": anchor_id,
        });
        let summary_ref = serde_json::json!({
            "kind": "compaction",
            "transcriptRef": summary_source_transcript_ref.clone(),
        });
        let anchor = AiChatMessage {
            id: anchor_id.clone(),
            role: AiChatRole::System,
            content: summary.to_string(),
            timestamp_ms: now,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: Some(AiChatMessageMetadata {
                kind: "compaction-anchor".to_string(),
                original_count: Some(plan.compact_messages.len()),
                compacted_at_ms: Some(now),
                original_messages: Some(plan.compact_messages.clone()),
            }),
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: Some(transcript_ref),
            summary_ref: Some(summary_ref),
            branches: None,
        };
        conversation.messages = std::iter::once(anchor)
            .chain(plan.keep_messages)
            .chain(appended)
            .collect();
        conversation.updated_at_ms = now;
        self.persist_ai_chat_state();
        self.persist_ai_summary_created(
            &conversation_id,
            &anchor_id,
            "compaction",
            summary,
            Some(summary_source_transcript_ref),
            Some(plan.compact_messages.len()),
            Some(if resume_after.is_some() { "background" } else { "manual" }),
            now,
        );
        self.resume_ai_chat_after_pre_send_compaction(resume_after, cx);
        cx.notify();
    }

    fn resume_ai_chat_after_pre_send_compaction(
        &mut self,
        resume_after: Option<AiPendingChatStream>,
        cx: &mut Context<Self>,
    ) {
        let pending = resume_after.or_else(|| self.ai_pending_chat_after_compaction.take());
        let Some(pending) = pending else {
            return;
        };
        self.ai_pending_chat_after_compaction = None;
        self.start_ai_chat_stream_after_budget_preflight(
            pending.conversation_id,
            pending.config,
            pending.request_content,
            pending.task_system_prompt,
            false,
            cx,
        );
    }

    fn finish_ai_summary(
        &mut self,
        conversation_id: String,
        base_ids: Vec<String>,
        summary: String,
        stream_error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai_compacting_conversations.remove(&conversation_id);
        self.ai_chat_loading = false;
        if let Some(error) = stream_error {
            self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
            cx.notify();
            return;
        }
        let summary = summary.trim();
        if summary.is_empty() {
            cx.notify();
            return;
        }
        let now = ai_now_ms();
        let summary_id = self.next_ai_chat_id(now);
        let original_count = base_ids.len();
        let prefix = self
            .i18n
            .t("ai.context.summary_prefix")
            .replace("{{count}}", &original_count.to_string());
        let Some(conversation) = self
            .ai_chat
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        else {
            cx.notify();
            return;
        };
        let latest_ids = conversation
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>();
        let stale = latest_ids.len() != base_ids.len()
            || latest_ids
                .iter()
                .zip(base_ids.iter())
                .any(|(latest, expected)| *latest != expected);
        if stale {
            cx.notify();
            return;
        }
        conversation.messages = vec![AiChatMessage {
            id: summary_id.clone(),
            role: AiChatRole::Assistant,
            content: format!("\u{1f4cb} **{prefix}**\n\n{summary}"),
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
            summary_ref: Some(serde_json::json!({ "kind": "conversation" })),
            branches: None,
        }];
        let metadata = conversation
            .session_metadata
            .get_or_insert_with(|| serde_json::json!({ "conversationId": conversation_id }));
        if let Some(object) = metadata.as_object_mut() {
            object.insert("lastSummaryAt".to_string(), serde_json::json!(now));
        }
        conversation.updated_at_ms = now;
        self.ai_model_switch_warning_percentage = None;
        self.persist_ai_chat_state();
        self.persist_ai_summary_created(
            &conversation_id,
            &summary_id,
            "conversation",
            summary,
            None,
            Some(original_count),
            Some("manual"),
            now,
        );
        cx.notify();
    }


}

#[derive(Clone)]
pub(super) struct AiCompactionPlan {
    pub(super) compact_messages: Vec<AiChatMessage>,
    pub(super) keep_messages: Vec<AiChatMessage>,
}

pub(super) struct AiStreamDelivery {
    pub(super) generation: u64,
    pub(super) conversation_id: String,
    pub(super) assistant_id: String,
    pub(super) event: AiStreamDeliveryEvent,
}

pub(super) struct AiCompactionDelivery {
    pub(super) kind: AiCompactionDeliveryKind,
    pub(super) conversation_id: String,
    pub(super) base_ids: Vec<String>,
    pub(super) plan: Option<AiCompactionPlan>,
    pub(super) summary: String,
    pub(super) stream_error: Option<String>,
    pub(super) resume_after: Option<AiPendingChatStream>,
}

pub(super) enum AiCompactionDeliveryKind {
    Compact,
    Summary,
}

fn ai_compaction_plan(messages: &[AiChatMessage], context_window: usize) -> Option<AiCompactionPlan> {
    if messages.len() < 4 {
        return None;
    }
    let total_tokens = messages
        .iter()
        .map(ai_message_estimated_tokens)
        .sum::<usize>();
    let keep_budget = ((context_window as f32) * 0.4) as usize;
    let manual_cap = ((total_tokens as f32) * 0.6) as usize;
    let budget = keep_budget.min(manual_cap).max(1);
    let mut keep_start = messages.len();
    let mut used = 0usize;
    for (index, message) in messages.iter().enumerate().rev() {
        let tokens = ai_message_estimated_tokens(message);
        if keep_start < messages.len() && used.saturating_add(tokens) > budget {
            break;
        }
        used = used.saturating_add(tokens);
        keep_start = index;
    }
    if keep_start < 2 {
        keep_start = messages.len().saturating_sub(2);
    }
    let compact_messages = messages[..keep_start].to_vec();
    if compact_messages.len() < 2 {
        return None;
    }
    let keep_messages = messages[keep_start..].to_vec();
    Some(AiCompactionPlan {
        compact_messages,
        keep_messages,
    })
}

fn ai_compaction_summary_messages(messages: &[AiChatMessage]) -> Vec<AiChatMessage> {
    let mut previous_summaries = Vec::new();
    let mut transcript = Vec::new();
    for message in messages {
        if message
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata.kind == "compaction-anchor")
        {
            previous_summaries.push(message.content.trim().to_string());
        } else {
            let role = match message.role {
                AiChatRole::User => "User",
                AiChatRole::Assistant => "Assistant",
                AiChatRole::System => "System",
                AiChatRole::Tool => "Tool",
            };
            transcript.push(format!("{role}: {}", message.content.trim()));
        }
    }
    let mut content = String::from(
        "Summarize the following conversation in a concise paragraph. Capture the key topics, questions asked, solutions provided, and any important context. Write in the same language as the conversation. Keep it under 200 words. If there is a \"[Previous Summary]\" section, integrate it into your summary.",
    );
    if !previous_summaries.is_empty() {
        content.push_str("\n\n[Previous Summary]\n");
        content.push_str(&previous_summaries.join("\n\n"));
    }
    content.push_str("\n\n[Conversation]\n");
    content.push_str(&transcript.join("\n\n"));
    vec![AiChatMessage {
        id: "compact-request".to_string(),
        role: AiChatRole::User,
        content,
        timestamp_ms: 0,
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
    }]
}

fn ai_conversation_summary_messages(messages: &[AiChatMessage]) -> Vec<AiChatMessage> {
    let history_text = messages
        .iter()
        .filter(|message| {
            matches!(
                message.role,
                AiChatRole::User | AiChatRole::Assistant | AiChatRole::Tool
            )
        })
        .map(|message| {
            let role = if message.role == AiChatRole::User {
                "User"
            } else {
                "Assistant"
            };
            format!("{role}: {}", message.content.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    vec![
        AiChatMessage {
            id: "summary-system".to_string(),
            role: AiChatRole::System,
            content: "Summarize the following conversation in a concise paragraph. Capture the key topics, questions asked, solutions provided, and any important context. Write in the same language as the conversation. Keep it under 200 words.".to_string(),
            timestamp_ms: 0,
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
        },
        AiChatMessage {
            id: "summary-request".to_string(),
            role: AiChatRole::User,
            content: history_text,
            timestamp_ms: 0,
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
        },
    ]
}

fn ai_message_estimated_tokens(message: &AiChatMessage) -> usize {
    ai_estimated_tokens(&message.content)
        + message.context.as_deref().map(ai_estimated_tokens).unwrap_or(0)
        + message
            .thinking_content
            .as_deref()
            .map(ai_estimated_tokens)
            .unwrap_or(0)
}

fn ai_tool_definitions_estimated_tokens(tools: &[oxideterm_ai::AiToolDefinition]) -> usize {
    tools
        .iter()
        .map(|tool| {
            ai_estimated_tokens(&tool.name)
                + ai_estimated_tokens(&tool.description)
                + ai_estimated_tokens(&tool.parameters.to_string())
        })
        .sum()
}

fn ai_summary_eligible_tokens(messages: &[&AiChatMessage]) -> usize {
    if messages.len() < 4 {
        return 0;
    }
    messages
        .iter()
        .take(messages.len().saturating_sub(3))
        .map(|message| ai_message_estimated_tokens(message))
        .sum()
}

fn ai_transcript_boundary_id(message: Option<&AiChatMessage>, edge: &str) -> Option<String> {
    let message = message?;
    let transcript_ref = message.transcript_ref.as_ref();
    let primary = if edge == "start" {
        "startEntryId"
    } else {
        "endEntryId"
    };
    let fallback = if edge == "start" {
        "endEntryId"
    } else {
        "startEntryId"
    };
    transcript_ref
        .and_then(|value| value.get(primary))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            transcript_ref
                .and_then(|value| value.get(fallback))
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_string)
        .or_else(|| Some(message.id.clone()))
}

fn ai_summary_source_transcript_ref(
    messages: &[AiChatMessage],
    conversation_id: &str,
) -> serde_json::Value {
    let start_entry_id = ai_transcript_boundary_id(messages.first(), "start");
    let end_entry_id = ai_transcript_boundary_id(messages.last(), "end");
    serde_json::json!({
        "conversationId": conversation_id,
        "startEntryId": start_entry_id,
        "endEntryId": end_entry_id,
    })
}

fn ai_find_prompt_transcript_lookup_reference(
    messages: &[AiChatMessage],
) -> Option<serde_json::Value> {
    messages.iter().rev().find_map(|message| {
        message
            .summary_ref
            .as_ref()
            .and_then(|summary_ref| summary_ref.get("transcriptRef"))
            .filter(|transcript_ref| !transcript_ref.is_null())
            .cloned()
    })
}

fn ai_build_transcript_lookup_prompt_reference(transcript_ref: serde_json::Value) -> String {
    let start_entry_id = transcript_ref
        .get("startEntryId")
        .and_then(serde_json::Value::as_str);
    let end_entry_id = transcript_ref
        .get("endEntryId")
        .and_then(serde_json::Value::as_str);
    let conversation_id = transcript_ref
        .get("conversationId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let mut range_parts = Vec::new();
    if let Some(start_entry_id) = start_entry_id {
        range_parts.push(format!("start={start_entry_id}"));
    }
    if let Some(end_entry_id) = end_entry_id {
        range_parts.push(format!("end={end_entry_id}"));
    }
    let range_text = if range_parts.is_empty() {
        "range=unknown".to_string()
    } else {
        range_parts.join(", ")
    };

    [
        "Earlier history is intentionally compacted out of this prompt.".to_string(),
        format!("Transcript reference: conversation={conversation_id}, {range_text}."),
        "Use the visible summary as the authoritative compressed context. Do not infer omitted details unless they are restated here or fetched through transcript lookup tooling.".to_string(),
    ]
    .join(" ")
}

fn ai_transcript_entry(
    id: String,
    conversation_id: &str,
    kind: &str,
    payload: serde_json::Value,
    turn_id: Option<String>,
    parent_id: Option<String>,
    timestamp: i64,
) -> oxideterm_ai::PersistedTranscriptEntry {
    oxideterm_ai::PersistedTranscriptEntry {
        id,
        conversation_id: conversation_id.to_string(),
        turn_id,
        parent_id,
        timestamp,
        kind: kind.to_string(),
        payload,
    }
}

fn ai_diagnostic_event(
    id: String,
    conversation_id: &str,
    event_type: &str,
    turn_id: Option<String>,
    round_id: Option<String>,
    timestamp: i64,
    data: serde_json::Value,
) -> oxideterm_ai::PersistedDiagnosticEvent {
    oxideterm_ai::PersistedDiagnosticEvent {
        id,
        conversation_id: conversation_id.to_string(),
        turn_id,
        round_id,
        timestamp,
        event_type: event_type.to_string(),
        data,
    }
}

fn upsert_ai_tool_call(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    arguments: &str,
    status: &str,
) {
    if let Some(slot) = message.tool_calls.iter_mut().find(|call| {
        call.get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|existing| existing == id)
    }) {
        if let Some(object) = slot.as_object_mut() {
            object.insert("name".to_string(), serde_json::json!(name));
            object.insert("arguments".to_string(), serde_json::json!(arguments));
            object.insert("status".to_string(), serde_json::json!(status));
        }
    } else {
        message.tool_calls.push(serde_json::json!({
            "id": id,
            "name": name,
            "arguments": arguments,
            "status": status,
            "result": serde_json::Value::Null,
        }));
    }
}

fn update_ai_tool_call_status(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    arguments: &str,
    status: &str,
    result: Option<serde_json::Value>,
    risk: Option<String>,
    summary: Option<String>,
    round_id_override: Option<&str>,
    round_number_override: Option<i64>,
) {
    upsert_ai_tool_call(message, id, name, arguments, status);
    update_ai_turn_tool_status(
        message,
        id,
        name,
        arguments,
        status,
        round_id_override,
        round_number_override,
    );
    let result_for_turn = result.clone();
    if let Some(slot) = message.tool_calls.iter_mut().find(|call| {
        call.get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|existing| existing == id)
    }) && let Some(object) = slot.as_object_mut()
    {
        if let Some(result) = result {
            object.insert("result".to_string(), result);
        }
        if let Some(risk) = risk {
            object.insert("risk".to_string(), serde_json::json!(risk));
        }
        if let Some(summary) = summary {
            object.insert("summary".to_string(), serde_json::json!(summary));
        }
    }
    if let Some(result) = result_for_turn {
        append_ai_turn_tool_result(message, id, name, status, &result);
    }
}

fn ensure_ai_turn(message: &mut AiChatMessage) {
    let needs_init = !message
        .turn
        .as_ref()
        .is_some_and(|turn| turn.as_object().is_some());
    if needs_init {
        message.turn = Some(serde_json::json!({
            "id": message.id.clone(),
            "status": if message.is_streaming { "streaming" } else { "complete" },
            "plainTextSummary": message.content.clone(),
            "parts": [],
            "toolRounds": [],
        }));
    }

    let Some(object) = message
        .turn
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    object
        .entry("id".to_string())
        .or_insert_with(|| serde_json::json!(message.id.clone()));
    object
        .entry("status".to_string())
        .or_insert_with(|| serde_json::json!(if message.is_streaming { "streaming" } else { "complete" }));
    object
        .entry("parts".to_string())
        .or_insert_with(|| serde_json::json!([]));
    object
        .entry("toolRounds".to_string())
        .or_insert_with(|| serde_json::json!([]));
    object
        .entry("pendingSummaries".to_string())
        .or_insert_with(|| serde_json::json!([]));
}

fn set_ai_turn_status(message: &mut AiChatMessage, status: &str) {
    ensure_ai_turn(message);
    if let Some(object) = message
        .turn
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
    {
        object.insert("status".to_string(), serde_json::json!(status));
        object.insert(
            "plainTextSummary".to_string(),
            serde_json::json!(message.content),
        );
    }
}

fn mutate_ai_turn_parts(message: &mut AiChatMessage, f: impl FnOnce(&mut Vec<serde_json::Value>)) {
    ensure_ai_turn(message);
    if let Some(parts) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("parts"))
        .and_then(serde_json::Value::as_array_mut)
    {
        f(parts);
    }
}

fn mutate_ai_turn_rounds(message: &mut AiChatMessage, f: impl FnOnce(&mut Vec<serde_json::Value>)) {
    ensure_ai_turn(message);
    if let Some(rounds) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("toolRounds"))
        .and_then(serde_json::Value::as_array_mut)
    {
        f(rounds);
    }
}

fn upsert_ai_round_summary(
    message: &mut AiChatMessage,
    round_id: &str,
    text: &str,
    metadata: serde_json::Value,
) {
    ensure_ai_turn(message);
    if attach_ai_round_summary(message, round_id, text, Some(metadata.clone())) {
        remove_ai_pending_round_summary(message, round_id);
        return;
    }

    if let Some(pending) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("pendingSummaries"))
        .and_then(serde_json::Value::as_array_mut)
    {
        let mut summary = serde_json::json!({
            "roundId": round_id,
            "text": text,
        });
        if let Some(object) = summary.as_object_mut()
            && !metadata.is_null()
        {
            object.insert("metadata".to_string(), metadata);
        }
        if let Some(existing) = pending.iter_mut().find(|summary| {
            summary
                .get("roundId")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == round_id)
        }) {
            *existing = summary;
        } else {
            pending.push(summary);
        }
    }
}

fn normalize_ai_pending_summaries(message: &mut AiChatMessage) {
    ensure_ai_turn(message);
    let pending = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("pendingSummaries"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if pending.is_empty() {
        return;
    }

    let mut unresolved = Vec::new();
    for summary in pending {
        let Some(round_id) = summary
            .get("roundId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let text = summary
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        if text.is_empty() {
            continue;
        }
        let metadata = summary.get("metadata").cloned();
        if !attach_ai_round_summary(message, &round_id, &text, metadata) {
            unresolved.push(summary);
        }
    }

    if let Some(pending) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("pendingSummaries"))
        .and_then(serde_json::Value::as_array_mut)
    {
        *pending = unresolved;
    }
}

fn attach_ai_round_summary(
    message: &mut AiChatMessage,
    round_id: &str,
    text: &str,
    metadata: Option<serde_json::Value>,
) -> bool {
    let Some(rounds) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("toolRounds"))
        .and_then(serde_json::Value::as_array_mut)
    else {
        return false;
    };
    let Some(round) = rounds.iter_mut().find(|round| {
        round
            .get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|existing| existing == round_id)
    }) else {
        return false;
    };
    let Some(object) = round.as_object_mut() else {
        return false;
    };
    object.insert("summary".to_string(), serde_json::json!(text));
    if let Some(metadata) = metadata
        && !metadata.is_null()
    {
        object.insert("summaryMetadata".to_string(), metadata);
    }
    true
}

fn remove_ai_pending_round_summary(message: &mut AiChatMessage, round_id: &str) {
    if let Some(pending) = message
        .turn
        .as_mut()
        .and_then(|turn| turn.get_mut("pendingSummaries"))
        .and_then(serde_json::Value::as_array_mut)
    {
        pending.retain(|summary| {
            !summary
                .get("roundId")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == round_id)
        });
    }
}

fn set_ai_turn_round_stateful_marker(
    message: &mut AiChatMessage,
    round_id: &str,
    marker: Option<&str>,
) {
    ensure_ai_turn(message);
    mutate_ai_turn_rounds(message, |rounds| {
        let Some(round) = rounds.iter_mut().find(|round| {
            round
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == round_id)
        }) else {
            return;
        };
        let Some(object) = round.as_object_mut() else {
            return;
        };
        if let Some(marker) = marker {
            object.insert("statefulMarker".to_string(), serde_json::json!(marker));
        } else {
            object.remove("statefulMarker");
        }
    });
}

fn ai_turn_plain_text_summary(message: &AiChatMessage) -> Option<String> {
    let parts = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("parts"))
        .and_then(serde_json::Value::as_array)?;
    let summary = parts
        .iter()
        .filter(|part| part.get("type").and_then(serde_json::Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
        .collect::<String>();
    Some(summary)
}

fn append_ai_turn_text_part(
    message: &mut AiChatMessage,
    part_type: &str,
    text: &str,
    streaming: bool,
) {
    if text.is_empty() {
        return;
    }
    mutate_ai_turn_parts(message, |parts| {
        if let Some(last) = parts
            .last_mut()
            .and_then(serde_json::Value::as_object_mut)
            .filter(|part| part.get("type").and_then(serde_json::Value::as_str) == Some(part_type))
        {
            let next = last
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
                + text;
            last.insert("text".to_string(), serde_json::json!(next));
            if part_type == "thinking" {
                last.insert("streaming".to_string(), serde_json::json!(streaming));
            }
            return;
        }
        let mut part = serde_json::json!({
            "type": part_type,
            "text": text,
        });
        if part_type == "thinking"
            && let Some(object) = part.as_object_mut()
        {
            object.insert("streaming".to_string(), serde_json::json!(streaming));
        }
        parts.push(part);
    });
}

fn append_ai_turn_error_part(message: &mut AiChatMessage, error: &str) {
    mutate_ai_turn_parts(message, |parts| {
        parts.push(serde_json::json!({
            "type": "error",
            "message": error,
            "code": "stream_error",
        }));
    });
}

fn append_ai_turn_guardrail_part(
    message: &mut AiChatMessage,
    code: &str,
    guardrail_message: &str,
    raw_text: Option<&str>,
) {
    mutate_ai_turn_parts(message, |parts| {
        let mut part = serde_json::json!({
            "type": "guardrail",
            "code": code,
            "message": guardrail_message,
        });
        if let Some(raw_text) = raw_text
            && let Some(object) = part.as_object_mut()
        {
            object.insert("rawText".to_string(), serde_json::json!(raw_text));
        }
        parts.push(part);
    });
}

fn upsert_ai_turn_tool_call(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    arguments: &str,
    status: &str,
) {
    let (round_id, round_number) = ai_turn_round_for_tool_call(message, id);
    mutate_ai_turn_parts(message, |parts| {
        if let Some(existing) = parts.iter_mut().find(|part| {
            part.get("type").and_then(serde_json::Value::as_str) == Some("tool_call")
                && part
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|existing| existing == id)
        }) && let Some(object) = existing.as_object_mut()
        {
            object.insert("name".to_string(), serde_json::json!(name));
            object.insert("argumentsText".to_string(), serde_json::json!(arguments));
            object.insert("status".to_string(), serde_json::json!(status));
            return;
        }

        parts.push(serde_json::json!({
            "type": "tool_call",
            "id": id,
            "name": name,
            "argumentsText": arguments,
            "status": status,
        }));
    });
    upsert_ai_turn_round_tool_call(message, id, name, arguments, status, &round_id, round_number);
}

fn update_ai_turn_tool_status(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    arguments: &str,
    status: &str,
    round_id_override: Option<&str>,
    round_number_override: Option<i64>,
) {
    if round_id_override.is_none() {
        upsert_ai_turn_tool_call(message, id, name, arguments, "complete");
    }
    let (round_id, round_number) =
        ai_turn_round_for_tool_call_with_override(message, id, round_id_override, round_number_override);
    upsert_ai_turn_round_tool_call(message, id, name, arguments, status, &round_id, round_number);
}

fn upsert_ai_turn_round_tool_call(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    arguments: &str,
    status: &str,
    round_id: &str,
    round_number: i64,
) {
    let timestamp = message.timestamp_ms;
    mutate_ai_turn_rounds(message, |rounds| {
        if !rounds.iter().any(|round| {
            round
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == round_id)
        }) {
            rounds.push(serde_json::json!({
                "id": round_id,
                "round": round_number,
                "timestamp": timestamp,
                "toolCalls": [],
            }));
        }
        let Some(tool_calls) = rounds
            .iter_mut()
            .find(|round| {
                round
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|existing| existing == round_id)
            })
            .and_then(|round| round.get_mut("toolCalls"))
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        let state_field = match status {
            "pending_user_approval" => Some(("approvalState", "pending")),
            "approved" => Some(("approvalState", "approved")),
            "rejected" => Some(("approvalState", "rejected")),
            "running" => Some(("executionState", "running")),
            "completed" => Some(("executionState", "completed")),
            "error" => Some(("executionState", "error")),
            "pending" | "partial" | "complete" => Some(("executionState", "pending")),
            _ => None,
        };
        if let Some(existing) = tool_calls.iter_mut().find(|tool_call| {
            tool_call
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|existing| existing == id)
        }) && let Some(object) = existing.as_object_mut()
        {
            object.insert("name".to_string(), serde_json::json!(name));
            object.insert("argumentsText".to_string(), serde_json::json!(arguments));
            if let Some((field, value)) = state_field {
                object.insert(field.to_string(), serde_json::json!(value));
            }
            return;
        }
        let mut call = serde_json::json!({
            "id": id,
            "name": name,
            "argumentsText": arguments,
        });
        if let Some((field, value)) = state_field
            && let Some(object) = call.as_object_mut()
        {
            object.insert(field.to_string(), serde_json::json!(value));
        }
        tool_calls.push(call);
    });
    normalize_ai_pending_summaries(message);
}

fn ai_turn_round_for_tool_call(message: &AiChatMessage, id: &str) -> (String, i64) {
    if let Some(existing) = ai_turn_round_for_existing_tool_call(message, id) {
        return existing;
    }

    let Some(rounds) = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("toolRounds"))
        .and_then(serde_json::Value::as_array)
    else {
        return (format!("{}-round-1", message.id), 1);
    };

    let latest_round = rounds
        .iter()
        .filter_map(|round| {
            let id = round.get("id").and_then(serde_json::Value::as_str)?;
            let number = round.get("round").and_then(serde_json::Value::as_i64)?;
            Some((id.to_string(), number))
        })
        .max_by_key(|(_, number)| *number);

    let Some((latest_round_id, latest_round_number)) = latest_round else {
        return (format!("{}-round-1", message.id), 1);
    };

    if ai_turn_round_has_result(message, &latest_round_id) {
        let next = latest_round_number.saturating_add(1);
        (format!("{}-round-{next}", message.id), next)
    } else {
        (latest_round_id, latest_round_number)
    }
}

fn ai_turn_round_for_tool_call_with_override(
    message: &AiChatMessage,
    id: &str,
    round_id_override: Option<&str>,
    round_number_override: Option<i64>,
) -> (String, i64) {
    if let Some(round_id) = round_id_override {
        return (
            round_id.to_string(),
            round_number_override.unwrap_or_else(|| {
                ai_turn_round_for_existing_tool_call(message, id)
                    .map(|(_, number)| number)
                    .unwrap_or(1)
            }),
        );
    }
    ai_turn_round_for_tool_call(message, id)
}

fn ai_turn_round_for_existing_tool_call(message: &AiChatMessage, id: &str) -> Option<(String, i64)> {
    let rounds = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("toolRounds"))
        .and_then(serde_json::Value::as_array)?;
    for round in rounds {
        let has_tool = round
            .get("toolCalls")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|tool_calls| {
                tool_calls.iter().any(|tool_call| {
                    tool_call
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|existing| existing == id)
                })
            });
        if has_tool {
            let round_id = round.get("id")?.as_str()?.to_string();
            let round_number = round.get("round")?.as_i64()?;
            return Some((round_id, round_number));
        }
    }
    None
}

fn ai_turn_round_has_result(message: &AiChatMessage, round_id: &str) -> bool {
    let Some(round_tool_ids) = message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("toolRounds"))
        .and_then(serde_json::Value::as_array)
        .and_then(|rounds| {
            rounds.iter().find(|round| {
                round
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|existing| existing == round_id)
            })
        })
        .and_then(|round| round.get("toolCalls"))
        .and_then(serde_json::Value::as_array)
        .map(|tool_calls| {
            tool_calls
                .iter()
                .filter_map(|tool_call| tool_call.get("id").and_then(serde_json::Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
    else {
        return false;
    };

    message
        .turn
        .as_ref()
        .and_then(|turn| turn.get("parts"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|parts| {
            parts.iter().any(|part| {
                part.get("type").and_then(serde_json::Value::as_str) == Some("tool_result")
                    && part
                        .get("toolCallId")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|tool_call_id| round_tool_ids.iter().any(|id| id == tool_call_id))
            })
        })
}

fn append_ai_turn_tool_result(
    message: &mut AiChatMessage,
    id: &str,
    name: &str,
    status: &str,
    result: &serde_json::Value,
) {
    let success = result
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(status == "completed");
    let output = result
        .get("output")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| serde_json::to_string_pretty(result).unwrap_or_default());
    mutate_ai_turn_parts(message, |parts| {
        if let Some(existing) = parts.iter_mut().find(|part| {
            part.get("type").and_then(serde_json::Value::as_str) == Some("tool_result")
                && part
                    .get("toolCallId")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|existing| existing == id)
        }) && let Some(object) = existing.as_object_mut()
        {
            object.insert("toolName".to_string(), serde_json::json!(name));
            object.insert("success".to_string(), serde_json::json!(success));
            object.insert("output".to_string(), serde_json::json!(output));
            object.insert("envelope".to_string(), result.clone());
            return;
        }
        parts.push(serde_json::json!({
            "type": "tool_result",
            "toolCallId": id,
            "toolName": name,
            "success": success,
            "output": output,
            "envelope": result,
        }));
    });
}

fn normalize_ai_stream_history_for_provider(history: &mut Vec<AiChatMessage>) {
    let mut normalized = Vec::with_capacity(history.len());
    for mut message in history.drain(..) {
        match message.role {
            AiChatRole::System if is_ai_compaction_anchor(&message) => {
                let summary = message.content.trim();
                if summary.is_empty() {
                    continue;
                }
                message.content = format!("Previous conversation summary:\n{summary}");
                message.metadata = None;
                message.tool_calls.clear();
                message.tool_call_id = None;
                message.thinking_content = None;
                normalized.push(message);
            }
            AiChatRole::System if is_runtime_ai_history_system(&message) => {
                if !message.content.trim().is_empty() {
                    normalized.push(message);
                }
            }
            AiChatRole::System => {}
            AiChatRole::User => {
                if !message.content.trim().is_empty() {
                    normalized.push(message);
                }
            }
            AiChatRole::Assistant => {
                if message.content.trim().is_empty() {
                    continue;
                }
                // Tauri replays prior turns as plain assistant text. Tool protocol
                // messages are only emitted inside the live tool loop, where every
                // assistant tool_call is immediately followed by its matching tool result.
                message.tool_calls.clear();
                message.tool_call_id = None;
                message.thinking_content = None;
                normalized.push(message);
            }
            AiChatRole::Tool => {}
        }
    }
    *history = normalized;
}

fn is_ai_compaction_anchor(message: &AiChatMessage) -> bool {
    message
        .metadata
        .as_ref()
        .is_some_and(|metadata| metadata.kind == "compaction-anchor")
}

fn is_runtime_ai_history_system(message: &AiChatMessage) -> bool {
    matches!(
        message.id.as_str(),
        "task-mode" | "current-terminal-context"
    )
}

fn reject_incomplete_ai_tool_calls_on_cancel(conversation: &mut AiConversation) {
    for message in &mut conversation.messages {
        if message.role != AiChatRole::Assistant || !message.is_streaming {
            continue;
        }
        let pending_calls = message
            .tool_calls
            .iter()
            .filter_map(cancel_rejected_tool_call)
            .collect::<Vec<_>>();
        for (id, name, arguments) in pending_calls {
            let result = serde_json::json!({
                "ok": false,
                "summary": "Generation was stopped.",
                "output": "Generation was stopped.",
                "data": serde_json::Value::Null,
                "error": {
                    "code": "generation_stopped",
                    "message": "Generation was stopped.",
                    "recoverable": true,
                },
                "targets": [],
                "meta": {
                    "toolName": name,
                    "durationMs": 0,
                    "verified": false,
                    "capability": serde_json::Value::Null,
                    "truncated": false,
                }
            });
            update_ai_tool_call_status(
                message,
                &id,
                &name,
                &arguments,
                "rejected",
                Some(result),
                None,
                Some("Generation was stopped.".to_string()),
                None,
                None,
            );
        }
    }
}

fn cancel_rejected_tool_call(call: &serde_json::Value) -> Option<(String, String, String)> {
    let status = call
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if matches!(status, "completed" | "error" | "rejected") {
        return None;
    }
    if call.get("result").is_some_and(|result| !result.is_null()) {
        return None;
    }
    let id = call.get("id").and_then(serde_json::Value::as_str)?;
    let name = call.get("name").and_then(serde_json::Value::as_str)?;
    let arguments = call
        .get("arguments")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    Some((id.to_string(), name.to_string(), arguments.to_string()))
}

#[cfg(test)]
mod ai_turn_order_tests {
    use super::*;

    fn assistant_message() -> AiChatMessage {
        AiChatMessage {
            id: "assistant-1".to_string(),
            role: AiChatRole::Assistant,
            content: String::new(),
            timestamp_ms: 1,
            model: None,
            context: None,
            is_streaming: true,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
        }
    }

    fn test_message(id: &str, role: AiChatRole, content: String) -> AiChatMessage {
        AiChatMessage {
            id: id.to_string(),
            role,
            content,
            timestamp_ms: 1,
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
        }
    }

    #[test]
    fn history_trimming_uses_tauri_history_budget_ratio() {
        let cjk_100 = "你".repeat(100);
        let mut history = vec![
            test_message("system", AiChatRole::System, cjk_100.clone()),
            test_message("user-1", AiChatRole::User, cjk_100.clone()),
            test_message("assistant-1", AiChatRole::Assistant, cjk_100.clone()),
            test_message("user-2", AiChatRole::User, cjk_100),
        ];

        let trimmed = trim_ai_stream_history_to_budget(&mut history, 1000, 150);

        assert_eq!(trimmed, 1);
        assert_eq!(
            history.iter().map(|message| message.id.as_str()).collect::<Vec<_>>(),
            vec!["system", "assistant-1", "user-2"]
        );
    }

    #[test]
    fn prompt_budget_policy_matches_tauri_levels() {
        let decision = determine_ai_compression_level(AiPromptBudgetInput {
            context_window: 1000,
            response_reserve: 150,
            system_budget: 50,
            history_tokens: 630,
            safety_margin: Some(0),
            trimmable_history_tokens: Some(630),
            summary_eligible_tokens: Some(630),
            can_summarize: true,
            can_lookup_transcript: false,
            in_tool_loop: false,
            auto_compact_threshold: Some(0.80),
            transcript_lookup_threshold: None,
            tool_loop_stop_threshold: None,
        });

        assert_eq!(decision.level, 2);

        let tool_loop_stop = determine_ai_compression_level(AiPromptBudgetInput {
            context_window: 1000,
            response_reserve: 100,
            system_budget: 0,
            history_tokens: 890,
            safety_margin: Some(0),
            trimmable_history_tokens: Some(0),
            summary_eligible_tokens: Some(0),
            can_summarize: false,
            can_lookup_transcript: false,
            in_tool_loop: true,
            auto_compact_threshold: None,
            transcript_lookup_threshold: None,
            tool_loop_stop_threshold: Some(0.98),
        });

        assert_eq!(tool_loop_stop.level, 4);
    }

    #[test]
    fn compaction_reference_survives_provider_history_normalization() {
        let compacted = vec![
            test_message("u-1", AiChatRole::User, "first".to_string()),
            test_message("a-1", AiChatRole::Assistant, "answer".to_string()),
            test_message("u-2", AiChatRole::User, "second".to_string()),
            test_message("a-2", AiChatRole::Assistant, "answer".to_string()),
        ];
        let source_ref = ai_summary_source_transcript_ref(&compacted, "conv-1");
        assert_eq!(
            source_ref.get("startEntryId").and_then(serde_json::Value::as_str),
            Some("u-1")
        );
        assert_eq!(
            source_ref.get("endEntryId").and_then(serde_json::Value::as_str),
            Some("a-2")
        );

        let mut history = vec![AiChatMessage {
            id: "anchor-1".to_string(),
            role: AiChatRole::System,
            content: "summary".to_string(),
            timestamp_ms: 1,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: Some(AiChatMessageMetadata {
                kind: "compaction-anchor".to_string(),
                original_count: Some(compacted.len()),
                compacted_at_ms: Some(1),
                original_messages: Some(compacted),
            }),
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: Some(serde_json::json!({
                "conversationId": "conv-1",
                "endEntryId": "anchor-1",
            })),
            summary_ref: Some(serde_json::json!({
                "kind": "compaction",
                "transcriptRef": source_ref,
            })),
            branches: None,
        }];

        normalize_ai_stream_history_for_provider(&mut history);
        let lookup_ref = ai_find_prompt_transcript_lookup_reference(&history)
            .expect("compaction transcript lookup reference");
        let lookup_prompt = ai_build_transcript_lookup_prompt_reference(lookup_ref);

        assert_eq!(history[0].content, "Previous conversation summary:\nsummary");
        assert!(lookup_prompt.contains("conversation=conv-1"));
        assert!(lookup_prompt.contains("start=u-1"));
        assert!(lookup_prompt.contains("end=a-2"));
    }

    #[test]
    fn old_tool_messages_are_condensed_like_tauri_tool_loop() {
        let mut history = (0..7)
            .map(|index| AiChatMessage {
                id: format!("tool-{index}"),
                role: AiChatRole::Tool,
                content: serde_json::json!({
                    "ok": true,
                    "output": format!("line 1\nline 2\nline 3\nline 4\nline 5 for {index}"),
                    "meta": { "toolName": "read_resource" },
                })
                .to_string(),
                timestamp_ms: index,
                model: None,
                context: None,
                is_streaming: false,
                thinking_content: None,
                metadata: None,
                tool_call_id: Some(format!("call-{index}")),
                tool_calls: Vec::new(),
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            })
            .collect::<Vec<_>>();

        condense_ai_tool_messages(&mut history);

        assert!(history[0].content.starts_with("[condensed] read_resource -> ok:"));
        assert!(history[1].content.starts_with("[condensed] read_resource -> ok:"));
        assert!(!history[2].content.starts_with("[condensed]"));
    }

    #[test]
    fn guardrail_parts_are_structured_like_tauri_turn_model() {
        let mut message = assistant_message();

        append_ai_turn_guardrail_part(
            &mut message,
            "tool-budget-limit",
            "Tool use stopped.",
            Some("raw candidate text"),
        );

        let parts = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(serde_json::Value::as_array)
            .expect("turn parts");
        assert_eq!(parts[0]["type"], "guardrail");
        assert_eq!(parts[0]["code"], "tool-budget-limit");
        assert_eq!(parts[0]["message"], "Tool use stopped.");
        assert_eq!(parts[0]["rawText"], "raw candidate text");
    }

    #[test]
    fn pending_round_summary_attaches_when_round_arrives() {
        let mut message = assistant_message();

        upsert_ai_round_summary(
            &mut message,
            "assistant-1-round-1",
            "read_resource: ok - inspected config",
            serde_json::json!({
                "source": "background",
                "summarizationMode": "background",
                "contextLengthBefore": 128,
            }),
        );

        assert_eq!(
            message
                .turn
                .as_ref()
                .and_then(|turn| turn.get("pendingSummaries"))
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(1),
        );

        upsert_ai_turn_round_tool_call(
            &mut message,
            "call-1",
            "read_resource",
            "{}",
            "completed",
            "assistant-1-round-1",
            1,
        );

        let turn = message.turn.as_ref().expect("turn");
        let rounds = turn
            .get("toolRounds")
            .and_then(serde_json::Value::as_array)
            .expect("rounds");
        assert_eq!(rounds[0]["summary"], "read_resource: ok - inspected config");
        assert_eq!(
            rounds[0]["summaryMetadata"]["contextLengthBefore"],
            serde_json::json!(128)
        );
        assert_eq!(
            turn.get("pendingSummaries")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(0),
        );
    }

    #[test]
    fn round_summary_updates_existing_round_without_pending_tail() {
        let mut message = assistant_message();

        upsert_ai_turn_round_tool_call(
            &mut message,
            "call-1",
            "run_command",
            "{}",
            "completed",
            "assistant-1-round-1",
            1,
        );
        upsert_ai_round_summary(
            &mut message,
            "assistant-1-round-1",
            "run_command: ok - printed working directory",
            serde_json::json!({ "model": "deepseek-v4-pro" }),
        );

        let turn = message.turn.as_ref().expect("turn");
        let rounds = turn
            .get("toolRounds")
            .and_then(serde_json::Value::as_array)
            .expect("rounds");
        assert_eq!(
            rounds[0]["summary"],
            "run_command: ok - printed working directory"
        );
        assert_eq!(rounds[0]["summaryMetadata"]["model"], "deepseek-v4-pro");
        assert_eq!(
            turn.get("pendingSummaries")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(0),
        );
    }

    #[test]
    fn round_stateful_marker_matches_tauri_awaiting_summary_lifecycle() {
        let mut message = assistant_message();

        upsert_ai_turn_round_tool_call(
            &mut message,
            "call-1",
            "run_command",
            "{}",
            "completed",
            "assistant-1-round-1",
            1,
        );
        set_ai_turn_round_stateful_marker(
            &mut message,
            "assistant-1-round-1",
            Some("awaiting-summary"),
        );

        let turn = message.turn.as_ref().expect("turn");
        let round = &turn
            .get("toolRounds")
            .and_then(serde_json::Value::as_array)
            .expect("rounds")[0];
        assert_eq!(round["statefulMarker"], "awaiting-summary");

        set_ai_turn_round_stateful_marker(&mut message, "assistant-1-round-1", None);
        let round = &message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("toolRounds"))
            .and_then(serde_json::Value::as_array)
            .expect("rounds")[0];
        assert!(round.get("statefulMarker").is_none());
    }

    #[test]
    fn turn_plain_text_summary_uses_text_parts_like_tauri_turn_end() {
        let mut message = assistant_message();

        append_ai_turn_text_part(&mut message, "thinking", "hidden reasoning", false);
        append_ai_turn_text_part(&mut message, "text", "visible ", false);
        append_ai_turn_tool_result(
            &mut message,
            "call-1",
            "run_command",
            "completed",
            &serde_json::json!({ "ok": true, "output": "tool output" }),
        );
        append_ai_turn_text_part(&mut message, "text", "answer", false);

        assert_eq!(
            ai_turn_plain_text_summary(&message).as_deref(),
            Some("visible answer")
        );
    }

    #[test]
    fn synthetic_denied_tool_status_uses_retry_round_override() {
        let mut message = assistant_message();

        update_ai_tool_call_status(
            &mut message,
            "assistant-1-hard-deny-1-tool",
            "tool_use_disabled",
            r#"{"reason":"tool_use_disabled","retryAttempt":1}"#,
            "rejected",
            Some(serde_json::json!({
                "ok": false,
                "output": "",
                "error": { "message": "Tool use is disabled." },
            })),
            Some("write".to_string()),
            Some("Tool use is disabled.".to_string()),
            Some("assistant-1-hard-deny-1"),
            Some(1),
        );

        let rounds = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("toolRounds"))
            .and_then(serde_json::Value::as_array)
            .expect("tool rounds");
        assert_eq!(rounds[0]["id"], "assistant-1-hard-deny-1");
        assert_eq!(rounds[0]["toolCalls"][0]["approvalState"], "rejected");
    }

    #[test]
    fn required_tool_obligation_retries_action_claims() {
        let obligation = ai_classify_orchestrator_obligation("打开本地终端");

        assert_eq!(obligation.mode, AiOrchestratorObligationMode::Required);
        assert!(obligation.candidate_tools.iter().any(|tool| tool == "open_app_surface"));
        assert!(ai_orchestrator_obligation_prompt(&obligation)
            .expect("prompt")
            .contains("Required Tool Call"));
        assert!(ai_should_retry_required_tool_round(
            &obligation,
            "我已经打开了本地终端。"
        ));
        assert!(!ai_should_retry_required_tool_round(
            &obligation,
            "需要你确认打开哪一个终端？"
        ));
    }

    #[test]
    fn pseudo_tool_json_hard_deny_respects_json_requests() {
        let pseudo = r#"{"name":"run_command","arguments":{"command":"ls"},"status":"ok"}"#;

        assert!(ai_should_trigger_hard_deny(pseudo, false));
        assert!(!ai_should_trigger_hard_deny(pseudo, true));
        assert!(!ai_should_trigger_hard_deny("正常回答", false));
    }

    #[test]
    fn turn_parts_keep_tool_call_before_later_text() {
        let mut message = assistant_message();
        upsert_ai_tool_call(&mut message, "call-1", "open_app_surface", "{}", "pending");
        upsert_ai_turn_tool_call(&mut message, "call-1", "open_app_surface", "{}", "complete");
        append_ai_turn_tool_result(
            &mut message,
            "call-1",
            "open_app_surface",
            "completed",
            &serde_json::json!({ "ok": true, "output": "opened" }),
        );
        message.content.push_str("Terminal opened.");
        append_ai_turn_text_part(&mut message, "text", "Terminal opened.", false);

        let parts = message
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(serde_json::Value::as_array)
            .expect("turn parts");
        assert_eq!(parts[0]["type"], "tool_call");
        assert_eq!(parts[1]["type"], "tool_result");
        assert_eq!(parts[2]["type"], "text");
        assert_eq!(message.tool_calls.len(), 1);
    }

    #[test]
    fn turn_parts_split_completed_tool_loops_into_distinct_rounds() {
        let mut message = assistant_message();
        upsert_ai_turn_tool_call(&mut message, "call-1", "open_app_surface", "{}", "complete");
        append_ai_turn_tool_result(
            &mut message,
            "call-1",
            "open_app_surface",
            "completed",
            &serde_json::json!({ "ok": true, "output": "opened" }),
        );
        upsert_ai_turn_tool_call(&mut message, "call-2", "get_state", "{}", "complete");
        append_ai_turn_tool_result(
            &mut message,
            "call-2",
            "get_state",
            "completed",
            &serde_json::json!({ "ok": true, "output": "ready" }),
        );

        let turn = message.turn.as_ref().expect("turn");
        let parts = turn
            .get("parts")
            .and_then(serde_json::Value::as_array)
            .expect("turn parts");
        assert_eq!(parts[0]["type"], "tool_call");
        assert_eq!(parts[1]["type"], "tool_result");
        assert_eq!(parts[2]["type"], "tool_call");
        assert_eq!(parts[3]["type"], "tool_result");

        let rounds = turn
            .get("toolRounds")
            .and_then(serde_json::Value::as_array)
            .expect("tool rounds");
        assert_eq!(rounds.len(), 2);
        assert_eq!(rounds[0]["toolCalls"][0]["id"], "call-1");
        assert_eq!(rounds[1]["toolCalls"][0]["id"], "call-2");
        let first_round = ai_tool_part_round_id(&message, &parts[0]).expect("first round");
        let second_round = ai_tool_part_round_id(&message, &parts[2]).expect("second round");
        assert_ne!(first_round, second_round);
    }

    #[test]
    fn turn_parts_keep_parallel_tool_calls_in_one_round_until_results_arrive() {
        let mut message = assistant_message();
        upsert_ai_turn_tool_call(&mut message, "call-1", "read_resource", "{}", "complete");
        upsert_ai_turn_tool_call(&mut message, "call-2", "get_state", "{}", "complete");
        append_ai_turn_tool_result(
            &mut message,
            "call-1",
            "read_resource",
            "completed",
            &serde_json::json!({ "ok": true, "output": "file" }),
        );
        append_ai_turn_tool_result(
            &mut message,
            "call-2",
            "get_state",
            "completed",
            &serde_json::json!({ "ok": true, "output": "state" }),
        );

        let turn = message.turn.as_ref().expect("turn");
        let parts = turn
            .get("parts")
            .and_then(serde_json::Value::as_array)
            .expect("turn parts");
        assert_eq!(
            parts
                .iter()
                .filter(|part| part.get("type").and_then(serde_json::Value::as_str)
                    == Some("tool_call"))
                .count(),
            2
        );

        let rounds = turn
            .get("toolRounds")
            .and_then(serde_json::Value::as_array)
            .expect("tool rounds");
        assert_eq!(rounds.len(), 1);
        assert_eq!(
            rounds[0]
                .get("toolCalls")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        let first_round = ai_tool_part_round_id(&message, &parts[0]).expect("first round");
        let second_round = ai_tool_part_round_id(&message, &parts[1]).expect("second round");
        assert_eq!(first_round, second_round);
    }

    #[test]
    fn provider_history_replays_legacy_tool_turns_as_plain_assistant_text() {
        let mut history = vec![
            AiChatMessage {
                id: "user-1".to_string(),
                role: AiChatRole::User,
                content: "打开终端".to_string(),
                timestamp_ms: 1,
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
            },
            AiChatMessage {
                id: "assistant-1".to_string(),
                role: AiChatRole::Assistant,
                content: "本地终端已重新打开。".to_string(),
                timestamp_ms: 2,
                model: None,
                context: None,
                is_streaming: false,
                thinking_content: Some("need a terminal".to_string()),
                metadata: None,
                tool_call_id: None,
                tool_calls: vec![serde_json::json!({
                    "id": "call-1",
                    "name": "open_app_surface",
                    "arguments": "{\"surface\":\"local_terminal\"}",
                    "status": "completed",
                    "result": {
                        "ok": true,
                        "output": "opened",
                        "meta": { "toolName": "open_app_surface" }
                    }
                })],
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            },
            AiChatMessage {
                id: "tool-result-call-1".to_string(),
                role: AiChatRole::Tool,
                content: "{\"ok\":true}".to_string(),
                timestamp_ms: 3,
                model: None,
                context: None,
                is_streaming: false,
                thinking_content: None,
                metadata: None,
                tool_call_id: Some("call-1".to_string()),
                tool_calls: Vec::new(),
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            },
        ];

        normalize_ai_stream_history_for_provider(&mut history);

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, AiChatRole::User);
        assert_eq!(history[1].role, AiChatRole::Assistant);
        assert_eq!(history[1].content, "本地终端已重新打开。");
        assert!(history[1].tool_calls.is_empty());
        assert!(history[1].thinking_content.is_none());
    }

    #[test]
    fn provider_history_drops_empty_tool_only_assistant_messages() {
        let mut history = vec![AiChatMessage {
            id: "assistant-tool-only".to_string(),
            role: AiChatRole::Assistant,
            content: String::new(),
            timestamp_ms: 1,
            model: None,
            context: None,
            is_streaming: false,
            thinking_content: None,
            metadata: None,
            tool_call_id: None,
            tool_calls: vec![serde_json::json!({
                "id": "call-1",
                "name": "open_app_surface",
                "arguments": "{}"
            })],
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
        }];

        normalize_ai_stream_history_for_provider(&mut history);

        assert!(history.is_empty());
    }

    #[test]
    fn provider_history_promotes_compaction_anchor_to_front_system_summary() {
        let mut history = vec![
            AiChatMessage {
                id: "task-mode".to_string(),
                role: AiChatRole::System,
                content: "Task instructions".to_string(),
                timestamp_ms: 0,
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
            },
            AiChatMessage {
                id: "stale-system".to_string(),
                role: AiChatRole::System,
                content: "Persisted stale system prompt".to_string(),
                timestamp_ms: 0,
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
            },
            AiChatMessage {
                id: "anchor-1".to_string(),
                role: AiChatRole::System,
                content: "用户之前打开过本地终端。".to_string(),
                timestamp_ms: 1,
                model: None,
                context: None,
                is_streaming: false,
                thinking_content: None,
                metadata: Some(AiChatMessageMetadata {
                    kind: "compaction-anchor".to_string(),
                    original_count: Some(4),
                    compacted_at_ms: Some(1),
                    original_messages: None,
                }),
                tool_call_id: None,
                tool_calls: Vec::new(),
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            },
            AiChatMessage {
                id: "user-1".to_string(),
                role: AiChatRole::User,
                content: "继续".to_string(),
                timestamp_ms: 2,
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
            },
        ];

        normalize_ai_stream_history_for_provider(&mut history);

        assert_eq!(history.len(), 3);
        assert_eq!(history[0].id, "task-mode");
        assert_eq!(history[1].role, AiChatRole::System);
        assert_eq!(
            history[1].content,
            "Previous conversation summary:\n用户之前打开过本地终端。"
        );
        assert!(history[1].metadata.is_none());
        assert_eq!(history[2].role, AiChatRole::User);
        assert!(history.iter().all(|message| message.id != "stale-system"));
    }

    #[test]
    fn completed_tool_calls_are_deduped_by_id_before_protocol_append() {
        let mut completed = Vec::new();
        record_completed_ai_tool_call(
            &mut completed,
            AiToolCall {
                id: "call-1".to_string(),
                name: "read_resource".to_string(),
                arguments: "{\"query\":\"old\"}".to_string(),
            },
        );
        record_completed_ai_tool_call(
            &mut completed,
            AiToolCall {
                id: "call-1".to_string(),
                name: "read_resource".to_string(),
                arguments: "{\"query\":\"new\"}".to_string(),
            },
        );
        record_completed_ai_tool_call(
            &mut completed,
            AiToolCall {
                id: "call-2".to_string(),
                name: "get_state".to_string(),
                arguments: "{}".to_string(),
            },
        );

        assert_eq!(completed.len(), 2);
        assert_eq!(completed[0].id, "call-1");
        assert_eq!(completed[0].arguments, "{\"query\":\"new\"}");
        assert_eq!(completed[1].id, "call-2");
    }

    #[test]
    fn cancel_rejects_streaming_pending_tool_calls_with_results() {
        let mut conversation = AiConversation {
            id: "conv-1".to_string(),
            title: "Chat".to_string(),
            messages: vec![AiChatMessage {
                id: "assistant-1".to_string(),
                role: AiChatRole::Assistant,
                content: String::new(),
                timestamp_ms: 1,
                model: None,
                context: None,
                is_streaming: true,
                thinking_content: None,
                metadata: None,
                tool_call_id: None,
                tool_calls: vec![serde_json::json!({
                    "id": "call-1",
                    "name": "open_app_surface",
                    "arguments": "{}",
                    "status": "pending_user_approval",
                    "result": serde_json::Value::Null,
                })],
                turn: None,
                transcript_ref: None,
                summary_ref: None,
                branches: None,
            }],
            created_at_ms: 1,
            updated_at_ms: 1,
            origin: "sidebar".to_string(),
            profile_id: None,
            message_count: 1,
            session_id: None,
            session_metadata: None,
            messages_loaded: true,
        };

        reject_incomplete_ai_tool_calls_on_cancel(&mut conversation);

        let call = &conversation.messages[0].tool_calls[0];
        assert_eq!(call["status"], "rejected");
        assert_eq!(call["result"]["ok"], false);
        assert_eq!(
            call["result"]["error"]["message"],
            "Generation was stopped."
        );
        let parts = conversation.messages[0]
            .turn
            .as_ref()
            .and_then(|turn| turn.get("parts"))
            .and_then(serde_json::Value::as_array)
            .expect("turn parts");
        assert!(parts.iter().any(|part| {
            part.get("type").and_then(serde_json::Value::as_str) == Some("tool_result")
                && part
                    .get("toolCallId")
                    .and_then(serde_json::Value::as_str)
                    == Some("call-1")
        }));
    }
}

fn ai_estimated_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let cjk_count = text
        .chars()
        .filter(|ch| {
            matches!(
                *ch as u32,
                0x4e00..=0x9fff | 0x3040..=0x309f | 0x30a0..=0x30ff | 0xac00..=0xd7af
            )
        })
        .count();
    let non_cjk_count = text.chars().count().saturating_sub(cjk_count);
    ((cjk_count as f32 * 1.5 + non_cjk_count as f32 * 0.25) * 1.15).ceil() as usize
}

fn ai_response_reserve(context_window: usize) -> usize {
    (((context_window as f32) * 0.15).floor() as usize).min(4096)
}

const AI_HISTORY_BUDGET_RATIO: f32 = 0.7;
const AI_COMPACTION_TRIGGER_THRESHOLD: f32 = 0.80;
const AI_TRANSCRIPT_LOOKUP_THRESHOLD: f32 = 0.92;
const AI_TOOL_LOOP_STOP_THRESHOLD: f32 = 0.98;
const AI_MIN_PROMPT_SAFETY_MARGIN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AiPromptBudget {
    usable_prompt_budget: usize,
    history_budget: usize,
}

#[derive(Debug, Clone, Copy)]
struct AiPromptBudgetInput {
    context_window: usize,
    response_reserve: usize,
    system_budget: usize,
    history_tokens: usize,
    safety_margin: Option<usize>,
    trimmable_history_tokens: Option<usize>,
    summary_eligible_tokens: Option<usize>,
    can_summarize: bool,
    can_lookup_transcript: bool,
    in_tool_loop: bool,
    auto_compact_threshold: Option<f32>,
    transcript_lookup_threshold: Option<f32>,
    tool_loop_stop_threshold: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AiPromptBudgetDecision {
    level: u8,
    usage_ratio: f32,
    overage: usize,
}

fn compute_ai_prompt_budget(
    context_window: usize,
    response_reserve: usize,
    system_budget: usize,
    safety_margin: Option<usize>,
) -> AiPromptBudget {
    let safety_margin = safety_margin
        .unwrap_or_else(|| AI_MIN_PROMPT_SAFETY_MARGIN.max((context_window as f32 * 0.02).floor() as usize));
    let usable_prompt_budget = context_window
        .saturating_sub(response_reserve)
        .saturating_sub(safety_margin);
    AiPromptBudget {
        usable_prompt_budget,
        history_budget: usable_prompt_budget.saturating_sub(system_budget),
    }
}

fn determine_ai_compression_level(input: AiPromptBudgetInput) -> AiPromptBudgetDecision {
    let prompt_budget = compute_ai_prompt_budget(
        input.context_window,
        input.response_reserve,
        input.system_budget,
        input.safety_margin,
    );
    let total_prompt_tokens = input.system_budget.saturating_add(input.history_tokens);
    let overage = total_prompt_tokens.saturating_sub(prompt_budget.usable_prompt_budget);
    let usage_ratio = if prompt_budget.usable_prompt_budget > 0 {
        total_prompt_tokens as f32 / prompt_budget.usable_prompt_budget as f32
    } else {
        f32::INFINITY
    };
    let trimmable_history_tokens = input.trimmable_history_tokens.unwrap_or(input.history_tokens);
    let summary_eligible_tokens = input.summary_eligible_tokens.unwrap_or(input.history_tokens);
    let auto_compact_threshold = input
        .auto_compact_threshold
        .unwrap_or(AI_COMPACTION_TRIGGER_THRESHOLD);
    let transcript_lookup_threshold = input
        .transcript_lookup_threshold
        .unwrap_or(AI_TRANSCRIPT_LOOKUP_THRESHOLD);
    let tool_loop_stop_threshold = input
        .tool_loop_stop_threshold
        .unwrap_or(AI_TOOL_LOOP_STOP_THRESHOLD);

    let level = if overage == 0 {
        if input.in_tool_loop && usage_ratio >= tool_loop_stop_threshold {
            4
        } else if input.can_lookup_transcript && usage_ratio >= transcript_lookup_threshold {
            3
        } else if input.can_summarize
            && summary_eligible_tokens > 0
            && usage_ratio >= auto_compact_threshold
        {
            2
        } else {
            0
        }
    } else if trimmable_history_tokens >= overage && trimmable_history_tokens > 0 {
        1
    } else if input.can_summarize
        && summary_eligible_tokens > 0
        && usage_ratio >= auto_compact_threshold
    {
        2
    } else if input.can_lookup_transcript && usage_ratio >= transcript_lookup_threshold {
        3
    } else if input.in_tool_loop && usage_ratio >= tool_loop_stop_threshold {
        4
    } else if input.can_lookup_transcript {
        3
    } else if input.can_summarize && summary_eligible_tokens > 0 {
        2
    } else if input.in_tool_loop {
        4
    } else {
        1
    };

    AiPromptBudgetDecision {
        level,
        usage_ratio,
        overage,
    }
}

fn trim_ai_stream_history_to_budget(
    history: &mut Vec<AiChatMessage>,
    context_window: usize,
    response_reserve: usize,
) -> usize {
    if history.is_empty() {
        return 0;
    }
    let system_tokens = history
        .iter()
        .filter(|message| message.role == AiChatRole::System)
        .map(ai_message_estimated_tokens)
        .sum::<usize>();
    let budget = ((context_window as f32) * AI_HISTORY_BUDGET_RATIO)
        .floor() as usize;
    let budget = budget
        .saturating_sub(response_reserve)
        .saturating_sub(system_tokens);
    if budget == 0 {
        return 0;
    }

    let regular_indices = history
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            matches!(
                message.role,
                AiChatRole::User | AiChatRole::Assistant | AiChatRole::Tool
            )
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let total_regular = regular_indices.len();
    if total_regular <= 1 {
        return 0;
    }

    let mut kept_indices = std::collections::HashSet::<usize>::new();
    let mut used = 0usize;
    for index in regular_indices.iter().rev().copied() {
        let tokens = ai_message_estimated_tokens(&history[index]);
        if used.saturating_add(tokens) > budget && !kept_indices.is_empty() {
            break;
        }
        used = used.saturating_add(tokens);
        kept_indices.insert(index);
    }

    let kept_regular = kept_indices.len();
    if kept_regular >= total_regular {
        return 0;
    }
    *history = history
        .drain(..)
        .enumerate()
        .filter_map(|(index, message)| {
            (message.role == AiChatRole::System || kept_indices.contains(&index)).then_some(message)
        })
        .collect();
    total_regular.saturating_sub(kept_regular)
}

fn ai_user_memory_prompt(content: &str, enabled: bool) -> Option<String> {
    if !enabled {
        return None;
    }
    let content = oxideterm_ai::sanitize_for_ai(content).trim().to_string();
    if content.is_empty() {
        return None;
    }
    let truncated = truncate_at_char_boundary(&content, AI_USER_MEMORY_MAX_CHARS);
    let suffix = if truncated.len() < content.len() {
        "\n...[truncated]"
    } else {
        ""
    };
    Some(format!(
        "## User Memory\nThe following are long-lived user preferences explicitly saved by the user. Treat them as preferences and background context, not as facts about the current task. Current user instructions and visible context take priority.\n\n<user_memory>\n{truncated}{suffix}\n</user_memory>"
    ))
}

fn truncate_at_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    &text[..end]
}

fn ai_orchestrator_system_prompt(tool_use_enabled: bool) -> String {
    let tool_use_policy = if tool_use_enabled {
        [
            "- You are using the OxideSens task-tool orchestrator. You only see high-level task tools; do not invent low-level tool names or fake command output.",
            "- For broad remote-host discovery such as \"which hosts/connections are available\", call `list_targets` with `view: \"connections\"`. Do not call `select_target` for broad discovery.",
            "- Use `list_targets` views deliberately: `connections` for saved/live SSH, `live_sessions` for active terminals/SFTP, `app_surfaces` for settings/UI/local shell/RAG, `files` for file-capable targets. Use `all` only for debugging or last-resort fallback.",
            "- For a named object, call `select_target` first with a required enum `intent` unless the user already supplied an exact target_id.",
            "- Every action that runs, writes, transfers, or sends input must use an explicit target_id.",
            "- For knowledge-base, documentation, runbook, SOP, or plugin-development-document queries, select or use `rag-index:default`, then call `read_resource` with `resource=\"rag\"` and `query`. Do not use local shell, terminal commands, or connection discovery for knowledge searches.",
            "- Do not pass command text such as `pwd`, `docker ps`, `ls -la`, or `sudo ...` to `select_target`; first select the execution target, then call `run_command`.",
            "- Saved SSH connections are not live shells. To run a command there, call `connect_target` first, then `run_command` on the returned `ssh-node:*` or `terminal-session:*` target.",
            "- Never open a local terminal and type `ssh user@host` to connect a saved host unless the user explicitly asked for raw/manual ssh.",
            "- Treat old transcript target_id/session_id/tab_id values as untrusted unless the latest tool result has the same `meta.runtimeEpoch`, `meta.verified: true`, and the target still appears in current `list_targets`/`get_state` results.",
        ]
        .join("\n")
    } else {
        "TOOL CALLING IS CURRENTLY DISABLED. Do not emit tool calls or JSON tool schemas. If a task requires a tool, explain what you cannot access.".to_string()
    };
    [
        "## OxideSens Runtime Rules",
        "",
        "### Identity / Scope",
        "- You are OxideSens inside OxideTerm. Treat terminals, files, saved connections, and app surfaces as real user resources.",
        "- Do not claim something was connected, executed, read, modified, or verified until current context or a successful tool result proves it.",
        "- Current UI tab is only a ranking hint. It is not a capability boundary.",
        "",
        "### Terminal Safety",
        "- Never echo, display, or log secrets. Redact tokens, passwords, private keys, API keys, cookies, and credentials from command output.",
        "- Dangerous commands must not be casual suggestions. Explain the risk and require explicit user confirmation before destructive, privileged, credential-sensitive, or service-impacting operations.",
        "- Do not guess passwords, passphrases, sudo prompts, host key answers, or interactive confirmation input.",
        "- If a result has `waitingForInput`, stop and tell the user what input is needed. Do not repeat the command.",
        "",
        "### Tool Use Rules",
        &tool_use_policy,
        "",
        "### Command Execution Rules",
        "- Commands that may use a pager must be made non-interactive: use forms such as `git --no-pager log`, `git --no-pager diff`, `GIT_PAGER=cat`, `journalctl --no-pager`, `systemctl --no-pager`, or pipe `man`/`less`-style output through bounded commands like `col -b | head`.",
        "- If a command or tool fails, read the error carefully and adapt the next step. Do not repeat the same failing call unchanged.",
        "- Prefer bounded, inspectable commands before broad writes or deletes.",
        "",
        "### Output Handling",
        "- If tool output is truncated, sampled, or incomplete, explicitly say what part you could see and that conclusions are limited by truncation.",
        "- Do not ask the user to manually create, copy, or paste files to report results when tools can read or write them. Use tool calls or answer directly.",
    ]
    .join("\n")
}

fn ai_context_window_from_maps(
    user_context_windows: &serde_json::Map<String, serde_json::Value>,
    model_context_windows: &serde_json::Map<String, serde_json::Value>,
    provider_id: &str,
    model: &str,
) -> Option<usize> {
    usize::try_from(oxideterm_ai::model_context_window(
        model,
        model_context_windows,
        Some(provider_id),
        user_context_windows,
    ))
    .ok()
    .filter(|tokens| *tokens > 0)
}

fn ai_tool_use_policy_from_settings(
    settings: &oxideterm_settings::AiToolUseSettings,
) -> AiToolUsePolicy {
    tool_policy_from_parts(
        settings.enabled,
        settings
            .auto_approve_tools
            .iter()
            .filter_map(|(key, value)| value.as_bool().map(|enabled| (key.clone(), enabled))),
        settings.disabled_tools.clone(),
        settings.max_rounds,
        settings.max_calls_per_round,
    )
}

fn ai_reasoning_effort_value(effort: oxideterm_settings::AiReasoningEffort) -> Option<String> {
    serde_json::to_value(effort)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .map(|value| match value.as_str() {
            "none" | "minimal" => "off".to_string(),
            "xhigh" => "max".to_string(),
            other => other.to_string(),
        })
}

fn ai_conversation_message_tokens(conversation: &AiConversation) -> usize {
    conversation
        .messages
        .iter()
        .filter(|message| {
            matches!(
                message.role,
                AiChatRole::User | AiChatRole::Assistant | AiChatRole::Tool
            )
        })
        .map(ai_message_estimated_tokens)
        .sum()
}

fn ai_context_percentage(tokens: usize, max_tokens: usize) -> f32 {
    if max_tokens == 0 {
        return 0.0;
    }
    ((tokens as f32 / max_tokens as f32) * 100.0).min(100.0)
}

const AI_CONTEXT_WARNING_PERCENT: f32 = 70.0;
const AI_CONTEXT_DANGER_PERCENT: f32 = 85.0;
const AI_COMPACTION_DEFAULT_CONTEXT_WINDOW: usize = oxideterm_ai::DEFAULT_CONTEXT_WINDOW as usize;
const AI_USER_MEMORY_MAX_CHARS: usize = 6_000;
const DEFAULT_AI_SYSTEM_PROMPT: &str = r#"You are OxideSens, a terminal-aware assistant inside OxideTerm.

## Identity / Scope
- Help with shell commands, scripts, terminal output, files, connections, and OxideTerm workflows.
- Be concise, direct, and honest about what you can verify.
- Do not claim that you connected, executed, changed, read, or verified anything unless the available context or a successful tool result proves it.

## Terminal Safety
- Treat terminal actions as real operations on the user's machine or remote hosts.
- Do not present dangerous commands as casual suggestions. For destructive, privileged, credential-sensitive, or service-impacting commands, explain the risk first and require explicit user confirmation.
- Never echo, display, or log secrets. If command output contains tokens, passwords, private keys, API keys, cookies, or credentials, redact them in your response.
- Do not guess passwords, passphrases, sudo prompts, host key answers, or interactive confirmation input.

## Output Handling
- If output is incomplete, sampled, or truncated, say that your conclusion is limited to the visible output.
- If a command or tool fails, read the error, explain the likely cause, and adapt the next step. Do not repeat the same failing command unchanged.
- When commands may invoke pagers, prefer non-pager forms such as `git --no-pager ...`, `GIT_PAGER=cat`, `journalctl --no-pager`, `man ... | col -b | head`, or command-specific no-pager flags.

## Response Style
- Prefer actionable answers over long theory.
- When tools or file access are available, do not ask the user to manually copy text into files just to complete a task; use the available mechanisms or answer directly.
- Format commands and paths clearly in markdown."#;
const AI_SUGGESTIONS_INSTRUCTION: &str = r#"

## Follow-Up Suggestions

At the END of your response, optionally include 2-4 follow-up suggestions the user might want to try next. Use this exact XML format:

<suggestions>
<s icon="IconName">Short actionable suggestion text</s>
</suggestions>

Rules:
- Only include suggestions when they add value (skip for simple greetings or one-off answers)
- Keep each suggestion under 60 characters
- Use Lucide icon names: Zap, Search, Bug, FileCode, Terminal, Settings, RefreshCw, Shield, BarChart, GitBranch, Download, Upload, Eye, Wrench, Play
- Suggestions must be contextually relevant to the conversation"#;
