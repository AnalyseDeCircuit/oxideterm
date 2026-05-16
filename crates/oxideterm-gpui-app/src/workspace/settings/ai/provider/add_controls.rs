impl WorkspaceApp {
    fn ai_provider_add_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = ai_provider_template_by_type(&self.ai_new_provider_type);
        let anchor_id = SettingsSelect::AiProviderTemplate.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, self.i18n.t(selected.label_key), false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select =
                        if this.open_settings_select == Some(SettingsSelect::AiProviderTemplate) {
                            None
                        } else {
                            Some(SettingsSelect::AiProviderTemplate)
                        };
                    cx.stop_propagation();
                    cx.notify();
                }),
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
                    .child(
                        div()
                            .relative()
                            .w(px(AI_PROVIDER_SELECT_W))
                            .child(select_anchor_probe(
                                anchor_id,
                                trigger,
                                move |anchor, _window, cx| {
                                    let _ = workspace.update(cx, |this, cx| {
                                        this.update_select_anchor(anchor, cx);
                                    });
                                },
                            )),
                    ),
            )
            .child(
                button_with(
                    &self.tokens,
                    format!("+ {}", self.i18n.t("settings_view.ai.add_provider")),
                    ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
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
