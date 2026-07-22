pub(in crate::workspace) fn ai_stream_tool_definitions(
    tool_use_enabled: bool,
    tool_policy: &oxideterm_ai::AiToolUsePolicy,
    mcp_registry: &oxideterm_ai::McpRegistry,
) -> Vec<oxideterm_ai::AiToolDefinition> {
    if !tool_use_enabled {
        return Vec::new();
    }
    let mut tools = oxideterm_ai::orchestrator_tool_definitions();
    // Native does not ship Tauri's autonomous agent path yet. Expose MCP
    // resource/dynamic tools through chat as a native-only bridge so MCP
    // remains usable from the primary AI surface.
    tools.extend(mcp_registry.tool_definitions().into_iter().filter(|tool| {
        !tool_policy
            .disabled_tools
            .iter()
            .any(|name| name == &tool.name)
    }));
    tools
}

impl WorkspaceApp {
    pub(in crate::workspace) fn should_force_ai_pre_send_compaction(
        &self,
        conversation_id: &str,
        config: &AiChatStreamConfig,
        request_content: Option<&str>,
        task_system_prompt: Option<&str>,
        rag_system_prompt: Option<&str>,
    ) -> bool {
        let Some(conversation) = self
            .ai
            .chat
            .conversation_state
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return false;
        };
        let Some(decision) = self.ai_send_budget_decision(
            conversation,
            config,
            request_content,
            task_system_prompt,
            rag_system_prompt,
        ) else {
            return false;
        };
        decision.level >= 2
            && ai_find_prompt_transcript_lookup_reference(&conversation.messages).is_none()
    }

    pub(in crate::workspace) fn resolve_ai_stream_config(
        &self,
    ) -> Result<AiChatStreamConfig, String> {
        let settings = self.settings_store.settings();
        let tool_policy = ai_tool_use_policy_from_settings(&settings.ai.tool_use);
        if settings.ai.active_backend == AiActiveBackend::Acp {
            let acp_agent_id = settings
                .ai
                .active_acp_agent_id
                .clone()
                .filter(|agent_id| !agent_id.trim().is_empty())
                .ok_or_else(|| "No ACP agent selected.".to_string())?;
            let session_state = self.active_ai_acp_session_state(&acp_agent_id);
            let model_label = session_state
                .as_ref()
                .and_then(|state| oxideterm_ai::acp_model_config_option(&state.config_options))
                .and_then(|option| {
                    oxideterm_ai::acp_selected_config_choice(
                        option,
                        session_state
                            .as_ref()
                            .and_then(|state| state.model_selection.as_ref()),
                    )
                    .map(|choice| choice.label.clone())
                })
                .unwrap_or_else(|| self.ai_acp_agent_model_fallback_label(&acp_agent_id));
            return Ok(AiChatStreamConfig {
                execution_backend: AiExecutionBackend::Acp,
                provider_id: None,
                acp_agent_id: Some(acp_agent_id),
                acp_session_id: session_state.as_ref().map(|state| state.session_id.clone()),
                acp_config_selection: session_state.and_then(|state| state.model_selection),
                provider_type: "acp".to_string(),
                base_url: String::new(),
                model: model_label,
                api_key: None,
                max_response_tokens: None,
                reasoning_effort: None,
                safety_mode: match self.active_ai_safety_mode() {
                    AiSafetyMode::Bypass => AiPolicySafetyMode::Bypass,
                    AiSafetyMode::Default => AiPolicySafetyMode::Default,
                },
                profile_id: None,
                tool_policy,
                tools: Vec::new(),
                tool_choice: oxideterm_ai::AiToolChoice::Auto,
            });
        }

        let providers = ai_provider_views(&settings.ai.providers);
        let provider = active_provider_view(&providers, settings.ai.active_provider_id.as_deref())
            .cloned()
            .ok_or_else(|| self.i18n.t("ai.model_selector.no_provider"))?;
        let model =
            active_model_or_provider_default(settings.ai.active_model.as_deref(), &provider)
                .ok_or_else(|| {
                    "No model selected. Please refresh models or select one in Settings > AI."
                        .to_string()
                })?;
        let max_response_tokens =
            ai_chat_request_max_response_tokens(settings, &provider.id, &model);
        let reasoning_effort = oxideterm_ai::resolve_ai_reasoning_effort(
            ai_reasoning_effort_value(settings.ai.reasoning_effort).as_deref(),
            &settings.ai.reasoning_provider_overrides,
            &settings.ai.reasoning_model_overrides,
            Some(&provider.id),
            Some(&model),
        );
        let tools = ai_stream_tool_definitions(
            tool_policy.enabled,
            &tool_policy,
            &self.ai.runtime.mcp_registry,
        );
        Ok(AiChatStreamConfig {
            execution_backend: AiExecutionBackend::Provider,
            provider_id: Some(provider.id),
            acp_agent_id: None,
            acp_session_id: None,
            acp_config_selection: None,
            provider_type: provider.provider_type,
            base_url: provider.base_url,
            model,
            api_key: None,
            max_response_tokens,
            reasoning_effort: Some(reasoning_effort),
            safety_mode: match self.active_ai_safety_mode() {
                AiSafetyMode::Bypass => AiPolicySafetyMode::Bypass,
                AiSafetyMode::Default => AiPolicySafetyMode::Default,
            },
            profile_id: None,
            tool_policy,
            tools,
            tool_choice: oxideterm_ai::AiToolChoice::Auto,
        })
    }

    pub(in crate::workspace) fn resolve_ai_summary_stream_config(
        &self,
        compact: bool,
    ) -> Result<AiChatStreamConfig, String> {
        let settings = self.settings_store.settings();
        let providers = ai_provider_views(&settings.ai.providers);
        let provider = active_provider_view(&providers, settings.ai.active_provider_id.as_deref())
            .cloned()
            .ok_or_else(|| self.i18n.t("ai.model_selector.no_provider"))?;
        let model =
            active_model_or_provider_default(settings.ai.active_model.as_deref(), &provider)
                .ok_or_else(|| {
                    "No model selected. Please refresh models or select one in Settings > AI."
                        .to_string()
                })?;
        let max_response_tokens = if compact {
            ai_model_max_response_tokens(
                &settings.ai.model_max_response_tokens,
                &provider.id,
                &model,
            )
            .or_else(|| {
                let context_window = oxideterm_ai::model_context_window(
                    &model,
                    &settings.ai.model_context_windows,
                    Some(&provider.id),
                    &settings.ai.user_context_windows,
                )
                .try_into()
                .ok()
                .filter(|value: &usize| *value > 0)
                .unwrap_or(AI_COMPACTION_DEFAULT_CONTEXT_WINDOW);
                i64::try_from(ai_response_reserve(context_window)).ok()
            })
        } else {
            None
        };
        let reasoning_effort = oxideterm_ai::resolve_ai_reasoning_effort(
            ai_reasoning_effort_value(settings.ai.reasoning_effort).as_deref(),
            &settings.ai.reasoning_provider_overrides,
            &settings.ai.reasoning_model_overrides,
            Some(&provider.id),
            Some(&model),
        );
        Ok(AiChatStreamConfig {
            execution_backend: AiExecutionBackend::Provider,
            provider_id: Some(provider.id),
            acp_agent_id: None,
            acp_session_id: None,
            acp_config_selection: None,
            provider_type: provider.provider_type,
            base_url: provider.base_url,
            model,
            api_key: None,
            max_response_tokens,
            reasoning_effort: Some(reasoning_effort),
            safety_mode: match self.active_ai_safety_mode() {
                AiSafetyMode::Bypass => AiPolicySafetyMode::Bypass,
                AiSafetyMode::Default => AiPolicySafetyMode::Default,
            },
            profile_id: None,
            tool_policy: AiToolUsePolicy::default(),
            tools: Vec::new(),
            tool_choice: oxideterm_ai::AiToolChoice::Auto,
        })
    }

    pub(in crate::workspace) fn active_ai_acp_session_state(
        &self,
        agent_id: &str,
    ) -> Option<AiAcpSessionState> {
        self.ai
            .chat
            .conversation_state
            .active_conversation()
            .and_then(ai_acp_session_state)
            .filter(|state| state.agent_id == agent_id)
    }

    pub(in crate::workspace) fn build_ai_stream_history(
        &self,
        conversation_id: &str,
        config: &AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        rag_system_prompt: Option<String>,
    ) -> Option<(Vec<AiChatMessage>, usize)> {
        let transcript_lookup_prompt = self.ai_transcript_lookup_prompt_for_conversation(
            conversation_id,
            config,
            request_content.as_deref(),
            task_system_prompt.as_deref(),
            rag_system_prompt.as_deref(),
        );
        let mut history = self
            .ai
            .chat
            .conversation_state
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .map(|conversation| conversation.messages.clone())?;
        apply_chat_request_overrides(&mut history, request_content, None);
        normalize_ai_stream_history_for_provider(&mut history);
        let mut base_system_prompt = self.build_ai_base_system_prompt(
            config,
            rag_system_prompt.as_deref(),
            task_system_prompt.as_deref(),
        );
        if let Some(prompt) = ai_orchestrator_obligation_prompt_for_history(config, &history) {
            base_system_prompt.push_str("\n\n");
            base_system_prompt.push_str(&prompt);
        }
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
                suggestions: Vec::new(),
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
                    suggestions: Vec::new(),
                },
            );
        }
        let trimmed_count = trim_ai_stream_history_to_budget(
            &mut history,
            context_window,
            config
                .max_response_tokens
                .and_then(|tokens| usize::try_from(tokens).ok())
                .filter(|tokens| *tokens > 0)
                .unwrap_or_else(|| ai_response_reserve(context_window)),
        );
        Some((history, trimmed_count))
    }

    pub(in crate::workspace) fn ai_send_budget_decision(
        &self,
        conversation: &AiConversation,
        config: &AiChatStreamConfig,
        request_content: Option<&str>,
        task_system_prompt: Option<&str>,
        rag_system_prompt: Option<&str>,
    ) -> Option<AiPromptBudgetDecision> {
        let context_window = self.ai_active_model_context_window(config);
        let response_reserve = config
            .max_response_tokens
            .and_then(|tokens| usize::try_from(tokens).ok())
            .filter(|tokens| *tokens > 0)
            .unwrap_or_else(|| ai_response_reserve(context_window));
        let base_system_tokens = ai_estimated_tokens(&self.build_ai_base_system_prompt(
            config,
            rag_system_prompt,
            task_system_prompt,
        ))
        .saturating_add(ai_tool_definitions_estimated_tokens(&config.tools));
        let obligation_tokens = request_content
            .map(str::to_string)
            .or_else(|| {
                conversation
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == AiChatRole::User)
                    .map(|message| message.content.clone())
            })
            .and_then(|request| ai_orchestrator_obligation_prompt_for_text(config, &request))
            .map(|prompt| ai_estimated_tokens(&prompt))
            .unwrap_or(0);
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
            system_budget: base_system_tokens
                .saturating_add(obligation_tokens)
                .saturating_add(anchor_tokens),
            history_tokens,
            trimmable_history_tokens: Some(history_tokens),
            summary_eligible_tokens: Some(summary_eligible_tokens),
            can_summarize: summary_eligible_tokens > 0,
            can_lookup_transcript: ai_find_prompt_transcript_lookup_reference(
                &conversation.messages,
            )
            .is_some(),
            in_tool_loop: false,
            auto_compact_threshold: None,
            transcript_lookup_threshold: None,
            tool_loop_stop_threshold: None,
            safety_margin: None,
        }))
    }

    pub(in crate::workspace) fn ai_budget_diagnostic_payload(
        &self,
        conversation: &AiConversation,
        config: &AiChatStreamConfig,
        request_content: Option<&str>,
        task_system_prompt: Option<&str>,
        rag_system_prompt: Option<&str>,
        decision: Option<AiPromptBudgetDecision>,
        trimmed_count: usize,
    ) -> serde_json::Value {
        let base_system_tokens = ai_estimated_tokens(&self.build_ai_base_system_prompt(
            config,
            rag_system_prompt,
            task_system_prompt,
        ))
        .saturating_add(ai_tool_definitions_estimated_tokens(&config.tools));
        let obligation_tokens = request_content
            .map(str::to_string)
            .or_else(|| {
                conversation
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == AiChatRole::User)
                    .map(|message| message.content.clone())
            })
            .and_then(|request| ai_orchestrator_obligation_prompt_for_text(config, &request))
            .map(|prompt| ai_estimated_tokens(&prompt))
            .unwrap_or(0);
        let anchor_tokens = conversation
            .messages
            .iter()
            .filter(|message| is_ai_compaction_anchor(message))
            .map(ai_message_estimated_tokens)
            .sum::<usize>();
        let history_tokens = conversation
            .messages
            .iter()
            .filter(|message| !is_ai_compaction_anchor(message))
            .map(ai_message_estimated_tokens)
            .sum::<usize>();
        let transcript_lookup_tokens = decision
            .filter(|decision| decision.level >= 3)
            .and_then(|_| ai_find_prompt_transcript_lookup_reference(&conversation.messages))
            .map(ai_build_transcript_lookup_prompt_reference)
            .map(|prompt| ai_estimated_tokens(&prompt))
            .unwrap_or(0);
        let previous_level = conversation
            .session_metadata
            .as_ref()
            .and_then(|metadata| metadata.get("lastBudgetLevel"))
            .and_then(serde_json::Value::as_i64);
        serde_json::json!({
            "requestKind": "chat",
            "budgetLevel": decision.map(|decision| decision.level).unwrap_or(0),
            "previousLevel": previous_level,
            "nextLevel": decision.map(|decision| decision.level).unwrap_or(0),
            "contextWindow": self.ai_active_model_context_window(config),
            "responseReserve": config.max_response_tokens,
            "systemBudget": base_system_tokens
                .saturating_add(obligation_tokens)
                .saturating_add(anchor_tokens)
                .saturating_add(transcript_lookup_tokens),
            "historyTokens": history_tokens,
            "trimmedCount": trimmed_count,
        })
    }

    pub(in crate::workspace) fn ai_transcript_lookup_prompt_for_conversation(
        &self,
        conversation_id: &str,
        config: &AiChatStreamConfig,
        request_content: Option<&str>,
        task_system_prompt: Option<&str>,
        rag_system_prompt: Option<&str>,
    ) -> Option<String> {
        let conversation = self
            .ai
            .chat
            .conversation_state
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)?;
        let decision = self.ai_send_budget_decision(
            conversation,
            config,
            request_content,
            task_system_prompt,
            rag_system_prompt,
        )?;
        (decision.level >= 3)
            .then(|| ai_find_prompt_transcript_lookup_reference(&conversation.messages))
            .flatten()
            .map(ai_build_transcript_lookup_prompt_reference)
    }

    pub(in crate::workspace) fn show_ai_trim_notice(
        &mut self,
        count: usize,
        cx: &mut Context<Self>,
    ) {
        self.ai.chat.context_trim_notice_count = Some(count);
        self.ai.chat.context_trim_notice_sequence =
            self.ai.chat.context_trim_notice_sequence.saturating_add(1);
        let sequence = self.ai.chat.context_trim_notice_sequence;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_secs(5)).await;
            let _ = weak.update(cx, |this, cx| {
                if this.ai.chat.context_trim_notice_sequence == sequence {
                    this.ai.chat.context_trim_notice_count = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn persist_ai_transcript_entries(
        &self,
        conversation_id: String,
        entries: Vec<oxideterm_ai::PersistedTranscriptEntry>,
    ) {
        if entries.is_empty() {
            return;
        }
        let Some(store) = self.ai.chat.persistence_store.clone() else {
            return;
        };
        self.forwarding_runtime.spawn_blocking(move || {
            if let Err(error) = store.append_transcript_entries(&conversation_id, &entries) {
                eprintln!("[AiChatStore] Failed to persist transcript entries: {error}");
            }
        });
    }

    pub(in crate::workspace) fn persist_ai_diagnostic_events(
        &self,
        conversation_id: String,
        events: Vec<oxideterm_ai::PersistedDiagnosticEvent>,
    ) {
        if events.is_empty() {
            return;
        }
        let Some(store) = self.ai.chat.persistence_store.clone() else {
            return;
        };
        self.forwarding_runtime.spawn_blocking(move || {
            if let Err(error) = store.append_diagnostic_events(&conversation_id, &events) {
                eprintln!("[AiChatStore] Failed to persist diagnostic events: {error}");
            }
        });
    }

    pub(in crate::workspace) fn ai_diagnostic_base(
        &self,
        data: serde_json::Value,
    ) -> serde_json::Value {
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
            serde_json::json!(self.settings_store.settings().ai.tool_use.enabled),
        );
        if let Some(provider_id) = self
            .settings_store
            .settings()
            .ai
            .active_provider_id
            .as_ref()
        {
            object.insert("providerId".to_string(), serde_json::json!(provider_id));
        }
        if let Some(model) = self.settings_store.settings().ai.active_model.as_ref() {
            object.insert("model".to_string(), serde_json::json!(model));
        }
        serde_json::Value::Object(object)
    }

    pub(in crate::workspace) fn build_ai_base_system_prompt(
        &self,
        config: &AiChatStreamConfig,
        rag_system_prompt: Option<&str>,
        task_system_prompt: Option<&str>,
    ) -> String {
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
        if let Some(memory) =
            ai_user_memory_prompt(&settings.ai.memory.content, settings.ai.memory.enabled)
        {
            prompt.push_str("\n\n");
            prompt.push_str(&memory);
        }
        if let Some(rag_system_prompt) = rag_system_prompt
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
        {
            prompt.push_str("\n\n");
            prompt.push_str(rag_system_prompt);
        }
        if let Some(task_system_prompt) = task_system_prompt
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
        {
            prompt.push_str("\n\n");
            prompt.push_str(task_system_prompt);
        }
        if self.ai_active_model_context_window(config) >= 8192 {
            prompt.push_str(AI_SUGGESTIONS_INSTRUCTION);
        }
        prompt.push_str("\n\n");
        prompt.push_str(&ai_orchestrator_system_prompt(config.tool_policy.enabled));
        prompt
    }
}

pub(in crate::workspace) fn ai_chat_request_max_response_tokens(
    settings: &oxideterm_settings::PersistedSettings,
    provider_id: &str,
    model: &str,
) -> Option<i64> {
    ai_model_max_response_tokens(&settings.ai.model_max_response_tokens, provider_id, model)
        .or_else(|| {
            let context_window = oxideterm_ai::model_context_window(
                model,
                &settings.ai.model_context_windows,
                Some(provider_id),
                &settings.ai.user_context_windows,
            );
            i64::try_from(ai_response_reserve(
                usize::try_from(context_window)
                    .ok()
                    .filter(|tokens| *tokens > 0)
                    .unwrap_or(AI_COMPACTION_DEFAULT_CONTEXT_WINDOW),
            ))
            .ok()
        })
}

pub(in crate::workspace) fn ai_orchestrator_obligation_prompt_for_history(
    config: &AiChatStreamConfig,
    history: &[AiChatMessage],
) -> Option<String> {
    history
        .iter()
        .rev()
        .find(|message| message.role == AiChatRole::User)
        .and_then(|message| ai_orchestrator_obligation_prompt_for_text(config, &message.content))
}

pub(in crate::workspace) fn ai_orchestrator_obligation_prompt_for_text(
    config: &AiChatStreamConfig,
    request_text: &str,
) -> Option<String> {
    config
        .tool_policy
        .enabled
        .then(|| ai_classify_orchestrator_obligation(request_text))
        .as_ref()
        .and_then(ai_orchestrator_obligation_prompt)
}

#[cfg(test)]
pub(in crate::workspace) fn ai_insert_rag_prompt_before_runtime_tail(
    system_prompt: &mut String,
    rag_prompt: &str,
) {
    let insert_at = [
        "\n\n## Follow-Up Suggestions",
        "\n\n## OxideSens Runtime Rules",
    ]
    .into_iter()
    .filter_map(|marker| system_prompt.find(marker))
    .min()
    .unwrap_or(system_prompt.len());
    let insertion = format!("\n\n{rag_prompt}");
    system_prompt.insert_str(insert_at, &insertion);
}
