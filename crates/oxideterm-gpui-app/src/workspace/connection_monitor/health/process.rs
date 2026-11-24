use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_processes_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.host_tools_selectable_text(cx);
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::ProcessSearch, cx)
            .expect("process search is a non-secret Host Tools input");
        let renice_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::ProcessRenice, cx)
            .expect("process renice is a non-secret Host Tools input");
        let sidebar_width = self.rails.right.width();
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_processes_panel(
                search_ime,
                renice_ime,
                sidebar_width,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn handle_host_process_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let focused_input = self.host_tools.read(cx).ui.focused_input;
        if !matches!(
            focused_input,
            Some(HostToolsTextInput::ProcessSearch | HostToolsTextInput::ProcessRenice)
        ) {
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

    pub(in crate::workspace) fn handle_host_process_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.host_tools.read(cx).process_confirm_is_open() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_process_confirm_exit(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_process_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn confirm_host_process_action(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let runtime = self.forwarding_runtime.handle().clone();
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_process_action(delay, runtime, cx)
        });
        for notice in notices {
            self.push_host_tools_notice(notice, cx);
        }
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_process_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_process_confirm_exit(delay, cx)
        })
    }

    pub(in crate::workspace) fn render_host_process_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let focused_action = self.standard_confirm_focus();
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_process_confirm_dialog(&tokens, i18n, focused_action, cx)
        })
    }
}
