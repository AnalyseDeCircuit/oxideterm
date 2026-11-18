use oxideterm_ai::{AiChatMessage, AiConversation, AiStreamEvent, stream_state::*};

use super::{
    AiAcpSessionState, AiToolExecutionRecord, AiToolResultFact, AiWorkspaceEntity,
    ai_acp_session_state, ai_now_ms, model::AI_ACP_SESSION_METADATA_KEY,
};

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

pub fn apply_ai_acp_session_started_to_conversations(
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
            plan: previous_state.as_ref().and_then(|state| state.plan.clone()),
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

pub fn sanitize_ai_tool_arguments_for_persistence(arguments: &str) -> String {
    // Execution payloads are current-turn data. Durable history keeps only
    // safe descriptors such as resource kind, path, and non-authority options.
    oxideterm_ai::sanitize_tool_arguments_text_for_persistence(arguments)
}

pub fn sanitize_ai_tool_arguments_for_approval(arguments: &str) -> String {
    // Approval is local and immediate, so a redacted command or input summary
    // may remain visible without entering the persistence projection.
    oxideterm_ai::sanitize_json_text_for_persistence(arguments)
}

pub enum AiStreamApplyOutcome {
    Applied,
    Completed,
    Failed(String),
    Stale,
}

impl AiWorkspaceEntity {
    pub fn mark_acp_handoff_cursor(
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
        self.history_metadata_changed(conversation_id);
        self.persist_chat_state();
        true
    }

    pub fn apply_acp_session_state_update(
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
        let title_changed = matches!(&update, oxideterm_ai::AcpSessionStateUpdate::SessionInfo { title: Some(title), .. } if !title.trim().is_empty());
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
        self.history_metadata_changed(conversation_id);
        if title_changed {
            self.history_title_changed(conversation_id);
        }
        self.persist_chat_state();
        true
    }

    pub fn apply_acp_session_started_state(
        &mut self,
        generation: u64,
        conversation_id: &str,
        session_id: &str,
        session_metadata: Option<serde_json::Value>,
        session_config_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
        session_modes: Option<oxideterm_ai::AcpSessionModeState>,
        agent_id: &str,
    ) -> bool {
        let current_generation = if self.is_chat_stream_generation(generation) {
            generation
        } else {
            return false;
        };
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
            self.history_metadata_changed(conversation_id);
            self.persist_chat_state();
        }
        applied
    }

    pub fn apply_stream_event_state(
        &mut self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        event: AiStreamEvent,
        safe_error: Option<String>,
    ) -> AiStreamApplyOutcome {
        if !self.chat_stream_matches(generation, conversation_id, message_id) {
            return AiStreamApplyOutcome::Stale;
        }
        match event {
            AiStreamEvent::Usage { .. } => AiStreamApplyOutcome::Applied,
            AiStreamEvent::Content(chunk) => {
                self.append_chat_stream_text(conversation_id, message_id, &chunk, false);
                AiStreamApplyOutcome::Applied
            }
            AiStreamEvent::Thinking(chunk) => {
                self.append_chat_stream_text(conversation_id, message_id, &chunk, true);
                AiStreamApplyOutcome::Applied
            }
            AiStreamEvent::ProviderResponsePart {
                provider_type,
                part,
            } => {
                self.update_chat_message(conversation_id, message_id, |message| {
                    oxideterm_ai::append_responses_round(message, &provider_type, part);
                });
                AiStreamApplyOutcome::Applied
            }
            AiStreamEvent::ToolCall {
                id,
                name,
                arguments,
            } => {
                let persisted_arguments = sanitize_ai_tool_arguments_for_persistence(&arguments);
                self.update_chat_message(conversation_id, message_id, |message| {
                    upsert_ai_tool_call(message, &id, &name, &persisted_arguments, "running");
                    upsert_ai_turn_tool_call(message, &id, &name, &persisted_arguments, "partial");
                });
                AiStreamApplyOutcome::Applied
            }
            AiStreamEvent::ToolCallComplete {
                id,
                name,
                arguments,
            } => {
                let persisted_arguments = sanitize_ai_tool_arguments_for_persistence(&arguments);
                self.update_chat_message(conversation_id, message_id, |message| {
                    upsert_ai_tool_call(message, &id, &name, &persisted_arguments, "pending");
                    upsert_ai_turn_tool_call(message, &id, &name, &persisted_arguments, "complete");
                });
                AiStreamApplyOutcome::Applied
            }
            AiStreamEvent::Done => {
                let child = self.is_child_stream(generation);
                let agent = self.agent_run(generation);
                self.update_chat_message(conversation_id, message_id, |message| {
                    // Older prompts asked models to append a private evidence block.
                    // Keep the visible answer and remove only that transport artifact.
                    strip_ai_evidence_claims(message);
                    finalize_ai_turn_suggestions(message);
                    message.is_streaming = false;
                    set_ai_turn_status(message, "complete");
                });
                self.complete_chat_stream(generation);
                if !child {
                    self.set_conversation_loading(conversation_id, false);
                    if let Some(agent) = agent {
                        self.agents.groups.remove(&agent.group_id);
                    }
                }
                self.refresh_agent_records();
                self.persist_chat_state();
                AiStreamApplyOutcome::Completed
            }
            AiStreamEvent::Error(_) => {
                let child = self.is_child_stream(generation);
                let agent = self.agent_run(generation);
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
                if !child {
                    self.set_conversation_loading(conversation_id, false);
                    if let Some(agent) = agent {
                        self.agents.groups.remove(&agent.group_id);
                    }
                }
                self.refresh_agent_records();
                self.persist_chat_state();
                AiStreamApplyOutcome::Failed(safe_error)
            }
        }
    }
}

pub fn store_ai_acp_handoff_cursor_in_conversation(
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

impl AiWorkspaceEntity {
    #[allow(clippy::too_many_arguments)]
    pub fn record_ai_tool_execution_status(
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

    pub fn record_ai_tool_result_facts(
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

pub fn strip_ai_evidence_claims(message: &mut AiChatMessage) {
    message.content = strip_ai_evidence_claims_from_text(&message.content);
    strip_ai_evidence_claims_block_from_turn_text_parts(message);
}

pub fn strip_ai_evidence_claims_from_text(text: &str) -> String {
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

pub fn extract_ai_evidence_claims_block(text: &str) -> Result<Option<(String, String)>, String> {
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

pub fn strip_ai_evidence_claims_block_from_turn_text_parts(message: &mut AiChatMessage) {
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

pub fn ai_tool_argument_summary(tool_name: &str, args: Option<&serde_json::Value>) -> String {
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

pub fn ai_tool_argument_resource_kind(
    args: Option<&serde_json::Value>,
) -> Option<oxideterm_ai::StableResourceKind> {
    args.and_then(|value| value.get("resource_ref"))
        .cloned()
        .and_then(|value| serde_json::from_value::<oxideterm_ai::StableResourceRef>(value).ok())
        .map(|resource_ref| resource_ref.kind())
}

pub fn ai_tool_result_resource_kind(
    result: Option<&serde_json::Value>,
) -> Option<oxideterm_ai::StableResourceKind> {
    result
        .and_then(|value| value.get("targets"))
        .and_then(serde_json::Value::as_array)
        .and_then(|targets| targets.first())
        .and_then(|target| target.pointer("/authority/resource_ref"))
        .cloned()
        .and_then(|value| serde_json::from_value::<oxideterm_ai::StableResourceRef>(value).ok())
        .map(|resource_ref| resource_ref.kind())
}

pub fn ai_tool_result_target_kind(result: Option<&serde_json::Value>) -> Option<String> {
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

pub fn ai_tool_visible_in_terminal(result: Option<&serde_json::Value>) -> Option<bool> {
    result
        .and_then(|value| value.pointer("/execution/visibleInTerminal"))
        .or_else(|| result.and_then(|value| value.pointer("/data/visibleInTerminal")))
        .and_then(serde_json::Value::as_bool)
}

pub fn ai_tool_execution_surface(
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

pub fn ai_tool_approval_source(status: &str, result: Option<&serde_json::Value>) -> Option<String> {
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

pub fn ai_tool_error_code(result: Option<&serde_json::Value>) -> Option<String> {
    result
        .and_then(|value| value.pointer("/error/code"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

pub fn ai_tool_duration_ms(result: Option<&serde_json::Value>) -> Option<u64> {
    result
        .and_then(|value| value.pointer("/meta/durationMs"))
        .and_then(serde_json::Value::as_u64)
}

pub fn ai_tool_execution_record_json(record: &AiToolExecutionRecord) -> serde_json::Value {
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

pub fn extract_ai_tool_result_facts(
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

pub fn ai_tool_result_fact(
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

pub fn ai_tool_result_fact_json(fact: &AiToolResultFact) -> serde_json::Value {
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

pub fn ai_fact_value_text(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| value.to_string())
}

pub fn truncate_ai_tool_record_text(value: &str, max_chars: usize) -> String {
    let mut result = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        result.push_str("...");
    }
    result
}

pub fn detect_ai_cli_agent_kind(command: &str) -> Option<String> {
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

#[cfg(test)]
mod tests;
