// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_gpui_ui::{
    checkbox::{CheckboxOptions, checkbox_with},
    confirm::{ConfirmDialogVariant, ConfirmDialogView, confirm_dialog},
};
use oxideterm_remote_desktop::{RemoteDesktopCertificateStore, RemoteDesktopServerCertificate};

use super::*;

#[derive(Clone, Debug)]
pub(super) struct RemoteDesktopCertificateChallengeState {
    pub(super) certificate: RemoteDesktopServerCertificate,
    pub(super) expected_fingerprint: Option<String>,
    pub(super) remember: bool,
}

impl WorkspaceApp {
    pub(super) fn handle_remote_desktop_certificate(
        &mut self,
        tab_id: TabId,
        generation: u64,
        certificate: RemoteDesktopServerCertificate,
        cx: &mut Context<Self>,
    ) {
        if !self.remote_desktop_worker_generation_matches(tab_id, generation) {
            return;
        }
        let store_path =
            RemoteDesktopCertificateStore::path_next_to_settings(self.settings_store.path());
        let persisted_fingerprint = RemoteDesktopCertificateStore::load(store_path)
            .ok()
            .and_then(|store| store.fingerprint(&certificate.endpoint).map(str::to_owned));
        let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) else {
            return;
        };
        if certificate.endpoint != session.profile.endpoint {
            // Never release credentials for a certificate challenge bound to a
            // different endpoint than the session the user opened.
            if let Some(sender) = session.request_tx.as_ref() {
                let _ = sender.send(RemoteDesktopHelperRequest::Close);
            }
            return;
        }
        let trusted_for_session = session.session_trusted_certificate_fingerprint.as_deref()
            == Some(certificate.sha256_fingerprint.as_str());
        let trusted_permanently =
            persisted_fingerprint.as_deref() == Some(certificate.sha256_fingerprint.as_str());

        if trusted_for_session || trusted_permanently {
            send_remote_desktop_authentication(session, &certificate);
            return;
        }

        session.certificate_challenge = Some(RemoteDesktopCertificateChallengeState {
            certificate,
            expected_fingerprint: persisted_fingerprint,
            remember: false,
        });
        cx.notify();
    }

    pub(super) fn toggle_remote_desktop_certificate_remember(
        &mut self,
        tab_id: TabId,
        challenge_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(challenge) = self
            .remote_desktop_sessions
            .get_mut(&tab_id)
            .and_then(|session| session.certificate_challenge.as_mut())
        else {
            return;
        };
        if challenge.certificate.challenge_id != challenge_id {
            return;
        }
        challenge.remember = !challenge.remember;
        cx.notify();
    }

    pub(super) fn accept_remote_desktop_certificate(
        &mut self,
        tab_id: TabId,
        generation: u64,
        challenge_id: &str,
        fingerprint: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.remote_desktop_worker_generation_matches(tab_id, generation) {
            return;
        }
        let Some(challenge) = self
            .remote_desktop_sessions
            .get(&tab_id)
            .and_then(|session| session.certificate_challenge.clone())
        else {
            return;
        };
        if challenge.certificate.challenge_id != challenge_id
            || challenge.certificate.sha256_fingerprint != fingerprint
        {
            return;
        }

        if challenge.remember {
            let store_path =
                RemoteDesktopCertificateStore::path_next_to_settings(self.settings_store.path());
            let persist_result =
                RemoteDesktopCertificateStore::load(store_path).and_then(|mut store| {
                    store.trust(
                        &challenge.certificate.endpoint,
                        challenge.certificate.sha256_fingerprint.clone(),
                    )
                });
            if let Err(error) = persist_result {
                self.push_command_palette_toast(
                    self.i18n
                        .t("remote_desktop.certificate_save_failed")
                        .replace("{{error}}", &error.to_string()),
                    None,
                    TerminalNoticeVariant::Error,
                );
                return;
            }
        }

        let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) else {
            return;
        };
        if !challenge.remember {
            session.session_trusted_certificate_fingerprint =
                Some(challenge.certificate.sha256_fingerprint.clone());
        }
        let request = remote_desktop_authenticate_request(session, &challenge.certificate);
        session.certificate_challenge = None;
        if let Some(sender) = session.request_tx.as_ref() {
            let _ = sender.send(request.unwrap_or(RemoteDesktopHelperRequest::Close));
        }
        cx.notify();
    }

    pub(super) fn reject_remote_desktop_certificate(
        &mut self,
        tab_id: TabId,
        generation: u64,
        challenge_id: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.remote_desktop_worker_generation_matches(tab_id, generation) {
            return;
        }
        let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) else {
            return;
        };
        if session
            .certificate_challenge
            .as_ref()
            .is_none_or(|challenge| challenge.certificate.challenge_id != challenge_id)
        {
            return;
        }
        session.certificate_challenge = None;
        if let Some(sender) = session.request_tx.as_ref() {
            let _ = sender.send(RemoteDesktopHelperRequest::Close);
        }
        cx.notify();
    }

    pub(super) fn render_remote_desktop_certificate_dialog(
        &self,
        tab_id: TabId,
        generation: u64,
        challenge: RemoteDesktopCertificateChallengeState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
}

fn remote_desktop_authenticate_request(
    session: &RemoteDesktopSession,
    certificate: &RemoteDesktopServerCertificate,
) -> Option<RemoteDesktopHelperRequest> {
    Some(RemoteDesktopHelperRequest::Authenticate {
        challenge_id: certificate.challenge_id.clone(),
        sha256_fingerprint: certificate.sha256_fingerprint.clone(),
        username: session.profile.username.clone()?,
        password: session.password.clone()?,
        domain: session.profile.domain.clone(),
    })
}

fn send_remote_desktop_authentication(
    session: &RemoteDesktopSession,
    certificate: &RemoteDesktopServerCertificate,
) {
    let Some(sender) = session.request_tx.as_ref() else {
        return;
    };
    // Missing credentials must terminate the staged handshake instead of
    // leaving a helper process waiting indefinitely behind the certificate gate.
    let request = remote_desktop_authenticate_request(session, certificate)
        .unwrap_or(RemoteDesktopHelperRequest::Close);
    let _ = sender.send(request);
}
