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

    fn report_ai_tool_progress(
        &mut self, owner: &AiToolRunContext, call: &str, name: &str,
        status: &str, data: Option<serde_json::Value>, risk: &str, cx: &mut Context<Self>,
    ) {
        if status == "pending_user_approval" && self.ai.state.read(cx).is_chat_stream_generation(owner.generation) {
            self.notify_ai_agent_attention(&owner.conversation_id, &owner.assistant_id, "ai.agents.approval", cx);
        }
        self.ai.state.update(cx, |ai, cx| {
            ai.apply_owned_tool_progress(owner, call, name, status, data, risk, |value| self.ai_diagnostic_base(value), cx);
        });
    }
}
