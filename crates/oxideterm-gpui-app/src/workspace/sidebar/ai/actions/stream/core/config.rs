use oxideterm_ai::chat_tools::ai_active_tool_count;
impl WorkspaceApp {
    pub(in crate::workspace) fn record_ai_memory_usage(
        &mut self,
        entry_ids: &[String],
        cx: &mut Context<Self>,
    ) {
        if entry_ids.is_empty() {
            return;
        }
        let entry_ids = entry_ids
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let now_ms = ai_memory_now_ms();
        self.edit_settings(
            move |settings| {
                for entry in settings
                    .ai
                    .memory
                    .entries
                    .iter_mut()
                    .filter(|entry| entry_ids.contains(&entry.id))
                {
                    entry.last_used_at_ms = Some(now_ms);
                    entry.use_count = entry.use_count.saturating_add(1);
                }
            },
            cx,
        );
    }

    pub(in crate::workspace) fn should_force_ai_pre_send_compaction(
        &self,
        conversation_id: &str,
        config: &AiChatStreamConfig,
        request_content: Option<&str>,
        task_system_prompt: Option<&str>,
        rag_system_prompt: Option<&str>,
        cx: &App,
    ) -> bool {
        self.ai_conversation_prompt(cx).should_force_ai_pre_send_compaction(
            conversation_id, config, request_content, task_system_prompt, rag_system_prompt,
        )
    }

    pub(in crate::workspace) fn resolve_ai_stream_config(&self, cx: &App) -> Result<AiChatStreamConfig, String> {
        let settings = &self.settings_store.settings().ai;
        let uses_scoped_memory = settings.memory.enabled && !settings.memory.entries.is_empty();
        let project = uses_scoped_memory
            .then(|| self.active_terminal_cwd_snapshot(cx))
            .flatten()
            .map(|snapshot| snapshot.path().to_string());
        let host = self.active_ssh_terminal_node_id(cx)
            .and_then(|id| self.node_router.resolve_connection_now(&id).ok())
            .map(|connection| connection.connection_id);
        let user = uses_scoped_memory.then(whoami::username);
        self.ai.state.read(cx).resolve_ai_stream_config(
            settings, &self.i18n, self.ai_web_provider_id().is_ok(), host.clone(),
            oxideterm_gpui_ai::conversation::AiMemoryScope {
                user: user.as_deref(), project: project.as_deref(), host: host.as_deref(),
            },
        )
    }

    pub(in crate::workspace) fn resolve_ai_summary_stream_config(
        &self,
        compact: bool,
        cx: &App,
    ) -> Result<AiChatStreamConfig, String> {
        self.ai.state.read(cx).resolve_ai_summary_stream_config(
            &self.settings_store.settings().ai, &self.i18n, compact,
        )
    }

    pub(in crate::workspace) fn ai_conversation_prompt<'a>(&'a self, cx: &'a App) -> oxideterm_gpui_ai::conversation::AiConversationPrompt<'a> {
        oxideterm_gpui_ai::conversation::AiConversationPrompt {
            state: self.ai.state.read(cx),
            settings: &self.settings_store.settings().ai,
            settings_updated_at: self.settings_store.updated_at(),
            skills: &self.ai.skills,
        }
    }

    pub(in crate::workspace) fn ai_diagnostic_base(
        &self,
        data: serde_json::Value,
    ) -> serde_json::Value {
        let ai = &self.settings_store.settings().ai;
        oxideterm_gpui_ai::stream::ai_stream_diagnostic(
            data, ai.tool_use.enabled, ai.active_provider_id.as_deref(), ai.active_model.as_deref(),
        )
    }

    pub(in crate::workspace) fn ai_skill_catalog_prompt(&self) -> Option<String> {
        self.ai.skills.prompt(self.settings_store.settings().ai.skills.enabled)
    }
}
