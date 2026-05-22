impl WorkspaceApp {
    fn ai_provider_add_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = ai_provider_template_by_type(&self.ai_new_provider_type);
        let provider_template_select = self.settings_select_control(
            SettingsSelect::AiProviderTemplate,
            self.i18n.t(selected.label_key),
            false,
            Some(AI_PROVIDER_SELECT_W),
            cx,
        );

        div()
            .w_full()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_wrap()
            .items_end()
            .gap(px(12.0))
            .child(
                div()
                    .grid()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.provider_template")),
                    )
                    .child(provider_template_select),
            )
            .child(
                // Tauri uses an outline small Button with literal "+ label"
                // text here, not a lucide icon. Route it through toolbar_button
                // so all compact settings actions share one Button primitive.
                toolbar_button(
                    &self.tokens,
                    format!("+ {}", self.i18n.t("settings_view.ai.add_provider")),
                    None,
                    ToolbarButtonOptions {
                        button: ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: false,
                        },
                        ..ToolbarButtonOptions::default()
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.add_ai_provider_from_selected_template(cx);
                    }),
                ),
            )
            .into_any_element()
    }


}
