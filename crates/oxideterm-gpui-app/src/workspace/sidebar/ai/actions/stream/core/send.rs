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
        let AiPendingChatStream {
            conversation_id,
            config,
            request_content,
            task_system_prompt,
            rag_system_prompt,
            launch_id,
        } = pending;
        if allow_pre_send_compaction
            && self.should_force_ai_pre_send_compaction(
                &conversation_id,
                &config,
                request_content.as_deref(),
                task_system_prompt.as_deref(),
                rag_system_prompt.as_deref(),
                cx,
            )
        {
            let pending = AiPendingChatStream {
                launch_id,
                conversation_id: conversation_id.clone(),
                config,
                request_content,
                task_system_prompt,
                rag_system_prompt,
            };
            let pending = match self.start_ai_compact_conversation_for(
                conversation_id,
                true,
                true,
                Some(pending),
                cx,
            ) {
                Ok(()) => return,
                Err(Some(pending)) => *pending,
                Err(None) => return,
            };

            return self.start_ai_chat_stream_after_budget_preflight(
                pending.conversation_id,
                pending.config,
                pending.request_content,
                pending.task_system_prompt,
                pending.rag_system_prompt,
                false,
                cx,
            );
        }

        let Some((history, trimmed_count)) = self.ai_conversation_prompt(cx).build_ai_stream_history(
            &conversation_id,
            &config,
            request_content.clone(),
            task_system_prompt.clone(),
            rag_system_prompt.clone(),
        ) else {
            return;
        };
        let context_window = self.ai_active_model_context_window(&config);
        let (budget_decision, budget_diagnostic_payload) = self
            .ai
            .state
            .read(cx)
            .conversation_state()
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .map(|conversation| {
                let decision = self.ai_conversation_prompt(cx).ai_send_budget_decision(
                    conversation,
                    &config,
                    request_content.as_deref(),
                    task_system_prompt.as_deref(),
                    rag_system_prompt.as_deref(),
                );
                let payload = self.ai_conversation_prompt(cx).ai_budget_diagnostic_payload(
                    conversation,
                    &config,
                    request_content.as_deref(),
                    task_system_prompt.as_deref(),
                    rag_system_prompt.as_deref(),
                    decision,
                    trimmed_count,
                );
                (decision, payload)
            })
            .unwrap_or_else(|| {
                let decision = None;
                let payload = serde_json::json!({
                    "requestKind": "chat",
                    "budgetLevel": 0,
                    "nextLevel": 0,
                    "contextWindow": self.ai_active_model_context_window(&config),
                    "responseReserve": config.max_response_tokens,
                    "trimmedCount": trimmed_count,
                });
                (decision, payload)
            });
        let provider_turn = oxideterm_gpui_ai::conversation::AiProviderTurn {
            conversation_id,
            config,
            request_content,
            history,
            trimmed_count,
            context_window,
            budget_level: budget_decision.map(|decision| decision.level).unwrap_or(0),
            budget_diagnostic_payload,
            services: self.ai_model_backend_services(cx),
            agent_options: self.global_agent_options(),
            agent_concurrency: self.agent_concurrency(),
        };
        self.ai.conversations.launch_provider_turn(
            provider_turn,
            |data| self.ai_diagnostic_base(data),
            cx,
        );
    }
}
