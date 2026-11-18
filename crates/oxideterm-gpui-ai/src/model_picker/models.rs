impl AiModelPickerView {
    fn render_ai_model_selector_models(
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
        let mut panel = ai_model_selector_models_panel(&self.display().tokens);
        if matches!(
            resolve_model_selector_provider_probe(&provider),
            ModelSelectorProviderProbe::ImplicitKey { .. }
        ) && !online
        {
            return panel
                .child(ai_model_selector_provider_message(
                    &self.display().tokens,
                    self.display().i18n.t("ai.model_selector.offline"),
                    AiModelSelectorProviderState::Offline,
                    false,
                ))
                .into_any_element();
        }
        if !has_key {
            return panel
                .child(
                    ai_model_selector_provider_message(
                        &self.display().tokens,
                        self.display().i18n.t("ai.model_selector.no_key_warning"),
                        AiModelSelectorProviderState::MissingKey,
                        true,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            (this.bindings.request)(
                                AiModelPickerRequest::Settings { close: true },
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
                )
                .into_any_element();
        }
        if visible_models.is_empty() {
            return panel
                .child(ai_model_selector_provider_message(
                    &self.display().tokens,
                    self.display().i18n.t("ai.model_selector.refresh_models"),
                    AiModelSelectorProviderState::Ready,
                    false,
                ))
                .into_any_element();
        }

        for model in visible_models {
            let active = self.display().active_provider.as_deref() == Some(provider.id.as_str())
                && self.display().active_model.as_deref() == Some(model.as_str());
            let model_for_click = model.clone();
            let provider_id = provider.id.clone();
            let highlighted = self
                .state
                .read(cx)
                .model_selector_highlight()
                .as_ref()
                .is_some_and(|(id, highlighted_model)| {
                    id == &provider.id && highlighted_model == &model
                });
            panel = panel.child(
                ai_model_selector_model_row(
                    &self.display().tokens,
                    model,
                    active,
                    highlighted,
                    active.then(|| {
                        render_lucide_icon(
                            LucideIcon::Check,
                            12.0,
                            rgb(self.display().tokens.ui.accent),
                        )
                    }),
                )
                .on_mouse_move({
                    let provider_id = provider_id.clone();
                    let model_for_hover = model_for_click.clone();
                    cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                        let next = Some((provider_id.clone(), model_for_hover.clone()));
                        if this
                            .state
                            .update(cx, |ai, _cx| ai.set_model_selector_highlight(next))
                        {
                            // Pointer hover and keyboard navigation share the
                            // same active-item state, matching Radix menu focus.
                            cx.notify();
                        }
                    })
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        (this.bindings.request)(
                            AiModelPickerRequest::Select(
                                provider_id.clone(),
                                model_for_click.clone(),
                            ),
                            window,
                            cx,
                        );
                        this.state.update(cx, |ai, _cx| {
                            ai.set_model_selector_highlight(None);
                        });
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
        let mut panel = ai_model_selector_models_panel(&self.display().tokens);
        let session_state = self.active_ai_acp_session_state(&agent_id, cx);
        let config_options = self.ai_acp_model_options_for_agent(&agent_id, cx);
        let model_option = config_options
            .as_ref()
            .and_then(|options| oxideterm_ai::acp_model_config_option(options));
        let Some(option) = model_option.filter(|option| !option.choices.is_empty()) else {
            let provider_id = Self::ai_acp_provider_id(&agent_id);
            let label = self.ai_acp_agent_model_fallback_label(&agent_id, cx);
            let active = self.ai_active_model_selector_provider_id().as_deref()
                == Some(provider_id.as_str());
            let row = ai_model_selector_model_row(
                &self.display().tokens,
                label.clone(),
                active,
                false,
                active.then(|| {
                    render_lucide_icon(
                        LucideIcon::Check,
                        12.0,
                        rgb(self.display().tokens.ui.accent),
                    )
                }),
            );
            if self.ai_acp_model_discovery_is_pending(&agent_id, cx) {
                return self
                    .append_ai_acp_session_controls(
                        panel.child(row.opacity(0.7)),
                        &agent_id,
                        session_state.as_ref(),
                        config_options.as_deref(),
                        None,
                        cx,
                    )
                    .into_any_element();
            }
            panel = panel.child(row.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    (this.bindings.request)(
                        AiModelPickerRequest::Select(provider_id.clone(), label.clone()),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ));
            return self
                .append_ai_acp_session_controls(
                    panel,
                    &agent_id,
                    session_state.as_ref(),
                    config_options.as_deref(),
                    None,
                    cx,
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
                .state
                .read(cx)
                .model_selector_highlight()
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
                    &self.display().tokens,
                    choice_label.clone(),
                    active,
                    highlighted,
                    active.then(|| {
                        render_lucide_icon(
                            LucideIcon::Check,
                            12.0,
                            rgb(self.display().tokens.ui.accent),
                        )
                    }),
                )
                .on_mouse_move({
                    let choice_label = choice_label.clone();
                    cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                        let next = Some((provider_id.clone(), choice_label.clone()));
                        if this
                            .state
                            .update(cx, |ai, _cx| ai.set_model_selector_highlight(next))
                        {
                            cx.notify();
                        }
                    })
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        (this.bindings.request)(
                            AiModelPickerRequest::SelectAcpModel(
                                agent_id_for_click.clone(),
                                config_id.clone(),
                                choice_value_id.clone(),
                            ),
                            window,
                            cx,
                        );
                        this.state.update(cx, |ai, _cx| {
                            ai.set_model_selector_highlight(None);
                        });
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        self.append_ai_acp_session_controls(
            panel,
            &agent_id,
            session_state.as_ref(),
            config_options.as_deref(),
            Some(option.config_id.as_str()),
            cx,
        )
        .into_any_element()
    }

    fn append_ai_acp_session_controls(
        &self,
        mut panel: Div,
        agent_id: &str,
        session_state: Option<&AiAcpSessionState>,
        config_options: Option<&[oxideterm_ai::AcpSessionConfigOption]>,
        model_config_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Div {
        if let Some(session_state) = session_state
            && !session_state.available_modes.is_empty()
        {
            panel = panel.child(
                div()
                    .mt(px(self.display().tokens.spacing.one))
                    .px(px(self.display().tokens.spacing.two))
                    .py(px(self.display().tokens.spacing.one))
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.display().tokens.ui.text_muted))
                    .child(self.display().i18n.t("ai.model_selector.session_mode")),
            );
            for mode in &session_state.available_modes {
                let active =
                    session_state.current_mode_id.as_deref() == Some(mode.mode_id.as_str());
                let agent_id_for_click = agent_id.to_string();
                let mode_id = mode.mode_id.clone();
                panel = panel.child(
                    ai_model_selector_model_row(
                        &self.display().tokens,
                        mode.name.clone(),
                        active,
                        false,
                        active.then(|| {
                            render_lucide_icon(
                                LucideIcon::Check,
                                12.0,
                                rgb(self.display().tokens.ui.accent),
                            )
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            (this.bindings.request)(
                                AiModelPickerRequest::SelectAcpMode(
                                    agent_id_for_click.clone(),
                                    mode_id.clone(),
                                ),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
                );
            }
        }

        for config_option in config_options
            .unwrap_or_default()
            .iter()
            .filter(|candidate| Some(candidate.config_id.as_str()) != model_config_id)
            .filter(|candidate| !candidate.choices.is_empty())
        {
            panel = panel.child(
                div()
                    .mt(px(self.display().tokens.spacing.one))
                    .px(px(self.display().tokens.spacing.two))
                    .py(px(self.display().tokens.spacing.one))
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.display().tokens.ui.text_muted))
                    .child(config_option.name.clone()),
            );
            let selected_value_id = session_state
                .and_then(|state| {
                    state
                        .config_selections
                        .iter()
                        .find(|selection| selection.config_id == config_option.config_id)
                })
                .map(|selection| selection.value_id.as_str())
                .unwrap_or(config_option.current_value_id.as_str());
            for choice in &config_option.choices {
                let active = choice.value_id == selected_value_id;
                let agent_id_for_click = agent_id.to_string();
                let config_id = config_option.config_id.clone();
                let value_id = choice.value_id.clone();
                panel = panel.child(
                    ai_model_selector_model_row(
                        &self.display().tokens,
                        choice.label.clone(),
                        active,
                        false,
                        active.then(|| {
                            render_lucide_icon(
                                LucideIcon::Check,
                                12.0,
                                rgb(self.display().tokens.ui.accent),
                            )
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            (this.bindings.request)(
                                AiModelPickerRequest::SelectAcpConfig(
                                    agent_id_for_click.clone(),
                                    config_id.clone(),
                                    value_id.clone(),
                                ),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
                );
            }
        }
        panel
    }
}
