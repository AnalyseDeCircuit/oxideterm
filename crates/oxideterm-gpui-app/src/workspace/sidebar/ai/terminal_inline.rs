use oxideterm_gpui_ai::inline::AiInlineKey;

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
        let budget = self.ai_ambient_context_budget();
        let selection = zeroize::Zeroizing::new(selection);
        self.ai.state.update(cx, |ai, _| ai.start_inline_panel((pane_id, session_id), &selection, budget));

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
        let composing = self.marked_text_for_target(WorkspaceImeTarget::AiInlinePrompt, cx).is_some();
        match self.ai.state.read(cx).inline_panel_key(&event.keystroke, submit, insert, composing) {
            AiInlineKey::Pass => return false,
            AiInlineKey::ModelSearch => return self.handle_ai_sidebar_key(event, cx),
            AiInlineKey::Close => self.close_terminal_ai_inline_panel(window, cx),
            AiInlineKey::Submit => self.send_terminal_ai_inline_prompt(cx),
            AiInlineKey::Execute => self.execute_terminal_ai_inline_response(window, cx),
            AiInlineKey::Insert => self.insert_terminal_ai_inline_response(window, cx),
            AiInlineKey::Handled => {}
        }
        true
    }

    pub(in crate::workspace) fn send_terminal_ai_inline_prompt(&mut self, cx: &mut Context<Self>) {
        self.request_terminal_ai_inline(false, cx);
    }

    pub(in crate::workspace) fn regenerate_terminal_ai_inline_response(&mut self, cx: &mut Context<Self>) {
        self.request_terminal_ai_inline(true, cx);
    }

    fn request_terminal_ai_inline(&self, regenerate: bool, cx: &mut App) {
        let Some(pane) = self.terminal_ai_inline_target_pane(cx) else { return; };
        let os_context = terminal_ai_inline_os_context(pane.read(cx).session_kind());
        self.ai.state.update(cx, |ai, cx| {
            ai.submit_inline_prompt(regenerate, os_context, &self.settings_store.settings().ai, &self.i18n, cx);
        });
    }

    pub(in crate::workspace) fn insert_terminal_ai_inline_response(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = self.ai.state.read(cx).inline_response_command() else {
            return;
        };
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
        let Some(command) = self.ai.state.read(cx).inline_response_command() else {
            return;
        };
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

    pub(in crate::workspace) fn resolve_terminal_ai_inline_config(&self) -> Result<AiChatStreamConfig, String> {
        oxideterm_gpui_ai::inline::resolve_terminal_ai_inline_config(&self.settings_store.settings().ai, &self.i18n)
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
