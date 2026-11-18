use oxideterm_gpui_ai::state::sanitize_ai_tool_arguments_for_approval;

impl WorkspaceApp {
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
        use oxideterm_gpui_ai::conversation::AiChatCompletion;
        let safe_error = match &event {
            AiStreamEvent::Error(error) => Some(
                self.ai_stream_error_text(error)
                    .unwrap_or_else(|| self.i18n.t("settings_view.ai.acp_agent_error_unknown")),
            ),
            _ => None,
        };
        let completion = self.ai.conversations.apply_stream_event(
            generation,
            conversation_id,
            message_id,
            event,
            safe_error,
            self.settings_store.settings().ai.enabled,
            |payload| self.ai_diagnostic_base(payload),
            cx,
        );
        match completion {
            AiChatCompletion::None => {}
            AiChatCompletion::NextTurn(pending) => self.launch_prepared_queued_turn(pending, cx),
            AiChatCompletion::AutoCompact => {
                self.maybe_start_ai_auto_compaction(conversation_id, cx)
            }
            AiChatCompletion::AgentFailed => {
                self.notify_ai_agent_attention(conversation_id, message_id, "ai.agents.failed", cx)
            }
            AiChatCompletion::Error(message) => {
                self.push_ai_settings_toast(message, TerminalNoticeVariant::Error, cx)
            }
        }
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
}
