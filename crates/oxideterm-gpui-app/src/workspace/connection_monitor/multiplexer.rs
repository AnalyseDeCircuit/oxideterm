use super::*;

use oxideterm_gpui_ui::button::ButtonVariant;
use oxideterm_gpui_ui::text_input::text_input_anchor_probe;

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_host_mux_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.host_tools.read(cx).mux_confirm_is_open() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_mux_confirm_exit(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_mux_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(in crate::workspace) fn handle_host_mux_input_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).ui.host_mux_input_dialog.is_none() {
            return false;
        }
        if event.keystroke.modifiers.platform {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_mux_input_dialog(cx);
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                true
            }
            "enter" => {
                self.submit_host_mux_input_dialog(window, cx);
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace) fn handle_host_mux_read_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.host_tools.read(cx).mux_read_is_open() {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" {
            self.host_tools.update(cx, |host_tools, cx| {
                host_tools.dismiss_mux_read_dialog(cx);
            });
            // The viewer has no input of its own, so an unfinished composition
            // would otherwise commit into whatever field held focus before it.
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
        }
        true
    }

    pub(super) fn confirm_host_mux_action(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_mux_action_from_view(delay, cx);
        });
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_mux_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_mux_confirm_exit(delay, cx)
        })
    }

    pub(super) fn submit_host_mux_input_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let i18n = &self.i18n;
        let handled = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.submit_mux_input(i18n, window, cx)
        });
        if handled {
            self.ime_marked_text = None;
            self.clear_ime_selection();
        }
    }

    pub(in crate::workspace) fn render_host_mux_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let focused_action = self.standard_confirm_focus();
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_mux_confirm_view(&tokens, i18n, focused_action, cx)
        })
    }

    pub(in crate::workspace) fn render_host_mux_input_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let target = WorkspaceImeTarget::HostMuxDialog;
        let (kind, submit_disabled, error, input_control) = {
            let host_tools = self.host_tools.read(cx);
            let ui = &host_tools.ui;
            let dialog = ui.host_mux_input_dialog.as_ref()?;
            let input_control = self
                .workspace_text_input(
                    target,
                    TextInputView {
                        value: dialog.value.as_str(),
                        placeholder: self.i18n.t(mux_input_placeholder_key(&dialog.kind)),
                        focused: ui.input_is_focused(HostToolsTextInput::MuxDialog),
                        caret_visible: self.input_caret.visible(),
                        secret: false,
                        selected_all: false,
                        selected_range: self.ime_selected_range_for_target(target, cx),
                        marked_text: self.marked_text_for_target(target, cx),
                    },
                )
                .h(px(34.0))
                .cursor(CursorStyle::IBeam);
            (
                dialog.kind.clone(),
                dialog.value.trim().is_empty() || host_tools.mux_action_running(),
                dialog.error,
                input_control,
            )
        };
        let workspace = cx.entity();
        let input_control = text_input_anchor_probe(
            target.anchor_id(),
            input_control
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.host_tools.update(cx, |host_tools, _cx| {
                            host_tools.ui.focus_input(HostToolsTextInput::MuxDialog);
                        });
                        this.ime_marked_text = None;
                        this.show_active_input_caret(cx);
                        window.focus(&this.focus_handle, cx);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element();
        let cancel_action = self.workspace_confirm_footer_action_button(
            self.i18n.t("sidebar.multiplexer.confirm.cancel"),
            ButtonVariant::Secondary,
            ConfirmDialogAction::Cancel,
            false,
            None,
            |this, _event, _window, cx| {
                this.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_mux_input_dialog(cx);
                });
                this.ime_marked_text = None;
                this.clear_ime_selection();
                cx.notify();
            },
            cx,
        );
        let submit_label = match kind {
            MuxInputDialogKind::RunPane { .. } => self.i18n.t("sidebar.multiplexer.actions.send"),
            _ => self.i18n.t("sidebar.multiplexer.confirm.run"),
        };
        let submit_action = self.workspace_confirm_footer_action_button(
            submit_label,
            ButtonVariant::Default,
            ConfirmDialogAction::Confirm,
            submit_disabled,
            None,
            |this, _event, window, cx| {
                this.submit_host_mux_input_dialog(window, cx);
            },
            cx,
        );
        let theme = self.tokens.ui;
        let dialog = div()
            .w(px(560.0))
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t(mux_input_title_key(&kind))),
                    ),
            )
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(input_control)
                    // The refusal is answered where the value was typed, so
                    // the reason and the text stay together.
                    .when_some(error, |body, key| {
                        body.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(MONITOR_RED))
                                .child(self.i18n.t(key)),
                        )
                    }),
            )
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(cancel_action)
                    .child(submit_action),
            );

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_mux_input_dialog(cx);
                        });
                        this.ime_marked_text = None;
                        this.clear_ime_selection();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(dialog))
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_host_mux_read_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_mux_read_dialog(&self.tokens, &self.i18n, mono_font_family, cx)
        })
    }
}
