use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn refresh_ai_provider_models(
        &mut self,
        index: usize,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.request_model_refresh(index, provider);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn handle_ai_workspace_event(
        &mut self,
        event: &ai_state::AiWorkspaceEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ai_state::AiWorkspaceEvent::AcpAgentProbeDeliveryReady => {
                let intents = self
                    .ai_entity
                    .update(cx, |ai, _cx| ai.take_acp_agent_probe_intents());
                self.edit_settings(
                    move |settings| {
                        for intent in intents {
                            if let Some(agent) = settings
                                .ai
                                .acp_agents
                                .iter_mut()
                                .find(|agent| agent.id == intent.agent_id)
                            {
                                agent.auth.status = intent.auth_status;
                                agent.status.state = intent.runtime_state;
                                agent.status.last_error_kind = intent.last_error_kind;
                            }
                        }
                    },
                    cx,
                );
            }
            ai_state::AiWorkspaceEvent::AcpModelDiscoveryDeliveryReady => {
                let intents = self
                    .ai_entity
                    .update(cx, |ai, _cx| ai.take_acp_model_discovery_intents());
                for intent in intents {
                    let conversation_exists = self
                        .ai
                        .chat
                        .conversation_state
                        .conversations
                        .iter()
                        .any(|conversation| conversation.id == intent.conversation_id);
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.apply_acp_model_discovery(intent, conversation_exists);
                    });
                }
                cx.notify();
            }
            ai_state::AiWorkspaceEvent::KnowledgeReindexDeliveryReady => {
                let intents = self
                    .ai_entity
                    .update(cx, |ai, _cx| ai.take_knowledge_reindex_intents());
                for intent in intents {
                    match intent {
                        ai_state::AiKnowledgeReindexIntent::Finished { failed } => {
                            if failed {
                                self.push_ai_settings_toast(
                                    self.i18n.t("settings_view.knowledge.error_reindex"),
                                    TerminalNoticeVariant::Error,
                                );
                            } else {
                                self.settings_page.clear_knowledge_error();
                            }
                        }
                    }
                }
                cx.notify();
            }
            ai_state::AiWorkspaceEvent::ModelRefreshDeliveryReady => {
                let intents = self
                    .ai_entity
                    .update(cx, |ai, _cx| ai.take_model_refresh_intents());
                for intent in intents {
                    match intent {
                        ai_state::AiModelRefreshIntent::Updated {
                            index,
                            provider_id,
                            refresh,
                        } => {
                            self.edit_settings(
                                |settings| {
                                    ai_apply_provider_model_refresh(
                                        &mut settings.ai.providers,
                                        &mut settings.ai.model_context_windows,
                                        index,
                                        &provider_id,
                                        refresh,
                                    );
                                },
                                cx,
                            );
                        }
                        ai_state::AiModelRefreshIntent::MissingApiKey { provider_id } => {
                            self.ai_entity.update(cx, |ai, _cx| {
                                ai.set_provider_key_status(provider_id, false);
                            });
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.ai.api_key_missing"),
                                TerminalNoticeVariant::Warning,
                            );
                        }
                        ai_state::AiModelRefreshIntent::Failed => {
                            let safe_error =
                                self.i18n.t("settings_view.ai.acp_agent_error_unknown");
                            self.push_ai_settings_toast(
                                self.ai_i18n_error("settings_view.ai.refresh_failed", &safe_error),
                                TerminalNoticeVariant::Error,
                            );
                        }
                    }
                }
                cx.notify();
            }
            ai_state::AiWorkspaceEvent::ProviderKeyStatusChanged
            | ai_state::AiWorkspaceEvent::SelectorProviderStatusChanged => cx.notify(),
        }
    }

    pub(in crate::workspace) fn push_ai_settings_toast(
        &mut self,
        title: String,
        variant: TerminalNoticeVariant,
    ) {
        let id = self.next_workspace_toast_id();
        self.workspace_toasts.push(WorkspaceToast {
            id,
            notice: TerminalNotice {
                title,
                description: None,
                status_text: None,
                progress: None,
                variant,
            },
            expires_at: Instant::now() + Duration::from_secs(4),
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
        });
    }
}
