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
        self.edit_settings(|settings| {
            oxideterm_ai::tool_resources::record_memory_usage(&mut settings.ai.memory, entry_ids);
        }, cx);
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
            self.global_agent_options().enabled, settings, &self.i18n, self.ai_web_provider_id().is_ok(), host.clone(),
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
