use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn refresh_ai_skill_registry(&mut self, cx: &mut App) {
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
        self.ai.skills.refresh(self.settings_store.path().to_path_buf(), plugin_roots, disabled_paths);
        self.ai.state.update(cx, |state, _| state.clear_loaded_skill_versions());
    }
}
