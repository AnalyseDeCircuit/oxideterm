impl WorkspaceApp {
    pub(in crate::workspace) fn ai_active_model_context_window(&self, config: &AiChatStreamConfig) -> usize {
        oxideterm_gpui_ai::conversation::ai_model_context_window(
            &self.settings_store.settings().ai, config.provider_id.as_deref(), &config.model,
        )
    }
}
