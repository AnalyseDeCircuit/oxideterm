use super::nodes_reconnect_helpers::{
    event_log_severity_for_connection_status, event_log_title_for_node_readiness,
    node_readiness_became_ready, node_readiness_became_unavailable,
    reconnect_cascade_child_should_start,
};
use super::*;
use crate::workspace::forwards::reconnect_forward_rule_from_rule;

const RECONNECT_CASCADE_DELAY: Duration = Duration::from_millis(100);
const RECONNECT_AUTO_CLEANUP_DELAY_MS: u64 = 30_000;

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_workspace_runtime_event(
        &mut self,
        event: &runtime_entity::WorkspaceRuntimeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            runtime_entity::WorkspaceRuntimeEvent::EffectsReady => {
                let effects = self
                    .workspace_runtime
                    .update(cx, |runtime, cx| runtime.take_runtime_effects(cx));
                let changed = effects.into_iter().fold(false, |changed, effect| {
                    self.apply_workspace_runtime_effect(effect, window, cx) || changed
                });
                if changed {
                    self.refresh_ssh_terminal_input_locks(cx);
                    cx.notify();
                }
            }
        }
    }

    fn apply_workspace_runtime_effect(
        &mut self,
        effect: runtime_entity::WorkspaceRuntimeEffect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match effect {
            runtime_entity::WorkspaceRuntimeEffect::Reconnect(effect) => {
                self.apply_reconnect_worker_effect(effect, window, cx)
            }
            runtime_entity::WorkspaceRuntimeEffect::Node(effect) => {
                self.apply_node_runtime_effect(effect, window, cx)
            }
            runtime_entity::WorkspaceRuntimeEffect::OpenReadySshTerminals { requests } => {
                self.open_ready_ssh_terminal_requests(requests, window, cx)
            }
            runtime_entity::WorkspaceRuntimeEffect::StartReconnectRoot { node_id }
            | runtime_entity::WorkspaceRuntimeEffect::StartReconnectPipeline { node_id } => {
                self.start_grace_period_reconnect(&node_id, cx);
                true
            }
            runtime_entity::WorkspaceRuntimeEffect::ContinueConnectionChain { node_id } => {
                if self
                    .workspace_runtime
                    .read(cx)
                    .connection_chain_waits_after_node(&node_id)
                {
                    self.start_next_connection_chain_node(cx);
                    true
                } else {
                    false
                }
            }
            runtime_entity::WorkspaceRuntimeEffect::ContinueReconnectCascade => {
                self.start_next_reconnect_cascade_node(cx)
            }
            runtime_entity::WorkspaceRuntimeEffect::RetryNodeConnect {
                node_id,
                attempt,
                max_attempts,
            } => {
                self.log_reconnect_phase(
                    &node_id,
                    ReconnectPhase::SshConnect,
                    Some(format!("starting retry {attempt}/{max_attempts}")),
                );
                self.start_reconnect_cascade_after_grace_expired(&node_id, cx);
                true
            }
            runtime_entity::WorkspaceRuntimeEffect::ReconnectRecoveredBeforeRetry { node_id } => {
                self.finish_reconnect_job(&node_id, Ok(0), cx);
                true
            }
            runtime_entity::WorkspaceRuntimeEffect::ReconnectJobCleaned => true,
            runtime_entity::WorkspaceRuntimeEffect::ConnectionTrace(event) => {
                self.apply_workspace_runtime_connection_trace_event(event, cx);
                true
            }
            runtime_entity::WorkspaceRuntimeEffect::ActiveConnectionsChanged => true,
        }
    }

    fn refresh_ssh_terminal_input_locks(&mut self, cx: &mut Context<Self>) {
        let terminal_nodes = self.workspace_runtime.read(cx).ssh_terminal_nodes();
        for (session_id, node_id) in terminal_nodes {
            let locked = self.ssh_terminal_input_locked_for_node(&node_id);
            let Some(pane_id) = self
                .tab_host
                .read(cx)
                .terminal_location(session_id)
                .map(|location| location.pane_id)
            else {
                continue;
            };
            if let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() {
                pane.update(cx, |pane, cx| pane.set_input_locked(locked, cx));
            }
        }
    }

    fn ssh_terminal_input_locked_for_node(&self, node_id: &NodeId) -> bool {
        let Some(connection_id) = self.node_router.connection_id_for_node(node_id) else {
            return true;
        };
        self.ssh_registry.get(&connection_id).is_none_or(|handle| {
            matches!(
                handle.state(),
                ConnectionState::LinkDown
                    | ConnectionState::Reconnecting
                    | ConnectionState::Disconnected
                    | ConnectionState::Disconnecting
                    | ConnectionState::Error(_)
            )
        })
    }

    fn cleanup_temporary_session_tree_node(
        &mut self,
        cleanup_root: &NodeId,
        cx: &mut Context<Self>,
    ) {
        let removed_nodes = self.workspace_runtime.update(cx, |runtime, cx| {
            runtime.remove_node_runtime_subtree(cleanup_root, cx)
        });
        for node_id in removed_nodes {
            self.workspace_runtime.update(cx, |runtime, _cx| {
                runtime.remove_pending_ssh_terminal_opens_for_node(&node_id);
            });
            self.ssh_nodes.remove(&node_id);
            self.expanded_ssh_nodes.remove(&node_id);
            self.saved_ssh_nodes
                .retain(|_, saved_node_id| saved_node_id != &node_id);
        }
    }

    pub(in crate::workspace) fn remove_inactive_session_tree_node(
        &mut self,
        cleanup_root: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut nodes_to_remove = self.node_router.subtree_postorder(cleanup_root);
        if nodes_to_remove.is_empty() {
            nodes_to_remove.push(cleanup_root.clone());
        }
        for node_id in &nodes_to_remove {
            // A failed node can still own stale tabs, reconnect jobs, forwards,
            // or transfer records. Clear those owners before dropping the tree.
            self.close_embedded_sftp_for_node(node_id, cx);
            self.close_tabs_for_node(node_id, window, cx);
            let _ = self.interrupt_sftp_transfers_by_node(
                node_id,
                "Connection removed".to_string(),
                cx,
            );
        }
        self.cleanup_temporary_session_tree_node(cleanup_root, cx);
        self.persist_session_tree_snapshot();
        cx.notify();
    }

    fn apply_reconnect_worker_effect(
        &mut self,
        effect: runtime_entity::ReconnectRuntimeEffect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut changed = false;
        for result in std::iter::once(effect) {
            match result {
                runtime_entity::ReconnectRuntimeEffect::NodeConnected {
                    node_id,
                    connection_id,
                    reconnecting,
                } => {
                    let mut resume_transfers_without_forwards = false;
                    if reconnecting {
                        self.log_connection_event(
                            &node_id,
                            Some(connection_id.clone()),
                            "event_log.events.connected",
                            WorkspaceEventSeverity::Info,
                            None,
                            "connect_node",
                        );
                        self.resolve_connection_notifications_for_node(&node_id);
                        self.log_reconnect_phase(&node_id, ReconnectPhase::AwaitTerminal, None);
                        let remounted =
                            self.remount_terminal_panes_for_reconnect(&node_id, window, cx);
                        let terminal_message =
                            format!("fixed {remounted} terminal pane(s) through native remount");
                        match self
                            .workspace_runtime
                            .read(cx)
                            .complete_reconnect_terminal_remount(&node_id, terminal_message)
                        {
                            Some(runtime_entity::ReconnectPostTerminalAction::RestoreForwards) => {
                                self.log_reconnect_phase(
                                    &node_id,
                                    ReconnectPhase::RestoreForwards,
                                    None,
                                );
                            }
                            Some(runtime_entity::ReconnectPostTerminalAction::ResumeTransfers) => {
                                resume_transfers_without_forwards = true;
                            }
                            None => {}
                        }
                    }
                    let projected_readiness = self
                        .node_router
                        .node_state(&node_id)
                        .map(|snapshot| snapshot.state.readiness)
                        .unwrap_or(NodeReadiness::Disconnected);
                    if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                        node.readiness = projected_readiness;
                    }
                    self.persist_session_tree_snapshot();
                    let connection_chain_node = self
                        .workspace_runtime
                        .read(cx)
                        .connection_chain_contains(&node_id);
                    if connection_chain_node {
                        self.advance_connection_chain_after_node_connected(&node_id, cx);
                    } else {
                        self.workspace_runtime.update(cx, |runtime, _cx| {
                            runtime.unlock_connecting_node(&node_id);
                        });
                        self.schedule_next_reconnect_cascade_node(cx);
                    }
                    if self.active_proxy_connect_waits_for_node(&node_id, cx) {
                        self.advance_active_proxy_connect_after_node_connected(
                            &node_id, window, cx,
                        );
                    }
                    self.restore_forwarding_rules_for_reconnect(&node_id, cx);
                    if resume_transfers_without_forwards {
                        self.log_reconnect_phase(
                            &node_id,
                            ReconnectPhase::ResumeTransfers,
                            Some("no forward rules in snapshot".to_string()),
                        );
                        let queued = self.resume_sftp_transfers_for_reconnect(&node_id, cx);
                        if queued == 0 {
                            self.finish_reconnect_after_transfer_resume(
                                &node_id,
                                PhaseResult::Skipped,
                                "no incomplete transfers in snapshot".to_string(),
                                0,
                                cx,
                            );
                        }
                    }
                    if !connection_chain_node {
                        let children_to_start = self
                            .node_router
                            .node_metadata(&node_id)
                            .map(|snapshot| snapshot.children_ids)
                            .unwrap_or_default();
                        for child_id in children_to_start {
                            if self
                                .node_router
                                .node_state(&child_id)
                                .is_ok_and(|snapshot| {
                                    snapshot.state.readiness == NodeReadiness::Connecting
                                })
                            {
                                self.ensure_node_connection_started(&child_id, cx);
                            }
                        }
                    }
                    changed = true;
                }
                runtime_entity::ReconnectRuntimeEffect::NodeConnectFailed {
                    node_id,
                    error,
                    action,
                } => {
                    let active_reconnect_job = !matches!(
                        &action,
                        runtime_entity::ReconnectFailureAction::InitialConnect
                    );
                    let connection_chain_node = self
                        .workspace_runtime
                        .read(cx)
                        .connection_chain_contains(&node_id);
                    let connection_failure_notice = (!active_reconnect_job)
                        .then(|| self.connection_failure_notice_for_node(&node_id, &error, cx))
                        .flatten();
                    self.workspace_runtime.update(cx, |runtime, _cx| {
                        runtime.abort_connection_chain_for_node(&node_id);
                    });
                    if !connection_chain_node {
                        self.workspace_runtime.update(cx, |runtime, _cx| {
                            runtime.unlock_connecting_node(&node_id);
                        });
                    } else {
                        self.workspace_runtime.update(cx, |runtime, _cx| {
                            runtime.clear_reconnect_cascade();
                        });
                    }
                    self.fail_active_proxy_connect_for_node(&node_id, error.clone(), cx);
                    if active_reconnect_job {
                        self.log_reconnect_phase(
                            &node_id,
                            ReconnectPhase::Failed,
                            Some(error.clone()),
                        );
                        self.push_notification_entry(
                            WorkspaceNotificationKind::Connection,
                            WorkspaceNotificationSeverity::Error,
                            "Reconnect failed",
                            Some(error.clone()),
                            WorkspaceNotificationScope::Node(node_id.0.clone()),
                            Some(format!("reconnect-failed:{}", node_id.0)),
                        );
                    }
                    match action {
                        runtime_entity::ReconnectFailureAction::Retry {
                            attempt,
                            max_attempts,
                            delay,
                            job_id,
                        } => {
                            if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                                // The retry timer is idle work, not an active transport attempt.
                                // Show the failed state until the next attempt actually starts.
                                node.readiness = NodeReadiness::Error;
                            }
                            self.log_reconnect_phase(
                                &node_id,
                                ReconnectPhase::Queued,
                                Some(format!(
                                    "retry {}/{} after {:?}",
                                    attempt, max_attempts, delay
                                )),
                            );
                            let retry_node_id = node_id.clone();
                            self.workspace_runtime.update(cx, |runtime, cx| {
                                runtime.schedule_reconnect_action(
                                    runtime_entity::ReconnectScheduleAction::RetryNodeConnect {
                                        node_id: retry_node_id,
                                        job_id,
                                    },
                                    delay,
                                    cx,
                                );
                            });
                            self.persist_session_tree_snapshot();
                            changed = true;
                            continue;
                        }
                        runtime_entity::ReconnectFailureAction::FinishReconnect => {
                            self.finish_reconnect_job(&node_id, Err(error.clone()), cx);
                        }
                        runtime_entity::ReconnectFailureAction::InitialConnect => {
                            if let Some((title, description)) = connection_failure_notice {
                                self.push_notification_entry(
                                    WorkspaceNotificationKind::Connection,
                                    WorkspaceNotificationSeverity::Error,
                                    title,
                                    description,
                                    WorkspaceNotificationScope::Node(node_id.0.clone()),
                                    Some(format!("connect-failed:{}", node_id.0)),
                                );
                            }
                        }
                    }
                    let cleanup_node_id = self
                        .workspace_runtime
                        .read(cx)
                        .pending_ssh_terminal_open_cleanup_for_node(&node_id);
                    self.workspace_runtime.update(cx, |runtime, _cx| {
                        runtime.remove_pending_ssh_terminal_opens_for_node(&node_id);
                    });
                    if let Some(cleanup_node_id) = cleanup_node_id {
                        self.cleanup_temporary_session_tree_node(&cleanup_node_id, cx);
                        if !connection_chain_node {
                            self.schedule_next_reconnect_cascade_node(cx);
                        }
                        self.persist_session_tree_snapshot();
                        changed = true;
                        continue;
                    }
                    if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                        node.readiness = NodeReadiness::Error;
                    }
                    if !connection_chain_node {
                        self.schedule_next_reconnect_cascade_node(cx);
                    }
                    self.persist_session_tree_snapshot();
                    changed = true;
                }
                runtime_entity::ReconnectRuntimeEffect::GraceRecovered {
                    node_id,
                    connection_id,
                    recovered_connections,
                } => {
                    self.finish_reconnect_job(&node_id, Ok(0), cx);
                    self.push_reconnect_notice(
                        self.i18n.t("connections.reconnect.recovered"),
                        None,
                        TerminalNoticeVariant::Success,
                        cx,
                    );
                    self.resolve_connection_notifications_for_node(&node_id);
                    let recovered_node_ids = self.workspace_runtime.update(cx, |runtime, _cx| {
                        runtime.apply_grace_recovery(
                            &node_id,
                            &connection_id,
                            recovered_connections,
                        )
                    });
                    for recovered_node_id in recovered_node_ids {
                        let projected_readiness = self
                            .node_router
                            .node_state(&recovered_node_id)
                            .map(|snapshot| snapshot.state.readiness)
                            .unwrap_or(NodeReadiness::Disconnected);
                        if let Some(node) = self.ssh_nodes.get_mut(&recovered_node_id) {
                            node.readiness = projected_readiness;
                        }
                    }
                    self.restore_forwarding_session_for_node(&node_id, cx);
                    changed = true;
                }
                runtime_entity::ReconnectRuntimeEffect::GraceExpired { node_id, detail } => {
                    if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                        node.readiness = NodeReadiness::Connecting;
                    }
                    self.log_reconnect_phase(&node_id, ReconnectPhase::SshConnect, Some(detail));
                    // Tauri falls back from grace-period probing to a full
                    // reconnectCascade(root): root reconnect first, and
                    // descendants marked link-down reconnect once their parent
                    // becomes Active.
                    self.start_reconnect_cascade_after_grace_expired(&node_id, cx);
                    changed = true;
                }
                runtime_entity::ReconnectRuntimeEffect::SftpTransfersSnapshotted {
                    node_id,
                    entered_grace_period,
                } => {
                    if entered_grace_period {
                        self.log_reconnect_phase(&node_id, ReconnectPhase::GracePeriod, None);
                    }
                    changed = true;
                }
                runtime_entity::ReconnectRuntimeEffect::RemoteShellIntegrationGateFinished {
                    notice,
                } => {
                    if let Some(notice) = notice {
                        self.push_remote_shell_integration_notice(notice, cx);
                    }
                    changed = true;
                }
                runtime_entity::ReconnectRuntimeEffect::RemoteShellIntegrationMaintenanceFinished {
                    notice,
                } => {
                    self.push_remote_shell_integration_notice(notice, cx);
                    changed = true;
                }
            }
        }
        changed
    }

    pub(in crate::workspace) fn emit_node_event(&self, event: NodeStateEvent) {
        self.node_router.emitter().emit(event);
    }

    fn apply_node_runtime_effect(
        &mut self,
        event: runtime_entity::NodeRuntimeEffect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match event {
            runtime_entity::NodeRuntimeEffect::ConnectionStatusChanged {
                node_id,
                connection_id,
                status,
                state,
                reason,
                affected_children,
            } => {
                self.ensure_workspace_ssh_node_from_runtime(&node_id);
                let previous = self
                    .ssh_nodes
                    .get(&node_id)
                    .map(|node| node.readiness.clone());
                if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                    node.readiness = state.clone();
                }
                self.sync_ai_runtime_node_owner(&node_id, &state, cx);
                if node_readiness_became_ready(previous.as_ref(), &state) {
                    // Registry readiness, not shell lifetime, restores shared
                    // forwards and completes the connection trace.
                    self.restore_forwarding_session_for_node(&node_id, cx);
                    self.workspace_runtime.update(cx, |runtime, cx| {
                        runtime.finish_connection_trace_success(&node_id, cx);
                    });
                } else if node_readiness_became_unavailable(previous.as_ref(), &state) {
                    self.workspace_runtime.update(cx, |runtime, cx| {
                        runtime.finish_connection_trace_failed(&node_id, Some(reason.clone()), cx);
                    });
                }
                let event_severity = event_log_severity_for_connection_status(&status);
                let affected_children_count = affected_children.len();
                if matches!(state, NodeReadiness::Error | NodeReadiness::Disconnected) {
                    let _ = self.cascade_connection_status_to_runtime_children(
                        &node_id,
                        Some(&affected_children),
                        state.clone(),
                        reason.clone(),
                        cx,
                    );
                }
                self.push_event_log_entry(
                    event_severity,
                    WorkspaceEventCategory::Connection,
                    Some(node_id.clone()),
                    Some(connection_id),
                    match status.as_str() {
                        "link_down" => "event_log.events.link_down",
                        "disconnected" => "event_log.events.disconnected",
                        "connected" => "event_log.events.connected",
                        "reconnecting" => "event_log.events.reconnecting",
                        _ => "event_log.events.node_state_unknown",
                    },
                    (affected_children_count > 0).then_some(format!(
                        "event_log.events.affected_children:{affected_children_count}"
                    )),
                    "connection_status_changed",
                );
                if matches!(state, NodeReadiness::Error) {
                    self.push_notification_entry(
                        WorkspaceNotificationKind::Connection,
                        WorkspaceNotificationSeverity::Error,
                        "Connection lost",
                        Some(if affected_children_count > 0 {
                            format!("{reason}; affected children: {affected_children_count}")
                        } else {
                            reason
                        }),
                        WorkspaceNotificationScope::Node(node_id.0.clone()),
                        Some(format!("connection-lost:{}", node_id.0)),
                    );
                } else if matches!(state, NodeReadiness::Ready) {
                    self.resolve_connection_notifications_for_node(&node_id);
                }
                if matches!(state, NodeReadiness::Error | NodeReadiness::Disconnected) {
                    self.mark_ide_interrupted_for_node(&node_id, cx);
                    let message = if matches!(state, NodeReadiness::Disconnected) {
                        "Connection closed".to_string()
                    } else {
                        self.i18n.t("sftp.errors.connection_lost")
                    };
                    let _ = self.interrupt_sftp_transfers_by_node(&node_id, message, cx);
                    let session_id = self.forwarding_session_id_for_node(&node_id);
                    let forwarding_connection_id = self.forwarding_connection_id_for_node(&node_id);
                    let forwarding_registry = self.forwarding_service.registry().clone();
                    self.forwarding_runtime.spawn(async move {
                        if let Some(connection_id) = forwarding_connection_id {
                            forwarding_registry.stop_port_profiler(&connection_id);
                        }
                        let _ = forwarding_registry.suspend_session(&session_id).await;
                    });
                }
                if status == "link_down" {
                    self.schedule_grace_period_reconnect(&node_id, cx);
                }
                if status == "disconnected" {
                    let mut nodes_to_close = if affected_children.is_empty() {
                        vec![node_id.clone()]
                    } else {
                        // Native idle-timeout cascades may unregister child
                        // connection ids before the root disconnected event is
                        // consumed, so affected_children is a subtree signal
                        // here rather than a reliable lookup table.
                        self.node_router.subtree_postorder(&node_id)
                    };
                    if nodes_to_close.is_empty() {
                        nodes_to_close.push(node_id.clone());
                    }
                    // Tauri's connection_status_changed(disconnected) handler
                    // closes tabs by root and affected child node ids; native
                    // must do the same for node-scoped SFTP/IDE/forwards tabs,
                    // not only for terminal panes.
                    for affected_node_id in nodes_to_close {
                        self.close_tabs_for_node(&affected_node_id, window, cx);
                    }
                }
                true
            }
            runtime_entity::NodeRuntimeEffect::ConnectionStateChanged {
                node_id,
                state: event_state,
                reason,
            } => {
                let node_id = NodeId::new(node_id);
                self.ensure_workspace_ssh_node_from_runtime(&node_id);
                let state = self
                    .node_router
                    .node_state(&node_id)
                    .map(|snapshot| snapshot.state.readiness)
                    .unwrap_or(event_state);
                let previous = self
                    .ssh_nodes
                    .get(&node_id)
                    .map(|node| node.readiness.clone());
                let event_severity = match state {
                    NodeReadiness::Error => WorkspaceEventSeverity::Error,
                    NodeReadiness::Disconnected => WorkspaceEventSeverity::Warn,
                    _ => WorkspaceEventSeverity::Info,
                };
                self.push_event_log_entry(
                    event_severity,
                    WorkspaceEventCategory::Node,
                    Some(node_id.clone()),
                    self.node_router.connection_id_for_node(&node_id),
                    event_log_title_for_node_readiness(&state),
                    (!reason.is_empty()).then_some(reason.clone()),
                    "node:state",
                );
                if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                    node.readiness = state.clone();
                }
                self.sync_ai_runtime_node_owner(&node_id, &state, cx);
                if node_readiness_became_ready(previous.as_ref(), &state) {
                    self.restore_forwarding_session_for_node(&node_id, cx);
                    self.workspace_runtime.update(cx, |runtime, cx| {
                        runtime.finish_connection_trace_success(&node_id, cx);
                    });
                } else if node_readiness_became_unavailable(previous.as_ref(), &state) {
                    self.workspace_runtime.update(cx, |runtime, cx| {
                        runtime.finish_connection_trace_failed(&node_id, Some(reason.clone()), cx);
                    });
                }
                if matches!(previous, Some(NodeReadiness::Ready))
                    && matches!(state, NodeReadiness::Error | NodeReadiness::Disconnected)
                {
                    self.mark_ide_interrupted_for_node(&node_id, cx);
                    let affected_children = self.cascade_connection_status_to_runtime_children(
                        &node_id,
                        None,
                        state.clone(),
                        reason.clone(),
                        cx,
                    );
                    self.push_event_log_entry(
                        event_severity,
                        WorkspaceEventCategory::Connection,
                        Some(node_id.clone()),
                        self.node_router.connection_id_for_node(&node_id),
                        if matches!(state, NodeReadiness::Error) {
                            "event_log.events.link_down"
                        } else {
                            "event_log.events.disconnected"
                        },
                        (affected_children > 0).then_some(format!(
                            "event_log.events.affected_children:{affected_children}"
                        )),
                        "connection_status_changed",
                    );
                    if matches!(state, NodeReadiness::Error) {
                        self.push_notification_entry(
                            WorkspaceNotificationKind::Connection,
                            WorkspaceNotificationSeverity::Error,
                            "Connection lost",
                            Some(if affected_children > 0 {
                                format!("{reason}; affected children: {affected_children}")
                            } else {
                                reason.clone()
                            }),
                            WorkspaceNotificationScope::Node(node_id.0.clone()),
                            Some(format!("connection-lost:{}", node_id.0)),
                        );
                    }
                    let message = if matches!(state, NodeReadiness::Disconnected) {
                        "Connection closed".to_string()
                    } else {
                        self.i18n.t("sftp.errors.connection_lost")
                    };
                    let _ = self.interrupt_sftp_transfers_by_node(&node_id, message, cx);
                    let session_id = self.forwarding_session_id_for_node(&node_id);
                    let connection_id = self.forwarding_connection_id_for_node(&node_id);
                    let forwarding_registry = self.forwarding_service.registry().clone();
                    self.forwarding_runtime.spawn(async move {
                        if let Some(connection_id) = connection_id {
                            forwarding_registry.stop_port_profiler(&connection_id);
                        }
                        let _ = forwarding_registry.suspend_session(&session_id).await;
                    });
                    if matches!(state, NodeReadiness::Error)
                        && reason.to_ascii_lowercase().contains("link")
                    {
                        self.schedule_grace_period_reconnect(&node_id, cx);
                    }
                    if matches!(state, NodeReadiness::Disconnected) {
                        let mut nodes_to_close = self.node_router.subtree_postorder(&node_id);
                        if nodes_to_close.is_empty() {
                            nodes_to_close.push(node_id.clone());
                        }
                        // Internal node:state disconnects are the native form
                        // of the same Tauri terminal cleanup boundary.
                        for affected_node_id in nodes_to_close {
                            self.close_tabs_for_node(&affected_node_id, window, cx);
                        }
                    }
                }
                true
            }
            runtime_entity::NodeRuntimeEffect::SftpReady {
                node_id,
                ready,
                cwd,
            } => {
                let node_id = NodeId::new(node_id);
                self.apply_sftp_ready_event(&node_id, ready, cwd, cx);
                true
            }
            runtime_entity::NodeRuntimeEffect::SharedSftpSessionChanged {
                node_id,
                connection_id,
                session_generation,
                ready,
            } => {
                let node_id = NodeId::new(node_id);
                self.sync_ai_runtime_sftp_owner(
                    &node_id,
                    connection_id,
                    session_generation,
                    ready,
                    cx,
                );
                true
            }
            runtime_entity::NodeRuntimeEffect::TerminalEndpointChanged => {
                cx.notify();
                true
            }
        }
    }

    pub(super) fn ensure_workspace_ssh_node_from_runtime(&mut self, node_id: &NodeId) -> bool {
        if self.ssh_nodes.contains_key(node_id) {
            return false;
        }
        let Some(snapshot) = self.node_router.node_metadata(node_id) else {
            return false;
        };
        let title = snapshot
            .origin
            .saved_connection_id()
            .and_then(|id| self.connection_store.get(id))
            .map(|connection| connection.name.clone())
            .unwrap_or_else(|| format!("{}@{}", snapshot.username, snapshot.host));
        let ssh_channel_strategy = snapshot
            .origin
            .saved_connection_id()
            .and_then(|id| self.connection_store.get(id))
            .map(|connection| connection.options.ssh_channel_strategy)
            .unwrap_or_default();
        self.ssh_nodes.insert(
            node_id.clone(),
            WorkspaceSshNode {
                saved_connection_id: snapshot.origin.saved_connection_id().map(str::to_string),
                endpoint: WorkspaceSshNodeEndpoint {
                    host: snapshot.host,
                    port: snapshot.port,
                    username: snapshot.username,
                },
                title,
                terminal_options: ConnectionTerminalOptions::default(),
                dedicated_new_terminal_connection: false,
                ssh_channel_strategy,
                terminal_ids: Vec::new(),
                readiness: snapshot.readiness,
            },
        );
        true
    }

    /// Bridges real NodeRouter lifecycle events into AI authority. Snapshot
    /// creation only issues leases from this owner; it never registers one.
    fn sync_ai_runtime_node_owner(
        &mut self,
        node_id: &NodeId,
        state: &NodeReadiness,
        cx: &mut Context<Self>,
    ) {
        if !matches!(state, NodeReadiness::Ready) {
            self.ai_runtime_context
                .update(cx, |runtime, _cx| runtime.revoke_node_connection(node_id));
            return;
        }
        let Some(connection_id) = self.node_router.connection_id_for_node(node_id) else {
            self.ai_runtime_context
                .update(cx, |runtime, _cx| runtime.revoke_node_connection(node_id));
            return;
        };
        let Some(node) = self.ssh_nodes.get(node_id) else {
            return;
        };
        let label = node.title.clone();
        let resource_ref = node.saved_connection_id.as_ref().and_then(|connection_id| {
            oxideterm_ai::StableResourceRef::new(
                oxideterm_ai::StableResourceKind::SavedConnection,
                connection_id.clone(),
                Some(label.clone()),
            )
            .ok()
        });
        self.ai_runtime_context.update(cx, |runtime, _cx| {
            runtime.register_node_connection(node_id.clone(), connection_id, label, resource_ref);
        });
    }

    /// Shared SFTP lifecycle is separate from short-lived directory-listing
    /// readiness. Only a concrete shared channel generation grants authority.
    fn sync_ai_runtime_sftp_owner(
        &mut self,
        node_id: &NodeId,
        connection_id: String,
        session_generation: Option<u64>,
        ready: bool,
        cx: &mut Context<Self>,
    ) {
        if !ready {
            self.ai_runtime_context
                .update(cx, |runtime, _cx| runtime.revoke_sftp_session(node_id));
            return;
        }
        let Some(session_generation) = session_generation else {
            self.ai_runtime_context
                .update(cx, |runtime, _cx| runtime.revoke_sftp_session(node_id));
            return;
        };
        let Some(node) = self.ssh_nodes.get(node_id) else {
            return;
        };
        let label = node.title.clone();
        let resource_ref = node.saved_connection_id.as_ref().and_then(|connection_id| {
            oxideterm_ai::StableResourceRef::new(
                oxideterm_ai::StableResourceKind::SavedConnection,
                connection_id.clone(),
                Some(label.clone()),
            )
            .ok()
        });
        self.ai_runtime_context.update(cx, |runtime, _cx| {
            runtime.register_sftp_session(
                node_id.clone(),
                connection_id,
                session_generation,
                format!("SFTP · {label}"),
                resource_ref,
            );
        });
    }

    fn cascade_connection_status_to_runtime_children(
        &mut self,
        root_node_id: &NodeId,
        affected_connection_ids: Option<&[String]>,
        state: NodeReadiness,
        reason: String,
        cx: &mut Context<Self>,
    ) -> usize {
        let affected = self
            .workspace_runtime
            .read(cx)
            .cascade_connection_status_to_children(
                root_node_id,
                affected_connection_ids,
                state.clone(),
                reason,
            );
        for affected_node_id in &affected {
            self.ensure_workspace_ssh_node_from_runtime(affected_node_id);
            self.ai_runtime_context.update(cx, |runtime, _cx| {
                runtime.revoke_node_connection(affected_node_id)
            });
            self.mark_ide_interrupted_for_node(affected_node_id, cx);
            if let Some(node) = self.ssh_nodes.get_mut(affected_node_id) {
                node.readiness = state.clone();
            }
            let message = if matches!(state, NodeReadiness::Disconnected) {
                "Connection closed".to_string()
            } else {
                self.i18n.t("sftp.errors.connection_lost")
            };
            let _ = self.interrupt_sftp_transfers_by_node(affected_node_id, message, cx);
            let session_id = self.forwarding_session_id_for_node(affected_node_id);
            let connection_id = self.forwarding_connection_id_for_node(affected_node_id);
            let forwarding_registry = self.forwarding_service.registry().clone();
            self.forwarding_runtime.spawn(async move {
                if let Some(connection_id) = connection_id {
                    forwarding_registry.stop_port_profiler(&connection_id);
                }
                let _ = forwarding_registry.suspend_session(&session_id).await;
            });
        }
        affected.len()
    }

    fn remount_terminal_panes_for_reconnect(
        &mut self,
        node_id: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        let old_session_ids = self
            .workspace_runtime
            .read(cx)
            .reconnect_terminal_session_ids(node_id);
        let mut remounted = 0;
        for old_session_id in old_session_ids {
            let Ok(raw_old_session_id) = old_session_id.parse::<u64>() else {
                continue;
            };
            let old_session_id = TerminalSessionId(raw_old_session_id);
            let Some(location) = self.tab_host.read(cx).terminal_location(old_session_id) else {
                continue;
            };
            let tab_id = location.tab_id;
            let old_pane_id = location.pane_id;
            let allow_dedicated_connection = remounted > 0;
            let Ok((new_pane_id, new_session_id)) = self
                .create_ssh_terminal_pane_for_existing_node(
                    node_id,
                    None,
                    allow_dedicated_connection,
                    window,
                    cx,
                )
            else {
                continue;
            };

            let replaced = self.tab_host.update(cx, |tab_host, _| {
                tab_host.replace_terminal_session(
                    tab_id,
                    old_session_id,
                    old_pane_id,
                    new_pane_id,
                    new_session_id,
                )
            });
            if let Some(replaced_pane_id) = replaced {
                self.remount_public_mcp_terminal_session(old_session_id, new_session_id, cx);
                if let Some(pane) = self.remove_terminal_pane(&replaced_pane_id, cx) {
                    let _ = pane.update(cx, |pane, _cx| pane.shutdown());
                }
                self.bind_terminal_location(tab_id, new_pane_id, new_session_id, cx);
                self.unregister_ssh_terminal_session(old_session_id, cx);
                remounted += 1;
            } else {
                if let Some(pane) = self.remove_terminal_pane(&new_pane_id, cx) {
                    let _ = pane.update(cx, |pane, _cx| pane.shutdown());
                }
                self.unregister_ssh_terminal_session(new_session_id, cx);
            }
        }
        if remounted > 0 {
            // Reconnect creates a new visible Shell lifecycle for the node, so
            // a previously declined or incomplete integration is checked again.
            self.start_remote_shell_integration_terminal_gate(node_id.clone(), false, cx);
            self.focus_active_pane(window, cx);
            cx.notify();
        }
        remounted
    }

    fn resume_sftp_transfers_for_reconnect(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> usize {
        let transfers_by_node = self
            .workspace_runtime
            .read(cx)
            .reconnect_incomplete_sftp_transfers(node_id);
        let candidates = transfers_by_node.into_iter().flat_map(|entry| {
            let entry_node_id = NodeId::new(entry.node_id);
            entry
                .transfer_ids
                .into_iter()
                .map(move |transfer_id| (entry_node_id.clone(), transfer_id))
        });
        let requests = self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.begin_reconnect_transfer_resumes(node_id, candidates)
        });
        let queued = requests.len();
        for (entry_node_id, transfer_id) in requests {
            self.request_sftp_transfer_resume_for_node(entry_node_id, transfer_id, cx);
        }
        queued
    }

    pub(in crate::workspace) fn on_sftp_transfer_finished_for_reconnect(
        &mut self,
        _transfer_node_id: &NodeId,
        transfer_id: &str,
        success: bool,
        cx: &mut Context<Self>,
    ) {
        let completions = self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.finish_reconnect_transfer_resume(transfer_id, success)
        });
        for completion in completions {
            self.finish_reconnect_after_transfer_resume(
                &completion.node_id,
                PhaseResult::Ok,
                format!("resumed {} transfer(s)", completion.resumed),
                completion.resumed,
                cx,
            );
        }
    }

    fn finish_reconnect_after_transfer_resume(
        &mut self,
        node_id: &NodeId,
        transfer_result: PhaseResult,
        transfer_detail: String,
        restored_transfers: u32,
        cx: &mut Context<Self>,
    ) {
        if !self
            .workspace_runtime
            .read(cx)
            .complete_reconnect_transfer_resume(node_id, transfer_result, transfer_detail)
        {
            return;
        }
        self.log_reconnect_phase(node_id, ReconnectPhase::RestoreIde, None);
        self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.remember_ide_restore_transfer_count(node_id.clone(), restored_transfers);
        });
        match self.restore_ide_for_reconnect(node_id, cx) {
            super::ide::IdeReconnectRestoreStatus::Restored => {
                self.complete_pending_ide_reconnect_restore(
                    node_id,
                    PhaseResult::Ok,
                    "restored IDE project and open files".to_string(),
                    cx,
                );
            }
            super::ide::IdeReconnectRestoreStatus::Pending => {}
            super::ide::IdeReconnectRestoreStatus::Skipped => {
                self.complete_pending_ide_reconnect_restore(
                    node_id,
                    PhaseResult::Skipped,
                    "no IDE snapshot for node".to_string(),
                    cx,
                );
            }
        }
    }

    pub(in crate::workspace) fn complete_pending_ide_reconnect_restore(
        &mut self,
        node_id: &NodeId,
        result: PhaseResult,
        detail: String,
        cx: &mut Context<Self>,
    ) {
        match self
            .workspace_runtime
            .read(cx)
            .complete_reconnect_ide_restore(node_id, result, detail.clone())
        {
            None => {
                self.workspace_runtime.update(cx, |runtime, _cx| {
                    runtime.clear_ide_restore_transfer_count(node_id);
                });
                return;
            }
            Some(runtime_entity::ReconnectPhaseOutcome::Failed) => {
                self.workspace_runtime.update(cx, |runtime, _cx| {
                    runtime.clear_ide_restore_transfer_count(node_id);
                });
                self.finish_reconnect_job(node_id, Err(detail), cx);
                return;
            }
            Some(runtime_entity::ReconnectPhaseOutcome::Continue) => {}
        }
        self.log_reconnect_phase(node_id, ReconnectPhase::Verify, None);
        let verification_detail = self.verify_forward_rules_for_reconnect(node_id, cx);
        let (restored_forwards, restored_transfers) =
            self.workspace_runtime.update(cx, |runtime, _cx| {
                runtime.complete_reconnect_restore_counts(node_id)
            });
        self.finish_reconnect_job_with_verification(
            node_id,
            Ok(1 + restored_forwards + restored_transfers),
            Some(verification_detail),
            cx,
        );
    }

    fn schedule_grace_period_reconnect(&mut self, node_id: &NodeId, cx: &mut Context<Self>) {
        if !self.settings_store.settings().reconnect.enabled {
            return;
        }
        if self
            .workspace_runtime
            .read(cx)
            .has_active_reconnect_job(node_id)
        {
            return;
        }
        self.workspace_runtime.update(cx, |runtime, cx| {
            runtime.queue_reconnect_root(node_id.clone(), cx);
        });
    }

    fn start_grace_period_reconnect(&mut self, node_id: &NodeId, cx: &mut Context<Self>) {
        let Some(node) = self.ssh_nodes.get(node_id) else {
            return;
        };
        let node_title = node.title.clone();
        let Some(connection_id) = self.node_router.connection_id_for_node(node_id) else {
            return;
        };
        if self
            .workspace_runtime
            .read(cx)
            .has_active_reconnect_job(node_id)
        {
            return;
        }
        if self.has_active_reconnect_job_for_ancestor(node_id, cx) {
            return;
        }
        let expected_connection_id = self.node_router.connection_id_for_node(node_id);
        let pipeline_claim = self.workspace_runtime.update(cx, |runtime, cx| {
            runtime.claim_reconnect_pipeline(node_id, expected_connection_id, cx)
        });
        match pipeline_claim {
            runtime_entity::ReconnectPipelineClaim::Acquired => {}
            runtime_entity::ReconnectPipelineClaim::Requeued => return,
            runtime_entity::ReconnectPipelineClaim::Exhausted => {
                self.finish_reconnect_job(node_id, Err("Pipeline queue exhausted".to_string()), cx);
                return;
            }
        }

        let mut affected_nodes = self.node_router.subtree_postorder(node_id);
        affected_nodes.reverse();
        let terminal_sessions_by_node = affected_nodes
            .iter()
            .filter_map(|affected_node_id| {
                let terminal_ids = self
                    .workspace_runtime
                    .read(cx)
                    .ssh_terminal_session_ids_for_node(affected_node_id)
                    .into_iter()
                    .map(|session_id| session_id.0.to_string())
                    .collect::<Vec<_>>();
                (!terminal_ids.is_empty()).then_some(ReconnectNodeTerminalSnapshot {
                    node_id: affected_node_id.0.clone(),
                    old_terminal_session_ids: terminal_ids,
                })
            })
            .collect::<Vec<_>>();
        let old_terminal_session_ids = terminal_sessions_by_node
            .iter()
            .flat_map(|entry| entry.old_terminal_session_ids.iter().cloned())
            .collect::<Vec<_>>();
        let old_connections_by_node = affected_nodes
            .iter()
            .filter_map(|affected_node_id| {
                self.node_router
                    .connection_id_for_node(affected_node_id)
                    .map(|old_connection_id| ReconnectNodeConnectionSnapshot {
                        node_id: affected_node_id.0.clone(),
                        old_connection_id,
                    })
            })
            .collect::<Vec<_>>();
        let old_connection_ids = old_connections_by_node
            .iter()
            .map(|entry| entry.old_connection_id.clone())
            .collect::<Vec<_>>();
        let forward_rules = self.forward_rules_snapshot_for_nodes(&affected_nodes);
        let active_port_forward_ids = forward_rules
            .iter()
            .flat_map(|entry| entry.rules.iter().map(|rule| rule.id.clone()))
            .collect::<Vec<_>>();
        let ide_snapshot = self.ide_snapshot_for_nodes(&affected_nodes, cx);
        let snapshot = ReconnectSnapshot {
            node_id: node_id.0.clone(),
            old_terminal_session_ids,
            terminal_sessions_by_node,
            forward_rules,
            active_port_forward_ids,
            old_connections_by_node: old_connections_by_node.clone(),
            old_connection_ids: old_connection_ids.clone(),
            ide_snapshot,
            snapshot_at: Some(SystemTime::now()),
            ..ReconnectSnapshot::default()
        };
        let reconnect_job = self
            .workspace_runtime
            .read(cx)
            .start_reconnect_job(node_id, node_title, snapshot);
        self.push_reconnect_notice(
            self.i18n_with(
                "connections.reconnect.starting",
                &[("name", reconnect_job.node_name.clone())],
            ),
            None,
            TerminalNoticeVariant::Default,
            cx,
        );
        self.log_reconnect_phase(
            node_id,
            ReconnectPhase::Queued,
            Some("scheduled after link-down debounce".to_string()),
        );
        self.log_reconnect_phase(node_id, ReconnectPhase::Snapshot, None);

        let node_id = node_id.clone();
        let affected_transfer_nodes = affected_nodes
            .iter()
            .filter_map(|affected_node_id| {
                self.node_router
                    .connection_id_for_node(affected_node_id)
                    .map(|connection_id| (affected_node_id.clone(), connection_id))
            })
            .collect::<Vec<_>>();
        let grace_probe_request = runtime_entity::ReconnectGraceProbeRequest {
            node_id,
            connection_id,
            affected_transfer_nodes,
            old_connections_by_node,
            old_connection_count: old_connection_ids.len(),
            progress_store: self.sftp_progress_store.clone(),
            job_id: reconnect_job.job_id,
        };
        self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.start_reconnect_grace_probe(grace_probe_request);
        });
    }

    fn start_reconnect_cascade_after_grace_expired(
        &mut self,
        root_node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
        let mut affected_nodes = self.node_router.subtree_postorder(root_node_id);
        affected_nodes.reverse();
        if affected_nodes.is_empty() {
            affected_nodes.push(root_node_id.clone());
        }

        let cascade_node_ids = affected_nodes
            .iter()
            .filter(|affected_node_id| *affected_node_id != root_node_id)
            .filter(|affected_node_id| {
                self.node_router
                    .node_state(affected_node_id)
                    .is_ok_and(|snapshot| {
                        reconnect_cascade_child_should_start(&snapshot.state.readiness)
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.replace_reconnect_cascade(cascade_node_ids);
        });

        if let Some(node) = self.ssh_nodes.get_mut(root_node_id) {
            node.readiness = NodeReadiness::Connecting;
        }
        if !self.ensure_node_connection_started(root_node_id, cx) {
            self.workspace_runtime.update(cx, |runtime, _cx| {
                runtime.clear_reconnect_cascade();
            });
        }
    }

    fn schedule_next_reconnect_cascade_node(&self, cx: &mut Context<Self>) {
        self.workspace_runtime.update(cx, |runtime, cx| {
            runtime.schedule_next_reconnect_cascade(RECONNECT_CASCADE_DELAY, cx);
        });
    }

    fn start_next_reconnect_cascade_node(&mut self, cx: &mut Context<Self>) -> bool {
        while let Some(node_id) = self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.take_next_reconnect_cascade_node()
        }) {
            let parent_ready = self
                .node_router
                .node_metadata(&node_id)
                .and_then(|snapshot| snapshot.parent_id)
                .is_some_and(|parent_id| self.node_is_ready_for_terminal(&parent_id));
            if !parent_ready {
                continue;
            }
            if self.ensure_node_connection_started_without_ancestors_with_mode(
                &node_id,
                ConnectionTraceMode::Reconnect,
                cx,
            ) {
                return true;
            }
        }
        false
    }

    fn finish_reconnect_job(
        &mut self,
        node_id: &NodeId,
        result: Result<u32, String>,
        cx: &mut Context<Self>,
    ) {
        self.finish_reconnect_job_with_verification(node_id, result, None, cx);
    }

    fn finish_reconnect_job_with_verification(
        &mut self,
        node_id: &NodeId,
        result: Result<u32, String>,
        verification_detail: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.cancel_forward_restore(node_id);
        });
        let notice = match &result {
            Ok(restored_count) => Some((
                self.i18n_with(
                    "connections.reconnect.completed",
                    &[("count", restored_count.to_string())],
                ),
                TerminalNoticeVariant::Success,
                ReconnectPhase::Done,
                None,
            )),
            Err(error) => Some((
                self.i18n_with("connections.reconnect.failed", &[("error", error.clone())]),
                TerminalNoticeVariant::Error,
                ReconnectPhase::Failed,
                Some(error.clone()),
            )),
        };
        if let Some(job) = self.workspace_runtime.read(cx).finish_reconnect_job_state(
            node_id,
            result,
            verification_detail,
        ) {
            if let Some((title, variant, phase, detail)) = notice {
                self.log_reconnect_phase(node_id, phase, detail.clone());
                if let Some(error) = detail.clone() {
                    self.push_notification_entry(
                        WorkspaceNotificationKind::Connection,
                        WorkspaceNotificationSeverity::Error,
                        "Reconnect failed",
                        Some(error),
                        WorkspaceNotificationScope::Node(node_id.0.clone()),
                        Some(format!("reconnect-failed:{}", node_id.0)),
                    );
                } else {
                    self.resolve_connection_notifications_for_node(node_id);
                }
                self.push_reconnect_notice(title, detail, variant, cx);
            }
            self.workspace_runtime.update(cx, |runtime, _cx| {
                runtime.release_reconnect_pipeline(node_id);
            });
            let cleanup_node_id = node_id.clone();
            let started_at = job.started_at;
            self.workspace_runtime.update(cx, |runtime, cx| {
                runtime.schedule_reconnect_action(
                    runtime_entity::ReconnectScheduleAction::CleanupReconnectJob {
                        node_id: cleanup_node_id,
                        started_at,
                    },
                    Duration::from_millis(RECONNECT_AUTO_CLEANUP_DELAY_MS),
                    cx,
                );
            });
        }
    }

    fn reconnect_worker_result_is_current(
        &self,
        node_id: &NodeId,
        worker_job_id: &str,
        cx: &App,
    ) -> bool {
        self.workspace_runtime
            .read(cx)
            .reconnect_job_is_current(node_id, worker_job_id)
    }

    fn cleanup_stale_reconnect_forward_restores(&self, created_forwards: Vec<(String, String)>) {
        if created_forwards.is_empty() {
            return;
        }
        let forwarding_registry = self.forwarding_service.registry().clone();
        self.forwarding_runtime.spawn(async move {
            for (session_id, rule_id) in created_forwards {
                if let Some(manager) = forwarding_registry.get(&session_id) {
                    let _ = manager.delete_forward(&rule_id).await;
                }
            }
        });
    }

    fn release_stale_reconnect_forward_bindings(
        &mut self,
        bindings: Vec<(String, String, ConnectionConsumer)>,
    ) {
        for (session_id, connection_id, consumer) in bindings {
            self.forwarding_service
                .discard_binding(&session_id, &connection_id, &consumer);
        }
    }

    pub(in crate::workspace) fn apply_reconnect_forward_restore_completion(
        &mut self,
        node_id: NodeId,
        result: PhaseResult,
        restored: u32,
        detail: String,
        job_id: String,
        created_forwards: Vec<(String, String)>,
        bindings: Vec<(String, String, ConnectionConsumer)>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.reconnect_worker_result_is_current(&node_id, &job_id, cx) {
            self.release_stale_reconnect_forward_bindings(bindings);
            self.cleanup_stale_reconnect_forward_restores(created_forwards);
            return true;
        }
        self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.complete_forward_restore(&node_id, restored);
        });
        for binding in bindings {
            self.remember_forwarding_binding(Some(binding));
        }
        match self
            .workspace_runtime
            .read(cx)
            .complete_reconnect_forward_restore(&node_id, result, detail.clone())
        {
            None => {}
            Some(runtime_entity::ReconnectPhaseOutcome::Failed) => {
                self.finish_reconnect_job(&node_id, Err(detail), cx);
                return true;
            }
            Some(runtime_entity::ReconnectPhaseOutcome::Continue) => {
                self.log_reconnect_phase(&node_id, ReconnectPhase::ResumeTransfers, None);
                let queued = self.resume_sftp_transfers_for_reconnect(&node_id, cx);
                if queued == 0 {
                    self.finish_reconnect_after_transfer_resume(
                        &node_id,
                        PhaseResult::Skipped,
                        "no incomplete transfers in snapshot".to_string(),
                        0,
                        cx,
                    );
                }
            }
        }
        true
    }

    fn has_active_reconnect_job_for_ancestor(&self, node_id: &NodeId, cx: &App) -> bool {
        let mut cursor = self
            .node_router
            .node_metadata(node_id)
            .and_then(|snapshot| snapshot.parent_id);
        while let Some(parent_id) = cursor {
            if self
                .workspace_runtime
                .read(cx)
                .has_active_reconnect_job(&parent_id)
            {
                return true;
            }
            cursor = self
                .node_router
                .node_metadata(&parent_id)
                .and_then(|snapshot| snapshot.parent_id);
        }
        false
    }

    pub(in crate::workspace) fn ensure_node_connection_started(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        let trace_mode = if self
            .workspace_runtime
            .read(cx)
            .has_active_reconnect_job(node_id)
        {
            ConnectionTraceMode::Reconnect
        } else {
            ConnectionTraceMode::Connect
        };
        self.connect_node_with_ancestors(node_id, trace_mode, cx)
    }

    pub(in crate::workspace) fn ensure_node_connection_started_without_ancestors(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> bool {
        self.ensure_node_connection_started_without_ancestors_with_mode(
            node_id,
            ConnectionTraceMode::Connect,
            cx,
        )
    }

    fn ensure_node_connection_started_without_ancestors_with_mode(
        &mut self,
        node_id: &NodeId,
        trace_mode: ConnectionTraceMode,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.node_connection_is_active_or_connecting(node_id) {
            return true;
        }
        if !self
            .workspace_runtime
            .update(cx, |runtime, _cx| runtime.try_lock_connecting_node(node_id))
        {
            return false;
        }
        let trace_plan = self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.new_connection_trace_plan(trace_mode, vec![node_id.clone()])
        });
        if !self.ensure_single_node_connection_started_with_trace(node_id, Some(&trace_plan), cx) {
            self.workspace_runtime
                .update(cx, |runtime, _cx| runtime.unlock_connecting_node(node_id));
            return false;
        }
        true
    }

    fn node_connection_is_active_or_connecting(&self, node_id: &NodeId) -> bool {
        let Some(connection_id) = self.node_router.connection_id_for_node(node_id) else {
            return false;
        };
        self.ssh_registry.get(&connection_id).is_some_and(|handle| {
            matches!(
                handle.state(),
                ConnectionState::Connecting
                    | ConnectionState::Reconnecting
                    | ConnectionState::Active
                    | ConnectionState::Idle
            )
        })
    }

    fn connect_node_with_ancestors(
        &mut self,
        node_id: &NodeId,
        trace_mode: ConnectionTraceMode,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .workspace_runtime
            .read(cx)
            .has_active_connection_chain()
        {
            return false;
        }
        let Ok(path_node_ids) = self.node_router.path_to_node(node_id) else {
            return false;
        };
        if path_node_ids.is_empty() {
            return false;
        }

        let start_index = path_node_ids
            .iter()
            .position(|candidate| !self.connection_trace_node_is_ready(candidate));
        let Some(start_index) = start_index else {
            return true;
        };
        let nodes_to_connect = path_node_ids[start_index..].to_vec();
        let trace_plan = self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.new_connection_trace_plan(trace_mode, nodes_to_connect.clone())
        });
        if !self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.try_begin_connection_chain(trace_plan)
        }) {
            return false;
        }
        for node_id in &nodes_to_connect {
            self.workspace_runtime
                .update(cx, |runtime, _cx| runtime.reset_node_connection(node_id));
            if let Some(node) = self.ssh_nodes.get_mut(node_id) {
                node.readiness = NodeReadiness::Disconnected;
            }
        }
        self.start_next_connection_chain_node(cx)
    }

    fn start_next_connection_chain_node(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(step) = self.workspace_runtime.read(cx).connection_chain_next_step() else {
            return false;
        };
        if !self.ensure_single_node_connection_started_with_trace(
            &step.node_id,
            Some(step.trace_plan.as_ref()),
            cx,
        ) {
            self.workspace_runtime.update(cx, |runtime, _cx| {
                runtime.abort_connection_chain_for_node(&step.node_id);
            });
            return false;
        }
        true
    }

    fn advance_connection_chain_after_node_connected(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
        match self
            .workspace_runtime
            .update(cx, |runtime, _cx| runtime.advance_connection_chain(node_id))
        {
            runtime_entity::ConnectionChainAdvance::Ignored => {}
            runtime_entity::ConnectionChainAdvance::Continue => {
                let completed_node_id = node_id.clone();
                self.workspace_runtime.update(cx, |runtime, cx| {
                    runtime.schedule_reconnect_action(
                        runtime_entity::ReconnectScheduleAction::ContinueConnectionChain {
                            node_id: completed_node_id,
                        },
                        RECONNECT_CASCADE_DELAY,
                        cx,
                    );
                });
            }
            runtime_entity::ConnectionChainAdvance::Complete => {
                self.schedule_next_reconnect_cascade_node(cx);
            }
        }
    }

    pub(in crate::workspace) fn ensure_single_node_connection_started_with_trace(
        &mut self,
        node_id: &NodeId,
        trace_plan: Option<&ConnectionTracePlan>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.ssh_nodes.contains_key(node_id) {
            return false;
        }
        let projected_readiness = self
            .node_router
            .node_state(node_id)
            .map(|snapshot| snapshot.state.readiness)
            .unwrap_or(NodeReadiness::Disconnected);
        if matches!(
            projected_readiness,
            NodeReadiness::Ready | NodeReadiness::Connecting
        ) {
            return true;
        }

        let Some(parent_id) = self
            .node_router
            .node_metadata(node_id)
            .map(|snapshot| snapshot.parent_id)
        else {
            return false;
        };
        if let Some(parent_id) = parent_id.as_ref()
            && !self.node_is_ready_for_terminal(parent_id)
        {
            let error = format!("Parent node {} has no SSH connection", parent_id.0);
            self.begin_connection_trace_for_node(node_id, trace_plan, Some(parent_id), cx);
            if let Some(node) = self.ssh_nodes.get_mut(node_id) {
                node.readiness = NodeReadiness::Error;
            }
            self.workspace_runtime.update(cx, |runtime, cx| {
                runtime.record_node_transport_start_failure(node_id, error, cx);
            });
            return false;
        }
        self.begin_connection_trace_for_node(node_id, trace_plan, parent_id.as_ref(), cx);
        let managed_key_resolver =
            oxideterm_session_adapter::managed_key_resolver_from_store(&self.connection_store);
        let start_result = self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.start_node_transport(node_id, managed_key_resolver)
        });
        if let Err(error) = start_result {
            let detail = match error {
                runtime_entity::NodeTransportStartError::MissingRuntime => {
                    "Node runtime configuration is unavailable".to_string()
                }
                runtime_entity::NodeTransportStartError::Route(detail) => detail,
            };
            if let Some(node) = self.ssh_nodes.get_mut(node_id) {
                node.readiness = NodeReadiness::Error;
            }
            self.workspace_runtime.update(cx, |runtime, cx| {
                runtime.record_node_transport_start_failure(node_id, detail, cx);
            });
            return false;
        }
        if let Some(node) = self.ssh_nodes.get_mut(node_id) {
            node.readiness = NodeReadiness::Connecting;
        }
        true
    }

    fn restore_forwarding_session_for_node(&mut self, node_id: &NodeId, cx: &mut Context<Self>) {
        self.forwarding.update(cx, |forwarding, _cx| {
            forwarding.request_session_restore(node_id.clone());
        });
    }

    fn restore_forwarding_rules_for_reconnect(&mut self, node_id: &NodeId, cx: &mut Context<Self>) {
        let Some(restore_plan) = self
            .workspace_runtime
            .read(cx)
            .reconnect_forward_restore_plan(node_id)
        else {
            return;
        };
        if restore_plan.forward_rules.is_empty() {
            return;
        }

        let job_id = restore_plan.job_id;
        let snapshots = restore_plan.forward_rules;
        let restore_token = self
            .workspace_runtime
            .update(cx, |runtime, _cx| runtime.begin_forward_restore(node_id));
        let old_connection_ids_by_node = restore_plan
            .old_connections_by_node
            .iter()
            .map(|entry| (entry.node_id.clone(), entry.old_connection_id.clone()))
            .collect::<HashMap<_, _>>();
        let owner_connection_ids = snapshots
            .iter()
            .map(|entry| {
                let entry_node_id = NodeId::new(entry.node_id.clone());
                let owner = self
                    .ssh_nodes
                    .get(&entry_node_id)
                    .and_then(|node| node.saved_connection_id.clone());
                (entry.node_id.clone(), owner)
            })
            .collect::<HashMap<_, _>>();
        let restore_request = forwards::ReconnectForwardRestoreRequest {
            root_node_id: node_id.clone(),
            snapshots,
            old_connection_ids_by_node,
            owner_connection_ids,
            cancellation: restore_token,
            job_id,
        };
        self.forwarding.update(cx, |forwarding, _cx| {
            forwarding.request_reconnect_restore(restore_request);
        });
    }

    fn forward_rules_snapshot_for_nodes(
        &self,
        affected_nodes: &[NodeId],
    ) -> Vec<ReconnectForwardRuleSnapshot> {
        affected_nodes
            .iter()
            .filter_map(|affected_node_id| {
                let manager = self
                    .forwarding_service
                    .registry()
                    .get(&self.forwarding_session_id_for_node(affected_node_id))?;
                let rules = manager
                    .list_forwards()
                    .into_iter()
                    .filter(|rule| rule.status != ForwardStatus::Stopped)
                    .map(reconnect_forward_rule_from_rule)
                    .collect::<Vec<_>>();
                (!rules.is_empty()).then_some(ReconnectForwardRuleSnapshot {
                    node_id: affected_node_id.0.clone(),
                    rules,
                })
            })
            .collect()
    }

    fn verify_forward_rules_for_reconnect(&self, node_id: &NodeId, cx: &App) -> String {
        let forward_rule_snapshots = self
            .workspace_runtime
            .read(cx)
            .reconnect_forward_rule_snapshots(node_id);
        if forward_rule_snapshots.is_empty() {
            return "native node reconnect verified".to_string();
        }
        let mut drifts = Vec::new();
        for entry in forward_rule_snapshots {
            let entry_node_id = NodeId::new(entry.node_id.clone());
            let expected = entry.rules.len();
            let live = self
                .forwarding_service
                .registry()
                .get(&self.forwarding_session_id_for_node(&entry_node_id))
                .map(|manager| {
                    manager
                        .list_forwards()
                        .into_iter()
                        .filter(|rule| rule.status == ForwardStatus::Active)
                        .count()
                })
                .unwrap_or_default();
            if expected > 0 && live < expected {
                drifts.push(format!(
                    "{} forwards: live={}, snapshotExpected={}",
                    entry.node_id, live, expected
                ));
            }
        }
        if drifts.is_empty() {
            "native node reconnect verified".to_string()
        } else {
            format!(
                "native node reconnect verified with drift: {}",
                drifts.join("; ")
            )
        }
    }

    pub(in crate::workspace) fn reconnect_all_link_down_nodes_from_palette(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let link_down_connections = self
            .ssh_registry
            .list_connection_summaries()
            .into_iter()
            .filter(|summary| summary.state == ConnectionPoolEntryState::LinkDown)
            .map(|summary| summary.id)
            .collect::<HashSet<_>>();
        if link_down_connections.is_empty() {
            return;
        }

        let mut node_ids = self
            .ssh_nodes
            .keys()
            .filter(|node_id| {
                self.node_router
                    .connection_id_for_node(node_id)
                    .is_some_and(|connection_id| link_down_connections.contains(&connection_id))
            })
            .cloned()
            .collect::<Vec<_>>();
        node_ids.sort_by(|left, right| left.0.cmp(&right.0));
        node_ids.dedup();

        for node_id in node_ids {
            self.schedule_grace_period_reconnect(&node_id, cx);
        }
    }
}
