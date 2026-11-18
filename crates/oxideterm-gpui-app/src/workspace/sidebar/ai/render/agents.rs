impl WorkspaceApp {
    pub(in crate::workspace) fn global_agent_options(&self) -> oxideterm_ai::agent::AgentOptions {
        oxideterm_gpui_settings_view::ai::subagents::global_agent_options(self.settings_store.settings())
    }

    fn agent_concurrency(&self) -> usize {
        oxideterm_gpui_settings_view::ai::subagents::agent_concurrency(self.settings_store.settings())
    }

}
