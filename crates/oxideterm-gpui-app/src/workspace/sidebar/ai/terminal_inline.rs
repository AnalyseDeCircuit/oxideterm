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

        let pane = self.tab_host.read(cx).panes().get(&pane_id).cloned();
        let budget = self.ai_ambient_context_budget();
        self.ai.state.update(cx, |ai, cx| ai.open_inline_for_pane((pane_id, session_id), pane.as_ref(), budget, cx));

        window.focus(&self.focus_handle, cx);
        let config = self.resolve_terminal_ai_inline_config();
        self.ai.state.update(cx, |ai, _| ai.refresh_terminal_inline_key_status(config));
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
        let kind = pane.read(cx).session_kind();
        self.ai.state.update(cx, |ai, cx| {
            ai.submit_inline_for_terminal(regenerate, kind, &self.settings_store.settings().ai, &self.i18n, cx);
        });
    }

    pub(in crate::workspace) fn insert_terminal_ai_inline_response(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_terminal_ai_inline_response(false, window, cx);
    }

    pub(in crate::workspace) fn execute_terminal_ai_inline_response(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_terminal_ai_inline_response(true, window, cx);
    }

    fn apply_terminal_ai_inline_response(&mut self, execute: bool, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.terminal_ai_inline_target_pane(cx);
        let sent = self.ai.state.read(cx).inline_response_command();
        if let Some(command) = sent {
            oxideterm_gpui_ai::inline::apply_inline_command(pane.as_ref(), command, execute, cx);
            self.close_terminal_ai_inline_panel(window, cx);
        }
    }

    pub(in crate::workspace) fn resolve_terminal_ai_inline_config(&self) -> Result<AiChatStreamConfig, String> {
        oxideterm_gpui_ai::inline::resolve_terminal_ai_inline_config(&self.settings_store.settings().ai, &self.i18n)
    }

}
