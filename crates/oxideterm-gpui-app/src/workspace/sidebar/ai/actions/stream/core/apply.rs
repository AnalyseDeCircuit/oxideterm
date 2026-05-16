impl WorkspaceApp {
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
}
