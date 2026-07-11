use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_general_settings_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(20.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(16.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.ai.general").to_uppercase()),
            )
            .child(self.ai_enabled_row(settings.ai.enabled, cx))
            .child(self.ai_privacy_notice());
        self.settings_card_surface(card, self.tokens.ui.bg_card)
            .into_any_element()
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

        let card = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(20.0))
            .flex()
            .flex_col()
            .child(body);
        self.settings_card_surface(card, self.tokens.ui.bg_card)
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_enabled_row(
        &self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .mb(px(24.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .whitespace_nowrap()
                            .child(self.i18n.t("settings_view.ai.enable")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.enable_hint")),
                    ),
            )
            .child(
                checkbox(&self.tokens, String::new(), enabled)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if !enabled && !this.settings_store.settings().ai.enabled_confirmed {
                                this.ai_settings_dialog_presence.reopen();
                                this.settings_page.set_ai_enable_confirm_open(true);
                                this.reset_standard_confirm_focus();
                                cx.notify();
                            } else {
                                this.edit_settings(
                                    |settings| set_ai_enabled(settings, !enabled),
                                    cx,
                                );
                            }
                        }),
                    )
                    .into_any_element(),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_privacy_notice(&self) -> AnyElement {
        div()
            .mb(px(24.0))
            .p(px(12.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            // Keep the nested notice as border-only chrome; the surrounding
            // settings card already owns the Tauri card elevation.
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .line_height(px(18.0))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("settings_view.ai.privacy_notice"),
                        self.i18n.t("settings_view.ai.privacy_text")
                    )),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_separator(&self) -> AnyElement {
        div()
            .my(px(24.0))
            .child(self.card_separator())
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
