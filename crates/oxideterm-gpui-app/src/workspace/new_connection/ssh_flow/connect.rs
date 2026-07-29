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
            if let Err(error) = validate_proxy_chain_form(form) {
                form.error = Some(error);
                cx.notify();
                return None;
            }
            // Resolve every fallible proxy input before moving any form-owned secret.
            let upstream_proxy = match upstream_proxy_config_from_form(
                &this.connection_store,
                this.settings_store.settings(),
                form,
            ) {
                Ok(upstream_proxy) => upstream_proxy,
                Err(error) => {
                    form.error = Some(error.to_string());
                    cx.notify();
                    return None;
                }
            };
            let auth = match form.auth_tab {
                SshAuthTab::Password => {
                    AuthMethod::password_secret(take_zeroizing_secret(&mut form.password))
                }
                SshAuthTab::Agent => AuthMethod::Agent,
                SshAuthTab::DefaultKey => AuthMethod::key_secret(
                    "",
                    take_zeroizing_non_empty_secret(&mut form.passphrase),
                ),
                SshAuthTab::SshKey => AuthMethod::key_secret(
                    form.key_path.trim().to_string(),
                    take_zeroizing_non_empty_secret(&mut form.passphrase),
                ),
                SshAuthTab::ManagedKey => {
                    // Runtime auth carries only the managed-key reference and moved passphrase.
                    AuthMethod::managed_key_secret(
                        form.managed_key_id.trim().to_string(),
                        take_zeroizing_non_empty_secret(&mut form.passphrase),
                    )
                }
                SshAuthTab::Certificate => AuthMethod::certificate_secret(
                    form.key_path.trim().to_string(),
                    form.cert_path.trim().to_string(),
                    take_zeroizing_non_empty_secret(&mut form.passphrase),
                ),
                SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
            };
            let proxy_chain = proxy_chain_from_form(form)
                .expect("proxy-chain validation completed before secret handoff");
            let config = SshConfig {
                host: host.clone(),
                port: port.unwrap_or(22),
                username: username.clone(),
                auth,
                agent_forwarding: form.agent_forwarding,
                identity_agent: form.identity_agent.clone(),
                agent_forwarding_socket: form.agent_forwarding_socket.clone(),
                legacy_ssh_compatibility: form.legacy_ssh_compatibility,
                proxy_chain,
                upstream_proxy,
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
                    status,
                } => {
                    // Restore the only upstream-proxy value before continuing the flow.
                    config.upstream_proxy = upstream_proxy;
                    self.handle_ssh_preflight_result(config, title, intent, status, window, cx)
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
                    let (form_message, session_message) = match result {
                        Ok(()) => (
                            self.i18n.t("ssh.form.test_success"),
                            self.i18n.t("sessionManager.toast.test_success"),
                        ),
                        Err(error) => (
                            error.clone(),
                            format!(
                                "{}: {error}",
                                self.i18n.t("sessionManager.toast.test_failed")
                            ),
                        ),
                    };
                    let reported_to_form =
                        self.connection_flow.update(cx, |connection_flow, cx| {
                            connection_flow.set_form_feedback(Some(false), Some(form_message), cx)
                        });
                    if !reported_to_form {
                        self.session_manager.update(cx, |session_manager, cx| {
                            session_manager.set_status(Some(session_message), cx);
                        });
                    }
                    cx.notify();
                }
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
                let host = config.host.clone();
                let port = config.port;
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
                                    Duration::from_secs(30),
                                )
                                .await
                            {
                                Ok(parent) => {
                                    let connection_id = parent.connection_id.clone();
                                    let status = parent
                                        .handle
                                        .preflight_host_key_via_direct_tcpip(
                                            &step.host, step.port, 10,
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
                                10,
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
            SshConnectionIntent::Connect(terminal_options) => {
                if let Some(node) = self.ssh_nodes.get_mut(&target_node_id) {
                    // Manual connections have no saved record, so the runtime node
                    // retains their terminal behavior until all panes are closed.
                    node.terminal_options = terminal_options;
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
            SshConnectionIntent::Test | SshConnectionIntent::DrillDown { .. } => {}
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
            SshConnectionIntent::Connect(terminal_options) => {
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
                                node.terminal_options = terminal_options;
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
                    node.terminal_options = terminal_options;
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
                child_node.terminal_options = terminal_options;
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
        }
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
