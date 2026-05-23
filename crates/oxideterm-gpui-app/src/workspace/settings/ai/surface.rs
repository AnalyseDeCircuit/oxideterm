impl WorkspaceApp {
    fn ai_settings_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings_store.settings();
        let mut disabled_body = div()
            .flex()
            .flex_col()
            .opacity(if settings.ai.enabled { 1.0 } else { 0.5 })
            .child(self.ai_execution_profiles_section(settings, cx))
            .child(self.ai_separator())
            .child(self.ai_provider_settings_section(cx))
            .child(self.ai_separator())
            .child(self.ai_context_controls_section(settings, cx))
            .child(self.ai_separator())
            .child(self.ai_system_prompt_section(settings, cx))
            .child(self.ai_separator())
            .child(self.ai_tool_use_section(settings, cx));

        if !settings.ai.enabled {
            disabled_body = disabled_body.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
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
            .child(
                div()
                    .mb(px(16.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.ai.general").to_uppercase()),
            )
            .child(self.ai_enabled_row(settings.ai.enabled, cx))
            .child(self.ai_privacy_notice())
            .child(self.ai_separator())
            .child(disabled_body);
        self.settings_card_surface(card, self.tokens.ui.bg_card)
            .into_any_element()
    }

    fn ai_enabled_row(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
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
                                this.show_ai_enable_confirm = true;
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

    fn ai_privacy_notice(&self) -> AnyElement {
        div()
            .mb(px(24.0))
            .p(px(12.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .shadow(oxideterm_gpui_ui::tauri_card_shadow(
                self.tokens.ui.bg_card,
            ))
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

    fn ai_separator(&self) -> AnyElement {
        div()
            .my(px(24.0))
            .child(self.card_separator())
            .into_any_element()
    }

    fn ai_section_title(&self, key: &str) -> AnyElement {
        div()
            .mb(px(16.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(self.i18n.t(key).to_uppercase())
            .into_any_element()
    }

    fn i18n_count(&self, key: &str, count: usize) -> String {
        self.i18n.t(key).replace("{{count}}", &count.to_string())
    }

    fn ai_i18n_error(&self, key: &str, error: &str) -> String {
        self.i18n.t(key).replace("{{error}}", error)
    }


}
