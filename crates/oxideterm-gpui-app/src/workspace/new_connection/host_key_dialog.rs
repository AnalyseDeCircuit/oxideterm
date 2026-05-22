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

#[derive(Clone, Debug)]
pub(in crate::workspace) struct HostKeyChallenge {
    pub(in crate::workspace) config: SshConfig,
    pub(in crate::workspace) title: String,
    pub(in crate::workspace) status: HostKeyStatus,
    pub(in crate::workspace) intent: SshConnectionIntent,
    pub(in crate::workspace) proxy_plan: Option<SshProxyPreflightPlan>,
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: u16,
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct SshProxyPreflightPlan {
    pub(in crate::workspace) config: SshConfig,
    pub(in crate::workspace) title: String,
    pub(in crate::workspace) intent: SshConnectionIntent,
    pub(in crate::workspace) current_index: usize,
}

impl WorkspaceApp {
    fn accept_host_key_challenge(
        &mut self,
        persist: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut challenge) = self.host_key_challenge.take() else {
            return;
        };
        let fingerprint = match &challenge.status {
            HostKeyStatus::Unknown { fingerprint, .. } => fingerprint.clone(),
            HostKeyStatus::Changed { .. } => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.error = Some(self.i18n.t("ssh.host_key.changed_requires_remove"));
                }
                cx.notify();
                return;
            }
            HostKeyStatus::Verified | HostKeyStatus::Error { .. } => return,
        };

        if let Some(mut plan) = challenge.proxy_plan.take() {
            apply_proxy_host_key_acceptance(
                &mut plan.config,
                plan.current_index,
                persist,
                fingerprint,
            );
            if let Some(form) = self.new_connection_form.as_mut() {
                form.pending = true;
                form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
            } else {
                self.session_manager.status = Some(self.i18n.t("ssh.form.checking_host_key"));
            }
            self.start_proxy_chain_preflight(plan);
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
        self.host_key_challenge = None;
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = Some(self.i18n.t("ssh.host_key.cancelled"));
        } else {
            self.session_manager.status = Some(self.i18n.t("ssh.host_key.cancelled"));
        }
        cx.notify();
    }

    fn remove_changed_host_key_challenge(&mut self, cx: &mut Context<Self>) {
        let Some(challenge) = self.host_key_challenge.take() else {
            return;
        };
        let HostKeyStatus::Changed {
            expected_fingerprint,
            key_type,
            ..
        } = &challenge.status
        else {
            self.host_key_challenge = Some(challenge);
            return;
        };

        match remove_host_key(
            &challenge.host,
            challenge.port,
            key_type,
            expected_fingerprint,
        ) {
            Ok(()) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.pending = true;
                    form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
                } else {
                    self.session_manager.status = Some(self.i18n.t("ssh.form.checking_host_key"));
                }
                if let Some(plan) = challenge.proxy_plan {
                    self.start_proxy_chain_preflight(plan);
                } else {
                    self.start_ssh_preflight(challenge.config, challenge.title, challenge.intent);
                }
            }
            Err(error) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.error = Some(error.to_string());
                } else {
                    self.session_manager.status = Some(error.to_string());
                }
                self.host_key_challenge = Some(challenge);
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn render_host_key_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(challenge) = self.host_key_challenge.as_ref() else {
            return div().into_any_element();
        };
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
            .child(
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
                                        rgba(0xef444480)
                                    } else {
                                        rgba(0xf59e0b80)
                                    })
                                    .bg(if changed {
                                        rgba(0x7f1d1d66)
                                    } else {
                                        rgba(0x78350f44)
                                    })
                                    .p(px(12.0))
                                    .text_size(px(self.tokens.metrics.form_text_font_size))
                                    .text_color(rgb(theme.text))
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
            )
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
                HostKeyButtonAction::RemoveSaved => this.remove_changed_host_key_challenge(cx),
            }),
        )
        .into_any_element()
    }
}

fn apply_proxy_host_key_acceptance(
    config: &mut SshConfig,
    step_index: usize,
    persist: bool,
    fingerprint: String,
) {
    if let Some(chain) = config.proxy_chain.as_mut()
        && let Some(hop) = chain.get_mut(step_index)
    {
        hop.strict_host_key_checking = true;
        hop.trust_host_key = Some(persist);
        hop.expected_host_key_fingerprint = Some(fingerprint);
        return;
    }

    config.strict_host_key_checking = true;
    config.trust_host_key = Some(persist);
    config.expected_host_key_fingerprint = Some(fingerprint);
}

#[cfg(test)]
mod tests {
    use oxideterm_ssh::{AuthMethod, ProxyHopConfig};

    use super::*;

    fn proxy_config() -> SshConfig {
        SshConfig {
            host: "target.internal".to_string(),
            port: 22,
            username: "alice".to_string(),
            auth: AuthMethod::Agent,
            proxy_chain: Some(vec![ProxyHopConfig {
                host: "jump.example.com".to_string(),
                port: 2222,
                username: "bob".to_string(),
                auth: AuthMethod::Agent,
                agent_forwarding: false,
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            }]),
            strict_host_key_checking: true,
            ..SshConfig::default()
        }
    }

    #[test]
    fn proxy_host_key_acceptance_updates_current_hop() {
        let mut config = proxy_config();

        apply_proxy_host_key_acceptance(&mut config, 0, true, "SHA256:jump".to_string());

        let hop = &config.proxy_chain.as_ref().unwrap()[0];
        assert_eq!(hop.trust_host_key, Some(true));
        assert_eq!(
            hop.expected_host_key_fingerprint.as_deref(),
            Some("SHA256:jump")
        );
        assert_eq!(config.trust_host_key, None);
        assert_eq!(config.expected_host_key_fingerprint, None);
    }

    #[test]
    fn proxy_host_key_acceptance_updates_target_after_hops() {
        let mut config = proxy_config();

        apply_proxy_host_key_acceptance(&mut config, 1, false, "SHA256:target".to_string());

        let hop = &config.proxy_chain.as_ref().unwrap()[0];
        assert_eq!(hop.trust_host_key, None);
        assert_eq!(hop.expected_host_key_fingerprint, None);
        assert_eq!(config.trust_host_key, Some(false));
        assert_eq!(
            config.expected_host_key_fingerprint.as_deref(),
            Some("SHA256:target")
        );
    }
}
