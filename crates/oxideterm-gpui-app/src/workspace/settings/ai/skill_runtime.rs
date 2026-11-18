use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn refresh_ai_skill_registry(&mut self) {
        let plugin_registry =
            plugin_host::NativePluginRegistry::discover(self.settings_store.path());
        let plugin_roots = plugin_registry
            .plugins()
            .iter()
            .filter(|plugin| {
                matches!(
                    plugin.state,
                    plugin_host::NativePluginState::ReadyManifestOnly
                        | plugin_host::NativePluginState::ReadyWasm
                        | plugin_host::NativePluginState::ReadyProcess
                        | plugin_host::NativePluginState::Active
                )
            })
            .map(|plugin| plugin.install_dir.clone())
            .collect();
        let disabled_paths = self
            .settings_store
            .settings()
            .ai
            .skills
            .disabled_paths
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        let registry =
            oxideterm_skills::SkillRegistry::discover(&oxideterm_skills::SkillDiscoveryOptions {
                workspace_root: self.ai.skills.workspace_root.clone(),
                settings_path: Some(self.settings_store.path().to_path_buf()),
                plugin_roots,
                disabled_paths,
            });
        *self.ai.skills.registry.write() = registry;
        // A disabled or replaced skill must be loaded again before resources
        // can be read in any existing conversation.
        self.ai.skills.loaded_by_conversation.clear();
    }
}
