use oxideterm_gpui_ai::state::{
    AiStreamApplyOutcome, ai_tool_execution_record_json, ai_tool_result_fact_json,
    detect_ai_cli_agent_kind, sanitize_ai_tool_arguments_for_approval,
    sanitize_ai_tool_arguments_for_persistence,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn apply_ai_acp_session_started(
        &mut self,
        generation: u64,
        conversation_id: &str,
        session_id: &str,
        session_metadata: Option<serde_json::Value>,
        session_config_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
        session_modes: Option<oxideterm_ai::AcpSessionModeState>,
        agent_id: &str,
        cx: &mut App,
    ) -> bool {
        let applied = self.ai.state.update(cx, |ai, _cx| {
            ai.apply_acp_session_started_state(
                generation,
                conversation_id,
                session_id,
                session_metadata,
                session_config_options,
                session_modes,
                agent_id,
            )
        });
        if !applied {
            return false;
        }
        true
    }

    pub(in crate::workspace) fn ai_stream_error_text(&self, error: &str) -> Option<String> {
        let text = self.i18n.t(oxideterm_ai::stream_error_label(error)?);
        Some(match oxideterm_ai::stream_error_http_status(error) {
            Some(status) => format!("{text} (HTTP {status})"),
            None => text,
        })
    }

    pub(in crate::workspace) fn apply_ai_stream_event(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        event: AiStreamEvent,
        cx: &mut Context<Self>,
    ) {
        let safe_error = match &event {
            AiStreamEvent::Error(error) => Some(
                self.ai_stream_error_text(error)
                    .unwrap_or_else(|| self.i18n.t("settings_view.ai.acp_agent_error_unknown")),
            ),
            _ => None,
        };
        let child_message = self.ai.state.read(cx).is_agent_message(message_id);
        let outcome = self.ai.state.update(cx, |ai, _cx| {
            ai.apply_stream_event_state(
                generation,
                conversation_id,
                message_id,
                event,
                safe_error,
            )
        });
        match outcome {
            // The delivery batch invalidates the assistant view once after
            // applying its deltas. Only lifecycle effects below reach the shell.
            AiStreamApplyOutcome::Applied => return,
            AiStreamApplyOutcome::Completed => {
                self.ai_runtime_context.update(cx, |runtime, _cx| {
                    runtime.finish_tool_session(
                        generation,
                        oxideterm_ai::RuntimeRevocationReason::ToolSessionFinished,
                    );
                });
                if child_message { self.ai.state.read(cx).persist_agent_records(); return; }
                self.persist_ai_assistant_turn_end(
                    conversation_id,
                    message_id,
                    "complete",
                    cx,
                );
                if !self.send_next_queued_ai_message(conversation_id, cx) {
                    self.maybe_start_ai_auto_compaction(conversation_id, cx);
                }
            }
            AiStreamApplyOutcome::Failed(safe_error) => {
                self.ai_runtime_context.update(cx, |runtime, _cx| {
                    runtime.finish_tool_session(
                        generation,
                        oxideterm_ai::RuntimeRevocationReason::ToolSessionFinished,
                    );
                });
                if child_message {
                    self.notify_ai_agent_attention(conversation_id, message_id, "ai.agents.failed", cx);
                    self.ai.state.read(cx).persist_agent_records(); return;
                }
                // Provider errors may contain response bodies or request
                // metadata. Only a localized stable category reaches the
                // conversation, diagnostics, notifications, and persistence.
                self.persist_ai_assistant_turn_end(conversation_id, message_id, "error", cx);
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
                            "message": safe_error.as_str(),
                        })),
                    )],
                    cx,
                );
                self.push_ai_settings_toast(safe_error, TerminalNoticeVariant::Error, cx);
            }
            AiStreamApplyOutcome::Stale => return,
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
        if !self
            .ai.state
            .read(cx)
            .is_chat_stream_generation(generation)
        {
            return;
        }
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        self.ai.state.update(cx, |ai, _cx| {
            ai.update_chat_message(conversation_id, message_id, |message| {
                upsert_ai_round_summary(message, round_id, text, metadata.clone());
            });
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
            cx,
        );
        self.ai.state.read(cx).persist_chat_state();
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
        if !self
            .ai.state
            .read(cx)
            .is_chat_stream_generation(generation)
        {
            return;
        }
        self.ai.state.update(cx, |ai, _cx| {
            ai.update_chat_message(conversation_id, message_id, |message| {
                set_ai_turn_round_stateful_marker(message, round_id, marker.as_deref());
            });
        });
        self.ai.state.read(cx).persist_chat_state();
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
        cx: &App,
    ) {
        if !self
            .ai.state
            .read(cx)
            .is_chat_stream_generation(generation)
        {
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
            cx,
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
        _raw_text: Option<String>,
        round_id_override: Option<String>,
        round_number_override: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        if !self
            .ai.state
            .read(cx)
            .is_chat_stream_generation(generation)
        {
            return;
        }
        if name == "ask_user" && matches!(status, "completed" | "error" | "rejected") {
            self.ai.state.update(cx, |ai, _| { ai.pending_user_questions.remove(&(generation, tool_call_id.to_owned())); });
        }
        if matches!(name, "cursor/ask_question" | "cursor/create_plan") && matches!(status, "completed" | "error" | "rejected") {
            self.ai.state.update(cx, |ai, _| { ai.pending_cursor_requests.remove(&(generation, tool_call_id.to_owned())); });
        }
        let persisted_arguments = sanitize_ai_tool_arguments_for_persistence(arguments);
        if status == "pending_user_approval" { self.notify_ai_agent_attention(conversation_id, message_id, "ai.agents.approval", cx); }
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
        self.ai.state.update(cx, |ai, _cx| {
            ai.update_tool_approval_preview(generation, message_id, tool_call_id, arguments, status);
            ai.update_chat_message(conversation_id, message_id, |message| {
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
                if let Some(call) = message.tool_calls.iter_mut().find(|call| call.get("id").and_then(serde_json::Value::as_str) == Some(tool_call_id)) {
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
        });
        if result.is_some() || matches!(status, "rejected" | "completed" | "error") {
            self.ai.state.update(cx, |ai, _cx| {
                let run = ai.agent_run(generation);
                ai.agents.tool_leases.retain(|(_, id), leases| id != tool_call_id || !leases.iter().any(|lease| Some(&lease.lease().owner) == run.as_ref()));
            });
        }
        if self.ai.state.read(cx).is_agent_message(message_id) {
            if should_persist { self.ai.state.read(cx).persist_agent_records(); }
            return;
        }
        if should_persist {
            let now = ai_now_ms();
            let round_id_value = round_id.clone();
            let round_number_value = round_number.unwrap_or(1);
            let tool_execution_record = self.ai.state.update(cx, |ai, _cx| {
                ai.record_ai_tool_execution_status(
                    conversation_id,
                    message_id,
                    tool_call_id,
                    name,
                    &persisted_arguments,
                    status,
                    persisted_result.as_ref(),
                    risk.as_deref(),
                    now,
                )
            });
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
                    self.ai_diagnostic_base(serde_json::json!({
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
                    let facts = self.ai.state.update(cx, |ai, _cx| {
                        ai.record_ai_tool_result_facts(record, persisted_result.as_ref(), now)
                    });
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
            }
            self.persist_ai_transcript_entries(
                conversation_id.to_string(),
                transcript_entries,
                cx,
            );
            self.persist_ai_diagnostic_events(
                conversation_id.to_string(),
                diagnostic_events,
                cx,
            );
            self.ai.state.read(cx).persist_chat_state();
        }
        cx.notify();
    }
}
impl WorkspaceApp {
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
        if !self
            .ai.state
            .read(cx)
            .is_chat_stream_generation(generation)
        {
            return;
        }
        let persisted_raw_text = raw_text
            .as_deref()
            .map(oxideterm_ai::sanitize_for_ai);
        self.ai.state.update(cx, |ai, _cx| {
            ai.update_chat_message(conversation_id, message_id, |message_value| {
                append_ai_turn_guardrail_part(
                    message_value,
                    code,
                    message,
                    persisted_raw_text.as_deref(),
                );
            });
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
                }),
                Some(message_id.to_string()),
                Some(message_id.to_string()),
                now,
            )],
            cx,
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
            cx,
        );
        self.ai.state.read(cx).persist_chat_state();
        cx.notify();
    }
}
