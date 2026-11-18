use oxideterm_ai::tool_loop::acp_session_cwd_from_agent;
use oxideterm_gpui_ai::model_picker::{active_model_picker_provider, ai_acp_agent_id_from_provider_id};

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
        self.ai.state.update(cx, |ai, _cx| {
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

    pub(in crate::workspace) fn schedule_ai_acp_model_discovery(
        &mut self,
        agent_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.ai.state.read(cx).picker_acp_options(&agent_id).is_some() {
            return;
        }
        let Some(conversation_id) = self
            .ai.state
            .read(cx)
            .conversation_state()
            .active_conversation()
            .map(|conversation| conversation.id.clone())
        else {
            return;
        };
        if self
            .ai.state
            .read(cx)
            .acp_model_discovery_is_pending(&conversation_id, &agent_id)
        {
            return;
        }
        let Some(mut agent) = self
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
        if agent.plugin_id.as_deref() != Some("com.oxideterm.acp.codex")
            && !oxideterm_ai::acp_model_report_is_available_during_session_start(&agent.args)
        {
            return;
        }
        if self.resolve_ai_acp_plugin(&mut agent, cx).is_err() {
            self.show_acp_plugin_requirement(&agent, cx);
            return;
        }
        let session_cwd = acp_session_cwd_from_agent(&agent);
        self.ai.state.update(cx, |ai, _cx| {
            ai.request_acp_model_discovery(conversation_id, agent, session_cwd);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn ensure_ai_model_selector_mount_statuses(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let providers = self.ai_model_selector_providers(cx);
        if !self.ai.state.update(cx, |ai, _| ai.picker_status_changed(&providers)) {
            return;
        }
        // Mirrors Tauri ModelSelector's mount/provider-change checkAllKeys
        // effect: the trigger indicator starts probing before the user opens it.
        self.refresh_ai_model_selector_provider_statuses(cx);
    }

    pub(in crate::workspace) fn sync_ai_workspace_visibility(&mut self, cx: &mut Context<Self>) {
        // Inline AI can be mounted in a detached terminal window, so its own
        // open state is the stable cross-window visibility signal.
        let terminal_inline_surface = self.ai.state.read(cx).terminal_inline_panel().open;
        let model_selector_surface = self.ai_sidebar_visible() || terminal_inline_surface;
        let settings_visible = self.tabs(cx).iter().any(|tab| {
            tab.kind == TabKind::Settings && self.tab_host.read(cx).surface_is_visible(tab.id)
        });
        let settings_surface = !self.app_lock.locked
            && settings_visible
            && self.settings_workspace.read(cx).route_snapshot().active_tab == SettingsTab::Ai;
        let visibility = AiWorkspaceVisibility {
            model_selector_surface: !self.app_lock.locked && model_selector_surface,
            settings_surface,
        };
        let changed = self
            .ai.state
            .update(cx, |ai, _cx| ai.set_workspace_visibility(visibility));
        if changed && !visibility.model_selector_surface {
            // A later remount must re-run status checks even when provider
            // configuration stayed unchanged while the surface was hidden.
            self.ai.state.update(cx, |ai, _cx| {
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
        let next_open = !self.ai.state.read(cx).model_selector_is_open(scope);
        self.close_ai_sidebar_popovers(cx);
        self.ai.state.update(cx, |ai, _cx| {
            ai.set_model_selector_open(scope, next_open);
        });
        if next_open {
            let providers = self.ai_model_selector_providers(cx);
            let mut active_acp_agent_id = None;
            if let Some(provider) = active_provider_view(
                &providers,
                active_model_picker_provider(&self.settings_store.settings().ai).as_deref(),
            ) {
                active_acp_agent_id =
                    ai_acp_agent_id_from_provider_id(&provider.id).map(str::to_string);
                self.ai.state.update(cx, |ai, _cx| {
                    ai.expand_model_selector_provider(provider.id.clone());
                });
            }
            if let Some(agent_id) = active_acp_agent_id {
                self.schedule_ai_acp_model_discovery(agent_id, cx);
            }
            self.ai.state.update(cx, |ai, _cx| {
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

    fn ai_model_selector_visible_model_keys(&self, cx: &App) -> Vec<(String, String)> {
        self.ai.state.read(cx).picker_visible_model_keys(
            &self.settings_store.settings().ai,
            &self.i18n,
            |id| self.ai_picker_plugin_available(id, cx),
        )
    }

    pub(in crate::workspace) fn ai_picker_plugin_available(&self, id: &str, cx: &App) -> bool {
        self.plugin_entity.read(cx).acp_agents().iter().any(|plugin| plugin.plugin_id == id)
    }

    pub(in crate::workspace) fn move_ai_model_selector_highlight(
        &mut self,
        delta: isize,
        cx: &mut Context<Self>,
    ) {
        let rows = self.ai_model_selector_visible_model_keys(cx);
        self.ai.state.update(cx, |ai, _cx| {
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
        self.ai.state.update(cx, |ai, _cx| {
            ai.set_model_selector_highlight_edge(&rows, last);
        });
    }

    pub(in crate::workspace) fn select_highlighted_ai_model(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((provider_id, model)) =
            self.ai.state.read(cx).model_selector_highlight().cloned()
        else {
            return false;
        };
        if !self
            .ai_model_selector_visible_model_keys(cx)
            .iter()
            .any(|row| row == &(provider_id.clone(), model.clone()))
        {
            self.ai.state.update(cx, |ai, _cx| {
                ai.set_model_selector_highlight(None);
            });
            return false;
        }
        self.select_ai_model_from_selector(provider_id, model, cx);
        self.ai.state.update(cx, |ai, _cx| {
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
        let settings = &self.settings_store.settings().ai;
        let agents = self.ai.state.update(cx, |ai, _| ai.refresh_picker_statuses(providers, settings));
        for agent_id in agents {
            self.test_ai_acp_agent(agent_id, cx);
        }
    }

    pub(in crate::workspace) fn refresh_ai_provider_from_selector(
        &mut self,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) {
        if !self.ai.state.read(cx).picker_has_key(&provider) {
            self.push_ai_settings_toast(
                self.i18n.t("ai.model_selector.no_key_warning"),
                TerminalNoticeVariant::Warning,
                cx,
            );
            cx.notify();
            return;
        }
        if !self.ai.state.read(cx).picker_online(&provider, &self.settings_store.settings().ai, |id| self.ai_picker_plugin_available(id, cx)) {
            self.push_ai_settings_toast(
                self.i18n.t("ai.model_selector.offline"),
                TerminalNoticeVariant::Warning,
                cx,
            );
            cx.notify();
            return;
        }
        if ai_acp_agent_id_from_provider_id(&provider.id).is_some() {
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
            ai_acp_agent_id_from_provider_id(&provider_id).map(str::to_string)
        {
            let session_model_selection = self
                .ai.state.read(cx).picker_acp_options(&agent_id)
                .and_then(|options| {
                    let option = oxideterm_ai::acp_model_config_option(&options)?;
                    let choice = option.choices.iter().find(|choice| choice.label == model)?;
                    Some((option.config_id.clone(), choice.value_id.clone()))
                });
            if let Some((config_id, value_id)) = session_model_selection {
                self.select_ai_acp_model_from_selector(agent_id, config_id, value_id, cx);
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
        if !self.ai.state.read(cx).chat_ui().images.is_empty() {
            self.check_ai_image_input(cx);
        }
        self.resume_ai_image_request_for_model(cx);
        self.close_ai_model_selector(cx);
        cx.notify();
    }

    fn ai_model_selector_providers(&self, cx: &App) -> Vec<AiProviderView> {
        self.ai.state.read(cx).picker_providers(&self.settings_store.settings().ai, &self.i18n)
    }

    pub(in crate::workspace) fn select_ai_acp_model_from_selector(
        &mut self,
        agent_id: String,
        config_id: String,
        value_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self.ai.conversations.select_acp_picker_config(&agent_id, config_id, value_id, true, cx) {
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
        &mut self, agent_id: String, config_id: String, value_id: String, cx: &mut Context<Self>,
    ) {
        self.ai.conversations.select_acp_picker_config(&agent_id, config_id, value_id, false, cx);
    }

    pub(in crate::workspace) fn select_ai_acp_mode_from_selector(
        &mut self, agent_id: String, mode_id: String, cx: &mut Context<Self>,
    ) {
        self.ai.conversations.select_acp_picker_mode(&agent_id, mode_id, cx);
    }

    pub(in crate::workspace) fn update_ai_model_switch_warning(
        &mut self,
        provider_id: &str,
        model: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(_conversation) = self
            .ai.state
            .read(cx)
            .conversation_state()
            .active_conversation()
        else {
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
            self.ai.state.update(cx, |ai, _cx| {
                ai.set_model_switch_warning(Some(percentage.round() as usize));
            });
        }
    }
}

use oxideterm_gpui_ai::state::ai_conversation_reasoning_effort;
