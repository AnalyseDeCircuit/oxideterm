use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn test_ai_acp_agent(
        &mut self,
        agent_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.ai.state.read(cx).acp_agent_probe_is_pending(&agent_id) {
            return;
        }
        let Some(mut agent) = self
            .settings_store
            .settings()
            .ai
            .acp_agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .cloned()
        else {
            return;
        };
        if let Err(message) = self.resolve_ai_acp_plugin(&mut agent, cx) {
            if !self.show_acp_plugin_requirement(&agent, cx) {
                self.push_ai_settings_toast(message, TerminalNoticeVariant::Error, cx);
            }
            return;
        }
        if self
            .ai
            .state
            .update(cx, |ai, _cx| ai.request_acp_agent_probe(agent))
        {
            self.settings_views.notify_acp(cx);
            self.ai.transcript.update(cx, |_, cx| cx.notify());
        }
    }
}
