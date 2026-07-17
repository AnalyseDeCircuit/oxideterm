use gpui::prelude::*;

use super::helpers::{
    ForwardButtonVariant, forward_type_label, forwards_palette_alpha, forwards_theme_border,
    forwards_theme_border_half, forwards_theme_panel_bg, forwards_theme_sunken_bg,
    forwards_theme_with_alpha, forwards_transparent,
};
use super::{
    AnyElement, ConfirmDialogVariant, ConfirmDialogView, Context, FORWARDS_TW_ALPHA_30,
    FORWARDS_TW_ALPHA_50, ForwardInput, ForwardType, LucideIcon, MouseButton, NodeId, TW_BLACK,
    TabId, TextInputView, WorkspaceApp, WorkspaceImeTarget, confirm_dialog, div, px, rgb,
    settings_mono_font_family, text_input, text_input_anchor_probe,
};

impl WorkspaceApp {
    pub(super) fn render_forward_create_form(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let form_visible = self.forwarding_view.new_form_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        let form = div()
            .relative()
            .rounded(px(self.tokens.radii.xs))
            .border_1()
            .border_color(forwards_theme_border(theme.border, has_background))
            .bg(forwards_theme_with_alpha(
                theme.bg_panel,
                FORWARDS_TW_ALPHA_30,
            ))
            .p_4()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(
                                self.render_forward_ui_text(self.i18n.t("forwards.form.new_title")),
                            ),
                    )
                    .child(
                        self.render_forward_button(
                            self.i18n.t("forwards.form.cancel"),
                            None,
                            ForwardButtonVariant::Ghost,
                            true,
                            has_background,
                            cx.listener(|this, _event, _window, cx| {
                                this.begin_forward_create_form_exit(cx);
                                this.forwarding_view.error = None;
                                cx.stop_propagation();
                            }),
                        )
                        .h(px(32.0))
                        .px_3()
                        .text_size(px(self.tokens.metrics.ui_text_xs)),
                    ),
            )
            .child(self.render_forward_type_picker(has_background, cx))
            .child(self.render_forward_address_form(false, has_background, cx))
            .when(
                self.forwarding_view.forward_type != ForwardType::Dynamic,
                |form| form.child(self.render_forward_skip_health_check(has_background, cx)),
            )
            .when_some(self.forwarding_view.error.as_ref(), |form, error| {
                form.child(self.render_forwards_error(error))
            })
            .child(div().flex().justify_end().child(self.render_forward_button(
                if self.forwarding_view.pending {
                    if self.forwarding_view.skip_health_check {
                        self.i18n.t("forwards.form.creating")
                    } else {
                        self.i18n.t("forwards.form.checking_port")
                    }
                } else {
                    self.i18n.t("forwards.form.create_forward")
                },
                None,
                ForwardButtonVariant::Primary,
                !self.forwarding_view.pending,
                has_background,
                cx.listener(move |this, _event, _window, cx| {
                    this.submit_forward_create(tab_id, node_id.clone(), cx);
                    cx.stop_propagation();
                }),
            )))
            .when(!form_visible, |form| {
                form.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        }),
                )
            });

        oxideterm_gpui_ui::motion::form_transition(
            &self.tokens,
            "forward-create-form-enter",
            form,
            form_visible,
        )
    }

    pub(in crate::workspace) fn render_forward_edit_modal(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(editing) = self.forwarding_view.editing_forward.as_ref() else {
            return div().into_any_element();
        };
        let form_visible = self.forwarding_view.edit_form_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;

        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(forwards_palette_alpha(TW_BLACK, FORWARDS_TW_ALPHA_50))
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "forward-edit-form-enter",
                div()
                    .w(px(500.0))
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(forwards_theme_border(theme.border, has_background))
                    .bg(forwards_theme_panel_bg(theme.bg_panel, has_background))
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(self.render_forward_ui_text(
                                        self.i18n.t("forwards.form.edit_title"),
                                    )),
                            )
                            .child(self.render_forward_icon_button(
                                LucideIcon::X,
                                theme.text_muted,
                                has_background,
                                |this, _event, _window, cx| {
                                    this.begin_forward_edit_form_exit(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{}:", self.i18n.t("forwards.form.type")))
                            .child(forward_type_label(editing.clone(), &self.i18n))
                            .child("|")
                            .child("ID:")
                            .child(
                                div()
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    ))
                                    .child(format!(
                                        "{}...",
                                        editing.id.chars().take(8).collect::<String>()
                                    )),
                            ),
                    )
                    .child(self.render_forward_address_form(true, has_background, cx))
                    .when_some(self.forwarding_view.error.as_ref(), |modal, error| {
                        modal.child(self.render_forwards_error(error))
                    })
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(self.render_forward_button(
                                self.i18n.t("forwards.form.cancel"),
                                None,
                                ForwardButtonVariant::Ghost,
                                true,
                                has_background,
                                cx.listener(|this, _event, _window, cx| {
                                    this.begin_forward_edit_form_exit(cx);
                                    cx.stop_propagation();
                                }),
                            ))
                            .child(self.render_forward_button(
                                self.i18n.t("forwards.form.save_changes"),
                                None,
                                ForwardButtonVariant::Primary,
                                !self.forwarding_view.pending,
                                has_background,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.submit_forward_edit(tab_id, node_id.clone(), cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
                form_visible,
            ))
            .when(!form_visible, |backdrop| {
                backdrop.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        }),
                )
            })
            .into_any_element()
    }

    pub(super) fn begin_forward_create_form_exit(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self.forwarding_view.new_form_presence.begin_exit() else {
            return false;
        };
        self.forwarding_view.focused_input = None;
        self.schedule_forward_form_exit(generation, true, cx);
        true
    }

    pub(super) fn begin_forward_edit_form_exit(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self.forwarding_view.edit_form_presence.begin_exit() else {
            return false;
        };
        self.forwarding_view.focused_input = None;
        self.schedule_forward_form_exit(generation, false, cx);
        true
    }

    fn schedule_forward_form_exit(
        &mut self,
        generation: u64,
        create_form: bool,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if delay.is_zero() {
            self.finish_forward_form_exit(generation, create_form);
            cx.notify();
            return;
        }
        cx.spawn(async move |weak, cx| {
            gpui::Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.finish_forward_form_exit(generation, create_form) {
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_forward_form_exit(&mut self, generation: u64, create_form: bool) -> bool {
        let presence = if create_form {
            &mut self.forwarding_view.new_form_presence
        } else {
            &mut self.forwarding_view.edit_form_presence
        };
        if !presence.finish_exit(generation) {
            return false;
        }
        presence.reopen();
        if create_form {
            self.forwarding_view.show_new_form = false;
        } else {
            self.forwarding_view.editing_forward = None;
        }
        true
    }

    pub(in crate::workspace) fn render_forward_delete_confirm(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(rule) = self.forwarding_view.pending_delete_forward.as_ref() else {
            return div().into_any_element();
        };
        let forward_id = rule.id.clone();
        let confirm_id = forward_id;
        confirm_dialog(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: self
                    .render_forward_ui_text(self.i18n.t("forwards.actions.confirm_delete_title"))
                    .into_any_element(),
                description: Some(
                    self.render_forward_ui_text(
                        self.i18n.t("forwards.actions.confirm_delete_desc"),
                    )
                    .into_any_element(),
                ),
                cancel_label: self
                    .render_forward_ui_text(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: self
                    .render_forward_ui_text(self.i18n.t("common.actions.confirm"))
                    .into_any_element(),
            },
            cx.listener(|this, _event, _window, cx| {
                this.forwarding_view.pending_delete_forward = None;
                cx.notify();
                cx.stop_propagation();
            }),
            cx.listener(move |this, _event, _window, cx| {
                this.forwarding_view.pending_delete_forward = None;
                let registry = this.forwarding_registry.clone();
                let delete_id = confirm_id.clone();
                this.start_forward_operation(
                    tab_id,
                    node_id.clone(),
                    "forwards.messages.deleted",
                    true,
                    move |manager| {
                        Box::pin(async move {
                            manager.delete_forward(&delete_id).await?;
                            let _ = registry.delete_persisted_forward(&delete_id);
                            Ok(())
                        })
                    },
                    cx,
                );
                cx.stop_propagation();
            }),
        )
    }

    fn render_forward_type_picker(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .gap(px(16.0))
            .child(self.render_forward_type_choice(
                ForwardType::Local,
                "forwards.form.type_local",
                has_background,
                cx,
            ))
            .child(self.render_forward_type_choice(
                ForwardType::Remote,
                "forwards.form.type_remote",
                has_background,
                cx,
            ))
            .child(self.render_forward_type_choice(
                ForwardType::Dynamic,
                "forwards.form.type_dynamic",
                has_background,
                cx,
            ))
            .into_any_element()
    }

    fn render_forward_type_choice(
        &self,
        forward_type: ForwardType,
        label_key: &'static str,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected = self.forwarding_view.forward_type == forward_type;
        let label = self.i18n.t(label_key);
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .size(px(14.0))
                    .rounded_full()
                    .border_1()
                    .border_color(if selected {
                        rgb(theme.accent)
                    } else {
                        forwards_theme_border(theme.border, has_background)
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(selected, |radio| {
                        radio.child(div().size(px(8.0)).rounded_full().bg(rgb(theme.accent)))
                    }),
            )
            .child(self.render_forward_ui_text(label))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.forwarding_view.forward_type = forward_type;
                    this.forwarding_view.error = None;
                    if forward_type == ForwardType::Dynamic {
                        this.forwarding_view.skip_health_check = false;
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_forward_skip_health_check(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let checked = self.forwarding_view.skip_health_check;
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px_2()
            .cursor_pointer()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .size(px(14.0))
                    .rounded(px(self.tokens.radii.xs))
                    .border_1()
                    .border_color(if checked {
                        rgb(theme.accent)
                    } else {
                        forwards_theme_border(theme.border, has_background)
                    })
                    .bg(if checked {
                        rgb(theme.accent)
                    } else {
                        forwards_transparent()
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(checked, |checkbox| {
                        checkbox.child(Self::render_lucide_icon(
                            LucideIcon::Check,
                            11.0,
                            rgb(theme.accent_text),
                        ))
                    }),
            )
            .child(self.render_forward_ui_text(self.i18n.t("forwards.form.skip_check")))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.forwarding_view.skip_health_check =
                        !this.forwarding_view.skip_health_check;
                    this.forwarding_view.error = None;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_forward_address_form(
        &self,
        editing: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let forward_type = if editing {
            self.forwarding_view
                .editing_forward
                .as_ref()
                .map(|rule| rule.forward_type)
                .unwrap_or(ForwardType::Local)
        } else {
            self.forwarding_view.forward_type
        };

        div()
            .flex()
            .items_center()
            .gap(px(16.0))
            .p_4()
            .rounded(px(self.tokens.radii.xs))
            .border_1()
            .border_color(forwards_theme_border_half(theme.border, has_background))
            .bg(forwards_theme_sunken_bg(theme.bg_sunken, has_background))
            .child(self.render_forward_address_side(
                if forward_type == ForwardType::Remote {
                    self.i18n.t("forwards.form.remote_server")
                } else {
                    self.i18n.t("forwards.form.local_client")
                },
                if editing {
                    ForwardInput::EditBindAddress
                } else {
                    ForwardInput::CreateBindAddress
                },
                if editing {
                    ForwardInput::EditBindPort
                } else {
                    ForwardInput::CreateBindPort
                },
                cx,
            ))
            .child(
                div()
                    .pt(px(22.0))
                    .text_size(px(18.0))
                    .text_color(rgb(theme.text_muted))
                    .child("→"),
            )
            .child(if forward_type == ForwardType::Dynamic {
                div()
                    .flex_1()
                    .pt(px(22.0))
                    .text_center()
                    .italic()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_forward_ui_text(self.i18n.t("forwards.form.socks5_mode")))
                    .into_any_element()
            } else {
                self.render_forward_address_side(
                    if forward_type == ForwardType::Remote {
                        self.i18n.t("forwards.form.local_client")
                    } else {
                        self.i18n.t("forwards.form.remote_server")
                    },
                    if editing {
                        ForwardInput::EditTargetHost
                    } else {
                        ForwardInput::CreateTargetHost
                    },
                    if editing {
                        ForwardInput::EditTargetPort
                    } else {
                        ForwardInput::CreateTargetPort
                    },
                    cx,
                )
            })
            .into_any_element()
    }

    fn render_forward_address_side(
        &self,
        label: String,
        host_input: ForwardInput,
        port_input: ForwardInput,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_forward_ui_text(label)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(self.render_forward_text_input(
                        host_input,
                        self.i18n.t("forwards.form.host_placeholder"),
                        true,
                        cx,
                    ))
                    .child(div().w(px(96.0)).child(self.render_forward_text_input(
                        port_input,
                        self.i18n.t("forwards.form.port_placeholder"),
                        true,
                        cx,
                    ))),
            )
            .into_any_element()
    }

    fn render_forward_text_input(
        &self,
        input: ForwardInput,
        placeholder: String,
        fill: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity();
        let focused = self.forwarding_view.focused_input == Some(input);
        let value = self.forward_input_value(input);
        let target = WorkspaceImeTarget::Forwards(input);
        text_input_anchor_probe(
            target.anchor_id(),
            div()
                .when(fill, |wrapper| wrapper.w_full())
                .font_family(self.forward_mono_font())
                .child(text_input(
                    &self.tokens,
                    TextInputView {
                        value,
                        placeholder,
                        focused,
                        caret_visible: self.new_connection_caret_visible,
                        secret: false,
                        selected_all: false,
                        selected_range: self.ime_selected_range_for_target(target),
                        marked_text: self.marked_text_for_target(target),
                    },
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        this.forwarding_view.focused_input = Some(input);
                        this.ime_marked_text = None;
                        this.needs_active_pane_focus = false;
                        window.focus(&this.focus_handle, cx);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(
                    cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                        this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                    }),
                ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }
}
