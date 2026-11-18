use super::*;

use oxideterm_gpui_ui::button::ButtonVariant;
use oxideterm_gpui_ui::text_input::text_input_anchor_probe;

impl WorkspaceApp {
    pub(super) fn render_host_tmux_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.host_tools_selectable_text(cx);
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::TmuxSearch, cx)
            .expect("tmux search is a non-secret Host Tools input");
        let sidebar_width = self.ai.state.read(cx).chat_ui().sidebar_width;
        // Attach buttons need a terminal target; the Entity cannot see the
        // workspace-owned node router, so the view resolves it once here.
        let terminal_available = self
            .host_tools
            .read(cx)
            .selected_connection_id_owned()
            .is_some_and(|connection_id| self.host_tools_terminal_available(&connection_id));
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_tmux_panel(
                search_ime,
                sidebar_width,
                terminal_available,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn handle_host_tmux_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::TmuxSearch)
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

    pub(in crate::workspace) fn handle_host_tmux_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.host_tools.read(cx).tmux_confirm_is_open() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_tmux_confirm_exit(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_tmux_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(in crate::workspace) fn handle_host_tmux_input_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).ui.host_tmux_input_dialog.is_none() {
            return false;
        }
        if event.keystroke.modifiers.platform {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_tmux_input_dialog(cx);
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                true
            }
            "enter" => {
                self.submit_host_tmux_input_dialog(cx);
                true
            }
            _ => false,
        }
    }

    pub(super) fn confirm_host_tmux_action(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.confirm_tmux_action_from_view(delay, cx);
        });
    }

    /// Keeps the request mounted until the current exit generation completes.
    fn begin_host_tmux_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_tmux_confirm_exit(delay, cx)
        })
    }

    pub(super) fn submit_host_tmux_input_dialog(&mut self, cx: &mut Context<Self>) {
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.submit_tmux_input_from_view(cx);
        });
        self.ime_marked_text = None;
        self.clear_ime_selection();
    }

    pub(in crate::workspace) fn render_host_tmux_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (phase, view) = self
            .host_tools
            .read(cx)
            .render_host_tmux_confirm_view(&self.i18n)?;
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                &self.tokens,
                "host-tmux-confirm-motion",
                phase,
                view,
                self.standard_confirm_focus(),
                cx.listener(|this, _event, _window, cx| {
                    this.begin_host_tmux_confirm_exit(cx);
                }),
                cx.listener(|this, _event, _window, cx| {
                    this.confirm_host_tmux_action(cx);
                }),
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_host_tmux_input_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let target = WorkspaceImeTarget::HostTmuxDialogInput;
        let (kind, session_name, target_label, submit_disabled, input_control) = {
            let host_tools = self.host_tools.read(cx);
            let ui = &host_tools.ui;
            let dialog = ui.host_tmux_input_dialog.as_ref()?;
            let input_control = self
                .workspace_text_input(
                    target,
                    TextInputView {
                        value: dialog.value.as_str(),
                        placeholder: self.i18n.t(host_tmux_input_placeholder_key(&dialog.kind)),
                        focused: ui.input_is_focused(HostToolsTextInput::TmuxDialog),
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
                dialog.session_name.clone(),
                dialog.target_label.clone(),
                dialog.value.trim().is_empty() || host_tools.tmux_action_running(),
                input_control,
            )
        };
        let submit_label = self.i18n.t(host_tmux_input_submit_key(&kind));
        let workspace = cx.entity();
        let input_control = text_input_anchor_probe(
            target.anchor_id(),
            input_control
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.host_tools.update(cx, |host_tools, _cx| {
                            host_tools.ui.focus_input(HostToolsTextInput::TmuxDialog);
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
            self.i18n.t("sidebar.host_tmux.confirm.cancel"),
            ButtonVariant::Secondary,
            ConfirmDialogAction::Cancel,
            false,
            None,
            |this, _event, _window, cx| {
                this.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_tmux_input_dialog(cx);
                });
                this.ime_marked_text = None;
                this.clear_ime_selection();
                cx.notify();
            },
            cx,
        );
        let submit_action = self.workspace_confirm_footer_action_button(
            submit_label,
            ButtonVariant::Default,
            ConfirmDialogAction::Confirm,
            submit_disabled,
            None,
            |this, _event, _window, cx| {
                this.submit_host_tmux_input_dialog(cx);
            },
            cx,
        );
        let dialog = self
            .host_tools
            .read(cx)
            .render_host_tmux_input_dialog_shell(
                kind,
                session_name,
                target_label,
                input_control,
                cancel_action,
                submit_action,
                &self.tokens,
                &self.i18n,
            );

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.dismiss_tmux_input_dialog(cx);
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
}
