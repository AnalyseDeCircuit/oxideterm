impl WorkspaceApp {
    pub(in crate::workspace) fn ai_context_message_usage_counts(
        &self,
        cx: &App,
    ) -> Option<(usize, usize)> {
        ai_context_counts_for_backend(
            self.settings_store.settings().ai.active_backend,
            self.active_ai_acp_usage(cx),
            || {
                let breakdown = self.ai_context_token_breakdown(cx);
                (breakdown.total, breakdown.max_tokens)
            },
        )
    }

}
