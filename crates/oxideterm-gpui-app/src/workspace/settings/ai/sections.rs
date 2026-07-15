use super::*;

pub(in crate::workspace) const AI_ACP_AGENT_TEXTAREA_MIN_H: f32 = 72.0; // Tauri min-h-[72px] for ACP args/env drafts.
pub(in crate::workspace) const AI_TOOL_NUMBER_INPUT_W: f32 = 96.0; // Tauri w-24.
pub(in crate::workspace) const AI_ACP_AGENT_FIELD_MIN_WIDTH: f32 = 220.0; // Keep ACP form fields readable on narrow settings panes.
pub(in crate::workspace) const AI_ACP_AGENT_TEXTAREA_FIELD_MIN_WIDTH: f32 = 240.0; // Multiline command fields need more room than compact selects.
pub(in crate::workspace) const AI_ACP_AGENT_AUTH_TOKEN_MIN_WIDTH: f32 = 220.0; // Let token actions wrap without crushing the secret input.
pub(in crate::workspace) const AI_ACP_AGENT_CAPABILITY_MIN_WIDTH: f32 = 150.0; // Capability checkboxes should wrap as chips, not grid columns.
pub(in crate::workspace) const AI_READONLY_VALUE_WIDTH: f32 = 180.0; // Keep compact status fields aligned with text inputs.

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_readonly_value(&self, label: String) -> AnyElement {
        div()
            .w(px(AI_READONLY_VALUE_WIDTH))
            .max_w_full()
            .min_w(px(0.0))
            .h(px(32.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            .bg(rgba((self.tokens.ui.bg << 8) | 0x66))
            .px(px(10.0))
            .flex()
            .items_center()
            .text_size(px(12.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(div().min_w(px(0.0)).truncate().child(label))
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_acp_agents_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let agent_count = settings.ai.acp_agents.len();
        // The page already owns the glass card. Keep this section structural so
        // each agent is the only nested card users need to distinguish.
        let mut section = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(settings_ai_section_heading(
                        &self.tokens,
                        self.i18n.t("settings_view.ai.acp_agents"),
                        self.i18n_count("settings_view.ai.acp_agents_summary", agent_count),
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_wrap()
                            .justify_end()
                            .gap(px(8.0))
                            .child(self.ai_acp_agent_add_button(
                                self.i18n.t("settings_view.ai.acp_agent_add"),
                                None,
                                cx,
                            ))
                            .child(self.ai_acp_agent_add_button(
                                AcpAgentPreset::ClaudeCode.display_name().to_string(),
                                Some(AcpAgentPreset::ClaudeCode),
                                cx,
                            ))
                            .child(self.ai_acp_agent_add_button(
                                AcpAgentPreset::Codex.display_name().to_string(),
                                Some(AcpAgentPreset::Codex),
                                cx,
                            ))
                            .child(self.ai_acp_agent_add_button(
                                AcpAgentPreset::GithubCopilot.display_name().to_string(),
                                Some(AcpAgentPreset::GithubCopilot),
                                cx,
                            )),
                    ),
            );

        if agent_count == 0 {
            return section
                .child(
                    div()
                        .py(px(12.0))
                        .text_size(px(12.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.ai.acp_agents_empty")),
                )
                .into_any_element();
        }

        for (index, agent) in settings.ai.acp_agents.iter().enumerate() {
            section = section.child(self.ai_acp_agent_card(index, agent, cx));
        }
        section.into_any_element()
    }

    pub(in crate::workspace) fn ai_acp_agent_card(
        &self,
        index: usize,
        agent: &oxideterm_settings::AcpAgentConfig,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let testing = self.ai.runtime.acp_agent_probe_pending.contains(&agent.id);
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x73))
            .bg(rgba((self.tokens.ui.bg_card << 8) | 0x73))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(self.ai_acp_agent_enabled_toggle(index, agent.enabled, cx))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_end()
                            .gap(px(8.0))
                            .when_some(agent.status.last_error_kind.as_ref(), |row, error| {
                                let error_label = self.i18n.t(acp_agent_error_kind_key(error));
                                row.child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(self.ai_i18n_error(
                                            "settings_view.ai.acp_agent_last_error",
                                            &error_label,
                                        )),
                                )
                            })
                            .child(self.ai_acp_agent_status_badge(agent))
                            .child(self.ai_acp_agent_test_button(index, agent, testing, cx))
                            .child(self.ai_icon_button(
                                LucideIcon::Trash2,
                                testing,
                                move |this, _event, _window, cx| {
                                    this.edit_settings(
                                        |settings| ai_delete_acp_agent(settings, index),
                                        cx,
                                    );
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(10.0))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_FIELD_MIN_WIDTH,
                        self.ai_labeled_text_input(
                            "settings_view.ai.acp_agent_name",
                            SettingsInput::AiAcpAgentDisplayName(index),
                            self.i18n.t("settings_view.ai.acp_agent_new_name"),
                            cx,
                        ),
                    ))
                    .child(
                        self.ai_responsive_field(
                            AI_ACP_AGENT_FIELD_MIN_WIDTH,
                            self.ai_labeled_text_input(
                                "settings_view.ai.acp_agent_command",
                                SettingsInput::AiAcpAgentCommand(index),
                                self.i18n
                                    .t("settings_view.ai.acp_agent_command_placeholder"),
                                cx,
                            ),
                        ),
                    )
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_FIELD_MIN_WIDTH,
                        self.ai_labeled_text_input(
                            "settings_view.ai.acp_agent_cwd",
                            SettingsInput::AiAcpAgentCwd(index),
                            self.i18n.t("settings_view.ai.acp_agent_cwd_placeholder"),
                            cx,
                        ),
                    ))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_FIELD_MIN_WIDTH,
                        self.ai_readonly_value(
                            self.i18n.t(acp_agent_auth_status_key(&agent.auth.status)),
                        ),
                    )),
            )
            .child(self.ai_acp_agent_auth_token_input(index, agent, cx))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(10.0))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_TEXTAREA_FIELD_MIN_WIDTH,
                        self.ai_textarea_row(
                            SettingsInput::AiAcpAgentArgs(index),
                            self.i18n.t("settings_view.ai.acp_agent_args"),
                            self.i18n.t("settings_view.ai.acp_agent_args_placeholder"),
                            self.i18n.t("settings_view.ai.acp_agent_args_placeholder"),
                            self.current_settings_input_value(SettingsInput::AiAcpAgentArgs(index)),
                            AI_ACP_AGENT_TEXTAREA_MIN_H,
                            cx,
                        ),
                    ))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_TEXTAREA_FIELD_MIN_WIDTH,
                        self.ai_textarea_row(
                            SettingsInput::AiAcpAgentEnv(index),
                            self.i18n.t("settings_view.ai.acp_agent_env"),
                            self.i18n.t("settings_view.ai.acp_agent_env_placeholder"),
                            self.i18n.t("settings_view.ai.acp_agent_env_placeholder"),
                            self.current_settings_input_value(SettingsInput::AiAcpAgentEnv(index)),
                            AI_ACP_AGENT_TEXTAREA_MIN_H,
                            cx,
                        ),
                    )),
            )
            .child(self.ai_acp_agent_capabilities(index, agent, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_responsive_field(
        &self,
        min_width: f32,
        control: AnyElement,
    ) -> AnyElement {
        // Flex-basis keeps desktop columns stable, while min-width zero lets the
        // field shrink only after it has wrapped onto its own line.
        div()
            .min_w(px(0.0))
            .max_w_full()
            .flex_1()
            .flex_basis(px(min_width))
            .child(control)
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_acp_agent_add_button(
        &self,
        label: String,
        preset: Option<AcpAgentPreset>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                LucideIcon::Plus,
                14.0,
                rgb(self.tokens.ui.text_muted),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                icon_gap: Some(6.0),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                this.edit_settings(
                    |settings| match preset {
                        Some(preset) => ai_add_acp_agent_preset(settings, preset),
                        None => ai_add_acp_agent(settings),
                    },
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_acp_agent_auth_token_input(
        &self,
        index: usize,
        agent: &oxideterm_settings::AcpAgentConfig,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = SettingsInput::AiAcpAgentAuthToken(index);
        let focused = self.focused_settings_input == Some(input);
        let draft = if focused {
            self.settings_input_draft.as_str()
        } else {
            ""
        };
        let save_disabled = draft.trim().is_empty();
        let remove_disabled =
            agent.auth.status != oxideterm_settings::AcpAgentAuthStatus::Authenticated;
        let save_agent_id = agent.id.clone();
        let remove_agent_id = agent.id.clone();

        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.acp_agent_auth_token")),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex_basis(px(AI_ACP_AGENT_AUTH_TOKEN_MIN_WIDTH))
                            .child(self.ai_provider_secret_input(
                                input,
                                draft,
                                if agent.auth.status
                                    == oxideterm_settings::AcpAgentAuthStatus::Authenticated
                                {
                                    self.i18n.t("settings_view.ai.acp_agent_auth_token_saved")
                                } else {
                                    self.i18n
                                        .t("settings_view.ai.acp_agent_auth_token_placeholder")
                                },
                                focused,
                                cx,
                            )),
                    )
                    .child(
                        self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.ai.save"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Secondary,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: save_disabled,
                                },
                                height: Some(32.0),
                                font_size: Some(self.tokens.metrics.ui_text_xs),
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(move |this, _event, _window, cx| {
                                this.save_ai_acp_agent_auth_token(index, save_agent_id.clone(), cx);
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element(),
                    )
                    .child(
                        self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.ai.remove"),
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Ghost,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: remove_disabled,
                                },
                                height: Some(32.0),
                                font_size: Some(self.tokens.metrics.ui_text_xs),
                                text_color: Some(rgb(self.tokens.ui.error)),
                                hover_text_color: Some(rgb(self.tokens.ui.error)),
                                hover_background: Some(rgba((self.tokens.ui.error << 8) | 0x1a)),
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(move |this, _event, _window, cx| {
                                this.delete_ai_acp_agent_auth_token(
                                    index,
                                    remove_agent_id.clone(),
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element(),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn save_ai_acp_agent_auth_token(
        &mut self,
        index: usize,
        agent_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.focused_settings_input != Some(SettingsInput::AiAcpAgentAuthToken(index)) {
            cx.notify();
            return;
        }

        // The ACP token draft is converted to a zeroizing owner at the
        // UI/backend boundary and is never applied through persisted settings.
        let Some(token) = ai_take_provider_key_secret(&mut self.settings_input_draft) else {
            cx.notify();
            return;
        };
        let key_store = self.ai.models.key_store.clone();
        let runtime = self.forwarding_runtime.clone();
        cx.spawn(async move |weak, cx| {
            let agent_id_for_store = agent_id.clone();
            let result = runtime
                .spawn_blocking(move || key_store.store_acp_auth_token(&agent_id_for_store, token))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.focused_settings_input = None;
                        this.edit_settings(
                            |settings| {
                                if let Some(agent) = settings
                                    .ai
                                    .acp_agents
                                    .iter_mut()
                                    .find(|agent| agent.id == agent_id)
                                {
                                    agent.auth.status =
                                        oxideterm_settings::AcpAgentAuthStatus::Authenticated;
                                    agent.auth.account_label = Some(
                                        (!agent.display_name.trim().is_empty())
                                            .then(|| agent.display_name.clone())
                                            .unwrap_or_else(|| agent.id.clone()),
                                    );
                                }
                            },
                            cx,
                        );
                    }
                    Err(error) => {
                        this.push_ai_settings_toast(
                            this.ai_i18n_error("settings_view.ai.save_failed", &error),
                            TerminalNoticeVariant::Error,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn delete_ai_acp_agent_auth_token(
        &mut self,
        _index: usize,
        agent_id: String,
        cx: &mut Context<Self>,
    ) {
        let key_store = self.ai.models.key_store.clone();
        let runtime = self.forwarding_runtime.clone();
        cx.spawn(async move |weak, cx| {
            let agent_id_for_delete = agent_id.clone();
            let result = runtime
                .spawn_blocking(move || key_store.delete_acp_auth_token(&agent_id_for_delete))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.edit_settings(
                            |settings| {
                                if let Some(agent) = settings
                                    .ai
                                    .acp_agents
                                    .iter_mut()
                                    .find(|agent| agent.id == agent_id)
                                {
                                    agent.auth.status =
                                        oxideterm_settings::AcpAgentAuthStatus::Unknown;
                                    agent.auth.account_label = None;
                                }
                            },
                            cx,
                        );
                    }
                    Err(error) => {
                        this.push_ai_settings_toast(
                            this.ai_i18n_error("settings_view.ai.remove_failed", &error),
                            TerminalNoticeVariant::Error,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn ai_labeled_text_input(
        &self,
        label_key: &str,
        input: SettingsInput,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.settings_text_input_control(
                input,
                self.current_settings_input_value(input),
                placeholder,
                240.0,
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_acp_agent_enabled_toggle(
        &self,
        index: usize,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        checkbox(
            &self.tokens,
            if enabled {
                self.i18n.t("settings_view.ai.acp_agent_enabled")
            } else {
                self.i18n.t("settings_view.ai.acp_agent_disabled")
            },
            enabled,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.edit_settings(
                    |settings| {
                        if let Some(agent) = settings.ai.acp_agents.get_mut(index) {
                            agent.enabled = !enabled;
                        }
                    },
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_acp_agent_status_badge(
        &self,
        agent: &oxideterm_settings::AcpAgentConfig,
    ) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.bg << 8) | 0x80))
            .px(px(8.0))
            .py(px(4.0))
            .text_size(px(10.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                self.i18n
                    .t(acp_agent_runtime_status_key(&agent.status.state)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_acp_agent_test_button(
        &self,
        index: usize,
        agent: &oxideterm_settings::AcpAgentConfig,
        testing: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let agent_for_probe = agent.clone();
        let mut options = ToolbarButtonOptions::compact_text(
            ButtonVariant::Outline,
            ButtonRadius::Md,
            28.0,
            8.0,
            12.0,
        );
        options.button.disabled = testing;
        options.icon_gap = Some(6.0);
        options.loading = testing;

        self.workspace_toolbar_action_button(
            if testing {
                self.i18n.t("settings_view.ai.acp_agent_testing")
            } else {
                self.i18n.t("settings_view.ai.acp_agent_test")
            },
            Some(Self::render_lucide_icon(
                LucideIcon::RefreshCw,
                12.0,
                rgb(self.tokens.ui.text_muted),
            )),
            options,
            cx.listener(move |this, _event, _window, cx| {
                this.test_ai_acp_agent(index, agent_for_probe.clone(), cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn test_ai_acp_agent(
        &mut self,
        _index: usize,
        agent: oxideterm_settings::AcpAgentConfig,
        cx: &mut Context<Self>,
    ) {
        if self.ai.runtime.acp_agent_probe_pending.contains(&agent.id) {
            cx.notify();
            return;
        }

        let agent_id = agent.id.clone();
        self.ai
            .runtime
            .acp_agent_probe_pending
            .insert(agent_id.clone());
        if self.ai.runtime.acp_agent_probe_tx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            self.ai.runtime.acp_agent_probe_tx = Some(tx);
            self.ai.runtime.acp_agent_probe_rx = Some(rx);
        }
        let Some(ui_tx) = self.ai.runtime.acp_agent_probe_tx.as_ref().cloned() else {
            self.ai.runtime.acp_agent_probe_pending.remove(&agent_id);
            cx.notify();
            return;
        };

        let launch_config = ai_acp_launch_config_from_settings(&agent);
        let capability_policy = ai_acp_capability_policy_from_settings(&agent.capability_policy);
        // ACP probe uses the shared backend runtime because launching a stdio
        // agent needs Tokio process IO. The UI receives only redacted status.
        self.forwarding_runtime.spawn(async move {
            let result = match oxideterm_ai::build_acp_stdio_launcher(launch_config) {
                Ok(launcher) => {
                    if !oxideterm_ai::acp_launch_command_available(launcher.config())
                        .unwrap_or(false)
                    {
                        ai_acp_probe_error_result("command_not_found")
                    } else {
                        let initialize_result = oxideterm_ai::initialize_acp_agent(
                            launcher,
                            env!("CARGO_PKG_VERSION").to_string(),
                            capability_policy,
                        )
                        .await;
                        match initialize_result {
                            Ok(response) => {
                                let auth_required = !response.auth_methods.is_empty();
                                AcpAgentProbeResult {
                                    runtime_state: if auth_required {
                                        oxideterm_settings::AcpAgentRuntimeState::AuthRequired
                                    } else {
                                        oxideterm_settings::AcpAgentRuntimeState::Ready
                                    },
                                    auth_status: if auth_required {
                                        oxideterm_settings::AcpAgentAuthStatus::Required
                                    } else {
                                        oxideterm_settings::AcpAgentAuthStatus::NotRequired
                                    },
                                    last_error_kind: None,
                                }
                            }
                            Err(_) => ai_acp_probe_error_result("initialize"),
                        }
                    }
                }
                Err(_) => ai_acp_probe_error_result("config"),
            };
            let _ = ui_tx.send(AcpAgentProbeDelivery { agent_id, result });
        });
        self.schedule_ai_acp_agent_probe_poll(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn poll_ai_acp_agent_probe_results(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.ai.runtime.acp_agent_probe_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        loop {
            match rx.try_recv() {
                Ok(delivery) => {
                    self.ai
                        .runtime
                        .acp_agent_probe_pending
                        .remove(&delivery.agent_id);
                    self.edit_settings(
                        |settings| {
                            if let Some(agent) = settings
                                .ai
                                .acp_agents
                                .iter_mut()
                                .find(|agent| agent.id == delivery.agent_id)
                            {
                                agent.auth.status = delivery.result.auth_status.clone();
                                agent.status.state = delivery.result.runtime_state.clone();
                                agent.status.last_error_kind =
                                    delivery.result.last_error_kind.clone();
                            }
                        },
                        cx,
                    );
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    keep_rx = false;
                    self.ai.runtime.acp_agent_probe_tx = None;
                    self.ai.runtime.acp_agent_probe_pending.clear();
                    break;
                }
            }
        }
        if keep_rx && !self.ai.runtime.acp_agent_probe_pending.is_empty() {
            self.ai.runtime.acp_agent_probe_rx = Some(rx);
        } else if self.ai.runtime.acp_agent_probe_pending.is_empty() {
            self.ai.runtime.acp_agent_probe_tx = None;
        }
    }

    pub(in crate::workspace) fn schedule_ai_acp_agent_probe_poll(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.ai.runtime.acp_agent_probe_polling {
            return;
        }
        self.ai.runtime.acp_agent_probe_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(50)).await;
            let _ = weak.update(cx, |this, cx| {
                this.ai.runtime.acp_agent_probe_polling = false;
                this.poll_ai_acp_agent_probe_results(cx);
                if !this.ai.runtime.acp_agent_probe_pending.is_empty() {
                    this.schedule_ai_acp_agent_probe_poll(cx);
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn ai_acp_agent_capabilities(
        &self,
        index: usize,
        agent: &oxideterm_settings::AcpAgentConfig,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Capabilities are part of the agent form, not another independent card.
        div()
            .w_full()
            .min_w(px(0.0))
            .border_t_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            .pt(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.ai.acp_agent_capabilities")),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_CAPABILITY_MIN_WIDTH,
                        self.ai_acp_agent_capability_toggle(
                            index,
                            "settings_view.ai.acp_agent_capability_read",
                            agent.capability_policy.fs_read_text_file,
                            |policy| policy.fs_read_text_file = !policy.fs_read_text_file,
                            cx,
                        ),
                    ))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_CAPABILITY_MIN_WIDTH,
                        self.ai_acp_agent_capability_toggle(
                            index,
                            "settings_view.ai.acp_agent_capability_write",
                            agent.capability_policy.fs_write_text_file,
                            |policy| policy.fs_write_text_file = !policy.fs_write_text_file,
                            cx,
                        ),
                    ))
                    .child(self.ai_responsive_field(
                        AI_ACP_AGENT_CAPABILITY_MIN_WIDTH,
                        self.ai_acp_agent_capability_toggle(
                            index,
                            "settings_view.ai.acp_agent_capability_terminal",
                            agent.capability_policy.terminal,
                            |policy| policy.terminal = !policy.terminal,
                            cx,
                        ),
                    )),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.acp_agent_capabilities_hint")),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_acp_agent_capability_toggle(
        &self,
        index: usize,
        label_key: &str,
        checked: bool,
        toggle: fn(&mut oxideterm_settings::AcpAgentCapabilityPolicy),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        checkbox(&self.tokens, label, checked)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(
                        |settings| {
                            if let Some(agent) = settings.ai.acp_agents.get_mut(index) {
                                toggle(&mut agent.capability_policy);
                            }
                        },
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_provider_settings_section(
        &self,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.settings_page.ai_provider_settings_expanded;
        let summary = self.i18n_count(
            "settings_view.ai.provider_settings_summary",
            self.settings_store.settings().ai.providers.len(),
        );
        self.sync_ai_provider_card_list_state(providers);
        let provider_list = if expanded {
            let state = self.ai.models.provider_card_list_state.clone();
            let spec = self.ai_provider_card_list_spec();
            let workspace = cx.entity();
            let list_height = self.ai_provider_card_list_estimated_height(providers);
            Some(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .mt(px(12.0))
                    .h(px(list_height))
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            workspace
                                .update(cx, |this, cx| this.ai_provider_card_list_item(index, cx))
                        },
                    ))
                    .into_any_element(),
            )
        } else {
            None
        };

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(self.ai_collapsible_header(
                "settings_view.ai.provider_settings",
                summary,
                expanded,
                |this, _event, _window, cx| {
                    this.settings_page
                        .toggle_ai_section(AiSettingsSection::ProviderSettings);
                    cx.stop_propagation();
                    cx.notify();
                },
                cx,
            ))
            .when_some(provider_list, |section, provider_list| {
                section.child(provider_list)
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_provider_card_list_estimated_height(
        &self,
        providers: &[AiProviderView],
    ) -> f32 {
        providers
            .iter()
            .map(|provider| self.ai_provider_card_estimated_height(provider))
            .sum::<f32>()
            + AI_PROVIDER_CARD_LIST_ESTIMATED_HEIGHT
    }

    pub(in crate::workspace) fn ai_provider_card_estimated_height(
        &self,
        provider: &AiProviderView,
    ) -> f32 {
        let active_provider = self
            .settings_store
            .settings()
            .ai
            .active_provider_id
            .as_deref()
            == Some(provider.id.as_str());
        let expanded = self
            .settings_page
            .expanded_ai_providers
            .get(&provider.id)
            .copied()
            .unwrap_or(active_provider);
        if !expanded {
            return 72.0;
        }
        let models_expanded = self
            .settings_page
            .expanded_ai_provider_models
            .contains(&provider.id);
        let visible_model_count = if models_expanded {
            provider.models.len()
        } else {
            provider.models.len().min(AI_PROVIDER_VISIBLE_MODEL_LIMIT)
        };
        let chip_rows = visible_model_count
            .div_ceil(AI_PROVIDER_MODEL_CHIPS_PER_VIRTUAL_ROW)
            .max(1);
        let key_input_height = if self
            .ai_provider_key_display_state(provider)
            .shows_key_control()
        {
            72.0
        } else {
            0.0
        };
        // This mirrors the nested card structure: header, toolbar, two-column
        // fields, model-chip rows, optional API-key editor, and row spacing.
        72.0 + 52.0
            + 112.0
            + 34.0
            + chip_rows as f32 * AI_PROVIDER_MODEL_CHIP_ROW_ESTIMATED_HEIGHT
            + key_input_height
            + 16.0
    }

    pub(in crate::workspace) fn sync_ai_provider_card_list_state(
        &self,
        providers: &[AiProviderView],
    ) {
        let mut signatures = providers
            .iter()
            .map(|provider| {
                let active_provider = self
                    .settings_store
                    .settings()
                    .ai
                    .active_provider_id
                    .as_deref()
                    == Some(provider.id.as_str());
                let expanded = self
                    .settings_page
                    .expanded_ai_providers
                    .get(&provider.id)
                    .copied()
                    .unwrap_or(active_provider);
                ai_provider_card_signature(
                    provider,
                    expanded,
                    self.settings_page
                        .expanded_ai_provider_models
                        .contains(&provider.id),
                    self.ai_provider_has_key_cached(&provider.id),
                )
            })
            .collect::<Vec<_>>();
        // The add-provider controls are the final virtual row inside this
        // section. Keep a stable sentinel signature for that fixed row.
        signatures.push(0xadd0_0001);
        sync_tauri_variable_list_state_by_signatures(
            &self.ai.models.provider_card_list_state,
            &mut self.ai.models.provider_card_list_cache.borrow_mut(),
            "ai-provider-cards",
            &signatures,
            self.ai_provider_card_list_spec(),
        );
    }

    pub(in crate::workspace) fn ai_provider_card_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(AI_PROVIDER_CARD_LIST_ESTIMATED_HEIGHT),
            AI_PROVIDER_CARD_LIST_OVERSCAN,
        )
    }

    pub(in crate::workspace) fn ai_provider_card_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let providers = ai_provider_views(self.settings_store.settings());
        if index == providers.len() {
            return div()
                .w_full()
                .min_w(px(0.0))
                .pb(px(12.0))
                .child(self.ai_provider_add_controls(cx))
                .into_any_element();
        }
        let Some(provider) = providers.get(index) else {
            return div().into_any_element();
        };
        div()
            .w_full()
            .min_w(px(0.0))
            .pb(px(12.0))
            .child(self.ai_provider_card(index, provider, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_context_controls_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_context_controls_section(
            AI_PROVIDER_MAX_W,
            self.ai_section_title("settings_view.ai.context_controls"),
            vec![
                self.ai_context_select_field(
                    "settings_view.ai.max_context",
                    "settings_view.ai.max_context_hint",
                    SettingsSelect::AiContextMaxChars,
                    self.ai_context_max_chars_label(settings.ai.context_max_chars),
                    cx,
                ),
                self.ai_context_select_field(
                    "settings_view.ai.buffer_history",
                    "settings_view.ai.buffer_history_hint",
                    SettingsSelect::AiContextVisibleLines,
                    self.ai_context_visible_lines_label(settings.ai.context_visible_lines),
                    cx,
                ),
            ],
            settings_ai_context_sources_group(
                &self.tokens,
                self.i18n.t("settings_view.ai.context_sources"),
                vec![
                    self.ai_context_source_row(
                        "settings_view.ai.context_source_ide",
                        "settings_view.ai.context_source_ide_hint",
                        settings.ai.context_sources.ide,
                        set_ai_context_source_ide,
                        cx,
                    ),
                    self.ai_context_source_row(
                        "settings_view.ai.context_source_sftp",
                        "settings_view.ai.context_source_sftp_hint",
                        settings.ai.context_sources.sftp,
                        set_ai_context_source_sftp,
                        cx,
                    ),
                ],
            ),
        )
    }

    pub(in crate::workspace) fn ai_context_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_context_select_field(
            &self.tokens,
            self.i18n.t(label_key),
            self.settings_select_control(select_id, label, false, None, cx),
            self.i18n.t(hint_key),
        )
    }

    pub(in crate::workspace) fn ai_context_source_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_context_source_row(
            &self.tokens,
            self.i18n.t(label_key),
            self.i18n.t(hint_key),
            checkbox(&self.tokens, String::new(), checked).into_any_element(),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.edit_settings(|settings| setter(settings, !checked), cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_context_max_chars_label(&self, value: i64) -> String {
        ai_context_max_chars_label_key(value)
            .map(|key| self.i18n.t(key))
            .unwrap_or_else(|| value.to_string())
    }

    pub(in crate::workspace) fn ai_context_visible_lines_label(&self, value: i64) -> String {
        ai_context_visible_lines_label_key(value)
            .map(|key| self.i18n.t(key))
            .unwrap_or_else(|| value.to_string())
    }

    pub(in crate::workspace) fn ai_system_prompt_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_custom_prompt = !settings.ai.custom_system_prompt.trim().is_empty();
        settings_ai_editor_summary_section(
            &self.tokens,
            self.ai_section_title("settings_view.ai.system_prompt_title"),
            self.i18n.t("settings_view.ai.system_prompt_hint"),
            None,
            self.i18n.t(if has_custom_prompt {
                "settings_view.ai.system_prompt_status_custom"
            } else {
                "settings_view.ai.system_prompt_status_default"
            }),
            self.ai_text_editor_action_button(AiTextEditorDialog::SystemPrompt, cx),
        )
    }

    pub(in crate::workspace) fn ai_memory_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_saved_memory = !settings.ai.memory.content.trim().is_empty();
        settings_ai_editor_summary_section(
            &self.tokens,
            settings_ai_icon_heading(
                Self::render_lucide_icon(LucideIcon::Brain, 16.0, rgb(self.tokens.ui.text)),
                self.ai_section_title("settings_view.ai.memory_title"),
            )
            .into_any_element(),
            self.i18n.t("settings_view.ai.memory_hint"),
            Some(self.bool_row(
                "settings_view.ai.memory_enabled",
                "settings_view.ai.memory_enabled_hint",
                settings.ai.memory.enabled,
                set_ai_memory_enabled,
                cx,
            )),
            self.i18n.t(if has_saved_memory {
                "settings_view.ai.memory_status_saved"
            } else {
                "settings_view.ai.memory_status_empty"
            }),
            self.ai_text_editor_action_button(AiTextEditorDialog::Memory, cx),
        )
    }

    pub(in crate::workspace) fn ai_reasoning_section(
        &self,
        settings: &PersistedSettings,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_reasoning_section(
            self.ai_global_reasoning_section(settings, cx),
            self.ai_model_reasoning_overrides_section(settings, providers, cx),
            self.ai_active_model_max_response_tokens_row(settings, cx),
        )
    }

    pub(in crate::workspace) fn ai_text_editor_action_button(
        &self,
        dialog: AiTextEditorDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t(match dialog {
                AiTextEditorDialog::SystemPrompt => "settings_view.ai.system_prompt_edit",
                AiTextEditorDialog::Memory => "settings_view.ai.memory_edit",
            }),
            Some(Self::render_lucide_icon(
                LucideIcon::Pencil,
                12.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| {
                this.open_ai_text_editor(dialog, window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_global_reasoning_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_global_reasoning_section(
            &self.tokens,
            self.i18n.t("settings_view.ai.reasoning_title"),
            self.settings_select_control(
                SettingsSelect::AiGlobalReasoning,
                self.i18n
                    .t(ai_reasoning_label_key(ai_reasoning_profile_value(
                        settings.ai.reasoning_effort,
                    ))),
                false,
                None,
                cx,
            ),
            self.i18n.t("settings_view.ai.reasoning_hint"),
            AI_PROVIDER_MAX_W,
        )
    }

    pub(in crate::workspace) fn ai_tool_use_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let approved_count = ai_tool_auto_approved_count(settings);
        let total_count = ai_tool_auto_approve_total_count(settings);
        let policy_groups = settings_ai_tool_policy_grid(
            ai_tool_policy_groups(settings)
                .into_iter()
                .map(|group| self.ai_tool_policy_group(group, cx))
                .collect(),
        );

        let collapsed_summary = (!self.settings_page.ai_tool_use_expanded).then(|| {
            settings_ai_tool_collapsed_summary(
                &self.tokens,
                format!(
                    "{} · {}",
                    self.i18n.t("settings_view.ai.tool_use_policy_summary"),
                    self.i18n
                        .t("settings_view.ai.tool_use_collapsed_summary")
                        .replace("{{approved}}", &approved_count.to_string())
                        .replace("{{total}}", &total_count.to_string())
                ),
            )
        });
        let expanded_body = self.settings_page.ai_tool_use_expanded.then(|| {
            settings_ai_tool_expanded_body(
                &self.tokens,
                settings.ai.tool_use.enabled,
                vec![
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.ai.tool_use_approve_hint"))
                        .into_any_element(),
                    self.ai_tool_number_input_row(
                        "settings_view.ai.tool_use_max_rounds",
                        "settings_view.ai.tool_use_max_rounds_hint",
                        SettingsInput::AiToolUseMaxRounds,
                        settings
                            .ai
                            .tool_use
                            .max_rounds
                            .unwrap_or(oxideterm_settings::DEFAULT_AI_TOOL_MAX_ROUNDS),
                        cx,
                    ),
                    policy_groups,
                    self.ai_disabled_tools_notice(settings, cx),
                    settings_ai_policy_warning(
                        &self.tokens,
                        self.i18n.t("settings_view.ai.tool_policy_warning"),
                    ),
                ],
            )
        });

        settings_ai_tool_use_section(
            AI_PROVIDER_MAX_W,
            settings_ai_icon_heading(
                Self::render_lucide_icon(LucideIcon::Wrench, 16.0, rgb(self.tokens.ui.text)),
                self.ai_section_title("settings_view.ai.tool_use"),
            ),
            self.ai_tool_expand_button(cx),
            self.bool_row(
                "settings_view.ai.tool_use_enabled",
                "settings_view.ai.tool_use_enabled_hint",
                settings.ai.tool_use.enabled,
                set_ai_tool_use_enabled,
                cx,
            ),
            collapsed_summary,
            expanded_body,
        )
    }

    pub(in crate::workspace) fn ai_tool_number_input_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_tool_number_input_row(
            &self.tokens,
            self.setting_row(
                label_key,
                hint_key,
                self.number_input(input, value.to_string(), AI_TOOL_NUMBER_INPUT_W, cx),
                cx,
            ),
        )
    }

    pub(in crate::workspace) fn ai_section_heading(
        &self,
        title_key: &str,
        hint_key: &str,
    ) -> AnyElement {
        settings_ai_section_heading(&self.tokens, self.i18n.t(title_key), self.i18n.t(hint_key))
    }

    pub(in crate::workspace) fn ai_collapsible_header(
        &self,
        title_key: &str,
        summary: String,
        expanded: bool,
        on_click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_collapsible_header(
            &self.tokens,
            self.i18n.t(title_key).to_uppercase(),
            summary,
            self.render_animated_chevron(
                (
                    gpui::SharedString::from(format!("ai-section-chevron-{title_key}")),
                    expanded as usize,
                ),
                expanded,
                16.0,
                rgb(self.tokens.ui.text_muted),
            ),
        )
        .on_mouse_down(MouseButton::Left, cx.listener(on_click))
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_icon_button(
        &self,
        icon: LucideIcon,
        disabled: bool,
        on_click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon_color = if matches!(icon, LucideIcon::Trash2) {
            rgb(self.tokens.ui.error)
        } else {
            rgb(self.tokens.ui.text_muted)
        };

        self.workspace_icon_action_button(
            icon,
            15.0,
            icon_color,
            IconButtonOptions {
                disabled,
                hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x80)),
                // Tauri AI action buttons are fully opaque until disabled; the
                // workspace wrapper owns the disabled activation guard.
                ..IconButtonOptions::opaque_toolbar(30.0, ButtonRadius::Md)
            },
            on_click,
            cx,
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_textarea_row(
        &self,
        input: SettingsInput,
        label: String,
        hint: String,
        placeholder: String,
        value: String,
        min_height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let marked_text = self
            .marked_text_for_target(target)
            .map(|marked| marked.to_string());
        let caret = focused.then(|| {
            text_caret(&self.tokens, self.new_connection_caret_visible).into_any_element()
        });
        let textarea = settings_ai_textarea_surface(
            &self.tokens,
            min_height,
            focused,
            display_value,
            &placeholder,
            marked_text,
            caret,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                let current = this.current_settings_input_value(input);
                this.focus_settings_input(input, current, cx);
                this.ime_marked_text = None;
                window.focus(&this.focus_handle);
                this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                cx.stop_propagation();
            }),
        )
        .on_mouse_move(
            cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
            }),
        );

        let control =
            text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            });

        settings_ai_textarea_row(&self.tokens, label, control.into_any_element(), hint)
    }

    pub(in crate::workspace) fn ai_model_reasoning_overrides_section(
        &self,
        settings: &PersistedSettings,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_panels = ai_model_reasoning_panels(settings, providers);
        div()
            .mt(px(8.0))
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(self.ai_model_reasoning_header(cx))
            .when(self.settings_page.ai_model_reasoning_expanded, |section| {
                if provider_panels.is_empty() {
                    section.child(settings_ai_model_empty_text(
                        &self.tokens,
                        self.i18n
                            .t("settings_view.ai.model_reasoning_overrides_empty"),
                    ))
                } else {
                    let mut list = div()
                        .w_full()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(16.0));
                    for panel in provider_panels {
                        list = list.child(self.ai_model_reasoning_provider(settings, panel, cx));
                    }
                    section.child(list)
                }
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_model_reasoning_header(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_model_reasoning_header(
            &self.tokens,
            self.i18n.t("settings_view.ai.model_reasoning_overrides"),
            self.i18n
                .t("settings_view.ai.model_reasoning_overrides_hint"),
            self.render_animated_chevron(
                (
                    "ai-model-reasoning-chevron",
                    self.settings_page.ai_model_reasoning_expanded as usize,
                ),
                self.settings_page.ai_model_reasoning_expanded,
                16.0,
                rgb(self.tokens.ui.text_muted),
            ),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.settings_page
                    .toggle_ai_section(AiSettingsSection::ModelReasoning);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_model_reasoning_provider(
        &self,
        settings: &PersistedSettings,
        panel: AiProviderModelPanel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = panel.provider_id.clone();
        let expanded = self
            .settings_page
            .expanded_ai_model_reasoning_providers
            .contains(&provider_id);
        let header_provider_id = provider_id.clone();
        let header = settings_ai_model_provider_header(
            &self.tokens,
            panel.provider_name.clone(),
            self.i18n
                .t("settings_view.ai.model_reasoning_provider_summary")
                .replace("{{count}}", &panel.model_count.to_string())
                .replace("{{overrides}}", &panel.override_count.to_string()),
            self.render_animated_chevron(
                (
                    gpui::SharedString::from(format!(
                        "ai-model-reasoning-provider-chevron-{provider_id}"
                    )),
                    expanded as usize,
                ),
                expanded,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.settings_page
                    .toggle_ai_model_reasoning_provider(header_provider_id.clone());
                cx.stop_propagation();
                cx.notify();
            }),
        );
        let rows = if expanded {
            let models = panel.models.clone();
            let state = self.sync_ai_reasoning_model_list_state(settings, &provider_id, &models);
            let spec = self.ai_provider_model_row_list_spec();
            let workspace = cx.entity();
            let provider_id_for_rows = provider_id;
            let provider_index = panel.provider_index;
            let list_height = models.len() as f32 * AI_PROVIDER_MODEL_ROW_LIST_ESTIMATED_HEIGHT;
            Some(settings_ai_model_row_list_frame(
                &self.tokens,
                list_height,
                tauri_virtual_list(state, spec, move |model_index, _window, cx| {
                    let Some(model) = models.get(model_index).cloned() else {
                        return div().into_any_element();
                    };
                    let provider_id = provider_id_for_rows.clone();
                    workspace.update(cx, |this, cx| {
                        let settings = this.settings_store.settings();
                        this.ai_model_reasoning_row(
                            provider_index,
                            model_index,
                            settings,
                            &provider_id,
                            &model,
                            cx,
                        )
                    })
                })
                .into_any_element(),
            ))
        } else {
            None
        };
        settings_ai_model_provider_section(header, rows)
    }

    pub(in crate::workspace) fn sync_ai_reasoning_model_list_state(
        &self,
        settings: &PersistedSettings,
        provider_id: &str,
        models: &[String],
    ) -> ListState {
        let signatures = models
            .iter()
            .map(|model| {
                ai_provider_model_row_signature(
                    provider_id,
                    model,
                    settings
                        .ai
                        .reasoning_model_overrides
                        .get(provider_id)
                        .and_then(|overrides| overrides.get(model)),
                )
            })
            .collect::<Vec<_>>();
        let state = {
            let mut states = self.ai.models.reasoning_model_list_states.borrow_mut();
            states
                .entry(provider_id.to_string())
                .or_insert_with(|| {
                    // Reasoning override rows are measured independently per
                    // provider so expanding one provider does not perturb
                    // another provider's virtual table.
                    ListState::new(
                        AI_PROVIDER_MODEL_ROW_LIST_INITIAL_ITEM_COUNT,
                        ListAlignment::Top,
                        self.ai_provider_model_row_list_spec().overdraw(),
                    )
                    .measure_all()
                })
                .clone()
        };
        {
            let mut caches = self.ai.models.reasoning_model_list_caches.borrow_mut();
            let cache = caches.entry(provider_id.to_string()).or_default();
            sync_tauri_variable_list_state_by_signatures(
                &state,
                cache,
                &format!("ai-reasoning-models:{provider_id}"),
                &signatures,
                self.ai_provider_model_row_list_spec(),
            );
        }
        state
    }

    pub(in crate::workspace) fn ai_model_reasoning_row(
        &self,
        provider_index: usize,
        model_index: usize,
        settings: &PersistedSettings,
        provider_id: &str,
        model: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = ai_model_reasoning_row_model(settings, provider_id, model);
        settings_ai_model_reasoning_row(
            &self.tokens,
            settings_mono_font_family(settings),
            model.to_string(),
            self.settings_select_control(
                SettingsSelect::AiModelReasoning(provider_index, model_index),
                self.i18n.t(row.label_key),
                false,
                Some(160.0),
                cx,
            ),
            model_index == 0,
        )
    }

    pub(in crate::workspace) fn ai_model_context_windows_section(
        &self,
        settings: &PersistedSettings,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_panels = ai_model_context_window_panels(settings, providers);
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(self.ai_context_windows_header(cx))
            .when(self.settings_page.ai_context_windows_expanded, |section| {
                if provider_panels.is_empty() {
                    section.child(settings_ai_model_empty_text(
                        &self.tokens,
                        self.i18n.t("settings_view.ai.model_context_windows_empty"),
                    ))
                } else {
                    let mut list = div()
                        .w_full()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(16.0));
                    for panel in provider_panels {
                        list = list.child(self.ai_context_window_provider(settings, panel, cx));
                    }
                    section.child(list)
                }
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_context_windows_header(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_context_windows_header(
            &self.tokens,
            self.i18n.t("settings_view.ai.model_context_windows"),
            self.i18n.t("settings_view.ai.model_context_windows_hint"),
            self.render_animated_chevron(
                (
                    "ai-context-windows-chevron",
                    self.settings_page.ai_context_windows_expanded as usize,
                ),
                self.settings_page.ai_context_windows_expanded,
                16.0,
                rgb(self.tokens.ui.text_muted),
            ),
            AI_PROVIDER_MAX_W,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.settings_page
                    .toggle_ai_section(AiSettingsSection::ContextWindows);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_context_window_provider(
        &self,
        settings: &PersistedSettings,
        panel: AiProviderModelPanel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = panel.provider_id.clone();
        let expanded = self
            .settings_page
            .expanded_ai_context_providers
            .contains(&provider_id);
        let header_provider_id = provider_id.clone();
        let header = settings_ai_model_provider_header(
            &self.tokens,
            panel.provider_name.clone(),
            self.i18n
                .t("settings_view.ai.ctx_provider_summary")
                .replace("{{count}}", &panel.model_count.to_string())
                .replace("{{overrides}}", &panel.override_count.to_string()),
            self.render_animated_chevron(
                (
                    gpui::SharedString::from(format!("ai-context-provider-chevron-{provider_id}")),
                    expanded as usize,
                ),
                expanded,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.settings_page
                    .toggle_ai_context_provider(header_provider_id.clone());
                cx.stop_propagation();
                cx.notify();
            }),
        );
        let rows = if expanded {
            let models = panel.models.clone();
            let state = self.sync_ai_context_model_list_state(settings, &provider_id, &models);
            let spec = self.ai_provider_model_row_list_spec();
            let workspace = cx.entity();
            let provider_id_for_rows = provider_id;
            let provider_index = panel.provider_index;
            let list_height = models.len() as f32 * AI_PROVIDER_MODEL_ROW_LIST_ESTIMATED_HEIGHT;
            Some(settings_ai_model_row_list_frame(
                &self.tokens,
                list_height,
                tauri_virtual_list(state, spec, move |model_index, _window, cx| {
                    let Some(model) = models.get(model_index).cloned() else {
                        return div().into_any_element();
                    };
                    let provider_id = provider_id_for_rows.clone();
                    workspace.update(cx, |this, cx| {
                        let settings = this.settings_store.settings();
                        this.ai_context_window_row(
                            provider_index,
                            model_index,
                            settings,
                            &provider_id,
                            &model,
                            cx,
                        )
                    })
                })
                .into_any_element(),
            ))
        } else {
            None
        };
        settings_ai_model_provider_section(header, rows)
    }

    pub(in crate::workspace) fn sync_ai_context_model_list_state(
        &self,
        settings: &PersistedSettings,
        provider_id: &str,
        models: &[String],
    ) -> ListState {
        let signatures = models
            .iter()
            .map(|model| {
                ai_provider_model_row_signature(
                    provider_id,
                    model,
                    settings
                        .ai
                        .user_context_windows
                        .get(provider_id)
                        .and_then(|windows| windows.get(model)),
                )
            })
            .collect::<Vec<_>>();
        let state = {
            let mut states = self.ai.models.context_model_list_states.borrow_mut();
            states
                .entry(provider_id.to_string())
                .or_insert_with(|| {
                    // Context override rows are measured independently per
                    // provider for the same reason as reasoning rows.
                    ListState::new(
                        AI_PROVIDER_MODEL_ROW_LIST_INITIAL_ITEM_COUNT,
                        ListAlignment::Top,
                        self.ai_provider_model_row_list_spec().overdraw(),
                    )
                    .measure_all()
                })
                .clone()
        };
        {
            let mut caches = self.ai.models.context_model_list_caches.borrow_mut();
            let cache = caches.entry(provider_id.to_string()).or_default();
            sync_tauri_variable_list_state_by_signatures(
                &state,
                cache,
                &format!("ai-context-models:{provider_id}"),
                &signatures,
                self.ai_provider_model_row_list_spec(),
            );
        }
        state
    }

    pub(in crate::workspace) fn ai_provider_model_row_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(AI_PROVIDER_MODEL_ROW_LIST_ESTIMATED_HEIGHT),
            AI_PROVIDER_MODEL_ROW_LIST_OVERSCAN,
        )
    }

    pub(in crate::workspace) fn ai_context_window_row(
        &self,
        provider_index: usize,
        model_index: usize,
        settings: &PersistedSettings,
        provider_id: &str,
        model: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = ai_model_context_window_row_model(settings, provider_id, model);
        let input = SettingsInput::AiModelContextWindow(provider_index, model_index);
        let reset_provider_id = provider_id.to_string();
        let reset_model = model.to_string();
        let reset = row.has_override.then(|| {
            div()
                .cursor_pointer()
                .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x99))
                .hover(|style| style.text_color(rgb(self.tokens.ui.text)))
                .child(Self::render_lucide_icon(
                    LucideIcon::X,
                    12.0,
                    rgb(self.tokens.ui.text_muted),
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        let provider_id = reset_provider_id.clone();
                        let model = reset_model.clone();
                        this.edit_settings(
                            move |settings| {
                                set_ai_user_context_window(settings, &provider_id, &model, None);
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
        });
        settings_ai_context_window_row(
            &self.tokens,
            settings_mono_font_family(settings),
            model.to_string(),
            settings_ai_context_source_badge_for_source(
                &self.tokens,
                self.i18n.t(row.source.i18n_key()),
                row.source,
            ),
            self.settings_text_input_control(
                input,
                self.current_settings_input_value(input),
                "Auto".to_string(),
                AI_CONTEXT_NUMBER_W,
                cx,
            )
            .into_any_element(),
            reset,
            row.has_override,
            model_index == 0,
        )
    }

    pub(in crate::workspace) fn ai_active_model_max_response_tokens_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(model) = settings.ai.active_model.clone() else {
            return div().into_any_element();
        };
        settings_ai_active_model_max_response_tokens_row(
            &self.tokens,
            self.i18n.t("settings_view.ai.max_response_tokens"),
            self.i18n.t("settings_view.ai.max_response_tokens_hint"),
            format!("{model}:"),
            self.settings_text_input_control(
                SettingsInput::AiActiveModelMaxResponseTokens,
                self.current_settings_input_value(SettingsInput::AiActiveModelMaxResponseTokens),
                "Auto".to_string(),
                128.0,
                cx,
            ),
            settings_mono_font_family(settings),
        )
    }

    pub(in crate::workspace) fn ai_tool_expand_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.settings_page.ai_tool_use_expanded;
        // Tool-policy expand/collapse is an outline small Button in Tauri.
        // Route it through the same shared primitive as other settings
        // command buttons.
        self.workspace_toolbar_action_button(
            if expanded {
                self.i18n.t("settings_view.ai.tool_use_collapse")
            } else {
                self.i18n.t("settings_view.ai.tool_use_expand")
            },
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.settings_page
                    .toggle_ai_section(AiSettingsSection::ToolUse);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_tool_policy_group(
        &self,
        group: AiToolPolicyGroup,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut items = Vec::new();
        for item in group.items {
            let tool_key = item.key.map(str::to_string);
            let checked = item.checked;
            let locked = item.locked;
            let control = checkbox(&self.tokens, String::new(), checked)
                .opacity(if locked { 0.5 } else { 1.0 })
                .when(!locked, |checkbox| {
                    checkbox.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            let Some(tool_key) = tool_key.clone() else {
                                return;
                            };
                            this.edit_settings(
                                move |settings| {
                                    settings
                                        .ai
                                        .tool_use
                                        .auto_approve_tools
                                        .insert(tool_key.clone(), serde_json::json!(!checked));
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                })
                .into_any_element();
            items.push(settings_ai_tool_policy_item(
                &self.tokens,
                self.i18n.t(item.label_key),
                control,
            ));
        }

        settings_ai_tool_policy_group(
            &self.tokens,
            self.i18n.t(group.title_key),
            self.i18n.t(group.description_key),
            items,
        )
    }

    pub(in crate::workspace) fn ai_disabled_tools_notice(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = settings.ai.tool_use.disabled_tools.len();
        if count == 0 {
            return div().into_any_element();
        }
        settings_ai_disabled_tools_notice(
            &self.tokens,
            self.i18n
                .t("settings_view.ai.tool_use_disabled_tools_title")
                .replace("{{count}}", &count.to_string()),
            self.workspace_toolbar_action_button(
                self.i18n
                    .t("settings_view.ai.tool_use_restore_disabled_tools"),
                None,
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                    ..ToolbarButtonOptions::default()
                },
                cx.listener(|this, _event, _window, cx| {
                    this.edit_settings(|settings| settings.ai.tool_use.disabled_tools.clear(), cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element(),
        )
    }
}

pub(in crate::workspace) fn acp_agent_auth_status_key(
    status: &oxideterm_settings::AcpAgentAuthStatus,
) -> &'static str {
    match status {
        oxideterm_settings::AcpAgentAuthStatus::Unknown => {
            "settings_view.ai.acp_agent_auth_unknown"
        }
        oxideterm_settings::AcpAgentAuthStatus::NotRequired => {
            "settings_view.ai.acp_agent_auth_not_required"
        }
        oxideterm_settings::AcpAgentAuthStatus::Required => {
            "settings_view.ai.acp_agent_auth_required"
        }
        oxideterm_settings::AcpAgentAuthStatus::Authenticated => {
            "settings_view.ai.acp_agent_auth_authenticated"
        }
        oxideterm_settings::AcpAgentAuthStatus::Expired => {
            "settings_view.ai.acp_agent_auth_expired"
        }
    }
}

pub(in crate::workspace) fn acp_agent_runtime_status_key(
    status: &oxideterm_settings::AcpAgentRuntimeState,
) -> &'static str {
    match status {
        oxideterm_settings::AcpAgentRuntimeState::Unknown => {
            "settings_view.ai.acp_agent_status_unknown"
        }
        oxideterm_settings::AcpAgentRuntimeState::Ready => {
            "settings_view.ai.acp_agent_status_ready"
        }
        oxideterm_settings::AcpAgentRuntimeState::AuthRequired => {
            "settings_view.ai.acp_agent_status_auth_required"
        }
        oxideterm_settings::AcpAgentRuntimeState::Error => {
            "settings_view.ai.acp_agent_status_error"
        }
    }
}

pub(in crate::workspace) fn acp_agent_error_kind_key(kind: &str) -> &'static str {
    match kind {
        "command_not_found" => "settings_view.ai.acp_agent_error_command_not_found",
        "config" => "settings_view.ai.acp_agent_error_config",
        "initialize" => "settings_view.ai.acp_agent_error_initialize",
        _ => "settings_view.ai.acp_agent_error_unknown",
    }
}

pub(in crate::workspace) fn ai_acp_launch_config_from_settings(
    agent: &oxideterm_settings::AcpAgentConfig,
) -> oxideterm_ai::AcpLaunchConfig {
    oxideterm_ai::AcpLaunchConfig {
        id: agent.id.clone(),
        display_name: agent.display_name.clone(),
        command: agent.command.clone(),
        args: agent.args.clone(),
        env: agent.env.clone(),
        cwd: agent.cwd.as_ref().map(std::path::PathBuf::from),
    }
}

pub(in crate::workspace) fn ai_acp_capability_policy_from_settings(
    policy: &oxideterm_settings::AcpAgentCapabilityPolicy,
) -> oxideterm_ai::AcpHostCapabilityPolicy {
    oxideterm_ai::AcpHostCapabilityPolicy {
        fs_read_text_file: policy.fs_read_text_file,
        fs_write_text_file: policy.fs_write_text_file,
        terminal: policy.terminal,
    }
}

pub(in crate::workspace) fn ai_acp_probe_error_result(kind: &'static str) -> AcpAgentProbeResult {
    // Probe failures store only stable categories. Raw process errors can
    // contain command args, env values, or auth material from local agents.
    AcpAgentProbeResult {
        runtime_state: oxideterm_settings::AcpAgentRuntimeState::Error,
        auth_status: oxideterm_settings::AcpAgentAuthStatus::Unknown,
        last_error_kind: Some(kind.to_string()),
    }
}
