pub(in crate::workspace) fn apply_ai_acp_session_started_to_conversations(
    conversations: &mut [AiConversation],
    current_generation: u64,
    delivery_generation: u64,
    conversation_id: &str,
    session_id: &str,
    session_metadata: Option<serde_json::Value>,
    session_config_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
    agent_id: &str,
) -> bool {
    if current_generation != delivery_generation {
        return false;
    }
    let Some(conversation) = conversations
        .iter_mut()
        .find(|conversation| conversation.id == conversation_id)
    else {
        return false;
    };

    conversation.session_id = Some(session_id.to_string());
    let model_selection = ai_acp_session_state(conversation)
        .filter(|state| state.agent_id == agent_id)
        .and_then(|state| state.model_selection)
        .filter(|selection| {
            session_config_options.iter().any(|option| {
                option.config_id == selection.config_id
                    && option
                        .choices
                        .iter()
                        .any(|choice| choice.value_id == selection.value_id)
            })
        });
    let metadata = conversation
        .session_metadata
        .get_or_insert_with(|| serde_json::json!({ "conversationId": conversation_id }));
    if let Some(object) = metadata.as_object_mut() {
        // ACP session metadata is redacted protocol state, not credentials;
        // store it with the conversation so native resumes match Tauri.
        object.insert(
            "conversationId".to_string(),
            serde_json::json!(conversation_id),
        );
        object.insert("origin".to_string(), serde_json::json!("sidebar"));
        let state = AiAcpSessionState {
            agent_id: agent_id.to_string(),
            session_id: session_id.to_string(),
            metadata: session_metadata,
            config_options: session_config_options,
            model_selection,
        };
        if let Ok(value) = serde_json::to_value(state) {
            object.insert(AI_ACP_SESSION_METADATA_KEY.to_string(), value);
        }
    }
    true
}

impl WorkspaceApp {
    pub(in crate::workspace) fn apply_ai_acp_session_started(
        &mut self,
        generation: u64,
        conversation_id: &str,
        session_id: &str,
        session_metadata: Option<serde_json::Value>,
        session_config_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
        agent_id: &str,
    ) -> bool {
        if !apply_ai_acp_session_started_to_conversations(
            &mut self.ai.chat.conversation_state.conversations,
            self.ai.chat.stream_generation,
            generation,
            conversation_id,
            session_id,
            session_metadata,
            session_config_options,
            agent_id,
        ) {
            return false;
        }
        self.persist_ai_chat_state();
        true
    }

    pub(in crate::workspace) fn apply_ai_stream_event(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        event: AiStreamEvent,
        cx: &mut Context<Self>,
    ) {
        if self.ai.chat.stream_generation != generation {
            return;
        }
        match event {
            AiStreamEvent::Content(chunk) => {
                self.ai.chat.conversation_state.update_message(
                    conversation_id,
                    message_id,
                    |message| {
                        message.content.push_str(&chunk);
                        append_ai_turn_text_part(message, "text", &chunk, false);
                    },
                );
            }
            AiStreamEvent::Thinking(chunk) => {
                self.ai.chat.conversation_state.update_message(
                    conversation_id,
                    message_id,
                    |message| {
                        message
                            .thinking_content
                            .get_or_insert_with(String::new)
                            .push_str(&chunk);
                        append_ai_turn_text_part(message, "thinking", &chunk, true);
                    },
                );
            }
            AiStreamEvent::ToolCall {
                id,
                name,
                arguments,
            } => {
                self.ai.chat.conversation_state.update_message(
                    conversation_id,
                    message_id,
                    |message| {
                        upsert_ai_tool_call(message, &id, &name, &arguments, "running");
                        upsert_ai_turn_tool_call(message, &id, &name, &arguments, "partial");
                    },
                );
            }
            AiStreamEvent::ToolCallComplete {
                id,
                name,
                arguments,
            } => {
                self.ai.chat.conversation_state.update_message(
                    conversation_id,
                    message_id,
                    |message| {
                        upsert_ai_tool_call(message, &id, &name, &arguments, "pending");
                        upsert_ai_turn_tool_call(message, &id, &name, &arguments, "complete");
                    },
                );
            }
            AiStreamEvent::Done => {
                self.ai.chat.conversation_state.update_message(
                    conversation_id,
                    message_id,
                    |message| {
                        // Older prompts asked models to append a private evidence block.
                        // Keep the visible answer and remove only that transport artifact.
                        strip_ai_evidence_claims(message);
                        finalize_ai_turn_suggestions(message);
                        message.is_streaming = false;
                        set_ai_turn_status(message, "complete");
                    },
                );
                self.persist_ai_assistant_turn_end(conversation_id, message_id, "complete");
                self.ai.chat.stream_task = None;
                self.ai.chat.loading = false;
                self.persist_ai_chat_state();
                self.maybe_start_ai_auto_compaction(conversation_id, cx);
            }
            AiStreamEvent::Error(error) => {
                self.ai.chat.conversation_state.update_message(
                    conversation_id,
                    message_id,
                    |message| {
                        message.is_streaming = false;
                        if message.content.is_empty() {
                            message.content = error.clone();
                        } else {
                            message.content.push_str("\n\n");
                            message.content.push_str(&error);
                        }
                        append_ai_turn_error_part(message, &error);
                        set_ai_turn_status(message, "error");
                    },
                );
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
                self.ai.chat.stream_task = None;
                self.ai.chat.loading = false;
                self.persist_ai_chat_state();
                self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn apply_ai_round_summary(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        round_id: &str,
        text: &str,
        metadata: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        if self.ai.chat.stream_generation != generation {
            return;
        }
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        self.ai
            .chat
            .conversation_state
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

    pub(in crate::workspace) fn apply_ai_round_stateful_marker(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        round_id: &str,
        marker: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.ai.chat.stream_generation != generation {
            return;
        }
        self.ai
            .chat
            .conversation_state
            .update_message(conversation_id, message_id, |message| {
                set_ai_turn_round_stateful_marker(message, round_id, marker.as_deref());
            });
        self.persist_ai_chat_state();
        cx.notify();
    }

    pub(in crate::workspace) fn persist_ai_stream_diagnostic(
        &self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        event_type: &str,
        round_id: Option<String>,
        data: serde_json::Value,
    ) {
        if self.ai.chat.stream_generation != generation {
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

    pub(in crate::workspace) fn apply_ai_tool_status(
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
        if self.ai.chat.stream_generation != generation {
            return;
        }
        let should_persist = result.is_some()
            || matches!(
                status,
                "pending_user_approval" | "rejected" | "completed" | "error"
            );
        let mut round_id = None;
        let mut round_number = None;
        self.ai
            .chat
            .conversation_state
            .update_message(conversation_id, message_id, |message| {
                update_ai_tool_call_status(
                    message,
                    tool_call_id,
                    name,
                    arguments,
                    status,
                    result.clone(),
                    risk.clone(),
                    summary,
                    round_id_override.as_deref(),
                    round_number_override,
                );
                let (id, number) = ai_turn_round_for_tool_call_with_override(
                    message,
                    tool_call_id,
                    round_id_override.as_deref(),
                    round_number_override,
                );
                round_id = Some(id);
                round_number = Some(number);
            });
        if should_persist {
            let now = ai_now_ms();
            let round_id_value = round_id.clone();
            let round_number_value = round_number.unwrap_or(1);
            let tool_execution_record = self.record_ai_tool_execution_status(
                conversation_id,
                message_id,
                tool_call_id,
                name,
                arguments,
                status,
                result.as_ref(),
                risk.as_deref(),
                now,
            );
            let mut transcript_entries = Vec::new();
            let mut diagnostic_events = Vec::new();
            if synthetic_denied || matches!(status, "pending" | "running" | "pending_user_approval")
            {
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
                if let Some(record) = tool_execution_record.as_ref() {
                    let facts = self.record_ai_tool_result_facts(record, result.as_ref(), now);
                    diagnostic_events.push(ai_diagnostic_event(
                        format!("diagnostic-tool-execution-{tool_call_id}"),
                        conversation_id,
                        "tool_execution",
                        Some(message_id.to_string()),
                        round_id_value.clone(),
                        now,
                        self.ai_diagnostic_base(ai_tool_execution_record_json(&record)),
                    ));
                    if !facts.is_empty() {
                        diagnostic_events.push(ai_diagnostic_event(
                            format!("diagnostic-tool-result-facts-{tool_call_id}"),
                            conversation_id,
                            "tool_result_facts",
                            Some(message_id.to_string()),
                            round_id_value,
                            now,
                            self.ai_diagnostic_base(serde_json::json!({
                                "facts": facts.iter().map(ai_tool_result_fact_json).collect::<Vec<_>>(),
                            })),
                        ));
                    }
                }
                self.record_ai_command_from_tool_status(
                    name,
                    arguments,
                    status,
                    result.as_ref(),
                    risk.as_deref(),
                );
            }
            self.persist_ai_transcript_entries(conversation_id.to_string(), transcript_entries);
            self.persist_ai_diagnostic_events(conversation_id.to_string(), diagnostic_events);
            self.persist_ai_chat_state();
        }
        cx.notify();
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::workspace) fn record_ai_tool_execution_status(
        &mut self,
        conversation_id: &str,
        message_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        arguments: &str,
        status: &str,
        result: Option<&serde_json::Value>,
        risk: Option<&str>,
        now: i64,
    ) -> Option<AiToolExecutionRecord> {
        let args = serde_json::from_str::<serde_json::Value>(arguments).ok();
        let existing = self
            .ai
            .runtime
            .tool_execution_records
            .iter()
            .position(|record| record.tool_call_id == tool_call_id);
        let mut record = existing
            .and_then(|index| self.ai.runtime.tool_execution_records.remove(index))
            .unwrap_or_else(|| AiToolExecutionRecord {
                record_id: format!("tool-exec-{tool_call_id}"),
                conversation_id: conversation_id.to_string(),
                assistant_message_id: message_id.to_string(),
                tool_call_id: tool_call_id.to_string(),
                tool_name: tool_name.to_string(),
                argument_summary: ai_tool_argument_summary(tool_name, args.as_ref()),
                target_id: ai_tool_argument_target_id(args.as_ref()),
                target_kind: None,
                risk: risk.unwrap_or("read").to_string(),
                approval_source: None,
                execution_surface: ai_tool_execution_surface(tool_name, args.as_ref(), result),
                visible_in_terminal: None,
                status: status.to_string(),
                success: None,
                error_code: None,
                result_summary: None,
                duration_ms: None,
                started_at: now,
                finished_at: None,
                runtime_epoch: self.ai.runtime.epoch.clone(),
            });

        record.status = status.to_string();
        record.risk = risk.unwrap_or(&record.risk).to_string();
        record.argument_summary = ai_tool_argument_summary(tool_name, args.as_ref());
        record.target_id =
            ai_tool_result_target_id(result).or_else(|| ai_tool_argument_target_id(args.as_ref()));
        record.target_kind = ai_tool_result_target_kind(result);
        record.execution_surface = ai_tool_execution_surface(tool_name, args.as_ref(), result);
        record.visible_in_terminal = ai_tool_visible_in_terminal(result);
        record.approval_source = ai_tool_approval_source(status, result);
        record.runtime_epoch =
            ai_tool_runtime_epoch(result).unwrap_or_else(|| self.ai.runtime.epoch.clone());

        if matches!(status, "rejected" | "completed" | "error") {
            record.finished_at = Some(now);
            record.success = Some(status == "completed");
            record.error_code = ai_tool_error_code(result);
            record.result_summary = result
                .and_then(|value| value.get("summary"))
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    result
                        .and_then(|value| value.get("output"))
                        .and_then(serde_json::Value::as_str)
                })
                .map(|value| truncate_ai_tool_record_text(value, 240));
            record.duration_ms = ai_tool_duration_ms(result);
        }

        self.ai
            .runtime
            .tool_execution_records
            .push_back(record.clone());
        while self.ai.runtime.tool_execution_records.len() > 500 {
            self.ai.runtime.tool_execution_records.pop_front();
        }
        Some(record)
    }

    pub(in crate::workspace) fn record_ai_tool_result_facts(
        &mut self,
        record: &AiToolExecutionRecord,
        result: Option<&serde_json::Value>,
        now: i64,
    ) -> Vec<AiToolResultFact> {
        if !matches!(record.status.as_str(), "completed" | "error" | "rejected") {
            return Vec::new();
        }
        let facts = extract_ai_tool_result_facts(record, result, now);
        for fact in &facts {
            self.ai.runtime.tool_result_facts.push_back(fact.clone());
        }
        while self.ai.runtime.tool_result_facts.len() > 1000 {
            self.ai.runtime.tool_result_facts.pop_front();
        }
        facts
    }

    pub(in crate::workspace) fn record_ai_command_from_tool_status(
        &mut self,
        tool_name: &str,
        arguments: &str,
        status: &str,
        result: Option<&serde_json::Value>,
        risk: Option<&str>,
    ) {
        if !matches!(tool_name, "run_command" | "send_terminal_input")
            || !matches!(status, "completed" | "error")
        {
            return;
        }
        let args = serde_json::from_str::<serde_json::Value>(arguments)
            .unwrap_or_else(|_| serde_json::json!({ "rawArguments": arguments }));
        let command = match tool_name {
            "run_command" => args
                .get("command")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            "send_terminal_input" => args
                .get("text")
                .or_else(|| args.get("keys"))
                .or_else(|| args.get("sequence"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            _ => String::new(),
        };
        let command = command.trim().to_string();
        if command.is_empty() {
            return;
        }

        let meta = result.and_then(|value| value.get("meta"));
        let data = result.and_then(|value| value.get("data"));
        let target = result
            .and_then(|value| value.get("targets"))
            .and_then(serde_json::Value::as_array)
            .and_then(|targets| targets.first());
        let target_refs = target.and_then(|value| value.get("refs")).or_else(|| {
            // Tauri tool result targets keep refs under metadata; retain
            // the old native fallback while reading the canonical shape.
            target
                .and_then(|value| value.get("metadata"))
                .and_then(|metadata| metadata.get("refs"))
        });
        let target_id = meta
            .and_then(|value| value.get("targetId"))
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .or_else(|| {
                target
                    .and_then(|value| value.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
            });
        let session_id = target_refs
            .and_then(|refs| refs.get("sessionId"))
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .or_else(|| {
                data.and_then(|value| value.get("sessionId"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
            });
        let node_id = target_refs
            .and_then(|refs| refs.get("nodeId"))
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);
        let exit_code = data
            .and_then(|value| value.get("exitCode"))
            .and_then(serde_json::Value::as_i64);
        let waiting_for_input = data
            .and_then(|value| value.get("waitingForInput"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let runtime_epoch = meta
            .and_then(|value| value.get("runtimeEpoch"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&self.ai.runtime.epoch)
            .to_string();
        let approval_mode = meta
            .and_then(|value| value.get("approvalMode"))
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);
        self.ai.runtime.command_record_sequence =
            self.ai.runtime.command_record_sequence.saturating_add(1);
        let now = ai_now_ms();
        let record = AiRuntimeCommandRecord {
            command_id: format!("cmd-{}-{}", now, self.ai.runtime.command_record_sequence),
            target_id,
            session_id,
            node_id,
            command,
            cwd: args
                .get("cwd")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
            source: if tool_name == "run_command" {
                "ai.run_command".to_string()
            } else {
                "ai.terminal_input".to_string()
            },
            status: if waiting_for_input {
                "waiting_for_input".to_string()
            } else if status == "completed" {
                "completed".to_string()
            } else {
                "error".to_string()
            },
            exit_code,
            started_at: now,
            finished_at: Some(now),
            runtime_epoch,
            approval_mode,
            risk: risk.unwrap_or("read").to_string(),
        };
        self.record_ai_cli_agent_command(&record);
        self.ai.runtime.command_records.push_back(record);
        while self.ai.runtime.command_records.len() > 200 {
            self.ai.runtime.command_records.pop_front();
        }
        self.trim_ai_command_records_per_session();
    }

    pub(in crate::workspace) fn trim_ai_command_records_per_session(&mut self) {
        let mut per_session: HashMap<String, usize> = HashMap::new();
        let mut keep = VecDeque::new();
        for record in self.ai.runtime.command_records.iter().rev() {
            let key = record
                .session_id
                .as_ref()
                .or(record.node_id.as_ref())
                .or(record.target_id.as_ref())
                .cloned()
                .unwrap_or_else(|| "global".to_string());
            let count = per_session.entry(key).or_insert(0);
            if *count < 50 {
                keep.push_front(record.clone());
                *count += 1;
            }
        }
        self.ai.runtime.command_records = keep;
    }

    pub(in crate::workspace) fn record_ai_cli_agent_command(
        &mut self,
        record: &AiRuntimeCommandRecord,
    ) {
        let Some(kind) = detect_ai_cli_agent_kind(&record.command) else {
            return;
        };
        let target_key = record
            .session_id
            .as_ref()
            .or(record.node_id.as_ref())
            .or(record.target_id.as_ref())
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let id = format!("cli-agent:{kind}:{target_key}");
        let existing_started_at = self
            .ai
            .runtime
            .cli_agent_sessions
            .get(&id)
            .map(|session| session.started_at)
            .unwrap_or(record.started_at);
        let status = match record.status.as_str() {
            "waiting_for_input" => "waiting_for_input",
            "error" => "failed",
            _ => "running",
        };
        self.ai.runtime.cli_agent_sessions.insert(
            id.clone(),
            AiCliAgentSession {
                id,
                kind: kind.clone(),
                label: format!("{kind} agent"),
                status: status.to_string(),
                target_id: record.target_id.clone(),
                session_id: record.session_id.clone(),
                node_id: record.node_id.clone(),
                command: record.command.clone(),
                started_at: existing_started_at,
                updated_at: record.finished_at.unwrap_or(record.started_at),
                runtime_epoch: record.runtime_epoch.clone(),
            },
        );
    }

    pub(in crate::workspace) fn apply_ai_guardrail(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        code: &str,
        message: &str,
        raw_text: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.ai.chat.stream_generation != generation {
            return;
        }
        self.ai.chat.conversation_state.update_message(
            conversation_id,
            message_id,
            |message_value| {
                append_ai_turn_guardrail_part(message_value, code, message, raw_text.as_deref());
            },
        );
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
}

pub(in crate::workspace) fn strip_ai_evidence_claims(message: &mut AiChatMessage) {
    message.content = strip_ai_evidence_claims_from_text(&message.content);
    strip_ai_evidence_claims_block_from_turn_text_parts(message);
}

pub(in crate::workspace) fn strip_ai_evidence_claims_from_text(text: &str) -> String {
    const OPEN: &str = "<evidence_claims>";
    let mut visible_text = text.to_string();
    loop {
        match extract_ai_evidence_claims_block(&visible_text) {
            Ok(Some((next_text, _))) => visible_text = next_text,
            Ok(None) => return visible_text.trim().to_string(),
            Err(_) => {
                // A partial streamed block is private protocol output too.
                let visible_end = visible_text.find(OPEN).unwrap_or(visible_text.len());
                return visible_text[..visible_end].trim().to_string();
            }
        }
    }
}

pub(in crate::workspace) fn extract_ai_evidence_claims_block(
    text: &str,
) -> Result<Option<(String, String)>, String> {
    const OPEN: &str = "<evidence_claims>";
    const CLOSE: &str = "</evidence_claims>";
    let Some(start) = text.find(OPEN) else {
        return Ok(None);
    };
    let block_start = start + OPEN.len();
    let Some(close_relative) = text[block_start..].find(CLOSE) else {
        return Err("evidence claims block missing closing tag".to_string());
    };
    let close_start = block_start + close_relative;
    let close_end = close_start + CLOSE.len();
    if text[close_end..].contains(OPEN) {
        return Err("multiple evidence claims blocks are not supported".to_string());
    }
    let visible_text = format!("{}{}", &text[..start], &text[close_end..])
        .trim()
        .to_string();
    let block = text[block_start..close_start].to_string();
    Ok(Some((visible_text, block)))
}

pub(in crate::workspace) fn strip_ai_evidence_claims_block_from_turn_text_parts(
    message: &mut AiChatMessage,
) {
    if message.turn.is_none() {
        return;
    }
    mutate_ai_turn_parts(message, |parts| {
        for part in parts {
            if part.get("type").and_then(serde_json::Value::as_str) != Some("text") {
                continue;
            }
            let Some(text) = part.get("text").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let visible_text = strip_ai_evidence_claims_from_text(text);
            if let Some(object) = part.as_object_mut() {
                object.insert("text".to_string(), serde_json::json!(visible_text));
            }
        }
    });
}

#[cfg(test)]
pub(in crate::workspace) fn ai_tool_result_facts_for_message(
    facts: &VecDeque<AiToolResultFact>,
    conversation_id: &str,
    assistant_message_id: &str,
) -> Vec<AiToolResultFact> {
    // Group facts by the assistant turn that produced the tool result so
    // diagnostics and transcript projections do not mix unrelated rounds.
    facts
        .iter()
        .filter(|fact| {
            fact.conversation_id == conversation_id
                && fact.assistant_message_id == assistant_message_id
        })
        .cloned()
        .collect()
}

pub(in crate::workspace) fn ai_tool_argument_summary(
    tool_name: &str,
    args: Option<&serde_json::Value>,
) -> String {
    // Audit summaries describe routing intent without retaining large or
    // secret-bearing payload fields such as write_resource.content.
    let Some(args) = args.and_then(serde_json::Value::as_object) else {
        return "arguments: invalid_json".to_string();
    };
    match tool_name {
        "run_command" => {
            let command = args
                .get("command")
                .and_then(serde_json::Value::as_str)
                .map(|value| truncate_ai_tool_record_text(value, 200))
                .unwrap_or_else(|| "<missing command>".to_string());
            let target = args
                .get("target_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing target>");
            let cwd = args
                .get("cwd")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!(" cwd={}", truncate_ai_tool_record_text(value, 120)))
                .unwrap_or_default();
            format!("target={target}{cwd} command={command}")
        }
        "send_terminal_input" => {
            let text_len = args
                .get("text")
                .and_then(serde_json::Value::as_str)
                .map(str::chars)
                .map(Iterator::count)
                .unwrap_or(0);
            let append_enter = args
                .get("append_enter")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            format!("text_chars={text_len} append_enter={append_enter}")
        }
        "read_resource" | "write_resource" | "transfer_resource" => {
            let resource = args
                .get("resource")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing resource>");
            let path = args
                .get("path")
                .and_then(serde_json::Value::as_str)
                .map(|value| truncate_ai_tool_record_text(value, 160))
                .unwrap_or_default();
            let target = args
                .get("target_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing target>");
            if path.is_empty() {
                format!("target={target} resource={resource}")
            } else {
                format!("target={target} resource={resource} path={path}")
            }
        }
        "connect_target" => {
            let target = args
                .get("target_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing target>");
            format!("target={target}")
        }
        "open_app_surface" => {
            let surface = args
                .get("surface")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing surface>");
            format!("surface={surface}")
        }
        _ => {
            let mut keys = args.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            format!("keys={}", keys.join(","))
        }
    }
}

pub(in crate::workspace) fn ai_tool_argument_target_id(
    args: Option<&serde_json::Value>,
) -> Option<String> {
    args.and_then(|value| value.get("target_id"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

pub(in crate::workspace) fn ai_tool_result_target_id(
    result: Option<&serde_json::Value>,
) -> Option<String> {
    result
        .and_then(|value| value.pointer("/meta/targetId"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            result
                .and_then(|value| value.pointer("/execution/target/id"))
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        })
        .or_else(|| {
            result
                .and_then(|value| value.get("targets"))
                .and_then(serde_json::Value::as_array)
                .and_then(|targets| targets.first())
                .and_then(|target| target.get("id"))
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        })
}

pub(in crate::workspace) fn ai_tool_result_target_kind(
    result: Option<&serde_json::Value>,
) -> Option<String> {
    result
        .and_then(|value| value.pointer("/execution/target/kind"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            result
                .and_then(|value| value.get("targets"))
                .and_then(serde_json::Value::as_array)
                .and_then(|targets| targets.first())
                .and_then(|target| target.get("kind"))
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        })
}

pub(in crate::workspace) fn ai_tool_visible_in_terminal(
    result: Option<&serde_json::Value>,
) -> Option<bool> {
    result
        .and_then(|value| value.pointer("/execution/visibleInTerminal"))
        .or_else(|| result.and_then(|value| value.pointer("/data/visibleInTerminal")))
        .and_then(serde_json::Value::as_bool)
}

pub(in crate::workspace) fn ai_tool_execution_surface(
    tool_name: &str,
    args: Option<&serde_json::Value>,
    result: Option<&serde_json::Value>,
) -> String {
    if ai_tool_visible_in_terminal(result) == Some(true) {
        return "visible_terminal".to_string();
    }
    match tool_name {
        "run_command" => {
            if ai_tool_argument_target_id(args).as_deref() == Some("local-shell:default") {
                "local_process".to_string()
            } else {
                "background_capture".to_string()
            }
        }
        "send_terminal_input" => "visible_terminal".to_string(),
        "connect_target" | "open_app_surface" | "remember_preference" => "ui_action".to_string(),
        "read_resource" | "write_resource" | "transfer_resource" => {
            let resource = args
                .and_then(|value| value.get("resource"))
                .and_then(serde_json::Value::as_str);
            if resource == Some("settings") {
                "settings".to_string()
            } else {
                "filesystem".to_string()
            }
        }
        "list_mcp_resources" | "read_mcp_resource" => "mcp".to_string(),
        name if oxideterm_ai::is_mcp_tool_name(name) => "mcp".to_string(),
        _ => "app_state".to_string(),
    }
}

pub(in crate::workspace) fn ai_tool_approval_source(
    status: &str,
    result: Option<&serde_json::Value>,
) -> Option<String> {
    result
        .and_then(|value| value.pointer("/meta/approvalMode"))
        .or_else(|| result.and_then(|value| value.pointer("/meta/policyDecision/approvalMode")))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .or_else(|| match status {
            "pending_user_approval" => Some("user_pending".to_string()),
            "rejected" => Some("user_rejected".to_string()),
            "approved" | "running" | "completed" => Some("policy_allowed".to_string()),
            _ => None,
        })
}

pub(in crate::workspace) fn ai_tool_error_code(
    result: Option<&serde_json::Value>,
) -> Option<String> {
    result
        .and_then(|value| value.pointer("/error/code"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

pub(in crate::workspace) fn ai_tool_duration_ms(result: Option<&serde_json::Value>) -> Option<u64> {
    result
        .and_then(|value| value.pointer("/meta/durationMs"))
        .and_then(serde_json::Value::as_u64)
}

pub(in crate::workspace) fn ai_tool_runtime_epoch(
    result: Option<&serde_json::Value>,
) -> Option<String> {
    result
        .and_then(|value| value.pointer("/meta/runtimeEpoch"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

pub(in crate::workspace) fn ai_tool_execution_record_json(
    record: &AiToolExecutionRecord,
) -> serde_json::Value {
    serde_json::json!({
        "recordId": record.record_id,
        "conversationId": record.conversation_id,
        "assistantMessageId": record.assistant_message_id,
        "toolCallId": record.tool_call_id,
        "toolName": record.tool_name,
        "argumentSummary": record.argument_summary,
        "targetId": record.target_id,
        "targetKind": record.target_kind,
        "risk": record.risk,
        "approvalSource": record.approval_source,
        "executionSurface": record.execution_surface,
        "visibleInTerminal": record.visible_in_terminal,
        "status": record.status,
        "success": record.success,
        "errorCode": record.error_code,
        "resultSummary": record.result_summary,
        "durationMs": record.duration_ms,
        "startedAt": record.started_at,
        "finishedAt": record.finished_at,
        "runtimeEpoch": record.runtime_epoch,
    })
}

pub(in crate::workspace) fn extract_ai_tool_result_facts(
    record: &AiToolExecutionRecord,
    result: Option<&serde_json::Value>,
    now: i64,
) -> Vec<AiToolResultFact> {
    let mut facts = Vec::new();
    if let Some(summary) = result
        .and_then(|value| value.get("summary"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        facts.push(ai_tool_result_fact(record, "summary", summary, now));
    }
    if let Some(output) = result
        .and_then(|value| value.get("output"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        facts.push(ai_tool_result_fact(record, "output", output, now));
    }
    if let Some(exit_code) = result
        .and_then(|value| value.pointer("/execution/exitCode"))
        .or_else(|| result.and_then(|value| value.pointer("/data/exitCode")))
    {
        facts.push(ai_tool_result_fact(
            record,
            "execution.exit_code",
            &format!("exit_code: {}", ai_fact_value_text(exit_code)),
            now,
        ));
    }
    if let Some(visible_in_terminal) = result
        .and_then(|value| value.pointer("/execution/visibleInTerminal"))
        .or_else(|| result.and_then(|value| value.pointer("/data/visibleInTerminal")))
    {
        facts.push(ai_tool_result_fact(
            record,
            "execution.visible_in_terminal",
            &format!(
                "visible_in_terminal: {}",
                ai_fact_value_text(visible_in_terminal)
            ),
            now,
        ));
    }
    if let Some(state) = result
        .and_then(|value| value.pointer("/execution/state"))
        .or_else(|| result.and_then(|value| value.pointer("/data/executionState")))
    {
        facts.push(ai_tool_result_fact(
            record,
            "execution.state",
            &format!("execution_state: {}", ai_fact_value_text(state)),
            now,
        ));
    }
    facts
}

pub(in crate::workspace) fn ai_tool_result_fact(
    record: &AiToolExecutionRecord,
    source_kind: &str,
    text: &str,
    now: i64,
) -> AiToolResultFact {
    let output_preview = truncate_ai_tool_record_text(text, 4000);
    AiToolResultFact {
        fact_id: format!("{}.{}", record.tool_call_id, source_kind),
        conversation_id: record.conversation_id.clone(),
        assistant_message_id: record.assistant_message_id.clone(),
        tool_call_id: record.tool_call_id.clone(),
        tool_name: record.tool_name.clone(),
        source_kind: source_kind.to_string(),
        text_hash: ai_tool_fact_hash(text),
        summary: truncate_ai_tool_record_text(text.lines().next().unwrap_or_default(), 240),
        output_preview,
        created_at: now,
        runtime_epoch: record.runtime_epoch.clone(),
    }
}

pub(in crate::workspace) fn ai_tool_result_fact_json(fact: &AiToolResultFact) -> serde_json::Value {
    serde_json::json!({
        "factId": fact.fact_id,
        "conversationId": fact.conversation_id,
        "assistantMessageId": fact.assistant_message_id,
        "toolCallId": fact.tool_call_id,
        "toolName": fact.tool_name,
        "sourceKind": fact.source_kind,
        "textHash": fact.text_hash,
        "summary": fact.summary,
        "outputPreview": fact.output_preview,
        "createdAt": fact.created_at,
        "runtimeEpoch": fact.runtime_epoch,
    })
}

pub(in crate::workspace) fn ai_tool_fact_hash(text: &str) -> String {
    let digest = <sha2::Sha256 as sha2::Digest>::digest(text.as_bytes());
    format!("sha256:{digest:x}")
}

pub(in crate::workspace) fn ai_fact_value_text(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| value.to_string())
}

pub(in crate::workspace) fn truncate_ai_tool_record_text(value: &str, max_chars: usize) -> String {
    let mut result = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        result.push_str("...");
    }
    result
}

pub(in crate::workspace) fn detect_ai_cli_agent_kind(command: &str) -> Option<String> {
    let tokens = command
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if token.eq_ignore_ascii_case("env") || token.contains('=') {
            index += 1;
            continue;
        }
        if token.eq_ignore_ascii_case("npx") {
            index += 1;
            continue;
        }
        let executable = token
            .rsplit('/')
            .next()
            .unwrap_or(token)
            .trim_start_matches('@')
            .to_ascii_lowercase();
        return match executable.as_str() {
            "codex" => Some("codex".to_string()),
            "claude" => Some("claude".to_string()),
            "gemini" => Some("gemini".to_string()),
            "opencode" => Some("opencode".to_string()),
            _ => None,
        };
    }
    None
}
