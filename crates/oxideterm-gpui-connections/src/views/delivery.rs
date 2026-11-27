// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Context;
use oxideterm_i18n::I18n;
use oxideterm_ssh::{ConnectionConsumer, KeyboardInteractivePromptRequest, SshConnectionHandle};

use super::ConnectionsWindowRequest as Request;
use crate::{
    entity::ConnectionFlowEntity,
    launch::SshConnectionWorkerResult,
    preflight::{PreflightOutcome, TreePreflightOutcome},
};

pub struct SftpConnectedEndpoint {
    pub id: String,
    pub path: Option<String>,
    pub consumer: ConnectionConsumer,
    pub handle: SshConnectionHandle,
}

pub struct SftpConnected {
    pub title: String,
    pub saved_profile_id: Option<String>,
    pub primary: SftpConnectedEndpoint,
    pub secondary: Option<SftpConnectedEndpoint>,
}

impl ConnectionFlowEntity {
    pub fn apply_ssh_worker_result(
        &mut self,
        result: SshConnectionWorkerResult,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Option<Request> {
        use SshConnectionWorkerResult as Result;
        match result {
            Result::Preflight {
                mut config,
                upstream_proxy,
                title,
                intent,
                host,
                port,
                checked_hop,
                status,
            } => {
                // Move the original proxy (including credentials) back to its launch owner.
                config.upstream_proxy = upstream_proxy;
                let discovery = config.discovery_origin.is_some();
                Some(
                    match self.resolve_preflight(
                        config,
                        title,
                        intent,
                        host,
                        port,
                        checked_hop,
                        status,
                        cx,
                    ) {
                        PreflightOutcome::Continue(config, title, intent) => Request::VerifiedSsh {
                            config,
                            title,
                            intent,
                        },
                        PreflightOutcome::ConfirmHostKey => Request::FocusHostKey,
                        PreflightOutcome::Failed {
                            intent,
                            message,
                            inline,
                        } => Request::PreflightFailed {
                            discovery,
                            intent,
                            message,
                            inline,
                        },
                    },
                )
            }
            Result::SessionTreePreflight { context, status } => {
                match self.resolve_tree_preflight(context, status, cx) {
                    TreePreflightOutcome::Stale => None,
                    TreePreflightOutcome::Advance => Some(Request::AdvanceProxyChain),
                    TreePreflightOutcome::ConfirmHostKey => Some(Request::FocusHostKey),
                    TreePreflightOutcome::Failed(message) => {
                        Some(Request::AbortProxyChain(message))
                    }
                }
            }
            Result::Test { result } => self
                .finish_connection_test(result, i18n, cx)
                .map(|message| Request::ConnectionStatus(Some(message))),
            Result::StandaloneSftpConnected {
                endpoint_id,
                saved_profile_id,
                title,
                initial_remote_path,
                consumer,
                result,
            } => match result {
                Ok(handle) => self.sftp_connected(
                    SftpConnected {
                        title,
                        saved_profile_id,
                        primary: SftpConnectedEndpoint {
                            id: endpoint_id,
                            path: initial_remote_path,
                            consumer,
                            handle,
                        },
                        secondary: None,
                    },
                    cx,
                ),
                Err(error) => self.worker_error(error, cx),
            },
            Result::StandaloneSftpPairConnected {
                saved_profile_id,
                title,
                primary_endpoint_id,
                secondary_endpoint_id,
                primary_initial_remote_path,
                secondary_initial_remote_path,
                primary_consumer,
                secondary_consumer,
                result,
            } => match result {
                Ok((primary_handle, secondary_handle)) => self.sftp_connected(
                    SftpConnected {
                        title,
                        saved_profile_id: Some(saved_profile_id),
                        primary: SftpConnectedEndpoint {
                            id: primary_endpoint_id,
                            path: primary_initial_remote_path,
                            consumer: primary_consumer,
                            handle: primary_handle,
                        },
                        secondary: Some(SftpConnectedEndpoint {
                            id: secondary_endpoint_id,
                            path: secondary_initial_remote_path,
                            consumer: secondary_consumer,
                            handle: secondary_handle,
                        }),
                    },
                    cx,
                ),
                Err(error) => self.worker_error(error, cx),
            },
            Result::KeyboardInteractivePrompt {
                request,
                response_tx,
            } => self
                .open_keyboard_interactive_challenge(request, response_tx, cx)
                .then_some(Request::FocusAuthentication),
            Result::SecurityKeyProvider { response_tx } => {
                Some(Request::GetSecurityKeyProvider(response_tx))
            }
            Result::SecurityKeyPrompt {
                flow_id,
                touch,
                retry,
                response_tx,
            } => {
                let request = KeyboardInteractivePromptRequest {
                    flow_id: flow_id.clone(),
                    name: i18n.t(if touch {
                        "ssh.fido.touch_title"
                    } else {
                        "ssh.fido.pin_title"
                    }),
                    instructions: i18n.t(if touch {
                        "ssh.fido.touch_instructions"
                    } else if retry {
                        "ssh.fido.pin_retry"
                    } else {
                        "ssh.fido.pin_instructions"
                    }),
                    prompts: if touch {
                        Vec::new()
                    } else {
                        vec![oxideterm_ssh::KeyboardInteractivePrompt {
                            prompt: i18n.t("ssh.fido.pin_label"),
                            echo: false,
                        }]
                    },
                    chained: false,
                };
                let opened = self.open_keyboard_interactive_challenge(request, response_tx, cx);
                if opened && touch {
                    self.wait_for_security_key_touch(&flow_id, cx);
                }
                opened.then_some(Request::FocusAuthentication)
            }
            Result::SecurityKeyError { error, response_tx } => {
                let _ = response_tx.send(i18n.t(error.message_key()));
                None
            }
            Result::PasswordPrompt {
                node_id,
                prompt,
                response_tx,
            } => Some(Request::PasswordPrompt {
                node_id,
                prompt,
                response_tx,
            }),
            Result::PasswordAuthenticated { target, password } => {
                Some(Request::SaveAuthenticatedPassword { target, password })
            }
            Result::AuthenticationCompleted {
                node_id,
                connection_id,
                configured_credentials_confirmed,
            } => Some(Request::SaveConfirmedAuthentication {
                node_id,
                connection_id,
                configured_credentials_confirmed,
            }),
        }
    }

    fn worker_error(&mut self, message: String, cx: &mut Context<Self>) -> Option<Request> {
        if self.set_form_feedback(Some(false), Some(message.clone()), cx) {
            None
        } else {
            Some(Request::ConnectionStatus(Some(message)))
        }
    }

    fn sftp_connected(
        &mut self,
        endpoint: SftpConnected,
        cx: &mut Context<Self>,
    ) -> Option<Request> {
        self.clear_form_after_submit(cx);
        Some(Request::AdoptSftp(endpoint))
    }
}
