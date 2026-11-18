use super::*;
use oxideterm_ai::stream_state::*;

impl AiWorkspaceEntity {
    pub fn apply_ai_round_summary(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        round_id: &str,
        text: &str,
        metadata: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        if !self.is_chat_stream_generation(generation) {
            return;
        }
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        {
            self.update_chat_message(conversation_id, message_id, |message| {
                upsert_ai_round_summary(message, round_id, text, metadata.clone());
            });
        }

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

        self.persist_transcript_entries(
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
        self.persist_chat_state();
        cx.notify();
    }

    pub fn apply_ai_round_stateful_marker(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        round_id: &str,
        marker: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if !self.is_chat_stream_generation(generation) {
            return;
        }
        {
            self.update_chat_message(conversation_id, message_id, |message| {
                set_ai_turn_round_stateful_marker(message, round_id, marker.as_deref());
            });
        }
        self.persist_chat_state();
        cx.notify();
    }

    pub fn persist_ai_stream_diagnostic(
        &self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        event_type: &str,
        round_id: Option<String>,
        data: serde_json::Value,
        diagnostic_base: impl Fn(serde_json::Value) -> serde_json::Value,
    ) {
        if !self.is_chat_stream_generation(generation) {
            return;
        }
        let now = ai_now_ms();
        self.persist_diagnostic_events(
            conversation_id.to_string(),
            vec![ai_diagnostic_event(
                format!("diagnostic-{event_type}-{message_id}-{now}"),
                conversation_id,
                event_type,
                Some(message_id.to_string()),
                round_id,
                now,
                diagnostic_base(data),
            )],
        );
    }

    pub fn apply_ai_tool_status(
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
        _raw_text: Option<String>,
        round_id_override: Option<String>,
        round_number_override: Option<i64>,
        diagnostic_base: impl Fn(serde_json::Value) -> serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        if !self.is_chat_stream_generation(generation) {
            return;
        }
        if name == "ask_user" && matches!(status, "completed" | "error" | "rejected") {
            self.pending_user_questions
                .remove(&(generation, tool_call_id.to_owned()));
        }
        if matches!(name, "cursor/ask_question" | "cursor/create_plan")
            && matches!(status, "completed" | "error" | "rejected")
        {
            self.pending_cursor_requests
                .remove(&(generation, tool_call_id.to_owned()));
        }
        let persisted_arguments = sanitize_ai_tool_arguments_for_persistence(arguments);
        let persisted_result = result
            .as_ref()
            .map(|result| oxideterm_ai::sanitize_tool_result_json_for_persistence(name, result));
        let persisted_summary = summary
            .as_deref()
            .map(oxideterm_ai::sanitize_for_persistence);
        let should_persist = result.is_some()
            || matches!(
                status,
                "pending_user_approval" | "rejected" | "completed" | "error"
            );
        let mut round_id = None;
        let mut round_number = None;
        {
            self.update_tool_approval_preview(
                generation,
                message_id,
                tool_call_id,
                arguments,
                status,
            );
            self.update_chat_message(conversation_id, message_id, |message| {
                update_ai_tool_call_status(
                    message,
                    tool_call_id,
                    name,
                    &persisted_arguments,
                    status,
                    persisted_result.clone(),
                    risk.clone(),
                    persisted_summary,
                    round_id_override.as_deref(),
                    round_number_override,
                );
                if let Some(call) = message.tool_calls.iter_mut().find(|call| {
                    call.get("id").and_then(serde_json::Value::as_str) == Some(tool_call_id)
                }) {
                    call["approvalGeneration"] = serde_json::json!(generation);
                }
                let (id, number) = ai_turn_round_for_tool_call_with_override(
                    message,
                    tool_call_id,
                    round_id_override.as_deref(),
                    round_number_override,
                );
                round_id = Some(id);
                round_number = Some(number);
            });
        }
        if result.is_some() || matches!(status, "rejected" | "completed" | "error") {
            {
                let run = self.agent_run(generation);
                self.agents.tool_leases.retain(|(_, id), leases| {
                    id != tool_call_id
                        || !leases
                            .iter()
                            .any(|lease| Some(&lease.lease().owner) == run.as_ref())
                });
            }
        }
        if self.is_agent_message(message_id) {
            if should_persist {
                self.persist_agent_records();
            }
            cx.notify();
            return;
        }
        if should_persist {
            let now = ai_now_ms();
            let round_id_value = round_id.clone();
            let round_number_value = round_number.unwrap_or(1);
            let tool_execution_record = self.record_ai_tool_execution_status(
                conversation_id,
                message_id,
                tool_call_id,
                name,
                &persisted_arguments,
                status,
                persisted_result.as_ref(),
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
                    "argumentsText": persisted_arguments.as_str(),
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
                    diagnostic_base(serde_json::json!({
                        "logicalRound": round_number_value,
                        "toolCallId": tool_call_id,
                        "toolName": name,
                        "arguments": persisted_arguments.as_str(),
                        "syntheticDenied": synthetic_denied,
                    })),
                ));
            }
            if matches!(status, "rejected" | "completed" | "error") {
                let success = status == "completed";
                let output = persisted_result
                    .as_ref()
                    .and_then(|value| value.get("output"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let error = persisted_result
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
                    diagnostic_base(serde_json::json!({
                        "logicalRound": round_number_value,
                        "toolCallId": tool_call_id,
                        "toolName": name,
                        "success": success,
                        "error": error,
                        "syntheticDenied": synthetic_denied,
                    })),
                ));
                if let Some(record) = tool_execution_record.as_ref() {
                    let facts =
                        self.record_ai_tool_result_facts(record, persisted_result.as_ref(), now);
                    diagnostic_events.push(ai_diagnostic_event(
                        format!("diagnostic-tool-execution-{tool_call_id}"),
                        conversation_id,
                        "tool_execution",
                        Some(message_id.to_string()),
                        round_id_value.clone(),
                        now,
                        diagnostic_base(ai_tool_execution_record_json(&record)),
                    ));
                    if !facts.is_empty() {
                        diagnostic_events.push(ai_diagnostic_event(
                            format!("diagnostic-tool-result-facts-{tool_call_id}"),
                            conversation_id,
                            "tool_result_facts",
                            Some(message_id.to_string()),
                            round_id_value,
                            now,
                            diagnostic_base(serde_json::json!({
                                "facts": facts.iter().map(ai_tool_result_fact_json).collect::<Vec<_>>(),
                            })),
                        ));
                    }
                }
            }
            self.persist_transcript_entries(conversation_id.to_string(), transcript_entries);
            self.persist_diagnostic_events(conversation_id.to_string(), diagnostic_events);
            self.persist_chat_state();
        }
        cx.notify();
    }
}

impl AiWorkspaceEntity {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn persist_ai_assistant_round(
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
        diagnostic: impl Fn(serde_json::Value) -> serde_json::Value,
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
        self.persist_transcript_entries(conversation_id.to_string(), transcript_entries);
        self.persist_diagnostic_events(
            conversation_id.to_string(),
            vec![ai_diagnostic_event(
                format!("diagnostic-assistant-round-{round_id}"),
                conversation_id,
                "assistant_round",
                Some(message_id.to_string()),
                Some(round_id.clone()),
                now,
                diagnostic(serde_json::json!({
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
}

impl AiWorkspaceEntity {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn persist_summary_created(
        &self,
        conversation_id: &str,
        message_id: &str,
        summary_kind: &str,
        summary_text: &str,
        round_id: Option<String>,
        transcript_ref: Option<serde_json::Value>,
        compacted_message_count: Option<usize>,
        compacted_until_message_id: Option<String>,
        source: Option<&str>,
        timestamp: i64,
        diagnostic: impl Fn(serde_json::Value) -> serde_json::Value,
    ) {
        let round_id_for_diagnostic = round_id.clone();
        self.persist_transcript_entries(
            conversation_id.to_string(),
            vec![ai_transcript_entry(
                format!("transcript-summary-created-{message_id}"),
                conversation_id,
                "summary_created",
                serde_json::json!({
                    "messageId": message_id,
                    "roundId": round_id,
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
                    "compactedUntilMessageId": compacted_until_message_id,
                }),
                Some(message_id.to_string()),
                Some(message_id.to_string()),
                timestamp,
            )],
        );
        self.persist_diagnostic_events(
            conversation_id.to_string(),
            vec![ai_diagnostic_event(
                format!("diagnostic-summary-created-{message_id}"),
                conversation_id,
                "compaction_completed",
                Some(message_id.to_string()),
                None,
                timestamp,
                diagnostic(serde_json::json!({
                    "roundId": round_id_for_diagnostic,
                    "summaryKind": summary_kind,
                    "summaryLength": summary_text.len(),
                    "compactedMessageCount": compacted_message_count,
                    "source": source,
                })),
            )],
        );
    }
}

impl AiWorkspaceEntity {
    pub fn persist_ai_assistant_turn_end(
        &self,
        conversation_id: &str,
        message_id: &str,
        status: &str,
    ) {
        let Some(message) = self
            .conversation_state()
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
        let has_parts = parts.as_array().is_some_and(|parts| !parts.is_empty());
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
        self.persist_transcript_entries(conversation_id.to_string(), entries);
    }

    pub fn persist_ai_removed_assistant_turn_end(
        &self,
        conversation_id: &str,
        message_id: &str,
        status: &str,
    ) {
        let now = ai_now_ms();
        self.persist_transcript_entries(
            conversation_id.to_string(),
            vec![ai_transcript_entry(
                format!("transcript-assistant-end-{message_id}"),
                conversation_id,
                "assistant_turn_end",
                serde_json::json!({
                    "status": status,
                    "messageId": message_id,
                    "plainTextSummary": "",
                    "toolRoundCount": 0,
                }),
                Some(message_id.to_string()),
                Some(message_id.to_string()),
                now,
            )],
        );
    }
}
impl AiWorkspaceEntity {
    pub fn apply_ai_guardrail(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        code: &str,
        message: &str,
        raw_text: Option<String>,
        diagnostic_base: impl Fn(serde_json::Value) -> serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        if !self.is_chat_stream_generation(generation) {
            return;
        }
        let persisted_raw_text = raw_text.as_deref().map(oxideterm_ai::sanitize_for_ai);
        {
            self.update_chat_message(conversation_id, message_id, |message_value| {
                append_ai_turn_guardrail_part(
                    message_value,
                    code,
                    message,
                    persisted_raw_text.as_deref(),
                );
            });
        }
        let now = ai_now_ms();
        self.persist_transcript_entries(
            conversation_id.to_string(),
            vec![ai_transcript_entry(
                format!("transcript-guardrail-{message_id}-{code}-{now}"),
                conversation_id,
                "guardrail",
                serde_json::json!({
                    "code": code,
                    "message": message,
                }),
                Some(message_id.to_string()),
                Some(message_id.to_string()),
                now,
            )],
        );
        self.persist_diagnostic_events(
            conversation_id.to_string(),
            vec![ai_diagnostic_event(
                format!("diagnostic-guardrail-{message_id}-{code}-{now}"),
                conversation_id,
                "guardrail",
                Some(message_id.to_string()),
                None,
                now,
                diagnostic_base(serde_json::json!({
                    "requestKind": "chat",
                    "code": code,
                    "message": message,
                    "rawTextLength": raw_text.as_ref().map(|text| text.len()).unwrap_or(0),
                })),
            )],
        );
        self.persist_chat_state();
        cx.notify();
    }
}
