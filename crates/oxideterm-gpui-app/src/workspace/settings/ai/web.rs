use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_web_search_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = ai_provider_views(settings).into_iter().find(|provider| {
            Some(provider.id.as_str()) == settings.ai.web_search.provider_id.as_deref()
                && oxideterm_ai::web::official_deepseek_provider(provider)
        });
        let label = selected
            .map(|provider| provider.name)
            .unwrap_or_else(|| self.i18n.t("settings_view.ai.web_provider_none"));
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(self.ai_section_title("settings_view.ai.web_title"))
            .child(self.bool_row(
                "settings_view.ai.web_enabled",
                "settings_view.ai.web_enabled_hint",
                settings.ai.web_search.enabled,
                |settings, enabled| settings.ai.web_search.enabled = enabled,
                cx,
            ))
            .child(self.setting_row(
                "settings_view.ai.web_provider",
                "settings_view.ai.web_provider_hint",
                self.ai_settings_select_control(
                    SettingsSelect::AiWebSearchProvider,
                    label,
                    AI_PROVIDER_SELECT_W,
                    cx,
                ),
                cx,
            ))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.web_billing_hint")),
            )
            .into_any_element()
    }
}
