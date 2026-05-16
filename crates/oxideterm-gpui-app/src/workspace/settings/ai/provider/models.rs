impl WorkspaceApp {
    fn ai_provider_models(
        &self,
        index: usize,
        provider: &AiProviderView,
        visible_models: Vec<String>,
        hidden_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        let models_expanded = self.expanded_ai_provider_models.contains(&provider.id);
        let mut body = div().flex().flex_col().gap(px(6.0)).child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(16.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(format!(
                            "{} ({})",
                            self.i18n.t("settings_view.ai.available_models"),
                            provider.models.len()
                        )),
                )
                .when(provider.models.len() > AI_PROVIDER_VISIBLE_MODEL_LIMIT, |row| {
                    row.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(self.tokens.ui.accent))
                            .cursor_pointer()
                            .child(if models_expanded {
                                self.i18n.t("settings_view.ai.show_fewer_models")
                            } else {
                                self.i18n_count(
                                    "settings_view.ai.show_all_models",
                                    provider.models.len(),
                                )
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    toggle_string_set(
                                        &mut this.expanded_ai_provider_models,
                                        &provider_id,
                                    );
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    )
                }),
        );

        if !visible_models.is_empty() {
            let mut chips = div()
                .px(px(16.0))
                .pb(px(16.0))
                .flex()
                .flex_wrap()
                .gap(px(4.0));
            for model in visible_models {
                chips = chips.child(self.ai_provider_model_chip(
                    index,
                    model.clone(),
                    provider.default_model == model,
                    cx,
                ));
            }
            if hidden_count > 0 {
                chips = chips.child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .text_size(px(10.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(format!("+{hidden_count}")),
                );
            }
            body = body.child(chips);
        }

        body.into_any_element()
    }

fn ai_provider_model_chip(
        &self,
        index: usize,
        model: String,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model_for_edit = model.clone();
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(if active {
                rgba((self.tokens.ui.accent << 8) | AI_PROVIDER_MODEL_ACTIVE_BORDER_ALPHA)
            } else {
                rgba((self.tokens.ui.border << 8) | AI_PROVIDER_MODEL_BORDER_ALPHA)
            })
            .bg(if active {
                rgba((self.tokens.ui.accent << 8) | AI_PROVIDER_MODEL_ACTIVE_BG_ALPHA)
            } else {
                rgb(self.tokens.ui.bg)
            })
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(10.0))
            .text_color(rgb(if active {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .cursor_pointer()
            .hover(|style| {
                style
                    .text_color(rgb(self.tokens.ui.text))
                    .border_color(rgb(self.tokens.ui.border))
            })
            .child(model)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(
                        |settings| {
                            ai_set_provider_default_model(
                                &mut settings.ai.providers,
                                index,
                                model_for_edit.clone(),
                            );
                        },
                        cx,
                    );
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_provider_has_key(&self, provider_id: &str) -> bool {
        self.ai_provider_key_status
            .get(provider_id)
            .copied()
            .unwrap_or_else(|| self.ai_key_store.has_provider_key(provider_id))
    }



}
