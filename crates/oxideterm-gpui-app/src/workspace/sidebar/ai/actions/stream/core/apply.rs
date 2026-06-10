fn apply_ai_acp_session_started_to_conversations(
    conversations: &mut [AiConversation],
    current_generation: u64,
    delivery_generation: u64,
    conversation_id: &str,
    session_id: &str,
    session_metadata: Option<serde_json::Value>,
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
        object.insert(
            "acp".to_string(),
            serde_json::json!({
                "agentId": agent_id,
                "sessionId": session_id,
                "metadata": session_metadata,
            }),
        );
    }
    true
}

impl WorkspaceApp {
    fn apply_ai_acp_session_started(
        &mut self,
        generation: u64,
        conversation_id: &str,
        session_id: &str,
        session_metadata: Option<serde_json::Value>,
        agent_id: &str,
    ) -> bool {
        if !apply_ai_acp_session_started_to_conversations(
            &mut self.ai_chat.conversations,
            self.ai_chat_stream_generation,
            generation,
            conversation_id,
            session_id,
            session_metadata,
            agent_id,
        ) {
            return false;
        }
        self.persist_ai_chat_state();
        true
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
                let turn_facts = self.ai_tool_result_facts_for_message(conversation_id, message_id);
                let mut result_binding_guardrail = None;
                self.ai_chat
                    .update_message(conversation_id, message_id, |message| {
                        result_binding_guardrail =
                            apply_ai_result_binding_guard(message, &turn_facts);
                        finalize_ai_turn_suggestions(message);
                        message.is_streaming = false;
                        set_ai_turn_status(message, "complete");
                    });
                if let Some(guardrail) = result_binding_guardrail {
                    let now = ai_now_ms();
                    self.persist_ai_transcript_entries(
                        conversation_id.to_string(),
                        vec![ai_transcript_entry(
                            format!("transcript-guardrail-{message_id}-result-binding-{now}"),
                            conversation_id,
                            "guardrail",
                            serde_json::json!({
                                "code": "result_binding_required",
                                "message": guardrail.message.as_str(),
                                "rawText": guardrail.raw_text.as_str(),
                            }),
                            Some(message_id.to_string()),
                            Some(message_id.to_string()),
                            now,
                        )],
                    );
                    self.persist_ai_diagnostic_events(
                        conversation_id.to_string(),
                        vec![ai_diagnostic_event(
                            format!("diagnostic-guardrail-{message_id}-result-binding-{now}"),
                            conversation_id,
                            "guardrail",
                            Some(message_id.to_string()),
                            None,
                            now,
                            self.ai_diagnostic_base(serde_json::json!({
                                "requestKind": "chat",
                                "code": "result_binding_required",
                                "message": guardrail.message.as_str(),
                                "rawTextLength": guardrail.raw_text.chars().count(),
                            })),
                        )],
                    );
                }
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
                    risk.clone(),
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
                            round_id_value.clone(),
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
    fn record_ai_tool_execution_status(
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
            .ai_tool_execution_records
            .iter()
            .position(|record| record.tool_call_id == tool_call_id);
        let mut record = existing
            .and_then(|index| self.ai_tool_execution_records.remove(index))
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
                runtime_epoch: self.ai_runtime_epoch.clone(),
            });

        record.status = status.to_string();
        record.risk = risk.unwrap_or(&record.risk).to_string();
        record.argument_summary = ai_tool_argument_summary(tool_name, args.as_ref());
        record.target_id = ai_tool_result_target_id(result).or_else(|| ai_tool_argument_target_id(args.as_ref()));
        record.target_kind = ai_tool_result_target_kind(result);
        record.execution_surface = ai_tool_execution_surface(tool_name, args.as_ref(), result);
        record.visible_in_terminal = ai_tool_visible_in_terminal(result);
        record.approval_source = ai_tool_approval_source(status, result);
        record.runtime_epoch = ai_tool_runtime_epoch(result)
            .unwrap_or_else(|| self.ai_runtime_epoch.clone());

        if matches!(status, "rejected" | "completed" | "error") {
            record.finished_at = Some(now);
            record.success = Some(status == "completed");
            record.error_code = ai_tool_error_code(result);
            record.result_summary = result
                .and_then(|value| value.get("summary"))
                .and_then(serde_json::Value::as_str)
                .or_else(|| result.and_then(|value| value.get("output")).and_then(serde_json::Value::as_str))
                .map(|value| truncate_ai_tool_record_text(value, 240));
            record.duration_ms = ai_tool_duration_ms(result);
        }

        self.ai_tool_execution_records.push_back(record.clone());
        while self.ai_tool_execution_records.len() > 500 {
            self.ai_tool_execution_records.pop_front();
        }
        Some(record)
    }

    fn record_ai_tool_result_facts(
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
            self.ai_tool_result_facts.push_back(fact.clone());
        }
        while self.ai_tool_result_facts.len() > 1000 {
            self.ai_tool_result_facts.pop_front();
        }
        facts
    }

    fn ai_tool_result_facts_for_message(
        &self,
        conversation_id: &str,
        assistant_message_id: &str,
    ) -> Vec<AiToolResultFact> {
        ai_tool_result_facts_for_message(
            &self.ai_tool_result_facts,
            conversation_id,
            assistant_message_id,
        )
    }

    fn record_ai_command_from_tool_status(
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
        let target_refs = target
            .and_then(|value| value.get("refs"))
            .or_else(|| {
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
            .unwrap_or(&self.ai_runtime_epoch)
            .to_string();
        let approval_mode = meta
            .and_then(|value| value.get("approvalMode"))
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);
        self.ai_command_record_sequence = self.ai_command_record_sequence.saturating_add(1);
        let now = ai_now_ms();
        let record = AiRuntimeCommandRecord {
            command_id: format!("cmd-{}-{}", now, self.ai_command_record_sequence),
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
        self.ai_command_records.push_back(record);
        while self.ai_command_records.len() > 200 {
            self.ai_command_records.pop_front();
        }
        self.trim_ai_command_records_per_session();
    }

    fn trim_ai_command_records_per_session(&mut self) {
        let mut per_session: HashMap<String, usize> = HashMap::new();
        let mut keep = VecDeque::new();
        for record in self.ai_command_records.iter().rev() {
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
        self.ai_command_records = keep;
    }

    fn record_ai_cli_agent_command(&mut self, record: &AiRuntimeCommandRecord) {
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
            .ai_cli_agent_sessions
            .get(&id)
            .map(|session| session.started_at)
            .unwrap_or(record.started_at);
        let status = match record.status.as_str() {
            "waiting_for_input" => "waiting_for_input",
            "error" => "failed",
            _ => "running",
        };
        self.ai_cli_agent_sessions.insert(
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
}

struct AiResultBindingGuardrail {
    message: String,
    raw_text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AiEvidenceClaim {
    text: String,
    evidence: Vec<String>,
    confidence: String,
}

fn apply_ai_result_binding_guard(
    message: &mut AiChatMessage,
    recent_facts: &[AiToolResultFact],
) -> Option<AiResultBindingGuardrail> {
    match parse_ai_evidence_claims_from_message(&message.content) {
        Ok(Some(parsed)) => {
            message.content = parsed.visible_text;
            strip_ai_evidence_claims_block_from_turn_text_parts(message);
            if ai_validate_evidence_claims(&message.content, &parsed.claims, recent_facts).is_ok()
            {
                append_ai_turn_claim_parts(message, &parsed.claims, "verified");
                return None;
            }
            return Some(ai_result_binding_guardrail_for_message(message));
        }
        Ok(None) => {}
        Err(_) => {
            return Some(ai_result_binding_guardrail_for_message(message));
        }
    }

    if !ai_text_claims_tool_backed_fact(&message.content)
        || ai_recent_facts_support_text(&message.content, recent_facts)
    {
        return None;
    }
    Some(ai_result_binding_guardrail_for_message(message))
}

fn ai_result_binding_guardrail_for_message(
    message: &mut AiChatMessage,
) -> AiResultBindingGuardrail {
    let raw_text = message.content.clone();
    let guardrail_message = "I do not have tool-result evidence for that claim, so I cannot present it as a verified fact yet. I need to run the appropriate tool first.".to_string();
    message.content = guardrail_message.clone();
    append_ai_turn_guardrail_part(
        message,
        "result_binding_required",
        &guardrail_message,
        Some(&raw_text),
    );
    AiResultBindingGuardrail {
        message: guardrail_message,
        raw_text,
    }
}

struct ParsedAiEvidenceClaims {
    visible_text: String,
    claims: Vec<AiEvidenceClaim>,
}

fn parse_ai_evidence_claims_from_message(
    text: &str,
) -> Result<Option<ParsedAiEvidenceClaims>, String> {
    let Some((visible_text, block)) = extract_ai_evidence_claims_block(text)? else {
        return Ok(None);
    };
    let json_text = strip_ai_evidence_claims_code_fence(block.trim());
    let value = serde_json::from_str::<serde_json::Value>(json_text)
        .map_err(|error| format!("invalid evidence claims json: {error}"))?;
    let claims_value = value
        .get("claims")
        .and_then(serde_json::Value::as_array)
        .or_else(|| value.as_array())
        .ok_or_else(|| "evidence claims must be an object with claims[] or an array".to_string())?;
    let mut claims = Vec::new();
    for claim in claims_value {
        let object = claim
            .as_object()
            .ok_or_else(|| "each evidence claim must be an object".to_string())?;
        let text = object
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let evidence = object
            .get("evidence")
            .or_else(|| object.get("evidenceFactIds"))
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "each evidence claim must include evidence[]".to_string())?
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let confidence = object
            .get("confidence")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("verified")
            .trim()
            .to_string();
        claims.push(AiEvidenceClaim {
            text,
            evidence,
            confidence,
        });
    }
    Ok(Some(ParsedAiEvidenceClaims {
        visible_text,
        claims,
    }))
}

fn extract_ai_evidence_claims_block(text: &str) -> Result<Option<(String, String)>, String> {
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

fn strip_ai_evidence_claims_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }
    let Some(first_newline) = trimmed.find('\n') else {
        return trimmed;
    };
    let body = &trimmed[first_newline + 1..];
    body.strip_suffix("```").map(str::trim).unwrap_or(body)
}

fn strip_ai_evidence_claims_block_from_turn_text_parts(message: &mut AiChatMessage) {
    mutate_ai_turn_parts(message, |parts| {
        for part in parts {
            if part.get("type").and_then(serde_json::Value::as_str) != Some("text") {
                continue;
            }
            let Some(text) = part.get("text").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Ok(Some((visible_text, _))) = extract_ai_evidence_claims_block(text) else {
                continue;
            };
            if let Some(object) = part.as_object_mut() {
                object.insert("text".to_string(), serde_json::json!(visible_text));
            }
        }
    });
}

fn append_ai_turn_claim_parts(
    message: &mut AiChatMessage,
    claims: &[AiEvidenceClaim],
    status: &str,
) {
    mutate_ai_turn_parts(message, |parts| {
        for claim in claims {
            parts.push(serde_json::json!({
                "type": "claim",
                "text": claim.text,
                "evidence": claim.evidence,
                "confidence": claim.confidence,
                "status": status,
            }));
        }
    });
}

fn ai_validate_evidence_claims(
    visible_text: &str,
    claims: &[AiEvidenceClaim],
    facts: &[AiToolResultFact],
) -> Result<(), String> {
    if claims.is_empty() {
        return Err("evidence claims block has no claims".to_string());
    }
    for claim in claims {
        ai_validate_evidence_claim(claim, facts)?;
    }
    let cited_facts = ai_cited_evidence_facts(claims, facts);
    if !ai_recent_facts_support_text(visible_text, &cited_facts) {
        return Err("visible answer is not supported by cited evidence".to_string());
    }
    Ok(())
}

fn ai_validate_evidence_claim(
    claim: &AiEvidenceClaim,
    facts: &[AiToolResultFact],
) -> Result<(), String> {
    if claim.text.trim().is_empty() {
        return Err("evidence claim text is empty".to_string());
    }
    if !claim.confidence.eq_ignore_ascii_case("verified") {
        return Err("first-pass evidence claims must be verified".to_string());
    }
    if claim.evidence.is_empty() {
        return Err("verified evidence claim has no evidence".to_string());
    }
    let cited_facts = facts
        .iter()
        .filter(|fact| claim.evidence.iter().any(|id| id == &fact.fact_id))
        .cloned()
        .collect::<Vec<_>>();
    if cited_facts.len() != claim.evidence.len() {
        return Err("evidence claim cites unknown fact ids".to_string());
    }
    if !ai_recent_facts_support_text(&claim.text, &cited_facts) {
        return Err("claim text is not supported by cited evidence".to_string());
    }
    Ok(())
}

fn ai_cited_evidence_facts(
    claims: &[AiEvidenceClaim],
    facts: &[AiToolResultFact],
) -> Vec<AiToolResultFact> {
    facts
        .iter()
        .filter(|fact| {
            claims
                .iter()
                .any(|claim| claim.evidence.iter().any(|id| id == &fact.fact_id))
        })
        .cloned()
        .collect()
}

fn ai_text_claims_tool_backed_fact(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    let english_markers = [
        "i ran ",
        "i executed ",
        "i checked ",
        "i verified ",
        "command output",
        "exit code",
        "stdout",
        "stderr",
        "system load",
        "load average",
        "uptime",
        "disk",
        "memory",
    ];
    if english_markers
        .iter()
        .any(|marker| normalized.contains(marker))
    {
        return true;
    }
    let chinese_markers = [
        "我执行",
        "我运行",
        "我检查",
        "检查过",
        "已经检查",
        "已执行",
        "已运行",
        "真正的系统状态",
        "真实的系统状态",
        "命令输出",
        "退出码",
        "运行时间",
        "系统负载",
        "磁盘",
        "内存",
        "负载",
        "结果是",
        "输出是",
    ];
    chinese_markers.iter().any(|marker| text.contains(marker))
}

fn ai_recent_facts_support_text(text: &str, facts: &[AiToolResultFact]) -> bool {
    if facts.is_empty() {
        return false;
    }
    let tokens = ai_numeric_evidence_tokens(text);
    if tokens.is_empty() {
        return false;
    }
    let support = facts
        .iter()
        .map(|fact| fact.output_preview.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    let compact_support = ai_compact_evidence_text(&support);
    tokens
        .iter()
        .all(|token| compact_support.contains(&ai_compact_evidence_text(token)))
}

fn ai_numeric_evidence_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        let allowed_after_digit =
            character.is_ascii_alphanumeric() || matches!(character, '.' | ':' | '%' | '-');
        if character.is_ascii_digit() || (!current.is_empty() && allowed_after_digit) {
            current.push(character.to_ascii_lowercase());
            continue;
        }
        ai_push_evidence_token(&mut tokens, &mut current);
    }
    ai_push_evidence_token(&mut tokens, &mut current);
    tokens.sort();
    tokens.dedup();
    tokens
}

fn ai_push_evidence_token(tokens: &mut Vec<String>, current: &mut String) {
    if current.is_empty() {
        return;
    }
    let has_digit = current.chars().any(|character| character.is_ascii_digit());
    if has_digit {
        tokens.push(current.clone());
    }
    current.clear();
}

fn ai_tool_result_facts_for_message(
    facts: &VecDeque<AiToolResultFact>,
    conversation_id: &str,
    assistant_message_id: &str,
) -> Vec<AiToolResultFact> {
    // Keep result binding local to the assistant turn that produced the tool
    // result. Older evidence can still be shown in history, but it must not
    // prove a new "I checked" claim after restart or resume.
    facts
        .iter()
        .filter(|fact| {
            fact.conversation_id == conversation_id
                && fact.assistant_message_id == assistant_message_id
        })
        .cloned()
        .collect()
}

fn ai_compact_evidence_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace() && !matches!(character, '*' | '`' | ','))
        .collect::<String>()
}

fn ai_tool_argument_summary(tool_name: &str, args: Option<&serde_json::Value>) -> String {
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

fn ai_tool_argument_target_id(args: Option<&serde_json::Value>) -> Option<String> {
    args.and_then(|value| value.get("target_id"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

fn ai_tool_result_target_id(result: Option<&serde_json::Value>) -> Option<String> {
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

fn ai_tool_result_target_kind(result: Option<&serde_json::Value>) -> Option<String> {
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

fn ai_tool_visible_in_terminal(result: Option<&serde_json::Value>) -> Option<bool> {
    result
        .and_then(|value| value.pointer("/execution/visibleInTerminal"))
        .or_else(|| result.and_then(|value| value.pointer("/data/visibleInTerminal")))
        .and_then(serde_json::Value::as_bool)
}

fn ai_tool_execution_surface(
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

fn ai_tool_approval_source(status: &str, result: Option<&serde_json::Value>) -> Option<String> {
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

fn ai_tool_error_code(result: Option<&serde_json::Value>) -> Option<String> {
    result
        .and_then(|value| value.pointer("/error/code"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

fn ai_tool_duration_ms(result: Option<&serde_json::Value>) -> Option<u64> {
    result
        .and_then(|value| value.pointer("/meta/durationMs"))
        .and_then(serde_json::Value::as_u64)
}

fn ai_tool_runtime_epoch(result: Option<&serde_json::Value>) -> Option<String> {
    result
        .and_then(|value| value.pointer("/meta/runtimeEpoch"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

fn ai_tool_execution_record_json(record: &AiToolExecutionRecord) -> serde_json::Value {
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

fn extract_ai_tool_result_facts(
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

fn ai_tool_result_fact(
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

fn ai_tool_result_fact_json(fact: &AiToolResultFact) -> serde_json::Value {
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

fn ai_tool_fact_hash(text: &str) -> String {
    let digest = <sha2::Sha256 as sha2::Digest>::digest(text.as_bytes());
    format!("sha256:{digest:x}")
}

fn ai_fact_value_text(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn truncate_ai_tool_record_text(value: &str, max_chars: usize) -> String {
    let mut result = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        result.push_str("...");
    }
    result
}

fn detect_ai_cli_agent_kind(command: &str) -> Option<String> {
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
