// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_gpui_ui::{
    checkbox::{CheckboxOptions, checkbox_with},
    confirm::{ConfirmDialogVariant, ConfirmDialogView, confirm_dialog},
};
use oxideterm_remote_desktop::{
    RemoteDesktopCertificateStore, RemoteDesktopServerCertificate, RemoteDesktopServerIdentityKind,
    RemoteDesktopVncSecurityPolicy,
};

use super::*;

#[derive(Clone, Debug)]
pub(super) struct RemoteDesktopCertificateChallengeState {
    pub(super) certificate: RemoteDesktopServerCertificate,
    pub(super) expected_fingerprint: Option<String>,
    pub(super) remember: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RemoteDesktopCertificateAcceptance {
    Ignored,
    Accepted,
    StoreFailed,
}

impl RemoteDesktopSessionEntity {
    pub(super) fn handle_certificate(
        &mut self,
        generation: u64,
        certificate: RemoteDesktopServerCertificate,
        cx: &mut Context<Self>,
    ) {
        if self.worker_generation != generation {
            return;
        }
        let persisted_fingerprint =
            RemoteDesktopCertificateStore::load(self.certificate_store_path.clone())
                .ok()
                .and_then(|store| {
                    store
                        .fingerprint(certificate.protocol, &certificate.endpoint)
                        .map(str::to_owned)
                });
        if certificate.endpoint != self.profile.endpoint
            || certificate.protocol != self.profile.protocol
        {
            // Credentials may only cross an identity challenge bound to this session.
            if let Some(worker) = self.worker.as_ref() {
                worker.send(RemoteDesktopHelperRequest::Close);
            }
            return;
        }
        if certificate.identity_kind != RemoteDesktopServerIdentityKind::X509Certificate {
            let policy = self.profile.session_options.vnc.security_policy;
            let explicitly_allowed = match certificate.identity_kind {
                RemoteDesktopServerIdentityKind::AnonymousTls => {
                    policy != RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption
                }
                RemoteDesktopServerIdentityKind::InsecureLegacy => {
                    policy == RemoteDesktopVncSecurityPolicy::AllowLegacy
                }
                RemoteDesktopServerIdentityKind::X509Certificate => true,
            };
            if explicitly_allowed {
                // Weak transport modes still require a per-session acknowledgement.
                self.certificate_challenge = Some(RemoteDesktopCertificateChallengeState {
                    certificate,
                    expected_fingerprint: None,
                    remember: false,
                });
                cx.notify();
            } else if let Some(worker) = self.worker.as_ref() {
                worker.send(RemoteDesktopHelperRequest::Close);
            }
            return;
        }
        let trusted_for_session = self.session_trusted_certificate_fingerprint.as_deref()
            == Some(certificate.sha256_fingerprint.as_str());
        let trusted_permanently =
            persisted_fingerprint.as_deref() == Some(certificate.sha256_fingerprint.as_str());
        if trusted_for_session || trusted_permanently {
            send_remote_desktop_authentication(self, &certificate);
            return;
        }

        self.certificate_challenge = Some(RemoteDesktopCertificateChallengeState {
            certificate,
            expected_fingerprint: persisted_fingerprint,
            remember: false,
        });
        cx.notify();
    }

    fn toggle_certificate_remember(&mut self, challenge_id: &str, cx: &mut Context<Self>) {
        let Some(challenge) = self.certificate_challenge.as_mut() else {
            return;
        };
        if challenge.certificate.challenge_id != challenge_id {
            return;
        }
        challenge.remember = !challenge.remember;
        cx.notify();
    }

    fn accept_certificate(
        &mut self,
        generation: u64,
        challenge_id: &str,
        fingerprint: &str,
        cx: &mut Context<Self>,
    ) -> RemoteDesktopCertificateAcceptance {
        if self.worker_generation != generation {
            return RemoteDesktopCertificateAcceptance::Ignored;
        }
        let Some(challenge) = self.certificate_challenge.clone() else {
            return RemoteDesktopCertificateAcceptance::Ignored;
        };
        if challenge.certificate.challenge_id != challenge_id
            || challenge.certificate.sha256_fingerprint != fingerprint
        {
            return RemoteDesktopCertificateAcceptance::Ignored;
        }

        let persistent_identity =
            challenge.certificate.identity_kind == RemoteDesktopServerIdentityKind::X509Certificate;
        if persistent_identity
            && challenge.remember
            && RemoteDesktopCertificateStore::load(self.certificate_store_path.clone())
                .and_then(|mut store| {
                    store.trust(
                        challenge.certificate.protocol,
                        &challenge.certificate.endpoint,
                        challenge.certificate.sha256_fingerprint.clone(),
                    )
                })
                .is_err()
        {
            return RemoteDesktopCertificateAcceptance::StoreFailed;
        }
        if persistent_identity && !challenge.remember {
            self.session_trusted_certificate_fingerprint =
                Some(challenge.certificate.sha256_fingerprint.clone());
        }
        let request = remote_desktop_authenticate_request(self, &challenge.certificate);
        self.certificate_challenge = None;
        if let Some(worker) = self.worker.as_ref() {
            worker.send(request);
        }
        cx.notify();
        RemoteDesktopCertificateAcceptance::Accepted
    }

    fn reject_certificate(&mut self, generation: u64, challenge_id: &str, cx: &mut Context<Self>) {
        if self.worker_generation != generation
            || self
                .certificate_challenge
                .as_ref()
                .is_none_or(|challenge| challenge.certificate.challenge_id != challenge_id)
        {
            return;
        }
        self.certificate_challenge = None;
        if let Some(worker) = self.worker.as_ref() {
            worker.send(RemoteDesktopHelperRequest::Close);
        }
        cx.notify();
    }
}

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
        let fingerprint = challenge.certificate.sha256_fingerprint.clone();
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

fn remote_desktop_authenticate_request(
    session: &mut RemoteDesktopSessionEntity,
    certificate: &RemoteDesktopServerCertificate,
) -> RemoteDesktopHelperRequest {
    let password = if matches!(
        certificate.security_method.as_str(),
        "none" | "tls-none" | "x509-none"
    ) {
        // A password supplied for a no-authentication server must not remain
        // resident for the rest of the session.
        drop(session.password.take());
        None
    } else {
        // The session retains one zeroizing owner so a reconnect can answer a
        // new certificate challenge without persisting plaintext credentials.
        session
            .password
            .as_ref()
            .map(RemoteDesktopSecret::duplicate_for_reauthentication)
    };
    RemoteDesktopHelperRequest::Authenticate {
        challenge_id: certificate.challenge_id.clone(),
        sha256_fingerprint: certificate.sha256_fingerprint.clone(),
        username: session.profile.username.clone(),
        password,
        domain: session.profile.domain.clone(),
    }
}

fn send_remote_desktop_authentication(
    session: &mut RemoteDesktopSessionEntity,
    certificate: &RemoteDesktopServerCertificate,
) {
    let Some(sender) = session
        .worker
        .as_ref()
        .and_then(RemoteDesktopWorkerOwner::request_sender_cloned)
    else {
        return;
    };
    // The helper validates which optional credentials the negotiated security
    // method requires without receiving them before this identity boundary.
    let _ = sender.send(remote_desktop_authenticate_request(session, certificate));
}
