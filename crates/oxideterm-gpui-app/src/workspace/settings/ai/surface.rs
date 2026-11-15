use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_general_settings_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_card(
            "settings_view.ai.general",
            "",
            vec![self.ai_enabled_row(settings.ai.enabled, cx)],
        )
    }

    pub(in crate::workspace) fn ai_settings_section(
        &self,
        header: AnyElement,
        content: impl IntoIterator<Item = AnyElement>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .child(div().w_full().min_w_0().px(px(2.0)).child(header))
            .children(content)
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_settings_availability(
        &self,
        section: AnyElement,
        enabled: bool,
    ) -> AnyElement {
        if enabled {
            return section;
        }

        div()
            .w_full()
            .min_w_0()
            .relative()
            .child(div().w_full().min_w_0().opacity(0.5).child(section))
            // Section actions can live above the card, so the shield covers both.
            .child(div().absolute().inset_0().block_mouse_except_scroll())
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_enabled_row(
        &self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            "settings_view.ai.enable",
            "settings_view.ai.enable_hint",
            switch(&self.tokens, "ai-enabled", String::new(), enabled)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if !enabled && !this.settings_store.settings().ai.enabled_confirmed {
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.open_ai_enable_confirm(cx);
                            });
                            this.reset_standard_confirm_focus();
                            cx.notify();
                        } else {
                            this.edit_settings(|settings| set_ai_enabled(settings, !enabled), cx);
                        }
                    }),
                )
                .into_any_element(),
            cx,
        )
    }

    pub(in crate::workspace) fn ai_privacy_settings_card(&self) -> AnyElement {
        // Privacy guidance is a peer settings section rather than auxiliary
        // chrome nested inside the feature-toggle card.
        self.settings_card(
            "settings_view.ai.privacy_notice",
            "",
            vec![div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .line_height(px(18.0))
                .child(self.i18n.t("settings_view.ai.privacy_text"))
                .into_any_element()],
        )
    }

    pub(in crate::workspace) fn i18n_count(&self, key: &str, count: usize) -> String {
        self.i18n.t(key).replace("{{count}}", &count.to_string())
    }

    pub(in crate::workspace) fn ai_i18n_error(&self, key: &str, error: &str) -> String {
        self.i18n.t(key).replace("{{error}}", error)
    }
}
