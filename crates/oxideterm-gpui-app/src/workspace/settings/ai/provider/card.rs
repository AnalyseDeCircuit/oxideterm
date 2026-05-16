impl WorkspaceApp {
    fn ai_provider_card(
        &self,
        index: usize,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.expanded_ai_providers.contains(&provider.id);
        let active_provider = self
            .settings_store
            .settings()
            .ai
            .active_provider_id
            .as_deref()
            == Some(provider.id.as_str());
        let models_expanded = self.expanded_ai_provider_models.contains(&provider.id);
        let visible_models = if models_expanded {
            provider.models.clone()
        } else {
            provider
                .models
                .iter()
                .take(AI_PROVIDER_VISIBLE_MODEL_LIMIT)
                .cloned()
                .collect()
        };
        let hidden_count = provider.models.len().saturating_sub(visible_models.len());

        let mut card = div()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(if active_provider {
                rgba((self.tokens.ui.accent << 8) | 0x99)
            } else {
                rgba((self.tokens.ui.border << 8) | AI_PROVIDER_SECTION_BORDER_ALPHA)
            })
            .bg(if active_provider {
                rgba((self.tokens.ui.accent << 8) | 0x0d)
            } else {
                rgba((self.tokens.ui.bg << 8) | 0xb3)
            })
            .flex()
            .flex_col()
            .child(self.ai_provider_card_header(&provider, active_provider, expanded, cx));

        if expanded {
            card = card
                .child(self.ai_provider_expanded_toolbar(index, &provider, cx))
                .child(self.ai_provider_fields(index, &provider, cx))
                .child(self.ai_provider_models(index, &provider, visible_models, hidden_count, cx));

            if self
                .ai_provider_key_display_state(&provider)
                .shows_key_control()
            {
                card = card.child(self.ai_provider_key_input(index, &provider, cx));
            }
        }

        card.into_any_element()
    }

    fn ai_provider_card_header(
        &self,
        provider: &AiProviderView,
        active_provider: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        let provider_key_state = self.ai_provider_key_display_state(provider);
        let provider_has_key = provider_key_state.has_usable_key();
        div()
            .w_full()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .p(px(16.0))
            .cursor_pointer()
            .child(
                div()
                .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(provider.name.clone()),
                            )
                            .child(self.ai_provider_type_badge(provider.provider_type.clone()))
                            .when(active_provider, |row| {
                                row.child(self.ai_provider_badge(
                                    self.i18n.t("settings_view.ai.active"),
                                    self.tokens.ui.accent,
                                    0x33,
                                ))
                            })
                            .child(self.ai_provider_badge(
                                if provider.enabled {
                                    self.i18n.t("settings_view.ai.provider_enabled")
                                } else {
                                    self.i18n.t("settings_view.ai.provider_disabled")
                                },
                                if provider.enabled {
                                    self.tokens.ui.success
                                } else {
                                    self.tokens.ui.text_muted
                                },
                                if provider.enabled { 0x1a } else { 0x33 },
                            )),
                    )
                    .child(
                        div()
                            .mt(px(0.0))
                            .flex()
                            .flex_wrap()
                            .gap_x(px(16.0))
                            .gap_y(px(4.0))
                            .text_size(px(11.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(format!(
                                "{}: {}",
                                self.i18n.t("settings_view.ai.default_model"),
                                if provider.default_model.trim().is_empty() {
                                    "—".to_string()
                                } else {
                                    provider.default_model.clone()
                                }
                            ))
                            .child(self.i18n_count(
                                "settings_view.ai.provider_models_summary",
                                provider.models.len(),
                            ))
                            .when(provider_key_state.shows_key_control(), |row| {
                                row.child(format!(
                                    "{}: {}",
                                    self.i18n.t("settings_view.ai.api_key"),
                                    if provider_has_key {
                                        self.i18n.t("settings_view.ai.api_key_stored")
                                    } else {
                                        self.i18n.t("settings_view.ai.api_key_missing")
                                    }
                                ))
                            }),
                    )
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(!active_provider, |row| {
                        row.child(self.ai_provider_active_button(provider, active_provider, cx))
                    })
                    .child(Self::render_lucide_icon(
                        if expanded {
                            LucideIcon::ChevronDown
                        } else {
                            LucideIcon::ChevronRight
                        },
                        16.0,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    toggle_string_set(&mut this.expanded_ai_providers, &provider_id);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }


}
