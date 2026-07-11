// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn save_after_open_request_for_connect_intent(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<Option<SaveConnectionRequest>, ()> {
        let mode = new_connection_form_mode(
            self.editing_saved_connection_id.as_deref(),
            self.duplicating_saved_connection_id.as_deref(),
            self.saved_connection_prompt_action,
        );
        if !mode.stores_connection_on_connect()
            || !self
                .new_connection_form
                .as_ref()
                .is_some_and(|form| form.save_connection)
        {
            return Ok(None);
        }

        match self
            .new_connection_form
            .as_ref()
            .map(|form| save_request_from_form(form, None))
        {
            Some(Ok(request)) => Ok(Some(request)),
            Some(Err(error)) => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.error = Some(error.to_string());
                }
                cx.notify();
                Err(())
            }
            None => Ok(None),
        }
    }

    pub(super) fn build_new_connection_config(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(SshConfig, String)> {
        let Some(form) = self.new_connection_form.as_mut() else {
            return None;
        };
        let host = form.host.trim().to_string();
        let username = form.username.trim().to_string();
        let port = form.port.trim().parse::<u16>().ok();
        if host.is_empty() || username.is_empty() || port.is_none() {
            form.error = Some(self.i18n.t("ssh.form.validation_required"));
            cx.notify();
            return None;
        }
        let auth = match form.auth_tab {
            SshAuthTab::Password => {
                // UI inputs own plain String drafts; crossing into SSH auth moves a
                // zeroizing clone so backend tasks never retain a normal String password.
                AuthMethod::password_secret(zeroizing_secret_clone(&form.password))
            }
            SshAuthTab::Agent => AuthMethod::Agent,
            SshAuthTab::DefaultKey => {
                AuthMethod::key_secret("", zeroizing_non_empty_secret(&form.passphrase))
            }
            SshAuthTab::SshKey => {
                if form.key_path.trim().is_empty() {
                    form.error = Some(self.i18n.t("ssh.form.key_path_required"));
                    cx.notify();
                    return None;
                }
                AuthMethod::key_secret(
                    form.key_path.trim().to_string(),
                    zeroizing_non_empty_secret(&form.passphrase),
                )
            }
            SshAuthTab::ManagedKey => {
                if form.managed_key_id.trim().is_empty() {
                    form.error = Some(self.i18n.t("ssh.form.managed_key_required"));
                    cx.notify();
                    return None;
                }
                // The connection config carries only the managed-key reference; the
                // private key remains owned by the local managed keychain resolver.
                AuthMethod::managed_key_secret(
                    form.managed_key_id.trim().to_string(),
                    zeroizing_non_empty_secret(&form.passphrase),
                )
            }
            SshAuthTab::Certificate => {
                if form.key_path.trim().is_empty() || form.cert_path.trim().is_empty() {
                    form.error = Some(self.i18n.t("ssh.form.certificate_paths_required"));
                    cx.notify();
                    return None;
                }
                AuthMethod::certificate_secret(
                    form.key_path.trim().to_string(),
                    form.cert_path.trim().to_string(),
                    zeroizing_non_empty_secret(&form.passphrase),
                )
            }
            SshAuthTab::TwoFactor => AuthMethod::KeyboardInteractive,
        };
        let proxy_chain = match proxy_chain_from_form(form) {
            Ok(proxy_chain) => proxy_chain,
            Err(error) => {
                form.error = Some(error);
                cx.notify();
                return None;
            }
        };
        let upstream_proxy = match upstream_proxy_config_from_form(
            &self.connection_store,
            self.settings_store.settings(),
            form,
        ) {
            Ok(upstream_proxy) => upstream_proxy,
            Err(error) => {
                form.error = Some(error.to_string());
                cx.notify();
                return None;
            }
        };
        let config = SshConfig {
            host: host.clone(),
            port: port.unwrap_or(22),
            username: username.clone(),
            auth,
            agent_forwarding: form.agent_forwarding,
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
    }

    pub(in crate::workspace) fn poll_ssh_worker_results(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut results = Vec::new();
        loop {
            match self.ssh_worker_rx.try_recv() {
                Ok(result) => results.push(result),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    break;
                }
            }
        }

        for result in results {
            match result {
                SshConnectionWorkerResult::Preflight {
                    config,
                    title,
                    intent,
                    status,
                } => self.handle_ssh_preflight_result(config, title, intent, status, window, cx),
                SshConnectionWorkerResult::SessionTreePreflight { run, status } => {
                    self.handle_session_tree_preflight_result(run, status, window, cx)
                }
                SshConnectionWorkerResult::Test { result } => {
                    if let Some(form) = self.new_connection_form.as_mut() {
                        form.pending = false;
                        form.error = Some(match result {
                            Ok(()) => self.i18n.t("ssh.form.test_success"),
                            Err(error) => error,
                        });
                    } else {
                        self.session_manager.status = Some(match result {
                            Ok(()) => self.i18n.t("sessionManager.toast.test_success"),
                            Err(error) => format!(
                                "{}: {error}",
                                self.i18n.t("sessionManager.toast.test_failed")
                            ),
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
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = None;
        }

        match status {
            HostKeyStatus::Verified => {
                self.continue_verified_ssh_flow(config, title, intent, window, cx)
            }
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                self.prepare_modal_interaction_boundary();
                let host = config.host.clone();
                let port = config.port;
                self.host_key_challenge = Some(HostKeyChallenge {
                    presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
                    config,
                    title,
                    status,
                    intent,
                    session_tree_challenge: None,
                    host,
                    port,
                });
                self.needs_active_pane_focus = false;
                cx.notify();
            }
            HostKeyStatus::Error { message } => {
                if let Some(form) = self.new_connection_form.as_mut() {
                    form.error = Some(message);
                } else {
                    self.session_manager.status = Some(message);
                }
                cx.notify();
            }
        }
    }

    pub(super) fn start_proxy_session_tree_connect(
        &mut self,
        config: SshConfig,
        title: String,
        intent: SshConnectionIntent,
        save_after_open: Option<SaveConnectionRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_proxy_connect_run.is_some() {
            self.report_proxy_session_tree_error(
                "CHAIN_LOCK_BUSY: Another connection chain is in progress".to_string(),
                cx,
            );
            return;
        }
        let upstream_proxy = config.upstream_proxy.clone();
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
        self.active_proxy_connect_run = Some(NativeProxyConnectRun {
            plan,
            title,
            intent,
            save_after_open,
            upstream_proxy,
        });
        self.continue_active_proxy_session_tree_connect(window, cx);
    }

    pub(in crate::workspace) fn start_existing_session_tree_connect(
        &mut self,
        target_node_id: NodeId,
        title: String,
        intent: SshConnectionIntent,
        save_after_open: Option<SaveConnectionRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.active_proxy_connect_run.is_some() || self.active_connection_chain.is_some() {
            self.report_proxy_session_tree_error(
                "CHAIN_LOCK_BUSY: Another connection chain is in progress".to_string(),
                cx,
            );
            return true;
        }

        let Ok(path_node_ids) = self.node_runtime_store.path_to_node(&target_node_id) else {
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
        if nodes_to_connect
            .iter()
            .any(|node_id| self.connecting_node_locks.contains(node_id))
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
                node.config.host.clone(),
                node.config.port,
            ));
        }

        let upstream_proxy = nodes_to_connect.first().and_then(|node_id| {
            self.node_runtime_store
                .snapshot(node_id)
                .filter(|snapshot| snapshot.parent_id.is_none())
                .and_then(|_| {
                    self.ssh_nodes
                        .get(node_id)
                        .and_then(|node| node.config.upstream_proxy.clone())
                })
        });
        let expansion = NodeTreeExpansion {
            target_node_id: target_node_id.clone(),
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

        self.active_proxy_connect_run = Some(NativeProxyConnectRun {
            plan,
            title,
            intent,
            save_after_open,
            upstream_proxy,
        });
        self.continue_active_proxy_session_tree_connect(window, cx);
        true
    }

    pub(super) fn handle_session_tree_preflight_result(
        &mut self,
        run: NativeProxyConnectRun,
        status: HostKeyStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.active_proxy_connect_result_is_current(&run) {
            return;
        }
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = None;
        }

        match status {
            HostKeyStatus::Verified => {
                self.mark_current_proxy_connect_step_verified(cx);
                self.continue_active_proxy_session_tree_connect(window, cx);
            }
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                let Some(active_run) = self.active_proxy_connect_run.as_ref() else {
                    return;
                };
                let Ok(challenge) = active_run.plan.challenge_for_current_step(status) else {
                    return;
                };
                let title = active_run.title.clone();
                let intent = active_run.intent.clone();
                self.prepare_modal_interaction_boundary();
                self.host_key_challenge = Some(HostKeyChallenge {
                    presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
                    config: SshConfig::default(),
                    title,
                    status: challenge.status.clone(),
                    intent,
                    session_tree_challenge: Some(challenge.clone()),
                    host: challenge.step.host,
                    port: challenge.step.port,
                });
                self.needs_active_pane_focus = false;
                cx.notify();
            }
            HostKeyStatus::Error { message } => {
                self.cancel_active_proxy_connect_run();
                self.report_proxy_session_tree_error(message, cx);
            }
        }
    }

    pub(in crate::workspace) fn continue_active_proxy_session_tree_connect(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(run) = self.active_proxy_connect_run.clone() else {
            return;
        };
        match run.plan.next_action() {
            NativeSessionTreeConnectAction::Preflight { step } => {
                self.start_session_tree_step_preflight(run, step, cx);
            }
            NativeSessionTreeConnectAction::Connect { step } => {
                self.connect_session_tree_step(step, window, cx);
            }
            NativeSessionTreeConnectAction::Complete { target_node_id } => {
                self.finish_proxy_session_tree_connect(target_node_id, run, window, cx);
            }
        }
    }

    pub(in crate::workspace) fn continue_active_proxy_session_tree_preflight_only(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(run) = self.active_proxy_connect_run.clone() else {
            return;
        };
        match run.plan.next_action() {
            NativeSessionTreeConnectAction::Preflight { step } => {
                self.start_session_tree_step_preflight(run, step, cx);
            }
            _ => self.report_proxy_session_tree_error(
                "proxy connect plan is not waiting for host-key preflight".to_string(),
                cx,
            ),
        }
    }

    pub(super) fn start_session_tree_step_preflight(
        &mut self,
        run: NativeProxyConnectRun,
        step: NativeSessionTreeConnectStep,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = true;
            form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
        } else {
            self.session_manager.status = Some(self.i18n.t("ssh.form.checking_host_key"));
        }
        let tx = self.ssh_worker_tx.clone();
        let router = self.node_router.clone();
        let runtime_store = self.node_runtime_store.clone();
        std::thread::spawn(move || {
            let root_upstream_proxy = run.upstream_proxy.clone();
            let status = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime.block_on(async move {
                    match runtime_store
                        .snapshot(&step.node_id)
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
                                root_upstream_proxy.as_ref(),
                            )
                            .await
                        }
                    }
                }),
                Err(error) => HostKeyStatus::Error {
                    message: format!("failed to initialize SSH runtime: {error}"),
                },
            };
            let _ = tx.send(SshConnectionWorkerResult::SessionTreePreflight { run, status });
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
            if let Some(run) = self.active_proxy_connect_run.as_mut() {
                run.plan.advance_after_connected_step();
            }
            self.continue_active_proxy_session_tree_connect(_window, cx);
            cx.notify();
            return;
        }

        self.apply_session_tree_step_host_key_options(&step);
        if !self.ensure_node_connection_started_without_ancestors(&step.node_id) {
            self.cancel_active_proxy_connect_run();
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
        self.active_proxy_connect_run = None;
        self.host_key_challenge = None;
        self.release_proxy_session_tree_locks(&run.plan);
        let Some(target_config) = self
            .node_runtime_store
            .snapshot(&target_node_id)
            .map(|snapshot| snapshot.config)
        else {
            self.report_proxy_session_tree_error(
                "target node was not materialized".to_string(),
                cx,
            );
            return;
        };

        match run.intent {
            SshConnectionIntent::Connect => {
                self.new_connection_form = None;
                self.duplicating_saved_connection_id = None;
                self.close_new_connection_select();
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
                if self.saved_connection_prompt_action.is_some() {
                    self.new_connection_form = None;
                    self.editing_saved_connection_id = None;
                    self.editing_saved_connection_connect_after_save_node_id = None;
                    self.duplicating_saved_connection_id = None;
                    self.saved_connection_prompt_action = None;
                    self.close_new_connection_select();
                }
                self.session_manager.status = None;
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
            SshConnectionIntent::Test | SshConnectionIntent::DrillDown(_) => {}
        }
    }

    pub(super) fn active_proxy_connect_result_is_current(
        &self,
        run: &NativeProxyConnectRun,
    ) -> bool {
        self.active_proxy_connect_run
            .as_ref()
            .is_some_and(|active| {
                active.plan.target_node_id == run.plan.target_node_id
                    && active.plan.current_index == run.plan.current_index
            })
    }

    pub(in crate::workspace) fn active_proxy_connect_waits_for_node(
        &self,
        node_id: &NodeId,
    ) -> bool {
        self.active_proxy_connect_run
            .as_ref()
            .and_then(|run| run.plan.steps.get(run.plan.current_index))
            .is_some_and(|step| &step.node_id == node_id)
    }

    pub(in crate::workspace) fn advance_active_proxy_connect_after_node_connected(
        &mut self,
        node_id: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.active_proxy_connect_waits_for_node(node_id) {
            return;
        }
        if let Some(run) = self.active_proxy_connect_run.as_mut() {
            run.plan.advance_after_connected_step();
        }
        self.continue_active_proxy_session_tree_connect(window, cx);
    }

    pub(in crate::workspace) fn fail_active_proxy_connect_for_node(
        &mut self,
        node_id: &NodeId,
        error: String,
        cx: &mut Context<Self>,
    ) {
        if self.active_proxy_connect_waits_for_node(node_id) {
            self.cancel_active_proxy_connect_run();
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
        let Some(run) = self.active_proxy_connect_run.as_mut() else {
            return;
        };
        if let Err(error) = run.plan.accept_current_host_key(persist, fingerprint) {
            self.report_proxy_session_tree_error(error, cx);
            return;
        }
        self.continue_active_proxy_session_tree_connect(window, cx);
    }

    pub(super) fn mark_current_proxy_connect_step_verified(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.active_proxy_connect_run.as_mut() else {
            return;
        };
        if let Err(error) = run.plan.mark_current_preflight_verified() {
            self.report_proxy_session_tree_error(error, cx);
        }
    }

    pub(super) fn apply_session_tree_step_host_key_options(
        &mut self,
        step: &NativeSessionTreeConnectStep,
    ) {
        let Some(trust_host_key) = step.trust_host_key else {
            return;
        };
        let Some(fingerprint) = step.expected_host_key_fingerprint.clone() else {
            return;
        };
        if let Some(node) = self.ssh_nodes.get_mut(&step.node_id) {
            node.config.strict_host_key_checking = true;
            node.config.trust_host_key = Some(trust_host_key);
            node.config.expected_host_key_fingerprint = Some(fingerprint);
            let origin = self
                .node_runtime_store
                .snapshot(&step.node_id)
                .map(|snapshot| snapshot.origin)
                .unwrap_or_default();
            // Tauri passes host-key acceptance as connectNode options. Native
            // stores the same one-step options on the node config immediately
            // before starting connect_tree_node.
            self.node_runtime_store.upsert_node_with_origin(
                step.node_id.clone(),
                node.config.clone(),
                origin,
            );
        }
    }

    pub(in crate::workspace) fn cancel_active_proxy_connect_run(&mut self) {
        let Some(run) = self.active_proxy_connect_run.take() else {
            return;
        };
        self.cleanup_proxy_session_tree_run(&run);
    }

    pub(super) fn cleanup_proxy_session_tree_run(&mut self, run: &NativeProxyConnectRun) {
        self.cleanup_proxy_session_tree_plan(&run.plan);
    }

    pub(super) fn cleanup_proxy_session_tree_plan(&mut self, plan: &NativeSessionTreeConnectPlan) {
        self.release_proxy_session_tree_locks(plan);
        let Some(cleanup_root) = plan.cleanup_root_node_id() else {
            return;
        };
        let nodes_to_cleanup = self.node_runtime_store.subtree_postorder(&cleanup_root);
        for node_id in &nodes_to_cleanup {
            self.cancel_connection_trace_for_node(node_id);
            self.connecting_node_locks.remove(node_id);
            self.remove_pending_ssh_terminal_opens_for_node(node_id);
            if let Some(connection_id) = self.node_router.connection_id_for_node(node_id) {
                let node_consumer = ConnectionConsumer::NodeRouter(node_id.0.clone());
                self.ssh_registry.release(&connection_id, &node_consumer);
                self.release_parent_ref_for_child_connection(node_id, &connection_id);
                if let Some(handle) = self.ssh_registry.get(&connection_id) {
                    let runtime = self.forwarding_runtime.clone();
                    runtime.spawn(async move {
                        handle.clear_physical().await;
                    });
                }
                let _ = self
                    .ssh_registry
                    .mark_state(&connection_id, ConnectionState::Disconnected);
                self.node_router.emitter().unregister(&connection_id);
                let _ = self.ssh_registry.retire_connection(&connection_id);
            }
        }

        // Tauri removes the temporary manual proxy expansion on cancel/failure.
        // Native stores that expansion in both NodeRuntimeStore and the
        // UI-facing node maps, so cleanup has to remove both owners.
        let removed_nodes = self.node_router.remove_runtime_subtree(&cleanup_root);
        for node_id in removed_nodes {
            self.ssh_nodes.remove(&node_id);
            self.expanded_ssh_nodes.remove(&node_id);
            self.saved_ssh_nodes
                .retain(|_saved_id, saved_node_id| saved_node_id != &node_id);
        }
        self.persist_session_tree_snapshot();
    }

    pub(super) fn release_proxy_session_tree_locks(&mut self, plan: &NativeSessionTreeConnectPlan) {
        for step in &plan.steps {
            self.connecting_node_locks.remove(&step.node_id);
        }
    }

    pub(super) fn report_proxy_session_tree_error(
        &mut self,
        error: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = false;
            form.error = Some(error);
        } else {
            self.session_manager.status = Some(error);
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

        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = true;
            form.error = Some(self.i18n.t("ssh.form.checking_host_key"));
        } else {
            self.session_manager.status = Some(self.i18n.t("ssh.form.checking_host_key"));
        }
        self.start_ssh_preflight(config, title, SshConnectionIntent::Test);
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
            SshConnectionIntent::Connect => {
                let mode = new_connection_form_mode(
                    self.editing_saved_connection_id.as_deref(),
                    self.duplicating_saved_connection_id.as_deref(),
                    self.saved_connection_prompt_action,
                );
                let save_after_open = if mode.stores_connection_on_connect()
                    && self
                        .new_connection_form
                        .as_ref()
                        .is_some_and(|form| form.save_connection)
                {
                    match self
                        .new_connection_form
                        .as_ref()
                        .map(|form| save_request_from_form(form, None))
                    {
                        Some(Ok(request)) => Some(request),
                        Some(Err(error)) => {
                            if let Some(form) = self.new_connection_form.as_mut() {
                                form.error = Some(error.to_string());
                            }
                            cx.notify();
                            return;
                        }
                        None => return,
                    }
                } else {
                    None
                };
                self.new_connection_form = None;
                self.duplicating_saved_connection_id = None;
                self.host_key_challenge = None;
                self.close_new_connection_select();
                if config
                    .proxy_chain
                    .as_ref()
                    .is_some_and(|chain| !chain.is_empty())
                {
                    let expansion_id = format!("manual-{}", self.next_ssh_node_id);
                    match self.expand_saved_connection_tree(&expansion_id, config, title.clone()) {
                        Ok(expansion) => {
                            if let Some(target_config) = self
                                .node_runtime_store
                                .snapshot(&expansion.target_node_id)
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
                                    save_after_open,
                                    window,
                                    cx,
                                );
                            }
                        }
                        Err(error) => {
                            self.session_manager.status = Some(error.to_string());
                        }
                    }
                    return;
                }
                let node_id = self.materialize_ssh_root_node(config.clone(), title.clone(), None);
                let post_connect_command = config.post_connect_command.clone();
                let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    node_id,
                    post_connect_command,
                    config,
                    title,
                    None,
                    None,
                    save_after_open,
                    window,
                    cx,
                );
            }
            SshConnectionIntent::ConnectSaved(id) => {
                self.host_key_challenge = None;
                if self.saved_connection_prompt_action.is_some() {
                    self.new_connection_form = None;
                    self.editing_saved_connection_id = None;
                    self.editing_saved_connection_connect_after_save_node_id = None;
                    self.duplicating_saved_connection_id = None;
                    self.saved_connection_prompt_action = None;
                    self.close_new_connection_select();
                }
                self.session_manager.status = None;
                let _ = self.open_or_create_saved_ssh_terminal_tab(id, config, title, window, cx);
            }
            SshConnectionIntent::DrillDown(parent_id) => {
                self.host_key_challenge = None;
                let child_id = match self
                    .node_router
                    .drill_down_node(parent_id.clone(), config.clone())
                {
                    Ok(child_id) => child_id,
                    Err(error) => {
                        if let Some(form) = self.new_connection_form.as_mut() {
                            form.pending = false;
                            form.error = Some(error.to_string());
                        } else {
                            self.session_manager.status = Some(error.to_string());
                        }
                        cx.notify();
                        return;
                    }
                };
                self.ssh_nodes.insert(
                    child_id.clone(),
                    crate::workspace::WorkspaceSshNode {
                        saved_connection_id: None,
                        config,
                        title,
                        terminal_ids: Vec::new(),
                        readiness: NodeReadiness::Connecting,
                    },
                );
                self.expanded_ssh_nodes.insert(parent_id);
                self.expanded_ssh_nodes.insert(child_id.clone());
                self.active_ssh_node_id = Some(child_id.clone());
                self.new_connection_form = None;
                self.drill_down_parent_node_id = None;
                self.duplicating_saved_connection_id = None;
                self.close_new_connection_select();
                self.session_manager.status = Some(self.i18n.t("ssh.drill_down.connecting"));
                self.ensure_node_connection_started(&child_id);
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
        if let Some(form) = self.new_connection_form.as_mut() {
            form.pending = true;
            form.error = Some(self.i18n.t("ssh.form.test_running"));
        } else {
            self.session_manager.status = Some(self.i18n.t("ssh.form.test_running"));
        }
        let tx = self.ssh_worker_tx.clone();
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
