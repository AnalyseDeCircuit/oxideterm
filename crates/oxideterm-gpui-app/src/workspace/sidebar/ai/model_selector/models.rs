impl WorkspaceApp {
    pub(in crate::workspace) fn active_ai_reasoning_level(
        &self,
        provider: &AiProviderView,
        model: &str,
        cx: &App,
    ) -> AiReasoningLevel {
        if let Some(value) = self
            .ai.state
            .read(cx)
            .conversation_state()
            .active_conversation()
            .and_then(|conversation| {
                ai_conversation_reasoning_effort(conversation, &provider.id, model)
            })
        {
            return oxideterm_ai::normalize_reasoning_level_for_model(
                &provider.provider_type,
                model,
                value,
            );
        }
        let settings = self.settings_store.settings();
        let value = settings
            .ai
            .reasoning_model_overrides
            .get(&provider.id)
            .and_then(|models| models.get(model))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("auto");
        oxideterm_ai::normalize_reasoning_level_for_model(&provider.provider_type, model, value)
    }

}
