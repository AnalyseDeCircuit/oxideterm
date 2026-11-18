use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_schedules_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::ScheduleSearch, cx)
            .expect("schedule search is a non-secret Host Tools input");
        let sidebar_width = self.ai.state.read(cx).chat_ui().sidebar_width;
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.host_tools_selectable_text(cx);
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_schedules_panel(
                search_ime,
                sidebar_width,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn handle_host_schedule_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.host_tools.read(cx).schedule_search_is_focused() {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.host_tools.update(cx, |host_tools, cx| {
                host_tools.clear_schedule_search_focus(cx);
            });
            // Selection and marked text remain window-owned IME coordination.
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(in crate::workspace) fn handle_host_schedule_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.host_tools.read(cx).schedule_confirm_is_open() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_schedule_confirm_exit(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_schedule_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    fn confirm_host_schedule_action(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let exit_delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_schedule_action_from_view(exit_delay, cx);
        });
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_schedule_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let exit_delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_schedule_confirm_exit(exit_delay, cx)
        })
    }

    pub(in crate::workspace) fn render_host_schedule_confirm_dialog(
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
            host_tools.render_host_schedule_confirm_dialog(
                &tokens,
                i18n,
                focused_action,
                exit_delay,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn render_host_schedule_logs_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let connection_id = self.host_tools.read(cx).schedule_logs_connection_id()?;
        let follow_terminal_available = self.host_tools_terminal_available(connection_id);
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_schedule_logs_dialog(
                follow_terminal_available,
                &tokens,
                i18n,
                mono_font_family,
                cx,
            )
        })
    }
}
