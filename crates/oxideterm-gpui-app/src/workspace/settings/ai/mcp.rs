use super::*;
use oxideterm_gpui_settings_view::ai::mcp::{AiMcpBindings, AiMcpRequest};

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_mcp_settings_bindings(cx: &Context<Self>) -> AiMcpBindings {
        let owner = cx.weak_entity();
        AiMcpBindings {
            controls: oxideterm_gpui_settings_view::surface::settings_controls_source(
                owner.clone(),
            ),
            snapshots: Rc::new({
                let owner = owner.clone();
                move |cx| {
                    owner.upgrade().map_or_else(Vec::new, |owner| {
                        owner.read(cx).ai.state.read(cx).mcp_registry().snapshots()
                    })
                }
            }),
            request: Rc::new(move |request, _, cx| {
                let _ = owner.update(cx, |app, cx| match request {
                    AiMcpRequest::Open => {
                        let dismissed_select = app.open_settings_select.is_some();
                        app.settings_workspace
                            .update(cx, |state, cx| state.open_ai_mcp(cx));
                        app.settings_views
                            .ai_mcp_dialog
                            .update(cx, |view, cx| view.set_focus(None, cx));
                        app.close_settings_select();
                        app.clear_standard_confirm_focus();
                        if dismissed_select {
                            cx.notify();
                        }
                    }
                    AiMcpRequest::FinishDialog { submit } => app.finish_ai_mcp_dialog(submit, cx),
                    AiMcpRequest::RefreshTools(id) => {
                        app.ai
                            .state
                            .update(cx, |ai, cx| ai.refresh_mcp_tools(id, cx));
                    }
                    AiMcpRequest::Remove(id) => {
                        app.ai
                            .state
                            .update(cx, |ai, cx| ai.remove_mcp_server(id, cx));
                    }
                    AiMcpRequest::SetConnected { id, was_connected } => {
                        if let Some(config) = ai_mcp_configs(app.settings_store.settings())
                            .into_iter()
                            .find(|config| config.id == id)
                        {
                            app.ai.state.update(cx, |ai, cx| {
                                ai.set_mcp_server_connected(config, was_connected, cx)
                            });
                        }
                    }
                    AiMcpRequest::Tick => {
                        app.ai.state.update(cx, |ai, cx| {
                            ai.request_mcp_status_tick(Duration::from_millis(500), cx)
                        });
                    }
                });
            }),
        }
    }
    pub(in crate::workspace) fn handle_ai_mcp_add_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let state = self.settings_workspace.read(cx);
        if state.ai_mcp.draft().is_none() {
            return false;
        }
        let can_add = state.ai_mcp.valid(self.settings_store.settings());
        if self.open_settings_select.is_some()
            || self.focused_settings_input.is_some()
            || state.settings_entity_focused_input().is_some()
        {
            return false;
        }

        let key = event.keystroke.key.as_str();
        let footer_focused = self.standard_confirm_focus_owner().is_some();
        if matches!(key, "enter" | "space" | " ") && !footer_focused {
            return false;
        }

        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.finish_ai_mcp_dialog(false, cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                if can_add {
                    self.finish_ai_mcp_dialog(true, cx);
                } else {
                    // Disabled primary buttons remain in the dialog; restore
                    // focus to the first footer action like a browser footer loop.
                    self.reset_standard_confirm_focus();
                    self.settings_views
                        .ai_mcp_dialog
                        .update(cx, |view, cx| view.set_focus(None, cx));
                }
                true
            }
            Some(ConfirmKeyboardAction::Handled) => {
                self.settings_views.ai_mcp_dialog.update(cx, |view, cx| {
                    view.set_focus(self.standard_confirm_focus(), cx)
                });
                true
            }
            None => false,
        }
    }

    fn finish_ai_mcp_dialog(&mut self, submit: bool, cx: &mut Context<Self>) {
        if submit
            && !self
                .settings_workspace
                .read(cx)
                .ai_mcp
                .valid(self.settings_store.settings())
        {
            return;
        }
        let dismissed_select = self.open_settings_select.is_some();
        self.close_settings_select();
        self.clear_standard_confirm_focus();
        self.ime_marked_text = None;
        self.clear_ime_selection();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.settings_workspace
            .update(cx, |state, cx| state.begin_ai_mcp_exit(submit, delay, cx));
        if dismissed_select {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn submit_ai_mcp_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self
            .settings_workspace
            .update(cx, |state, _| state.take_ai_mcp_submission())
        else {
            return;
        };
        if !oxideterm_settings_model::ai_mcp_draft_valid(&draft, self.settings_store.settings()) {
            self.settings_workspace.update(cx, |state, cx| {
                state.complete_ai_mcp_submission(Some(draft), cx)
            });
            return;
        }
        self.ai
            .state
            .update(cx, |ai, cx| ai.start_mcp_server_add(draft, cx));
    }
}
