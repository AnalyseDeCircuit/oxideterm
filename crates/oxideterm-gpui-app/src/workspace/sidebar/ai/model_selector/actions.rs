impl WorkspaceApp {
    pub(in crate::workspace) fn ai_acp_model_options_for_agent(
        &self,
        agent_id: &str,
    ) -> Option<Vec<oxideterm_ai::AcpSessionConfigOption>> {
        if let Some(state) = self.active_ai_acp_session_state(agent_id)
            && oxideterm_ai::acp_model_config_option(&state.config_options)
                .is_some_and(|option| !option.choices.is_empty())
        {
            return Some(state.config_options);
        }
        let conversation_id = self
            .ai
            .chat
            .conversation_state
            .active_conversation()
            .map(|conversation| conversation.id.as_str())?;
        self.ai
            .models
            .acp_model_options
            .get(&(conversation_id.to_string(), agent_id.to_string()))
            .cloned()
    }

    pub(in crate::workspace) fn ai_acp_model_discovery_is_pending(
        &self,
        agent_id: &str,
    ) -> bool {
        let Some(conversation_id) = self
            .ai
            .chat
            .conversation_state
            .active_conversation()
            .map(|conversation| conversation.id.as_str())
        else {
            return false;
        };
        self.ai
            .models
            .acp_model_discovery_pending
            .contains(&(conversation_id.to_string(), agent_id.to_string()))
    }

    pub(in crate::workspace) fn schedule_ai_acp_model_discovery(
        &mut self,
        agent_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.ai_acp_model_options_for_agent(&agent_id).is_some() {
            return;
        }
        let Some(conversation_id) = self
            .ai
            .chat
            .conversation_state
            .active_conversation()
            .map(|conversation| conversation.id.clone())
        else {
            return;
        };
        let discovery_key = (conversation_id.clone(), agent_id.clone());
        if self
            .ai
            .models
            .acp_model_discovery_pending
            .contains(&discovery_key)
        {
            return;
        }
        let Some(agent) = self
            .settings_store
            .settings()
            .ai
            .acp_agents
            .iter()
            .find(|agent| agent.id == agent_id && agent.enabled)
            .cloned()
        else {
            return;
        };
        // Only the native Codex adapter promises model metadata during session/new.
        // Other ACP agents keep their existing first-prompt or explicit-model behavior.
        if !oxideterm_ai::acp_model_report_is_available_during_session_start(&agent.args) {
            return;
        }
        if self.ai.models.acp_model_discovery_tx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            self.ai.models.acp_model_discovery_tx = Some(tx);
            self.ai.models.acp_model_discovery_rx = Some(rx);
        }
        let Some(ui_tx) = self.ai.models.acp_model_discovery_tx.as_ref().cloned() else {
            return;
        };
        self.ai
            .models
            .acp_model_discovery_pending
            .insert(discovery_key);
        let launch_config = acp_launch_config_from_agent(&agent);
        let capability_policy = acp_host_capability_policy_from_agent(&agent);
        let session_cwd = acp_session_cwd_from_agent(&agent);
        self.forwarding_runtime.spawn(async move {
            let config_options = match oxideterm_ai::build_acp_stdio_launcher(launch_config) {
                Ok(launcher) => oxideterm_ai::discover_acp_session_config_options(
                    launcher,
                    env!("CARGO_PKG_VERSION").to_string(),
                    capability_policy,
                    session_cwd,
                )
                .await
                .ok()
                .filter(|options| {
                    oxideterm_ai::acp_model_config_option(options)
                        .is_some_and(|option| !option.choices.is_empty())
                }),
                Err(_) => None,
            };
            let _ = ui_tx.send(AcpModelDiscoveryDelivery {
                conversation_id,
                agent_id,
                config_options,
            });
        });
        self.schedule_ai_acp_model_discovery_poll(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn poll_ai_acp_model_discovery_results(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(rx) = self.ai.models.acp_model_discovery_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        loop {
            match rx.try_recv() {
                Ok(delivery) => {
                    let key = (delivery.conversation_id.clone(), delivery.agent_id);
                    self.ai.models.acp_model_discovery_pending.remove(&key);
                    if let Some(options) = delivery.config_options
                        && self
                            .ai
                            .chat
                            .conversation_state
                            .conversations
                            .iter()
                            .any(|conversation| conversation.id == delivery.conversation_id)
                    {
                        self.ai.models.acp_model_options.insert(key, options);
                    }
                    cx.notify();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    keep_rx = false;
                    self.ai.models.acp_model_discovery_tx = None;
                    self.ai.models.acp_model_discovery_pending.clear();
                    break;
                }
            }
        }
        if keep_rx && !self.ai.models.acp_model_discovery_pending.is_empty() {
            self.ai.models.acp_model_discovery_rx = Some(rx);
        } else if self.ai.models.acp_model_discovery_pending.is_empty() {
            self.ai.models.acp_model_discovery_tx = None;
        }
    }

    pub(in crate::workspace) fn schedule_ai_acp_model_discovery_poll(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.ai.models.acp_model_discovery_polling {
            return;
        }
        self.ai.models.acp_model_discovery_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(50)).await;
            let _ = weak.update(cx, |this, cx| {
                this.ai.models.acp_model_discovery_polling = false;
                this.poll_ai_acp_model_discovery_results(cx);
                if !this.ai.models.acp_model_discovery_pending.is_empty() {
                    this.schedule_ai_acp_model_discovery_poll(cx);
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn ensure_ai_model_selector_mount_statuses(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let providers = self.ai_model_selector_providers();
        let signature = ai_model_selector_status_signature(&providers);
        if self.ai.models.selector_status_signature == signature {
            return;
        }
        self.ai.models.selector_status_signature = signature;
        // Mirrors Tauri ModelSelector's mount/provider-change checkAllKeys
        // effect: the trigger indicator starts probing before the user opens it.
        self.refresh_ai_model_selector_provider_statuses(cx);
    }

    pub(in crate::workspace) fn toggle_ai_model_selector(
        &mut self,
        scope: AiModelSelectorScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next_open =
            !(self.ai.models.selector_open && self.ai.models.selector_scope == Some(scope));
        self.close_ai_sidebar_popovers();
        self.ai.models.selector_open = next_open;
        self.ai.models.selector_scope = next_open.then_some(scope);
        if self.ai.models.selector_open {
            let providers = self.ai_model_selector_providers();
            let mut active_acp_agent_id = None;
            if let Some(provider) = active_provider_view(
                &providers,
                self.ai_active_model_selector_provider_id().as_deref(),
            ) {
                active_acp_agent_id =
                    Self::ai_acp_agent_id_from_provider_id(&provider.id).map(str::to_string);
                self.ai
                    .models
                    .selector_expanded_providers
                    .insert(provider.id.clone());
            }
            if let Some(agent_id) = active_acp_agent_id {
                self.schedule_ai_acp_model_discovery(agent_id, cx);
            }
            self.ai.models.selector_search_focused = true;
            self.ai.models.selector_highlighted_model = None;
            self.ai.chat.input_focused = false;
            self.ai.chat.inline_panel.prompt_focused = false;
            self.refresh_ai_model_selector_provider_statuses(cx);
window.focus(&self.focus_handle, cx);
        } else {
            self.close_ai_model_selector();
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn ai_model_selector_visible_model_keys(
        &self,
    ) -> Vec<(String, String)> {
        let providers = self.ai_model_selector_providers();
        let searching = !self.ai.models.selector_search_query.trim().is_empty();
        // Tauri renders models as focusable dropdown items only for expanded
        // providers, while search mode expands matching providers. Keep the
        // keyboard target list identical to the rendered, selectable rows.
        model_selector_visible_provider_groups(&providers, &self.ai.models.selector_search_query)
            .into_iter()
            .filter(|group| {
                searching
                    || self
                        .ai
                        .models
                        .selector_expanded_providers
                        .contains(&group.provider.id)
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

    pub(in crate::workspace) fn move_ai_model_selector_highlight(&mut self, delta: isize) {
        let rows = self.ai_model_selector_visible_model_keys();
        if rows.is_empty() {
            self.ai.models.selector_highlighted_model = None;
            return;
        }
        let current = self
            .ai
            .models
            .selector_highlighted_model
            .as_ref()
            .and_then(|highlighted| rows.iter().position(|row| row == highlighted));
        let next = match (current, delta.is_negative()) {
            (Some(index), false) => (index + delta as usize).min(rows.len() - 1),
            (Some(index), true) => index.saturating_sub(delta.unsigned_abs()),
            (None, false) => 0,
            (None, true) => rows.len() - 1,
        };
        self.ai.models.selector_highlighted_model = rows.get(next).cloned();
    }

    pub(in crate::workspace) fn set_ai_model_selector_highlight_edge(&mut self, last: bool) {
        let rows = self.ai_model_selector_visible_model_keys();
        // Home/End in Radix-style menu focus moves to the first/last selectable
        // model row, not to provider headers or disabled provider messages.
        self.ai.models.selector_highlighted_model = if last {
            rows.last().cloned()
        } else {
            rows.first().cloned()
        };
    }

    pub(in crate::workspace) fn select_highlighted_ai_model(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((provider_id, model)) = self.ai.models.selector_highlighted_model.clone() else {
            return false;
        };
        if !self
            .ai_model_selector_visible_model_keys()
            .iter()
            .any(|row| row == &(provider_id.clone(), model.clone()))
        {
            self.ai.models.selector_highlighted_model = None;
            return false;
        }
        self.select_ai_model_from_selector(provider_id, model, cx);
        self.ai.models.selector_highlighted_model = None;
        true
    }

    pub(in crate::workspace) fn refresh_ai_model_selector_provider_statuses(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.ensure_ai_provider_key_statuses(cx);
        let providers = self.ai_model_selector_providers();
        for provider in providers {
            if Self::ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
                self.ai
                    .models
                    .provider_key_status
                    .insert(provider.id.clone(), true);
                self.ai.models.selector_provider_online.insert(
                    provider.id.clone(),
                    self.ai_acp_provider_ready(&provider.id),
                );
                continue;
            }
            match resolve_model_selector_provider_probe(&provider) {
                ModelSelectorProviderProbe::Disabled => {
                    self.ai
                        .models
                        .provider_key_status
                        .insert(provider.id.clone(), false);
                    self.ai
                        .models
                        .selector_provider_online
                        .insert(provider.id.clone(), false);
                }
                ModelSelectorProviderProbe::StoredKey => {
                    let has_key = self.ai_provider_has_key(&provider.id);
                    self.ai
                        .models
                        .provider_key_status
                        .insert(provider.id.clone(), has_key);
                    self.ai
                        .models
                        .selector_provider_online
                        .insert(provider.id.clone(), true);
                }
                ModelSelectorProviderProbe::ImplicitKey { endpoint } => {
                    self.ai
                        .models
                        .provider_key_status
                        .insert(provider.id.clone(), true);
                    if let Some(endpoint) = endpoint {
                        self.schedule_ai_model_selector_online_probe(
                            provider.clone(),
                            endpoint,
                            cx,
                        );
                    } else {
                        self.ai
                            .models
                            .selector_provider_online
                            .insert(provider.id.clone(), true);
                    }
                }
            }
        }
    }

    pub(in crate::workspace) fn schedule_ai_model_selector_online_probe(
        &mut self,
        provider: AiProviderView,
        endpoint: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.ai.models.next_selector_probe_generation = self
            .ai
            .models
            .next_selector_probe_generation
            .saturating_add(1);
        let generation = self.ai.models.next_selector_probe_generation;
        let provider_id = provider.id.clone();
        self.ai
            .models
            .selector_probe_generations
            .insert(provider_id.clone(), generation);
        if self.ai.models.selector_probe_tx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            self.ai.models.selector_probe_tx = Some(tx);
            self.ai.models.selector_probe_rx = Some(rx);
        }
        let Some(ui_tx) = self.ai.models.selector_probe_tx.as_ref().cloned() else {
            return;
        };
        self.ai.models.selector_probe_pending =
            self.ai.models.selector_probe_pending.saturating_add(1);
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

    pub(in crate::workspace) fn poll_ai_model_selector_probe_results(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(rx) = self.ai.models.selector_probe_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        loop {
            match rx.try_recv() {
                Ok(delivery) => {
                    self.ai.models.selector_probe_pending =
                        self.ai.models.selector_probe_pending.saturating_sub(1);
                    if self
                        .ai
                        .models
                        .selector_probe_generations
                        .get(&delivery.provider_id)
                        == Some(&delivery.generation)
                    {
                        self.ai
                            .models
                            .selector_provider_online
                            .insert(delivery.provider_id, delivery.online);
                        cx.notify();
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    keep_rx = false;
                    self.ai.models.selector_probe_tx = None;
                    self.ai.models.selector_probe_pending = 0;
                    break;
                }
            }
        }
        if keep_rx && self.ai.models.selector_probe_pending > 0 {
            self.ai.models.selector_probe_rx = Some(rx);
        } else if self.ai.models.selector_probe_pending == 0 {
            self.ai.models.selector_probe_tx = None;
        }
    }

    pub(in crate::workspace) fn schedule_ai_model_selector_probe_poll(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.ai.models.selector_probe_polling {
            return;
        }
        self.ai.models.selector_probe_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(50)).await;
            let _ = weak.update(cx, |this, cx| {
                this.ai.models.selector_probe_polling = false;
                this.poll_ai_model_selector_probe_results(cx);
                if this.ai.models.selector_probe_pending > 0 {
                    this.schedule_ai_model_selector_probe_poll(cx);
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn ai_model_selector_has_key(
        &self,
        provider: &AiProviderView,
    ) -> bool {
        if Self::ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
            return provider.enabled;
        }
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::Disabled => false,
            ModelSelectorProviderProbe::ImplicitKey { .. } => true,
            ModelSelectorProviderProbe::StoredKey => self.ai_provider_has_key(&provider.id),
        }
    }

    pub(in crate::workspace) fn ai_model_selector_provider_is_online(
        &self,
        provider: &AiProviderView,
    ) -> bool {
        if Self::ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
            return self.ai_acp_provider_ready(&provider.id);
        }
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::Disabled => false,
            ModelSelectorProviderProbe::StoredKey => true,
            ModelSelectorProviderProbe::ImplicitKey { .. } => self
                .ai
                .models
                .selector_provider_online
                .get(&provider.id)
                .copied()
                .unwrap_or(true),
        }
    }

    pub(in crate::workspace) fn refresh_ai_provider_from_selector(
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

    pub(in crate::workspace) fn select_ai_model_from_selector(
        &mut self,
        provider_id: String,
        model: String,
        cx: &mut Context<Self>,
    ) {
        let previous_model = self.settings_store.settings().ai.active_model.clone();
        if let Some(agent_id) =
            Self::ai_acp_agent_id_from_provider_id(&provider_id).map(str::to_string)
        {
            let session_model_selection = self
                .ai_acp_model_options_for_agent(&agent_id)
                .and_then(|options| {
                    let option = oxideterm_ai::acp_model_config_option(&options)?;
                    let choice = option.choices.iter().find(|choice| choice.label == model)?;
                    Some((option.config_id.clone(), choice.value_id.clone()))
                });
            if let Some((config_id, value_id)) = session_model_selection {
                self.select_ai_acp_model_from_selector(
                    agent_id,
                    config_id,
                    value_id,
                    cx,
                );
                return;
            }
            self.edit_settings(
                move |settings| {
                    settings.ai.active_backend = AiActiveBackend::Acp;
                    settings.ai.active_acp_agent_id = Some(agent_id);
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

    pub(in crate::workspace) fn ai_model_selector_providers(&self) -> Vec<AiProviderView> {
        let settings = self.settings_store.settings();
        let mut providers = ai_provider_views(&settings.ai.providers);
        providers.extend(
            settings
                .ai
                .acp_agents
                .iter()
                .map(|agent| self.ai_acp_agent_provider_view(agent)),
        );
        providers
    }

    pub(in crate::workspace) fn ai_active_model_selector_provider_id(&self) -> Option<String> {
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

    pub(in crate::workspace) fn ai_acp_agent_provider_view(
        &self,
        agent: &AcpAgentConfig,
    ) -> AiProviderView {
        let label = Self::ai_acp_agent_label(agent);
        let fallback_model = self.ai_acp_agent_model_fallback_label(&agent.id);
        let models = self
            .ai_acp_model_options_for_agent(&agent.id)
            .and_then(|options| oxideterm_ai::acp_model_config_option(&options).cloned())
            .map(|option| {
                option
                    .choices
                    .into_iter()
                    .map(|choice| choice.label)
                    .collect::<Vec<_>>()
            })
            .filter(|models| !models.is_empty())
            .unwrap_or_else(|| vec![fallback_model.clone()]);
        AiProviderView {
            id: Self::ai_acp_provider_id(&agent.id),
            provider_type: "acp".to_string(),
            name: format!("{label} (ACP)"),
            base_url: String::new(),
            default_model: fallback_model,
            models,
            enabled: agent.enabled,
            custom: false,
        }
    }

    pub(in crate::workspace) fn ai_acp_provider_id(agent_id: &str) -> String {
        format!("acp:{agent_id}")
    }

    pub(in crate::workspace) fn ai_acp_agent_id_from_provider_id(
        provider_id: &str,
    ) -> Option<&str> {
        provider_id.strip_prefix("acp:")
    }

    pub(in crate::workspace) fn ai_acp_agent_label(agent: &AcpAgentConfig) -> String {
        if agent.display_name.trim().is_empty() {
            agent.id.clone()
        } else {
            agent.display_name.clone()
        }
    }

    /// Reports explicit launch configuration without inventing unavailable ACP metadata.
    pub(in crate::workspace) fn ai_acp_agent_model_fallback_label(
        &self,
        agent_id: &str,
    ) -> String {
        let agent = self
            .settings_store
            .settings()
            .ai
            .acp_agents
            .iter()
            .find(|agent| agent.id == agent_id);
        let explicit_model = agent
            .and_then(|agent| oxideterm_ai::acp_launch_model_hint(&agent.args))
            .and_then(|hint| match hint {
                oxideterm_ai::AcpLaunchModelHint::Fixed(model) => Some(model),
                oxideterm_ai::AcpLaunchModelHint::Automatic => None,
            });
        explicit_model.unwrap_or_else(|| {
            if self.ai_acp_model_discovery_is_pending(agent_id) {
                self.i18n.t("ai.model_selector.agent_model_loading")
            } else if agent.is_some_and(|agent| {
                oxideterm_ai::acp_model_report_is_deferred_until_first_prompt(&agent.args)
            }) {
                self.i18n
                    .t("ai.model_selector.agent_model_after_first_message")
            } else {
                self.i18n.t("ai.model_selector.agent_model_unavailable")
            }
        })
    }

    pub(in crate::workspace) fn ai_acp_provider_ready(&self, provider_id: &str) -> bool {
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

    pub(in crate::workspace) fn select_ai_acp_model_from_selector(
        &mut self,
        agent_id: String,
        config_id: String,
        value_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(discovered_options) = self.ai_acp_model_options_for_agent(&agent_id) else {
            return;
        };
        let Some(conversation) = self.ai.chat.conversation_state.active_conversation_mut() else {
            return;
        };
        if !store_ai_acp_model_selection_in_conversation(
            conversation,
            &agent_id,
            discovered_options,
            &config_id,
            &value_id,
        ) {
            return;
        }
        self.edit_settings(
            move |settings| {
                settings.ai.active_backend = AiActiveBackend::Acp;
                settings.ai.active_acp_agent_id = Some(agent_id);
            },
            cx,
        );
        self.persist_ai_chat_state();
        self.close_ai_model_selector();
        cx.notify();
    }

    pub(in crate::workspace) fn update_ai_model_switch_warning(
        &mut self,
        provider_id: &str,
        model: &str,
    ) {
        let Some(conversation) = self.ai.chat.conversation_state.active_conversation() else {
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
            self.ai.chat.model_switch_warning_percentage = Some(percentage.round() as usize);
        }
    }
}

pub(in crate::workspace) fn store_ai_acp_model_selection_in_conversation(
    conversation: &mut AiConversation,
    agent_id: &str,
    discovered_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
    config_id: &str,
    value_id: &str,
) -> bool {
    let mut state = ai_acp_session_state(conversation)
        .filter(|state| state.agent_id == agent_id)
        .unwrap_or_else(|| AiAcpSessionState {
            agent_id: agent_id.to_string(),
            session_id: String::new(),
            metadata: None,
            config_options: discovered_options,
            model_selection: None,
        });
    let Some(option) = state
        .config_options
        .iter_mut()
        .find(|option| option.config_id == config_id)
    else {
        return false;
    };
    if !option
        .choices
        .iter()
        .any(|choice| choice.value_id == value_id)
    {
        return false;
    }

    // An empty session id marks a pre-prompt choice. The prompt path creates a
    // real session and applies this value before sending the user's message.
    option.current_value_id = value_id.to_string();
    state.model_selection = Some(oxideterm_ai::AcpSessionConfigSelection {
        config_id: config_id.to_string(),
        value_id: value_id.to_string(),
    });
    let conversation_id = conversation.id.clone();
    let metadata = conversation.session_metadata.get_or_insert_with(|| {
        serde_json::json!({
            "conversationId": conversation_id,
            "origin": "sidebar",
        })
    });
    let Some(metadata) = metadata.as_object_mut() else {
        return false;
    };
    let Ok(value) = serde_json::to_value(state) else {
        return false;
    };
    metadata.insert(AI_ACP_SESSION_METADATA_KEY.to_string(), value);
    true
}

pub(in crate::workspace) struct AiModelSelectorProbeDelivery {
    pub(in crate::workspace) provider_id: String,
    pub(in crate::workspace) generation: u64,
    pub(in crate::workspace) online: bool,
}

pub(in crate::workspace) fn ai_model_selector_status_signature(
    providers: &[AiProviderView],
) -> u64 {
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
mod acp_model_selection_tests {
    use super::*;

    #[test]
    fn discovered_model_choice_is_stored_for_the_next_real_session() {
        let mut conversation = AiConversation {
            id: "conversation-1".to_string(),
            title: "Conversation".to_string(),
            messages: Vec::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
            origin: "sidebar".to_string(),
            profile_id: None,
            message_count: 0,
            session_id: None,
            session_metadata: None,
            messages_loaded: true,
        };
        let options = vec![oxideterm_ai::AcpSessionConfigOption {
            config_id: "model".to_string(),
            name: "Model".to_string(),
            category: Some("model".to_string()),
            current_value_id: "gpt-5.6-sol".to_string(),
            choices: vec![
                oxideterm_ai::AcpSessionConfigChoice {
                    value_id: "gpt-5.6-sol".to_string(),
                    label: "gpt-5.6-sol".to_string(),
                },
                oxideterm_ai::AcpSessionConfigChoice {
                    value_id: "gpt-5.6-terra".to_string(),
                    label: "gpt-5.6-terra".to_string(),
                },
            ],
        }];

        assert!(store_ai_acp_model_selection_in_conversation(
            &mut conversation,
            "codex",
            options,
            "model",
            "gpt-5.6-terra",
        ));

        let state = ai_acp_session_state(&conversation).expect("stored ACP model state");
        assert_eq!(state.agent_id, "codex");
        assert!(state.session_id.is_empty());
        assert_eq!(
            state.model_selection,
            Some(oxideterm_ai::AcpSessionConfigSelection {
                config_id: "model".to_string(),
                value_id: "gpt-5.6-terra".to_string(),
            })
        );
        assert_eq!(state.config_options[0].current_value_id, "gpt-5.6-terra");
    }
}

#[cfg(test)]
mod model_selector_status_signature_tests {
    use super::*;

    pub(in crate::workspace) fn provider(
        id: &str,
        provider_type: &str,
        base_url: &str,
        enabled: bool,
    ) -> AiProviderView {
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
    pub(in crate::workspace) fn model_selector_status_signature_tracks_provider_probe_inputs() {
        let base = vec![provider(
            "openai",
            "openai",
            "https://api.openai.com/v1",
            true,
        )];
        let changed_base_url = vec![provider("openai", "openai", "http://localhost:11434", true)];
        let disabled = vec![provider(
            "openai",
            "openai",
            "https://api.openai.com/v1",
            false,
        )];
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
