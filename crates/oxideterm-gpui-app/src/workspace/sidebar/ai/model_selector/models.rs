impl WorkspaceApp {
    pub(in crate::workspace) fn active_ai_reasoning_level(
        &self,
        provider: &AiProviderView,
        model: &str,
        cx: &App,
    ) -> AiReasoningLevel {
        if let Some(value) = self.ai_entity.read(cx).conversation_state()
            .active_conversation()
            .and_then(|conversation| {
                ai_conversation_reasoning_effort(conversation, &provider.id, model)
            })
        {
            return oxideterm_ai::normalize_reasoning_level_for_model(
                &provider.provider_type,
                model,
                value,
            );
        }
        let settings = self.settings_store.settings();
        let value = settings
            .ai
            .reasoning_model_overrides
            .get(&provider.id)
            .and_then(|models| models.get(model))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("auto");
        oxideterm_ai::normalize_reasoning_level_for_model(
            &provider.provider_type,
            model,
            value,
        )
    }

    pub(in crate::workspace) fn render_ai_reasoning_indicator(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let settings = self.settings_store.settings();
        if settings.ai.active_backend == AiActiveBackend::Acp {
            return None;
        }
        let providers = ai_provider_views(&settings.ai.providers);
        let provider =
            active_provider_view(&providers, settings.ai.active_provider_id.as_deref())?;
        let model = active_model_selection(settings.ai.active_model.as_deref())?;
        let capability = model_reasoning_capability(&provider.provider_type, &model);
        if capability.levels.is_empty() {
            return None;
        }
        let selected = self.active_ai_reasoning_level(provider, &model, cx);
        let open = self.ai.chat.reasoning_menu_open;
        let trigger = div()
            .flex()
            .flex_none()
            .items_center()
            .rounded(px(self.tokens.radii.md))
            .px(px(self.tokens.spacing.one))
            .py(px(self.tokens.spacing.one / 2.0))
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if open {
                rgb(self.tokens.ui.text)
            } else {
                rgb(self.tokens.ui.text_muted)
            })
            .bg(if open {
                rgba((self.tokens.ui.accent << 8) | 0x1a)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(rgba((self.tokens.ui.accent << 8) | 0x1a))
                    .text_color(rgb(self.tokens.ui.text))
            })
            // The compact value stays English because it is a protocol level.
            .child(selected.display_name())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    let next_open = !this.ai.chat.reasoning_menu_open;
                    this.close_ai_sidebar_popovers(cx);
                    this.ai.chat.reasoning_menu_open = next_open;
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        Some(
            select_anchor_probe(
                SelectAnchorId::AiReasoningMenu,
                trigger,
                Self::deferred_ai_select_anchor_update(cx.entity()),
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_ai_reasoning_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let settings = self.settings_store.settings();
        if settings.ai.active_backend == AiActiveBackend::Acp {
            return None;
        }
        let providers = ai_provider_views(&settings.ai.providers);
        let provider =
            active_provider_view(&providers, settings.ai.active_provider_id.as_deref())?;
        let model = active_model_selection(settings.ai.active_model.as_deref())?;
        let capability = model_reasoning_capability(&provider.provider_type, &model);
        if capability.levels.is_empty() {
            return None;
        }
        let selected = self.active_ai_reasoning_level(provider, &model, cx);
        let mut levels = vec![AiReasoningLevel::Auto];
        levels.extend(capability.levels);
        let mut menu = div()
            .w(px(AI_REASONING_MENU_WIDTH))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .shadow_lg()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .py(px(self.tokens.spacing.one))
            .child(
                div()
                    .px(px(self.tokens.spacing.three))
                    .py(px(self.tokens.spacing.one))
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ai.reasoning.title")),
            );
        for level in levels {
            let provider_id = provider.id.clone();
            let provider_type = provider.provider_type.clone();
            let model_for_click = model.clone();
            let label = self.ai_reasoning_level_display(level);
            let is_selected = selected == level;
            menu = menu.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .mx(px(self.tokens.spacing.one))
                    .rounded(px(self.tokens.radii.md))
                    .px(px(self.tokens.spacing.two))
                    .py(px(self.tokens.spacing.one + self.tokens.spacing.one / 2.0))
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.text))
                    .bg(if is_selected {
                        rgba((self.tokens.ui.accent << 8) | 0x1a)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                    .child(
                        div()
                            .w(px(14.0))
                            .flex_none()
                            .when(is_selected, |check| {
                                check.child(Self::render_lucide_icon(
                                    LucideIcon::Check,
                                    14.0,
                                    rgb(self.tokens.ui.accent),
                                ))
                            }),
                    )
                    .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.select_ai_reasoning_level(
                            provider_id.clone(),
                            provider_type.clone(),
                            model_for_click.clone(),
                            level,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        if !capability.known_model {
            menu = menu.child(
                div()
                    .mt(px(self.tokens.spacing.one))
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
                    .px(px(self.tokens.spacing.three))
                    .pt(px(self.tokens.spacing.two))
                    .pb(px(self.tokens.spacing.one))
                    .text_size(px(9.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ai.reasoning.custom_model_hint")),
            )
        }
        Some(menu.into_any_element())
    }

    fn ai_reasoning_level_display(&self, level: AiReasoningLevel) -> String {
        self.i18n
            .t(&format!("ai.reasoning.level_{}", level.as_str()))
    }

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
                            this.close_ai_model_selector(cx);
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
            if self.ai_acp_model_discovery_is_pending(&agent_id, cx) {
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
