use super::*;
use oxideterm_gpui_host_tools::*;
use oxideterm_gpui_ui::text_input::text_input_anchor_probe;

impl WorkspaceApp {
    pub(super) fn render_host_container_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.host_tools_selectable_text(cx);
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::ContainerSearch, cx)
            .expect("container search is a non-secret Host Tools input");
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_container_panel(
                search_ime,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
    }

    pub(in crate::workspace) fn handle_host_container_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::ContainerSearch)
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

    pub(in crate::workspace) fn handle_host_container_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_tools.read(cx).container_confirm_view().is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.begin_host_container_confirm_exit(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.clear_standard_confirm_focus();
                let delay = oxideterm_gpui_ui::motion::duration(
                    &self.tokens,
                    oxideterm_gpui_ui::motion::MotionDuration::Control,
                );
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.confirm_container_action_from_view(delay, cx);
                });
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    /// Escape closes and Enter submits the container input dialog. IME
    /// composition owns Enter before this runs (`defer_active_ime_key`), so a
    /// candidate commit never submits.
    pub(in crate::workspace) fn handle_host_container_input_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .host_tools
            .read(cx)
            .ui
            .host_container_input_dialog
            .is_none()
        {
            return false;
        }
        if event.keystroke.modifiers.platform {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_container_input_dialog(cx);
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                true
            }
            "enter" => {
                self.submit_host_container_input_dialog(window, cx);
                true
            }
            _ => false,
        }
    }

    fn begin_host_container_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.begin_container_confirm_exit(delay, cx)
        })
    }

    pub(in crate::workspace) fn render_host_container_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (request, phase) = self.host_tools.read(cx).container_confirm_view()?;
        // The two image prune controls share one action; the scope selects the
        // copy so "dangling" never describes an unused-image prune.
        let description_key = if matches!(
            request.kind,
            HostContainerConfirmKind::Image(
                oxideterm_connection_monitor::container::ImageAction::Prune
            )
        ) && request.target_id == IMAGE_PRUNE_ALL_SCOPE
        {
            "sidebar.host_container.confirms.prune_all_images"
        } else {
            confirm_description_key(request.kind)
        };
        let description = self
            .i18n
            .t(description_key)
            .replace("{{name}}", &request.label);
        let destructive = kind_is_destructive(request.kind);
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                &self.tokens,
                "host-container-confirm-motion",
                phase,
                ConfirmDialogView {
                    variant: if destructive {
                        ConfirmDialogVariant::Danger
                    } else {
                        ConfirmDialogVariant::Default
                    },
                    title: div()
                        .child(self.i18n.t("sidebar.host_container.confirm.title"))
                        .into_any_element(),
                    description: Some(div().child(description).into_any_element()),
                    cancel_label: div()
                        .child(self.i18n.t("sidebar.host_container.confirm.cancel"))
                        .into_any_element(),
                    confirm_label: div()
                        .child(self.i18n.t(confirm_label_key(request.kind)))
                        .into_any_element(),
                },
                self.standard_confirm_focus(),
                cx.listener(|this, _event, _window, cx| {
                    this.begin_host_container_confirm_exit(cx);
                }),
                cx.listener(|this, _event, _window, cx| {
                    let delay = oxideterm_gpui_ui::motion::duration(
                        &this.tokens,
                        oxideterm_gpui_ui::motion::MotionDuration::Control,
                    );
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.confirm_container_action_from_view(delay, cx);
                    });
                }),
            )
            .into_any_element(),
        )
    }
}
impl WorkspaceApp {
    pub(super) fn render_container_run_spec_area(
        &self,
        target: WorkspaceImeTarget,
        input: HostToolsTextInput,
        value: &str,
        placeholder: String,
        focused: bool,
        scroll_id: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let line_height = crate::workspace::ime::HOST_REDIS_VALUE_EDITOR_LINE_HEIGHT;
        let workspace = cx.entity();
        let mut lines = div()
            .w_full()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(0.0))
            .text_size(px(12.0))
            .line_height(px(line_height))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_color(if value.is_empty() {
                rgb(theme.text_muted)
            } else {
                rgb(theme.text)
            });
        if value.is_empty() {
            lines = lines.child(placeholder);
        } else {
            lines = self.render_settings_multiline_textarea_lines(
                lines,
                target,
                value,
                false,
                line_height,
                cx,
            );
        }
        let content = lines
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.host_tools.update(cx, |host_tools, _cx| {
                        host_tools.ui.focus_input(input);
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
            }));
        let surface =
            text_input_anchor_probe(target.anchor_id(), content, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            });
        div()
            .id(scroll_id)
            .w_full()
            .h(px(66.0))
            .overflow_y_scroll()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(if focused {
                (theme.accent << 8) | 0x99
            } else {
                (theme.border << 8) | 0xFF
            }))
            .bg(rgb(theme.bg))
            .px_3()
            .py_2()
            .cursor(CursorStyle::IBeam)
            .child(surface)
            .into_any_element()
    }

    pub(super) fn render_container_dialog_text_field(
        &self,
        target: WorkspaceImeTarget,
        input: HostToolsTextInput,
        value: &str,
        placeholder: String,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = self
            .workspace_text_input(
                target,
                TextInputView {
                    value,
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.host_tools.update(cx, |host_tools, _cx| {
                        host_tools.ui.focus_input(input);
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
            }));
        let workspace = cx.entity();
        text_input_anchor_probe(target.anchor_id(), control, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        })
        .into_any_element()
    }

    pub(in crate::workspace) fn submit_host_container_input_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (kind, registry, value, run) = {
            let host_tools = self.host_tools.read(cx);
            let Some(dialog) = host_tools.ui.host_container_input_dialog.as_ref() else {
                return;
            };
            (
                dialog.kind.clone(),
                dialog.registry.trim().to_string(),
                dialog.value.trim().to_string(),
                dialog.run.clone(),
            )
        };
        if value.is_empty() {
            return;
        }
        let opened_notice = self.i18n.t("sidebar.host_container.toast.terminal_opened");
        let missing_notice = self
            .i18n
            .t("sidebar.host_container.toast.connection_missing");
        self.ime_marked_text = None;
        self.clear_ime_selection();
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.dismiss_container_input_dialog(cx);
            host_tools.confirm_container_input_dialog(
                kind,
                registry,
                value,
                run,
                opened_notice,
                missing_notice,
                window,
                cx,
            );
        });
    }
}
