// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

use oxideterm_ssh::{
    HostKeyStatus, KeyboardInteractivePromptRequest, KeyboardInteractiveResponses, SshConfig,
    SshPasswordResponse, SshPromptError,
};
use tokio::sync::oneshot;

use crate::SshConnectionIntent;

pub struct HostKeyChallenge {
    pub presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub config: SshConfig,
    pub title: String,
    pub status: HostKeyStatus,
    pub intent: SshConnectionIntent,
    pub session_tree_challenge: bool,
    pub checked_hop: Option<usize>,
    pub host: String,
    pub port: u16,
}

const KBI_PROMPT_TIMEOUT_SECS: u64 = 60;

pub enum NativeSshPromptSender {
    Interactive(oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>),
    Password {
        response_tx: oneshot::Sender<Result<SshPasswordResponse, SshPromptError>>,
        on_authenticated: Option<Box<dyn FnOnce(zeroize::Zeroizing<String>) + Send>>,
    },
}

impl From<oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>>
    for NativeSshPromptSender
{
    fn from(sender: oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>) -> Self {
        Self::Interactive(sender)
    }
}

impl NativeSshPromptSender {
    pub(super) fn is_closed(&self) -> bool {
        match self {
            Self::Interactive(sender) => sender.is_closed(),
            Self::Password { response_tx, .. } => response_tx.is_closed(),
        }
    }

    pub fn can_remember(&self) -> bool {
        matches!(
            self,
            Self::Password {
                on_authenticated: Some(_),
                ..
            }
        )
    }

    pub(super) fn send(
        self,
        result: Result<KeyboardInteractiveResponses, SshPromptError>,
    ) -> Result<(), ()> {
        self.send_with_remember(result, false)
    }

    pub(super) fn send_with_remember(
        self,
        result: Result<KeyboardInteractiveResponses, SshPromptError>,
        remember: bool,
    ) -> Result<(), ()> {
        match self {
            Self::Interactive(sender) => sender.send(result).map_err(|_| ()),
            Self::Password {
                response_tx,
                on_authenticated,
            } => {
                let result = result.and_then(|mut responses| {
                    if responses.len() != 1 {
                        return Err(SshPromptError::Failed("Invalid password response".into()));
                    }
                    Ok(SshPasswordResponse {
                        password: zeroize::Zeroizing::new(std::mem::take(&mut responses[0])),
                        on_authenticated: if remember { on_authenticated } else { None },
                    })
                });
                response_tx.send(result).map_err(|_| ())
            }
        }
    }
}
pub struct KeyboardInteractiveChallenge {
    pub presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub request: KeyboardInteractivePromptRequest,
    pub responses: KeyboardInteractiveResponses,
    pub focused_prompt: usize,
    pub remember_password: bool,
    pub external_wait: bool,
    pub(super) expires_at: Instant,
    pub response_tx: Option<NativeSshPromptSender>,
}

impl crate::entity::ConnectionFlowEntity {
    pub fn start_host_key_cancellation(
        &mut self,
        delay: Duration,
        cx: &mut gpui::Context<Self>,
    ) -> (Option<crate::launch::SshConnectionIntent>, Option<bool>) {
        let intent = self.host_key_challenge_intent();
        if let Some(token) = intent
            .as_ref()
            .and_then(|intent| intent.standalone_sftp_pair_launch_token())
        {
            self.take_sftp_pair_launch(token);
        }
        let inline = self
            .begin_host_key_challenge_exit(delay, cx)
            .then(|| self.set_form_feedback(Some(false), None, cx));
        (intent, inline)
    }

    pub fn paste_interactive_clipboard(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return false;
        };
        let text = zeroize::Zeroizing::new(text);
        let normalized = zeroize::Zeroizing::new(text.replace("\r\n", "\n"));
        let normalized = zeroize::Zeroizing::new(normalized.replace('\r', "\n"));
        let single_line = zeroize::Zeroizing::new(normalized.lines().collect::<Vec<_>>().join(" "));
        self.paste_keyboard_interactive_response(&single_line, cx)
    }
}

impl KeyboardInteractiveChallenge {
    pub(super) fn new(
        request: KeyboardInteractivePromptRequest,
        response_tx: NativeSshPromptSender,
    ) -> Self {
        let responses =
            KeyboardInteractiveResponses::new(vec![String::new(); request.prompts.len()]);
        Self {
            presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            request,
            responses,
            focused_prompt: 0,
            remember_password: false,
            external_wait: false,
            expires_at: Instant::now() + Duration::from_secs(KBI_PROMPT_TIMEOUT_SECS),
            response_tx: Some(response_tx),
        }
    }

    pub fn timed_out(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    pub fn seconds_left(&self) -> u64 {
        self.expires_at
            .saturating_duration_since(Instant::now())
            .as_secs()
    }

    pub(super) fn all_responses_filled(&self) -> bool {
        !self.external_wait && self.responses.iter().all(|response| !response.is_empty())
    }
}

/// The accepted challenge relinquishes its SSH configuration exactly once.
pub enum HostKeyApproval {
    Tree {
        fingerprint: String,
        replacement: Option<oxideterm_ssh::HostKeyReplacement>,
        status: Option<String>,
    },
    Retry {
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
    },
    Continue {
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
    },
    Report(String),
}

impl crate::entity::ConnectionFlowEntity {
    pub fn accept_host_key(
        &mut self,
        persist: bool,
        i18n: &oxideterm_i18n::I18n,
        cx: &mut gpui::Context<Self>,
    ) -> Option<HostKeyApproval> {
        let mut challenge = self.take_host_key_challenge(cx)?;
        let (fingerprint, replacement) = match &challenge.status {
            HostKeyStatus::Unknown { fingerprint, .. } => (fingerprint.clone(), None),
            HostKeyStatus::Changed {
                expected_fingerprint,
                actual_fingerprint,
                key_type,
            } => (
                actual_fingerprint.clone(),
                Some(oxideterm_ssh::HostKeyReplacement {
                    host: challenge.host.clone(),
                    port: challenge.port,
                    key_type: key_type.clone(),
                    previous_fingerprint: expected_fingerprint.clone(),
                    fingerprint: actual_fingerprint.clone(),
                }),
            ),
            HostKeyStatus::Verified | HostKeyStatus::Error { .. } => return None,
        };
        if challenge.session_tree_challenge {
            let message = i18n.t("ssh.form.checking_host_key");
            let status =
                (!self.set_form_feedback(Some(true), Some(message.clone()), cx)).then_some(message);
            return Some(HostKeyApproval::Tree {
                fingerprint,
                replacement,
                status,
            });
        }
        challenge.config.host_key_replacement = replacement;
        let embedded_route = challenge.intent.uses_embedded_route(&challenge.config);
        let challenge_matches_endpoint = challenge.checked_hop.is_none()
            && challenge.config.host == challenge.host
            && challenge.config.port == challenge.port;
        if embedded_route && !challenge_matches_endpoint {
            let hop = challenge.config.proxy_chain.as_mut().and_then(|chain| {
                let matches = |hop: &oxideterm_ssh::ProxyHopConfig| {
                    hop.host == challenge.host && hop.port == challenge.port
                };
                match challenge.checked_hop {
                    Some(index) => chain.get_mut(index).filter(|hop| matches(hop)),
                    None => chain.iter_mut().find(|hop| matches(hop)),
                }
            });
            let Some(hop) = hop else {
                let message = i18n.t("sftp.standalone.route_host_key_unavailable");
                return (!self.set_form_feedback(None, Some(message.clone()), cx))
                    .then_some(HostKeyApproval::Report(message));
            };
            hop.strict_host_key_checking = true;
            hop.trust_host_key = Some(persist);
            hop.expected_host_key_fingerprint = Some(fingerprint);
        } else {
            challenge.config.strict_host_key_checking = true;
            challenge.config.trust_host_key = Some(persist);
            challenge.config.expected_host_key_fingerprint = Some(fingerprint);
        }
        Some(
            if embedded_route
                && (!challenge_matches_endpoint || challenge.config.discovery_origin.is_none())
            {
                self.set_form_feedback(Some(true), Some(i18n.t("ssh.form.checking_host_key")), cx);
                HostKeyApproval::Retry {
                    config: challenge.config,
                    title: challenge.title,
                    intent: challenge.intent,
                }
            } else {
                HostKeyApproval::Continue {
                    config: challenge.config,
                    title: challenge.title,
                    intent: challenge.intent,
                }
            },
        )
    }
}
