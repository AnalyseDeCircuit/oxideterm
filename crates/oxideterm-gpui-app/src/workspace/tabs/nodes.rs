impl WorkspaceApp {
    pub(super) fn sync_ssh_node_lifecycle(&mut self, cx: &mut Context<Self>) {
        let terminal_nodes = self.terminal_ssh_nodes.clone();
        let mut changed = false;
        let mut sessions_to_suspend = Vec::new();
        let mut nodes_to_restore = Vec::new();
        let mut nodes_to_grace = Vec::new();
        for (session_id, node_id) in terminal_nodes {
            let terminal_snapshot = self
                .pane_id_for_session(session_id)
                .and_then(|pane_id| self.panes.get(&pane_id))
                .map(|pane| {
                    let pane = pane.read(cx);
                    let readiness = match pane.lifecycle() {
                        TerminalLifecycle::Running => NodeReadiness::Ready,
                        TerminalLifecycle::Exited(_) => NodeReadiness::Error,
                        TerminalLifecycle::Closed => NodeReadiness::Disconnected,
                    };
                    (readiness, pane.ssh_connection_handle())
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
            if let Some(node) = self.ssh_nodes.get_mut(&node_id)
                && node.readiness != readiness
            {
                if matches!(node.readiness, NodeReadiness::Ready)
                    && matches!(
                        readiness,
                        NodeReadiness::Error | NodeReadiness::Disconnected
                    )
                {
                    sessions_to_suspend.push(forwarding_session_id);
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
        if !sessions_to_suspend.is_empty() {
            let forwarding_registry = self.forwarding_registry.clone();
            let forwarding_runtime = self.forwarding_runtime.clone();
            forwarding_runtime.spawn(async move {
                for session_id in sessions_to_suspend {
                    let _ = forwarding_registry.suspend_session(&session_id).await;
                }
            });
        }
        for node_id in nodes_to_grace {
            self.schedule_grace_period_reconnect(&node_id);
        }
        let sessions_to_restore: Vec<_> = nodes_to_restore
            .into_iter()
            .filter_map(|node_id| {
                let node = self.ssh_nodes.get(&node_id)?;
                if self.node_router.node_state(&node_id).is_err() {
                    self.node_router
                        .upsert_node(node_id.clone(), node.config.clone());
                }
                let session_id = self.forwarding_session_id_for_node(&node_id);
                let consumer = ConnectionConsumer::PortForward(session_id.clone());
                let handle = self
                    .node_router
                    .acquire_connection(&node_id, consumer.clone())
                    .ok()?;
                Some((session_id, handle, consumer))
            })
            .collect();
        if !sessions_to_restore.is_empty() {
            for (session_id, handle, consumer) in sessions_to_restore.iter() {
                self.forwarding_connection_consumers.insert(
                    session_id.clone(),
                    (handle.connection_id.clone(), consumer.clone()),
                );
            }
            let forwarding_registry = self.forwarding_registry.clone();
            let forwarding_runtime = self.forwarding_runtime.clone();
            forwarding_runtime.spawn(async move {
                for (session_id, handle, _consumer) in sessions_to_restore {
                    let _ = forwarding_registry
                        .restore_session(session_id, handle.handle)
                        .await;
                }
            });
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

    pub(super) fn poll_reconnect_worker_results(&mut self, cx: &mut Context<Self>) {
        let mut results = Vec::new();
        while let Ok(result) = self.reconnect_worker_rx.try_recv() {
            results.push(result);
        }

        let mut changed = false;
        for result in results {
            match result {
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
                    let _ = self
                        .reconnect_orchestrator
                        .finish(&node_id.0, Err(detail.clone()));
                    if let Some(node) = self.ssh_nodes.get_mut(&node_id) {
                        node.readiness = NodeReadiness::Error;
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
                    && matches!(state, NodeReadiness::Error)
                    && reason.to_ascii_lowercase().contains("link")
                {
                    self.schedule_grace_period_reconnect(&node_id);
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
            .is_some_and(|seen| generation < *seen)
    }

    fn schedule_grace_period_reconnect(&mut self, node_id: &NodeId) {
        let Some(node) = self.ssh_nodes.get(node_id) else {
            return;
        };
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

        let snapshot = ReconnectSnapshot {
            old_terminal_session_ids: node
                .terminal_ids
                .iter()
                .map(|session_id| session_id.0.to_string())
                .collect(),
            old_connection_ids: vec![connection_id.clone()],
            ..ReconnectSnapshot::default()
        };
        let _ =
            self.reconnect_orchestrator
                .schedule(node_id.0.clone(), node.title.clone(), snapshot);
        let _ = self
            .reconnect_orchestrator
            .advance(&node_id.0, ReconnectPhase::Snapshot);
        let _ = self.reconnect_orchestrator.complete_phase(
            &node_id.0,
            PhaseResult::Ok,
            Some("captured native node snapshot".to_string()),
        );
        let _ = self
            .reconnect_orchestrator
            .advance(&node_id.0, ReconnectPhase::GracePeriod);

        let node_id = node_id.clone();
        let registry = self.ssh_registry.clone();
        let tx = self.reconnect_worker_tx.clone();
        let timing = self.reconnect_orchestrator.timing();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
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

    fn restore_forwarding_session_for_node(&mut self, node_id: &NodeId) {
        let session_id = self.forwarding_session_id_for_node(node_id);
        let consumer = ConnectionConsumer::PortForward(session_id.clone());
        let Ok(handle) = self
            .node_router
            .acquire_connection(node_id, consumer.clone())
        else {
            return;
        };
        self.forwarding_connection_consumers
            .insert(session_id.clone(), (handle.connection_id.clone(), consumer));
        let forwarding_registry = self.forwarding_registry.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let _ = forwarding_registry
                .restore_session(&session_id, handle.handle)
                .await;
        });
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
