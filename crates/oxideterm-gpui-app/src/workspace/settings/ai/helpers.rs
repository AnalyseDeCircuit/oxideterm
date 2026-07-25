use super::*;

// Settings page model helpers live in oxideterm-settings-model. This file keeps
// thin app adapters that are shared by multiple settings subpages.

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_settings_select_control(
        &self,
        select_id: SettingsSelect,
        label: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_select_control(select_id, label, false, Some(width), cx)
    }
}
