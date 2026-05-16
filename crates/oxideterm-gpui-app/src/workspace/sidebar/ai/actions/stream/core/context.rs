impl WorkspaceApp {
    fn ai_active_model_context_window(&self, config: &AiChatStreamConfig) -> usize {
        let settings = self.settings_store.settings();
        config
            .provider_id
            .as_deref()
            .and_then(|provider_id| {
                ai_context_window_from_maps(
                    &settings.ai.user_context_windows,
                    &settings.ai.model_context_windows,
                    provider_id,
                    &config.model,
                )
            })
            .unwrap_or(AI_COMPACTION_DEFAULT_CONTEXT_WINDOW)
    }
}
