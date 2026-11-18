use super::AiPickerSelection;
use crate::{
    conversation::actions::AiChatActions,
    state::{AiChatPopover, AiModelSelectorScope},
};
use gpui::{App, Window};
use oxideterm_ai::{
    AiProviderView, AiReasoningLevel, select_provider_model as ai_select_provider_model,
    tool_loop::acp_session_cwd_from_agent,
};
use oxideterm_settings::{AcpAgentConfig, AiActiveBackend, PersistedSettings};
use oxideterm_settings_model::set_ai_model_reasoning_override;

/// Owns picker transitions while the window supplies persistence, probes and focus.
pub trait AiModelPickerHost: AiChatActions {
    fn write_picker_settings(
        &mut self,
        edit: impl FnOnce(&mut PersistedSettings),
        cx: &mut Self::Cx<'_>,
    );
    fn focus_picker_window(&self, window: &mut Window, cx: &mut Self::Cx<'_>);
    fn apply_acp_picker_config(
        &mut self,
        agent: &str,
        config: String,
        value: String,
        model: bool,
        cx: &mut Self::Cx<'_>,
    ) -> bool;
    fn ai_context_token_total(&self, cx: &App) -> usize;
    fn resolve_ai_acp_plugin(&self, agent: &mut AcpAgentConfig, cx: &App) -> Result<(), String>;
    fn show_acp_plugin_requirement(&self, agent: &AcpAgentConfig, cx: &mut Self::Cx<'_>) -> bool;
    fn ai_picker_plugin_available(&self, id: &str, cx: &App) -> bool;
    fn ensure_ai_provider_key_statuses(&mut self, cx: &mut Self::Cx<'_>);
    fn test_ai_acp_agent(&mut self, id: String, cx: &mut Self::Cx<'_>);
    fn refresh_ai_provider_models(
        &mut self,
        index: usize,
        provider: AiProviderView,
        cx: &mut Self::Cx<'_>,
    );
    fn close_ai_model_selector(&mut self, cx: &mut App);
    fn check_ai_image_input(&mut self, cx: &mut Self::Cx<'_>) -> bool;
    fn resume_ai_image_request_for_model(&mut self, cx: &mut Self::Cx<'_>);

    fn handle_picker_action(
        &mut self,
        request: super::AiModelPickerRequest,
        cx: &mut Self::Cx<'_>,
    ) -> Option<super::AiModelPickerRequest> {
        use super::AiModelPickerRequest::*;
        match request {
            Refresh(provider) => self.refresh_ai_provider_from_selector(provider, cx),
            Discover(id) => self.schedule_ai_acp_model_discovery(id, cx),
            Select(provider, model) => self.select_ai_model_from_selector(provider, model, cx),
            SelectAcpModel(agent, config, value) => {
                self.select_ai_acp_model_from_selector(agent, config, value, cx)
            }
            other => return Some(other),
        }
        None
    }

    fn select_ai_reasoning_level(
        &mut self,
        provider_id: String,
        provider_type: String,
        model: String,
        level: AiReasoningLevel,
        cx: &mut Self::Cx<'_>,
    ) {
        let level = oxideterm_ai::normalize_reasoning_level_for_model(
            &provider_type,
            &model,
            level.as_str(),
        );
        self.chat_state().update(&mut **cx, |ai, _cx| {
            ai.set_chat_popover_open(AiChatPopover::Reasoning, false);
            ai.set_active_reasoning_level(&provider_id, &model, level);
        });
        self.write_picker_settings(
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
        self.redraw_chat_chrome(cx);
    }

    fn schedule_ai_acp_model_discovery(&mut self, agent_id: String, cx: &mut Self::Cx<'_>) {
        let Some((conversation_id, mut agent)) = self
            .chat_state()
            .read(cx)
            .prepare_picker_acp_discovery(&agent_id, self.chat_settings())
        else {
            return;
        };
        if self.resolve_ai_acp_plugin(&mut agent, cx).is_err() {
            self.show_acp_plugin_requirement(&agent, cx);
            return;
        }
        let session_cwd = acp_session_cwd_from_agent(&agent);
        self.chat_state().update(&mut **cx, |ai, _cx| {
            ai.request_acp_model_discovery(conversation_id, agent, session_cwd);
        });
        self.redraw_chat_chrome(cx);
    }

    fn ensure_ai_model_selector_mount_statuses(&mut self, cx: &mut Self::Cx<'_>) {
        let providers = self.ai_model_selector_providers(cx);
        if !self
            .chat_state()
            .update(&mut **cx, |ai, _| ai.picker_status_changed(&providers))
        {
            return;
        }
        // Mirrors Tauri ModelSelector's mount/provider-change checkAllKeys
        // effect: the trigger indicator starts probing before the user opens it.
        self.refresh_ai_model_selector_provider_statuses(cx);
    }

    fn toggle_ai_model_selector(
        &mut self,
        scope: AiModelSelectorScope,
        window: &mut Window,
        cx: &mut Self::Cx<'_>,
    ) {
        let opened = self.chat_state().update(&mut **cx, |ai, cx| {
            ai.toggle_picker(scope, self.chat_settings(), self.chat_i18n(), cx)
        });
        self.clear_chat_composition();
        if let Some(agent) = opened {
            if let Some(agent_id) = agent {
                self.schedule_ai_acp_model_discovery(agent_id, cx);
            }
            self.refresh_ai_model_selector_provider_statuses(cx);
            self.focus_picker_window(window, cx);
        }
        self.redraw_chat_chrome(cx);
    }

    fn ai_model_selector_visible_model_keys(&self, cx: &App) -> Vec<(String, String)> {
        self.chat_state().read(cx).picker_visible_model_keys(
            self.chat_settings(),
            self.chat_i18n(),
            |id| self.ai_picker_plugin_available(id, cx),
        )
    }

    fn move_ai_model_selector_highlight(&mut self, delta: isize, cx: &mut Self::Cx<'_>) {
        let rows = self.ai_model_selector_visible_model_keys(cx);
        self.chat_state().update(&mut **cx, |ai, _cx| {
            ai.move_model_selector_highlight(&rows, delta);
        });
    }

    fn set_ai_model_selector_highlight_edge(&mut self, last: bool, cx: &mut Self::Cx<'_>) {
        let rows = self.ai_model_selector_visible_model_keys(cx);
        // Home/End in Radix-style menu focus moves to the first/last selectable
        // model row, not to provider headers or disabled provider messages.
        self.chat_state().update(&mut **cx, |ai, _cx| {
            ai.set_model_selector_highlight_edge(&rows, last);
        });
    }

    fn select_highlighted_ai_model(&mut self, cx: &mut Self::Cx<'_>) -> bool {
        let rows = self.ai_model_selector_visible_model_keys(cx);
        let Some((provider_id, model)) = self
            .chat_state()
            .update(&mut **cx, |ai, _| ai.validated_picker_highlight(&rows))
        else {
            return false;
        };
        self.select_ai_model_from_selector(provider_id, model, cx);
        self.chat_state().update(&mut **cx, |ai, _cx| {
            ai.set_model_selector_highlight(None);
        });
        true
    }

    fn refresh_ai_model_selector_provider_statuses(&mut self, cx: &mut Self::Cx<'_>) {
        self.ensure_ai_provider_key_statuses(cx);
        let providers = self.ai_model_selector_providers(cx);
        let settings = self.chat_settings();
        let agents = self.chat_state().update(&mut **cx, |ai, _| {
            ai.refresh_picker_statuses(providers, settings)
        });
        for agent_id in agents {
            self.test_ai_acp_agent(agent_id, cx);
        }
    }

    fn refresh_ai_provider_from_selector(&mut self, provider: AiProviderView, cx: &mut Self::Cx<'_>) {
        match self.chat_state().read(cx).picker_refresh_index(
            &provider,
            self.chat_settings(),
            |id| self.ai_picker_plugin_available(id, cx),
        ) {
            Ok(Some(index)) => self.refresh_ai_provider_models(index, provider, cx),
            Ok(None) => {}
            Err(key) => {
                self.chat_notice(self.chat_i18n().t(key), false, cx);
            }
        }
    }

    fn select_ai_model_from_selector(
        &mut self,
        provider_id: String,
        model: String,
        cx: &mut Self::Cx<'_>,
    ) {
        let previous_model = self.chat_settings().active_model.clone();
        match self
            .chat_state()
            .read(cx)
            .plan_picker_selection(&provider_id, &model)
        {
            AiPickerSelection::AcpSession {
                agent_id,
                config_id,
                value_id,
            } => {
                self.select_ai_acp_model_from_selector(agent_id, config_id, value_id, cx);
                return;
            }
            AiPickerSelection::AcpAgent(agent_id) => {
                self.write_picker_settings(
                    move |settings| {
                        settings.ai.active_backend = AiActiveBackend::Acp;
                        settings.ai.active_acp_agent_id = Some(agent_id);
                    },
                    cx,
                );
                self.close_ai_model_selector(cx);
                self.redraw_chat_chrome(cx);
                return;
            }
            AiPickerSelection::Provider => {}
        }
        self.write_picker_settings(
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
        if !self.chat_state().read(cx).chat_ui().images.is_empty() {
            self.check_ai_image_input(cx);
        }
        self.resume_ai_image_request_for_model(cx);
        self.close_ai_model_selector(cx);
        self.redraw_chat_chrome(cx);
    }

    fn ai_model_selector_providers(&self, cx: &App) -> Vec<AiProviderView> {
        self.chat_state()
            .read(cx)
            .picker_providers(self.chat_settings(), self.chat_i18n())
    }

    fn select_ai_acp_model_from_selector(
        &mut self,
        agent_id: String,
        config_id: String,
        value_id: String,
        cx: &mut Self::Cx<'_>,
    ) {
        if !self.apply_acp_picker_config(&agent_id, config_id, value_id, true, cx) {
            return;
        }
        self.write_picker_settings(
            move |settings| {
                settings.ai.active_backend = AiActiveBackend::Acp;
                settings.ai.active_acp_agent_id = Some(agent_id);
            },
            cx,
        );
        self.close_ai_model_selector(cx);
        self.redraw_chat_chrome(cx);
    }

    fn update_ai_model_switch_warning(
        &mut self,
        provider_id: &str,
        model: &str,
        cx: &mut Self::Cx<'_>,
    ) {
        if self
            .chat_state()
            .read(cx)
            .conversation_state()
            .active_conversation()
            .is_none()
        {
            return;
        }
        let total_tokens = self.ai_context_token_total(cx);
        let settings = self.chat_settings();
        self.chat_state().update(&mut **cx, |ai, _| {
            ai.update_model_switch_warning(settings, provider_id, model, total_tokens)
        });
    }
}
