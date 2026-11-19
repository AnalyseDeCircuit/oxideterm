use oxideterm_gpui_ai::model_picker::host::AiModelPickerHost;
use oxideterm_settings::{AcpAgentConfig, PersistedSettings};

impl AiModelPickerHost for WorkspaceApp {
    fn write_picker_settings(&mut self, edit: impl FnOnce(&mut PersistedSettings), cx: &mut Self::Cx<'_>) {
        self.edit_settings(edit, cx);
    }
    fn focus_picker_window(&self, window: &mut Window, cx: &mut Self::Cx<'_>) { window.focus(&self.focus_handle, cx); }
    fn apply_acp_picker_config(&mut self, agent: &str, config: String, value: String, model: bool, cx: &mut Self::Cx<'_>) -> bool {
        self.ai.conversations.select_acp_picker_config(agent, config, value, model, cx)
    }
    fn ai_context_token_total(&self, cx: &App) -> usize { self.ai_context_token_breakdown(cx).total }
    fn resolve_ai_acp_plugin(&self, agent: &mut AcpAgentConfig, cx: &App) -> Result<(), String> { WorkspaceApp::resolve_ai_acp_plugin(self, agent, cx) }
    fn show_acp_plugin_requirement(&self, agent: &AcpAgentConfig, cx: &mut Self::Cx<'_>) -> bool { WorkspaceApp::show_acp_plugin_requirement(self, agent, cx) }
    fn ai_picker_plugin_available(&self, id: &str, cx: &App) -> bool { WorkspaceApp::ai_picker_plugin_available(self, id, cx) }
    fn ensure_ai_provider_key_statuses(&mut self, cx: &mut Self::Cx<'_>) { WorkspaceApp::ensure_ai_provider_key_statuses(self, cx) }
    fn test_ai_acp_agent(&mut self, id: String, cx: &mut Self::Cx<'_>) { WorkspaceApp::test_ai_acp_agent(self, id, cx) }
    fn refresh_ai_provider_models(&mut self, index: usize, provider: AiProviderView, cx: &mut Self::Cx<'_>) { WorkspaceApp::refresh_ai_provider_models(self, index, provider, cx) }
    fn close_ai_model_selector(&mut self, cx: &mut App) { WorkspaceApp::close_ai_model_selector(self, cx) }
    fn check_ai_image_input(&mut self, cx: &mut Self::Cx<'_>) -> bool { WorkspaceApp::check_ai_image_input(self, cx) }
    fn resume_ai_image_request_for_model(&mut self, cx: &mut Self::Cx<'_>) { WorkspaceApp::resume_ai_image_request_for_model(self, cx) }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn sync_ai_workspace_visibility(&mut self, cx: &mut Context<Self>) {
        // Inline AI can be mounted in a detached terminal window, so its own
        // open state is the stable cross-window visibility signal.
        let terminal_inline_surface = self.ai.state.read(cx).terminal_inline_panel().open;
        let model_selector_surface = self.ai_sidebar_visible() || terminal_inline_surface;
        let settings_visible = self.tabs(cx).iter().any(|tab| {
            tab.kind == TabKind::Settings && self.tab_host.read(cx).surface_is_visible(tab.id)
        });
        let settings_surface = !self.app_lock.locked
            && settings_visible
            && self.settings_workspace.read(cx).route_snapshot().active_tab == SettingsTab::Ai;
        let visibility = AiWorkspaceVisibility {
            model_selector_surface: !self.app_lock.locked && model_selector_surface,
            settings_surface,
        };
        self.ai.state.update(cx, |ai, _| ai.update_picker_visibility(visibility));
        if visibility.model_selector_surface {
            self.ensure_ai_model_selector_mount_statuses(cx);
        }
    }

    pub(in crate::workspace) fn ai_picker_plugin_available(&self, id: &str, cx: &App) -> bool {
        self.plugin_entity.read(cx).acp_agents().iter().any(|plugin| plugin.plugin_id == id)
    }

    pub(in crate::workspace) fn select_ai_acp_config_from_selector(
        &mut self, agent_id: String, config_id: String, value_id: String, cx: &mut Context<Self>,
    ) {
        self.ai.conversations.select_acp_picker_config(&agent_id, config_id, value_id, false, cx);
    }

    pub(in crate::workspace) fn select_ai_acp_mode_from_selector(
        &mut self, agent_id: String, mode_id: String, cx: &mut Context<Self>,
    ) {
        self.ai.conversations.select_acp_picker_mode(&agent_id, mode_id, cx);
    }

}
