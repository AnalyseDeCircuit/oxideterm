use oxideterm_gpui_ai::conversation::AiProviderSendPlan;

impl WorkspaceApp {
    pub(in crate::workspace) fn start_acp_chat_thread(
        &mut self,
        conversation_id: String,
        config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let launch = match self.ai_acp_chat_launch(&config, cx).and_then(|launch| {
            launch.ok_or_else(|| "ACP launch configuration was not prepared.".to_string())
        }) {
            Ok(launch) => launch,
            Err(_) => {
                self.ai.state.update(cx, |ai, _cx| {
                    ai.set_conversation_loading(&conversation_id, false)
                });
                let agent = self
                    .settings_store
                    .settings()
                    .ai
                    .acp_agents
                    .iter()
                    .find(|agent| Some(&agent.id) == config.acp_agent_id.as_ref())
                    .cloned();
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
            conversation_id,
            config,
            request_content,
            task_system_prompt,
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
        &mut self,
        conversation_id: String,
        config: AiChatStreamConfig,
        request_content: Option<String>,
        task_system_prompt: Option<String>,
        rag_system_prompt: Option<String>,
        allow_pre_send_compaction: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.ai.state.update(cx, |ai, cx| {
            let launch_id = ai.chat_launches.get(&conversation_id).cloned();
            ai.defer_for_unsupported_images(
                AiPendingChatStream {
                    launch_id,
                    conversation_id,
                    config,
                    request_content,
                    task_system_prompt,
                    rag_system_prompt,
                },
                cx,
            )
        }) else {
            return;
        };
        match self.ai_conversation_prompt(cx).plan_provider_turn(pending, allow_pre_send_compaction) {
            Some(AiProviderSendPlan::Compact(pending)) => {
                let id = pending.conversation_id.clone();
                if let Err(Some(pending)) = self.start_ai_compact_conversation_for(id, true, true, Some(pending), cx) {
                    self.start_ai_chat_stream_after_budget_preflight(
                        pending.conversation_id, pending.config, pending.request_content,
                        pending.task_system_prompt, pending.rag_system_prompt, false, cx,
                    );
                }
            }
            Some(AiProviderSendPlan::Launch(turn)) => {
                let services = self.ai_model_backend_services(cx);
                self.ai.conversations.launch_provider_turn(
                    *turn, services, self.global_agent_options(), self.agent_concurrency(),
                    |data| self.ai_diagnostic_base(data), cx,
                );
            }
            None => {}
        }
    }
}
