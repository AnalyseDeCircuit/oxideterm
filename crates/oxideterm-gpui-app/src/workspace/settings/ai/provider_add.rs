use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn add_ai_provider_from_selected_template(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let template =
            ai_provider_template_by_type(self.ai_entity.read(cx).settings_new_provider_type());
        let now_ms = current_time_millis();
        let id = generated_provider_id(template.provider_type, now_ms);
        let label = self.i18n.t(template.label_key);
        self.edit_settings(
            |settings| {
                ai_add_provider_from_template(
                    &mut settings.ai.providers,
                    &mut settings.ai.active_provider_id,
                    &mut settings.ai.active_model,
                    template,
                    id.clone(),
                    label,
                    now_ms,
                );
            },
            cx,
        );
        let active = self
            .settings_store
            .settings()
            .ai
            .active_provider_id
            .as_deref()
            == Some(id.as_str());
        self.ai_entity.update(cx, |ai, cx| {
            if !ai.settings_section_expanded(AiSettingsViewSection::ProviderSettings) {
                ai.toggle_settings_section(AiSettingsViewSection::ProviderSettings, cx);
            }
            if !ai.settings_provider_expanded(&id, active) {
                ai.toggle_settings_provider_expanded(&id, active, cx);
            }
        });
    }
}
