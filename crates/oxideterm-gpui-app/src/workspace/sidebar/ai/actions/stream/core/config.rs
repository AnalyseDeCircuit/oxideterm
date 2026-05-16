impl WorkspaceApp {
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
}
