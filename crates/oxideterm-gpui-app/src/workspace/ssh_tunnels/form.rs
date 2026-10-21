use super::*;
use gpui::prelude::*;
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonSize, ButtonVariant, button_with},
    checkbox::checkbox,
    modal::{modal_body, modal_container, modal_footer, modal_header, modal_overlay},
    scroll::ScrollableElement,
};

impl WorkspaceApp {
    fn tunnel_input(&self, label: &str, editor: &Entity<TextEditorView>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(self.i18n.t(label)),
            )
            .child(
                div()
                    .h(px(self.tokens.metrics.form_input_height))
                    .px_3()
                    .py_2()
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(self.tokens.ui.bg))
                    .child(editor.clone()),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ssh_tunnel_form(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let form = self.ssh_tunnels.form.as_ref()?;
        let host = form
            .host_id
            .as_deref()
            .and_then(|id| self.connection_store.get(id));
        let host_label = host
            .map(|host| format!("{} · {}@{}", host.name, host.username, host.host))
            .unwrap_or_else(|| self.i18n.t("ssh_tunnel.choose_host"));
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.tunnel_input("ssh_tunnel.name", &form.name))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.i18n.t("ssh_tunnel.host"))
                    .child(
                        button_with(
                            &self.tokens,
                            host_label,
                            ButtonOptions {
                                disabled: form.saved.is_some(),
                                ..Default::default()
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                if let Some(form) = this.ssh_tunnels.form.as_mut()
                                    && form.saved.is_none()
                                {
                                    form.choosing_host = !form.choosing_host;
                                    cx.notify();
                                }
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            );
        if form.choosing_host {
            let choices = self.connection_store.connection_infos();
            body = body.child(
                div().max_h(px(160.0)).overflow_y_scrollbar().child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .when(choices.is_empty(), |list| {
                            list.child(self.i18n.t("ssh_tunnel.choose_host"))
                        })
                        .children(choices.into_iter().map(|host| {
                            button_with(
                                &self.tokens,
                                format!("{} · {}@{}", host.name, host.username, host.host),
                                ButtonOptions {
                                    size: ButtonSize::Sm,
                                    variant: ButtonVariant::Ghost,
                                    ..Default::default()
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    if let Some(form) = this.ssh_tunnels.form.as_mut() {
                                        form.host_id = Some(host.id.clone());
                                        form.choosing_host = false;
                                        cx.notify();
                                    }
                                    cx.stop_propagation();
                                }),
                            )
                        })),
                ),
            );
        }
        body = body.child(self.tunnel_input("ssh_tunnel.group", &form.group));
        if form.saved.is_none() {
            body = body
                .child(
                    div().flex().gap_2().children(
                        [
                            (ForwardType::Local, "forwards.form.type_local"),
                            (ForwardType::Remote, "forwards.form.type_remote"),
                            (ForwardType::Dynamic, "forwards.form.type_dynamic"),
                        ]
                        .into_iter()
                        .map(|(kind, label)| {
                            button_with(
                                &self.tokens,
                                self.i18n.t(label),
                                ButtonOptions {
                                    size: ButtonSize::Sm,
                                    variant: if form.kind == kind {
                                        ButtonVariant::Default
                                    } else {
                                        ButtonVariant::Outline
                                    },
                                    ..Default::default()
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    if let Some(form) = this.ssh_tunnels.form.as_mut() {
                                        form.kind = kind;
                                        cx.notify();
                                    }
                                    cx.stop_propagation();
                                }),
                            )
                        }),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(self.tunnel_input("forwards.form.bind_address", &form.bind_host))
                        .child(
                            self.tunnel_input("forwards.form.port_placeholder", &form.bind_port),
                        ),
                );
            if form.kind != ForwardType::Dynamic {
                body = body.child(
                    div()
                        .flex()
                        .gap_3()
                        .child(self.tunnel_input("forwards.form.target_address", &form.target_host))
                        .child(
                            self.tunnel_input("forwards.form.port_placeholder", &form.target_port),
                        ),
                );
            }
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
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            ),
        );
        if let Some(error) = &form.error {
            body = body.child(
                div()
                    .text_color(rgb(self.tokens.ui.error))
                    .child(error.clone()),
            );
        }
        let dialog = modal_container(&self.tokens)
            .bg(rgb(self.tokens.ui.bg_elevated))
            .w(px(600.0_f32.min(
                (f32::from(window.viewport_size().width) - 32.0).max(0.0),
            )))
            .max_h(px(
                (f32::from(window.viewport_size().height) - 32.0).max(0.0)
            ))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key != "tab" {
                    return;
                }
                let Some(form) = this.ssh_tunnels.form.as_ref() else {
                    return;
                };
                let mut inputs = vec![&form.name, &form.group];
                if form.saved.is_none() {
                    inputs.extend([&form.bind_host, &form.bind_port]);
                    if form.kind != ForwardType::Dynamic {
                        inputs.extend([&form.target_host, &form.target_port]);
                    }
                }
                let current = inputs
                    .iter()
                    .position(|input| input.read(cx).focus_handle(cx).is_focused(window))
                    .unwrap_or(0);
                let next = if event.keystroke.modifiers.shift {
                    (current + inputs.len() - 1) % inputs.len()
                } else {
                    (current + 1) % inputs.len()
                };
                window.focus(&inputs[next].read(cx).focus_handle(cx), cx);
                window.prevent_default();
                cx.stop_propagation();
            }))
            .child(modal_header(
                &self.tokens,
                self.i18n.t("ssh_tunnel.title"),
                String::new(),
            ))
            .child(
                modal_body(&self.tokens)
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(body),
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
                            ButtonOptions::default(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.save_ssh_tunnel(false, window, cx)
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("forwards.form.save_and_start"),
                            ButtonOptions {
                                variant: ButtonVariant::Default,
                                ..Default::default()
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.save_ssh_tunnel(true, window, cx)
                            }),
                        ),
                    ),
            );
        Some(modal_overlay(&self.tokens, dialog))
    }
}
