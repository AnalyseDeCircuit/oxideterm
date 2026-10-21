use super::*;
use crate::workspace::new_connection::NewConnectionField;
use gpui::prelude::*;
use oxideterm_gpui_ui::{
    SectionHeaderOptions,
    button::{ButtonOptions, ButtonVariant, button_with},
    checkbox::checkbox,
    form_field,
    modal::{dismissible_dialog_backdrop, modal_body, modal_container, modal_footer, modal_header},
    section_header, segmented_tab, segmented_tabs,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn ssh_tunnel_input_fields(&self) -> Option<Vec<NewConnectionField>> {
        let form = self.ssh_tunnels.form.as_ref()?;
        let mut fields = vec![NewConnectionField::Name];
        if form.saved.is_none() {
            fields.extend([NewConnectionField::Host, NewConnectionField::Port]);
            if form.kind != ForwardType::Dynamic {
                fields.extend([
                    NewConnectionField::TunnelTargetHost,
                    NewConnectionField::TunnelTargetPort,
                ]);
            }
        }
        Some(fields)
    }

    fn tunnel_input(
        &self,
        label: &str,
        value: &str,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_connection_field(self.i18n.t(label), value, String::new(), field, false, cx)
    }

    pub(in crate::workspace) fn render_ssh_tunnel_form(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let form = self.ssh_tunnels.form.as_ref()?;
        let (name, group, bind_host, bind_port, target_host, target_port, error) = {
            let draft = self.connection_form_state(cx).form.as_ref()?;
            (
                draft.name.clone(),
                draft.group.clone(),
                draft.host.clone(),
                draft.port.clone(),
                draft.tunnel_target_host.clone(),
                draft.tunnel_target_port.clone(),
                draft.error.clone(),
            )
        };
        let valid_port = |value: &str| value.trim().parse::<u16>().is_ok_and(|port| port > 0);
        let can_save = !name.trim().is_empty()
            && form
                .host_id
                .as_deref()
                .is_some_and(|id| self.connection_store.get(id).is_some())
            && (form.saved.is_some()
                || (!bind_host.trim().is_empty()
                    && valid_port(&bind_port)
                    && (form.kind == ForwardType::Dynamic
                        || (!target_host.trim().is_empty() && valid_port(&target_port)))));
        let gap = self.tokens.metrics.modal_section_gap;
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(gap))
            .child(section_header(
                &self.tokens,
                self.i18n.t("ssh.form.basic_information"),
                SectionHeaderOptions::new().compact(),
                None,
                None,
            ))
            .child(self.tunnel_input("ssh_tunnel.name", &name, NewConnectionField::Name, cx))
            .child(self.render_connection_group_select(self.i18n.t("ssh_tunnel.group"), &group, cx))
            .child(self.render_tunnel_host_select(
                form.host_id.as_deref(),
                form.saved.is_some(),
                cx,
            ));
        if form.saved.is_none() {
            let types = segmented_tabs(&self.tokens).children(
                [
                    (ForwardType::Local, "forwards.form.type_local"),
                    (ForwardType::Remote, "forwards.form.type_remote"),
                    (ForwardType::Dynamic, "forwards.form.type_dynamic"),
                ]
                .into_iter()
                .map(|(kind, label)| {
                    segmented_tab(&self.tokens, self.i18n.t(label), form.kind == kind)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.close_new_connection_select(cx);
                                if let Some(form) = this.ssh_tunnels.form.as_mut() {
                                    form.kind = kind;
                                }
                                this.update_connection_form_state(cx, |state| {
                                    if let Some(form) = state.form.as_mut() {
                                        form.field_focused = false;
                                        form.selected_field = None;
                                        form.focused_field = NewConnectionField::Host;
                                        form.error = None;
                                    }
                                });
                                this.ime_marked_text = None;
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                }),
            );
            let mut rule = div()
                .flex()
                .flex_col()
                .gap(px(gap))
                .pt(px(gap))
                .border_t_1()
                .border_color(rgb(self.tokens.ui.border))
                .child(section_header(
                    &self.tokens,
                    self.i18n.t("forwards.table.title"),
                    SectionHeaderOptions::new().compact(),
                    None,
                    None,
                ))
                .child(form_field(
                    &self.tokens,
                    self.i18n.t("forwards.form.type"),
                    types,
                ))
                .child(
                    div()
                        .flex()
                        .gap(px(gap))
                        .child(div().flex_1().min_w_0().child(self.tunnel_input(
                            "forwards.form.bind_address",
                            &bind_host,
                            NewConnectionField::Host,
                            cx,
                        )))
                        .child(div().w(px(112.0)).flex_none().child(self.tunnel_input(
                            "forwards.form.port_placeholder",
                            &bind_port,
                            NewConnectionField::Port,
                            cx,
                        ))),
                );
            if form.kind != ForwardType::Dynamic {
                rule = rule.child(
                    div()
                        .flex()
                        .gap(px(gap))
                        .child(div().flex_1().min_w_0().child(self.tunnel_input(
                            "forwards.form.target_address",
                            &target_host,
                            NewConnectionField::TunnelTargetHost,
                            cx,
                        )))
                        .child(div().w(px(112.0)).flex_none().child(self.tunnel_input(
                            "forwards.form.port_placeholder",
                            &target_port,
                            NewConnectionField::TunnelTargetPort,
                            cx,
                        ))),
                );
            }
            body = body.child(rule);
        }
        body = body.child(
            checkbox(
                &self.tokens,
                self.i18n.t("ssh_tunnel.startup"),
                form.startup,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if let Some(form) = this.ssh_tunnels.form.as_mut() {
                        form.startup = !form.startup;
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            ),
        );
        if let Some(error) = error {
            body = body.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(error),
            );
        }
        let width = self.tokens.metrics.modal_width
            + crate::workspace::new_connection::NEW_CONNECTION_TYPE_SIDEBAR_WIDTH
            + gap;
        let height = f32::from(window.viewport_size().height)
            * self.tokens.metrics.modal_max_viewport_height_ratio;
        let dialog = modal_container(&self.tokens)
            .bg(rgb(self.tokens.ui.bg_elevated))
            .w(px(width.min(
                (f32::from(window.viewport_size().width) - 64.0).max(0.0),
            )))
            .h(px(height))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(modal_header(
                &self.tokens,
                self.i18n.t("ssh_tunnel.title"),
                self.i18n.t("ssh_tunnel.description"),
            ))
            .child(
                modal_body(&self.tokens).flex_1().min_h_0().child(
                    div()
                        .size_full()
                        .min_h_0()
                        .flex()
                        .gap(px(gap))
                        .child(self.render_transport_selector(cx))
                        .child(
                            div()
                                .id("ssh-tunnel-form-scroll")
                                .flex_1()
                                .min_w_0()
                                .min_h_0()
                                .selectable_overflow_y_scroll(
                                    &self.selectable_text_scroll_handle("ssh-tunnel-form-scroll"),
                                )
                                .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                                    if this.connection_form_state(cx).open_select.is_some() {
                                        this.close_new_connection_select(cx);
                                        this.clear_new_connection_select_anchor(cx);
                                        cx.notify();
                                    }
                                }))
                                .child(body),
                        ),
                ),
            )
            .child(
                modal_footer(&self.tokens)
                    .flex_none()
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("common.cancel"),
                            ButtonOptions::default(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.close_ssh_tunnel_form(window, cx)
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("ssh.form.save"),
                            ButtonOptions {
                                disabled: !can_save,
                                ..Default::default()
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                if can_save {
                                    this.save_ssh_tunnel(false, window, cx);
                                }
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("forwards.form.save_and_start"),
                            ButtonOptions {
                                variant: ButtonVariant::Default,
                                disabled: !can_save,
                                ..Default::default()
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                if can_save {
                                    this.save_ssh_tunnel(true, window, cx);
                                }
                            }),
                        ),
                    ),
            );
        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.close_ssh_tunnel_form(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(dialog)
                .into_any_element(),
        )
    }
}
