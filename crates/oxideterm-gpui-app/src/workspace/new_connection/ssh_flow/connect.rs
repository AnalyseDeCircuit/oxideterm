// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn ssh_worker_sender(
        &self,
        cx: &App,
    ) -> ActiveDeliverySender<SshConnectionWorkerResult> {
        self.connection_flow.read(cx).ssh_worker_sender()
    }

    pub(in crate::workspace) fn apply_connection_flow_worker_delivery(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Secret-bearing worker results stay in ConnectionFlowEntity until the
        // registry supplies a live native window for their root adapter.
        let results = self.connection_flow.update(cx, |connection_flow, _cx| {
            connection_flow.take_worker_results()
        });
        self.apply_ssh_worker_results(results, window, cx);
    }

    pub(super) fn build_new_connection_config(
        &mut self,
        secret_handoff: RuntimeSecretHandoff,
        cx: &mut Context<Self>,
    ) -> Option<(SshConfig, String)> {
        self.with_connection_form_mut(cx, |this, form, cx| {
            let form = form?;
            let host = form.host.trim().to_string();
            let username = form.username.trim().to_string();
            let port = form.port.trim().parse::<u16>().ok();
            if host.is_empty() || username.is_empty() || port.is_none() {
                form.error = Some(this.i18n.t("ssh.form.validation_required"));
                cx.notify();
                return None;
            }
            match form.auth_tab {
                SshAuthTab::Password
                | SshAuthTab::Agent
                | SshAuthTab::DefaultKey
                | SshAuthTab::TwoFactor => {}
                SshAuthTab::SshKey => {
                    if form.key_path.trim().is_empty() {
                        form.error = Some(this.i18n.t("ssh.form.key_path_required"));
                        cx.notify();
                        return None;
                    }
                }
                SshAuthTab::ManagedKey => {
                    if form.managed_key_id.trim().is_empty() {
                        form.error = Some(this.i18n.t("ssh.form.managed_key_required"));
                        cx.notify();
                        return None;
                    }
                }
                SshAuthTab::Certificate => {
                    if form.key_path.trim().is_empty() || form.cert_path.trim().is_empty() {
                        form.error = Some(this.i18n.t("ssh.form.certificate_paths_required"));
                        cx.notify();
                        return None;
                    }
                }
            }
            let saved_proxy_hop_auth = if form.proxy_command_enabled {
                Vec::new()
            } else {
                if let Err(error) = validate_proxy_chain_form(form) {
                    form.error = Some(error);
                    cx.notify();
                    return None;
                }
                let missing_credentials_message =
                    this.i18n.t("sessions.saved_next_hop.missing_credentials");
                match saved_proxy_hop_auth_from_store(
                    &this.connection_store,
                    form,
                    &missing_credentials_message,
                ) {
                    Ok(saved_auth) => saved_auth,
                    Err(error) => {
                        form.error = Some(error);
                        cx.notify();
                        return None;
                    }
                }
            };
            // Resolve every fallible route input before moving authentication drafts.
            let proxy_command = if form.proxy_command_enabled {
                let command = if form.proxy_command.trim().is_empty() {
                    let saved_command = SavedProxyCommand {
                        keychain_id: form.proxy_command_keychain_id.clone(),
                        plaintext_command: None,
                    };
                    match this
                        .connection_store
                        .get_saved_proxy_command(&saved_command)
                    {
                        Ok(command) => command,
                        Err(error) => {
                            form.error = Some(error.to_string());
                            cx.notify();
                            return None;
                        }
                    }
                } else {
                    SecretString::from(secret_handoff.zeroizing(&mut form.proxy_command))
                };
                let proxy_alias = if form.name.trim().is_empty() {
                    host.as_str()
                } else {
                    form.name.trim()
                };
                Some(proxy_command_from_value(
                    this.settings_store
                        .settings()
                        .ssh_config
                        .allow_proxy_command,
                    command,
                    proxy_alias,
                    &host,
                    Some(&username),
                    port,
                ))
            } else {
                None
            };
            let upstream_proxy = if proxy_command.is_some() {
                None
            } else {
                match upstream_proxy_config_from_form(
                    &this.connection_store,
                    this.settings_store.settings(),
                    form,
                    secret_handoff,
                ) {
                    Ok(upstream_proxy) => upstream_proxy,
                    Err(error) => {
                        form.error = Some(error.to_string());
                        cx.notify();
                        return None;
                    }
                }
            };
            let fallback_auth = match form.auth_tab {
                SshAuthTab::Password => {
                    AuthMethod::password_secret(secret_handoff.zeroizing(&mut form.password))
                }
                SshAuthTab::Agent => AuthMethod::Agent,
                SshAuthTab::DefaultKey => AuthMethod::key_secret(
                    "",
                    secret_handoff.zeroizing_non_empty(&mut form.passphrase),
                ),
                SshAuthTab::SshKey => AuthMethod::key_secret(
                    form.key_path.trim().to_string(),
                    secret_handoff.zeroizing_non_empty(&mut form.passphrase),
                ),
                SshAuthTab::ManagedKey => {
                    // Runtime auth carries only the managed-key reference and runtime-owned passphrase.
                    AuthMethod::managed_key_secret(
                        form.managed_key_id.trim().to_string(),
                        secret_handoff.zeroizing_non_empty(&mut form.passphrase),
                    )
                }
                SshAuthTab::Certificate => AuthMethod::certificate_secret(
                    form.key_path.trim().to_string(),
                    form.cert_path.trim().to_string(),
                    secret_handoff.zeroizing_non_empty(&mut form.passphrase),
                ),
                SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
            };
            let auth = if form.gssapi_enabled {
                AuthMethod::kerberos_preferred(
                    fallback_auth,
                    (!form.gssapi_server_identity.trim().is_empty())
                        .then(|| form.gssapi_server_identity.trim().to_string()),
                    form.gssapi_delegate_credentials,
                )
            } else {
                fallback_auth
            };
            let proxy_chain = if proxy_command.is_some() {
                None
            } else {
                proxy_chain_from_form(form, secret_handoff, saved_proxy_hop_auth)
            };
            let config = SshConfig {
                host: host.clone(),
                port: port.unwrap_or(22),
                username: username.clone(),
                auth,
                timeout_secs: form.connect_timeout_seconds,
                agent_forwarding: form.agent_forwarding,
                identity_agent: identity_agent_from_form(&form.identity_agent),
                agent_forwarding_socket: form.agent_forwarding_socket.clone(),
                legacy_ssh_compatibility: form.legacy_ssh_compatibility,
                ssh_algorithms: form.ssh_algorithms.clone(),
                x11_forwarding: x11_forward_policy(form.x11_forwarding),
                proxy_chain,
                upstream_proxy,
                proxy_command,
                strict_host_key_checking: true,
                post_connect_command: (!form.post_connect_command.trim().is_empty())
                    .then(|| form.post_connect_command.trim().to_string()),
                ..SshConfig::default()
            };
            let title = if form.name.trim().is_empty() {
                format!("{username}@{host}")
            } else {
                form.name.trim().to_string()
            };
            Some((config, title))
        })
    }

    pub(in crate::workspace) fn apply_ssh_worker_results(
        &mut self,
        results: std::collections::VecDeque<SshConnectionWorkerResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for result in results {
            match result {
                SshConnectionWorkerResult::Preflight {
                    mut config,
                    upstream_proxy,
                    title,
                    intent,
                    host,
                    port,
                    status,
                } => {
                    // Restore the only upstream-proxy value before continuing the flow.
                    config.upstream_proxy = upstream_proxy;
                    self.handle_ssh_preflight_result(
                        config, title, intent, host, port, status, window, cx,
                    )
                }
                SshConnectionWorkerResult::SessionTreePreflight {
                    generation,
                    step_index,
                    upstream_proxy,
                    status,
                } => self.handle_session_tree_preflight_result(
                    generation,
                    step_index,
                    upstream_proxy,
                    status,
                    window,
                    cx,
                ),
                SshConnectionWorkerResult::Test { result } => {
                    match result {
                        Ok(()) => {
                            let form_message = self.i18n.t("ssh.form.test_success");
                            let session_message = self.i18n.t("sessionManager.toast.test_success");
                            // Preserve the existing inline location while selecting success chrome.
                            let reported_to_form =
                                self.connection_flow.update(cx, |connection_flow, cx| {
                                    connection_flow.set_form_success_feedback(form_message, cx)
                                });
                            if !reported_to_form {
                                self.session_manager.update(cx, |session_manager, cx| {
                                    session_manager.set_status(Some(session_message), cx);
                                });
                            }
                        }
                        Err(error) => {
                            let session_message = format!(
                                "{}: {error}",
                                self.i18n.t("sessionManager.toast.test_failed")
                            );
                            let reported_to_form =
                                self.connection_flow.update(cx, |connection_flow, cx| {
                                    connection_flow.set_form_feedback(Some(false), Some(error), cx)
                                });
                            if !reported_to_form {
                                self.session_manager.update(cx, |session_manager, cx| {
                                    session_manager.set_status(Some(session_message), cx);
                                });
                            }
                        }
                    }
                    cx.notify();
                }
                SshConnectionWorkerResult::StandaloneSftpConnected {
                    endpoint_id,
                    saved_profile_id,
                    title,
                    initial_remote_path,
                    consumer,
                    result,
                } => match result {
                    Ok(handle) => {
                        let connection_id = handle.connection_id().to_string();
                        self.standalone_sftp_sessions.insert(
                            endpoint_id.clone(),
                            crate::workspace::sftp::StandaloneSftpRuntime {
                                connection_id,
                                consumer,
                                handle,
                                title: title.clone(),
                            },
                        );
                        if let Some(profile_id) = saved_profile_id {
                            let _ = self
                                .connection_store
                                .mark_standalone_sftp_profile_used(&profile_id);
                        }
                        self.session_manager.update(cx, |session_manager, cx| {
                            session_manager.set_status(None, cx);
                        });
                        self.update_connection_form_state(cx, ConnectionFormState::clear);
                        self.close_new_connection_select(cx);
                        self.open_standalone_sftp_tab_surface(
                            endpoint_id,
                            title,
                            initial_remote_path,
                            cx,
                        );
                    }
                    Err(error) => {
                        let reported_to_form =
                            self.connection_flow.update(cx, |connection_flow, cx| {
                                connection_flow.set_form_feedback(
                                    Some(false),
                                    Some(error.clone()),
                                    cx,
                                )
                            });
                        if !reported_to_form {
                            self.session_manager.update(cx, |session_manager, cx| {
                                session_manager.set_status(Some(error), cx);
                            });
                        }
                    }
                },
                SshConnectionWorkerResult::StandaloneSftpPairConnected {
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
                    Ok((primary_handle, secondary_handle)) => {
                        self.standalone_sftp_sessions.insert(
                            primary_endpoint_id.clone(),
                            crate::workspace::sftp::StandaloneSftpRuntime {
                                connection_id: primary_handle.connection_id().to_string(),
                                consumer: primary_consumer,
                                handle: primary_handle,
                                title: title.clone(),
                            },
                        );
                        self.standalone_sftp_sessions.insert(
                            secondary_endpoint_id.clone(),
                            crate::workspace::sftp::StandaloneSftpRuntime {
                                connection_id: secondary_handle.connection_id().to_string(),
                                consumer: secondary_consumer,
                                handle: secondary_handle,
                                title: title.clone(),
                            },
                        );
                        let _ = self
                            .connection_store
                            .mark_standalone_sftp_profile_used(&saved_profile_id);
                        self.session_manager.update(cx, |session_manager, cx| {
                            session_manager.set_status(None, cx);
                        });
                        self.update_connection_form_state(cx, ConnectionFormState::clear);
                        self.close_new_connection_select(cx);
                        self.open_standalone_sftp_pair_tab_surface(
                            primary_endpoint_id,
                            secondary_endpoint_id,
                            title,
                            primary_initial_remote_path,
                            secondary_initial_remote_path,
                            cx,
                        );
                    }
                    Err(error) => {
                        let reported_to_form =
                            self.connection_flow.update(cx, |connection_flow, cx| {
                                connection_flow.set_form_feedback(
                                    Some(false),
                                    Some(error.clone()),
                                    cx,
                                )
                            });
                        if !reported_to_form {
                            self.session_manager.update(cx, |session_manager, cx| {
                                session_manager.set_status(Some(error), cx);
                            });
                        }
                    }
                },
                SshConnectionWorkerResult::KeyboardInteractivePrompt {
                    request,
                    response_tx,
                } => {
                    self.open_keyboard_interactive_challenge(request, response_tx, window, cx);
                }
            }
        }
    }

    pub(super) fn handle_ssh_preflight_result(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        host: String,
        port: u16,
        status: HostKeyStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.pending = false;
                form.error = None;
            }
        });

        match status {
            HostKeyStatus::Verified => {
                self.continue_verified_ssh_flow(config, title, intent, window, cx)
            }
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                self.prepare_modal_interaction_boundary(cx);
                let challenge = HostKeyChallenge {
                    presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
                    config,
                    title,
                    status,
                    intent,
                    session_tree_challenge: false,
                    host,
                    port,
                };
                self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.open_host_key_challenge(challenge, cx);
                });
                self.needs_active_pane_focus = false;
                cx.notify();
            }
            HostKeyStatus::Error { message } => {
                if let Some(token) = intent.standalone_sftp_pair_launch_token() {
                    self.pending_standalone_sftp_pair_launches.remove(token);
                }
                self.fail_public_mcp_mosh_open_for_intent(&intent, message.clone());
                let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.set_form_feedback(None, Some(message.clone()), cx)
                });
                if !reported_to_form {
                    self.session_manager.update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(message), cx);
                    });
                }
                cx.notify();
            }
        }
    }

    pub(super) fn start_proxy_session_tree_connect(
        &mut self,
        mut config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        save_after_open: Option<SaveConnectionRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.connection_flow.read(cx).has_active_proxy_connect_run() {
            self.report_proxy_session_tree_error(
                "CHAIN_LOCK_BUSY: Another connection chain is in progress".to_string(),
                cx,
            );
            return;
        }
        let upstream_proxy = config.upstream_proxy.take();
        let endpoints = proxy_session_tree_endpoints(&config);
        let expansion_id = match &intent {
            SshConnectionIntent::ConnectSaved(id) => id.clone(),
            _ => format!("manual-{}", self.next_ssh_node_id),
        };
        let expansion =
            match self.expand_saved_connection_tree(&expansion_id, config, title.clone()) {
                Ok(expansion) => expansion,
                Err(error) => {
                    self.report_proxy_session_tree_error(error.to_string(), cx);
                    return;
                }
            };
        let cleanup_node_id = Some(expansion.target_node_id.clone());
        let plan = match NativeSessionTreeConnectPlan::from_expansion(
            &expansion,
            endpoints,
            cleanup_node_id,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                self.report_proxy_session_tree_error(error, cx);
                return;
            }
        };
        let run = NativeProxyConnectRun {
            generation: 0,
            plan,
            title,
            intent,
            save_after_open,
            upstream_proxy,
        };
        let _ = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.start_proxy_connect_run(run, cx)
        });
        self.continue_active_proxy_session_tree_connect(window, cx);
    }

    pub(in crate::workspace) fn start_existing_session_tree_connect(
        &mut self,
        target_node_id: NodeId,
        title: String,
        intent: SshConnectionIntent,
        save_after_open: &mut Option<SaveConnectionRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.connection_flow.read(cx).has_active_proxy_connect_run()
            || self
                .workspace_runtime
                .read(cx)
                .has_active_connection_chain()
        {
            self.report_proxy_session_tree_error(
                "CHAIN_LOCK_BUSY: Another connection chain is in progress".to_string(),
                cx,
            );
            return true;
        }

        let Ok(path_node_ids) = self.node_router.path_to_node(&target_node_id) else {
            self.report_proxy_session_tree_error(
                format!("Node path not found for {}", target_node_id.0),
                cx,
            );
            return true;
        };
        if path_node_ids.len() <= 1 {
            return false;
        }

        let start_index = path_node_ids
            .iter()
            .position(|candidate| !self.connection_trace_node_is_ready(candidate))
            .unwrap_or(path_node_ids.len());
        let nodes_to_connect = path_node_ids[start_index..].to_vec();
        if nodes_to_connect.is_empty() {
            return false;
        }
        if self
            .workspace_runtime
            .read(cx)
            .any_connecting_node_is_locked(&nodes_to_connect)
        {
            self.report_proxy_session_tree_error(
                "NODE_LOCK_BUSY: Node is already connecting".to_string(),
                cx,
            );
            return true;
        }

        let mut endpoints = Vec::with_capacity(nodes_to_connect.len());
        for node_id in &nodes_to_connect {
            let Some(node) = self.ssh_nodes.get(node_id) else {
                self.report_proxy_session_tree_error(
                    format!("SSH node {} not found", node_id.0),
                    cx,
                );
                return true;
            };
            endpoints.push(NativeSessionTreeConnectEndpoint::new(
                node.endpoint.host.clone(),
                node.endpoint.port,
            ));
        }

        let upstream_proxy = nodes_to_connect.first().and_then(|node_id| {
            self.node_router
                .node_runtime_snapshot(node_id)
                .filter(|snapshot| snapshot.parent_id.is_none())
                .and_then(|snapshot| snapshot.config.upstream_proxy)
        });
        let expansion = NodeTreeExpansion {
            target_node_id: target_node_id,
            path_node_ids: nodes_to_connect,
            chain_depth: path_node_ids.len() as u32,
        };
        let plan = match NativeSessionTreeConnectPlan::from_expansion(&expansion, endpoints, None) {
            Ok(plan) => plan,
            Err(error) => {
                self.report_proxy_session_tree_error(error, cx);
                return true;
            }
        };

        let run = NativeProxyConnectRun {
            generation: 0,
            plan,
            title,
            intent,
            save_after_open: save_after_open.take(),
            upstream_proxy,
        };
        let _ = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.start_proxy_connect_run(run, cx)
        });
        self.continue_active_proxy_session_tree_connect(window, cx);
        true
    }

    pub(super) fn handle_session_tree_preflight_result(
        &mut self,
        generation: u64,
        step_index: usize,
        upstream_proxy: Option<UpstreamProxyConfig>,
        status: HostKeyStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result_is_current = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.restore_proxy_connect_preflight_context(
                ProxyConnectPreflightContext {
                    generation,
                    step_index,
                    upstream_proxy,
                },
                cx,
            )
        });
        if !result_is_current {
            return;
        }
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                form.pending = false;
                form.error = None;
            }
        });

        match status {
            HostKeyStatus::Verified => {
                self.mark_current_proxy_connect_step_verified(cx);
                self.continue_active_proxy_session_tree_connect(window, cx);
            }
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                self.prepare_modal_interaction_boundary(cx);
                let challenge_opened = self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.open_active_proxy_host_key_challenge(status, cx)
                });
                if !challenge_opened {
                    return;
                }
                self.needs_active_pane_focus = false;
                cx.notify();
            }
            HostKeyStatus::Error { message } => {
                self.cancel_active_proxy_connect_run(cx);
                self.report_proxy_session_tree_error(message, cx);
            }
        }
    }

    pub(in crate::workspace) fn continue_active_proxy_session_tree_connect(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.connection_flow.read(cx).proxy_connect_next_action() else {
            return;
        };
        match action {
            NativeSessionTreeConnectAction::Preflight { step } => {
                self.start_session_tree_step_preflight(step, cx);
            }
            NativeSessionTreeConnectAction::Connect { step } => {
                self.connect_session_tree_step(step, window, cx);
            }
            NativeSessionTreeConnectAction::Complete { target_node_id } => {
                let Some(run) = self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.take_active_proxy_connect_run(cx)
                }) else {
                    return;
                };
                self.finish_proxy_session_tree_connect(target_node_id, run, window, cx);
            }
        }
    }

    pub(in crate::workspace) fn continue_active_proxy_session_tree_preflight_only(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.connection_flow.read(cx).proxy_connect_next_action() else {
            return;
        };
        match action {
            NativeSessionTreeConnectAction::Preflight { step } => {
                self.start_session_tree_step_preflight(step, cx);
            }
            _ => self.report_proxy_session_tree_error(
                "proxy connect plan is not waiting for host-key preflight".to_string(),
                cx,
            ),
        }
    }

    pub(super) fn start_session_tree_step_preflight(
        &mut self,
        step: NativeSessionTreeConnectStep,
        cx: &mut Context<Self>,
    ) {
        let message = self.i18n.t("ssh.form.checking_host_key");
        let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.set_form_feedback(Some(true), Some(message.clone()), cx)
        });
        if !reported_to_form {
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(message), cx);
            });
        }
        let tx = self.ssh_worker_sender(cx);
        let router = self.node_router.clone();
        let connect_timeout_seconds = router
            .node_runtime_snapshot(&step.node_id)
            .map(|snapshot| snapshot.config.timeout_secs)
            .unwrap_or(DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS);
        let Some(preflight_context) = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.take_proxy_connect_preflight_context(cx)
        }) else {
            return;
        };
        let ProxyConnectPreflightContext {
            generation,
            step_index,
            upstream_proxy,
        } = preflight_context;
        std::thread::spawn(move || {
            let status = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime.block_on(async {
                    match router
                        .node_runtime_snapshot(&step.node_id)
                        .and_then(|snapshot| snapshot.parent_id)
                    {
                        Some(parent_id) => {
                            let consumer = ConnectionConsumer::NodeRouter(format!(
                                "{}:preflight",
                                step.node_id.0
                            ));
                            match router
                                .acquire_connection_wait(
                                    &parent_id,
                                    consumer.clone(),
                                    Duration::from_secs(connect_timeout_seconds),
                                )
                                .await
                            {
                                Ok(parent) => {
                                    let connection_id = parent.connection_id.clone();
                                    let status = parent
                                        .handle
                                        .preflight_host_key_via_direct_tcpip(
                                            &step.host,
                                            step.port,
                                            connect_timeout_seconds,
                                        )
                                        .await;
                                    router.release_consumer(&connection_id, &consumer);
                                    status
                                }
                                Err(error) => HostKeyStatus::Error {
                                    message: error.to_string(),
                                },
                            }
                        }
                        None => {
                            // The root ProxyJump step uses the same initial TCP outlet
                            // as the eventual SSH connection; child steps keep using
                            // parent direct-tcpip streams.
                            check_host_key_with_upstream_proxy(
                                &step.host,
                                step.port,
                                connect_timeout_seconds,
                                upstream_proxy.as_ref(),
                            )
                            .await
                        }
                    }
                }),
                Err(error) => HostKeyStatus::Error {
                    message: format!("failed to initialize SSH runtime: {error}"),
                },
            };
            let _ = tx.send(SshConnectionWorkerResult::SessionTreePreflight {
                generation,
                step_index,
                upstream_proxy,
                status,
            });
        });
        cx.notify();
    }

    pub(super) fn connect_session_tree_step(
        &mut self,
        step: NativeSessionTreeConnectStep,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.connection_trace_node_is_ready(&step.node_id) {
            self.connection_flow.update(cx, |connection_flow, cx| {
                connection_flow
                    .advance_active_proxy_connect_after_node_connected(&step.node_id, cx);
            });
            self.continue_active_proxy_session_tree_connect(_window, cx);
            cx.notify();
            return;
        }

        let upstream_proxy = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.take_active_proxy_upstream_proxy_for_node(&step.node_id, cx)
        });
        self.apply_session_tree_step_connection_options(&step, upstream_proxy);
        if !self.ensure_node_connection_started_without_ancestors(&step.node_id, cx) {
            self.cancel_active_proxy_connect_run(cx);
            self.report_proxy_session_tree_error(
                format!("failed to start SSH node {}", step.node_id.0),
                cx,
            );
        }
    }

    pub(super) fn finish_proxy_session_tree_connect(
        &mut self,
        target_node_id: NodeId,
        run: NativeProxyConnectRun,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.clear_host_key_challenge(cx);
        });
        self.release_proxy_session_tree_locks(&run.plan, cx);
        let Some(target_config) = self
            .node_router
            .node_runtime_snapshot(&target_node_id)
            .map(|snapshot| snapshot.config)
        else {
            self.report_proxy_session_tree_error(
                "target node was not materialized".to_string(),
                cx,
            );
            return;
        };

        match run.intent {
            SshConnectionIntent::Connect(connection_options) => {
                if let Some(node) = self.ssh_nodes.get_mut(&target_node_id) {
                    // Manual connections have no saved record, so the runtime node
                    // retains their terminal and SSH connection behavior until all panes close.
                    node.terminal_options = connection_options.terminal;
                    node.dedicated_new_terminal_connection =
                        connection_options.dedicated_new_terminal_connection;
                }
                self.update_connection_form_state(cx, ConnectionFormState::clear);
                let post_connect_command = target_config.post_connect_command.clone();
                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    target_node_id,
                    post_connect_command,
                    target_config,
                    run.title,
                    None,
                    None,
                    run.save_after_open,
                    window,
                    cx,
                );
            }
            SshConnectionIntent::ConnectSaved(id) => {
                if let Some(connection_options) = self.connection_store.get(&id).map(|connection| {
                    (
                        connection.options.terminal.clone(),
                        connection.options.dedicated_new_terminal_connection,
                    )
                }) && let Some(node) = self.ssh_nodes.get_mut(&target_node_id)
                {
                    // The expanded target must keep the saved host's terminal
                    // connection policy after its proxy path is materialized.
                    node.terminal_options = connection_options.0;
                    node.dedicated_new_terminal_connection = connection_options.1;
                }
                if self.connection_form_state(cx).form.is_some() {
                    self.update_connection_form_state(cx, ConnectionFormState::clear);
                }
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(None, cx);
                });
                let post_connect_command = target_config.post_connect_command.clone();
                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    target_node_id,
                    post_connect_command,
                    target_config,
                    run.title,
                    Some(id.clone()),
                    Some(id),
                    None,
                    window,
                    cx,
                );
            }
            SshConnectionIntent::Test
            | SshConnectionIntent::TestStandaloneSftp
            | SshConnectionIntent::DrillDown { .. }
            | SshConnectionIntent::Mosh(_)
            | SshConnectionIntent::StandaloneSftp { .. }
            | SshConnectionIntent::StandaloneSftpSecondary { .. } => {}
        }
    }

    pub(in crate::workspace) fn active_proxy_connect_waits_for_node(
        &self,
        node_id: &NodeId,
        cx: &App,
    ) -> bool {
        self.connection_flow
            .read(cx)
            .active_proxy_connect_waits_for_node(node_id)
    }

    pub(in crate::workspace) fn advance_active_proxy_connect_after_node_connected(
        &mut self,
        node_id: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let advanced = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.advance_active_proxy_connect_after_node_connected(node_id, cx)
        });
        if !advanced {
            return;
        }
        self.continue_active_proxy_session_tree_connect(window, cx);
    }

    pub(in crate::workspace) fn fail_active_proxy_connect_for_node(
        &mut self,
        node_id: &NodeId,
        error: String,
        cx: &mut Context<Self>,
    ) {
        if self.active_proxy_connect_waits_for_node(node_id, cx) {
            self.cancel_active_proxy_connect_run(cx);
            self.report_proxy_session_tree_error(error, cx);
        }
    }

    pub(in crate::workspace) fn accept_active_proxy_connect_host_key(
        &mut self,
        persist: bool,
        fingerprint: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.accept_active_proxy_connect_host_key(persist, fingerprint, cx)
        });
        if let Err(error) = result {
            self.report_proxy_session_tree_error(error, cx);
            return;
        }
        self.continue_active_proxy_session_tree_connect(window, cx);
    }

    pub(super) fn mark_current_proxy_connect_step_verified(&mut self, cx: &mut Context<Self>) {
        let result = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.mark_current_proxy_connect_step_verified(cx)
        });
        if let Err(error) = result {
            self.report_proxy_session_tree_error(error, cx);
        }
    }

    pub(super) fn apply_session_tree_step_connection_options(
        &mut self,
        step: &NativeSessionTreeConnectStep,
        upstream_proxy: Option<UpstreamProxyConfig>,
    ) {
        if step.trust_host_key.is_none() && upstream_proxy.is_none() {
            return;
        }
        if let Some(snapshot) = self.node_router.node_runtime_snapshot(&step.node_id) {
            let mut config = snapshot.config;
            if let Some(upstream_proxy) = upstream_proxy {
                // The root hop receives the same proxy value that preflight borrowed.
                config.upstream_proxy = Some(upstream_proxy);
            }
            if let (Some(trust_host_key), Some(fingerprint)) = (
                step.trust_host_key,
                step.expected_host_key_fingerprint.clone(),
            ) {
                config.strict_host_key_checking = true;
                config.trust_host_key = Some(trust_host_key);
                config.expected_host_key_fingerprint = Some(fingerprint);
            }
            // Tauri passes host-key acceptance as connectNode options. Native
            // stores the same one-step options on the node config immediately
            // before starting connect_tree_node.
            self.node_router
                .upsert_node_with_origin(step.node_id.clone(), config, snapshot.origin);
            self.persist_session_tree_snapshot();
        }
    }

    pub(in crate::workspace) fn cancel_active_proxy_connect_run(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.take_active_proxy_connect_run(cx)
        }) else {
            return;
        };
        self.cleanup_proxy_session_tree_run(&run, cx);
    }

    pub(in crate::workspace) fn cleanup_cancelled_proxy_connect_runs(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let runs = self.connection_flow.update(cx, |connection_flow, _cx| {
            connection_flow.take_cancelled_proxy_connect_runs()
        });
        for run in runs {
            self.cleanup_proxy_session_tree_run(&run, cx);
        }
    }

    pub(super) fn cleanup_proxy_session_tree_run(
        &mut self,
        run: &NativeProxyConnectRun,
        cx: &mut Context<Self>,
    ) {
        self.cleanup_proxy_session_tree_plan(&run.plan, cx);
    }

    pub(super) fn cleanup_proxy_session_tree_plan(
        &mut self,
        plan: &NativeSessionTreeConnectPlan,
        cx: &mut Context<Self>,
    ) {
        self.release_proxy_session_tree_locks(plan, cx);
        let Some(cleanup_root) = plan.cleanup_root_node_id() else {
            return;
        };
        let removed_nodes = self.workspace_runtime.update(cx, |runtime, cx| {
            runtime.remove_node_runtime_subtree(&cleanup_root, cx)
        });
        for node_id in removed_nodes {
            self.workspace_runtime.update(cx, |runtime, _cx| {
                runtime.remove_pending_ssh_terminal_opens_for_node(&node_id);
            });
            self.ssh_nodes.remove(&node_id);
            self.expanded_ssh_nodes.remove(&node_id);
            self.saved_ssh_nodes
                .retain(|_saved_id, saved_node_id| saved_node_id != &node_id);
        }
        self.persist_session_tree_snapshot();
    }

    pub(super) fn release_proxy_session_tree_locks(
        &mut self,
        plan: &NativeSessionTreeConnectPlan,
        cx: &mut Context<Self>,
    ) {
        self.workspace_runtime.update(cx, |runtime, _cx| {
            for step in &plan.steps {
                runtime.unlock_connecting_node(&step.node_id);
            }
        });
    }

    pub(super) fn report_proxy_session_tree_error(
        &mut self,
        error: String,
        cx: &mut Context<Self>,
    ) {
        let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.set_form_feedback(Some(false), Some(error.clone()), cx)
        });
        if !reported_to_form {
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(error), cx);
            });
        }
        cx.notify();
    }

    pub(in crate::workspace) fn start_ssh_test_flow(
        &mut self,
        mut config: SshConfig,
        title: String,
        cx: &mut Context<Self>,
    ) {
        if config
            .proxy_chain
            .as_ref()
            .is_some_and(|chain| !chain.is_empty())
        {
            prepare_proxy_chain_test_config(&mut config);
            self.start_ssh_test(config, cx);
            return;
        }

        let message = self.i18n.t("ssh.form.checking_host_key");
        let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.set_form_feedback(Some(true), Some(message.clone()), cx)
        });
        if !reported_to_form {
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(message), cx);
            });
        }
        self.start_ssh_preflight(config, title, SshConnectionIntent::Test, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn continue_verified_ssh_flow(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match intent {
            SshConnectionIntent::Connect(connection_options) => {
                self.update_connection_form_state(cx, ConnectionFormState::clear);
                self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.clear_host_key_challenge(cx);
                });
                self.close_new_connection_select(cx);
                if config
                    .proxy_chain
                    .as_ref()
                    .is_some_and(|chain| !chain.is_empty())
                {
                    let expansion_id = format!("manual-{}", self.next_ssh_node_id);
                    match self.expand_saved_connection_tree(&expansion_id, config, title.clone()) {
                        Ok(expansion) => {
                            if let Some(node) = self.ssh_nodes.get_mut(&expansion.target_node_id) {
                                node.terminal_options = connection_options.terminal;
                                node.dedicated_new_terminal_connection =
                                    connection_options.dedicated_new_terminal_connection;
                            }
                            if let Some(target_config) = self
                                .node_router
                                .node_runtime_snapshot(&expansion.target_node_id)
                                .map(|snapshot| snapshot.config)
                            {
                                let post_connect_command =
                                    target_config.post_connect_command.clone();
                                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                                    expansion.target_node_id,
                                    post_connect_command,
                                    target_config,
                                    title,
                                    None,
                                    None,
                                    None,
                                    window,
                                    cx,
                                );
                            }
                        }
                        Err(error) => {
                            let message = error.to_string();
                            self.session_manager.update(cx, |session_manager, cx| {
                                session_manager.set_status(Some(message), cx);
                            });
                        }
                    }
                    return;
                }
                let node_id = self.materialize_ssh_root_node(config.clone(), title.clone(), None);
                if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                    node.terminal_options = connection_options.terminal;
                    node.dedicated_new_terminal_connection =
                        connection_options.dedicated_new_terminal_connection;
                }
                let post_connect_command = config.post_connect_command.clone();
                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    node_id,
                    post_connect_command,
                    config,
                    title,
                    None,
                    None,
                    None,
                    window,
                    cx,
                );
            }
            SshConnectionIntent::ConnectSaved(id) => {
                self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.clear_host_key_challenge(cx);
                });
                if self.connection_form_state(cx).form.is_some() {
                    self.update_connection_form_state(cx, ConnectionFormState::clear);
                }
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(None, cx);
                });
                let _ = self.open_or_create_saved_ssh_terminal_tab(id, config, title, window, cx);
            }
            SshConnectionIntent::Mosh(options) => {
                let public_mcp_open_token = options.public_mcp_open_token.clone();
                if public_mcp_open_token
                    .as_deref()
                    .is_some_and(|token| self.cancel_public_mcp_mosh_open_if_request_ended(token))
                {
                    return;
                }
                self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.clear_host_key_challenge(cx);
                });
                if self.connection_form_state(cx).form.is_some() {
                    self.update_connection_form_state(cx, ConnectionFormState::clear);
                }
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(None, cx);
                });

                let mut bootstrap = MoshBootstrapConfig::new("pending", config);
                bootstrap.server_executable = options.server_executable;
                bootstrap.udp_host_override = options.udp_host_override;
                bootstrap.udp_port = match options.udp_port {
                    SavedMoshUdpPortSelection::Automatic => {
                        oxideterm_mosh::MoshUdpPortSelection::Automatic
                    }
                    SavedMoshUdpPortSelection::Fixed { port } => {
                        oxideterm_mosh::MoshUdpPortSelection::Fixed(port)
                    }
                    SavedMoshUdpPortSelection::Range { start, end } => {
                        oxideterm_mosh::MoshUdpPortSelection::Range { start, end }
                    }
                };
                bootstrap.ip_family = match options.ip_family {
                    SavedMoshIpFamily::Auto => oxideterm_mosh::MoshIpFamily::Auto,
                    SavedMoshIpFamily::Ipv4 => oxideterm_mosh::MoshIpFamily::Ipv4,
                    SavedMoshIpFamily::Ipv6 => oxideterm_mosh::MoshIpFamily::Ipv6,
                };
                bootstrap.locale_assignments = options
                    .locale
                    .filter(|locale| !locale.trim().is_empty())
                    .map(|locale| vec![("LANG".to_string(), locale)])
                    .unwrap_or_default();
                let bootstrap_context = MoshBootstrapContext {
                    registry: self.ssh_registry.clone(),
                    prompt_handler: Some(
                        self.workspace_runtime.read(cx).native_ssh_prompt_handler(),
                    ),
                    managed_key_resolver: Some(managed_key_resolver_from_store(
                        &self.connection_store,
                    )),
                };
                let terminal_config = MoshTerminalConfig {
                    title: title.clone(),
                    bootstrap,
                    bootstrap_context,
                    prediction: match options.prediction {
                        MoshPredictionMode::Adaptive => {
                            oxideterm_terminal::MoshPredictionDisplay::Adaptive
                        }
                        MoshPredictionMode::Always => {
                            oxideterm_terminal::MoshPredictionDisplay::Always
                        }
                        MoshPredictionMode::Never => {
                            oxideterm_terminal::MoshPredictionDisplay::Never
                        }
                    },
                    task_runtime: self.workspace_runtime.read(cx).task_runtime(),
                };
                match self.create_mosh_terminal_tab(
                    terminal_config,
                    options.terminal,
                    title,
                    window,
                    cx,
                ) {
                    Ok(session_id) => {
                        if let Some(saved_profile_id) = options.saved_profile_id {
                            self.register_terminal_saved_connection(
                                session_id,
                                oxideterm_terminal_triggers::SavedConnectionKind::Mosh,
                                saved_profile_id.clone(),
                                cx,
                            );
                            let _ = self
                                .connection_store
                                .mark_mosh_profile_used(&saved_profile_id);
                        }
                        if let Some(token) = public_mcp_open_token {
                            self.complete_public_mcp_mosh_terminal_open(token, Ok(session_id), cx);
                        }
                    }
                    Err(error) => {
                        let error = error.to_string();
                        if let Some(token) = public_mcp_open_token {
                            self.complete_public_mcp_mosh_terminal_open(
                                token,
                                Err(error.clone()),
                                cx,
                            );
                        }
                        self.session_manager.update(cx, |session_manager, cx| {
                            session_manager.set_status(Some(error), cx);
                        });
                    }
                }
            }
            SshConnectionIntent::StandaloneSftp {
                saved_profile_id,
                initial_remote_path,
                pair_launch_token,
            } => {
                self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.clear_host_key_challenge(cx);
                });
                if let Some(pair_launch_token) = pair_launch_token {
                    let Some(secondary_config) = self
                        .pending_standalone_sftp_pair_launches
                        .get_mut(&pair_launch_token)
                        .and_then(|launch| {
                            launch.primary_config = Some(config);
                            launch.secondary_config.take()
                        })
                    else {
                        self.pending_standalone_sftp_pair_launches
                            .remove(&pair_launch_token);
                        let message = self.i18n.t("sftp.standalone.missing_credentials");
                        self.connection_flow.update(cx, |connection_flow, cx| {
                            connection_flow.set_form_feedback(None, Some(message), cx)
                        });
                        cx.notify();
                        return;
                    };
                    self.start_ssh_preflight(
                        secondary_config,
                        title,
                        SshConnectionIntent::StandaloneSftpSecondary { pair_launch_token },
                        cx,
                    );
                    return;
                }
                let endpoint_id = saved_profile_id
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                if self.standalone_sftp_sessions.contains_key(&endpoint_id) {
                    self.update_connection_form_state(cx, ConnectionFormState::clear);
                    self.close_new_connection_select(cx);
                    self.open_standalone_sftp_tab_surface(
                        endpoint_id,
                        title,
                        initial_remote_path,
                        cx,
                    );
                    return;
                }

                let consumer = ConnectionConsumer::Sftp(format!("standalone-tab:{endpoint_id}"));
                let worker_registry = self.ssh_registry.clone();
                let tx = self.ssh_worker_sender(cx);
                let prompt_handler = Arc::new(NativeSshPromptHandler::new(tx.clone()));
                let managed_key_resolver = managed_key_resolver_from_store(&self.connection_store);
                let worker_consumer = consumer.clone();
                let worker_endpoint_id = endpoint_id.clone();
                let worker_title = title.clone();
                let worker_saved_profile_id = saved_profile_id.clone();
                let worker_initial_remote_path = initial_remote_path.clone();
                self.forwarding_runtime.spawn(async move {
                    let client = SshTransportClient::new(config)
                        .with_prompt_handler(prompt_handler)
                        .with_managed_key_resolver(managed_key_resolver);
                    let result = match client
                        .connect_dedicated_node_with_registry(
                            worker_registry.clone(),
                            worker_consumer.clone(),
                        )
                        .await
                    {
                        Ok(handle) => match handle.acquire_sftp().await {
                            Ok(_) => Ok(handle),
                            Err(error) => {
                                worker_registry.release(handle.connection_id(), &worker_consumer);
                                Err(error.to_string())
                            }
                        },
                        Err(error) => Err(error.to_string()),
                    };
                    let connected_id = result
                        .as_ref()
                        .ok()
                        .map(|handle| handle.connection_id().to_string());
                    if tx
                        .send(SshConnectionWorkerResult::StandaloneSftpConnected {
                            endpoint_id: worker_endpoint_id,
                            saved_profile_id: worker_saved_profile_id,
                            title: worker_title,
                            initial_remote_path: worker_initial_remote_path,
                            consumer: worker_consumer.clone(),
                            result,
                        })
                        .is_err()
                        && let Some(connection_id) = connected_id
                    {
                        // Failed delivery means no workspace runtime adopted the tab consumer.
                        worker_registry.release(&connection_id, &worker_consumer);
                    }
                });
            }
            SshConnectionIntent::StandaloneSftpSecondary { pair_launch_token } => {
                self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.clear_host_key_challenge(cx);
                });
                let Some(mut launch) = self
                    .pending_standalone_sftp_pair_launches
                    .remove(&pair_launch_token)
                else {
                    return;
                };
                let Some(primary_config) = launch.primary_config.take() else {
                    return;
                };
                let primary_endpoint_id = launch.saved_profile_id.clone();
                let secondary_endpoint_id = format!("{}:secondary", launch.saved_profile_id);
                let primary_consumer = ConnectionConsumer::Sftp(format!(
                    "standalone-tab:{primary_endpoint_id}:primary"
                ));
                let secondary_consumer = ConnectionConsumer::Sftp(format!(
                    "standalone-tab:{primary_endpoint_id}:secondary"
                ));
                let worker_registry = self.ssh_registry.clone();
                let tx = self.ssh_worker_sender(cx);
                let prompt_handler = Arc::new(NativeSshPromptHandler::new(tx.clone()));
                let managed_key_resolver = managed_key_resolver_from_store(&self.connection_store);
                let worker_primary_consumer = primary_consumer.clone();
                let worker_secondary_consumer = secondary_consumer.clone();
                self.forwarding_runtime.spawn(async move {
                    let primary_client = SshTransportClient::new(primary_config)
                        .with_prompt_handler(prompt_handler.clone())
                        .with_managed_key_resolver(managed_key_resolver.clone());
                    let result = match primary_client
                        .connect_dedicated_node_with_registry(
                            worker_registry.clone(),
                            worker_primary_consumer.clone(),
                        )
                        .await
                    {
                        Ok(primary_handle) => match primary_handle.acquire_sftp().await {
                            Ok(_) => {
                                let secondary_client = SshTransportClient::new(config)
                                    .with_prompt_handler(prompt_handler)
                                    .with_managed_key_resolver(managed_key_resolver);
                                match secondary_client
                                    .connect_dedicated_node_with_registry(
                                        worker_registry.clone(),
                                        worker_secondary_consumer.clone(),
                                    )
                                    .await
                                {
                                    Ok(secondary_handle) => {
                                        match secondary_handle.acquire_sftp().await {
                                            Ok(_) => Ok((primary_handle, secondary_handle)),
                                            Err(error) => {
                                                worker_registry.release(
                                                    secondary_handle.connection_id(),
                                                    &worker_secondary_consumer,
                                                );
                                                worker_registry.release(
                                                    primary_handle.connection_id(),
                                                    &worker_primary_consumer,
                                                );
                                                Err(error.to_string())
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        worker_registry.release(
                                            primary_handle.connection_id(),
                                            &worker_primary_consumer,
                                        );
                                        Err(error.to_string())
                                    }
                                }
                            }
                            Err(error) => {
                                worker_registry.release(
                                    primary_handle.connection_id(),
                                    &worker_primary_consumer,
                                );
                                Err(error.to_string())
                            }
                        },
                        Err(error) => Err(error.to_string()),
                    };
                    let connected_ids =
                        result
                            .as_ref()
                            .ok()
                            .map(|(primary_handle, secondary_handle)| {
                                (
                                    primary_handle.connection_id().to_string(),
                                    secondary_handle.connection_id().to_string(),
                                )
                            });
                    if tx
                        .send(SshConnectionWorkerResult::StandaloneSftpPairConnected {
                            saved_profile_id: launch.saved_profile_id,
                            title: launch.title,
                            primary_endpoint_id,
                            secondary_endpoint_id,
                            primary_initial_remote_path: launch.primary_initial_remote_path,
                            secondary_initial_remote_path: launch.secondary_initial_remote_path,
                            primary_consumer: worker_primary_consumer.clone(),
                            secondary_consumer: worker_secondary_consumer.clone(),
                            result,
                        })
                        .is_err()
                        && let Some((primary_connection_id, secondary_connection_id)) =
                            connected_ids
                    {
                        // Failed delivery leaves no workspace owner for either endpoint consumer.
                        worker_registry.release(&primary_connection_id, &worker_primary_consumer);
                        worker_registry
                            .release(&secondary_connection_id, &worker_secondary_consumer);
                    }
                });
            }
            SshConnectionIntent::DrillDown {
                parent_id,
                saved_connection_id,
                terminal_options,
            } => {
                self.connection_flow.update(cx, |connection_flow, cx| {
                    connection_flow.clear_host_key_challenge(cx);
                });
                let child_id = match self
                    .node_router
                    .drill_down_node(parent_id.clone(), config.clone())
                {
                    Ok(child_id) => child_id,
                    Err(error) => {
                        let message = error.to_string();
                        let reported_to_form =
                            self.connection_flow.update(cx, |connection_flow, cx| {
                                connection_flow.set_form_feedback(
                                    Some(false),
                                    Some(message.clone()),
                                    cx,
                                )
                            });
                        if !reported_to_form {
                            self.session_manager.update(cx, |session_manager, cx| {
                                session_manager.set_status(Some(message), cx);
                            });
                        }
                        cx.notify();
                        return;
                    }
                };
                let mut child_node = crate::workspace::WorkspaceSshNode::new(
                    saved_connection_id.clone(),
                    &config,
                    title,
                    Vec::new(),
                    NodeReadiness::Connecting,
                );
                child_node.terminal_options = terminal_options.terminal;
                child_node.dedicated_new_terminal_connection =
                    terminal_options.dedicated_new_terminal_connection;
                self.ssh_nodes.insert(child_id.clone(), child_node);
                if let Some(saved_connection_id) = saved_connection_id {
                    self.saved_ssh_nodes
                        .insert(saved_connection_id, child_id.clone());
                }
                self.expanded_ssh_nodes.insert(parent_id);
                self.expanded_ssh_nodes.insert(child_id.clone());
                self.active_ssh_node_id = Some(child_id.clone());
                self.update_connection_form_state(cx, ConnectionFormState::clear);
                let message = self.i18n.t("ssh.drill_down.connecting");
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(message), cx);
                });
                self.ensure_node_connection_started(&child_id, cx);
                self.persist_session_tree_snapshot();
            }
            SshConnectionIntent::Test => self.start_ssh_test(config, cx),
            SshConnectionIntent::TestStandaloneSftp => self.start_standalone_sftp_test(config, cx),
        }
    }

    fn start_standalone_sftp_test(&mut self, config: SshConfig, cx: &mut Context<Self>) {
        let message = self.i18n.t("ssh.form.test_running");
        self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.set_form_feedback(Some(true), Some(message), cx)
        });
        let tx = self.ssh_worker_sender(cx);
        let prompt_handler = Arc::new(NativeSshPromptHandler::new(tx.clone()));
        let managed_key_resolver = managed_key_resolver_from_store(&self.connection_store);
        let registry = self.ssh_registry.clone();
        let consumer =
            ConnectionConsumer::Sftp(format!("standalone-test:{}", uuid::Uuid::new_v4()));
        self.forwarding_runtime.spawn(async move {
            let client = SshTransportClient::new(config)
                .with_prompt_handler(prompt_handler)
                .with_managed_key_resolver(managed_key_resolver);
            let result = match client
                .connect_dedicated_node_with_registry(registry.clone(), consumer.clone())
                .await
            {
                Ok(handle) => {
                    let result = handle
                        .acquire_sftp()
                        .await
                        .map(|_| ())
                        .map_err(|error| error.to_string());
                    registry.release(handle.connection_id(), &consumer);
                    result
                }
                Err(error) => Err(error.to_string()),
            };
            let _ = tx.send(SshConnectionWorkerResult::Test { result });
        });
        cx.notify();
    }

    pub(in crate::workspace) fn start_ssh_test(
        &mut self,
        config: SshConfig,
        cx: &mut Context<Self>,
    ) {
        let message = self.i18n.t("ssh.form.test_running");
        let reported_to_form = self.connection_flow.update(cx, |connection_flow, cx| {
            connection_flow.set_form_feedback(Some(true), Some(message.clone()), cx)
        });
        if !reported_to_form {
            self.session_manager.update(cx, |session_manager, cx| {
                session_manager.set_status(Some(message), cx);
            });
        }
        let tx = self.ssh_worker_sender(cx);
        let managed_key_resolver = managed_key_resolver_from_store(&self.connection_store);
        std::thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(runtime) => {
                    let prompt_handler = Arc::new(NativeSshPromptHandler::new(tx.clone()));
                    runtime
                        .block_on(
                            SshTransportClient::new(config)
                                .with_prompt_handler(prompt_handler)
                                .with_managed_key_resolver(managed_key_resolver)
                                .test_connection(),
                        )
                        .map_err(|error| error.to_string())
                }
                Err(error) => Err(format!("failed to initialize SSH runtime: {error}")),
            };
            let _ = tx.send(SshConnectionWorkerResult::Test { result });
        });
        cx.notify();
    }
}
