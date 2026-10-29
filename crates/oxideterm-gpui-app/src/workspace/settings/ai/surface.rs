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

    pub(in crate::workspace) fn ai_disabled_settings_card(
        &self,
        body: AnyElement,
        enabled: bool,
    ) -> AnyElement {
        let mut body = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .opacity(if enabled { 1.0 } else { 0.5 })
            .child(body);
        if !enabled {
            // Disabled OxideSens subsections should look inert and must not let
            // nested controls fire while the top-level feature toggle remains
            // usable in the separate general card.
            body = body.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            });
        }

        self.plain_settings_card(vec![body.into_any_element()])
    }

    pub(in crate::workspace) fn ai_enabled_row(
        &self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            "settings_view.ai.enable",
            "settings_view.ai.enable_hint",
            checkbox(&self.tokens, "ai-enabled", String::new(), enabled)
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
        let card = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(
                        self.i18n
                            .t("settings_view.ai.privacy_notice")
                            .to_uppercase(),
                    ),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .line_height(px(18.0))
                    .child(self.i18n.t("settings_view.ai.privacy_text")),
            );
        self.settings_card_surface(card, self.tokens.ui.bg_card)
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_section_title(&self, key: &str) -> AnyElement {
        div()
            .mb(px(16.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(self.i18n.t(key).to_uppercase())
            .into_any_element()
    }

    pub(in crate::workspace) fn i18n_count(&self, key: &str, count: usize) -> String {
        self.i18n.t(key).replace("{{count}}", &count.to_string())
    }

    pub(in crate::workspace) fn ai_i18n_error(&self, key: &str, error: &str) -> String {
        self.i18n.t(key).replace("{{error}}", error)
    }
}
