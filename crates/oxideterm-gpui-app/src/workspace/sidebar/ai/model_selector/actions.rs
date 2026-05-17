impl WorkspaceApp {
    fn toggle_ai_model_selector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next_open = !self.ai_model_selector_open;
        self.close_ai_sidebar_popovers();
        self.ai_model_selector_open = next_open;
        if self.ai_model_selector_open {
            let providers = ai_provider_views(&self.settings_store.settings().ai.providers);
            if let Some(provider) = active_provider_view(
                &providers,
                self.settings_store
                    .settings()
                    .ai
                    .active_provider_id
                    .as_deref(),
            ) {
                self.ai_model_selector_expanded_providers
                    .insert(provider.id.clone());
            }
            self.ai_model_selector_search_focused = true;
            self.refresh_ai_model_selector_provider_statuses(cx);
            window.focus(&self.focus_handle);
        } else {
            self.ai_model_selector_search_focused = false;
            self.ai_model_selector_search_query.clear();
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    fn refresh_ai_model_selector_provider_statuses(&mut self, cx: &mut Context<Self>) {
        self.ensure_ai_provider_key_statuses(cx);
        let providers = ai_provider_views(&self.settings_store.settings().ai.providers);
        for provider in providers {
            match resolve_model_selector_provider_probe(&provider) {
                ModelSelectorProviderProbe::Disabled => {
                    self.ai_provider_key_status.insert(provider.id.clone(), false);
                    self.ai_model_selector_provider_online
                        .insert(provider.id.clone(), false);
                }
                ModelSelectorProviderProbe::StoredKey => {
                    let has_key = self.ai_provider_has_key(&provider.id);
                    self.ai_provider_key_status.insert(provider.id.clone(), has_key);
                    self.ai_model_selector_provider_online
                        .insert(provider.id.clone(), true);
                }
                ModelSelectorProviderProbe::ImplicitKey { endpoint } => {
                    self.ai_provider_key_status.insert(provider.id.clone(), true);
                    if let Some(endpoint) = endpoint {
                        self.schedule_ai_model_selector_online_probe(provider.clone(), endpoint, cx);
                    } else {
                        self.ai_model_selector_provider_online
                            .insert(provider.id.clone(), true);
                    }
                }
            }
        }
    }

    fn schedule_ai_model_selector_online_probe(
        &mut self,
        provider: AiProviderView,
        endpoint: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.next_ai_model_selector_probe_generation =
            self.next_ai_model_selector_probe_generation.saturating_add(1);
        let generation = self.next_ai_model_selector_probe_generation;
        let provider_id = provider.id.clone();
        self.ai_model_selector_probe_generations
            .insert(provider_id.clone(), generation);
        if self.ai_model_selector_probe_tx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            self.ai_model_selector_probe_tx = Some(tx);
            self.ai_model_selector_probe_rx = Some(rx);
        }
        let Some(ui_tx) = self.ai_model_selector_probe_tx.as_ref().cloned() else {
            return;
        };
        self.ai_model_selector_probe_pending =
            self.ai_model_selector_probe_pending.saturating_add(1);
        self.forwarding_runtime.spawn(async move {
            let online = check_model_selector_provider_online(&provider.base_url, endpoint).await;
            let _ = ui_tx.send(AiModelSelectorProbeDelivery {
                provider_id,
                generation,
                online,
            });
        });
        self.schedule_ai_model_selector_probe_poll(cx);
    }

    pub(super) fn poll_ai_model_selector_probe_results(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.ai_model_selector_probe_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        loop {
            match rx.try_recv() {
                Ok(delivery) => {
                    self.ai_model_selector_probe_pending =
                        self.ai_model_selector_probe_pending.saturating_sub(1);
                    if self
                        .ai_model_selector_probe_generations
                        .get(&delivery.provider_id)
                        == Some(&delivery.generation)
                    {
                        self.ai_model_selector_provider_online
                            .insert(delivery.provider_id, delivery.online);
                        cx.notify();
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    keep_rx = false;
                    self.ai_model_selector_probe_tx = None;
                    self.ai_model_selector_probe_pending = 0;
                    break;
                }
            }
        }
        if keep_rx && self.ai_model_selector_probe_pending > 0 {
            self.ai_model_selector_probe_rx = Some(rx);
        } else if self.ai_model_selector_probe_pending == 0 {
            self.ai_model_selector_probe_tx = None;
        }
    }

    fn schedule_ai_model_selector_probe_poll(&mut self, cx: &mut Context<Self>) {
        if self.ai_model_selector_probe_polling {
            return;
        }
        self.ai_model_selector_probe_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(50)).await;
            let _ = weak.update(cx, |this, cx| {
                this.ai_model_selector_probe_polling = false;
                this.poll_ai_model_selector_probe_results(cx);
                if this.ai_model_selector_probe_pending > 0 {
                    this.schedule_ai_model_selector_probe_poll(cx);
                }
            });
        })
        .detach();
    }

    fn ai_model_selector_has_key(&self, provider: &AiProviderView) -> bool {
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::Disabled => false,
            ModelSelectorProviderProbe::ImplicitKey { .. } => true,
            ModelSelectorProviderProbe::StoredKey => self.ai_provider_has_key(&provider.id),
        }
    }

    fn ai_model_selector_provider_is_online(&self, provider: &AiProviderView) -> bool {
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::Disabled => false,
            ModelSelectorProviderProbe::StoredKey => true,
            ModelSelectorProviderProbe::ImplicitKey { .. } => self
                .ai_model_selector_provider_online
                .get(&provider.id)
                .copied()
                .unwrap_or(true),
        }
    }

    fn refresh_ai_provider_from_selector(
        &mut self,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) {
        if !self.ai_model_selector_has_key(&provider) {
            self.push_ai_settings_toast(
                self.i18n.t("ai.model_selector.no_key_warning"),
                TerminalNoticeVariant::Warning,
            );
            cx.notify();
            return;
        }
        if !self.ai_model_selector_provider_is_online(&provider) {
            self.push_ai_settings_toast(
                self.i18n.t("ai.model_selector.offline"),
                TerminalNoticeVariant::Warning,
            );
            cx.notify();
            return;
        }
        let Some(index) = ai_provider_views(&self.settings_store.settings().ai.providers)
            .iter()
            .position(|candidate| candidate.id == provider.id)
        else {
            return;
        };
        self.refresh_ai_provider_models(index, provider, cx);
    }

    fn select_ai_model_from_selector(
        &mut self,
        provider_id: String,
        model: String,
        cx: &mut Context<Self>,
    ) {
        let previous_model = self.settings_store.settings().ai.active_model.clone();
        self.edit_settings(
            |settings| {
                ai_select_provider_model(
                    &mut settings.ai.providers,
                    &mut settings.ai.active_provider_id,
                    &mut settings.ai.active_model,
                    &provider_id,
                    model.clone(),
                );
            },
            cx,
        );
        if previous_model.as_deref() != Some(model.as_str()) {
            self.update_ai_model_switch_warning(&provider_id, &model);
        }
        self.ai_model_selector_open = false;
        self.ai_model_selector_search_focused = false;
        self.ai_model_selector_search_query.clear();
        cx.notify();
    }

    fn update_ai_model_switch_warning(&mut self, provider_id: &str, model: &str) {
        let Some(conversation) = self.ai_chat.active_conversation() else {
            return;
        };
        let total_tokens = ai_conversation_message_tokens(conversation);
        if total_tokens == 0 {
            return;
        }
        let settings = self.settings_store.settings();
        let max_tokens = ai_context_window_from_maps(
            &settings.ai.user_context_windows,
            &settings.ai.model_context_windows,
            provider_id,
            model,
        )
        .unwrap_or(AI_COMPACTION_DEFAULT_CONTEXT_WINDOW);
        let percentage = ai_context_percentage(total_tokens, max_tokens);
        if percentage > AI_CONTEXT_WARNING_PERCENT {
            self.ai_model_switch_warning_percentage = Some(percentage.round() as usize);
        }
    }



}

pub(super) struct AiModelSelectorProbeDelivery {
    pub(super) provider_id: String,
    pub(super) generation: u64,
    pub(super) online: bool,
}
