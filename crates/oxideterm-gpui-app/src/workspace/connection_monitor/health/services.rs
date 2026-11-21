use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_services_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.host_tools_selectable_text(cx);
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::ServiceSearch, cx)
            .expect("service search is a non-secret Host Tools input");
        let connections = self.monitor_connections(cx);
        let selected_connection_id = self.host_tools.read(cx).selected_connection_id_owned();
        let selected_connection_id = selected_connection_id.as_deref().or_else(|| {
            connections
                .first()
                .map(|connection| connection.connection_id.as_str())
        });
        let follow_terminal_available = selected_connection_id
            .is_some_and(|connection_id| self.host_tools_terminal_available(connection_id));
        let sidebar_width = self.ai.state.read(cx).chat_ui().sidebar_width;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_services_panel(
                search_ime,
                follow_terminal_available,
                sidebar_width,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn handle_host_service_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::ServiceSearch)
        {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.host_tools.update(cx, |host_tools, _cx| {
                host_tools.ui.clear_input_focus();
            });
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(in crate::workspace) fn handle_host_service_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.host_tools.read(cx).service_confirm_is_open() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_service_confirm_exit(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_service_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn confirm_host_service_action(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let runtime = self.forwarding_runtime.handle().clone();
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_service_action(delay, runtime, cx)
        });
        for notice in notices {
            self.push_host_tools_notice(notice, cx);
        }
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_service_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_service_confirm_exit(delay, cx)
        })
    }

    pub(in crate::workspace) fn render_host_service_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let focused_action = self.standard_confirm_focus();
        let exit_delay = oxideterm_gpui_ui::motion::duration(
            &tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_service_confirm_dialog(
                &tokens,
                i18n,
                focused_action,
                exit_delay,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn render_host_service_logs_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let connection_id = self.host_tools.read(cx).service_logs_connection_id()?;
        let follow_terminal_available = self.host_tools_terminal_available(connection_id);
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_service_logs_dialog(
                follow_terminal_available,
                &tokens,
                i18n,
                mono_font_family,
                cx,
            )
        })
    }
}
