fn synchronize_ai_acp_config_selections(
    config_options: &[oxideterm_ai::AcpSessionConfigOption],
    model_selection: &mut Option<oxideterm_ai::AcpSessionConfigSelection>,
    config_selections: &mut Vec<oxideterm_ai::AcpSessionConfigSelection>,
) {
    // The protocol response is the authoritative complete catalog. Rebuild
    // selections from its current values so rejected or agent-adjusted choices
    // can never remain persisted as if they had succeeded.
    *config_selections = config_options
        .iter()
        .filter(|option| {
            option
                .choices
                .iter()
                .any(|choice| choice.value_id == option.current_value_id)
        })
        .map(|option| oxideterm_ai::AcpSessionConfigSelection {
            config_id: option.config_id.clone(),
            value_id: option.current_value_id.clone(),
        })
        .collect();
    *model_selection = oxideterm_ai::acp_model_config_option(config_options).and_then(|option| {
        config_selections
            .iter()
            .find(|selection| selection.config_id == option.config_id)
            .cloned()
    });
}

pub(in crate::workspace) fn apply_ai_acp_session_started_to_conversations(
    conversations: &mut [AiConversation],
    current_generation: u64,
    delivery_generation: u64,
    conversation_id: &str,
    session_id: &str,
    session_metadata: Option<serde_json::Value>,
    session_config_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
    session_modes: Option<oxideterm_ai::AcpSessionModeState>,
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
    let previous_state =
        ai_acp_session_state(conversation).filter(|state| state.agent_id == agent_id);
    let mut model_selection = previous_state
        .as_ref()
        .and_then(|state| state.model_selection.clone());
    let mut config_selections = previous_state
        .as_ref()
        .map(|state| state.config_selections.clone())
        .unwrap_or_default();
    synchronize_ai_acp_config_selections(
        &session_config_options,
        &mut model_selection,
        &mut config_selections,
    );
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
            config_selections,
            current_mode_id: session_modes
                .as_ref()
                .map(|modes| modes.current_mode_id.clone()),
            available_modes: session_modes
                .as_ref()
                .map(|modes| modes.available_modes.clone())
                .unwrap_or_default(),
            available_commands: previous_state
                .as_ref()
                .map(|state| state.available_commands.clone())
                .unwrap_or_default(),
            plan: previous_state
                .as_ref()
                .and_then(|state| state.plan.clone()),
            usage: previous_state
                .as_ref()
                .and_then(|state| state.usage.clone()),
            title: previous_state
                .as_ref()
                .and_then(|state| state.title.clone()),
            handoff_cursor: previous_state
                .as_ref()
                .filter(|state| state.session_id.is_empty() || state.session_id == session_id)
                .and_then(|state| state.handoff_cursor.clone()),
        };
        if let Ok(value) = serde_json::to_value(state) {
            object.insert(AI_ACP_SESSION_METADATA_KEY.to_string(), value);
        }
    }
    true
}

pub(in crate::workspace) fn sanitize_ai_tool_arguments_for_persistence(
    arguments: &str,
) -> String {
    // Execution payloads are current-turn data. Durable history keeps only
    // safe descriptors such as resource kind, path, and non-authority options.
    oxideterm_ai::sanitize_tool_arguments_text_for_persistence(arguments)
}

pub(in crate::workspace) fn sanitize_ai_tool_arguments_for_approval(arguments: &str) -> String {
    // Approval is local and immediate, so a redacted command or input summary
    // may remain visible without entering the persistence projection.
    oxideterm_ai::sanitize_json_text_for_persistence(arguments)
}

enum AiStreamApplyOutcome {
    Applied,
    Completed,
    Failed(String),
    Stale,
}

impl AiWorkspaceEntity {
    pub(in crate::workspace) fn mark_acp_handoff_cursor(
        &mut self,
        conversation_id: &str,
        agent_id: &str,
        message_id: &str,
    ) -> bool {
        let Some(conversation) = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return false;
        };
        if !store_ai_acp_handoff_cursor_in_conversation(conversation, agent_id, message_id) {
            return false;
        }
        // Advance only after the ACP prompt completed successfully. Failed or
        // cancelled turns retain the previous cursor so context is never lost.
        self.persist_chat_state();
        true
    }

    pub(in crate::workspace) fn apply_acp_session_state_update(
        &mut self,
        conversation_id: &str,
        update: oxideterm_ai::AcpSessionStateUpdate,
    ) -> bool {
        let Some(conversation) = self
            .conversation_state_mut()
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return false;
        };
        let Some(mut state) = ai_acp_session_state(conversation) else {
            return false;
        };
        match update {
            oxideterm_ai::AcpSessionStateUpdate::ConfigOptions(config_options) => {
                synchronize_ai_acp_config_selections(
                    &config_options,
                    &mut state.model_selection,
                    &mut state.config_selections,
                );
                state.config_options = config_options;
            }
            oxideterm_ai::AcpSessionStateUpdate::CurrentMode(mode_id) => {
                state.current_mode_id = Some(mode_id);
            }
            oxideterm_ai::AcpSessionStateUpdate::AvailableCommands(commands) => {
                state.available_commands = commands;
            }
            oxideterm_ai::AcpSessionStateUpdate::Plan(plan) => state.plan = Some(plan),
            oxideterm_ai::AcpSessionStateUpdate::SessionInfo { title, .. } => {
                if let Some(title) = title
                    .as_deref()
                    .map(str::trim)
                    .filter(|title| !title.is_empty())
                {
                    conversation.title = title.to_string();
                    conversation.updated_at_ms = ai_now_ms();
                }
                state.title = title;
            }
            oxideterm_ai::AcpSessionStateUpdate::Usage(usage) => state.usage = Some(usage),
        }
        let metadata = conversation
            .session_metadata
            .get_or_insert_with(|| serde_json::json!({}));
        let Some(metadata) = metadata.as_object_mut() else {
            return false;
        };
        let Ok(value) = serde_json::to_value(state) else {
            return false;
        };
        metadata.insert(AI_ACP_SESSION_METADATA_KEY.to_string(), value);
        self.persist_chat_state();
        true
    }

    fn apply_acp_session_started_state(
        &mut self,
        generation: u64,
        conversation_id: &str,
        session_id: &str,
        session_metadata: Option<serde_json::Value>,
        session_config_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
        session_modes: Option<oxideterm_ai::AcpSessionModeState>,
        agent_id: &str,
    ) -> bool {
        let current_generation = self.chat_stream_generation();
        let applied = apply_ai_acp_session_started_to_conversations(
            &mut self.conversation_state_mut().conversations,
            current_generation,
            generation,
            conversation_id,
            session_id,
            session_metadata,
            session_config_options,
            session_modes,
            agent_id,
        );
        if applied {
            self.persist_chat_state();
        }
        applied
    }

    fn apply_stream_event_state(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        event: AiStreamEvent,
        safe_error: Option<String>,
    ) -> AiStreamApplyOutcome {
        if !self.is_chat_stream_generation(generation) {
            return AiStreamApplyOutcome::Stale;
        }
        match event {
            AiStreamEvent::Content(chunk) => {
                self.update_chat_message(conversation_id, message_id, |message| {
                    message.content.push_str(&chunk);
                    append_ai_turn_text_part(message, "text", &chunk, false);
                });
                AiStreamApplyOutcome::Applied
            }
            AiStreamEvent::Thinking(chunk) => {
                self.update_chat_message(conversation_id, message_id, |message| {
                    message
                        .thinking_content
                        .get_or_insert_with(String::new)
                        .push_str(&chunk);
                    append_ai_turn_text_part(message, "thinking", &chunk, true);
                });
                AiStreamApplyOutcome::Applied
            }
            AiStreamEvent::ProviderResponsePart { .. } => {
                // The live tool loop consumes provider replay metadata before
                // UI delivery; other stream surfaces intentionally ignore it.
                AiStreamApplyOutcome::Applied
            }
            AiStreamEvent::ToolCall {
                id,
                name,
                arguments,
            } => {
                let persisted_arguments =
                    sanitize_ai_tool_arguments_for_persistence(&arguments);
                self.update_chat_message(conversation_id, message_id, |message| {
                    upsert_ai_tool_call(
                        message,
                        &id,
                        &name,
                        &persisted_arguments,
                        "running",
                    );
                    upsert_ai_turn_tool_call(
                        message,
                        &id,
                        &name,
                        &persisted_arguments,
                        "partial",
                    );
                });
                AiStreamApplyOutcome::Applied
            }
            AiStreamEvent::ToolCallComplete {
                id,
                name,
                arguments,
            } => {
                let persisted_arguments =
                    sanitize_ai_tool_arguments_for_persistence(&arguments);
                self.update_chat_message(conversation_id, message_id, |message| {
                    upsert_ai_tool_call(
                        message,
                        &id,
                        &name,
                        &persisted_arguments,
                        "pending",
                    );
                    upsert_ai_turn_tool_call(
                        message,
                        &id,
                        &name,
                        &persisted_arguments,
                        "complete",
                    );
                });
                AiStreamApplyOutcome::Applied
            }
            AiStreamEvent::Done => {
                self.update_chat_message(conversation_id, message_id, |message| {
                    // Older prompts asked models to append a private evidence block.
                    // Keep the visible answer and remove only that transport artifact.
                    strip_ai_evidence_claims(message);
                    finalize_ai_turn_suggestions(message);
                    message.is_streaming = false;
                    set_ai_turn_status(message, "complete");
                });
                self.complete_chat_stream(generation);
                self.set_chat_loading(false);
                self.persist_chat_state();
                AiStreamApplyOutcome::Completed
            }
            AiStreamEvent::Error(_) => {
                let safe_error = safe_error.unwrap_or_default();
                self.update_chat_message(conversation_id, message_id, |message| {
                    message.is_streaming = false;
                    if message.content.is_empty() {
                        message.content = safe_error.clone();
                    } else {
                        message.content.push_str("\n\n");
                        message.content.push_str(&safe_error);
                    }
                    append_ai_turn_error_part(message, &safe_error);
                    set_ai_turn_status(message, "error");
                });
                self.complete_chat_stream(generation);
                self.set_chat_loading(false);
                self.persist_chat_state();
                AiStreamApplyOutcome::Failed(safe_error)
            }
        }
    }
}

pub(in crate::workspace) fn store_ai_acp_handoff_cursor_in_conversation(
    conversation: &mut AiConversation,
    agent_id: &str,
    message_id: &str,
) -> bool {
    let Some(mut state) =
        ai_acp_session_state(conversation).filter(|state| state.agent_id == agent_id)
    else {
        return false;
    };
    let Some(cursor) = oxideterm_ai::acp_conversation_handoff_cursor(conversation, message_id)
    else {
        return false;
    };
    state.handoff_cursor = Some(cursor);
    let metadata = conversation
        .session_metadata
        .get_or_insert_with(|| serde_json::json!({}));
    let Some(metadata) = metadata.as_object_mut() else {
        return false;
    };
    let Ok(value) = serde_json::to_value(state) else {
        return false;
    };
    metadata.insert(AI_ACP_SESSION_METADATA_KEY.to_string(), value);
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
        session_modes: Option<oxideterm_ai::AcpSessionModeState>,
        agent_id: &str,
        cx: &mut App,
    ) -> bool {
        let applied = self.ai_entity.update(cx, |ai, _cx| {
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

    pub(in crate::workspace) fn apply_ai_stream_event(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        event: AiStreamEvent,
        cx: &mut Context<Self>,
    ) {
        let safe_error = matches!(&event, AiStreamEvent::Error(_))
            .then(|| self.i18n.t("settings_view.ai.acp_agent_error_unknown"));
        let outcome = self.ai_entity.update(cx, |ai, _cx| {
            ai.apply_stream_event_state(
                generation,
                conversation_id,
                message_id,
                event,
                safe_error,
            )
        });
        match outcome {
            AiStreamApplyOutcome::Applied => {}
            AiStreamApplyOutcome::Completed => {
                self.ai_runtime_context.update(cx, |runtime, _cx| {
                    runtime.finish_tool_session(
                        generation,
                        oxideterm_ai::RuntimeRevocationReason::ToolSessionFinished,
                    );
                });
                self.persist_ai_assistant_turn_end(
                    conversation_id,
                    message_id,
                    "complete",
                    cx,
                );
                self.maybe_start_ai_auto_compaction(conversation_id, cx);
            }
            AiStreamApplyOutcome::Failed(safe_error) => {
                self.ai_runtime_context.update(cx, |runtime, _cx| {
                    runtime.finish_tool_session(
                        generation,
                        oxideterm_ai::RuntimeRevocationReason::ToolSessionFinished,
                    );
                });
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
            .ai_entity
            .read(cx)
            .is_chat_stream_generation(generation)
        {
            return;
        }
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        self.ai_entity.update(cx, |ai, _cx| {
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
        self.ai_entity.read(cx).persist_chat_state();
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
            .ai_entity
            .read(cx)
            .is_chat_stream_generation(generation)
        {
            return;
        }
        self.ai_entity.update(cx, |ai, _cx| {
            ai.update_chat_message(conversation_id, message_id, |message| {
                set_ai_turn_round_stateful_marker(message, round_id, marker.as_deref());
            });
        });
        self.ai_entity.read(cx).persist_chat_state();
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
            .ai_entity
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
            .ai_entity
            .read(cx)
            .is_chat_stream_generation(generation)
        {
            return;
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
        self.ai_entity.update(cx, |ai, _cx| {
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
        if should_persist {
            let now = ai_now_ms();
            let round_id_value = round_id.clone();
            let round_number_value = round_number.unwrap_or(1);
            let tool_execution_record = self.ai_entity.update(cx, |ai, _cx| {
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
                    let facts = self.ai_entity.update(cx, |ai, _cx| {
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
            self.ai_entity.read(cx).persist_chat_state();
        }
        cx.notify();
    }
}

impl crate::workspace::ai_state::AiWorkspaceEntity {
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
            .tool_execution_records
            .iter()
            .position(|record| record.tool_call_id == tool_call_id);
        let mut record = existing
            .and_then(|index| self.tool_execution_records.remove(index))
            .unwrap_or_else(|| AiToolExecutionRecord {
                record_id: format!("tool-exec-{tool_call_id}"),
                conversation_id: conversation_id.to_string(),
                assistant_message_id: message_id.to_string(),
                tool_call_id: tool_call_id.to_string(),
                tool_name: tool_name.to_string(),
                argument_summary: ai_tool_argument_summary(tool_name, args.as_ref()),
                resource_kind: ai_tool_argument_resource_kind(args.as_ref()),
                target_kind: None,
                risk: risk.unwrap_or("read").to_string(),
                approval_source: None,
                execution_surface: ai_tool_execution_surface(tool_name, args.as_ref(), result),
                visible_in_terminal: None,
                status: status.to_string(),
                success: None,
                error_code: None,
                duration_ms: None,
                started_at: now,
                finished_at: None,
            });

        record.status = status.to_string();
        record.risk = risk.unwrap_or(&record.risk).to_string();
        record.argument_summary = ai_tool_argument_summary(tool_name, args.as_ref());
        record.resource_kind = ai_tool_result_resource_kind(result)
            .or_else(|| ai_tool_argument_resource_kind(args.as_ref()));
        record.target_kind = ai_tool_result_target_kind(result);
        record.execution_surface = ai_tool_execution_surface(tool_name, args.as_ref(), result);
        record.visible_in_terminal = ai_tool_visible_in_terminal(result);
        record.approval_source = ai_tool_approval_source(status, result);
        if matches!(status, "rejected" | "completed" | "error") {
            record.finished_at = Some(now);
            record.success = Some(status == "completed");
            record.error_code = ai_tool_error_code(result);
            record.duration_ms = ai_tool_duration_ms(result);
        }

        self.tool_execution_records.push_back(record.clone());
        while self.tool_execution_records.len() > 500 {
            self.tool_execution_records.pop_front();
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
            self.tool_result_facts.push_back(fact.clone());
        }
        while self.tool_result_facts.len() > 1000 {
            self.tool_result_facts.pop_front();
        }
        facts
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
            .ai_entity
            .read(cx)
            .is_chat_stream_generation(generation)
        {
            return;
        }
        let persisted_raw_text = raw_text
            .as_deref()
            .map(oxideterm_ai::sanitize_for_ai);
        self.ai_entity.update(cx, |ai, _cx| {
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
        self.ai_entity.read(cx).persist_chat_state();
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
            let command_chars = args
                .get("command")
                .and_then(serde_json::Value::as_str)
                .map(str::chars)
                .map(Iterator::count)
                .unwrap_or(0);
            let has_cwd = args
                .get("cwd")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| !value.trim().is_empty());
            format!("runtime_target=current command_chars={command_chars} has_cwd={has_cwd}")
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
            format!("resource={resource}")
        }
        "connect_target" => "resource=saved_connection".to_string(),
        "open_app_surface" => "resource=app_surface".to_string(),
        _ => {
            let mut keys = args.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            format!("keys={}", keys.join(","))
        }
    }
}

pub(in crate::workspace) fn ai_tool_argument_resource_kind(
    args: Option<&serde_json::Value>,
) -> Option<oxideterm_ai::StableResourceKind> {
    args.and_then(|value| value.get("resource_ref"))
        .cloned()
        .and_then(|value| {
            serde_json::from_value::<oxideterm_ai::StableResourceRef>(value).ok()
        })
        .map(|resource_ref| resource_ref.kind())
}

pub(in crate::workspace) fn ai_tool_result_resource_kind(
    result: Option<&serde_json::Value>,
) -> Option<oxideterm_ai::StableResourceKind> {
    result
        .and_then(|value| value.get("targets"))
        .and_then(serde_json::Value::as_array)
        .and_then(|targets| targets.first())
        .and_then(|target| target.pointer("/authority/resource_ref"))
        .cloned()
        .and_then(|value| {
            serde_json::from_value::<oxideterm_ai::StableResourceRef>(value).ok()
        })
        .map(|resource_ref| resource_ref.kind())
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
        "run_command" => "background_capture".to_string(),
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
        // Diagnostics retain the resource class but never a stable identifier.
        "resourceKind": record.resource_kind,
        "targetKind": record.target_kind,
        "risk": record.risk,
        "approvalSource": record.approval_source,
        "executionSurface": record.execution_surface,
        "visibleInTerminal": record.visible_in_terminal,
        "status": record.status,
        "success": record.success,
        "errorCode": record.error_code,
        "durationMs": record.duration_ms,
        "startedAt": record.started_at,
        "finishedAt": record.finished_at,
        "historical": true,
        "actionable": false,
    })
}

pub(in crate::workspace) fn extract_ai_tool_result_facts(
    record: &AiToolExecutionRecord,
    result: Option<&serde_json::Value>,
    now: i64,
) -> Vec<AiToolResultFact> {
    let mut facts = Vec::new();
    // Default diagnostics retain only structured execution state. Human-readable
    // summaries and output remain in the redacted conversation projection.
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
    // Only bounded structured state reaches this helper.
    let safe_text = oxideterm_ai::sanitize_for_ai(text);
    AiToolResultFact {
        fact_id: format!("{}.{}", record.tool_call_id, source_kind),
        conversation_id: record.conversation_id.clone(),
        assistant_message_id: record.assistant_message_id.clone(),
        tool_call_id: record.tool_call_id.clone(),
        tool_name: record.tool_name.clone(),
        source_kind: source_kind.to_string(),
        summary: truncate_ai_tool_record_text(&safe_text, 240),
        created_at: now,
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
        "summary": fact.summary,
        "createdAt": fact.created_at,
        "historical": true,
        "actionable": false,
    })
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
