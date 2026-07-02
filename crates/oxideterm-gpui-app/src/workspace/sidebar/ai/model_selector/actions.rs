impl WorkspaceApp {
    pub(super) fn ensure_ai_model_selector_mount_statuses(&mut self, cx: &mut Context<Self>) {
        let providers = self.ai_model_selector_providers();
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
            let providers = self.ai_model_selector_providers();
            if let Some(provider) = active_provider_view(
                &providers,
                self.ai_active_model_selector_provider_id().as_deref(),
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
        let providers = self.ai_model_selector_providers();
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
        let providers = self.ai_model_selector_providers();
        for provider in providers {
            if Self::ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
                self.ai_provider_key_status.insert(provider.id.clone(), true);
                self.ai_model_selector_provider_online
                    .insert(provider.id.clone(), self.ai_acp_provider_ready(&provider.id));
                continue;
            }
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
        if Self::ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
            return provider.enabled;
        }
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::Disabled => false,
            ModelSelectorProviderProbe::ImplicitKey { .. } => true,
            ModelSelectorProviderProbe::StoredKey => self.ai_provider_has_key(&provider.id),
        }
    }

    fn ai_model_selector_provider_is_online(&self, provider: &AiProviderView) -> bool {
        if Self::ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
            return self.ai_acp_provider_ready(&provider.id);
        }
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
        if Self::ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
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
        if let Some(agent_id) =
            Self::ai_acp_agent_id_from_provider_id(&provider_id).map(str::to_string)
        {
            self.edit_settings(
                move |settings| {
                    settings.ai.active_backend = AiActiveBackend::Acp;
                    settings.ai.active_acp_agent_id = Some(agent_id.clone());
                },
                cx,
            );
            self.close_ai_model_selector();
            cx.notify();
            return;
        }
        self.edit_settings(
            |settings| {
                settings.ai.active_backend = AiActiveBackend::Provider;
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
        self.close_ai_model_selector();
        cx.notify();
    }

    fn ai_model_selector_providers(&self) -> Vec<AiProviderView> {
        let settings = self.settings_store.settings();
        let mut providers = ai_provider_views(&settings.ai.providers);
        providers.extend(settings.ai.acp_agents.iter().map(Self::ai_acp_agent_provider_view));
        providers
    }

    fn ai_active_model_selector_provider_id(&self) -> Option<String> {
        let settings = self.settings_store.settings();
        if settings.ai.active_backend == AiActiveBackend::Acp {
            return settings
                .ai
                .active_acp_agent_id
                .as_deref()
                .map(Self::ai_acp_provider_id);
        }
        settings.ai.active_provider_id.clone()
    }

    fn ai_acp_agent_provider_view(agent: &AcpAgentConfig) -> AiProviderView {
        let label = Self::ai_acp_agent_label(agent);
        AiProviderView {
            id: Self::ai_acp_provider_id(&agent.id),
            provider_type: "acp".to_string(),
            name: format!("{label} (ACP)"),
            base_url: String::new(),
            default_model: label.clone(),
            models: vec![label],
            enabled: agent.enabled,
            custom: false,
        }
    }

    fn ai_acp_provider_id(agent_id: &str) -> String {
        format!("acp:{agent_id}")
    }

    fn ai_acp_agent_id_from_provider_id(provider_id: &str) -> Option<&str> {
        provider_id.strip_prefix("acp:")
    }

    fn ai_acp_agent_label(agent: &AcpAgentConfig) -> String {
        if agent.display_name.trim().is_empty() {
            agent.id.clone()
        } else {
            agent.display_name.clone()
        }
    }

    fn ai_acp_provider_ready(&self, provider_id: &str) -> bool {
        let Some(agent_id) = Self::ai_acp_agent_id_from_provider_id(provider_id) else {
            return false;
        };
        self.settings_store
            .settings()
            .ai
            .acp_agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .is_some_and(|agent| agent.enabled && agent.status.state == AcpAgentRuntimeState::Ready)
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
