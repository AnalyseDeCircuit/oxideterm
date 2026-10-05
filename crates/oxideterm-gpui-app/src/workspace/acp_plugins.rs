use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn sync_acp_plugins(&mut self, cx: &mut Context<Self>) {
        let agents = self.plugin_entity.read(cx).acp_agents();
        let missing = agents
            .iter()
            .filter(|plugin| {
                !self
                    .settings_store
                    .settings()
                    .ai
                    .acp_agents
                    .iter()
                    .any(|agent| agent.plugin_id.as_deref() == Some(&plugin.plugin_id))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            self.edit_settings(
                |settings| {
                    for plugin in missing {
                        oxideterm_settings_model::ai_add_acp_plugin_agent(
                            settings,
                            &plugin.plugin_id,
                            &plugin.name,
                        );
                    }
                },
                cx,
            );
        }
        let bindings = self
            .settings_store
            .settings()
            .ai
            .acp_agents
            .iter()
            .filter_map(|agent| {
                agent
                    .plugin_id
                    .as_ref()
                    .map(|plugin| (agent.id.clone(), plugin.clone()))
            })
            .collect::<Vec<_>>();
        let changed = self
            .acp_entity
            .update(cx, |entity, cx| entity.sync_plugins(agents, &bindings, cx));
        for agent_id in changed {
            self.ai_entity
                .update(cx, |ai, _cx| ai.invalidate_acp_agent_metadata(&agent_id));
            self.edit_settings(
                |settings| {
                    if let Some(agent) = settings
                        .ai
                        .acp_agents
                        .iter_mut()
                        .find(|agent| agent.id == agent_id)
                    {
                        agent.status = Default::default();
                    }
                },
                cx,
            );
        }
    }

    pub(in crate::workspace) fn stop_acp_plugin(
        &mut self,
        plugin_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let agent_ids = self
            .settings_store
            .settings()
            .ai
            .acp_agents
            .iter()
            .filter(|agent| {
                agent
                    .plugin_id
                    .as_deref()
                    .is_some_and(|id| plugin_id.is_none_or(|plugin| plugin == id))
            })
            .map(|agent| agent.id.clone())
            .collect::<Vec<_>>();
        self.acp_entity
            .update(cx, |entity, cx| entity.stop_agents(&agent_ids, cx));
        for id in agent_ids {
            self.ai_entity
                .update(cx, |ai, _cx| ai.invalidate_acp_agent_metadata(&id));
        }
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
        if self.acp_entity.read(cx).plugin_is_updating(plugin_id) {
            return Err(self.i18n.t("settings_view.ai.acp_agent_plugin_unavailable"));
        }
        let plugin = plugins
            .acp_agents()
            .into_iter()
            .find(|plugin| plugin.plugin_id == plugin_id)
            .ok_or_else(|| self.i18n.t("settings_view.ai.acp_agent_plugin_unavailable"))?;
        agent.command = plugin.command.to_string_lossy().into_owned();
        Ok(())
    }
}
