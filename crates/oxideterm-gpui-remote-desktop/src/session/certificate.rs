// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_remote_desktop::{
    RemoteDesktopCertificateStore, RemoteDesktopServerCertificate, RemoteDesktopServerIdentityKind,
    RemoteDesktopVncSecurityPolicy,
};

use super::*;

#[derive(Clone, Debug)]
pub struct RemoteDesktopCertificateChallengeState {
    pub certificate: RemoteDesktopServerCertificate,
    pub expected_fingerprint: Option<String>,
    pub remember: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteDesktopCertificateAcceptance {
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

    pub fn toggle_certificate_remember(&mut self, challenge_id: &str, cx: &mut Context<Self>) {
        let Some(challenge) = self.certificate_challenge.as_mut() else {
            return;
        };
        if challenge.certificate.challenge_id != challenge_id {
            return;
        }
        challenge.remember = !challenge.remember;
        cx.notify();
    }

    pub fn accept_certificate(
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

    pub fn reject_certificate(
        &mut self,
        generation: u64,
        challenge_id: &str,
        cx: &mut Context<Self>,
    ) {
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
