impl AiWorkspaceEntity {
    fn set_active_reasoning_level(
        &mut self,
        provider_id: &str,
        model: &str,
        level: AiReasoningLevel,
    ) {
        if let Some(conversation) = self.conversation_state_mut().active_conversation_mut() {
            store_ai_reasoning_level_in_conversation(conversation, provider_id, model, level);
            self.persist_chat_state();
        }
    }

    fn set_active_acp_model_selection(
        &mut self,
        agent_id: &str,
        discovered_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
        config_id: &str,
        value_id: &str,
    ) -> bool {
        let Some(conversation) = self.conversation_state_mut().active_conversation_mut() else {
            return false;
        };
        let stored = store_ai_acp_model_selection_in_conversation(
            conversation,
            agent_id,
            discovered_options,
            config_id,
            value_id,
        );
        if stored {
            self.persist_chat_state();
        }
        stored
    }

    fn set_active_acp_config_selection(
        &mut self,
        agent_id: &str,
        discovered_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
        config_id: &str,
        value_id: &str,
    ) -> bool {
        let Some(conversation) = self.conversation_state_mut().active_conversation_mut() else {
            return false;
        };
        let stored = store_ai_acp_config_selection_in_conversation(
            conversation,
            agent_id,
            discovered_options,
            config_id,
            value_id,
            false,
        );
        if stored {
            self.persist_chat_state();
        }
        stored
    }

    fn set_active_acp_mode(&mut self, agent_id: &str, mode_id: &str) -> bool {
        let Some(conversation) = self.conversation_state_mut().active_conversation_mut() else {
            return false;
        };
        let Some(mut state) =
            ai_acp_session_state(conversation).filter(|state| state.agent_id == agent_id)
        else {
            return false;
        };
        if !state
            .available_modes
            .iter()
            .any(|mode| mode.mode_id == mode_id)
        {
            return false;
        }
        state.current_mode_id = Some(mode_id.to_string());
        let Some(metadata) = conversation
            .session_metadata
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
        else {
            return false;
        };
        let Ok(value) = serde_json::to_value(state) else {
            return false;
        };
        metadata.insert(AI_ACP_SESSION_METADATA_KEY.to_string(), value);
        self.persist_chat_state();
        true
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn select_ai_reasoning_level(
        &mut self,
        provider_id: String,
        provider_type: String,
        model: String,
        level: AiReasoningLevel,
        cx: &mut Context<Self>,
    ) {
        let level = oxideterm_ai::normalize_reasoning_level_for_model(
            &provider_type,
            &model,
            level.as_str(),
        );
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_chat_popover_open(AiChatPopover::Reasoning, false);
            ai.set_active_reasoning_level(&provider_id, &model, level);
        });
        self.edit_settings(
            move |settings| {
                set_ai_model_reasoning_override(
                    settings,
                    &provider_id,
                    &model,
                    (level != AiReasoningLevel::Auto).then_some(level.as_str()),
                );
            },
            cx,
        );
        cx.notify();
    }

    pub(in crate::workspace) fn ai_acp_model_options_for_agent(
        &self,
        agent_id: &str,
        cx: &App,
    ) -> Option<Vec<oxideterm_ai::AcpSessionConfigOption>> {
        if let Some(state) = self.active_ai_acp_session_state(agent_id, cx)
            && oxideterm_ai::acp_model_config_option(&state.config_options)
                .is_some_and(|option| !option.choices.is_empty())
        {
            return Some(state.config_options);
        }
        let conversation_id = self.ai_entity.read(cx).conversation_state()
            .active_conversation()
            .map(|conversation| conversation.id.as_str())?;
        self.ai_entity
            .read(cx)
            .acp_model_options(conversation_id, agent_id)
    }

    pub(in crate::workspace) fn ai_acp_model_discovery_is_pending(
        &self,
        agent_id: &str,
        cx: &App,
    ) -> bool {
        let Some(conversation_id) = self.ai_entity.read(cx).conversation_state()
            .active_conversation()
            .map(|conversation| conversation.id.as_str())
        else {
            return false;
        };
        self.ai_entity
            .read(cx)
            .acp_model_discovery_is_pending(conversation_id, agent_id)
    }

    pub(in crate::workspace) fn schedule_ai_acp_model_discovery(
        &mut self,
        agent_id: String,
        cx: &mut Context<Self>,
    ) {
        if self
            .ai_acp_model_options_for_agent(&agent_id, cx)
            .is_some()
        {
            return;
        }
        let Some(conversation_id) = self.ai_entity.read(cx).conversation_state()
            .active_conversation()
            .map(|conversation| conversation.id.clone())
        else {
            return;
        };
        if self
            .ai_entity
            .read(cx)
            .acp_model_discovery_is_pending(&conversation_id, &agent_id)
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
        let session_cwd = acp_session_cwd_from_agent(&agent);
        self.ai_entity.update(cx, |ai, _cx| {
            ai.request_acp_model_discovery(conversation_id, agent, session_cwd);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn ensure_ai_model_selector_mount_statuses(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let providers = self.ai_model_selector_providers(cx);
        let signature = ai_model_selector_status_signature(&providers);
        if self
            .ai_entity
            .read(cx)
            .model_selector_status_signature_matches(signature)
        {
            return;
        }
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_model_selector_status_signature(Some(signature));
        });
        // Mirrors Tauri ModelSelector's mount/provider-change checkAllKeys
        // effect: the trigger indicator starts probing before the user opens it.
        self.refresh_ai_model_selector_provider_statuses(cx);
    }

    pub(in crate::workspace) fn sync_ai_workspace_visibility(&mut self, cx: &mut Context<Self>) {
        // Inline AI can be mounted in a detached terminal window, so its own
        // open state is the stable cross-window visibility signal.
        let terminal_inline_surface = self.ai_entity.read(cx).terminal_inline_panel().open;
        let model_selector_surface = self.ai_sidebar_visible() || terminal_inline_surface;
        let main_settings_surface = self
            .active_tab(cx)
            .is_some_and(|tab| tab.kind == TabKind::Settings);
        let detached_settings_surface = self.tabs(cx).iter().any(|tab| {
            tab.kind == TabKind::Settings && self.tab_host.read(cx).is_outside_main_window(tab.id)
        });
        let settings_surface = !self.app_lock.locked
            && (main_settings_surface || detached_settings_surface)
            && self.settings_workspace.read(cx).route_snapshot().active_tab == SettingsTab::Ai;
        let visibility = AiWorkspaceVisibility {
            model_selector_surface: !self.app_lock.locked && model_selector_surface,
            settings_surface,
        };
        let changed = self
            .ai_entity
            .update(cx, |ai, _cx| ai.set_workspace_visibility(visibility));
        if changed && !visibility.model_selector_surface {
            // A later remount must re-run status checks even when provider
            // configuration stayed unchanged while the surface was hidden.
            self.ai_entity.update(cx, |ai, _cx| {
                ai.set_model_selector_status_signature(None);
            });
        }
        if visibility.model_selector_surface {
            self.ensure_ai_model_selector_mount_statuses(cx);
        }
    }

    pub(in crate::workspace) fn toggle_ai_model_selector(
        &mut self,
        scope: AiModelSelectorScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next_open = !self.ai_entity.read(cx).model_selector_is_open(scope);
        self.close_ai_sidebar_popovers(cx);
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_model_selector_open(scope, next_open);
        });
        if next_open {
            let providers = self.ai_model_selector_providers(cx);
            let mut active_acp_agent_id = None;
            if let Some(provider) = active_provider_view(
                &providers,
                self.ai_active_model_selector_provider_id().as_deref(),
            ) {
                active_acp_agent_id =
                    Self::ai_acp_agent_id_from_provider_id(&provider.id).map(str::to_string);
                self.ai_entity.update(cx, |ai, _cx| {
                    ai.expand_model_selector_provider(provider.id.clone());
                });
            }
            if let Some(agent_id) = active_acp_agent_id {
                self.schedule_ai_acp_model_discovery(agent_id, cx);
            }
            self.ai_entity.update(cx, |ai, _cx| {
                ai.blur_chat_input(false);
                ai.terminal_inline_panel_mut().prompt_focused = false;
            });
            self.refresh_ai_model_selector_provider_statuses(cx);
            window.focus(&self.focus_handle, cx);
        } else {
            self.close_ai_model_selector(cx);
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn ai_model_selector_visible_model_keys(
        &self,
        cx: &App,
    ) -> Vec<(String, String)> {
        let providers = self.ai_model_selector_providers(cx);
        let ai = self.ai_entity.read(cx);
        let model_ui = ai.model_ui();
        let searching = !model_ui.selector_search_query.trim().is_empty();
        // Tauri renders models as focusable dropdown items only for expanded
        // providers, while search mode expands matching providers. Keep the
        // keyboard target list identical to the rendered, selectable rows.
        model_selector_visible_provider_groups(&providers, &model_ui.selector_search_query)
            .into_iter()
            .filter(|group| {
                searching
                    || model_ui
                        .selector_expanded_providers
                        .contains(&group.provider.id)
            })
            .filter(|group| {
                self.ai_model_selector_has_key(&group.provider, cx)
                    && self.ai_model_selector_provider_is_online(&group.provider, cx)
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

    pub(in crate::workspace) fn move_ai_model_selector_highlight(
        &mut self,
        delta: isize,
        cx: &mut Context<Self>,
    ) {
        let rows = self.ai_model_selector_visible_model_keys(cx);
        self.ai_entity.update(cx, |ai, _cx| {
            ai.move_model_selector_highlight(&rows, delta);
        });
    }

    pub(in crate::workspace) fn set_ai_model_selector_highlight_edge(
        &mut self,
        last: bool,
        cx: &mut Context<Self>,
    ) {
        let rows = self.ai_model_selector_visible_model_keys(cx);
        // Home/End in Radix-style menu focus moves to the first/last selectable
        // model row, not to provider headers or disabled provider messages.
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_model_selector_highlight_edge(&rows, last);
        });
    }

    pub(in crate::workspace) fn select_highlighted_ai_model(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((provider_id, model)) = self
            .ai_entity
            .read(cx)
            .model_selector_highlight()
            .cloned()
        else {
            return false;
        };
        if !self
            .ai_model_selector_visible_model_keys(cx)
            .iter()
            .any(|row| row == &(provider_id.clone(), model.clone()))
        {
            self.ai_entity.update(cx, |ai, _cx| {
                ai.set_model_selector_highlight(None);
            });
            return false;
        }
        self.select_ai_model_from_selector(provider_id, model, cx);
        self.ai_entity.update(cx, |ai, _cx| {
            ai.set_model_selector_highlight(None);
        });
        true
    }

    pub(in crate::workspace) fn refresh_ai_model_selector_provider_statuses(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.ensure_ai_provider_key_statuses(cx);
        let providers = self.ai_model_selector_providers(cx);
        for provider in providers {
            if Self::ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
                self.ai_entity.update(cx, |ai, _cx| {
                    ai.set_provider_key_status(provider.id.clone(), true);
                });
                continue;
            }
            match resolve_model_selector_provider_probe(&provider) {
                ModelSelectorProviderProbe::Disabled => {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.set_provider_key_status(provider.id.clone(), false);
                        ai.set_selector_provider_online(provider.id.clone(), false);
                    });
                }
                ModelSelectorProviderProbe::StoredKey => {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.set_selector_provider_online(provider.id.clone(), true);
                    });
                }
                ModelSelectorProviderProbe::ImplicitKey { endpoint } => {
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.set_provider_key_status(provider.id.clone(), true);
                    });
                    if let Some(endpoint) = endpoint {
                        self.ai_entity.update(cx, |ai, _cx| {
                            ai.request_selector_provider_probe(provider, endpoint);
                        });
                    } else {
                        self.ai_entity.update(cx, |ai, _cx| {
                            ai.set_selector_provider_online(provider.id, true);
                        });
                    }
                }
            }
        }
    }

    pub(in crate::workspace) fn ai_model_selector_has_key(
        &self,
        provider: &AiProviderView,
        cx: &App,
    ) -> bool {
        if Self::ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
            return provider.enabled;
        }
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::Disabled => false,
            ModelSelectorProviderProbe::ImplicitKey { .. } => true,
            ModelSelectorProviderProbe::StoredKey => {
                !ai_provider_chat_requires_key(&provider.provider_type)
                    || self.ai_provider_has_key(&provider.id, cx)
            }
        }
    }

    pub(in crate::workspace) fn ai_model_selector_provider_is_online(
        &self,
        provider: &AiProviderView,
        cx: &App,
    ) -> bool {
        if Self::ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
            return self.ai_acp_provider_ready(&provider.id);
        }
        match resolve_model_selector_provider_probe(provider) {
            ModelSelectorProviderProbe::Disabled => false,
            ModelSelectorProviderProbe::StoredKey => true,
            ModelSelectorProviderProbe::ImplicitKey { .. } => {
                self.ai_entity.read(cx).selector_provider_is_online(&provider.id)
            }
        }
    }

    pub(in crate::workspace) fn refresh_ai_provider_from_selector(
        &mut self,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) {
        if !self.ai_model_selector_has_key(&provider, cx) {
            self.push_ai_settings_toast(
                self.i18n.t("ai.model_selector.no_key_warning"),
                TerminalNoticeVariant::Warning,
                cx,
            );
            cx.notify();
            return;
        }
        if !self.ai_model_selector_provider_is_online(&provider, cx) {
            self.push_ai_settings_toast(
                self.i18n.t("ai.model_selector.offline"),
                TerminalNoticeVariant::Warning,
                cx,
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
                .ai_acp_model_options_for_agent(&agent_id, cx)
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
            self.close_ai_model_selector(cx);
            cx.notify();
            return;
        }
        self.edit_settings(
            |settings| {
                settings.ai.active_backend = AiActiveBackend::Provider;
                ai_select_provider_model(
                    &mut settings.ai.active_provider_id,
                    &mut settings.ai.active_model,
                    &provider_id,
                    model.clone(),
                );
            },
            cx,
        );
        if previous_model.as_deref() != Some(model.as_str()) {
            self.update_ai_model_switch_warning(&provider_id, &model, cx);
        }
        self.close_ai_model_selector(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn ai_model_selector_providers(&self, cx: &App) -> Vec<AiProviderView> {
        let settings = self.settings_store.settings();
        let mut providers = ai_provider_views(&settings.ai.providers);
        providers.extend(
            settings
                .ai
                .acp_agents
                .iter()
                .map(|agent| self.ai_acp_agent_provider_view(agent, cx)),
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
        cx: &App,
    ) -> AiProviderView {
        let label = Self::ai_acp_agent_label(agent);
        let fallback_model = self.ai_acp_agent_model_fallback_label(&agent.id, cx);
        let models = self
            .ai_acp_model_options_for_agent(&agent.id, cx)
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
        cx: &App,
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
            if self.ai_acp_model_discovery_is_pending(agent_id, cx) {
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
        let Some(discovered_options) = self.ai_acp_model_options_for_agent(&agent_id, cx) else {
            return;
        };
        let conversation_id = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .active_conversation()
            .map(|conversation| conversation.id.clone());
        let sent_to_session = conversation_id.as_deref().is_some_and(|conversation_id| {
            self.acp_entity.update(cx, |entity, _cx| {
                entity.set_config_selection(
                    conversation_id,
                    oxideterm_ai::AcpSessionConfigSelection {
                        config_id: config_id.clone(),
                        value_id: value_id.clone(),
                    },
                )
            })
        });
        let accepted = sent_to_session
            || self.ai_entity.update(cx, |ai, _cx| {
                ai.set_active_acp_model_selection(
                    &agent_id,
                    discovered_options,
                    &config_id,
                    &value_id,
                )
            });
        if !accepted {
            return;
        }
        self.edit_settings(
            move |settings| {
                settings.ai.active_backend = AiActiveBackend::Acp;
                settings.ai.active_acp_agent_id = Some(agent_id);
            },
            cx,
        );
        self.close_ai_model_selector(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn select_ai_acp_config_from_selector(
        &mut self,
        agent_id: String,
        config_id: String,
        value_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(conversation_id) = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .active_conversation()
            .map(|conversation| conversation.id.clone())
        else {
            return;
        };
        let Some(discovered_options) = self
            .active_ai_acp_session_state(&agent_id, cx)
            .map(|state| state.config_options)
            .or_else(|| self.ai_acp_model_options_for_agent(&agent_id, cx))
        else {
            return;
        };
        let selection = oxideterm_ai::AcpSessionConfigSelection {
            config_id: config_id.clone(),
            value_id: value_id.clone(),
        };
        let sent_to_session = self.acp_entity.update(cx, |entity, _cx| {
            entity.set_config_selection(&conversation_id, selection)
        });
        let accepted = sent_to_session
            || self.ai_entity.update(cx, |ai, _cx| {
                ai.set_active_acp_config_selection(
                    &agent_id,
                    discovered_options,
                    &config_id,
                    &value_id,
                )
            });
        if !accepted {
            return;
        }
        cx.notify();
    }

    pub(in crate::workspace) fn select_ai_acp_mode_from_selector(
        &mut self,
        agent_id: String,
        mode_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(conversation_id) = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .active_conversation()
            .map(|conversation| conversation.id.clone())
        else {
            return;
        };
        let sent_to_session = self.acp_entity.update(cx, |entity, _cx| {
            entity.set_mode(&conversation_id, mode_id.clone())
        });
        let accepted = sent_to_session
            || self
                .ai_entity
                .update(cx, |ai, _cx| ai.set_active_acp_mode(&agent_id, &mode_id));
        if !accepted {
            return;
        }
        cx.notify();
    }

    pub(in crate::workspace) fn update_ai_model_switch_warning(
        &mut self,
        provider_id: &str,
        model: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(_conversation) = self.ai_entity.read(cx).conversation_state().active_conversation() else {
            return;
        };
        let total_tokens = self.ai_context_token_breakdown(cx).total;
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
            self.ai_entity.update(cx, |ai, _cx| {
                ai.set_model_switch_warning(Some(percentage.round() as usize));
            });
        }
    }
}

pub(in crate::workspace) fn ai_conversation_reasoning_effort<'a>(
    conversation: &'a AiConversation,
    provider_id: &str,
    model: &str,
) -> Option<&'a str> {
    let value = conversation
        .session_metadata
        .as_ref()?
        .get(AI_REASONING_EFFORT_SESSION_METADATA_KEY)?;
    // Read the first implementation's scalar value for backward compatibility.
    value.as_str().or_else(|| value.get(provider_id)?.get(model)?.as_str())
}

pub(in crate::workspace) fn store_ai_reasoning_level_in_conversation(
    conversation: &mut AiConversation,
    provider_id: &str,
    model: &str,
    level: AiReasoningLevel,
) {
    let metadata = conversation
        .session_metadata
        .get_or_insert_with(|| serde_json::json!({}));
    if !metadata.is_object() {
        *metadata = serde_json::json!({});
    }
    let object = metadata
        .as_object_mut()
        .expect("reasoning session metadata must be an object");
    let reasoning = object
        .entry(AI_REASONING_EFFORT_SESSION_METADATA_KEY)
        .or_insert_with(|| serde_json::json!({}));
    if !reasoning.is_object() {
        *reasoning = serde_json::json!({});
    }
    let providers = reasoning
        .as_object_mut()
        .expect("reasoning session metadata must be an object");
    let models = providers
        .entry(provider_id.to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !models.is_object() {
        *models = serde_json::json!({});
    }
    let models = models
        .as_object_mut()
        .expect("reasoning provider metadata must be an object");
    if level == AiReasoningLevel::Auto {
        models.remove(model);
    } else {
        models.insert(model.to_string(), serde_json::json!(level.as_str()));
    }
}

pub(in crate::workspace) fn store_ai_acp_model_selection_in_conversation(
    conversation: &mut AiConversation,
    agent_id: &str,
    discovered_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
    config_id: &str,
    value_id: &str,
) -> bool {
    store_ai_acp_config_selection_in_conversation(
        conversation,
        agent_id,
        discovered_options,
        config_id,
        value_id,
        true,
    )
}

pub(in crate::workspace) fn store_ai_acp_config_selection_in_conversation(
    conversation: &mut AiConversation,
    agent_id: &str,
    discovered_options: Vec<oxideterm_ai::AcpSessionConfigOption>,
    config_id: &str,
    value_id: &str,
    is_model_selection: bool,
) -> bool {
    let mut state = ai_acp_session_state(conversation)
        .filter(|state| state.agent_id == agent_id)
        .unwrap_or_else(|| AiAcpSessionState {
            agent_id: agent_id.to_string(),
            session_id: String::new(),
            metadata: None,
            config_options: discovered_options,
            model_selection: None,
            config_selections: Vec::new(),
            current_mode_id: None,
            available_modes: Vec::new(),
            available_commands: Vec::new(),
            plan: None,
            usage: None,
            title: None,
            handoff_cursor: None,
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
    let selection = oxideterm_ai::AcpSessionConfigSelection {
        config_id: config_id.to_string(),
        value_id: value_id.to_string(),
    };
    if is_model_selection {
        state.model_selection = Some(selection.clone());
    }
    state
        .config_selections
        .retain(|existing| existing.config_id != config_id);
    state.config_selections.push(selection);
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
            turn_count: 0,
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

    #[test]
    fn reasoning_level_is_scoped_to_the_conversation_without_erasing_other_metadata() {
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
            session_metadata: Some(serde_json::json!({ "other": true })),
            messages_loaded: true,
            turn_count: 0,
        };

        store_ai_reasoning_level_in_conversation(
            &mut conversation,
            "openai",
            "gpt-5.6-sol",
            AiReasoningLevel::High,
        );
        assert_eq!(
            ai_conversation_reasoning_effort(&conversation, "openai", "gpt-5.6-sol"),
            Some("high")
        );
        assert_eq!(
            conversation.session_metadata.as_ref().and_then(|value| {
                value.get("other").and_then(serde_json::Value::as_bool)
            }),
            Some(true)
        );

        store_ai_reasoning_level_in_conversation(
            &mut conversation,
            "openai",
            "gpt-5.6-sol",
            AiReasoningLevel::Auto,
        );
        assert_eq!(
            ai_conversation_reasoning_effort(&conversation, "openai", "gpt-5.6-sol"),
            None
        );
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
