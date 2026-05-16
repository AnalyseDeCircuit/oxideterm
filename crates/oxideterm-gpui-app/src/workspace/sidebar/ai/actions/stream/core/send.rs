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
            suggestions: Vec::new(),
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
}
