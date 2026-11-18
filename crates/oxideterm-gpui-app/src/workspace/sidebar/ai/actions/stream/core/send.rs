use oxideterm_gpui_ai::conversation::AiProviderSendPlan;

impl WorkspaceApp {
    pub(in crate::workspace) fn start_acp_chat_thread(
        &mut self,
        turn: AiPendingChatStream,
        cx: &mut Context<Self>,
    ) {
        let launch = match self.ai_acp_chat_launch(&turn.config, cx).and_then(|launch| {
            launch.ok_or_else(|| "ACP launch configuration was not prepared.".to_string())
        }) {
            Ok(launch) => launch,
            Err(_) => {
                let agent = self.ai.state.update(cx, |ai, _| {
                    ai.fail_acp_launch(&turn, &self.settings_store.settings().ai)
                });
                if agent
                    .as_ref()
                    .is_some_and(|agent| self.show_acp_plugin_requirement(agent, cx))
                {
                    return;
                }
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.ai.acp_agent_error_unknown"),
                    TerminalNoticeVariant::Error,
                    cx,
                );
                return;
            }
        };
        let skill_catalog = self.ai_skill_catalog_prompt();
        let services = self.ai_model_backend_services(cx);
        if !self.ai.conversations.start_acp_turn(
            turn.conversation_id, turn.config, turn.request_content, turn.task_system_prompt,
            launch,
            skill_catalog,
            services,
            cx,
        ) {
            self.push_ai_settings_toast(
                self.i18n.t("settings_view.ai.acp_agent_error_unknown"),
                TerminalNoticeVariant::Error,
                cx,
            );
        }
    }

    pub(in crate::workspace) fn start_ai_chat_stream_after_budget_preflight(
        &mut self, turn: AiPendingChatStream, allow_compaction: bool, cx: &mut Context<Self>,
    ) {
        let Some(turn) = self.ai.state.update(cx, |ai, cx| ai.prepare_provider_send(turn, cx)) else { return };
        self.send_prepared_provider_turn(turn, allow_compaction, cx);
    }

    fn send_prepared_provider_turn(&mut self, pending: AiPendingChatStream, allow_compaction: bool, cx: &mut Context<Self>) {
        match self.ai_conversation_prompt(cx).plan_provider_turn(pending, allow_compaction) {
            Some(AiProviderSendPlan::Compact(pending)) => {
                let id = pending.conversation_id.clone();
                if let Err(Some(pending)) = self.start_ai_compact_conversation_for(id, true, true, Some(pending), cx) {
                    self.start_ai_chat_stream_after_budget_preflight(*pending, false, cx);
                }
            }
            Some(AiProviderSendPlan::Launch(turn)) => self.ai.conversations.launch_provider_turn(
                *turn, self.ai_model_backend_services(cx), self.global_agent_options(),
                self.agent_concurrency(), |data| self.ai_diagnostic_base(data), cx,
            ),
            None => {}
        }
    }
}
