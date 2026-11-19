use std::hash::{DefaultHasher, Hash, Hasher};

use oxideterm_ai::{ModelSelectorProviderProbe, provider_views as ai_provider_views};
use oxideterm_settings::{AcpAgentConfig, AcpAgentRuntimeState, AiActiveBackend, AiSettings};

use super::*;

pub fn ai_acp_provider_id(agent_id: &str) -> String {
    format!("acp:{agent_id}")
}

pub fn ai_acp_agent_id_from_provider_id(provider_id: &str) -> Option<&str> {
    provider_id.strip_prefix("acp:")
}

pub fn active_model_picker_provider(settings: &AiSettings) -> Option<String> {
    if settings.active_backend == AiActiveBackend::Acp {
        return settings
            .active_acp_agent_id
            .as_deref()
            .map(ai_acp_provider_id);
    }
    settings.active_provider_id.clone()
}

pub enum AiPickerSelection {
    AcpSession {
        agent_id: String,
        config_id: String,
        value_id: String,
    },
    AcpAgent(String),
    Provider,
}

impl AiWorkspaceEntity {
    pub fn update_model_switch_warning(
        &mut self,
        settings: &AiSettings,
        provider_id: &str,
        model: &str,
        total_tokens: usize,
    ) {
        use oxideterm_ai::stream_state::{
            AI_COMPACTION_DEFAULT_CONTEXT_WINDOW, AI_CONTEXT_WARNING_PERCENT,
            ai_context_percentage, ai_context_window_from_maps,
        };
        if total_tokens == 0 {
            return;
        }
        let max_tokens = ai_context_window_from_maps(
            &settings.user_context_windows,
            &settings.model_context_windows,
            provider_id,
            model,
        )
        .unwrap_or(AI_COMPACTION_DEFAULT_CONTEXT_WINDOW);
        let percentage = ai_context_percentage(total_tokens, max_tokens);
        if percentage > AI_CONTEXT_WARNING_PERCENT {
            self.set_model_switch_warning(Some(percentage.round() as usize));
        }
    }

    pub fn plan_picker_selection(&self, provider: &str, model: &str) -> AiPickerSelection {
        let Some(agent_id) = ai_acp_agent_id_from_provider_id(provider) else {
            return AiPickerSelection::Provider;
        };
        if let Some(options) = self.picker_acp_options(agent_id)
            && let Some(option) = oxideterm_ai::acp_model_config_option(&options)
            && let Some(choice) = option.choices.iter().find(|choice| choice.label == model)
        {
            return AiPickerSelection::AcpSession {
                agent_id: agent_id.to_owned(),
                config_id: option.config_id.clone(),
                value_id: choice.value_id.clone(),
            };
        }
        AiPickerSelection::AcpAgent(agent_id.to_owned())
    }

    pub fn validated_picker_highlight(
        &mut self,
        rows: &[(String, String)],
    ) -> Option<(String, String)> {
        let row = self.model_selector_highlight()?.clone();
        if rows.contains(&row) {
            Some(row)
        } else {
            self.set_model_selector_highlight(None);
            None
        }
    }

    pub fn toggle_picker(
        &mut self,
        scope: AiModelSelectorScope,
        settings: &AiSettings,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<Option<String>> {
        let next_open = !self.model_selector_is_open(scope);
        self.close_chat_popovers();
        self.dismiss_model_selector();
        self.set_model_selector_open(scope, next_open);
        cx.notify();
        if !next_open {
            self.dismiss_model_selector();
            return None;
        }
        let providers = self.picker_providers(settings, i18n);
        let active = active_provider_view(
            &providers,
            active_model_picker_provider(settings).as_deref(),
        );
        let agent = active.and_then(|provider| {
            self.expand_model_selector_provider(provider.id.clone());
            ai_acp_agent_id_from_provider_id(&provider.id).map(str::to_owned)
        });
        self.blur_chat_input(false);
        self.terminal_inline_panel_mut().prompt_focused = false;
        Some(agent)
    }

    pub fn prepare_picker_acp_discovery(
        &self,
        agent_id: &str,
        settings: &AiSettings,
    ) -> Option<(String, AcpAgentConfig)> {
        if self.picker_acp_options(agent_id).is_some() {
            return None;
        }
        let conversation = &self.conversation_state().active_conversation()?.id;
        if self.acp_model_discovery_is_pending(conversation, agent_id) {
            return None;
        }
        let agent = settings
            .acp_agents
            .iter()
            .find(|agent| agent.id == agent_id && agent.enabled)?;
        // Other ACP agents discover models after the first prompt or use an explicit model.
        (agent.plugin_id.as_deref() == Some("com.oxideterm.acp.codex")
            || oxideterm_ai::acp_model_report_is_available_during_session_start(&agent.args))
        .then(|| (conversation.clone(), agent.clone()))
    }

    pub fn picker_refresh_index(
        &self,
        provider: &AiProviderView,
        settings: &AiSettings,
        plugin_available: impl Fn(&str) -> bool,
    ) -> Result<Option<usize>, &'static str> {
        if !self.picker_has_key(provider) {
            return Err("ai.model_selector.no_key_warning");
        }
        if !self.picker_online(provider, settings, plugin_available) {
            return Err("ai.model_selector.offline");
        }
        if ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
            return Ok(None);
        }
        Ok(ai_provider_views(&settings.providers)
            .iter()
            .position(|candidate| candidate.id == provider.id))
    }

    pub fn active_ai_acp_session_state(&self, agent_id: &str) -> Option<AiAcpSessionState> {
        self.conversation_state()
            .active_conversation()
            .and_then(ai_acp_session_state)
            .filter(|state| state.agent_id == agent_id)
    }

    pub fn picker_acp_options(&self, agent_id: &str) -> Option<Vec<AcpSessionConfigOption>> {
        if let Some(state) = self.active_ai_acp_session_state(agent_id)
            && oxideterm_ai::acp_model_config_option(&state.config_options)
                .is_some_and(|option| !option.choices.is_empty())
        {
            return Some(state.config_options);
        }
        let conversation_id = &self.conversation_state().active_conversation()?.id;
        self.acp_model_options(conversation_id, agent_id)
    }

    fn picker_acp_discovery_pending(&self, agent_id: &str) -> bool {
        self.conversation_state()
            .active_conversation()
            .is_some_and(|conversation| {
                self.acp_model_discovery_is_pending(&conversation.id, agent_id)
            })
    }

    pub fn picker_providers(&self, settings: &AiSettings, i18n: &I18n) -> Vec<AiProviderView> {
        let mut providers = ai_provider_views(&settings.providers);
        providers.extend(settings.acp_agents.iter().map(|agent| {
            let label = if agent.display_name.trim().is_empty() {
                &agent.id
            } else {
                &agent.display_name
            };
            let models = self
                .picker_acp_options(&agent.id)
                .and_then(|options| oxideterm_ai::acp_model_config_option(&options).cloned())
                .map(|option| {
                    option
                        .choices
                        .into_iter()
                        .map(|choice| choice.label)
                        .collect::<Vec<_>>()
                })
                .filter(|models| !models.is_empty())
                .unwrap_or_else(|| vec![self.picker_acp_fallback(agent, i18n)]);
            AiProviderView {
                api_protocol: oxideterm_ai::AiApiProtocol::default(),
                id: ai_acp_provider_id(&agent.id),
                provider_type: "acp".to_owned(),
                name: format!("{label} (ACP)"),
                base_url: String::new(),
                models,
                enabled: agent.enabled,
                custom: false,
            }
        }));
        providers
    }

    pub(crate) fn picker_acp_fallback_for_id(
        &self,
        settings: &AiSettings,
        agent_id: &str,
        i18n: &I18n,
    ) -> String {
        if let Some(agent) = settings
            .acp_agents
            .iter()
            .find(|agent| agent.id == agent_id)
        {
            return self.picker_acp_fallback(agent, i18n);
        }
        if self.picker_acp_discovery_pending(agent_id) {
            i18n.t("ai.model_selector.agent_model_loading")
        } else {
            i18n.t("ai.model_selector.agent_model_unavailable")
        }
    }

    fn picker_acp_fallback(&self, agent: &AcpAgentConfig, i18n: &I18n) -> String {
        if let Some(oxideterm_ai::AcpLaunchModelHint::Fixed(model)) =
            oxideterm_ai::acp_launch_model_hint(&agent.args)
        {
            return model;
        }
        if self.picker_acp_discovery_pending(&agent.id) {
            i18n.t("ai.model_selector.agent_model_loading")
        } else if agent.plugin_id.as_deref() == Some("com.oxideterm.acp.claude-code")
            || oxideterm_ai::acp_model_report_is_deferred_until_first_prompt(&agent.args)
        {
            i18n.t("ai.model_selector.agent_model_after_first_message")
        } else {
            i18n.t("ai.model_selector.agent_model_unavailable")
        }
    }

    pub fn picker_has_key(&self, provider: &AiProviderView) -> bool {
        if ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
            return provider.enabled;
        }
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::Disabled => false,
            ModelSelectorProviderProbe::ImplicitKey { .. } => true,
            ModelSelectorProviderProbe::StoredKey => {
                !ai_provider_chat_requires_key(&provider.provider_type)
                    || self.provider_has_key(&provider.id)
            }
        }
    }

    pub fn picker_online(
        &self,
        provider: &AiProviderView,
        settings: &AiSettings,
        plugin_available: impl Fn(&str) -> bool,
    ) -> bool {
        if let Some(agent_id) = ai_acp_agent_id_from_provider_id(&provider.id) {
            return settings
                .acp_agents
                .iter()
                .find(|agent| agent.id == agent_id)
                .is_some_and(|agent| {
                    agent.enabled
                        && agent.status.state == AcpAgentRuntimeState::Ready
                        && agent.plugin_id.as_deref().is_none_or(plugin_available)
                });
        }
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::Disabled => false,
            ModelSelectorProviderProbe::StoredKey => true,
            ModelSelectorProviderProbe::ImplicitKey { .. } => {
                self.selector_provider_is_online(&provider.id)
            }
        }
    }

    pub fn picker_status(
        &self,
        provider: &AiProviderView,
        settings: &AiSettings,
        i18n: &I18n,
        plugin_available: impl Fn(&str) -> bool,
    ) -> AiModelPickerStatus {
        let agent_id = ai_acp_agent_id_from_provider_id(&provider.id);
        AiModelPickerStatus {
            has_key: self.picker_has_key(provider),
            online: self.picker_online(provider, settings, plugin_available),
            acp_fallback_label: agent_id
                .and_then(|id| settings.acp_agents.iter().find(|agent| agent.id == id))
                .map(|agent| self.picker_acp_fallback(agent, i18n)),
            acp_discovery_pending: agent_id.is_some_and(|id| self.picker_acp_discovery_pending(id)),
            acp_options: agent_id.and_then(|id| self.picker_acp_options(id)),
        }
    }

    pub fn picker_visible_model_keys(
        &self,
        settings: &AiSettings,
        i18n: &I18n,
        plugin_available: impl Fn(&str) -> bool,
    ) -> Vec<(String, String)> {
        let providers = self.picker_providers(settings, i18n);
        let ui = self.model_ui();
        let searching = !ui.selector_search_query.trim().is_empty();
        // Keyboard navigation follows the expanded, selectable model rows.
        model_selector_visible_provider_groups(&providers, &ui.selector_search_query)
            .into_iter()
            .filter(|group| {
                searching || ui.selector_expanded_providers.contains(&group.provider.id)
            })
            .filter(|group| {
                self.picker_has_key(&group.provider)
                    && self.picker_online(&group.provider, settings, &plugin_available)
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

    pub fn update_picker_visibility(&mut self, visibility: crate::state::AiWorkspaceVisibility) {
        if self.set_workspace_visibility(visibility) && !visibility.model_selector_surface {
            // Remounting must probe again even if the provider list didn't change.
            self.set_model_selector_status_signature(None);
        }
    }

    pub fn picker_status_changed(&mut self, providers: &[AiProviderView]) -> bool {
        let signature = ai_model_selector_status_signature(providers);
        if self.model_selector_status_signature_matches(signature) {
            return false;
        }
        self.set_model_selector_status_signature(Some(signature));
        true
    }

    // The picker owns status and probe scheduling; only installed-plugin preflight needs the host.
    pub fn refresh_picker_statuses(
        &mut self,
        providers: Vec<AiProviderView>,
        settings: &AiSettings,
    ) -> Vec<String> {
        let mut agents_to_probe = Vec::new();
        for provider in providers {
            if let Some(agent_id) = ai_acp_agent_id_from_provider_id(&provider.id) {
                self.set_provider_key_status(provider.id.clone(), true);
                if settings.acp_agents.iter().any(|agent| {
                    agent.id == agent_id
                        && agent.enabled
                        && agent.plugin_id.is_some()
                        && agent.status.state == AcpAgentRuntimeState::Unknown
                }) {
                    agents_to_probe.push(agent_id.to_owned());
                }
                continue;
            }
            match resolve_model_selector_provider_probe(&provider) {
                ModelSelectorProviderProbe::Disabled => {
                    self.set_provider_key_status(provider.id.clone(), false);
                    self.set_selector_provider_online(provider.id, false);
                }
                ModelSelectorProviderProbe::StoredKey => {
                    self.set_selector_provider_online(provider.id, true)
                }
                ModelSelectorProviderProbe::ImplicitKey { endpoint } => {
                    self.set_provider_key_status(provider.id.clone(), true);
                    if let Some(endpoint) = endpoint {
                        self.request_selector_provider_probe(provider, endpoint);
                    } else {
                        self.set_selector_provider_online(provider.id, true);
                    }
                }
            }
        }
        agents_to_probe
    }
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
            api_protocol: oxideterm_ai::AiApiProtocol::default(),
            id: id.to_string(),
            provider_type: provider_type.to_string(),
            name: id.to_string(),
            base_url: base_url.to_string(),
            models: vec!["model-a".to_string()],
            enabled,
            custom: false,
        }
    }

    #[test]
    fn model_selector_status_signature_tracks_provider_probe_inputs() {
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
