use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_packages_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.host_tools_selectable_text(cx);
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::PackageSearch, cx)
            .expect("package search is a non-secret Host Tools input");
        let sidebar_width = self.ai.state.read(cx).chat_ui().sidebar_width;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_packages_panel(
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

    pub(in crate::workspace) fn handle_host_package_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::PackageSearch)
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
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_host_package_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.host_tools.read(cx).package_confirm_is_open() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.dismiss_host_package_confirm(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_package_uninstall(window, cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(in crate::workspace) fn render_host_package_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (package_name, phase) = self.host_tools.read(cx).package_confirm_label()?;
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let focused_action = self.standard_confirm_focus();
        let description = i18n
            .t("sidebar.host_packages.confirm.uninstall_description")
            .replace("{{name}}", package_name);
        let view = ConfirmDialogView {
            variant: ConfirmDialogVariant::Default,
            title: div()
                .child(i18n.t("sidebar.host_packages.confirm.uninstall_title"))
                .into_any_element(),
            description: Some(div().child(description).into_any_element()),
            cancel_label: div().child(i18n.t("common.cancel")).into_any_element(),
            confirm_label: div()
                .child(i18n.t("sidebar.host_packages.confirm.uninstall"))
                .into_any_element(),
        };
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                &tokens,
                "host-package-confirm-motion",
                phase,
                view,
                focused_action,
                cx.listener(move |workspace, _event, _window, cx| {
                    workspace.dismiss_host_package_confirm(cx);
                }),
                cx.listener(move |workspace, _event, window, cx| {
                    workspace.confirm_host_package_uninstall(window, cx);
                }),
            )
            .into_any_element(),
        )
    }

    fn dismiss_host_package_confirm(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let exit_delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_package_confirm_exit(exit_delay, cx);
        });
    }

    fn confirm_host_package_uninstall(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let exit_delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        let i18n = &self.i18n;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_package_uninstall_in_terminal(i18n, exit_delay, window, cx);
        });
    }
}
