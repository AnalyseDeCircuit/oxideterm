use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn refresh_ai_provider_models(
        &mut self,
        index: usize,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) {
        self.ai.state.update(cx, |ai, _cx| {
            ai.request_model_refresh(index, provider);
        });
        // The refresh button's listener already holds the provider view.
        App::notify(cx, self.settings_views.ai_providers.entity_id());
    }

    pub(in crate::workspace) fn handle_ai_workspace_event(
        &mut self,
        event: &ai_state::AiWorkspaceEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ai_state::AiWorkspaceEvent::AcpAgentProbeDeliveryReady => {
                let intents = self
                    .ai
                    .state
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
                self.settings_views.notify_acp(cx);
            }
            ai_state::AiWorkspaceEvent::AcpModelDiscoveryDeliveryReady => {
                let intents = self
                    .ai
                    .state
                    .update(cx, |ai, _cx| ai.take_acp_model_discovery_intents());
                for intent in intents {
                    let conversation_exists = self
                        .ai
                        .state
                        .read(cx)
                        .conversation_state()
                        .conversations
                        .iter()
                        .any(|conversation| conversation.id == intent.conversation_id);
                    self.ai.state.update(cx, |ai, _cx| {
                        ai.apply_acp_model_discovery(intent, conversation_exists);
                    });
                }
                cx.notify();
            }
            ai_state::AiWorkspaceEvent::ChatStreamDeliveryReady => {
                let request = self
                    .ai
                    .state
                    .update(cx, |ai, cx| ai.take_chat_window_request(cx));
                if let Some((ticket, request)) = request {
                    self.apply_ai_chat_window_request(request, window, cx);
                    self.ai
                        .state
                        .update(cx, |ai, cx| ai.acknowledge_chat_window_request(ticket, cx));
                }
            }
            ai_state::AiWorkspaceEvent::CompactionDeliveryReady => {
                let deliveries = self
                    .ai
                    .state
                    .update(cx, |ai, _cx| ai.take_compaction_deliveries());
                self.apply_ai_compaction_deliveries(deliveries, cx);
                cx.notify();
            }
            ai_state::AiWorkspaceEvent::CompactionStateChanged => cx.notify(),
            ai_state::AiWorkspaceEvent::CredentialOperationReady => {
                let intents = self
                    .ai
                    .state
                    .update(cx, |ai, _cx| ai.take_credential_intents());
                for intent in intents {
                    match intent {
                        ai_state::AiCredentialIntent::ProviderKeyStored { index, provider_id } => {
                            if let Some(provider) = self
                                .settings_store
                                .settings()
                                .ai
                                .providers
                                .get(index)
                                .and_then(ai_provider_view)
                                .filter(|provider| provider.id == provider_id)
                            {
                                self.refresh_ai_provider_models(index, provider, cx);
                            }
                        }
                        ai_state::AiCredentialIntent::ProviderKeyRemoved => {}
                        ai_state::AiCredentialIntent::McpServerReady { config } => {
                            self.settings_workspace
                                .update(cx, |state, cx| state.complete_ai_mcp_submission(None, cx));
                            self.edit_settings(
                                move |settings| settings.ai.mcp_servers.push(config),
                                cx,
                            );
                        }
                        ai_state::AiCredentialIntent::McpServerSaveFailed { draft } => {
                            self.settings_workspace.update(cx, |state, cx| {
                                state.complete_ai_mcp_submission(Some(draft), cx)
                            });
                            let safe_error =
                                self.i18n.t("settings_view.ai.acp_agent_error_unknown");
                            self.push_ai_settings_toast(
                                self.ai_i18n_error("settings_view.ai.save_failed", &safe_error),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                        ai_state::AiCredentialIntent::McpServerRemoved { server_id } => {
                            self.edit_settings(
                                move |settings| {
                                    settings.ai.mcp_servers.retain(|value| {
                                        value.get("id").and_then(serde_json::Value::as_str)
                                            != Some(server_id.as_str())
                                    });
                                },
                                cx,
                            );
                        }
                        ai_state::AiCredentialIntent::Failed(failure) => {
                            let message_key = match failure {
                                ai_state::AiCredentialFailure::SaveProviderKey => {
                                    "settings_view.ai.save_failed"
                                }
                                ai_state::AiCredentialFailure::RemoveProviderKey => {
                                    "settings_view.ai.remove_failed"
                                }
                            };
                            // Keychain errors can include local account details.
                            // Only a localized stable category reaches the toast.
                            let safe_error =
                                self.i18n.t("settings_view.ai.acp_agent_error_unknown");
                            self.push_ai_settings_toast(
                                self.ai_i18n_error(message_key, &safe_error),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            }
            ai_state::AiWorkspaceEvent::McpRuntimeChanged => {
                self.settings_views.ai_mcp.update(cx, |_, cx| cx.notify());
            }
            ai_state::AiWorkspaceEvent::ModelRefreshDeliveryReady => {
                let intents = self
                    .ai
                    .state
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
                            self.ai.state.update(cx, |ai, _cx| {
                                ai.set_provider_key_status(provider_id, false);
                            });
                            self.push_ai_settings_toast(
                                self.i18n.t("settings_view.ai.api_key_missing"),
                                TerminalNoticeVariant::Warning,
                                cx,
                            );
                        }
                        ai_state::AiModelRefreshIntent::Failed { message } => {
                            self.push_ai_settings_toast(
                                self.ai_i18n_error("settings_view.ai.refresh_failed", &message),
                                TerminalNoticeVariant::Error,
                                cx,
                            );
                        }
                    }
                }
                self.settings_views
                    .ai_providers
                    .update(cx, |_, cx| cx.notify());
            }
            ai_state::AiWorkspaceEvent::ProviderKeyStatusChanged => {
                self.settings_views
                    .ai_providers
                    .update(cx, |_, cx| cx.notify());
                self.settings_views.surface.update(cx, |_, cx| cx.notify());
                App::notify(cx, self.ai.transcript.entity_id());
            }
            ai_state::AiWorkspaceEvent::SelectorProviderStatusChanged => cx.notify(),
        }
    }

    pub(in crate::workspace) fn push_ai_settings_toast(
        &mut self,
        title: String,
        variant: TerminalNoticeVariant,
        cx: &App,
    ) {
        self.push_workspace_notice(
            TerminalNotice {
                title,
                description: None,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{
        session_manager::tests::isolated_workspace_fixture, window_shell::WorkspaceWindowShell,
    };
    use oxideterm_settings_model::settings_section_list_item_index;

    #[gpui::test]
    fn provider_settings_refresh_button_starts_model_refresh(cx: &mut gpui::TestAppContext) {
        if isolated_workspace_fixture(cx, "OXIDETERM_PROVIDER_REFRESH_TEST_DIR").is_none() {
            return;
        }
        // Leave the loopback response pending so the loading state is observable after the click.
        let server = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut store = SettingsStore::load_default().unwrap();
        let settings = store.settings_mut();
        settings.onboarding_completed = true;
        settings.ssh_config.auto_load_hosts = false;
        settings.ai.enabled = true;
        settings.ai.active_provider_id = Some("local-test".into());
        settings.ai.providers = vec![serde_json::json!({
            "id": "local-test", "type": "ollama", "name": "Local Test",
            "enabled": true, "baseUrl": format!("http://{}", server.local_addr().unwrap())
        })];
        store.save().unwrap();
        crate::migration_snapshot::acknowledge_pre_2_0_migration_notice(&default_settings_path())
            .unwrap();
        cx.executor().allow_parking();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.simulate_resize(gpui::size(px(1400.), px(1000.)));
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_ai_settings(window, cx);
            workspace.settings_workspace.update(cx, |settings, cx| {
                settings.set_ai_page(oxideterm_settings_model::AiSettingsPage::Providers, cx);
            });
            workspace.sync_settings_section_list_state(cx);
            let item_ix = settings_section_list_item_index(
                SettingsTab::Ai,
                SettingsSectionId::AiProvidersSettings,
                workspace.settings_dynamic_section_counts(cx),
            )
            .unwrap();
            workspace
                .settings_workspace
                .read(cx)
                .section_list
                .state()
                .scroll_to(gpui::ListOffset {
                    item_ix,
                    offset_in_item: px(0.),
                });
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = cx
            .debug_bounds("refresh-models-local-test")
            .expect("visible refresh button");
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        workspace.read_with(cx, |workspace, cx| {
            assert!(
                workspace
                    .ai
                    .state
                    .read(cx)
                    .model_is_refreshing("local-test")
            );
        });
    }
}
