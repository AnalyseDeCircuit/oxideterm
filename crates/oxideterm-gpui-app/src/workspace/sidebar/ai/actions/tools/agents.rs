use oxideterm_ai::agent::{
    AgentState, AgentText,
};

impl WorkspaceApp {
    fn notify_ai_agent_attention(
        &mut self,
        conversation_id: &str,
        message_id: &str,
        label: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(description) = self.ai.state.read(cx).agent_attention_description(conversation_id, message_id) else {
            return;
        };
        self.push_notification_entry(
            WorkspaceNotificationKind::Agent,
            WorkspaceNotificationSeverity::Warning,
            self.i18n.t(label),
            Some(description),
            WorkspaceNotificationScope::Global,
            Some(format!(
                "agent-attention:{conversation_id}:{message_id}:{label}"
            )),
        );
    }
    fn handle_ai_agent_command(
        &self,
        generation: u64,
        session: &ToolSessionId,
        call: AiToolCall,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        cx: &mut Context<Self>,
    ) {
        self.ai.conversations.handle_ai_agent_command(generation, session, call, sender, self.global_agent_options(), cx);
    }

}

impl WorkspaceApp {
    pub(in crate::workspace) fn global_agent_options(&self) -> oxideterm_ai::agent::AgentOptions {
        oxideterm_gpui_settings_view::ai::subagents::global_agent_options(self.settings_store.settings())
    }

    fn agent_concurrency(&self) -> usize {
        oxideterm_gpui_settings_view::ai::subagents::agent_concurrency(self.settings_store.settings())
    }

}
