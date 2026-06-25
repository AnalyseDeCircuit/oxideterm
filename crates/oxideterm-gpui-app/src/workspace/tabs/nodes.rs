const RECONNECT_DEBOUNCE_MS: u64 = 500;
const RECONNECT_MAX_REQUEUE: u32 = 120;
const RECONNECT_AUTO_CLEANUP_DELAY_MS: u64 = 30_000;

impl WorkspaceApp {
    pub(super) fn sync_ssh_node_lifecycle(&mut self, cx: &mut Context<Self>) {
        let terminal_nodes = self.terminal_ssh_nodes.clone();
        let mut changed = false;
        let mut forwarding_to_suspend = Vec::new();
        let mut nodes_to_restore = Vec::new();
        let mut nodes_to_grace = Vec::new();
        let mut trace_ready_nodes = Vec::new();
        let mut trace_failed_nodes = Vec::new();
        for (session_id, node_id) in terminal_nodes {
            let terminal_snapshot =
                self.terminal_endpoint_sessions
                    .get(&session_id)
                    .map(|endpoint_session| {
                        // This mirrors Tauri's SessionRegistry boundary: node
                        // lifecycle is read from the terminal endpoint owner,
                        // not from the currently mounted GPUI pane. Panes may
                        // be replaced during reconnect/remount; the endpoint
                        // owner is the stable terminal-session record.
                        let terminal = endpoint_session.session.lock();
                        let readiness = match terminal.lifecycle() {
                            TerminalLifecycle::Running => NodeReadiness::Ready,
                            TerminalLifecycle::Exited(_) => NodeReadiness::Error,
                            TerminalLifecycle::Closed => NodeReadiness::Disconnected,
                        };
                        (readiness, terminal.ssh_connection_handle())
                    });
            let Some((terminal_readiness, ssh_handle)) = terminal_snapshot else {
                self.unregister_ssh_terminal_session(session_id);
                changed = true;
                continue;
            };
            if let Some(handle) = ssh_handle {
                if let Ok(event) = self
                    .node_router
                    .bind_connection(&node_id, handle.connection_id().to_string())
                {
                    self.emit_node_event(event);
                }
            }
            let registry_readiness = self
                .ssh_nodes
                .get(&node_id)
                .and_then(|node| self.readiness_for_ssh_node_connection(node));
            let readiness = registry_readiness.unwrap_or(terminal_readiness);
            let forwarding_session_id = self.forwarding_session_id_for_node(&node_id);
            let forwarding_connection_id = self.forwarding_connection_id_for_node(&node_id);
            if let Some(node) = self.ssh_nodes.get_mut(&node_id)
                && node.readiness != readiness
            {
                if matches!(node.readiness, NodeReadiness::Ready)
                    && matches!(
                        readiness,
                        NodeReadiness::Error | NodeReadiness::Disconnected
                    )
                {
                    forwarding_to_suspend.push((forwarding_session_id, forwarding_connection_id));
                    if matches!(readiness, NodeReadiness::Error) {
                        nodes_to_grace.push(node_id.clone());
                    }
                }
                if !matches!(node.readiness, NodeReadiness::Ready)
                    && matches!(readiness, NodeReadiness::Ready)
                {
                    nodes_to_restore.push(node_id.clone());
                    trace_ready_nodes.push(node_id.clone());
                }
                if !matches!(
                    node.readiness,
                    NodeReadiness::Error | NodeReadiness::Disconnected
                ) && matches!(
                    readiness,
                    NodeReadiness::Error | NodeReadiness::Disconnected
                ) {
                    trace_failed_nodes.push((
                        node_id.clone(),
                        match readiness {
                            NodeReadiness::Error => Some("terminal session exited".to_string()),
                            NodeReadiness::Disconnected => {
                                Some("terminal session closed".to_string())
                            }
                            _ => None,
                        },
                    ));
                }
                node.readiness = readiness;
                changed = true;
            }
        }
        if !forwarding_to_suspend.is_empty() {
            let forwarding_registry = self.forwarding_registry.clone();
            let forwarding_runtime = self.forwarding_runtime.clone();
            forwarding_runtime.spawn(async move {
                for (session_id, connection_id) in forwarding_to_suspend {
                    if let Some(connection_id) = connection_id {
                        forwarding_registry.stop_port_profiler(&connection_id);
                    }
                    let _ = forwarding_registry.suspend_session(&session_id).await;
                }
            });
        }
        for node_id in nodes_to_grace {
            self.schedule_grace_period_reconnect(&node_id, cx);
        }
        for node_id in nodes_to_restore {
            self.restore_forwarding_session_for_node(&node_id);
        }
        for node_id in trace_ready_nodes {
            self.finish_connection_trace_success(&node_id);
        }
        for (node_id, detail) in trace_failed_nodes {
            self.finish_connection_trace_failed(&node_id, detail);
        }
        if changed {
            self.refresh_ssh_terminal_input_locks(cx);
            cx.notify();
        }
    }

    pub(super) fn poll_node_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut events = Vec::new();
        while let Ok(event) = self.node_event_rx.try_recv() {
            events.push(event);
        }

        let mut changed = false;
        for event in events {
            changed |= self.apply_node_event(event, window, cx);
        }
        if changed {
            self.refresh_ssh_terminal_input_locks(cx);
            cx.notify();
        }
    }

    fn refresh_ssh_terminal_input_locks(&mut self, cx: &mut Context<Self>) {
        let terminal_nodes = self.terminal_ssh_nodes.clone();
        for (session_id, node_id) in terminal_nodes {
            let locked = self.ssh_terminal_input_locked_for_node(&node_id);
            let Some(pane_id) = self.tabs.iter().find_map(|tab| {
                tab.root_pane
                    .as_ref()
                    .and_then(|root| root.pane_id_for_session(session_id))
            }) else {
                continue;
            };
            if let Some(pane) = self.panes.get(&pane_id) {
                pane.update(cx, |pane, cx| pane.set_input_locked(locked, cx));
            }
        }
    }

    fn ssh_terminal_input_locked_for_node(&self, node_id: &NodeId) -> bool {
        let Some(connection_id) = self.node_router.connection_id_for_node(node_id) else {
            return true;
        };
        self.ssh_registry
            .get(&connection_id)
            .is_none_or(|handle| {
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

    fn cleanup_temporary_session_tree_node(&mut self, cleanup_root: &NodeId) {
        let mut nodes_to_cleanup = self.node_runtime_store.subtree_postorder(cleanup_root);
        if nodes_to_cleanup.is_empty() {
            nodes_to_cleanup.push(cleanup_root.clone());
        }
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

        // Tauri cleanupNodeId removes the temporary root created for saved
        // direct connect failures. Native stores that root in both the runtime
        // tree and GPUI mirrors, so all owners must be cleared together.
        let removed_nodes = self.node_router.remove_runtime_subtree(cleanup_root);
        for node_id in removed_nodes {
            self.ssh_nodes.remove(&node_id);
            self.expanded_ssh_nodes.remove(&node_id);
            self.saved_ssh_nodes
                .retain(|_, saved_node_id| saved_node_id != &node_id);
        }
    }

    pub(super) fn poll_reconnect_worker_results(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut results = Vec::new();
        while let Ok(result) = self.reconnect_worker_rx.try_recv() {
            results.push(result);
        }

        let mut changed = false;
        for result in results {
            match result {
                ReconnectWorkerResult::NodeConnected {
                    node_id,
                    connection_id,
                    job_id,
                } => {
                    if !self.reconnect_worker_result_is_current(&node_id, job_id.as_deref()) {
                        self.drop_stale_node_connection(&node_id, &connection_id);
                        changed = true;
                        continue;
                    }
                    self.finish_connection_trace_success(&node_id);
                    if self
                        .reconnect_orchestrator
                        .job(&node_id.0)
                        .is_some_and(|job| job.ended_at.is_none())
                    {
                        self.log_connection_event(
                            &node_id,
                            Some(connection_id.clone()),
                            "event_log.events.connected",
                            WorkspaceEventSeverity::Info,
                            None,
                            "connect_node",
                        );
                        self.resolve_connection_notifications_for_node(&node_id);
                        let _ = self.reconnect_orchestrator.complete_phase(
                            &node_id.0,
                            PhaseResult::Ok,
                            Some(format!("reconnected as {connection_id}")),
                        );
                        let _ = self
                            .reconnect_orchestrator
                            .advance(&node_id.0, ReconnectPhase::AwaitTerminal);
                        self.log_reconnect_phase(&node_id, ReconnectPhase::AwaitTerminal, None);
                        let remounted =
                            self.remount_terminal_panes_for_reconnect(&node_id, window, cx);
                        let terminal_message =
                            format!("fixed {remounted} terminal pane(s) through native remount");
                        let _ = self.reconnect_orchestrator.complete_phase(
                            &node_id.0,
                            PhaseResult::Ok,
                            Some(terminal_message),
                        );
                        let _ = self
                            .reconnect_orchestrator
                            .advance(&node_id.0, ReconnectPhase::RestoreForwards);
                        self.log_reconnect_phase(&node_id, ReconnectPhase::RestoreForwards, None);
                    }
                    if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                        node.readiness = NodeReadiness::Ready;
                    }
                    if let Ok(event) = self.node_router.bind_connection(&node_id, connection_id) {
                        self.emit_node_event(event);
                    }
                    self.persist_session_tree_snapshot();
                    let connection_chain_node = self
                        .active_connection_chain
                        .as_ref()
                        .is_some_and(|run| run.node_ids.contains(&node_id));
                    if connection_chain_node {
                        self.advance_connection_chain_after_node_connected(&node_id);
                    } else {
                        self.connecting_node_locks.remove(&node_id);
                        self.schedule_next_reconnect_cascade_node();
                    }
                    if self.active_proxy_connect_waits_for_node(&node_id) {
                        self.advance_active_proxy_connect_after_node_connected(&node_id, window, cx);
                    }
                    let _ = self.drain_ready_pending_ssh_terminal_opens(window, cx);
                    self.restore_forwarding_rules_for_reconnect(&node_id);
                    if self
                        .reconnect_orchestrator
                        .job(&node_id.0)
                        .is_some_and(|job| job.ended_at.is_none())
                    {
                        let has_forward_snapshot = self
                            .reconnect_orchestrator
                            .job(&node_id.0)
                            .is_some_and(|job| !job.snapshot.forward_rules.is_empty());
                        if !has_forward_snapshot {
                            let _ = self.reconnect_orchestrator.complete_phase(
                                &node_id.0,
                                PhaseResult::Skipped,
                                Some("no forward rules in snapshot".to_string()),
                            );
                            let _ = self
                                .reconnect_orchestrator
                                .advance(&node_id.0, ReconnectPhase::ResumeTransfers);
                            self.log_reconnect_phase(
                                &node_id,
                                ReconnectPhase::ResumeTransfers,
                                Some("no forward rules in snapshot".to_string()),
                            );
                            let queued = self.resume_sftp_transfers_for_reconnect(&node_id);
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
                    if !connection_chain_node {
                        let children_to_start = self
                            .node_runtime_store
                            .snapshot(&node_id)
                            .map(|snapshot| snapshot.children_ids)
                            .unwrap_or_default();
                        for child_id in children_to_start {
                            if self
                                .ssh_nodes
                                .get(&child_id)
                                .is_some_and(|child| child.readiness == NodeReadiness::Connecting)
                            {
                                self.ensure_node_connection_started(&child_id);
                            }
                        }
                    }
                    changed = true;
                }
                ReconnectWorkerResult::NodeConnectFailed {
                    node_id,
                    error,
                    job_id,
                } => {
                    if !self.reconnect_worker_result_is_current(&node_id, job_id.as_deref()) {
                        continue;
                    }
                    let active_reconnect_job = self
                        .reconnect_orchestrator
                        .job(&node_id.0)
                        .is_some_and(|job| job.ended_at.is_none());
                    let connection_chain_node = self
                        .active_connection_chain
                        .as_ref()
                        .is_some_and(|run| run.node_ids.contains(&node_id));
                    let connection_failure_notice = (!active_reconnect_job)
                        .then(|| self.connection_failure_notice_for_node(&node_id, &error))
                        .flatten();
                    self.abort_connection_chain_for_node(&node_id);
                    if !connection_chain_node {
                        self.connecting_node_locks.remove(&node_id);
                    } else {
                        self.pending_reconnect_cascade_nodes.clear();
                    }
                    self.finish_connection_trace_failed(&node_id, Some(error.clone()));
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
                        let _ = self.reconnect_orchestrator.complete_phase(
                            &node_id.0,
                            PhaseResult::Failed,
                            Some(error.clone()),
                        );
                        if !reconnect_error_is_non_retryable(&error)
                            && let Some(retry) =
                                self.reconnect_orchestrator.schedule_retry(&node_id.0)
                        {
                            self.log_reconnect_phase(
                                &node_id,
                                ReconnectPhase::Queued,
                                Some(format!(
                                    "retry {}/{} after {:?}",
                                    retry.attempt, retry.max_attempts, retry.delay
                                )),
                            );
                            let tx = self.reconnect_worker_tx.clone();
                            let retry_node_id = node_id.clone();
                            let retry_job_id = job_id.clone().unwrap_or_else(|| {
                                self.reconnect_orchestrator
                                    .job(&node_id.0)
                                    .map(|job| job.job_id)
                                    .unwrap_or_default()
                            });
                            self.forwarding_runtime.spawn(async move {
                                tokio::time::sleep(retry.delay).await;
                                let _ = tx.send(ReconnectWorkerResult::RetryNodeConnect {
                                    node_id: retry_node_id,
                                    job_id: retry_job_id,
                                });
                            });
                            self.persist_session_tree_snapshot();
                            changed = true;
                            continue;
                        } else {
                            self.finish_reconnect_job(&node_id, Err(error.clone()));
                        }
                    } else if let Some((title, description)) = connection_failure_notice {
                        self.push_reconnect_notice(
                            title.clone(),
                            description.clone(),
                            TerminalNoticeVariant::Error,
                        );
                        self.push_notification_entry(
                            WorkspaceNotificationKind::Connection,
                            WorkspaceNotificationSeverity::Error,
                            title,
                            description,
                            WorkspaceNotificationScope::Node(node_id.0.clone()),
                            Some(format!("connect-failed:{}", node_id.0)),
                        );
                    }
                    let cleanup_node_id = self.pending_ssh_terminal_open_cleanup_for_node(&node_id);
                    self.remove_pending_ssh_terminal_opens_for_node(&node_id);
                    if let Some(cleanup_node_id) = cleanup_node_id {
                        self.cleanup_temporary_session_tree_node(&cleanup_node_id);
                        if !connection_chain_node {
                            self.schedule_next_reconnect_cascade_node();
                        }
                        self.persist_session_tree_snapshot();
                        changed = true;
                        continue;
                    }
                    if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                        node.readiness = NodeReadiness::Error;
                    }
                    let event = NodeStateEvent::ConnectionStateChanged {
                        node_id: node_id.0.clone(),
                        generation: self.node_router.emitter().sequencer().next(&node_id),
                        state: NodeReadiness::Error,
                        reason: error,
                    };
                    self.emit_node_event(event);
                    if !connection_chain_node {
                        self.schedule_next_reconnect_cascade_node();
                    }
                    self.persist_session_tree_snapshot();
                    changed = true;
                }
                ReconnectWorkerResult::ContinueConnectionChain { node_id } => {
                    if self.connection_chain_waits_after_node(&node_id) {
                        self.start_next_connection_chain_node();
                        changed = true;
                    }
                }
                ReconnectWorkerResult::ContinueReconnectCascade => {
                    if self.start_next_reconnect_cascade_node() {
                        changed = true;
                    }
                }
                ReconnectWorkerResult::FlushPendingReconnect { generation } => {
                    self.flush_pending_reconnects(generation, cx);
                    changed = true;
                }
                ReconnectWorkerResult::StartReconnectPipeline {
                    node_id,
                    expected_connection_id,
                } => {
                    if expected_connection_id.as_ref().is_some_and(|expected| {
                        self.node_router.connection_id_for_node(&node_id).as_ref() != Some(expected)
                    }) {
                        continue;
                    }
                    self.start_grace_period_reconnect(&node_id, cx);
                    changed = true;
                }
                ReconnectWorkerResult::CleanupReconnectJob {
                    node_id,
                    started_at,
                } => {
                    if self
                        .reconnect_orchestrator
                        .cleanup_terminal_job(&node_id.0, started_at)
                    {
                        changed = true;
                    }
                }
                ReconnectWorkerResult::RetryNodeConnect { node_id, job_id } => {
                    if !self.reconnect_worker_result_is_current(&node_id, Some(&job_id)) {
                        continue;
                    }
                    if let Some(job) = self.reconnect_orchestrator.job(&node_id.0)
                        && job.ended_at.is_none()
                    {
                        if !self.node_still_needs_reconnect(&node_id) {
                            let _ = self.reconnect_orchestrator.complete_phase(
                                &node_id.0,
                                PhaseResult::Ok,
                                Some("node recovered before retry".to_string()),
                            );
                            self.finish_reconnect_job(&node_id, Ok(0));
                            changed = true;
                            continue;
                        }
                        let _ = self.reconnect_orchestrator.complete_phase(
                            &node_id.0,
                            PhaseResult::Ok,
                            Some(format!(
                                "starting retry {}/{}",
                                job.attempt, job.max_attempts
                            )),
                        );
                        let _ = self
                            .reconnect_orchestrator
                            .advance(&node_id.0, ReconnectPhase::SshConnect);
                        self.log_reconnect_phase(
                            &node_id,
                            ReconnectPhase::SshConnect,
                            Some(format!(
                                "starting retry {}/{}",
                                job.attempt, job.max_attempts
                            )),
                        );
                        let _ = self.reconnect_orchestrator.begin_ssh_attempt(&node_id.0);
                        self.start_reconnect_cascade_after_grace_expired(&node_id);
                        changed = true;
                    }
                }
                ReconnectWorkerResult::GraceRecovered {
                    node_id,
                    connection_id,
                    recovered_connections,
                    job_id,
                } => {
                    if !self.reconnect_worker_result_is_current(&node_id, Some(&job_id)) {
                        continue;
                    }
                    let _ = self.reconnect_orchestrator.complete_phase(
                        &node_id.0,
                        PhaseResult::Ok,
                        Some(format!(
                            "connection {connection_id} recovered during grace period"
                        )),
                    );
                    self.finish_reconnect_job(&node_id, Ok(0));
                    self.push_reconnect_notice(
                        self.i18n.t("connections.reconnect.recovered"),
                        None,
                        TerminalNoticeVariant::Success,
                    );
                    self.resolve_connection_notifications_for_node(&node_id);
                    if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                        node.readiness = NodeReadiness::Ready;
                    }
                    let _ = self.node_router.sync_node_readiness_event(
                        &node_id,
                        NodeReadiness::Ready,
                        "grace recovered",
                    );
                    // Tauri's phaseGracePeriod calls clearLinkDown(root) and
                    // then clearLinkDown(each descendant). The descendant
                    // probes only decide whether their backend connection can
                    // also be marked Active; UI link-down is cleared for the
                    // whole affected subtree once the root connection survives.
                    for affected_node_id in self.node_runtime_store.subtree_postorder(&node_id) {
                        if let Some(node) = self.ssh_nodes.get_mut(&affected_node_id) {
                            node.readiness = NodeReadiness::Ready;
                        }
                        let _ = self.node_router.sync_node_readiness_event(
                            &affected_node_id,
                            NodeReadiness::Ready,
                            "grace recovered",
                        );
                    }
                    let _ = self
                        .ssh_registry
                        .mark_state_without_event(&connection_id, ConnectionState::Active);
                    for (recovered_node_id, recovered_connection_id) in recovered_connections {
                        let _ = self.ssh_registry.mark_state_without_event(
                            &recovered_connection_id,
                            ConnectionState::Active,
                        );
                        if let Some(node) = self.ssh_nodes.get_mut(&recovered_node_id) {
                            node.readiness = NodeReadiness::Ready;
                        }
                    }
                    self.restore_forwarding_session_for_node(&node_id);
                    changed = true;
                }
                ReconnectWorkerResult::GraceExpired {
                    node_id,
                    connection_id,
                    detail,
                    job_id,
                } => {
                    if !self.reconnect_worker_result_is_current(&node_id, Some(&job_id)) {
                        continue;
                    }
                    let _ = self.reconnect_orchestrator.complete_phase(
                        &node_id.0,
                        PhaseResult::Failed,
                        Some(detail.clone()),
                    );
                    if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                        node.readiness = NodeReadiness::Connecting;
                    }
                    if let Some(info) = self
                        .ssh_registry
                        .mark_state(&connection_id, ConnectionState::LinkDown)
                        && let Some(event) = self
                            .node_router
                            .sync_connection_state_by_connection_id(&info, "grace expired")
                    {
                        self.emit_node_event(event);
                    }
                    let _ = self
                        .reconnect_orchestrator
                        .advance(&node_id.0, ReconnectPhase::SshConnect);
                    self.log_reconnect_phase(&node_id, ReconnectPhase::SshConnect, Some(detail));
                    let _ = self.reconnect_orchestrator.begin_ssh_attempt(&node_id.0);
                    // Tauri falls back from grace-period probing to a full
                    // reconnectCascade(root): root reconnect first, and
                    // descendants marked link-down reconnect once their parent
                    // becomes Active.
                    self.start_reconnect_cascade_after_grace_expired(&node_id);
                    changed = true;
                }
                ReconnectWorkerResult::SftpTransfersSnapshotted {
                    node_id,
                    transfers_by_node,
                    detail,
                    job_id,
                } => {
                    if !self.reconnect_worker_result_is_current(&node_id, Some(&job_id)) {
                        continue;
                    }
                    let _ = self
                        .reconnect_orchestrator
                        .update_snapshot(&node_id.0, |snapshot| {
                            snapshot.inflight_sftp_transfer_ids = transfers_by_node
                                .iter()
                                .flat_map(|entry| entry.transfer_ids.iter().cloned())
                                .collect();
                            snapshot.incomplete_sftp_transfers_by_node = transfers_by_node;
                        });
                    if self
                        .reconnect_orchestrator
                        .job(&node_id.0)
                        .is_some_and(|job| job.ended_at.is_none())
                    {
                        let _ = self.reconnect_orchestrator.complete_phase(
                            &node_id.0,
                            PhaseResult::Ok,
                            Some(detail),
                        );
                        let _ = self
                            .reconnect_orchestrator
                            .advance(&node_id.0, ReconnectPhase::GracePeriod);
                        self.log_reconnect_phase(&node_id, ReconnectPhase::GracePeriod, None);
                    }
                    changed = true;
                }
                ReconnectWorkerResult::ForwardRulesRestored {
                    node_id,
                    result,
                    restored,
                    detail,
                    job_id,
                    created_forwards,
                    bindings,
                } => {
                    if !self.reconnect_worker_result_is_current(&node_id, Some(&job_id)) {
                        self.release_stale_reconnect_forward_bindings(bindings);
                        self.cleanup_stale_reconnect_forward_restores(created_forwards);
                        changed = true;
                        continue;
                    }
                    self.reconnect_forward_restore_tokens.remove(&node_id);
                    for binding in bindings {
                        self.remember_forwarding_binding(Some(binding));
                    }
                    self.reconnect_forward_restore_totals
                        .insert(node_id.clone(), restored);
                    if self
                        .reconnect_orchestrator
                        .job(&node_id.0)
                        .is_some_and(|job| job.ended_at.is_none())
                    {
                        let _ = self.reconnect_orchestrator.complete_phase(
                            &node_id.0,
                            result,
                            Some(detail.clone()),
                        );
                        if result == PhaseResult::Failed {
                            self.finish_reconnect_job(&node_id, Err(detail));
                            changed = true;
                            continue;
                        }
                        let _ = self
                            .reconnect_orchestrator
                            .advance(&node_id.0, ReconnectPhase::ResumeTransfers);
                        self.log_reconnect_phase(&node_id, ReconnectPhase::ResumeTransfers, None);
                        let queued = self.resume_sftp_transfers_for_reconnect(&node_id);
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
                    changed = true;
                }
                ReconnectWorkerResult::ActiveConnectionsProbed {
                    changed: probed_changed,
                } => {
                    self.ssh_active_probe_in_flight = false;
                    changed = probed_changed > 0;
                }
            }
        }
        if changed {
            self.refresh_ssh_terminal_input_locks(cx);
            cx.notify();
        }
    }

    pub(super) fn maybe_probe_active_ssh_connections(&mut self, cx: &mut Context<Self>) {
        if self.ssh_active_probe_in_flight {
            return;
        }
        let timing = self.reconnect_orchestrator.timing();
        if self
            .last_ssh_active_probe_at
            .is_some_and(|last| last.elapsed() < timing.ssh_keepalive_interval)
        {
            return;
        }
        let stats = self.ssh_registry.stats();
        if stats.active == 0 && stats.idle == 0 {
            self.last_ssh_active_probe_at = Some(Instant::now());
            return;
        }
        self.last_ssh_active_probe_at = Some(Instant::now());
        self.ssh_active_probe_in_flight = true;
        let registry = self.ssh_registry.clone();
        let timeout = timing.proactive_keepalive_timeout;
        let tx = self.reconnect_worker_tx.clone();
        self.forwarding_runtime.spawn(async move {
            let changed = registry.probe_active_connections(timeout).await.len();
            let _ = tx.send(ReconnectWorkerResult::ActiveConnectionsProbed { changed });
        });
        cx.notify();
    }

    pub(super) fn emit_node_event(&self, event: NodeStateEvent) {
        let _ = self.node_event_tx.send(event);
    }

    fn apply_node_event(
        &mut self,
        event: NodeStateEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match event {
            NodeStateEvent::ConnectionStatusChanged {
                connection_id,
                status,
                affected_children,
                ..
            } => {
                let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
                    return false;
                };
                let Some(state) = readiness_for_connection_status(&status) else {
                    return false;
                };
                let reason = reason_for_connection_status(&status);
                self.ensure_workspace_ssh_node_from_runtime(&node_id);
                let _ = self.node_router.sync_node_readiness_event(
                    &node_id,
                    state.clone(),
                    reason.clone(),
                );
                if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                    node.readiness = state.clone();
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
                    Some(connection_id.clone()),
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
                            reason.clone()
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
                    let _ = self.interrupt_sftp_transfers_by_node(&node_id, message);
                    let session_id = self.forwarding_session_id_for_node(&node_id);
                    let forwarding_connection_id = self.forwarding_connection_id_for_node(&node_id);
                    let forwarding_registry = self.forwarding_registry.clone();
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
                        self.node_runtime_store.subtree_postorder(&node_id)
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
            NodeStateEvent::ConnectionStateChanged {
                node_id,
                generation,
                state,
                reason,
            } => {
                let node_id = NodeId::new(node_id);
                if self.is_stale_node_event(&node_id, generation) {
                    return false;
                }
                self.ensure_workspace_ssh_node_from_runtime(&node_id);
                let _ = self.node_router.sync_node_readiness_event(
                    &node_id,
                    state.clone(),
                    reason.clone(),
                );
                self.node_event_generations
                    .insert(node_id.clone(), generation);
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
                    let _ = self.interrupt_sftp_transfers_by_node(&node_id, message);
                    let session_id = self.forwarding_session_id_for_node(&node_id);
                    let connection_id = self.forwarding_connection_id_for_node(&node_id);
                    let forwarding_registry = self.forwarding_registry.clone();
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
                        let mut nodes_to_close = self.node_runtime_store.subtree_postorder(&node_id);
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
            NodeStateEvent::SftpReady {
                node_id,
                generation,
                ready,
                cwd,
            } => {
                let node_id = NodeId::new(node_id);
                if self.is_stale_node_event(&node_id, generation) {
                    return false;
                }
                self.node_event_generations
                    .insert(node_id.clone(), generation);
                self.apply_sftp_ready_event(&node_id, ready, cwd);
                true
            }
            NodeStateEvent::TerminalEndpointChanged { .. } => {
                cx.notify();
                true
            }
        }
    }

    fn ensure_workspace_ssh_node_from_runtime(&mut self, node_id: &NodeId) -> bool {
        if self.ssh_nodes.contains_key(node_id) {
            return false;
        }
        let Some(snapshot) = self.node_runtime_store.snapshot(node_id) else {
            return false;
        };
        let title = snapshot
            .origin
            .saved_connection_id()
            .and_then(|id| self.connection_store.get(id))
            .map(|connection| connection.name.clone())
            .unwrap_or_else(|| format!("{}@{}", snapshot.config.username, snapshot.config.host));
        self.ssh_nodes.insert(
            node_id.clone(),
            WorkspaceSshNode {
                saved_connection_id: snapshot.origin.saved_connection_id().map(str::to_string),
                config: snapshot.config,
                title,
                terminal_ids: Vec::new(),
                readiness: snapshot.state.readiness,
            },
        );
        true
    }

    fn cascade_connection_status_to_runtime_children(
        &mut self,
        root_node_id: &NodeId,
        affected_connection_ids: Option<&[String]>,
        state: NodeReadiness,
        reason: String,
        cx: &mut Context<Self>,
    ) -> usize {
        let connection_state = match state {
            NodeReadiness::Error => ConnectionState::LinkDown,
            NodeReadiness::Disconnected => ConnectionState::Disconnected,
            NodeReadiness::Ready | NodeReadiness::Connecting => return 0,
        };
        let affected = self
            .node_router
            .connection_id_for_node(root_node_id)
            .map(|root_connection_id| {
                affected_connection_ids
                    .map(|ids| ids.to_vec())
                    .unwrap_or_else(|| {
                        self.ssh_registry
                            .descendant_connection_infos(&root_connection_id)
                            .into_iter()
                            .map(|info| info.connection_id)
                            .collect::<Vec<_>>()
                    })
                    .into_iter()
                    .filter_map(|connection_id| {
                        self.node_router.node_id_for_connection(&connection_id)
                    })
                    .filter(|node_id| node_id != root_node_id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for affected_node_id in &affected {
            self.ensure_workspace_ssh_node_from_runtime(affected_node_id);
            self.mark_ide_interrupted_for_node(affected_node_id, cx);
            if let Some(node) = self.ssh_nodes.get_mut(affected_node_id) {
                node.readiness = state.clone();
            }
            let _ = self.node_router.sync_node_readiness_event(
                affected_node_id,
                state.clone(),
                reason.clone(),
            );
            if let Some(connection_id) = self.node_router.connection_id_for_node(affected_node_id) {
                let _ = self
                    .ssh_registry
                    .mark_state_without_event(&connection_id, connection_state.clone());
            }
            let message = if matches!(state, NodeReadiness::Disconnected) {
                "Connection closed".to_string()
            } else {
                self.i18n.t("sftp.errors.connection_lost")
            };
            let _ = self.interrupt_sftp_transfers_by_node(affected_node_id, message);
            let session_id = self.forwarding_session_id_for_node(affected_node_id);
            let connection_id = self.forwarding_connection_id_for_node(affected_node_id);
            let forwarding_registry = self.forwarding_registry.clone();
            self.forwarding_runtime.spawn(async move {
                if let Some(connection_id) = connection_id {
                    forwarding_registry.stop_port_profiler(&connection_id);
                }
                let _ = forwarding_registry.suspend_session(&session_id).await;
            });
        }
        affected.len()
    }

    fn is_stale_node_event(&self, node_id: &NodeId, generation: u64) -> bool {
        self.node_event_generations
            .get(node_id)
            .is_some_and(|seen| generation <= *seen)
    }

    fn remount_terminal_panes_for_reconnect(
        &mut self,
        node_id: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        let old_session_ids = self
            .reconnect_orchestrator
            .job(&node_id.0)
            .and_then(|job| {
                job.snapshot
                    .terminal_sessions_by_node
                    .iter()
                    .find(|entry| entry.node_id == node_id.0)
                    .map(|entry| entry.old_terminal_session_ids.clone())
                    .or_else(|| Some(job.snapshot.old_terminal_session_ids))
            })
            .unwrap_or_default();
        let mut remounted = 0;
        for old_session_id in old_session_ids {
            let Ok(raw_old_session_id) = old_session_id.parse::<u64>() else {
                continue;
            };
            let old_session_id = TerminalSessionId(raw_old_session_id);
            let Some((tab_id, old_pane_id)) = self.tabs.iter().find_map(|tab| {
                tab.root_pane
                    .as_ref()
                    .and_then(|root| root.pane_id_for_session(old_session_id))
                    .map(|pane_id| (tab.id, pane_id))
            }) else {
                continue;
            };
            let Ok((new_pane_id, new_session_id)) =
                self.create_ssh_terminal_pane_for_existing_node(node_id, window, cx)
            else {
                continue;
            };

            let replaced = self
                .tabs
                .iter_mut()
                .find(|tab| tab.id == tab_id)
                .and_then(|tab| {
                    let old = tab.root_pane.as_mut()?.replace_session(
                        old_session_id,
                        new_pane_id,
                        new_session_id,
                    )?;
                    if tab.active_pane_id == Some(old_pane_id) {
                        tab.active_pane_id = Some(new_pane_id);
                    }
                    Some(old)
                });
            if let Some(replaced_pane_id) = replaced {
                if let Some(pane) = self.remove_terminal_pane(&replaced_pane_id) {
                    let _ = pane.update(cx, |pane, _cx| pane.shutdown());
                }
                self.unregister_ssh_terminal_session(old_session_id);
                remounted += 1;
            } else {
                if let Some(pane) = self.remove_terminal_pane(&new_pane_id) {
                    let _ = pane.update(cx, |pane, _cx| pane.shutdown());
                }
                self.unregister_ssh_terminal_session(new_session_id);
            }
        }
        if remounted > 0 {
            self.focus_active_pane(window, cx);
            cx.notify();
        }
        remounted
    }

    fn resume_sftp_transfers_for_reconnect(&mut self, node_id: &NodeId) -> usize {
        let transfers_by_node = self
            .reconnect_orchestrator
            .job(&node_id.0)
            .map(|job| job.snapshot.incomplete_sftp_transfers_by_node)
            .unwrap_or_default();
        let mut queued = 0;
        let mut requests = Vec::new();
        for entry in transfers_by_node {
            let entry_node_id = NodeId::new(entry.node_id);
            if entry.transfer_ids.is_empty() {
                continue;
            }
            let pending = self
                .pending_reconnect_transfer_resumes
                .entry(node_id.clone())
                .or_default();
            for transfer_id in entry.transfer_ids {
                if pending.insert(transfer_id.clone()) {
                    requests.push((entry_node_id.clone(), transfer_id));
                    queued += 1;
                }
            }
        }
        for (entry_node_id, transfer_id) in requests {
            self.request_sftp_transfer_resume_for_node(entry_node_id, transfer_id);
        }
        if queued > 0 {
            self.reconnect_transfer_resume_totals
                .insert(node_id.clone(), queued);
            self.reconnect_transfer_resume_successes
                .insert(node_id.clone(), 0);
        }
        queued
    }

    pub(super) fn on_sftp_transfer_finished_for_reconnect(
        &mut self,
        _transfer_node_id: &NodeId,
        transfer_id: &str,
        success: bool,
        cx: &mut Context<Self>,
    ) {
        let roots = self
            .pending_reconnect_transfer_resumes
            .iter()
            .filter_map(|(root_id, pending)| {
                pending.contains(transfer_id).then_some(root_id.clone())
            })
            .collect::<Vec<_>>();
        for root_id in roots {
            let Some(pending) = self.pending_reconnect_transfer_resumes.get_mut(&root_id) else {
                continue;
            };
            if success {
                *self
                    .reconnect_transfer_resume_successes
                    .entry(root_id.clone())
                    .or_default() += 1;
            }
            pending.remove(transfer_id);
            if !pending.is_empty() {
                continue;
            }
            self.pending_reconnect_transfer_resumes.remove(&root_id);
            let _queued = self
                .reconnect_transfer_resume_totals
                .remove(&root_id)
                .unwrap_or_default();
            let resumed = self
                .reconnect_transfer_resume_successes
                .remove(&root_id)
                .unwrap_or_default();
            self.finish_reconnect_after_transfer_resume(
                &root_id,
                PhaseResult::Ok,
                format!("resumed {resumed} transfer(s)"),
                resumed as u32,
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
            .reconnect_orchestrator
            .job(&node_id.0)
            .is_some_and(|job| job.ended_at.is_none())
        {
            return;
        }
        let _ = self.reconnect_orchestrator.complete_phase(
            &node_id.0,
            transfer_result,
            Some(transfer_detail),
        );
        let _ = self
            .reconnect_orchestrator
            .advance(&node_id.0, ReconnectPhase::RestoreIde);
        self.log_reconnect_phase(node_id, ReconnectPhase::RestoreIde, None);
        match self.restore_ide_for_reconnect(node_id, cx) {
            super::ide::IdeReconnectRestoreStatus::Restored => {
                self.pending_ide_restore_transfer_counts
                    .insert(node_id.clone(), restored_transfers);
                self.complete_pending_ide_reconnect_restore(
                    node_id,
                    PhaseResult::Ok,
                    "restored IDE project and open files".to_string(),
                );
            }
            super::ide::IdeReconnectRestoreStatus::Pending => {
                self.pending_ide_restore_transfer_counts
                    .insert(node_id.clone(), restored_transfers);
            }
            super::ide::IdeReconnectRestoreStatus::Skipped => {
                self.pending_ide_restore_transfer_counts
                    .insert(node_id.clone(), restored_transfers);
                self.complete_pending_ide_reconnect_restore(
                    node_id,
                    PhaseResult::Skipped,
                    "no IDE snapshot for node".to_string(),
                );
            }
        }
    }

    pub(super) fn complete_pending_ide_reconnect_restore(
        &mut self,
        node_id: &NodeId,
        result: PhaseResult,
        detail: String,
    ) {
        if !self
            .reconnect_orchestrator
            .job(&node_id.0)
            .is_some_and(|job| job.ended_at.is_none())
        {
            self.pending_ide_restore_transfer_counts.remove(node_id);
            return;
        }
        let _ =
            self.reconnect_orchestrator
                .complete_phase(&node_id.0, result, Some(detail.clone()));
        if result == PhaseResult::Failed {
            self.pending_ide_restore_transfer_counts.remove(node_id);
            self.finish_reconnect_job(node_id, Err(detail));
            return;
        }
        let _ = self
            .reconnect_orchestrator
            .advance(&node_id.0, ReconnectPhase::Verify);
        self.log_reconnect_phase(node_id, ReconnectPhase::Verify, None);
        let _ = self.reconnect_orchestrator.complete_phase(
            &node_id.0,
            PhaseResult::Ok,
            Some(self.verify_forward_rules_for_reconnect(node_id)),
        );
        let restored_forwards = self
            .reconnect_forward_restore_totals
            .remove(node_id)
            .unwrap_or_default();
        let restored_transfers = self
            .pending_ide_restore_transfer_counts
            .remove(node_id)
            .unwrap_or_default();
        self.finish_reconnect_job(node_id, Ok(1 + restored_forwards + restored_transfers));
    }

    fn schedule_grace_period_reconnect(&mut self, node_id: &NodeId, cx: &mut Context<Self>) {
        if !self.settings_store.settings().reconnect.enabled {
            return;
        }
        if self
            .reconnect_orchestrator
            .job(&node_id.0)
            .is_some_and(|job| job.ended_at.is_none())
        {
            return;
        }
        self.pending_reconnect_node_ids.insert(node_id.clone());
        self.reconnect_debounce_generation = self.reconnect_debounce_generation.saturating_add(1);
        let generation = self.reconnect_debounce_generation;
        self.reconnect_debounce_scheduled = true;
        let tx = self.reconnect_worker_tx.clone();
        self.forwarding_runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(RECONNECT_DEBOUNCE_MS)).await;
            let _ = tx.send(ReconnectWorkerResult::FlushPendingReconnect { generation });
        });
        cx.notify();
    }

    fn flush_pending_reconnects(&mut self, generation: u64, cx: &mut Context<Self>) {
        if generation != self.reconnect_debounce_generation {
            return;
        }
        self.reconnect_debounce_scheduled = false;
        if !self.settings_store.settings().reconnect.enabled {
            self.pending_reconnect_node_ids.clear();
            return;
        }

        let pending = self.pending_reconnect_node_ids.drain().collect::<Vec<_>>();
        let mut roots = pending
            .into_iter()
            .filter(|node_id| self.ssh_nodes.contains_key(node_id))
            .collect::<Vec<_>>();
        roots.sort_by_key(|node_id| {
            self.node_runtime_store
                .snapshot(node_id)
                .map(|snapshot| (snapshot.depth, node_id.0.clone()))
                .unwrap_or((u32::MAX, node_id.0.clone()))
        });

        let mut selected_roots: Vec<NodeId> = Vec::new();
        for node_id in roots {
            if selected_roots
                .iter()
                .any(|root_id| self.node_is_descendant_of(&node_id, root_id))
            {
                continue;
            }
            selected_roots.push(node_id);
        }

        for node_id in selected_roots {
            self.start_grace_period_reconnect(&node_id, cx);
        }
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
            .reconnect_orchestrator
            .job(&node_id.0)
            .is_some_and(|job| job.ended_at.is_none())
        {
            return;
        }
        if self.has_active_reconnect_job_for_ancestor(node_id) {
            return;
        }
        if self
            .reconnect_pipeline_active_node
            .as_ref()
            .is_some_and(|active_node_id| active_node_id != node_id)
        {
            self.requeue_reconnect_pipeline(node_id);
            return;
        }
        self.reconnect_pipeline_active_node = Some(node_id.clone());
        self.reconnect_requeue_counts.remove(node_id);

        let mut affected_nodes = self.node_runtime_store.subtree_postorder(node_id);
        affected_nodes.reverse();
        let terminal_sessions_by_node = affected_nodes
            .iter()
            .filter_map(|affected_node_id| {
                let terminal_ids = self
                    .ssh_nodes
                    .get(affected_node_id)?
                    .terminal_ids
                    .iter()
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
        let reconnect_job =
            self.reconnect_orchestrator
                .schedule(node_id.0.clone(), node_title, snapshot);
        self.push_reconnect_notice(
            self.i18n_with(
                "connections.reconnect.starting",
                &[("name", reconnect_job.node_name.clone())],
            ),
            None,
            TerminalNoticeVariant::Default,
        );
        self.log_reconnect_phase(
            node_id,
            ReconnectPhase::Queued,
            Some("scheduled after link-down debounce".to_string()),
        );
        let _ = self
            .reconnect_orchestrator
            .advance(&node_id.0, ReconnectPhase::Snapshot);
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
        let progress_store = self.sftp_progress_store.clone();
        let registry = self.ssh_registry.clone();
        let tx = self.reconnect_worker_tx.clone();
        let timing = self.reconnect_orchestrator.timing();
        let runtime = self.forwarding_runtime.clone();
        let reconnect_job_id = reconnect_job.job_id.clone();
        runtime.spawn(async move {
            let mut transfers_by_node = Vec::new();
            for (affected_node_id, old_connection_id) in affected_transfer_nodes {
                match progress_store.list_incomplete(&old_connection_id).await {
                    Ok(transfers) => {
                        let transfer_ids = transfers
                            .into_iter()
                            .filter(StoredTransferProgress::is_incomplete)
                            .map(|transfer| transfer.transfer_id)
                            .collect::<Vec<_>>();
                        if !transfer_ids.is_empty() {
                            transfers_by_node.push(ReconnectNodeTransferSnapshot {
                                node_id: affected_node_id.0,
                                transfer_ids,
                            });
                        }
                    }
                    Err(_error) => {}
                }
            }
            let transfer_count = transfers_by_node
                .iter()
                .map(|entry| entry.transfer_ids.len())
                .sum::<usize>();
            let detail = format!(
                "{} transfer(s), {} connection(s)",
                transfer_count,
                old_connection_ids.len()
            );
            let _ = tx.send(ReconnectWorkerResult::SftpTransfersSnapshotted {
                node_id: node_id.clone(),
                transfers_by_node,
                detail,
                job_id: reconnect_job_id.clone(),
            });
            let started_at = tokio::time::Instant::now();
            loop {
                match registry
                    .probe_single_connection(&connection_id, timing.proactive_keepalive_timeout)
                    .await
                {
                    ProbeConnectionStatus::Alive => {
                        let mut recovered_connections = Vec::new();
                        for old_connection in &old_connections_by_node {
                            if old_connection.node_id == node_id.0 {
                                continue;
                            }
                            if matches!(
                                registry
                                    .probe_single_connection(
                                        &old_connection.old_connection_id,
                                        timing.proactive_keepalive_timeout,
                                    )
                                    .await,
                                ProbeConnectionStatus::Alive
                            ) {
                                recovered_connections.push((
                                    NodeId::new(old_connection.node_id.clone()),
                                    old_connection.old_connection_id.clone(),
                                ));
                            }
                        }
                        let _ = tx.send(ReconnectWorkerResult::GraceRecovered {
                            node_id,
                            connection_id,
                            recovered_connections,
                            job_id: reconnect_job_id,
                        });
                        return;
                    }
                    ProbeConnectionStatus::NotFound => {
                        let detail =
                            format!("connection {connection_id} is unavailable for grace probe");
                        let _ = tx.send(ReconnectWorkerResult::GraceExpired {
                            node_id,
                            connection_id,
                            detail,
                            job_id: reconnect_job_id,
                        });
                        return;
                    }
                    ProbeConnectionStatus::Dead | ProbeConnectionStatus::NotApplicable => {
                        if started_at.elapsed() >= timing.grace_period {
                            let detail = format!(
                                "connection {connection_id} did not recover within {:?}",
                                timing.grace_period
                            );
                            let _ = tx.send(ReconnectWorkerResult::GraceExpired {
                                node_id,
                                connection_id,
                                detail,
                                job_id: reconnect_job_id,
                            });
                            return;
                        }
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                }
            }
        });
    }

    fn start_reconnect_cascade_after_grace_expired(&mut self, root_node_id: &NodeId) {
        let mut affected_nodes = self.node_runtime_store.subtree_postorder(root_node_id);
        affected_nodes.reverse();
        if affected_nodes.is_empty() {
            affected_nodes.push(root_node_id.clone());
        }

        self.pending_reconnect_cascade_nodes.clear();
        for affected_node_id in affected_nodes
            .iter()
            .filter(|affected_node_id| *affected_node_id != root_node_id)
        {
            if self
                .ssh_nodes
                .get(affected_node_id)
                .is_some_and(|node| reconnect_cascade_child_should_start(&node.readiness))
            {
                self.pending_reconnect_cascade_nodes
                    .push_back(affected_node_id.clone());
            }
        }

        if let Some(node) = self.ssh_nodes.get_mut(root_node_id) {
            node.readiness = NodeReadiness::Connecting;
        }
        let _ = self.node_router.sync_node_readiness_event(
            root_node_id,
            NodeReadiness::Connecting,
            "grace expired",
        );
        if !self.ensure_node_connection_started(root_node_id) {
            self.pending_reconnect_cascade_nodes.clear();
        }
    }

    fn schedule_next_reconnect_cascade_node(&self) {
        if self.pending_reconnect_cascade_nodes.is_empty() {
            return;
        }
        let tx = self.reconnect_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = tx.send(ReconnectWorkerResult::ContinueReconnectCascade);
        });
    }

    fn start_next_reconnect_cascade_node(&mut self) -> bool {
        while let Some(node_id) = self.pending_reconnect_cascade_nodes.pop_front() {
            let parent_ready = self
                .node_runtime_store
                .snapshot(&node_id)
                .and_then(|snapshot| snapshot.parent_id)
                .is_some_and(|parent_id| self.node_is_ready_for_terminal(&parent_id));
            if !parent_ready {
                continue;
            }
            if self.ensure_node_connection_started_without_ancestors_with_mode(
                &node_id,
                ConnectionTraceMode::Reconnect,
            ) {
                return true;
            }
        }
        false
    }

    fn finish_reconnect_job(&mut self, node_id: &NodeId, result: Result<u32, String>) {
        self.cancel_forward_restore_token(node_id);
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
        if let Some(job) = self.reconnect_orchestrator.finish(&node_id.0, result) {
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
                self.push_reconnect_notice(title, detail, variant);
            }
            self.clear_reconnect_pipeline_active(node_id);
            self.reconnect_orchestrator
                .enforce_terminal_job_cap(MAX_RETAINED_RECONNECT_JOBS);
            let tx = self.reconnect_worker_tx.clone();
            let cleanup_node_id = node_id.clone();
            let started_at = job.started_at;
            self.forwarding_runtime.spawn(async move {
                tokio::time::sleep(Duration::from_millis(RECONNECT_AUTO_CLEANUP_DELAY_MS)).await;
                let _ = tx.send(ReconnectWorkerResult::CleanupReconnectJob {
                    node_id: cleanup_node_id,
                    started_at,
                });
            });
        }
    }

    fn requeue_reconnect_pipeline(&mut self, node_id: &NodeId) {
        let count = self
            .reconnect_requeue_counts
            .entry(node_id.clone())
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
        if *count > RECONNECT_MAX_REQUEUE {
            self.reconnect_requeue_counts.remove(node_id);
            self.finish_reconnect_job(node_id, Err("Pipeline queue exhausted".to_string()));
            return;
        }
        let tx = self.reconnect_worker_tx.clone();
        let expected_connection_id = self.node_router.connection_id_for_node(node_id);
        let node_id = node_id.clone();
        let delay = self.reconnect_orchestrator.retry_delay_for_attempt(1);
        self.forwarding_runtime.spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = tx.send(ReconnectWorkerResult::StartReconnectPipeline {
                node_id,
                expected_connection_id,
            });
        });
    }

    pub(super) fn clear_reconnect_pipeline_active(&mut self, node_id: &NodeId) {
        if self
            .reconnect_pipeline_active_node
            .as_ref()
            .is_some_and(|active_node_id| active_node_id == node_id)
        {
            self.reconnect_pipeline_active_node = None;
        }
        self.reconnect_requeue_counts.remove(node_id);
    }

    fn reconnect_worker_result_is_current(
        &self,
        node_id: &NodeId,
        worker_job_id: Option<&str>,
    ) -> bool {
        let Some(worker_job_id) = worker_job_id else {
            return true;
        };
        self.reconnect_orchestrator
            .job(&node_id.0)
            .is_some_and(|job| job.ended_at.is_none() && job.job_id == worker_job_id)
    }

    fn begin_forward_restore_token(&mut self, node_id: &NodeId) -> Arc<AtomicBool> {
        self.cancel_forward_restore_token(node_id);
        let token = Arc::new(AtomicBool::new(true));
        self.reconnect_forward_restore_tokens
            .insert(node_id.clone(), token.clone());
        token
    }

    pub(super) fn cancel_forward_restore_token(&mut self, node_id: &NodeId) {
        if let Some(token) = self.reconnect_forward_restore_tokens.remove(node_id) {
            token.store(false, Ordering::Release);
        }
    }

    fn cleanup_stale_reconnect_forward_restores(&self, created_forwards: Vec<(String, String)>) {
        if created_forwards.is_empty() {
            return;
        }
        let forwarding_registry = self.forwarding_registry.clone();
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
            self.forwarding_registry.stop_port_profiler(&connection_id);
            self.ssh_registry.release(&connection_id, &consumer);
            if self
                .forwarding_connection_consumers
                .get(&session_id)
                .is_some_and(|(stored_connection_id, stored_consumer)| {
                    stored_connection_id == &connection_id && stored_consumer == &consumer
                })
            {
                self.forwarding_connection_consumers.remove(&session_id);
            }
        }
    }

    fn drop_stale_node_connection(&mut self, node_id: &NodeId, connection_id: &str) {
        let consumer = ConnectionConsumer::NodeRouter(node_id.0.clone());
        self.ssh_registry.release(connection_id, &consumer);
        self.release_parent_ref_for_child_connection(node_id, connection_id);
        if let Some(handle) = self.ssh_registry.get(connection_id) {
            let runtime = self.forwarding_runtime.clone();
            runtime.spawn(async move {
                handle.clear_physical().await;
            });
        }
        let _ = self
            .ssh_registry
            .mark_state_without_event(connection_id, ConnectionState::Disconnected);
        self.node_router.emitter().unregister(connection_id);
        let _ = self.ssh_registry.retire_connection(connection_id);
    }

    pub(super) fn release_parent_ref_for_child_connection(
        &self,
        child_node_id: &NodeId,
        child_connection_id: &str,
    ) {
        let Some(parent_connection_id) = self
            .ssh_registry
            .get(child_connection_id)
            .and_then(|handle| handle.info().parent_connection_id)
        else {
            return;
        };
        // Tauri increments the parent connection ref when a tunneled child is
        // established and releases it when that child connection is destroyed.
        // Native represents that ref as a stable ancestor consumer.
        self.ssh_registry.release(
            &parent_connection_id,
            &ConnectionConsumer::NodeRouter(format!("{}:ancestor", child_node_id.0)),
        );
        let _ = self
            .ssh_registry
            .set_parent_connection_id(child_connection_id, None);
    }

    fn node_still_needs_reconnect(&self, node_id: &NodeId) -> bool {
        let Some(node) = self.ssh_nodes.get(node_id) else {
            return false;
        };
        if !matches!(node.readiness, NodeReadiness::Ready) {
            return true;
        }
        self.node_router
            .connection_id_for_node(node_id)
            .and_then(|connection_id| self.ssh_registry.get(&connection_id))
            .is_some_and(|handle| {
                matches!(
                    handle.state(),
                    ConnectionState::LinkDown
                        | ConnectionState::Disconnected
                        | ConnectionState::Disconnecting
                        | ConnectionState::Error(_)
                )
            })
    }

    fn node_is_descendant_of(&self, node_id: &NodeId, ancestor_id: &NodeId) -> bool {
        if node_id == ancestor_id {
            return true;
        }
        let mut cursor = self
            .node_runtime_store
            .snapshot(node_id)
            .and_then(|snapshot| snapshot.parent_id);
        while let Some(parent_id) = cursor {
            if &parent_id == ancestor_id {
                return true;
            }
            cursor = self
                .node_runtime_store
                .snapshot(&parent_id)
                .and_then(|snapshot| snapshot.parent_id);
        }
        false
    }

    fn has_active_reconnect_job_for_ancestor(&self, node_id: &NodeId) -> bool {
        let mut cursor = self
            .node_runtime_store
            .snapshot(node_id)
            .and_then(|snapshot| snapshot.parent_id);
        while let Some(parent_id) = cursor {
            if self
                .reconnect_orchestrator
                .job(&parent_id.0)
                .is_some_and(|job| job.ended_at.is_none())
            {
                return true;
            }
            cursor = self
                .node_runtime_store
                .snapshot(&parent_id)
                .and_then(|snapshot| snapshot.parent_id);
        }
        false
    }

    pub(super) fn ensure_node_connection_started(&mut self, node_id: &NodeId) -> bool {
        let trace_mode = if self
            .reconnect_orchestrator
            .job(&node_id.0)
            .is_some_and(|job| job.ended_at.is_none())
        {
            ConnectionTraceMode::Reconnect
        } else {
            ConnectionTraceMode::Connect
        };
        self.connect_node_with_ancestors(node_id, trace_mode)
    }

    pub(super) fn ensure_node_connection_started_without_ancestors(
        &mut self,
        node_id: &NodeId,
    ) -> bool {
        self.ensure_node_connection_started_without_ancestors_with_mode(
            node_id,
            ConnectionTraceMode::Connect,
        )
    }

    fn ensure_node_connection_started_without_ancestors_with_mode(
        &mut self,
        node_id: &NodeId,
        trace_mode: ConnectionTraceMode,
    ) -> bool {
        if self.node_connection_is_active_or_connecting(node_id) {
            return true;
        }
        if self.connecting_node_locks.contains(node_id) {
            return false;
        }
        self.connecting_node_locks.insert(node_id.clone());
        let trace_plan = ConnectionTracePlan {
            attempt_id: self.next_connection_trace_attempt_id(),
            mode: trace_mode,
            node_ids: vec![node_id.clone()],
        };
        if !self.ensure_single_node_connection_started_with_trace(node_id, Some(&trace_plan)) {
            self.connecting_node_locks.remove(node_id);
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
    ) -> bool {
        if self.active_connection_chain.is_some() {
            return false;
        }
        let Ok(path_node_ids) = self.node_runtime_store.path_to_node(node_id) else {
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
        if nodes_to_connect
            .iter()
            .any(|node_id| self.connecting_node_locks.contains(node_id))
        {
            return false;
        }
        for node_id in &nodes_to_connect {
            self.connecting_node_locks.insert(node_id.clone());
        }

        for node_id in &nodes_to_connect {
            self.reset_node_for_connection_chain(node_id);
        }

        let trace_plan = ConnectionTracePlan {
            attempt_id: self.next_connection_trace_attempt_id(),
            mode: trace_mode,
            node_ids: nodes_to_connect.clone(),
        };
        self.active_connection_chain = Some(ConnectionChainRun {
            node_ids: nodes_to_connect,
            next_index: 0,
            trace_plan,
        });
        self.start_next_connection_chain_node()
    }

    fn reset_node_for_connection_chain(&mut self, node_id: &NodeId) {
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
        if let Some(node) = self.ssh_nodes.get_mut(node_id) {
            node.readiness = NodeReadiness::Disconnected;
        }
        if let Ok(event) = self
            .node_router
            .disconnect_node_runtime(node_id, "reset before linear connection")
        {
            self.emit_node_event(event);
        }
    }

    fn start_next_connection_chain_node(&mut self) -> bool {
        let Some(run) = self.active_connection_chain.clone() else {
            return false;
        };
        let Some(node_id) = run.node_ids.get(run.next_index).cloned() else {
            self.release_connection_chain();
            return true;
        };
        if !self.ensure_single_node_connection_started_with_trace(&node_id, Some(&run.trace_plan)) {
            self.abort_connection_chain_for_node(&node_id);
            return false;
        }
        true
    }

    fn advance_connection_chain_after_node_connected(&mut self, node_id: &NodeId) {
        let Some(run) = self.active_connection_chain.as_mut() else {
            return;
        };
        let Some(current_id) = run.node_ids.get(run.next_index) else {
            self.release_connection_chain();
            return;
        };
        if current_id != node_id {
            return;
        }
        run.next_index += 1;
        if run.next_index >= run.node_ids.len() {
            self.release_connection_chain();
            self.schedule_next_reconnect_cascade_node();
            return;
        }
        let tx = self.reconnect_worker_tx.clone();
        let node_id = node_id.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = tx.send(ReconnectWorkerResult::ContinueConnectionChain { node_id });
        });
    }

    fn connection_chain_waits_after_node(&self, node_id: &NodeId) -> bool {
        let Some(run) = self.active_connection_chain.as_ref() else {
            return false;
        };
        run.next_index > 0
            && run
                .node_ids
                .get(run.next_index - 1)
                .is_some_and(|current_id| current_id == node_id)
    }

    fn abort_connection_chain_for_node(&mut self, node_id: &NodeId) {
        if self
            .active_connection_chain
            .as_ref()
            .is_some_and(|run| run.node_ids.contains(node_id))
        {
            self.release_connection_chain();
        }
    }

    fn release_connection_chain(&mut self) {
        if let Some(run) = self.active_connection_chain.take() {
            for node_id in run.node_ids {
                self.connecting_node_locks.remove(&node_id);
            }
        }
    }

    fn ensure_single_node_connection_started_with_trace(
        &mut self,
        node_id: &NodeId,
        trace_plan: Option<&ConnectionTracePlan>,
    ) -> bool {
        let Some(node) = self.ssh_nodes.get(node_id).cloned() else {
            return false;
        };
        let stale_connection_id =
            self.node_router
                .connection_id_for_node(node_id)
                .filter(|connection_id| {
                    self.ssh_registry.get(connection_id).is_some_and(|handle| {
                        matches!(
                            handle.state(),
                            ConnectionState::LinkDown
                                | ConnectionState::Disconnected
                                | ConnectionState::Disconnecting
                                | ConnectionState::Error(_)
                        )
                    })
                });
        let force_reconnect = stale_connection_id.is_some();
        if matches!(
            node.readiness,
            NodeReadiness::Ready | NodeReadiness::Connecting
        ) && let Some(connection_id) = self.node_router.connection_id_for_node(node_id)
            && let Some(handle) = self.ssh_registry.get(&connection_id)
        {
            let state = handle.state();
            let has_terminal_consumer = !node.terminal_ids.is_empty();
            // Terminal panes are only shell-channel consumers. When no terminal
            // remains, reopening SFTP/forwards must prove or rebuild the node
            // transport through connect_tree_node instead of treating the old
            // shell-created connection as authoritative.
            if matches!(
                state,
                ConnectionState::Connecting | ConnectionState::Reconnecting
            ) || (has_terminal_consumer
                && matches!(state, ConnectionState::Active | ConnectionState::Idle))
            {
                return true;
            }
            // Tauri's node workflows can be reopened after all terminal panes
            // are closed because connect_tree_node owns the physical transport.
            // If native has no terminal consumer left, re-enter the node-only
            // connect path instead of trusting a possibly stale shell-created
            // handle. The transport layer will cheaply reuse an open pooled
            // connection, or replace it when it has been closed.
        }

        let parent_id = self
            .node_runtime_store
            .snapshot(node_id)
            .and_then(|snapshot| snapshot.parent_id);
        if let Some(parent_id) = parent_id.as_ref()
            && !self.node_is_ready_for_terminal(parent_id)
        {
            let error = format!("Parent node {} has no SSH connection", parent_id.0);
            self.begin_connection_trace_for_node(node_id, trace_plan, Some(parent_id));
            if let Some(node) = self.ssh_nodes.get_mut(node_id) {
                node.readiness = NodeReadiness::Error;
            }
            if let Ok(event) = self.node_router.sync_node_readiness_event(
                node_id,
                NodeReadiness::Error,
                error.clone(),
            ) {
                self.emit_node_event(event);
            }
            self.finish_connection_trace_failed(node_id, Some(error));
            return false;
        }
        self.begin_connection_trace_for_node(node_id, trace_plan, parent_id.as_ref());
        if let Some(connection_id) = stale_connection_id.as_deref() {
            self.drop_stale_node_connection(node_id, connection_id);
        }

        let origin = self
            .node_runtime_store
            .snapshot(node_id)
            .map(|snapshot| snapshot.origin)
            .or_else(|| {
                node.saved_connection_id
                    .as_ref()
                    .map(|id| NodeOrigin::Restored {
                        saved_connection_id: id.clone(),
                    })
            })
            .unwrap_or(NodeOrigin::Direct);
        self.node_runtime_store.upsert_node_with_origin(
            node_id.clone(),
            node.config.clone(),
            origin,
        );
        let consumer = ConnectionConsumer::NodeRouter(node_id.0.clone());
        let handle = self
            .ssh_registry
            .acquire(node.config.clone(), consumer.clone());
        let connection_id = handle.connection_id().to_string();
        let _ = self
            .ssh_registry
            .mark_state(&connection_id, ConnectionState::Connecting);
        if let Ok(event) = self
            .node_router
            .bind_connection(node_id, connection_id.clone())
        {
            self.emit_node_event(event);
        }
        if let Some(node) = self.ssh_nodes.get_mut(node_id) {
            node.readiness = NodeReadiness::Connecting;
        }

        let config = node.config;
        let registry = self.ssh_registry.clone();
        let router = self.node_router.clone();
        let tx = self.reconnect_worker_tx.clone();
        let worker_job_id = self
            .reconnect_orchestrator
            .job(&node_id.0)
            .filter(|job| job.ended_at.is_none())
            .map(|job| job.job_id);
        let node_id = node_id.clone();
        let node_handle = handle.clone();
        let prompt_handler =
            std::sync::Arc::new(NativeSshPromptHandler::new(self.ssh_worker_tx.clone()));
        let managed_key_resolver =
            oxideterm_session_adapter::managed_key_resolver_from_store(&self.connection_store);
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            // This is the native connect_tree_node path: authenticate the SSH
            // transport into the registry's physical slot without creating a
            // terminal shell. SFTP/forwarding then resolve the node through
            // NodeRouter exactly like Tauri node_* commands.
            if force_reconnect {
                node_handle.clear_physical().await;
            }
            let client = SshTransportClient::new(config)
                .with_prompt_handler(prompt_handler)
                .with_managed_key_resolver(managed_key_resolver);
            let parent_consumer = parent_id
                .as_ref()
                .map(|_| ConnectionConsumer::NodeRouter(format!("{}:ancestor", node_id.0)));
            let parent_handle = if let Some(parent_id) = parent_id {
                // Tauri's connect_tree_node waits for the parent path before
                // dialing a tunneled child. Native must do the same here: a
                // fast SFTP/terminal open can request the target while the
                // jump host is still Connecting.
                match router
                    .acquire_connection_wait(
                        &parent_id,
                        parent_consumer.clone().expect("parent consumer"),
                        Duration::from_secs(30),
                    )
                    .await
                {
                    Ok(parent) => Some(parent.handle),
                    Err(error) => {
                        let _ = tx.send(ReconnectWorkerResult::NodeConnectFailed {
                            node_id,
                            error: error.to_string(),
                            job_id: worker_job_id.clone(),
                        });
                        return;
                    }
                }
            } else {
                None
            };
            let result = if let Some(parent_handle) = parent_handle {
                client
                    .connect_child_node_via_parent_with_registry(
                        registry,
                        consumer,
                        node_handle,
                        parent_handle,
                        parent_consumer.expect("parent consumer"),
                    )
                    .await
            } else {
                client
                    .connect_existing_node_with_registry(registry, consumer, node_handle)
                    .await
            }
            .map(|handle| handle.connection_id().to_string())
            .map_err(|error| error.to_string());
            let _ = match result {
                Ok(connection_id) => tx.send(ReconnectWorkerResult::NodeConnected {
                    node_id,
                    connection_id,
                    job_id: worker_job_id,
                }),
                Err(error) => tx.send(ReconnectWorkerResult::NodeConnectFailed {
                    node_id,
                    error,
                    job_id: worker_job_id,
                }),
            };
        });
        true
    }

    fn restore_forwarding_session_for_node(&mut self, node_id: &NodeId) {
        let session_id = self.forwarding_session_id_for_node(node_id);
        let consumer = ConnectionConsumer::PortForward(session_id.clone());
        let forwarding_registry = self.forwarding_registry.clone();
        let runtime = self.forwarding_runtime.clone();
        let router = self.node_router.clone();
        let node_id = node_id.clone();
        let tx = self.forwarding_worker_tx.clone();
        runtime.spawn(async move {
            let binding = match router
                .acquire_connection_wait(&node_id, consumer.clone(), Duration::from_secs(15))
                .await
            {
                Ok(handle) => {
                    let connection_id = handle.connection_id.clone();
                    let _ = forwarding_registry
                        .restore_session(&session_id, handle.handle)
                        .await;
                    Some((session_id, connection_id, consumer))
                }
                Err(_) => None,
            };
            let _ = tx.send(ForwardingWorkerResult::Binding { binding });
        });
    }

    fn restore_forwarding_rules_for_reconnect(&mut self, node_id: &NodeId) {
        let Some(job) = self.reconnect_orchestrator.job(&node_id.0) else {
            return;
        };
        if job.snapshot.forward_rules.is_empty() {
            return;
        }

        let job_id = job.job_id.clone();
        let snapshots = job.snapshot.forward_rules.clone();
        let restore_token = self.begin_forward_restore_token(node_id);
        let old_connection_ids_by_node = job
            .snapshot
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
        let router = self.node_router.clone();
        let forwarding_registry = self.forwarding_registry.clone();
        let runtime = self.forwarding_runtime.clone();
        let tx = self.reconnect_worker_tx.clone();
        let root_node_id = node_id.clone();
        runtime.spawn(async move {
            let mut restored = 0_u32;
            let mut failures = 0_u32;
            let mut failure_details = Vec::<String>::new();
            let mut created_forwards = Vec::<(String, String)>::new();
            let mut bindings = Vec::<(String, String, ConnectionConsumer)>::new();
            for entry in snapshots {
                if !restore_token.load(Ordering::Acquire) {
                    cleanup_reconnect_created_forwards(&forwarding_registry, &created_forwards)
                        .await;
                    release_reconnect_forward_bindings(&router, &bindings);
                    return;
                }
                let entry_node_id = NodeId::new(entry.node_id.clone());
                let session_id = format!(
                    "{}{}",
                    crate::workspace::forwards::FORWARDS_NODE_SESSION_PREFIX,
                    entry.node_id
                );
                let consumer = ConnectionConsumer::PortForward(session_id.clone());
                let resolved = match router
                    .acquire_connection_wait(
                        &entry_node_id,
                        consumer.clone(),
                        Duration::from_secs(15),
                    )
                    .await
                {
                    Ok(resolved) => resolved,
                    Err(error) => {
                        failures += entry.rules.len() as u32;
                        for rule in &entry.rules {
                            failure_details.push(format!(
                                "{}: {}",
                                forward_restore_failure_label(rule),
                                error
                            ));
                        }
                        continue;
                    }
                };
                let binding = (
                    session_id.clone(),
                    resolved.connection_id.clone(),
                    consumer.clone(),
                );
                if !restore_token.load(Ordering::Acquire) {
                    router.release_consumer(&resolved.connection_id, &consumer);
                    cleanup_reconnect_created_forwards(&forwarding_registry, &created_forwards)
                        .await;
                    release_reconnect_forward_bindings(&router, &bindings);
                    return;
                }
                let manager = forwarding_registry
                    .register_for_reconnect_restore(
                        session_id.clone(),
                        resolved.handle,
                        old_connection_ids_by_node
                            .get(&entry.node_id)
                            .map(String::as_str),
                    )
                    .await;
                bindings.push(binding);
                let live_keys = manager
                    .list_forwards()
                    .into_iter()
                    .map(|rule| forward_restore_key_for_rule(&rule))
                    .collect::<HashSet<_>>();
                let mut live_keys = live_keys;
                for snapshot_rule in entry.rules {
                    let key = forward_restore_key_for_snapshot_rule(&snapshot_rule);
                    for live_rule in manager.list_forwards() {
                        live_keys.insert(forward_restore_key_for_rule(&live_rule));
                    }
                    if live_keys.contains(&key) {
                        continue;
                    }
                    if !restore_token.load(Ordering::Acquire) {
                        cleanup_reconnect_created_forwards(&forwarding_registry, &created_forwards)
                            .await;
                        release_reconnect_forward_bindings(&router, &bindings);
                        return;
                    }
                    let failure_label = forward_restore_failure_label(&snapshot_rule);
                    let Some(rule) = forward_rule_from_reconnect_snapshot(&snapshot_rule) else {
                        failures += 1;
                        failure_details.push(format!(
                            "{failure_label}: unsupported forward type '{}'",
                            snapshot_rule.forward_type
                        ));
                        continue;
                    };
                    match manager.create_forward_with_health_check(rule, true).await {
                        Ok(created) => {
                            live_keys.insert(forward_restore_key_for_rule(&created));
                            restored += 1;
                            created_forwards.push((session_id.clone(), created.id.clone()));
                            if let Some(owner_connection_id) =
                                owner_connection_ids.get(&entry.node_id).cloned().flatten()
                            {
                                let created_id = created.id.clone();
                                let _ = forwarding_registry.sync_persisted_forward_rule(
                                    &created_id,
                                    &session_id,
                                    Some(owner_connection_id),
                                    created,
                                );
                            }
                        }
                        Err(error) => {
                            failures += 1;
                            failure_details.push(format!("{failure_label}: {error}"));
                        }
                    }
                }
            }
            let detail = forward_restore_result_detail(restored, failures, &failure_details);
            let _ = tx.send(ReconnectWorkerResult::ForwardRulesRestored {
                node_id: root_node_id,
                result: forward_restore_phase_result(failures),
                restored,
                detail,
                job_id,
                created_forwards,
                bindings,
            });
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
                    .forwarding_registry
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

    fn verify_forward_rules_for_reconnect(&self, node_id: &NodeId) -> String {
        let Some(job) = self.reconnect_orchestrator.job(&node_id.0) else {
            return "native node reconnect verified".to_string();
        };
        let mut drifts = Vec::new();
        for entry in job.snapshot.forward_rules {
            let entry_node_id = NodeId::new(entry.node_id.clone());
            let expected = entry.rules.len();
            let live = self
                .forwarding_registry
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

    fn readiness_for_ssh_node_connection(&self, node: &WorkspaceSshNode) -> Option<NodeReadiness> {
        let connection_key = node.config.connection_key();
        self.ssh_registry
            .list()
            .into_iter()
            .find(|connection| connection.key == connection_key)
            .map(|connection| readiness_for_connection_state(&connection.state))
    }

    pub(super) fn reconnect_all_link_down_nodes_from_palette(&mut self, cx: &mut Context<Self>) {
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
