impl WorkspaceApp {
    pub(in crate::workspace) fn render_ai_model_selector_models(
        &self,
        provider: AiProviderView,
        visible_models: Vec<String>,
        has_key: bool,
        online: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(agent_id) = Self::ai_acp_agent_id_from_provider_id(&provider.id) {
            return self.render_ai_acp_model_selector_models(
                agent_id.to_string(),
                visible_models,
                cx,
            );
        }
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
            let highlighted = self
                .ai
                .models
                .selector_highlighted_model
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
                        if this.ai.models.selector_highlighted_model != next {
                            // Pointer hover and keyboard navigation share the
                            // same active-item state, matching Radix menu focus.
                            this.ai.models.selector_highlighted_model = next;
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
                        this.ai.models.selector_highlighted_model = None;
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        panel.into_any_element()
    }

    fn render_ai_acp_model_selector_models(
        &self,
        agent_id: String,
        visible_models: Vec<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut panel = ai_model_selector_models_panel(&self.tokens);
        let session_state = self.active_ai_acp_session_state(&agent_id);
        let config_options = self.ai_acp_model_options_for_agent(&agent_id);
        let model_option = config_options
            .as_ref()
            .and_then(|options| oxideterm_ai::acp_model_config_option(options));
        let Some(option) = model_option.filter(|option| !option.choices.is_empty()) else {
            let provider_id = Self::ai_acp_provider_id(&agent_id);
            let label = self.ai_acp_agent_model_fallback_label(&agent_id);
            let active = self.ai_active_model_selector_provider_id().as_deref()
                == Some(provider_id.as_str());
            let row = ai_model_selector_model_row(
                &self.tokens,
                label.clone(),
                active,
                false,
                active.then(|| {
                    Self::render_lucide_icon(
                        LucideIcon::Check,
                        12.0,
                        rgb(self.tokens.ui.accent),
                    )
                }),
            );
            if self.ai_acp_model_discovery_is_pending(&agent_id) {
                return panel.child(row.opacity(0.7)).into_any_element();
            }
            return panel
                .child(
                    row
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.select_ai_model_from_selector(
                                provider_id.clone(),
                                label.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
                )
                .into_any_element();
        };

        let selected_value_id = oxideterm_ai::acp_selected_config_choice(
            option,
            session_state
                .as_ref()
                .and_then(|state| state.model_selection.as_ref()),
        )
        .map(|choice| choice.value_id.as_str());
        for choice in option
            .choices
            .iter()
            .filter(|choice| visible_models.contains(&choice.label))
        {
            let active = Some(choice.value_id.as_str()) == selected_value_id;
            let highlighted = self
                .ai
                .models
                .selector_highlighted_model
                .as_ref()
                .is_some_and(|(id, model)| {
                    id == &Self::ai_acp_provider_id(&agent_id) && model == &choice.label
                });
            let provider_id = Self::ai_acp_provider_id(&agent_id);
            let choice_label = choice.label.clone();
            let choice_value_id = choice.value_id.clone();
            let config_id = option.config_id.clone();
            let agent_id_for_click = agent_id.clone();
            panel = panel.child(
                ai_model_selector_model_row(
                    &self.tokens,
                    choice_label.clone(),
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
                    let choice_label = choice_label.clone();
                    cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                        let next = Some((provider_id.clone(), choice_label.clone()));
                        if this.ai.models.selector_highlighted_model != next {
                            this.ai.models.selector_highlighted_model = next;
                            cx.notify();
                        }
                    })
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.select_ai_acp_model_from_selector(
                            agent_id_for_click.clone(),
                            config_id.clone(),
                            choice_value_id.clone(),
                            cx,
                        );
                        this.ai.models.selector_highlighted_model = None;
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        panel.into_any_element()
    }
}
