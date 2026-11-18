impl WorkspaceApp {
    pub(in crate::workspace) fn terminal_ai_inline_target_pane(
        &self,
        cx: &App,
    ) -> Option<Entity<TerminalPane>> {
        let (pane_id, session_id) = self.ai.state.read(cx).terminal_inline_panel().target?;
        self.tab_host
            .read(cx)
            .terminal_pane_for_target(pane_id, session_id)
    }

    pub(in crate::workspace) fn terminal_ai_inline_window(
        &self,
        cx: &App,
    ) -> Option<gpui::WindowId> {
        let (_, session_id) = self.ai.state.read(cx).terminal_inline_panel().target?;
        let host = self.tab_host.read(cx);
        let location = host.terminal_location(session_id)?;
        host.detached_window_handle(location.tab_id)
            .or_else(|| {
                self.window_registry
                    .handle_for_role(crate::workspace::window_registry::WindowRole::Main)
            })
            .map(|handle| handle.window_id())
    }

    pub(in crate::workspace) fn terminal_ai_keybinding_label(&self, id: &str) -> Option<String> {
        crate::keybindings::action_definition(id)
            .and_then(|definition| {
                crate::keybindings::effective_combo(
                    definition,
                    &self.settings_store.settings().keybindings.overrides,
                    crate::keybindings::KeybindingSide::current(),
                )
            })
            .map(|combo| crate::keybindings::format_combo(&combo))
    }

    pub(in crate::workspace) fn toggle_terminal_ai_inline_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ai.state.read(cx).terminal_inline_panel().open {
            self.close_terminal_ai_inline_panel(window, cx);
        } else {
            self.open_terminal_ai_inline_panel(window, cx);
        }
    }

    pub(in crate::workspace) fn open_terminal_ai_inline_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane_id) = self.terminal_pane_for_window(window, cx) else {
            return;
        };
        let Some(session_id) = self.session_id_for_pane(pane_id, cx) else {
            return;
        };
        self.hide_search(pane_id, cx);
        self.close_terminal_command_overlays(cx);
        self.close_ai_model_selector(cx);

        let selection = self
            .tab_host
            .read(cx)
            .panes()
            .get(&pane_id)
            .and_then(|pane| pane.read(cx).selected_text_snapshot())
            .unwrap_or_default();
        let sanitized_selection = truncate_ai_inline_context(
            oxideterm_ai::sanitize_for_ai(&selection),
            self.ai_ambient_context_budget() as i64,
        );
        self.ai.state.update(cx, |ai, _cx| {
            ai.open_terminal_inline_panel(sanitized_selection);
            ai.terminal_inline_panel_mut().target = Some((pane_id, session_id));
        });

        window.focus(&self.focus_handle, cx);
        self.refresh_terminal_ai_inline_key_status(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_ai_inline_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = self.terminal_ai_inline_target_pane(cx);
        let own_window =
            self.terminal_ai_inline_window(cx) == Some(window.window_handle().window_id());
        self.ai.state
            .update(cx, |ai, _cx| ai.close_terminal_inline_panel());
        self.ime_marked_text = None;
        self.close_ai_model_selector(cx);
        if own_window && let Some(pane) = pane {
            pane.update(cx, |pane, cx| pane.focus(window, cx));
        }
        cx.notify();
    }

    pub(in crate::workspace) fn handle_ai_inline_panel_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.terminal_ai_inline_window(cx) != Some(window.window_handle().window_id()) {
            return false;
        }
        let (panel_open, panel_loading, response_is_empty) = {
            let panel = self.ai.state.read(cx).terminal_inline_panel();
            (
                panel.open && panel.prompt_focused,
                panel.loading,
                panel.response.trim().is_empty(),
            )
        };
        let submit = crate::keybindings::keystroke_matches_action(
            &event.keystroke,
            "terminal.aiSubmit",
            &self.settings_store.settings().keybindings.overrides,
        );
        let insert = crate::keybindings::keystroke_matches_action(
            &event.keystroke,
            "terminal.aiInsert",
            &self.settings_store.settings().keybindings.overrides,
        );
        if !panel_open || (event.keystroke.modifiers.platform && !submit && !insert) {
            return false;
        }
        if self
            .ai.state
            .read(cx)
            .model_selector_is_open(AiModelSelectorScope::TerminalInline)
            && self.ai.state.read(cx).model_selector_search_focused()
        {
            return self.handle_ai_sidebar_key(event, cx);
        }

        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_terminal_ai_inline_panel(window, cx);
                true
            }
            "enter"
                if self
                    .marked_text_for_target(WorkspaceImeTarget::AiInlinePrompt, cx)
                    .is_some() =>
            {
                true
            }
            _ if submit => {
                if panel_loading {
                    return true;
                }
                if response_is_empty {
                    self.send_terminal_ai_inline_prompt(cx);
                } else {
                    self.execute_terminal_ai_inline_response(window, cx);
                }
                true
            }
            _ if insert && !response_is_empty && !panel_loading => {
                self.insert_terminal_ai_inline_response(window, cx);
                true
            }
            _ => true,
        }
    }

    pub(in crate::workspace) fn send_terminal_ai_inline_prompt(&mut self, cx: &mut Context<Self>) {
        let Some((prompt, selection)) = self.ai.state.read(cx).terminal_inline_request_context()
        else {
            return;
        };
        let Some(pane) = self.terminal_ai_inline_target_pane(cx) else {
            return;
        };
        let messages = terminal_ai_inline_messages(
            terminal_ai_inline_os_context(pane.read(cx).session_kind()),
            selection,
            prompt,
        );
        let config_result = self.resolve_terminal_ai_inline_config();
        let api_key_not_found = self.i18n.t("ai.model_selector.api_key_not_found");
        let failed_to_get_key = self.i18n.t("ai.model_selector.failed_to_get_api_key");
        let stream_failed = self.i18n.t("settings_view.ai.acp_agent_error_unknown");
        self.ai.state.update(cx, |ai, _cx| {
            ai.request_terminal_inline(
                config_result,
                messages,
                api_key_not_found,
                failed_to_get_key,
                stream_failed,
            );
        });
        cx.notify();
    }

    pub(in crate::workspace) fn regenerate_terminal_ai_inline_response(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.ai.state.read(cx).terminal_inline_panel().loading {
            return;
        }
        self.ai.state.update(cx, |ai, _cx| {
            let panel = ai.terminal_inline_panel_mut();
            panel.response.clear();
            panel.error = None;
        });
        self.send_terminal_ai_inline_prompt(cx);
    }

    pub(in crate::workspace) fn insert_terminal_ai_inline_response(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = extract_terminal_ai_inline_command(
            &self.ai.state.read(cx).terminal_inline_panel().response,
        );
        if command.trim().is_empty() {
            return;
        }
        if let Some(pane) = self.terminal_ai_inline_target_pane(cx) {
            let _ = pane.update(cx, |pane, cx| {
                pane.send_ai_input_bytes(command.as_bytes(), cx);
            });
        }
        self.close_terminal_ai_inline_panel(window, cx);
    }

    pub(in crate::workspace) fn execute_terminal_ai_inline_response(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = extract_terminal_ai_inline_command(
            &self.ai.state.read(cx).terminal_inline_panel().response,
        );
        if command.trim().is_empty() {
            return;
        }
        if let Some(pane) = self.terminal_ai_inline_target_pane(cx) {
            let _ = pane.update(cx, |pane, cx| {
                pane.begin_command_mark(
                    &command,
                    oxideterm_terminal::TerminalCommandMarkDetectionSource::Ai,
                    cx,
                );
                pane.send_command_line(&command, cx);
            });
        }
        self.close_terminal_ai_inline_panel(window, cx);
    }

    pub(in crate::workspace) fn refresh_terminal_ai_inline_key_status(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let config_result = self.resolve_terminal_ai_inline_config();
        self.ai.state.update(cx, |ai, _cx| {
            ai.refresh_terminal_inline_key_status(config_result);
        });
    }

    pub(in crate::workspace) fn resolve_terminal_ai_inline_config(
        &self,
    ) -> Result<AiChatStreamConfig, String> {
        let settings = self.settings_store.settings();
        let providers = ai_provider_views(&settings.ai.providers);
        let provider = active_provider_view(&providers, settings.ai.active_provider_id.as_deref())
            .cloned()
            .ok_or_else(|| self.i18n.t("ai.model_selector.no_provider"))?;
        let model = active_model_selection(settings.ai.active_model.as_deref())
            .ok_or_else(|| self.i18n.t("ai.model_selector.no_model_selected"))?;
        let reasoning_effort = settings
            .ai
            .reasoning_model_overrides
            .get(&provider.id)
            .and_then(|models| models.get(&model))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("auto");
        let reasoning_effort = oxideterm_ai::normalize_reasoning_level_for_model(
            &provider.provider_type,
            &model,
            reasoning_effort,
        )
        .as_str()
        .to_string();
        Ok(AiChatStreamConfig {
            api_protocol: provider.api_protocol,
            image_input: Default::default(),
            omit_history_images: false,
            execution_backend: AiExecutionBackend::Provider,
            provider_id: Some(provider.id.clone()),
            acp_agent_id: None,
            acp_session_id: None,
            acp_config_selection: None,
            provider_type: provider.provider_type,
            base_url: provider.base_url,
            model: model.clone(),
            api_key: None,
            max_response_tokens: None,
            reasoning_effort: Some(reasoning_effort),
            safety_mode: AiPolicySafetyMode::Default,
            profile_id: None,
            memory_context: None,
            memory_entry_ids: Vec::new(),
            tool_policy: AiToolUsePolicy::default(),
            tools: Vec::new(),
            tool_choice: oxideterm_ai::AiToolChoice::Auto,
        })
    }
}

pub(in crate::workspace) fn terminal_ai_inline_os_context(
    kind: oxideterm_terminal::TerminalSessionKind,
) -> String {
    let local_os = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "Unknown"
    };
    match kind {
        oxideterm_terminal::TerminalSessionKind::SshPty => {
            format!("SSH terminal (remote OS unknown, local: {local_os})")
        }
        oxideterm_terminal::TerminalSessionKind::Mosh => {
            format!("Mosh terminal (remote OS unknown, local: {local_os})")
        }
        oxideterm_terminal::TerminalSessionKind::LocalPty => {
            format!("Local terminal on {local_os}")
        }
        oxideterm_terminal::TerminalSessionKind::Telnet => "Telnet (remote OS unknown)".into(),
        oxideterm_terminal::TerminalSessionKind::Serial => {
            "Serial device (command interface unknown)".into()
        }
        oxideterm_terminal::TerminalSessionKind::Ble => {
            "BLE device (command interface unknown)".into()
        }
    }
}

pub(in crate::workspace) fn terminal_ai_inline_messages(
    os_context: String,
    selection_context: String,
    prompt: String,
) -> Vec<AiChatMessage> {
    let mut user_content = String::new();
    if !selection_context.trim().is_empty() {
        user_content.push_str("### Context (Selected Text):\n");
        user_content.push_str(&selection_context);
        user_content.push_str("\n\n");
    }
    user_content.push_str("### Question/Instruction:\n");
    user_content.push_str(&prompt);

    vec![
        AiChatMessage {
            id: "terminal-inline-system".to_string(),
            role: AiChatRole::System,
            content: format!(
                "You are OxideSens, an expert terminal assistant. Environment: {os_context}. Respond ONLY with the command or code itself unless asked for explanation. If asked which AI model you are, answer truthfully."
            ),
            timestamp_ms: 0,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        },
        AiChatMessage {
            id: "terminal-inline-user".to_string(),
            role: AiChatRole::User,
            content: user_content,
            timestamp_ms: 0,
            model: None,
            context: None,
            thinking_content: None,
            is_streaming: false,
            metadata: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            turn: None,
            transcript_ref: None,
            summary_ref: None,
            branches: None,
            suggestions: Vec::new(),
        },
    ]
}

pub(in crate::workspace) fn truncate_ai_inline_context(
    mut context: String,
    max_chars: i64,
) -> String {
    let max_chars = usize::try_from(max_chars).unwrap_or_default();
    if max_chars == 0 || context.chars().count() <= max_chars {
        return context;
    }
    let keep_from = context
        .char_indices()
        .rev()
        .nth(max_chars.saturating_sub(1))
        .map(|(index, _)| index)
        .unwrap_or_default();
    context.drain(..keep_from);
    context
}

use oxideterm_gpui_ai::inline::extract_terminal_ai_inline_command;
