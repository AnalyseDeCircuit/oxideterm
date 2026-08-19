use gpui::{
    AnyElement, Context, MouseButton, ParentElement, SharedString, Styled, Window, div, prelude::*,
    px, rgb, rgba,
};
use oxideterm_gpui_ui::{
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions},
    modal::dismissible_dialog_backdrop,
};
use oxideterm_ssh::{HostKeyStatus, SshConfig, remove_host_key};

use super::ssh_flow::SshConnectionIntent;
use crate::workspace::WorkspaceApp;

#[derive(Clone, Debug, Eq, PartialEq)]
enum HostKeyButtonAction {
    Cancel,
    TrustOnce,
    TrustSave,
    RemoveSaved,
}

pub(in crate::workspace) struct HostKeyChallenge {
    pub(in crate::workspace) presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(in crate::workspace) config: SshConfig,
    pub(in crate::workspace) title: String,
    pub(in crate::workspace) status: HostKeyStatus,
    pub(in crate::workspace) intent: SshConnectionIntent,
    pub(in crate::workspace) session_tree_challenge: bool,
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: u16,
}

impl WorkspaceApp {
    fn accept_host_key_challenge(
        &mut self,
        persist: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut challenge) = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.take_host_key_challenge(cx)
        }) else {
            return;
        };
        let fingerprint = match &challenge.status {
            HostKeyStatus::Unknown { fingerprint, .. } => fingerprint.clone(),
            HostKeyStatus::Changed { .. } => {
                let message = self.i18n.t("ssh.host_key.changed_requires_remove");
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.error = Some(message);
                    }
                });
                cx.notify();
                return;
            }
            HostKeyStatus::Verified | HostKeyStatus::Error { .. } => return,
        };

        if challenge.session_tree_challenge {
            let message = self.i18n.t("ssh.form.checking_host_key");
            if self.connection_form_state(cx).form.is_some() {
                self.update_connection_form_state(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.pending = true;
                        form.error = Some(message);
                    }
                });
            } else {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(message), cx);
                });
            }
            self.accept_active_proxy_connect_host_key(persist, fingerprint, window, cx);
            cx.notify();
            return;
        }

        let routed_standalone_sftp = challenge
            .config
            .proxy_chain
            .as_ref()
            .is_some_and(|chain| !chain.is_empty())
            && matches!(
                &challenge.intent,
                SshConnectionIntent::StandaloneSftp { .. }
                    | SshConnectionIntent::StandaloneSftpSecondary { .. }
                    | SshConnectionIntent::TestStandaloneSftp
            );
        if routed_standalone_sftp {
            if challenge.config.host == challenge.host && challenge.config.port == challenge.port {
                challenge.config.strict_host_key_checking = true;
                challenge.config.trust_host_key = Some(persist);
                challenge.config.expected_host_key_fingerprint = Some(fingerprint);
            } else if let Some(hop) = challenge.config.proxy_chain.as_mut().and_then(|chain| {
                chain
                    .iter_mut()
                    .find(|hop| hop.host == challenge.host && hop.port == challenge.port)
            }) {
                hop.strict_host_key_checking = true;
                hop.trust_host_key = Some(persist);
                hop.expected_host_key_fingerprint = Some(fingerprint);
            } else {
                let message = self.i18n.t("sftp.standalone.route_host_key_unavailable");
                let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.set_form_feedback(None, Some(message.clone()), cx)
                });
                if !reported_to_form {
                    self.session_manager.update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(message), cx);
                    });
                }
                cx.notify();
                return;
            }
            let message = self.i18n.t("ssh.form.checking_host_key");
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.pending = true;
                    form.error = Some(message);
                }
            });
            self.start_ssh_preflight(challenge.config, challenge.title, challenge.intent, cx);
            cx.notify();
            return;
        }

        challenge.config.strict_host_key_checking = true;
        challenge.config.trust_host_key = Some(persist);
        challenge.config.expected_host_key_fingerprint = Some(fingerprint);
        self.continue_verified_ssh_flow(
            challenge.config,
            challenge.title,
            challenge.intent,
            window,
            cx,
        );
    }

    pub(in crate::workspace) fn cancel_host_key_challenge(&mut self, cx: &mut Context<Self>) {
        if let Some(intent) = self.connection_flow.read(cx).host_key_challenge_intent() {
            if let Some(token) = intent.standalone_sftp_pair_launch_token() {
                self.pending_standalone_sftp_pair_launches.remove(token);
            }
            self.fail_public_mcp_mosh_open_for_intent(
                &intent,
                "The host-key confirmation was cancelled",
            );
        }
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        let began_exit = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.begin_host_key_challenge_exit(delay, cx)
        });
        if !began_exit {
            return;
        }
        self.cancel_active_proxy_connect_run(cx);
        // Tauri HostKeyConfirmDialog cancellation only clears pending
        // connect/test state. It does not surface a form or session-manager
        // error for a user-initiated close.
        if self.connection_form_state(cx).form.is_some() {
            self.update_connection_form_state(cx, |state| {
                if let Some(form) = state.form.as_mut() {
                    form.pending = false;
                    form.error = None;
                }
            });
        } else {
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(None, cx);
            });
        }
        cx.notify();
    }

    fn remove_changed_host_key_challenge(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(challenge) = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.take_host_key_challenge(cx)
        }) else {
            return;
        };
        let HostKeyStatus::Changed {
            expected_fingerprint,
            key_type,
            ..
        } = &challenge.status
        else {
            self.connection_flow.update(cx, |connection_flow, cx| {
                connection_flow.restore_host_key_challenge(challenge, cx);
            });
            return;
        };

        match remove_host_key(
            &challenge.host,
            challenge.port,
            key_type,
            expected_fingerprint,
        ) {
            Ok(()) => {
                let message = self.i18n.t("ssh.form.checking_host_key");
                if self.connection_form_state(cx).form.is_some() {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.pending = true;
                            form.error = Some(message);
                        }
                    });
                } else {
                    self.session_manager.update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(message), cx);
                    });
                }
                if challenge.session_tree_challenge {
                    self.continue_active_proxy_session_tree_preflight_only(cx);
                } else {
                    self.start_ssh_preflight(
                        challenge.config,
                        challenge.title,
                        challenge.intent,
                        cx,
                    );
                }
            }
            Err(error) => {
                let message = error.to_string();
                if self.connection_form_state(cx).form.is_some() {
                    self.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.error = Some(message);
                        }
                    });
                } else {
                    self.session_manager.update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(message), cx);
                    });
                }
                self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.restore_host_key_challenge(challenge, cx);
                });
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn render_host_key_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(challenge) = self.connection_flow.read(cx).host_key_dialog_snapshot() else {
            return div().into_any_element();
        };
        let dialog_visible = challenge.visible;
        let theme = self.tokens.ui;
        let (title, message, key_type, fingerprint, changed) = match &challenge.status {
            HostKeyStatus::Unknown {
                fingerprint,
                key_type,
            } => (
                self.i18n.t("ssh.host_key.title_unknown"),
                self.i18n.t("ssh.host_key.unknown_message"),
                key_type.clone(),
                fingerprint.clone(),
                false,
            ),
            HostKeyStatus::Changed {
                expected_fingerprint,
                actual_fingerprint,
                key_type,
            } => (
                self.i18n.t("ssh.host_key.title_changed"),
                format!(
                    "{}\n{}: {}\n{}: {}",
                    self.i18n.t("ssh.host_key.changed_warning"),
                    self.i18n.t("ssh.host_key.expected_fingerprint"),
                    expected_fingerprint,
                    self.i18n.t("ssh.host_key.actual_fingerprint"),
                    actual_fingerprint
                ),
                key_type.clone(),
                actual_fingerprint.clone(),
                true,
            ),
            HostKeyStatus::Error { message } => (
                self.i18n.t("ssh.host_key.title_error"),
                message.clone(),
                String::new(),
                String::new(),
                false,
            ),
            HostKeyStatus::Verified => (
                self.i18n.t("ssh.host_key.title_unknown"),
                String::new(),
                String::new(),
                String::new(),
                false,
            ),
        };

        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri HostKeyConfirmDialog closes via Radix onOpenChange
                    // when not loading; native host-key actions are synchronous,
                    // so backdrop dismiss follows the same cancel path as Esc.
                    this.cancel_host_key_challenge(cx);
                    cx.stop_propagation();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "host-key-dialog-transition",
                div()
                    .w(px(480.0))
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_elevated))
                    .shadow_lg()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px(px(self.tokens.metrics.modal_header_padding_x))
                            .py(px(self.tokens.metrics.modal_header_padding_y))
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.modal_title_font_size))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(theme.text))
                                    .child(title),
                            )
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .text_size(px(self.tokens.metrics.form_text_font_size))
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!("{}:{}", challenge.host, challenge.port)),
                            ),
                    )
                    .child(
                        div()
                            .p(px(self.tokens.metrics.modal_body_padding))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.md))
                                    .border_1()
                                    .border_color(if changed {
                                        rgba((theme.error << 8) | 0x80)
                                    } else {
                                        rgba((theme.warning << 8) | 0x80)
                                    })
                                    .bg(if changed {
                                        rgba((theme.error << 8) | 0x18)
                                    } else {
                                        rgba((theme.warning << 8) | 0x14)
                                    })
                                    .p(px(12.0))
                                    .text_size(px(self.tokens.metrics.form_text_font_size))
                                    .text_color(rgb(if changed {
                                        theme.error
                                    } else {
                                        theme.warning
                                    }))
                                    .child(message),
                            )
                            .when(!key_type.is_empty(), |body| {
                                body.child(self.render_host_key_value(
                                    self.i18n.t("ssh.host_key.key_type_label"),
                                    key_type,
                                    cx,
                                ))
                                .child(
                                    self.render_host_key_value(
                                        self.i18n.t("ssh.host_key.fingerprint_label"),
                                        fingerprint,
                                        cx,
                                    ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .px(px(self.tokens.metrics.modal_footer_padding_x))
                            .py(px(12.0))
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(self.render_host_key_button(
                                self.i18n.t("ssh.host_key.actions.cancel"),
                                false,
                                HostKeyButtonAction::Cancel,
                                cx,
                            ))
                            .when(changed, |footer| {
                                footer.child(self.render_host_key_button(
                                    self.i18n.t("ssh.host_key.actions.remove_saved"),
                                    true,
                                    HostKeyButtonAction::RemoveSaved,
                                    cx,
                                ))
                            })
                            .when(!changed, |footer| {
                                footer
                                    .child(self.render_host_key_button(
                                        self.i18n.t("ssh.host_key.actions.trust_once"),
                                        false,
                                        HostKeyButtonAction::TrustOnce,
                                        cx,
                                    ))
                                    .child(self.render_host_key_button(
                                        self.i18n.t("ssh.host_key.actions.trust_save"),
                                        true,
                                        HostKeyButtonAction::TrustSave,
                                        cx,
                                    ))
                            }),
                    ),
                dialog_visible,
            ))
            .into_any_element()
    }

    fn render_host_key_value(
        &self,
        label: String,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.form_label_font_size))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "host-key-label",
                        (&label, &value),
                        label.clone(),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(self.tokens.ui.bg_hover))
                    .p(px(8.0))
                    .text_size(px(self.tokens.metrics.form_text_font_size))
                    .text_color(rgb(self.tokens.ui.text))
                    .font_family(SharedString::from("SF Mono"))
                    .child(self.render_selectable_text(
                        crate::workspace::selectable_text::selectable_text_id(
                            "host-key-value",
                            (&label, &value),
                        ),
                        value,
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_host_key_button(
        &self,
        label: String,
        primary: bool,
        action: HostKeyButtonAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let variant = match action {
            HostKeyButtonAction::Cancel => ButtonVariant::Outline,
            HostKeyButtonAction::TrustOnce => ButtonVariant::Secondary,
            HostKeyButtonAction::TrustSave if primary => ButtonVariant::Default,
            HostKeyButtonAction::RemoveSaved if primary => ButtonVariant::Destructive,
            _ => ButtonVariant::Secondary,
        };
        // Host-key prompts are protected dialogs; only the button chrome moves
        // to the shared shadcn-style primitive. The explicit challenge actions
        // and non-dismissible backdrop semantics stay local.
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                height: Some(self.tokens.metrics.form_button_height),
                padding_x: Some(self.tokens.metrics.form_button_padding_x),
                font_size: Some(self.tokens.metrics.form_text_font_size),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| match action {
                HostKeyButtonAction::Cancel => this.cancel_host_key_challenge(cx),
                HostKeyButtonAction::TrustOnce => this.accept_host_key_challenge(false, window, cx),
                HostKeyButtonAction::TrustSave => this.accept_host_key_challenge(true, window, cx),
                HostKeyButtonAction::RemoveSaved => {
                    this.remove_changed_host_key_challenge(window, cx)
                }
            }),
        )
        .into_any_element()
    }
}
