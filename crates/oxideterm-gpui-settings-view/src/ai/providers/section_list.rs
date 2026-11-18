use super::*;

impl AiProviderSettingsView {
    pub(super) fn ai_provider_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.expanded;
        let summary = self.i18n_count(
            "settings_view.ai.provider_settings_summary",
            self.providers.len(),
        );
        self.sync_ai_provider_card_list_state(cx);
        let provider_list = if expanded {
            let state = self.provider_card_list_state.clone();
            let spec = self.ai_provider_card_list_spec();
            let workspace = cx.entity();
            let list_height = self.ai_provider_card_list_estimated_height(cx);
            Some(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .h(px(list_height))
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            workspace
                                .update(cx, |this, cx| this.ai_provider_card_list_item(index, cx))
                        },
                    ))
                    .into_any_element(),
            )
        } else {
            None
        };

        section(
            &self.tokens,
            self.ai_collapsible_header(
                "settings_view.ai.provider_settings",
                summary,
                expanded,
                |this, _event, _window, cx| {
                    this.expanded = !this.expanded;
                    cx.emit(ProviderLayoutChanged);
                    cx.stop_propagation();
                    cx.notify();
                },
                cx,
            ),
            std::iter::once(self.ai_provider_add_controls(cx)).chain(provider_list),
        )
    }

    pub(super) fn ai_provider_card_list_estimated_height(&self, cx: &App) -> f32 {
        self.providers
            .iter()
            .flatten()
            .map(|provider| self.ai_provider_card_estimated_height(provider, cx))
            .sum::<f32>()
    }

    pub(super) fn ai_provider_card_estimated_height(
        &self,
        provider: &AiProviderView,
        cx: &App,
    ) -> f32 {
        let active_provider = self.active_provider_id.as_deref() == Some(provider.id.as_str());
        let expanded = self.settings_provider_expanded(&provider.id, active_provider);
        if !expanded {
            return 72.0;
        }
        let models_expanded = self.settings_provider_models_expanded(&provider.id);
        let visible_model_count = if models_expanded {
            provider.models.len()
        } else {
            provider.models.len().min(AI_PROVIDER_VISIBLE_MODEL_LIMIT)
        };
        let chip_rows = visible_model_count
            .div_ceil(AI_PROVIDER_MODEL_CHIPS_PER_VIRTUAL_ROW)
            .max(1);
        let key_input_height = if self
            .ai_provider_key_display_state(provider, cx)
            .shows_key_control()
        {
            72.0
        } else {
            0.0
        };
        let protocol_height = if matches!(
            provider.provider_type.as_str(),
            "openai" | "openai_compatible" | "xai"
        ) {
            64.0
        } else {
            0.0
        };
        let remove_height = if provider.custom { 40.0 } else { 0.0 };
        // Account for independently wrapped fields and the optional protocol/key controls.
        72.0 + 52.0
            + 144.0
            + 34.0
            + chip_rows as f32 * AI_PROVIDER_MODEL_CHIP_ROW_ESTIMATED_HEIGHT
            + 48.0
            + key_input_height
            + protocol_height
            + remove_height
            + 16.0
    }

    pub(super) fn sync_ai_provider_card_list_state(&self, cx: &App) {
        let signatures = self
            .providers
            .iter()
            .flatten()
            .map(|provider| {
                let active_provider =
                    self.active_provider_id.as_deref() == Some(provider.id.as_str());
                let expanded = self.settings_provider_expanded(&provider.id, active_provider);
                let mut hasher = DefaultHasher::new();
                ai_provider_card_signature(
                    provider,
                    expanded,
                    self.settings_provider_models_expanded(&provider.id),
                    self.ai_provider_has_key_cached(&provider.id, cx),
                )
                .hash(&mut hasher);
                self.is_web_provider(provider).hash(&mut hasher);
                hasher.finish()
            })
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.provider_card_list_state,
            &mut self.provider_card_list_cache.borrow_mut(),
            "ai-provider-cards",
            &signatures,
            self.ai_provider_card_list_spec(),
        );
    }

    pub(super) fn ai_provider_card_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(AI_PROVIDER_CARD_LIST_ESTIMATED_HEIGHT),
            AI_PROVIDER_CARD_LIST_OVERSCAN,
        )
    }

    pub(super) fn ai_provider_card_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((provider_index, provider)) = self
            .providers
            .iter()
            .enumerate()
            .filter_map(|(index, provider)| Some((index, provider.as_ref()?)))
            .nth(index)
        else {
            return div().into_any_element();
        };
        div()
            .w_full()
            .min_w(px(0.0))
            .pb(px(12.0))
            .child(self.ai_provider_card(provider_index, provider, cx))
            .into_any_element()
    }

    pub(super) fn ai_collapsible_header(
        &self,
        title_key: &str,
        summary: String,
        expanded: bool,
        on_click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_collapsible_header(
            &self.tokens,
            self.i18n.t(title_key),
            summary,
            self.render_animated_chevron(
                (
                    gpui::SharedString::from(format!("ai-section-chevron-{title_key}")),
                    expanded as usize,
                ),
                expanded,
                16.0,
                rgb(self.tokens.ui.text_muted),
            ),
        )
        .on_mouse_down(MouseButton::Left, cx.listener(on_click))
        .into_any_element()
    }

    pub(super) fn add_ai_provider_from_selected_template(&mut self, cx: &mut Context<Self>) {
        let template = ai_provider_template_by_type(self.provider_type());
        let now_ms = current_time_millis();
        let id = generated_provider_id(template.provider_type, now_ms);
        let label = self.i18n.t(template.label_key);
        let id_for_edit = id.clone();
        self.edit_settings(
            move |settings| {
                ai_add_provider_from_template(
                    &mut settings.ai.providers,
                    &mut settings.ai.active_provider_id,
                    &mut settings.ai.active_model,
                    template,
                    id_for_edit,
                    label,
                    now_ms,
                );
            },
            cx,
        );
        self.expanded = true;
        self.expanded_providers.insert(id, true);
        cx.emit(ProviderLayoutChanged);
        cx.notify();
    }
}
