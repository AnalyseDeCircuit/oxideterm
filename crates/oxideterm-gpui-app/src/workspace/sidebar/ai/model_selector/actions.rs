impl WorkspaceApp {
    pub(super) fn ensure_ai_model_selector_mount_statuses(&mut self, cx: &mut Context<Self>) {
        let providers = ai_provider_views(&self.settings_store.settings().ai.providers);
        let signature = ai_model_selector_status_signature(&providers);
        if self.ai_model_selector_status_signature == signature {
            return;
        }
        self.ai_model_selector_status_signature = signature;
        // Mirrors Tauri ModelSelector's mount/provider-change checkAllKeys
        // effect: the trigger indicator starts probing before the user opens it.
        self.refresh_ai_model_selector_provider_statuses(cx);
    }

    fn toggle_ai_model_selector(
        &mut self,
        scope: AiModelSelectorScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next_open = !(self.ai_model_selector_open
            && self.ai_model_selector_scope == Some(scope));
        self.close_ai_sidebar_popovers();
        self.ai_model_selector_open = next_open;
        self.ai_model_selector_scope = next_open.then_some(scope);
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
            self.ai_model_selector_highlighted_model = None;
            self.ai_chat_input_focused = false;
            self.ai_inline_panel.prompt_focused = false;
            self.refresh_ai_model_selector_provider_statuses(cx);
            window.focus(&self.focus_handle);
        } else {
            self.close_ai_model_selector();
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    fn ai_model_selector_visible_model_keys(&self) -> Vec<(String, String)> {
        let providers = ai_provider_views(&self.settings_store.settings().ai.providers);
        let searching = !self.ai_model_selector_search_query.trim().is_empty();
        // Tauri renders models as focusable dropdown items only for expanded
        // providers, while search mode expands matching providers. Keep the
        // keyboard target list identical to the rendered, selectable rows.
        model_selector_visible_provider_groups(&providers, &self.ai_model_selector_search_query)
            .into_iter()
            .filter(|group| {
                searching || self.ai_model_selector_expanded_providers.contains(&group.provider.id)
            })
            .filter(|group| {
                self.ai_model_selector_has_key(&group.provider)
                    && self.ai_model_selector_provider_is_online(&group.provider)
            })
            .flat_map(|group| {
                let provider_id = group.provider.id;
                group
                    .visible_models
                    .into_iter()
                    .map(move |model| (provider_id.clone(), model))
            })
            .collect()
    }

    fn move_ai_model_selector_highlight(&mut self, delta: isize) {
        let rows = self.ai_model_selector_visible_model_keys();
        if rows.is_empty() {
            self.ai_model_selector_highlighted_model = None;
            return;
        }
        let current = self
            .ai_model_selector_highlighted_model
            .as_ref()
            .and_then(|highlighted| rows.iter().position(|row| row == highlighted));
        let next = match (current, delta.is_negative()) {
            (Some(index), false) => (index + delta as usize).min(rows.len() - 1),
            (Some(index), true) => index.saturating_sub(delta.unsigned_abs()),
            (None, false) => 0,
            (None, true) => rows.len() - 1,
        };
        self.ai_model_selector_highlighted_model = rows.get(next).cloned();
    }

    fn set_ai_model_selector_highlight_edge(&mut self, last: bool) {
        let rows = self.ai_model_selector_visible_model_keys();
        // Home/End in Radix-style menu focus moves to the first/last selectable
        // model row, not to provider headers or disabled provider messages.
        self.ai_model_selector_highlighted_model = if last {
            rows.last().cloned()
        } else {
            rows.first().cloned()
        };
    }

    fn select_highlighted_ai_model(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((provider_id, model)) = self.ai_model_selector_highlighted_model.clone() else {
            return false;
        };
        if !self
            .ai_model_selector_visible_model_keys()
            .iter()
            .any(|row| row == &(provider_id.clone(), model.clone()))
        {
            self.ai_model_selector_highlighted_model = None;
            return false;
        }
        self.select_ai_model_from_selector(provider_id, model, cx);
        self.ai_model_selector_highlighted_model = None;
        true
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
        let profile_id = self.active_ai_conversation_profile_id().or_else(|| {
            self.settings_store
                .settings()
                .ai
                .execution_profiles
                .get("defaultProfileId")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
        self.edit_settings(
            |settings| {
                ai_select_provider_model(
                    &mut settings.ai.providers,
                    &mut settings.ai.active_provider_id,
                    &mut settings.ai.active_model,
                    &provider_id,
                    model.clone(),
                );
                Self::sync_ai_execution_profile_model(
                    &mut settings.ai.execution_profiles,
                    profile_id.as_deref(),
                    &provider_id,
                    &model,
                );
            },
            cx,
        );
        if previous_model.as_deref() != Some(model.as_str()) {
            self.update_ai_model_switch_warning(&provider_id, &model);
        }
        self.close_ai_model_selector();
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

    fn sync_ai_execution_profile_model(
        execution_profiles: &mut serde_json::Value,
        profile_id: Option<&str>,
        provider_id: &str,
        model: &str,
    ) {
        let Some(profile_id) = profile_id.filter(|profile_id| !profile_id.trim().is_empty()) else {
            return;
        };
        let Some(profiles) = execution_profiles
            .get_mut("profiles")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        let Some(profile) = profiles
            .iter_mut()
            .filter_map(serde_json::Value::as_object_mut)
            .find(|profile| {
                profile
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    == Some(profile_id)
            })
        else {
            return;
        };
        if profile
            .get("backend")
            .and_then(serde_json::Value::as_str)
            == Some("acp")
        {
            // ACP profiles delegate model choice to the selected agent, so the
            // provider/model selector must not rewrite their launch profile.
            return;
        }
        profile.insert(
            "providerId".to_string(),
            serde_json::Value::String(provider_id.to_string()),
        );
        profile.insert(
            "model".to_string(),
            serde_json::Value::String(model.to_string()),
        );
        profile.insert("updatedAt".to_string(), serde_json::json!(ai_now_ms()));
    }
}

pub(super) struct AiModelSelectorProbeDelivery {
    pub(super) provider_id: String,
    pub(super) generation: u64,
    pub(super) online: bool,
}

fn ai_model_selector_status_signature(providers: &[AiProviderView]) -> u64 {
    let mut hasher = DefaultHasher::new();
    providers.len().hash(&mut hasher);
    for provider in providers {
        provider.id.hash(&mut hasher);
        provider.enabled.hash(&mut hasher);
        provider.provider_type.hash(&mut hasher);
        provider.base_url.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod model_selector_status_signature_tests {
    use super::*;

    fn provider(id: &str, provider_type: &str, base_url: &str, enabled: bool) -> AiProviderView {
        AiProviderView {
            id: id.to_string(),
            provider_type: provider_type.to_string(),
            name: id.to_string(),
            base_url: base_url.to_string(),
            default_model: "model-a".to_string(),
            models: vec!["model-a".to_string()],
            enabled,
            custom: false,
        }
    }

    #[test]
    fn model_selector_status_signature_tracks_provider_probe_inputs() {
        let base = vec![provider("openai", "openai", "https://api.openai.com/v1", true)];
        let changed_base_url = vec![provider("openai", "openai", "http://localhost:11434", true)];
        let disabled = vec![provider("openai", "openai", "https://api.openai.com/v1", false)];
        let mut model_only_change = base.clone();
        model_only_change[0].models.push("model-b".to_string());

        assert_ne!(
            ai_model_selector_status_signature(&base),
            ai_model_selector_status_signature(&changed_base_url)
        );
        assert_ne!(
            ai_model_selector_status_signature(&base),
            ai_model_selector_status_signature(&disabled)
        );
        assert_eq!(
            ai_model_selector_status_signature(&base),
            ai_model_selector_status_signature(&model_only_change)
        );
    }
}
