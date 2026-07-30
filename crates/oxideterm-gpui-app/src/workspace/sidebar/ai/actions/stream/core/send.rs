impl AiWorkspaceEntity {
    pub(in crate::workspace) fn begin_assistant_turn(
        &mut self,
        conversation_id: &str,
        message: AiChatMessage,
        budget_level: u8,
        backend: oxideterm_ai::AiMessageBackendProvenance,
    ) {
        let message_id = message.id.clone();
        self.conversation_state_mut()
            .add_message(conversation_id, message);
        if let Some(conversation) = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        {
            let metadata = conversation.session_metadata.get_or_insert_with(|| {
                serde_json::json!({ "conversationId": conversation_id })
            });
            if let Some(object) = metadata.as_object_mut() {
                object.insert(
                    "conversationId".to_string(),
                    serde_json::json!(conversation_id),
                );
                object.insert("origin".to_string(), serde_json::json!("sidebar"));
                object.insert(
                    "lastBudgetLevel".to_string(),
                    serde_json::json!(budget_level),
                );
            }
            // The backend owner is persisted separately from display model
            // text so mixed provider/ACP history can be synchronized exactly.
            oxideterm_ai::store_ai_message_backend_provenance(
                conversation,
                &message_id,
                backend,
            );
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn start_acp_chat_thread(
        &mut self,
        conversation_id: String,
        config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let launch = match self.ai_acp_chat_launch(&config).and_then(|launch| {
            launch.ok_or_else(|| "ACP launch configuration was not prepared.".to_string())
        }) {
            Ok(launch) => launch,
            Err(_) => {
                self.ai_entity.update(cx, |ai, _cx| ai.set_chat_loading(false));
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.ai.acp_agent_error_unknown"),
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
        };
        let request_message = self
            .ai_entity
            .read(cx)
            .conversation_state()
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
        let user_request = request_content
            .or_else(|| request_message.as_ref().map(|message| message.content.clone()))
            .filter(|request| !request.trim().is_empty());
        let Some(user_request) = user_request else {
            self.push_ai_settings_toast(
                self.i18n.t("settings_view.ai.acp_agent_error_unknown"),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        };

        let handoff = self.ai_entity.read(cx).conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(|conversation| {
                let cursor = ai_acp_session_state(conversation)
                    .filter(|state| state.agent_id == launch.launch_config.id)
                    .and_then(|state| state.handoff_cursor);
                oxideterm_ai::build_acp_conversation_handoff(
                    conversation,
                    request_message.as_ref().map(|message| message.id.as_str())?,
                    cursor.as_ref(),
                )
            });
        let mut prompt = zeroize::Zeroizing::new(String::new());
        if let Some(handoff) = handoff {
            prompt.push_str(handoff.as_str());
        }
        let mut append_prompt_section = |heading: &str, value: &str| {
            let safe_value = zeroize::Zeroizing::new(oxideterm_ai::sanitize_for_ai(value));
            if !prompt.is_empty() {
                prompt.push_str("\n\n");
            }
            prompt.push_str("## ");
            prompt.push_str(heading);
            prompt.push('\n');
            prompt.push_str(safe_value.as_str());
        };
        if let Some(instructions) = task_system_prompt.filter(|value| !value.trim().is_empty()) {
            append_prompt_section("OxideTerm Instructions", &instructions);
        }
        if let Some(context) = request_message
            .as_ref()
            .and_then(|message| message.context.as_deref())
            .filter(|value| !value.trim().is_empty())
        {
            append_prompt_section("OxideTerm Current Context", context);
        }
        append_prompt_section("User Request", &user_request);
        drop(append_prompt_section);

        let now = ai_now_ms();
        let assistant_id = self.next_ai_chat_id(now, cx);
        let backend = ai_message_backend_for_stream(&config);
        self.ai_entity.update(cx, |ai, _cx| {
            ai.begin_assistant_turn(
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
                    suggestions: Vec::new(),
                },
                0,
                backend,
            );
            ai.set_chat_loading(true);
        });
        let (generation, _) = self
            .ai_entity
            .update(cx, |ai, _cx| ai.begin_chat_stream());
        let turn_id = format!("{conversation_id}:{generation}");
        let mut config_selections = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(ai_acp_session_state)
            .map(|state| state.config_selections)
            .unwrap_or_default();
        if config_selections.is_empty()
            && let Some(model_selection) = config.acp_config_selection
        {
            config_selections.push(model_selection);
        }
        let mode_id = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .and_then(ai_acp_session_state)
            .and_then(|state| state.current_mode_id);
        let started = self.acp_entity.update(cx, |entity, _cx| {
            entity.start_turn(crate::workspace::acp_workspace::AcpThreadStart {
                route: crate::workspace::acp_workspace::AcpTurnRoute {
                    generation,
                    conversation_id: conversation_id.clone(),
                    assistant_id: assistant_id.clone(),
                },
                launch_config: launch.launch_config,
                host_policy: launch.host_policy,
                request: oxideterm_ai::AcpManagedPromptRequest {
                    thread_id: conversation_id.clone(),
                    turn_id,
                    existing_session_id: config.acp_session_id,
                    cwd: launch.session_cwd,
                    config_selections,
                    mode_id,
                    mcp_servers: Vec::new(),
                    prompt,
                },
            })
        });
        if !started {
            self.ai_entity.update(cx, |ai, _cx| {
                ai.enqueue_chat_stream_delivery(AiStreamDelivery {
                    generation,
                    conversation_id,
                    assistant_id,
                    event: AiStreamDeliveryEvent::Stream(AiStreamEvent::Error(
                        "stream_failed".to_string(),
                    )),
                });
            });
        }
        cx.notify();
    }

    pub(in crate::workspace) fn start_ai_chat_stream_after_rag_lookup(
        &mut self,
        conversation_id: String,
        config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let rag_query = request_content
            .clone()
            .filter(|query| query.trim().chars().count() >= 4);
        let Some(rag_query) = rag_query else {
            self.start_ai_chat_stream_after_budget_preflight(
                conversation_id,
                config,
                request_content,
                task_system_prompt,
                None,
                true,
                cx,
            );
            return;
        };

        let services = self.ai_model_backend_services(cx);
        let (rag_tx, rag_rx) = std::sync::mpsc::channel();
        self.forwarding_runtime.spawn(async move {
            let rag_system_prompt = tokio::time::timeout(
                std::time::Duration::from_millis(3000),
                services.build_rag_system_prompt(Some(&rag_query), &config),
            )
            .await
            .ok()
            .flatten();
            // Return the unique configuration with the lookup result instead
            // of cloning a handle that extends the provider-key lifetime.
            let _ = rag_tx.send((config, rag_system_prompt));
        });
        cx.spawn(async move |weak, cx| {
            let Some((config, rag_system_prompt)) = (loop {
                match rag_rx.try_recv() {
                    Ok(result) => break Some(result),
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        Timer::after(Duration::from_millis(16)).await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break None,
                }
            }) else {
                return;
            };
            let _ = weak.update(cx, |this, cx| {
                this.start_ai_chat_stream_after_budget_preflight(
                    conversation_id,
                    config,
                    request_content,
                    task_system_prompt,
                    rag_system_prompt,
                    true,
                    cx,
                );
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn start_ai_chat_stream_after_budget_preflight(
        &mut self,
        conversation_id: String,
        config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        rag_system_prompt: Option<String>,
        allow_pre_send_compaction: bool,
        cx: &mut Context<Self>,
    ) {
        if allow_pre_send_compaction
            && self.should_force_ai_pre_send_compaction(
                &conversation_id,
                &config,
                request_content.as_deref(),
                task_system_prompt.as_deref(),
                rag_system_prompt.as_deref(),
                cx,
            )
        {
            let pending = AiPendingChatStream {
                conversation_id: conversation_id.clone(),
                config,
                request_content,
                task_system_prompt,
                rag_system_prompt,
            };
            let pending = match self.start_ai_compact_conversation_for(
                conversation_id,
                true,
                true,
                Some(pending),
                cx,
            ) {
                Ok(()) => return,
                Err(Some(pending)) => pending,
                Err(None) => return,
            };

            return self.start_ai_chat_stream_after_budget_preflight(
                pending.conversation_id,
                pending.config,
                pending.request_content,
                pending.task_system_prompt,
                pending.rag_system_prompt,
                false,
                cx,
            );
        }

        let Some((history, trimmed_count)) = self.build_ai_stream_history(
            &conversation_id,
            &config,
            request_content.clone(),
            task_system_prompt.clone(),
            rag_system_prompt.clone(),
            cx,
        ) else {
            return;
        };
        if trimmed_count > 0 {
            self.show_ai_trim_notice(trimmed_count, cx);
        }
        let now = ai_now_ms();
        let assistant_id = self.next_ai_chat_id(now, cx);
        let request_message = self.ai_entity.read(cx).conversation_state()
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
        let (budget_decision, budget_diagnostic_payload) = self.ai_entity.read(cx).conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .map(|conversation| {
                let decision = self.ai_send_budget_decision(
                    conversation,
                    &config,
                    request_content.as_deref(),
                    task_system_prompt.as_deref(),
                    rag_system_prompt.as_deref(),
                );
                let payload = self.ai_budget_diagnostic_payload(
                    conversation,
                    &config,
                    request_content.as_deref(),
                    task_system_prompt.as_deref(),
                    rag_system_prompt.as_deref(),
                    decision,
                    trimmed_count,
                );
                (decision, payload)
            })
            .unwrap_or_else(|| {
                let decision = None;
                let payload = serde_json::json!({
                    "requestKind": "chat",
                    "budgetLevel": 0,
                    "nextLevel": 0,
                    "contextWindow": self.ai_active_model_context_window(&config),
                    "responseReserve": config.max_response_tokens,
                    "trimmedCount": trimmed_count,
                });
                (decision, payload)
            });
        let budget_level = budget_decision.map(|decision| decision.level).unwrap_or(0);
        let backend = ai_message_backend_for_stream(&config);
        self.ai_entity.update(cx, |ai, _cx| {
            ai.begin_assistant_turn(
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
                    suggestions: Vec::new(),
                },
                budget_level,
                backend,
            );
        });
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
            Some(request_message_id),
            now,
        ));
        diagnostic_events.push(ai_diagnostic_event(
            format!("diagnostic-budget-{assistant_id}"),
            &conversation_id,
            "budget_level_changed",
            Some(assistant_id.clone()),
            None,
            now,
            self.ai_diagnostic_base(budget_diagnostic_payload),
        ));
        self.persist_ai_transcript_entries(
            conversation_id.clone(),
            transcript_entries,
            cx,
        );
        self.persist_ai_diagnostic_events(conversation_id.clone(), diagnostic_events, cx);
        self.ai_entity.update(cx, |ai, _cx| ai.set_chat_loading(true));
        let (generation, ui_tx) = self
            .ai_entity
            .update(cx, |ai, _cx| ai.begin_chat_stream());
        // Every model turn receives a fresh authority lease. The token remains
        // transient and never enters conversation history or diagnostics.
        let tool_session_id = self
            .ai_runtime_context
            .update(cx, |runtime, _cx| runtime.begin_tool_session(generation));
        let model_runtime = AiModelRuntimeState {
            context_window: self.ai_active_model_context_window(&config),
        };
        let services = self.ai_model_backend_services(cx);
        let task = self.forwarding_runtime.spawn(run_ai_chat_tool_loop(
            config,
            history,
            model_runtime,
            services,
            budget_decision.map(|decision| decision.level).unwrap_or(0),
            generation,
            tool_session_id,
            conversation_id,
            assistant_id,
            ui_tx,
        ));
        self.ai_entity
            .update(cx, |ai, _cx| ai.set_chat_stream_task(generation, task));
    }
}
