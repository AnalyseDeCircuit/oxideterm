use gpui::{
    AnyElement, Context, MouseButton, ParentElement, SharedString, Styled, Window, div, prelude::*,
    px, rgb, rgba,
};
use oxideterm_ssh::{HostKeyStatus, SshConfig};

use super::ssh_flow::SshConnectionIntent;
use crate::workspace::WorkspaceApp;

#[derive(Clone, Debug, Eq, PartialEq)]
enum HostKeyButtonAction {
    Cancel,
    TrustOnce,
    TrustSave,
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct HostKeyChallenge {
    pub(in crate::workspace) config: SshConfig,
    pub(in crate::workspace) title: String,
    pub(in crate::workspace) status: HostKeyStatus,
    pub(in crate::workspace) intent: SshConnectionIntent,
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

        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x000000cc))
            .child(
                div()
                    .w(px(480.0))
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_elevated))
                    .shadow_lg()
                    .overflow_hidden()
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
                                    .child(format!(
                                        "{}:{}",
                                        challenge.config.host, challenge.config.port
                                    )),
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
                                ))
                                .child(
                                    self.render_host_key_value(
                                        self.i18n.t("ssh.host_key.fingerprint_label"),
                                        fingerprint,
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

    fn render_host_key_value(&self, label: String, value: String) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.form_label_font_size))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(label),
            )
            .child(
                div()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(self.tokens.ui.bg_hover))
                    .p(px(8.0))
                    .text_size(px(self.tokens.metrics.form_text_font_size))
                    .text_color(rgb(self.tokens.ui.text))
                    .font_family(SharedString::from("SF Mono"))
                    .child(value),
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
        let theme = self.tokens.ui;
        div()
            .h(px(self.tokens.metrics.form_button_height))
            .px(px(self.tokens.metrics.form_button_padding_x))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(if primary {
                rgb(theme.accent)
            } else {
                rgb(theme.bg_elevated)
            })
            .text_size(px(self.tokens.metrics.form_text_font_size))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if primary {
                rgb(theme.accent_text)
            } else {
                rgb(theme.text)
            })
            .cursor_pointer()
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| match action {
                    HostKeyButtonAction::Cancel => this.cancel_host_key_challenge(cx),
                    HostKeyButtonAction::TrustOnce => {
                        this.accept_host_key_challenge(false, window, cx)
                    }
                    HostKeyButtonAction::TrustSave => {
                        this.accept_host_key_challenge(true, window, cx)
                    }
                }),
            )
            .into_any_element()
    }
}
