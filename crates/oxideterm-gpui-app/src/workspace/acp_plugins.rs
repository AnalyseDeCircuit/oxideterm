use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn sync_acp_plugins(&mut self, cx: &mut Context<Self>) {
        use oxideterm_gpui_ai::acp::{AcpPluginRevision, missing_acp_plugins, reset_acp_plugin_agent_status};
        let revisions = self.plugin_entity.read(cx).acp_agents().into_iter().map(|agent| AcpPluginRevision {
            plugin_id: agent.plugin_id, name: agent.name, version: agent.version, command: agent.command,
        }).collect::<Vec<_>>();
        let missing = missing_acp_plugins(&revisions, &self.settings_store.settings().ai);
        if !missing.is_empty() {
            self.edit_settings(|settings| {
                for (id, name) in missing {
                    oxideterm_settings_model::ai_add_acp_plugin_agent(settings, &id, &name);
                }
            }, cx);
        }
        let changed = self.ai.conversations.sync_acp_plugins(revisions, &self.settings_store.settings().ai, cx);
        for id in changed {
            self.edit_settings(|settings| reset_acp_plugin_agent_status(&mut settings.ai, &id), cx);
        }
    }

    pub(in crate::workspace) fn stop_acp_plugin(&mut self, plugin_id: Option<&str>, cx: &mut Context<Self>) {
        self.ai.conversations.stop_acp_plugin(plugin_id, &self.settings_store.settings().ai, cx);
    }

    pub(in crate::workspace) fn resolve_ai_acp_plugin(
        &self,
        agent: &mut oxideterm_settings::AcpAgentConfig,
        cx: &App,
    ) -> Result<(), String> {
        let Some(plugin_id) = agent.plugin_id.as_deref() else {
            return Ok(());
        };
        let plugins = self.plugin_entity.read(cx);
        if self.ai.acp.read(cx).plugin_is_updating(plugin_id) {
            return Err(self.plugin_requirement_message(plugin_id, &agent.display_name, cx));
        }
        let plugin = plugins
            .acp_agents()
            .into_iter()
            .find(|plugin| plugin.plugin_id == plugin_id)
            .ok_or_else(|| self.plugin_requirement_message(plugin_id, &agent.display_name, cx))?;
        agent.command = plugin.command.to_string_lossy().into_owned();
        Ok(())
    }

    pub(in crate::workspace) fn show_acp_plugin_requirement(
        &self,
        agent: &oxideterm_settings::AcpAgentConfig,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(plugin_id) = agent.plugin_id.as_deref() else {
            return false;
        };
        if !self.ai.acp.read(cx).plugin_is_updating(plugin_id)
            && self
                .plugin_entity
                .read(cx)
                .acp_agents()
                .iter()
                .any(|plugin| plugin.plugin_id == plugin_id)
        {
            return false;
        }
        self.show_required_plugin_notice(plugin_id, &agent.display_name, cx);
        true
    }
}
