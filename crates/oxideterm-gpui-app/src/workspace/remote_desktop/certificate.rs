// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_gpui_ui::{
    checkbox::{CheckboxOptions, checkbox_with},
    confirm::{ConfirmDialogVariant, ConfirmDialogView, confirm_dialog},
};
use oxideterm_remote_desktop::RemoteDesktopServerIdentityKind;

use super::*;

impl WorkspaceApp {
    pub(super) fn toggle_remote_desktop_certificate_remember(
        &mut self,
        tab_id: TabId,
        challenge_id: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, cx| {
                session.toggle_certificate_remember(challenge_id, cx);
            });
        }
    }

    pub(super) fn accept_remote_desktop_certificate(
        &mut self,
        tab_id: TabId,
        generation: u64,
        challenge_id: &str,
        fingerprint: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            let acceptance = session.update(cx, |session, cx| {
                session.accept_certificate(generation, challenge_id, fingerprint, cx)
            });
            if acceptance == RemoteDesktopCertificateAcceptance::StoreFailed {
                self.push_command_palette_toast(
                    self.i18n.t("remote_desktop.certificate_save_failed"),
                    None,
                    TerminalNoticeVariant::Error,
                    cx,
                );
            }
        }
    }

    pub(super) fn reject_remote_desktop_certificate(
        &mut self,
        tab_id: TabId,
        generation: u64,
        challenge_id: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.remote_desktop_session_entity(tab_id, cx) {
            session.update(cx, |session, cx| {
                session.reject_certificate(generation, challenge_id, cx);
            });
        }
    }

    pub(super) fn render_remote_desktop_certificate_dialog(
        &self,
        tab_id: TabId,
        generation: u64,
        challenge: RemoteDesktopCertificateChallengeState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if challenge.certificate.identity_kind != RemoteDesktopServerIdentityKind::X509Certificate {
            return self
                .render_remote_desktop_weak_security_dialog(tab_id, generation, challenge, cx);
        }
        let certificate_changed = challenge.expected_fingerprint.is_some();
        let title_key = if certificate_changed {
            "remote_desktop.certificate_changed_title"
        } else {
            "remote_desktop.certificate_unknown_title"
        };
        let warning = if certificate_changed {
            self.i18n
                .t("remote_desktop.certificate_changed_description")
        } else {
            self.i18n
                .t("remote_desktop.certificate_unknown_description")
        };
        let fingerprint = challenge.certificate.sha256_fingerprint.clone();
        let challenge_id = challenge.certificate.challenge_id.clone();
        let remember_challenge_id = challenge_id.clone();
        let reject_challenge_id = challenge_id.clone();
        let accept_challenge_id = challenge_id;
        let accept_fingerprint = fingerprint.clone();
        let endpoint = challenge.certificate.endpoint.format_authority();
        let endpoint_label = self
            .i18n
            .t("remote_desktop.certificate_endpoint")
            .replace("{{endpoint}}", &endpoint);
        let presented_fingerprint_label = self
            .i18n
            .t("remote_desktop.certificate_presented_fingerprint");
        let description = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(warning)
            .child(endpoint_label)
            .child(presented_fingerprint_label)
            .child(
                div()
                    .w_full()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(self.tokens.ui.bg))
                    .p_3()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .child(fingerprint),
            )
            .when_some(
                challenge.expected_fingerprint.clone(),
                |description, expected_fingerprint| {
                    description
                        .child(self.i18n.t("remote_desktop.certificate_saved_fingerprint"))
                        .child(
                            div()
                                .w_full()
                                .rounded(px(self.tokens.radii.sm))
                                .bg(rgb(self.tokens.ui.bg))
                                .p_3()
                                .font_family(settings_mono_font_family(
                                    self.settings_store.settings(),
                                ))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .child(expected_fingerprint),
                        )
                },
            )
            .child(
                checkbox_with(
                    &self.tokens,
                    format!("certificate-remember-{tab_id:?}-{remember_challenge_id}"),
                    self.i18n.t("remote_desktop.certificate_remember"),
                    challenge.remember,
                    CheckboxOptions::default(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.toggle_remote_desktop_certificate_remember(
                            tab_id,
                            &remember_challenge_id,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element();

        confirm_dialog(
            &self.tokens,
            ConfirmDialogView {
                variant: if certificate_changed {
                    ConfirmDialogVariant::Danger
                } else {
                    ConfirmDialogVariant::Default
                },
                title: div().child(self.i18n.t(title_key)).into_any_element(),
                description: Some(description),
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("remote_desktop.certificate_trust"))
                    .into_any_element(),
            },
            cx.listener(move |this, _event, _window, cx| {
                this.reject_remote_desktop_certificate(
                    tab_id,
                    generation,
                    &reject_challenge_id,
                    cx,
                );
                cx.stop_propagation();
            }),
            cx.listener(move |this, _event, _window, cx| {
                this.accept_remote_desktop_certificate(
                    tab_id,
                    generation,
                    &accept_challenge_id,
                    &accept_fingerprint,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
    }

    fn render_remote_desktop_weak_security_dialog(
        &self,
        tab_id: TabId,
        generation: u64,
        challenge: RemoteDesktopCertificateChallengeState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (title_key, description_key) = match challenge.certificate.identity_kind {
            RemoteDesktopServerIdentityKind::AnonymousTls => (
                "remote_desktop.vnc_anonymous_tls_title",
                "remote_desktop.vnc_anonymous_tls_description",
            ),
            RemoteDesktopServerIdentityKind::InsecureLegacy => (
                "remote_desktop.vnc_legacy_security_title",
                "remote_desktop.vnc_legacy_security_description",
            ),
            RemoteDesktopServerIdentityKind::X509Certificate => {
                unreachable!("X.509 certificate challenges use the certificate trust dialog")
            }
        };
        let endpoint = challenge.certificate.endpoint.format_authority();
        let endpoint_label = self
            .i18n
            .t("remote_desktop.certificate_endpoint")
            .replace("{{endpoint}}", &endpoint);
        let method_label = self
            .i18n
            .t("remote_desktop.vnc_security_method")
            .replace("{{method}}", &challenge.certificate.security_method);
        let description = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.i18n.t(description_key))
            .child(endpoint_label)
            .child(method_label)
            .into_any_element();
        let challenge_id = challenge.certificate.challenge_id.clone();
        let fingerprint = challenge.certificate.sha256_fingerprint;
        let reject_challenge_id = challenge_id.clone();

        confirm_dialog(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div().child(self.i18n.t(title_key)).into_any_element(),
                description: Some(description),
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("remote_desktop.vnc_continue_anyway"))
                    .into_any_element(),
            },
            cx.listener(move |this, _event, _window, cx| {
                this.reject_remote_desktop_certificate(
                    tab_id,
                    generation,
                    &reject_challenge_id,
                    cx,
                );
                cx.stop_propagation();
            }),
            cx.listener(move |this, _event, _window, cx| {
                this.accept_remote_desktop_certificate(
                    tab_id,
                    generation,
                    &challenge_id,
                    &fingerprint,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
    }
}
