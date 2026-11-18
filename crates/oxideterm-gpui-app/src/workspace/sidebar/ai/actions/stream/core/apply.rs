use oxideterm_gpui_ai::state::{
    AiStreamApplyOutcome, detect_ai_cli_agent_kind, sanitize_ai_tool_arguments_for_approval,
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
            ai.apply_stream_event_state(generation, conversation_id, message_id, event, safe_error)
        });
        match outcome {
            // The delivery batch invalidates the assistant view once after
            // applying its deltas. Only lifecycle effects below reach the shell.
            AiStreamApplyOutcome::Applied => return,
            AiStreamApplyOutcome::Completed => {
                self.ai.authority.update(cx, |runtime, _cx| {
                    runtime.finish_tool_session(
                        generation,
                        oxideterm_ai::RuntimeRevocationReason::ToolSessionFinished,
                    );
                });
                if child_message {
                    self.ai.state.read(cx).persist_agent_records();
                    return;
                }
                self.persist_ai_assistant_turn_end(conversation_id, message_id, "complete", cx);
                if !self.send_next_queued_ai_message(conversation_id, cx) {
                    self.maybe_start_ai_auto_compaction(conversation_id, cx);
                }
            }
            AiStreamApplyOutcome::Failed(safe_error) => {
                self.ai.authority.update(cx, |runtime, _cx| {
                    runtime.finish_tool_session(
                        generation,
                        oxideterm_ai::RuntimeRevocationReason::ToolSessionFinished,
                    );
                });
                if child_message {
                    self.notify_ai_agent_attention(
                        conversation_id,
                        message_id,
                        "ai.agents.failed",
                        cx,
                    );
                    self.ai.state.read(cx).persist_agent_records();
                    return;
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
        self.ai.state.update(cx, |state, cx| {
            state.apply_ai_round_summary(
                generation,
                conversation_id,
                message_id,
                round_id,
                text,
                metadata,
                cx,
            );
        });
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
        self.ai.state.update(cx, |state, cx| {
            state.apply_ai_round_stateful_marker(
                generation,
                conversation_id,
                message_id,
                round_id,
                marker,
                cx,
            );
        });
    }

    pub(in crate::workspace) fn persist_ai_stream_diagnostic(
        &self,
        generation: u64,
        conversation_id: &str,
        message_id: &str,
        event_type: &str,
        round_id: Option<String>,
        data: serde_json::Value,
        cx: &mut App,
    ) {
        self.ai.state.update(cx, |state, _| {
            state.persist_ai_stream_diagnostic(
                generation,
                conversation_id,
                message_id,
                event_type,
                round_id,
                data,
                |data| self.ai_diagnostic_base(data),
            );
        });
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
        if status == "pending_user_approval"
            && self.ai.state.read(cx).is_chat_stream_generation(generation)
        {
            self.notify_ai_agent_attention(conversation_id, message_id, "ai.agents.approval", cx);
        }
        self.ai.state.update(cx, |state, cx| {
            state.apply_ai_tool_status(
                generation,
                conversation_id,
                message_id,
                tool_call_id,
                name,
                arguments,
                status,
                result,
                risk,
                summary,
                synthetic_denied,
                _raw_text,
                round_id_override,
                round_number_override,
                |data| self.ai_diagnostic_base(data),
                cx,
            );
        });
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
        self.ai.state.update(cx, |state, cx| {
            state.apply_ai_guardrail(
                generation,
                conversation_id,
                message_id,
                code,
                message,
                raw_text,
                |data| self.ai_diagnostic_base(data),
                cx,
            );
        });
    }
}
