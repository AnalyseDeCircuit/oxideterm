impl WorkspaceApp {
    fn render_ai_model_selector_models(
        &self,
        provider: AiProviderView,
        visible_models: Vec<String>,
        has_key: bool,
        online: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut panel = ai_model_selector_models_panel(&self.tokens);
        if matches!(
            resolve_model_selector_provider_probe(&provider),
            ModelSelectorProviderProbe::ImplicitKey { .. }
        ) && !online
        {
            return panel
                .child(ai_model_selector_provider_message(
                    &self.tokens,
                    self.i18n.t("ai.model_selector.offline"),
                    AiModelSelectorProviderState::Offline,
                    false,
                ))
                .into_any_element();
        }
        if !has_key {
            return panel
                .child(
                    ai_model_selector_provider_message(
                        &self.tokens,
                        self.i18n.t("ai.model_selector.no_key_warning"),
                        AiModelSelectorProviderState::MissingKey,
                        true,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.close_ai_model_selector();
                            this.open_ai_settings(window, cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
                .into_any_element();
        }
        if visible_models.is_empty() {
            return panel
                .child(ai_model_selector_provider_message(
                    &self.tokens,
                    self.i18n.t("ai.model_selector.refresh_models"),
                    AiModelSelectorProviderState::Ready,
                    false,
                ))
                .into_any_element();
        }

        for model in visible_models {
            let active = self
                .settings_store
                .settings()
                .ai
                .active_provider_id
                .as_deref()
                == Some(provider.id.as_str())
                && self.settings_store.settings().ai.active_model.as_deref()
                    == Some(model.as_str());
            let model_for_click = model.clone();
            let provider_id = provider.id.clone();
            let highlighted =
                self.ai_model_selector_highlighted_model
                    .as_ref()
                    .is_some_and(|(id, highlighted_model)| {
                        id == &provider.id && highlighted_model == &model
                    });
            panel = panel.child(
                ai_model_selector_model_row(
                    &self.tokens,
                    model,
                    active,
                    highlighted,
                    active.then(|| {
                        Self::render_lucide_icon(
                            LucideIcon::Check,
                            12.0,
                            rgb(self.tokens.ui.accent),
                        )
                    }),
                )
                .on_mouse_move({
                    let provider_id = provider_id.clone();
                    let model_for_hover = model_for_click.clone();
                    cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                        let next = Some((provider_id.clone(), model_for_hover.clone()));
                        if this.ai_model_selector_highlighted_model != next {
                            // Pointer hover and keyboard navigation share the
                            // same active-item state, matching Radix menu focus.
                            this.ai_model_selector_highlighted_model = next;
                            cx.notify();
                        }
                    })
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.select_ai_model_from_selector(
                            provider_id.clone(),
                            model_for_click.clone(),
                            cx,
                        );
                        this.ai_model_selector_highlighted_model = None;
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        panel.into_any_element()
    }

}
