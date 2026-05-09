impl WorkspaceApp {
    pub(super) fn sync_ssh_node_lifecycle(&mut self, cx: &mut Context<Self>) {
        let terminal_nodes = self.terminal_ssh_nodes.clone();
        let mut changed = false;
        let mut forwarding_to_suspend = Vec::new();
        let mut nodes_to_restore = Vec::new();
        let mut nodes_to_grace = Vec::new();
        for (session_id, node_id) in terminal_nodes {
            let terminal_snapshot = self.terminal_endpoint_sessions.get(&session_id).map(
                |endpoint_session| {
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
                },
            );
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
        if changed {
            cx.notify();
        }
    }

    pub(super) fn poll_node_events(&mut self, cx: &mut Context<Self>) {
        let mut events = Vec::new();
        while let Ok(event) = self.node_event_rx.try_recv() {
            events.push(event);
        }

        let mut changed = false;
        for event in events {
            changed |= self.apply_node_event(event, cx);
        }
        if changed {
            cx.notify();
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
                } => {
                    if self
                        .reconnect_orchestrator
                        .job(&node_id.0)
                        .is_some_and(|job| job.ended_at.is_none())
                    {
                        let _ = self.reconnect_orchestrator.complete_phase(
                            &node_id.0,
                            PhaseResult::Ok,
                            Some(format!("reconnected as {connection_id}")),
                        );
                        let _ = self
                            .reconnect_orchestrator
                            .advance(&node_id.0, ReconnectPhase::AwaitTerminal);
                        let remounted =
                            self.remount_terminal_panes_for_reconnect(&node_id, window, cx);
                        let terminal_message = format!(
                            "fixed {remounted} terminal pane(s) through native remount"
                        );
                        let _ = self.reconnect_orchestrator.complete_phase(
                            &node_id.0,
                            PhaseResult::Ok,
                            Some(terminal_message),
                        );
                        let _ = self
                            .reconnect_orchestrator
                            .advance(&node_id.0, ReconnectPhase::RestoreForwards);
                    }
                    if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                        node.readiness = NodeReadiness::Ready;
                    }
                    if let Ok(event) = self.node_router.bind_connection(&node_id, connection_id) {
                        self.emit_node_event(event);
                    }
                    self.persist_session_tree_snapshot();
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
                    let children_to_start = self
                        .node_runtime_store
                        .snapshot(&node_id)
                        .map(|snapshot| snapshot.children_ids)
                        .unwrap_or_default();
                    for child_id in children_to_start {
                        if self.ssh_nodes.get(&child_id).is_some_and(|child| {
                            child.readiness == NodeReadiness::Connecting
                        }) {
                            self.ensure_node_connection_started(&child_id);
                        }
                    }
                    changed = true;
                }
                ReconnectWorkerResult::NodeConnectFailed { node_id, error } => {
                    if self
                        .reconnect_orchestrator
                        .job(&node_id.0)
                        .is_some_and(|job| job.ended_at.is_none())
                    {
                        let _ = self.reconnect_orchestrator.complete_phase(
                            &node_id.0,
                            PhaseResult::Failed,
                            Some(error.clone()),
                        );
                        let _ = self
                            .reconnect_orchestrator
                            .finish(&node_id.0, Err(error.clone()));
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
                    self.persist_session_tree_snapshot();
                    changed = true;
                }
                ReconnectWorkerResult::GraceRecovered {
                    node_id,
                    connection_id,
                } => {
                    let _ = self.reconnect_orchestrator.complete_phase(
                        &node_id.0,
                        PhaseResult::Ok,
                        Some(format!(
                            "connection {connection_id} recovered during grace period"
                        )),
                    );
                    let _ = self.reconnect_orchestrator.finish(&node_id.0, Ok(0));
                    if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                        node.readiness = NodeReadiness::Ready;
                    }
                    if let Some(info) = self
                        .ssh_registry
                        .mark_state(&connection_id, ConnectionState::Active)
                        && let Some(event) = self
                            .node_router
                            .sync_connection_state_by_connection_id(&info, "grace recovered")
                    {
                        self.emit_node_event(event);
                    }
                    self.restore_forwarding_session_for_node(&node_id);
                    changed = true;
                }
                ReconnectWorkerResult::GraceExpired {
                    node_id,
                    connection_id,
                    detail,
                } => {
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
                    // Tauri falls back from grace-period probing to a full
                    // connect_tree_node rebuild. Native does the same by
                    // restarting the node-only transport path instead of
                    // fabricating a terminal pane.
                    self.ensure_node_connection_started(&node_id);
                    changed = true;
                }
                ReconnectWorkerResult::SftpTransfersSnapshotted {
                    node_id,
                    transfers_by_node,
                    detail,
                } => {
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
                    }
                    changed = true;
                }
                ReconnectWorkerResult::ForwardRulesRestored {
                    node_id,
                    result,
                    restored,
                    detail,
                } => {
                    self.reconnect_forward_restore_totals
                        .insert(node_id.clone(), restored);
                    if self
                        .reconnect_orchestrator
                        .job(&node_id.0)
                        .is_some_and(|job| job.ended_at.is_none())
                    {
                        let _ =
                            self.reconnect_orchestrator
                                .complete_phase(&node_id.0, result, Some(detail));
                        let _ = self
                            .reconnect_orchestrator
                            .advance(&node_id.0, ReconnectPhase::ResumeTransfers);
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
            }
        }
        if changed {
            cx.notify();
        }
    }

    pub(super) fn emit_node_event(&self, event: NodeStateEvent) {
        let _ = self.node_event_tx.send(event);
    }

    fn apply_node_event(&mut self, event: NodeStateEvent, cx: &mut Context<Self>) -> bool {
        match event {
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
                self.node_event_generations
                    .insert(node_id.clone(), generation);
                let previous = self
                    .ssh_nodes
                    .get(&node_id)
                    .map(|node| node.readiness.clone());
                if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                    node.readiness = state.clone();
                }
                if matches!(previous, Some(NodeReadiness::Ready))
                    && matches!(state, NodeReadiness::Error | NodeReadiness::Disconnected)
                {
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
                if let Some(pane) = self.panes.remove(&replaced_pane_id) {
                    let _ = pane.update(cx, |pane, _cx| pane.shutdown());
                }
                self.unregister_ssh_terminal_session(old_session_id);
                remounted += 1;
            } else {
                if let Some(pane) = self.panes.remove(&new_pane_id) {
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
        }
        queued
    }

    pub(super) fn on_sftp_transfer_finished_for_reconnect(
        &mut self,
        _transfer_node_id: &NodeId,
        transfer_id: &str,
        cx: &mut Context<Self>,
    ) {
        let roots = self
            .pending_reconnect_transfer_resumes
            .iter()
            .filter_map(|(root_id, pending)| {
                pending
                    .contains(transfer_id)
                    .then_some(root_id.clone())
            })
            .collect::<Vec<_>>();
        for root_id in roots {
            let Some(pending) = self.pending_reconnect_transfer_resumes.get_mut(&root_id) else {
                continue;
            };
            pending.remove(transfer_id);
            if !pending.is_empty() {
                continue;
            }
            self.pending_reconnect_transfer_resumes.remove(&root_id);
            let total = self
                .reconnect_transfer_resume_totals
                .remove(&root_id)
                .unwrap_or_default();
            self.finish_reconnect_after_transfer_resume(
                &root_id,
                PhaseResult::Ok,
                format!("resumed {total} transfer(s)"),
                total as u32,
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
        let restored_ide = self.restore_ide_for_reconnect(node_id, cx);
        let _ = self.reconnect_orchestrator.complete_phase(
            &node_id.0,
            if restored_ide {
                PhaseResult::Ok
            } else {
                PhaseResult::Skipped
            },
            Some(if restored_ide {
                "restored IDE project and open files".to_string()
            } else {
                "no IDE snapshot for node".to_string()
            }),
        );
        let _ = self
            .reconnect_orchestrator
            .advance(&node_id.0, ReconnectPhase::Verify);
        let _ = self.reconnect_orchestrator.complete_phase(
            &node_id.0,
            PhaseResult::Ok,
            Some(self.verify_forward_rules_for_reconnect(node_id)),
        );
        let restored_forwards = self
            .reconnect_forward_restore_totals
            .remove(node_id)
            .unwrap_or_default();
        let _ = self
            .reconnect_orchestrator
            .finish(&node_id.0, Ok(1 + restored_forwards + restored_transfers));
    }

    fn schedule_grace_period_reconnect(&mut self, node_id: &NodeId, cx: &mut Context<Self>) {
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
        let old_connection_ids = affected_nodes
            .iter()
            .filter_map(|affected_node_id| self.node_router.connection_id_for_node(affected_node_id))
            .collect::<Vec<_>>();
        let forward_rules = self.forward_rules_snapshot_for_nodes(&affected_nodes);
        let active_port_forward_ids = forward_rules
            .iter()
            .flat_map(|entry| entry.rules.iter().map(|rule| rule.id.clone()))
            .collect::<Vec<_>>();
        let ide_snapshot = self.ide_snapshot_for_node(node_id, cx);
        let snapshot = ReconnectSnapshot {
            old_terminal_session_ids,
            terminal_sessions_by_node,
            forward_rules,
            active_port_forward_ids,
            old_connection_ids: old_connection_ids.clone(),
            ide_snapshot,
            ..ReconnectSnapshot::default()
        };
        let _ = self
            .reconnect_orchestrator
            .schedule(node_id.0.clone(), node_title, snapshot);
        let _ = self
            .reconnect_orchestrator
            .advance(&node_id.0, ReconnectPhase::Snapshot);

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
            });
            let started_at = tokio::time::Instant::now();
            loop {
                match registry
                    .probe_single_connection(&connection_id, timing.proactive_keepalive_timeout)
                    .await
                {
                    ProbeConnectionStatus::Alive => {
                        let _ = tx.send(ReconnectWorkerResult::GraceRecovered {
                            node_id,
                            connection_id,
                        });
                        return;
                    }
                    ProbeConnectionStatus::NotFound | ProbeConnectionStatus::NotApplicable => {
                        let detail =
                            format!("connection {connection_id} is unavailable for grace probe");
                        let _ = tx.send(ReconnectWorkerResult::GraceExpired {
                            node_id,
                            connection_id,
                            detail,
                        });
                        return;
                    }
                    ProbeConnectionStatus::Dead => {
                        if started_at.elapsed() >= timing.grace_period {
                            let detail = format!(
                                "connection {connection_id} did not recover within {:?}",
                                timing.grace_period
                            );
                            let _ = tx.send(ReconnectWorkerResult::GraceExpired {
                                node_id,
                                connection_id,
                                detail,
                            });
                            return;
                        }
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                }
            }
        });
    }

    pub(super) fn ensure_node_connection_started(&mut self, node_id: &NodeId) -> bool {
        let Some(node) = self.ssh_nodes.get(node_id).cloned() else {
            return false;
        };
        let force_reconnect = self
            .node_router
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
            });
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
        self.node_runtime_store
            .upsert_node_with_origin(node_id.clone(), node.config.clone(), origin);
        let consumer = ConnectionConsumer::NodeRouter(node_id.0.clone());
        let handle = self.ssh_registry.acquire(node.config.clone(), consumer.clone());
        let connection_id = handle.connection_id().to_string();
        let _ = self
            .ssh_registry
            .mark_state(&connection_id, ConnectionState::Connecting);
        if let Ok(event) = self.node_router.bind_connection(node_id, connection_id.clone()) {
            self.emit_node_event(event);
        }
        if let Some(node) = self.ssh_nodes.get_mut(node_id) {
            node.readiness = NodeReadiness::Connecting;
        }

        let parent_id = self
            .node_runtime_store
            .snapshot(node_id)
            .and_then(|snapshot| snapshot.parent_id);
        if let Some(parent_id) = parent_id.as_ref() {
            self.ensure_node_connection_started(parent_id);
        }
        let config = node.config;
        let registry = self.ssh_registry.clone();
        let router = self.node_router.clone();
        let tx = self.reconnect_worker_tx.clone();
        let node_id = node_id.clone();
        let node_handle = handle.clone();
        let prompt_handler =
            std::sync::Arc::new(NativeSshPromptHandler::new(self.ssh_worker_tx.clone()));
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            // This is the native connect_tree_node path: authenticate the SSH
            // transport into the registry's physical slot without creating a
            // terminal shell. SFTP/forwarding then resolve the node through
            // NodeRouter exactly like Tauri node_* commands.
            if force_reconnect {
                node_handle.clear_physical().await;
            }
            let client = SshTransportClient::new(config).with_prompt_handler(prompt_handler);
            let parent_handle = if let Some(parent_id) = parent_id {
                // Tauri's connect_tree_node waits for the parent path before
                // dialing a tunneled child. Native must do the same here: a
                // fast SFTP/terminal open can request the target while the
                // jump host is still Connecting.
                match router
                    .acquire_connection_wait(
                        &parent_id,
                        ConnectionConsumer::NodeRouter(format!("{}:ancestor", node_id.0)),
                        Duration::from_secs(30),
                    )
                    .await
                {
                    Ok(parent) => Some(parent.handle),
                    Err(error) => {
                        let _ = tx.send(ReconnectWorkerResult::NodeConnectFailed {
                            node_id,
                            error: error.to_string(),
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
                }),
                Err(error) => tx.send(ReconnectWorkerResult::NodeConnectFailed {
                    node_id,
                    error,
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

        let snapshots = job.snapshot.forward_rules.clone();
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
        let forwarding_worker_tx = self.forwarding_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        let tx = self.reconnect_worker_tx.clone();
        let root_node_id = node_id.clone();
        runtime.spawn(async move {
            let mut restored = 0_u32;
            let mut failures = 0_u32;
            for entry in snapshots {
                let entry_node_id = NodeId::new(entry.node_id.clone());
                let session_id = format!("{}{}", crate::workspace::forwards::FORWARDS_NODE_SESSION_PREFIX, entry.node_id);
                let consumer = ConnectionConsumer::PortForward(session_id.clone());
                let resolved = match router
                    .acquire_connection_wait(&entry_node_id, consumer.clone(), Duration::from_secs(15))
                    .await
                {
                    Ok(resolved) => resolved,
                    Err(_) => {
                        failures += entry.rules.len() as u32;
                        continue;
                    }
                };
                let _ = forwarding_worker_tx.send(ForwardingWorkerResult::Binding {
                    binding: Some((
                        session_id.clone(),
                        resolved.connection_id.clone(),
                        consumer.clone(),
                    )),
                });
                let manager = forwarding_registry.register(session_id.clone(), resolved.handle);
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
                    let Some(rule) = forward_rule_from_reconnect_snapshot(&snapshot_rule) else {
                        failures += 1;
                        continue;
                    };
                    match manager.create_forward_with_health_check(rule, true).await {
                        Ok(created) => {
                            live_keys.insert(forward_restore_key_for_rule(&created));
                            restored += 1;
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
                        Err(_) => failures += 1,
                    }
                }
            }
            let detail = if failures == 0 {
                format!("restored {restored} forward(s)")
            } else {
                format!("restored {restored} forward(s), {failures} failed")
            };
            let _ = tx.send(ReconnectWorkerResult::ForwardRulesRestored {
                node_id: root_node_id,
                result: PhaseResult::Ok,
                restored,
                detail,
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
            format!("native node reconnect verified with drift: {}", drifts.join("; "))
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
}

fn reconnect_forward_rule_from_rule(rule: ForwardRule) -> ReconnectForwardRule {
    ReconnectForwardRule {
        id: rule.id,
        forward_type: forward_type_to_snapshot(rule.forward_type).to_string(),
        bind_address: rule.bind_address,
        bind_port: rule.bind_port,
        target_host: rule.target_host,
        target_port: rule.target_port,
        status: forward_status_to_snapshot(&rule.status).to_string(),
        description: rule.description,
    }
}

fn forward_rule_from_reconnect_snapshot(rule: &ReconnectForwardRule) -> Option<ForwardRule> {
    let mut restored = match rule.forward_type.as_str() {
        "local" => ForwardRule::local(
            rule.bind_address.clone(),
            rule.bind_port,
            rule.target_host.clone(),
            rule.target_port,
        ),
        "remote" => ForwardRule::remote(
            rule.bind_address.clone(),
            rule.bind_port,
            rule.target_host.clone(),
            rule.target_port,
        ),
        "dynamic" => ForwardRule {
            target_host: rule.target_host.clone(),
            target_port: rule.target_port,
            ..ForwardRule::dynamic(rule.bind_address.clone(), rule.bind_port)
        },
        _ => return None,
    };
    // Tauri restore calls nodeCreateForward, which allocates a fresh id and
    // starts from Starting. Preserve that instead of resurrecting stale ids.
    restored.description = rule.description.clone();
    restored.status = ForwardStatus::Starting;
    Some(restored)
}

fn forward_restore_key_for_rule(rule: &ForwardRule) -> String {
    [
        forward_type_to_snapshot(rule.forward_type).to_string(),
        rule.bind_address.clone(),
        rule.bind_port.to_string(),
        rule.target_host.clone(),
        rule.target_port.to_string(),
    ]
    .join(":")
}

fn forward_restore_key_for_snapshot_rule(rule: &ReconnectForwardRule) -> String {
    [
        rule.forward_type.clone(),
        rule.bind_address.clone(),
        rule.bind_port.to_string(),
        rule.target_host.clone(),
        rule.target_port.to_string(),
    ]
    .join(":")
}

fn forward_type_to_snapshot(forward_type: ForwardType) -> &'static str {
    match forward_type {
        ForwardType::Local => "local",
        ForwardType::Remote => "remote",
        ForwardType::Dynamic => "dynamic",
    }
}

fn forward_status_to_snapshot(status: &ForwardStatus) -> &'static str {
    match status {
        ForwardStatus::Starting => "starting",
        ForwardStatus::Active => "active",
        ForwardStatus::Stopped => "stopped",
        ForwardStatus::Error => "error",
        ForwardStatus::Suspended => "suspended",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_forward_restore_key_keeps_distinct_targets() {
        let service_a = ReconnectForwardRule {
            forward_type: "local".to_string(),
            bind_address: "127.0.0.1".to_string(),
            bind_port: 8080,
            target_host: "service-a".to_string(),
            target_port: 3000,
            ..ReconnectForwardRule::default()
        };
        let service_b = ReconnectForwardRule {
            target_host: "service-b".to_string(),
            target_port: 4000,
            ..service_a.clone()
        };

        assert_ne!(
            forward_restore_key_for_snapshot_rule(&service_a),
            forward_restore_key_for_snapshot_rule(&service_b)
        );
    }

    #[test]
    fn reconnect_forward_restore_allocates_fresh_starting_rule() {
        let snapshot = ReconnectForwardRule {
            id: "old-forward-id".to_string(),
            forward_type: "dynamic".to_string(),
            bind_address: "127.0.0.1".to_string(),
            bind_port: 1080,
            target_host: "0.0.0.0".to_string(),
            target_port: 0,
            status: "active".to_string(),
            description: "socks".to_string(),
        };

        let restored = forward_rule_from_reconnect_snapshot(&snapshot)
            .expect("dynamic snapshot should restore");

        assert_ne!(restored.id, snapshot.id);
        assert_eq!(restored.status, ForwardStatus::Starting);
        assert_eq!(restored.target_host, "0.0.0.0");
        assert_eq!(restored.target_port, 0);
        assert_eq!(restored.description, "socks");
    }
}
