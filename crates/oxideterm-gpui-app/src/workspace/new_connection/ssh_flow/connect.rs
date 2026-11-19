// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::VecDeque;

use super::*;
use crate::workspace::new_connection::BleConnectionWorkerResult;

impl WorkspaceApp {
    pub(in crate::workspace) fn ssh_worker_sender(
        &self,
        cx: &App,
    ) -> ActiveDeliverySender<SshConnectionWorkerResult> {
        self.connections.flow.read(cx).ssh_worker_sender()
    }

    pub(in crate::workspace) fn apply_connection_flow_worker_delivery(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Secret-bearing worker results stay in ConnectionFlowEntity until the
        // registry supplies a live native window for their root adapter.
        let ssh_results = self.connections.flow.update(cx, |connection_flow, _cx| {
            connection_flow.take_worker_results()
        });
        self.apply_ssh_worker_results(ssh_results, window, cx);

        let ble_results = self.connections.flow.update(cx, |connection_flow, _cx| {
            connection_flow.take_ble_worker_results()
        });
        self.apply_ble_worker_results(ble_results, window, cx);
    }

    pub(super) fn apply_ble_worker_results(
        &mut self,
        results: VecDeque<BleConnectionWorkerResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Delivery polls also run with nothing drained; skip the repaint in
        // that case. Worker results mutate standalone records (bind/mark/
        // cancel), and the sidebar only refreshes when the workspace entity
        // notifies.
        if results.is_empty() {
            return;
        }
        for result in results {
            self.handle_ble_worker_result(result, window, cx);
        }
        cx.notify();
    }

    fn apply_ssh_worker_results(
        &mut self,
        results: VecDeque<SshConnectionWorkerResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for result in results {
            let request = self.connections.flow.update(cx, |flow, cx| flow.apply_ssh_worker_result(result, &self.i18n, cx));
            if let Some(request) = request {
                self.dispatch_connections_request(request, Some(window), cx);
            }
        }
    }

    pub(in crate::workspace::new_connection) fn adopt_connected_sftp(
        &mut self,
        sftp: oxideterm_gpui_connections::views::SftpConnected,
        cx: &mut Context<Self>,
    ) {
        let mut adopt = |endpoint: oxideterm_gpui_connections::views::SftpConnectedEndpoint| {
            self.standalone_sftp_sessions.insert(endpoint.id.clone(), crate::workspace::sftp::StandaloneSftpRuntime {
                connection_id: endpoint.handle.connection_id().to_string(),
                consumer: endpoint.consumer,
                handle: endpoint.handle,
                title: sftp.title.clone(),
            });
            (endpoint.id, endpoint.path)
        };
        let (primary, primary_path) = adopt(sftp.primary);
        let secondary = sftp.secondary.map(adopt);
        if let Some(profile_id) = sftp.saved_profile_id {
            let _ = self.connection_store.mark_standalone_sftp_profile_used(&profile_id);
        }
        self.set_connection_status(None, cx);
        if let Some((secondary, secondary_path)) = secondary {
            self.open_standalone_sftp_pair_tab_surface(primary, secondary, sftp.title, primary_path, secondary_path, cx);
        } else {
            self.open_standalone_sftp_tab_surface(primary, sftp.title, primary_path, cx);
        }
    }

    pub(in crate::workspace::new_connection) fn set_connection_status(&self, message: Option<String>, cx: &mut Context<Self>) {
        self.connections.session_manager.update(cx, |manager, cx| manager.set_status(message, cx));
    }

    pub(super) fn start_proxy_session_tree_connect(
        &mut self, mut config: SshConfig, title: String, intent: SshConnectionIntent,
        save: Option<SaveConnectionRequest>, window: &mut Window, cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::proxy_launch::{CHAIN_LOCK_BUSY, ProxyExpansion};
        if self.connections.flow.read(cx).has_active_proxy_connect_run() {
            return self.report_proxy_session_tree_error(CHAIN_LOCK_BUSY.to_owned(), cx);
        }
        let request = ProxyExpansion::prepare(&mut config, title, intent, save);
        let id = request.expansion_id(self.next_ssh_node_id);
        let expansion = match self.expand_saved_connection_tree(&id, config, request.title().to_owned()) {
            Ok(expansion) => expansion,
            Err(error) => return self.report_proxy_session_tree_error(error.to_string(), cx),
        };
        let result = self.connections.flow.update(cx, |flow, cx| flow.stage_proxy_expansion(request, &expansion, cx));
        match result {
            Ok(consent) => self.arm_saved_auth_save(&expansion.target_node_id, consent),
            Err(error) => return self.report_proxy_session_tree_error(error, cx),
        }
        self.continue_active_proxy_session_tree_connect(window, cx);
    }

    pub(in crate::workspace) fn start_existing_session_tree_connect(
        &mut self, target: NodeId, title: String, intent: SshConnectionIntent,
        save: &mut Option<SaveConnectionRequest>, window: &mut Window, cx: &mut Context<Self>,
    ) -> bool {
        let runtime = self.workspace_runtime.read(cx);
        let chain_locked = runtime.has_active_connection_chain();
        let node_locks = runtime.connecting_node_locks().clone();
        let result = self.connections.flow.update(cx, |flow, cx| {
            flow.stage_existing_proxy_chain(
                &self.node_router, target, title, intent, save, chain_locked,
                |nodes| nodes.iter().any(|node| node_locks.contains(node)),
                |id| self.ssh_nodes.get(id).map(|node| NativeSessionTreeConnectEndpoint::new(node.endpoint.host.clone(), node.endpoint.port)),
                cx,
            )
        });
        match result {
            Ok(false) => false,
            Ok(true) => {
                self.continue_active_proxy_session_tree_connect(window, cx);
                true
            }
            Err(error) => {
                self.report_proxy_session_tree_error(error, cx);
                true
            }
        }
    }

    pub(in crate::workspace) fn continue_active_proxy_session_tree_connect(
        &mut self, window: &mut Window, cx: &mut Context<Self>,
    ) {
        let request = self.connections.flow.update(cx, |flow, cx| {
            flow.next_proxy_chain_request(&self.node_router, &self.connection_store, &self.i18n, cx)
        });
        if let Some(request) = request {
            self.dispatch_connections_request(request, Some(window), cx);
        }
    }

    pub(in crate::workspace::new_connection) fn finish_proxy_session_tree_connect(
        &mut self, plan: NativeSessionTreeConnectPlan,
        terminal: Option<oxideterm_gpui_connections::proxy_launch::ReadyProxyTerminal>,
        report: Option<String>, window: &mut Window, cx: &mut Context<Self>,
    ) {
        self.release_proxy_session_tree_locks(&plan, cx);
        if let Some(report) = report {
            return self.report_proxy_session_tree_error(report, cx);
        }
        let Some(terminal) = terminal else { return; };
        if let Some(options) = terminal.options && let Some(node) = self.ssh_nodes.get_mut(&terminal.node_id) {
            node.terminal_options = options.terminal;
            node.dedicated_new_terminal_connection = options.dedicated_new_terminal_connection;
            node.ssh_channel_strategy = options.ssh_channel_strategy;
        }
        if terminal.saved_id.is_some() {
            self.set_connection_status(None, cx);
        }
        let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
            terminal.node_id, terminal.config.post_connect_command.clone(), terminal.config, terminal.title,
            terminal.saved_id.clone(), terminal.saved_id, terminal.save_after_open, terminal.open_sftp, window, cx,
        );
    }

    pub(in crate::workspace) fn active_proxy_connect_waits_for_node(
        &self,
        node_id: &NodeId,
        cx: &App,
    ) -> bool {
        self.connections
            .flow
            .read(cx)
            .active_proxy_connect_waits_for_node(node_id)
    }

    pub(in crate::workspace) fn advance_active_proxy_connect_after_node_connected(
        &mut self,
        node_id: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let advanced = self.connections.flow.update(cx, |connection_flow, cx| {
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
        replacement: Option<oxideterm_ssh::HostKeyReplacement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self.connections.flow.update(cx, |connection_flow, cx| {
            connection_flow.accept_active_proxy_connect_host_key(
                persist,
                fingerprint,
                replacement,
                cx,
            )
        });
        if let Err(error) = result {
            self.report_proxy_session_tree_error(error, cx);
            return;
        }
        self.continue_active_proxy_session_tree_connect(window, cx);
    }


    pub(in crate::workspace) fn cancel_active_proxy_connect_run(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.connections.flow.update(cx, |connection_flow, cx| {
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
        let runs = self.connections.flow.update(cx, |connection_flow, _cx| {
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

    pub(in crate::workspace::new_connection) fn report_proxy_session_tree_error(
        &mut self,
        error: String,
        cx: &mut Context<Self>,
    ) {
        self.connections.report_connection_error(error, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn start_ssh_test_flow(
        &mut self,
        mut config: SshConfig,
        title: String,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::preflight::SshTestRoute;
        match self.connections.prepare_ssh_test_route(&mut config, &self.i18n, cx) {
            SshTestRoute::Direct => self.start_ssh_test(config, cx),
            SshTestRoute::HostKeyPreflight => {
                self.start_ssh_preflight(config, title, SshConnectionIntent::Test, cx);
                cx.notify();
            }
        }
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
            SshConnectionIntent::ConnectTemporary => {
                self.connections
                    .flow
                    .update(cx, |flow, cx| flow.clear_host_key_challenge(cx));
                if let Err(error) = self.connect_verified_temporary_ssh(config, title, cx) {
                    self.connections.session_manager.update(cx, |manager, cx| {
                        manager.set_status(Some(error.to_string()), cx);
                    });
                }
            }
            SshConnectionIntent::Connect(connection_options) => {
                self.open_verified_manual_ssh(config, title, connection_options, window, cx);
            }
            SshConnectionIntent::ConnectSaved {
                id,
                auth_save_target,
                open_sftp_on_connect,
            } => {
                self.connections.finish_verified_saved_form(cx);
                let _ = self.open_or_create_saved_ssh_terminal_tab(
                    id,
                    config,
                    title,
                    auth_save_target,
                    open_sftp_on_connect,
                    window,
                    cx,
                );
            }
            SshConnectionIntent::Mosh(options) => {
                let Some(executable) = self.plugin_entity.read(cx).mosh_executable() else {
                    let message =
                        self.plugin_requirement_message("com.oxideterm.terminal.mosh", "Mosh", cx);
                    self.show_required_plugin_notice("com.oxideterm.terminal.mosh", "Mosh", cx);
                    self.fail_public_mcp_mosh_open_for_intent(
                        &SshConnectionIntent::Mosh(options),
                        message.clone(),
                    );
                    self.connections.session_manager
                        .update(cx, |manager, cx| manager.set_status(Some(message), cx));
                    return;
                };
                let public_mcp_open_token = options.public_mcp_open_token.clone();
                let runtime_connection_attempt_id = options.runtime_connection_attempt_id.clone();
                if runtime_connection_attempt_id
                    .as_deref()
                    .is_some_and(|attempt_id| {
                        !self
                            .standalone_connections
                            .is_connecting_attempt(attempt_id)
                    })
                {
                    // The logical connection was cancelled while host-key work was pending.
                    return;
                }
                if public_mcp_open_token
                    .as_deref()
                    .is_some_and(|token| self.cancel_public_mcp_mosh_open_if_request_ended(token))
                {
                    if let Some(attempt_id) = runtime_connection_attempt_id.as_deref() {
                        self.standalone_connections.remove_attempt(attempt_id);
                    }
                    return;
                }
                self.connections.finish_verified_saved_form(cx);

                let launch = options.into_verified_terminal(config);
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
                    executable,
                    plugin_sessions: self.mosh_plugin_sessions.clone(),
                    title: title.clone(),
                    bootstrap: launch.bootstrap,
                    bootstrap_context,
                    prediction: launch.prediction,
                    task_runtime: self.workspace_runtime.read(cx).task_runtime(),
                };
                let Some(runtime_connection_attempt_id) = runtime_connection_attempt_id else {
                    return;
                };
                match self.create_mosh_terminal_tab_for_connection(
                    terminal_config,
                    launch.terminal,
                    title,
                    runtime_connection_attempt_id.clone(),
                    window,
                    cx,
                ) {
                    Ok(session_id) => {
                        if let Some(saved_profile_id) = launch.saved_profile_id {
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
                        self.standalone_connections
                            .mark_attempt_error(&runtime_connection_attempt_id);
                        let error = error.to_string();
                        if let Some(token) = public_mcp_open_token {
                            self.complete_public_mcp_mosh_terminal_open(
                                token,
                                Err(error.clone()),
                                cx,
                            );
                        }
                        self.connections.session_manager.update(cx, |session_manager, cx| {
                            session_manager.set_status(Some(error), cx);
                        });
                    }
                }
            }
            intent @ (SshConnectionIntent::StandaloneSftp { .. } | SshConnectionIntent::StandaloneSftpSecondary { .. }) => {
                let surface = self.connections.flow.update(cx, |flow, cx| {
                    flow.open_verified_sftp(
                        config, title, intent, |id| self.standalone_sftp_sessions.contains_key(id),
                        &self.connection_store, self.ssh_registry.clone(), self.forwarding_runtime.as_ref(),
                        &self.i18n, cx,
                    )
                });
                if let Some(surface) = surface {
                    self.open_standalone_sftp_tab_surface(surface.endpoint_id, surface.title, surface.path, cx);
                }
            }
            SshConnectionIntent::DrillDown {
                parent_id,
                saved_connection_id,
                terminal_options,
            } => {
                self.connections.flow.update(cx, |connection_flow, cx| {
                    connection_flow.clear_host_key_challenge(cx);
                });
                let child_id = match self
                    .node_router
                    .drill_down_node(parent_id.clone(), config.clone())
                {
                    Ok(child_id) => child_id,
                    Err(error) => {
                        self.report_proxy_session_tree_error(error.to_string(), cx);
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
                child_node.ssh_channel_strategy = terminal_options.ssh_channel_strategy;
                self.ssh_nodes.insert(child_id.clone(), child_node);
                if let Some(saved_connection_id) = saved_connection_id {
                    self.saved_ssh_nodes
                        .insert(saved_connection_id, child_id.clone());
                }
                self.expanded_ssh_nodes.insert(parent_id);
                self.expanded_ssh_nodes.insert(child_id.clone());
                self.active_ssh_node_id = Some(child_id.clone());
                self.connections.finish_drill_down_form(&self.i18n, cx);
                self.ensure_node_connection_started(&child_id, cx);
                self.persist_session_tree_snapshot();
            }
            SshConnectionIntent::Test => self.start_ssh_test(config, cx),
            SshConnectionIntent::TestStandaloneSftp => self.start_standalone_sftp_test(config, cx),
        }
    }

    fn open_verified_manual_ssh(
        &mut self,
        config: SshConfig,
        title: String,
        options: SshTerminalConnectionOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::preflight::VerifiedManualSshRoute;
        let route = self.connections.flow.update(cx, |flow, cx| {
            flow.prepare_verified_manual_terminal(&config, cx)
        });
        let (node_id, config) = match route {
            VerifiedManualSshRoute::Direct => (
                self.materialize_ssh_root_node(config.clone(), title.clone(), None),
                config,
            ),
            VerifiedManualSshRoute::SessionTree => {
                let expansion_id = format!("manual-{}", self.next_ssh_node_id);
                match self.expand_saved_connection_tree(&expansion_id, config, title.clone()) {
                    Ok(expansion) => {
                        self.set_opening_node_terminal_options(&expansion.target_node_id, &options);
                        let Some(snapshot) = self.node_router.node_runtime_snapshot(&expansion.target_node_id) else {
                            return;
                        };
                        (expansion.target_node_id, snapshot.config)
                    }
                    Err(error) => return self.set_connection_status(Some(error.to_string()), cx),
                }
            }
        };
        if matches!(route, VerifiedManualSshRoute::Direct) {
            self.set_opening_node_terminal_options(&node_id, &options);
        }
        let post_connect_command = config.post_connect_command.clone();
        let _ = self.queue_ssh_terminal_tab_for_node_with_mark_used(
            node_id, post_connect_command, config, title, None, None, None,
            options.open_sftp_on_connect, window, cx,
        );
    }

    fn set_opening_node_terminal_options(&mut self, node_id: &NodeId, options: &SshTerminalConnectionOptions) {
        if let Some(node) = self.ssh_nodes.get_mut(node_id) {
            node.terminal_options = options.terminal.clone();
            node.dedicated_new_terminal_connection = options.dedicated_new_terminal_connection;
            node.ssh_channel_strategy = options.ssh_channel_strategy;
        }
    }

    fn start_standalone_sftp_test(&mut self, config: SshConfig, cx: &mut Context<Self>) {
        self.connections.flow.update(cx, |flow, cx| {
            flow.start_standalone_sftp_test(
                config, &self.connection_store, self.ssh_registry.clone(),
                self.forwarding_runtime.as_ref(), &self.i18n, cx,
            );
        });
    }

    pub(in crate::workspace) fn start_ssh_test(
        &mut self,
        config: SshConfig,
        cx: &mut Context<Self>,
    ) {
        if let Some(message) = self.connections.flow.update(cx, |flow, cx| {
            flow.start_connection_test(config, &self.connection_store, &self.i18n, cx)
        }) {
            self.set_connection_status(Some(message), cx);
        }
    }
}
