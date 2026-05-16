impl WorkspaceApp {
    pub(in crate::workspace) fn render_ai_enable_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        dialog_backdrop()
            .child(
                dialog_content(&self.tokens)
                    .w(px(AI_CONFIRM_DIALOG_WIDTH))
                    .max_w(relative(0.92))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .child(
                        dialog_header(&self.tokens)
                            .child(dialog_title(
                                &self.tokens,
                                self.i18n.t("settings_view.ai_confirm.title"),
                            ))
                            .child(dialog_description(
                                &self.tokens,
                                self.i18n.t("settings_view.ai_confirm.description"),
                            )),
                    )
                    .child(
                        div()
                            .p(px(16.0))
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t("settings_view.ai_confirm.intro")),
                            )
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.sm))
                                    .border_1()
                                    .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                                    .bg(rgba((self.tokens.ui.bg_panel << 8) | 0x4d))
                                    .p(px(12.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(self.ai_confirm_bullet(
                                        "settings_view.ai_confirm.point_local",
                                    ))
                                    .child(self.ai_confirm_bullet(
                                        "settings_view.ai_confirm.point_no_server",
                                    ))
                                    .child(self.ai_confirm_bullet(
                                        "settings_view.ai_confirm.point_context",
                                    )),
                            ),
                    )
                    .child(
                        dialog_footer(&self.tokens)
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("settings_view.ai_confirm.cancel"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.show_ai_enable_confirm = false;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("settings_view.ai_confirm.enable"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.edit_settings(
                                            |settings| {
                                                settings.ai.enabled = true;
                                                settings.ai.enabled_confirmed = true;
                                            },
                                            cx,
                                        );
                                        this.show_ai_enable_confirm = false;
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn ai_confirm_bullet(&self, label_key: &str) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .child(
                div()
                    .mt(px(6.0))
                    .size(px(AI_CONFIRM_BULLET_SIZE))
                    .rounded(px(AI_CONFIRM_BULLET_SIZE / 2.0))
                    .bg(rgb(self.tokens.ui.text_muted)),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_provider_key_remove_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        dialog_backdrop()
            .child(
                dialog_content(&self.tokens)
                    .w(px(AI_KEY_REMOVE_DIALOG_WIDTH))
                    .max_w(relative(0.92))
                    .shadow_lg()
                    .rounded(px(self.tokens.radii.lg))
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x99))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(24.0))
                            .pt(px(24.0))
                            .pb(px(16.0))
                            .child(
                                div()
                                    .size(px(AI_CONFIRM_ICON_WRAP))
                                    .rounded(px(AI_CONFIRM_ICON_WRAP / 2.0))
                                    .border_1()
                                    .border_color(rgba((self.tokens.ui.error << 8) | 0x33))
                                    .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::AlertTriangle,
                                        AI_CONFIRM_ICON,
                                        rgb(self.tokens.ui.error),
                                    )),
                            )
                            .child(
                                div()
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .line_height(px(20.0))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t("settings_view.ai.remove_confirm")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .border_t_1()
                            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                            .child(
                                div()
                                    .flex_1()
                                    .py(px(10.0))
                                    .border_r_1()
                                    .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style
                                            .bg(rgba((self.tokens.ui.bg_hover << 8) | 0x80))
                                            .text_color(rgb(self.tokens.ui.text))
                                    })
                                    .child(self.i18n.t("common.actions.cancel"))
                                    .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.ai_provider_key_remove_confirm = None;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .py(px(10.0))
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.error))
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style
                                            .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                                            .text_color(rgb(self.tokens.ui.error))
                                    })
                                    .child(self.i18n.t("settings_view.ai.remove"))
                                    .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        if let Some((index, provider_id)) =
                                            this.ai_provider_key_remove_confirm.take()
                                        {
                                            this.remove_ai_provider_api_key(
                                                index,
                                                &provider_id,
                                                cx,
                                            );
                                        }
                                        cx.stop_propagation();
                                    }),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

}
