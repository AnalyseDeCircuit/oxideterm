impl WorkspaceApp {
    fn ai_provider_add_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = ai_provider_template_by_type(&self.settings_page.ai_new_provider_type);
        let provider_template_select = self.settings_select_control(
            SettingsSelect::AiProviderTemplate,
            self.i18n.t(selected.label_key),
            false,
            Some(AI_PROVIDER_SELECT_W),
            cx,
        );

        div()
            .w_full()
            .min_w(px(0.0))
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
                // text here, not a lucide icon. Use the workspace action
                // wrapper so provider creation follows the same guarded Button
                // path as refresh/remove/save actions.
                self.workspace_toolbar_action_button(
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
                    cx.listener(|this, _event, _window, cx| {
                        this.add_ai_provider_from_selected_template(cx);
                    }),
                )
            )
            .into_any_element()
    }


}
