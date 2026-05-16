impl WorkspaceApp {
    fn start_ai_chat_stream(
        &mut self,
        conversation_id: String,
        config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let rag_query = if self.resolved_ai_execution_profile().include_rag {
            request_content.clone()
        } else {
            None
        };
        let Some((history, trimmed_count)) = self.build_ai_stream_history(
            &conversation_id,
            &config,
            request_content,
            task_system_prompt,
        ) else {
            return;
        };
        if trimmed_count > 0 {
            self.show_ai_trim_notice(trimmed_count, cx);
        }
        let now = ai_now_ms();
        let assistant_id = self.next_ai_chat_id(now);
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
        let mut history = self
            .ai_chat
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .map(|conversation| conversation.messages.clone())?;
        apply_chat_request_overrides(&mut history, request_content, task_system_prompt);
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
                    });
            }
            AiStreamEvent::Thinking(chunk) => {
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        message
                            .thinking_content
                            .get_or_insert_with(String::new)
                            .push_str(&chunk);
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
                    });
            }
            AiStreamEvent::Done => {
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        message.is_streaming = false;
                    });
                self.ai_chat_stream_task = None;
                self.ai_chat_loading = false;
                self.persist_ai_chat_state();
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
                    });
                self.ai_chat_stream_task = None;
                self.ai_chat_loading = false;
                self.persist_ai_chat_state();
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
            }
        }
        cx.notify();
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
        cx: &mut Context<Self>,
    ) {
        if self.ai_chat_stream_generation != generation {
            return;
        }
        self.ai_chat
            .update_message(conversation_id, message_id, |message| {
                update_ai_tool_call_status(
                    message,
                    tool_call_id,
                    name,
                    arguments,
                    status,
                    result,
                    risk,
                    summary,
                );
            });
        cx.notify();
    }

    fn start_ai_compact_conversation(&mut self, cx: &mut Context<Self>) {
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
        let context_window = self.ai_active_model_context_window(&config);
        let Some(plan) = ai_compaction_plan(&conversation.messages, context_window) else {
            self.ai_compacting_conversations.remove(&conversation.id);
            return;
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
            });
        });
        self.schedule_ai_compaction_poll(cx);
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
                AiStreamDeliveryEvent::ToolStatus {
                    tool_call_id,
                    name,
                    arguments,
                    status,
                    result,
                    risk,
                    summary,
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
        cx: &mut Context<Self>,
    ) {
        self.ai_compacting_conversations.remove(&conversation_id);
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
        let anchor_id = self.next_ai_chat_id(now);
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
            .take(base_ids.len())
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
        let appended = conversation
            .messages
            .iter()
            .skip(base_ids.len())
            .cloned()
            .collect::<Vec<_>>();
        let anchor = AiChatMessage {
            id: anchor_id,
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
            transcript_ref: None,
            summary_ref: None,
            branches: None,
        };
        conversation.messages = std::iter::once(anchor)
            .chain(plan.keep_messages)
            .chain(appended)
            .collect();
        conversation.updated_at_ms = now;
        self.persist_ai_chat_state();
        cx.notify();
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
            id: summary_id,
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
) {
    upsert_ai_tool_call(message, id, name, arguments, status);
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
    let budget = context_window
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
