impl WorkspaceApp {
    fn submit_forward_create(&mut self, tab_id: TabId, node_id: NodeId, cx: &mut Context<Self>) {
        let forward_type = self.forwarding_view.forward_type;
        let bind_port_value = self.forwarding_view.bind_port.clone();
        let target_port_value = self.forwarding_view.target_port.clone();
        let Some((bind_port, target_port)) =
            self.validate_forward_form(forward_type, &bind_port_value, &target_port_value)
        else {
            cx.notify();
            return;
        };
        let rule = match forward_type {
            ForwardType::Local => ForwardRule::local(
                self.forwarding_view.bind_address.clone(),
                bind_port,
                self.forwarding_view.target_host.clone(),
                target_port.unwrap_or(0),
            ),
            ForwardType::Remote => ForwardRule::remote(
                self.forwarding_view.bind_address.clone(),
                bind_port,
                self.forwarding_view.target_host.clone(),
                target_port.unwrap_or(0),
            ),
            ForwardType::Dynamic => ForwardRule {
                target_host: "0.0.0.0".to_string(),
                ..ForwardRule::dynamic(self.forwarding_view.bind_address.clone(), bind_port)
            },
        };
        let check_health = !self.forwarding_view.skip_health_check;
        let persist = self.forward_persist_context_for_node(&node_id);
        let registry = self.forwarding_registry.clone();
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.created",
            true,
            move |manager| {
                Box::pin(async move {
                    let created = manager
                        .create_forward_with_health_check(rule, check_health)
                        .await?;
                    if let Some((session_id, owner_connection_id)) = persist {
                        let forward_id = created.id.clone();
                        let _ = registry.sync_persisted_forward_rule(
                            &forward_id,
                            &session_id,
                            owner_connection_id,
                            created,
                        );
                    }
                    Ok(())
                })
            },
            cx,
        );
    }

    fn create_local_forward_for_detected_port(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        port: DetectedPort,
        cx: &mut Context<Self>,
    ) {
        let mut rule = ForwardRule::local(
            FORWARDS_DEFAULT_BIND_ADDRESS,
            port.port,
            FORWARDS_DEFAULT_TARGET_HOST,
            port.port,
        );
        rule.description = port
            .process_name
            .as_ref()
            .map(|process| format!("{process} ({})", self.i18n.t("forwards.detection.auto")))
            .unwrap_or_else(|| {
                format!(
                    "{} {} ({})",
                    self.i18n.t("forwards.detection.port"),
                    port.port,
                    self.i18n.t("forwards.detection.auto")
                )
            });
        self.dismiss_detected_port(port.port);
        let persist = self.forward_persist_context_for_node(&node_id);
        let registry = self.forwarding_registry.clone();
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.created",
            true,
            move |manager| {
                Box::pin(async move {
                    let created = manager.create_forward_with_health_check(rule, true).await?;
                    if let Some((session_id, owner_connection_id)) = persist {
                        let forward_id = created.id.clone();
                        let _ = registry.sync_persisted_forward_rule(
                            &forward_id,
                            &session_id,
                            owner_connection_id,
                            created,
                        );
                    }
                    Ok(())
                })
            },
            cx,
        );
    }

    fn dismiss_detected_port(&mut self, port: u16) {
        self.forwarding_view
            .new_ports
            .retain(|detected| detected.port != port);
        if let Some(tab_id) = self.active_tab_id
            && let Some(node_id) = self.forward_tab_nodes.get(&tab_id)
        {
            let connection_id = self.forwarding_connection_id_for_node(node_id);
            if let Some(state) = self.forwarding_port_detection_by_node.get_mut(node_id) {
                state.new_ports.retain(|detected| detected.port != port);
            }
            if let Some(connection_id) = connection_id {
                self.forwarding_registry
                    .ignore_detected_port(&connection_id, port);
            }
            if let Some(manager) = self.forwarding_manager_for_node_readonly(node_id) {
                manager.ignore_detected_port(port);
            }
        }
    }

    fn submit_forward_edit(&mut self, tab_id: TabId, node_id: NodeId, cx: &mut Context<Self>) {
        let Some(editing) = self.forwarding_view.editing_forward.clone() else {
            return;
        };
        let edit_bind_port = self.forwarding_view.edit_bind_port.clone();
        let edit_target_port = self.forwarding_view.edit_target_port.clone();
        let Some((bind_port, target_port)) =
            self.validate_forward_form(editing.forward_type, &edit_bind_port, &edit_target_port)
        else {
            cx.notify();
            return;
        };
        let update = ForwardUpdate {
            bind_address: Some(self.forwarding_view.edit_bind_address.clone()),
            bind_port: Some(bind_port),
            target_host: (editing.forward_type != ForwardType::Dynamic)
                .then(|| self.forwarding_view.edit_target_host.clone()),
            target_port,
            ..ForwardUpdate::default()
        };
        let forward_id = editing.id;
        let persist = self.forward_persist_context_for_node(&node_id);
        let registry = self.forwarding_registry.clone();
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.updated",
            true,
            move |manager| {
                Box::pin(async move {
                    let updated = manager.update_forward(&forward_id, update)?;
                    if let Some((session_id, owner_connection_id)) = persist {
                        let forward_id = updated.id.clone();
                        let _ = registry.sync_persisted_forward_rule(
                            &forward_id,
                            &session_id,
                            owner_connection_id,
                            updated,
                        );
                    }
                    Ok(())
                })
            },
            cx,
        );
    }

    fn validate_forward_form(
        &mut self,
        forward_type: ForwardType,
        bind_port: &str,
        target_port: &str,
    ) -> Option<(u16, Option<u16>)> {
        let Some(bind_port) = parse_port(bind_port) else {
            self.forwarding_view.error = Some(self.i18n.t(if bind_port.trim().is_empty() {
                "forwards.form.port_required"
            } else {
                "forwards.form.port_invalid"
            }));
            return None;
        };
        if forward_type == ForwardType::Dynamic {
            self.forwarding_view.error = None;
            return Some((bind_port, None));
        }
        let Some(target_port) = parse_port(target_port) else {
            self.forwarding_view.error = Some(self.i18n.t(if target_port.trim().is_empty() {
                "forwards.form.port_required"
            } else {
                "forwards.form.port_invalid"
            }));
            return None;
        };
        self.forwarding_view.error = None;
        Some((bind_port, Some(target_port)))
    }

    fn start_forward_operation<F>(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        message_key: &'static str,
        sync_saved_forwards_on_success: bool,
        operation: F,
        cx: &mut Context<Self>,
    ) where
        F: FnOnce(
                Arc<ForwardingManager>,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<(), oxideterm_forwarding::ForwardingError>,
                        > + Send,
                >,
            > + Send
            + 'static,
    {
        // Tauri gates ForwardsView work on nodeReady and its node_forwarding
        // commands require an existing forwarding manager; opening this surface
        // must not become an implicit SSH connect action.
        if !self.node_is_ready_for_forwarding(&node_id) {
            self.forwarding_view.error = Some(self.i18n.t("forwards.messages.node_not_ready"));
            cx.notify();
            return;
        }
        self.forwarding_view.pending = true;
        self.forwarding_view.error = None;
        let session_id = self.forwarding_session_id_for_node(&node_id);
        let owner_connection_id = self
            .ssh_nodes
            .get(&node_id)
            .and_then(|node| node.saved_connection_id.clone());
        let router = self.node_router.clone();
        let registry = self.forwarding_registry.clone();
        let tx = self.forwarding_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        thread::spawn(move || {
            let (binding, result) = match runtime.block_on(Self::forwarding_manager_for_node_async(
                router,
                registry,
                session_id,
                node_id.clone(),
                owner_connection_id,
            )) {
                Ok((manager, binding)) => {
                    let result = runtime
                        .block_on(operation(manager))
                        .map_err(|error| error.to_string());
                    (binding, result)
                }
                Err(error) => (None, Err(error)),
            };
            let _ = tx.send(ForwardingWorkerResult::Operation {
                tab_id,
                message_key,
                sync_saved_forwards_on_success,
                binding,
                result,
            });
        });
        cx.notify();
    }

    fn start_port_profiler_for_node(&mut self, node_id: NodeId, cx: &mut Context<Self>) {
        // Tauri starts a per-connection ResourceProfiler from usePortDetection
        // and leaves it running after the Forwards view unmounts. Native mirrors
        // that lifecycle with a node-owned scanner that emits PortScan results
        // independent of the currently active tab.
        self.forwarding_port_profiler_nodes.insert(node_id.clone());
        self.sync_forwarding_view_port_detection(&node_id);
        self.start_port_scan(node_id, true, cx);
    }

    pub(super) fn start_port_profiler_for_node_without_notify(&mut self, node_id: NodeId) {
        self.forwarding_port_profiler_nodes.insert(node_id.clone());
        self.sync_forwarding_view_port_detection(&node_id);
    }

    pub(super) fn maybe_start_forwards_port_scan(&mut self, cx: &mut Context<Self>) {
        let nodes = self
            .forwarding_port_profiler_nodes
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for node_id in nodes {
            let state = self
                .forwarding_port_detection_by_node
                .entry(node_id.clone())
                .or_default();
            if state.port_scan_pending {
                continue;
            }
            let due = state
                .last_port_scan_started
                .is_none_or(|last| last.elapsed() >= FORWARDS_PORT_SCAN_INTERVAL);
            if due {
                self.start_port_scan(node_id, false, cx);
            }
        }
    }

    pub(super) fn maybe_refresh_forwards_stats(&mut self, cx: &mut Context<Self>) {
        let Some(tab_id) = self.active_tab_id else {
            return;
        };
        if self
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .is_none_or(|tab| tab.kind != TabKind::Forwards)
        {
            return;
        }
        let due = self
            .forwarding_view
            .last_stats_refresh
            .is_none_or(|last| last.elapsed() >= FORWARDS_STATS_REFRESH_INTERVAL);
        if due {
            self.forwarding_view.last_stats_refresh = Some(Instant::now());
            cx.notify();
        }
    }

    fn start_port_scan(
        &mut self,
        node_id: NodeId,
        restart_degraded_profiler: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .forwarding_port_detection_by_node
            .get(&node_id)
            .is_some_and(|state| state.port_scan_pending)
        {
            return;
        }
        // Port detection follows the same nodeReady gate as Tauri's
        // usePortDetection hook. A restored Forwards tab should stay passive
        // until the user reconnects the node explicitly.
        if !self.node_is_ready_for_forwarding(&node_id) {
            self.forwarding_port_detection_by_node
                .entry(node_id.clone())
                .or_default()
                .port_scan_pending = false;
            self.sync_forwarding_view_port_detection(&node_id);
            cx.notify();
            return;
        }

        {
            let state = self
                .forwarding_port_detection_by_node
                .entry(node_id.clone())
                .or_default();
            state.port_scan_pending = true;
            state.port_scan_error = None;
            state.last_port_scan_started = Some(Instant::now());
        }
        self.sync_forwarding_view_port_detection(&node_id);
        let session_id = self.forwarding_session_id_for_node(&node_id);
        let owner_connection_id = self
            .ssh_nodes
            .get(&node_id)
            .and_then(|node| node.saved_connection_id.clone());
        let router = self.node_router.clone();
        let registry = self.forwarding_registry.clone();
        let tx = self.forwarding_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        thread::spawn(move || {
            let (connection_id, binding, result) = match runtime.block_on(
                Self::forwarding_manager_for_node_async(
                    router,
                    registry.clone(),
                    session_id,
                    node_id.clone(),
                    owner_connection_id,
                ),
            ) {
                Ok((manager, binding)) => {
                    let connection_id = binding
                        .as_ref()
                        .map(|(_, connection_id, _)| connection_id.clone());
                    let result = if let Some(connection_id) = connection_id.as_ref() {
                        if restart_degraded_profiler {
                            let _ = registry.restart_degraded_port_profiler(
                                connection_id.clone(),
                                manager.ssh_connection_handle(),
                            );
                        } else {
                            let _ = registry.start_port_profiler(
                                connection_id.clone(),
                                manager.ssh_connection_handle(),
                            );
                        }
                        Ok(registry
                            .detected_ports(connection_id)
                            .unwrap_or_default())
                    } else {
                        Err("node has no forwarding connection binding".to_string())
                    };
                    (connection_id, binding, result)
                }
                Err(error) => (None, None, Err(error)),
            };
            let _ = tx.send(ForwardingWorkerResult::PortScan {
                node_id,
                connection_id,
                binding,
                result,
            });
        });
        cx.notify();
    }

    fn node_is_ready_for_forwarding(&self, node_id: &NodeId) -> bool {
        self.ssh_nodes
            .get(node_id)
            .is_some_and(|node| node.readiness == NodeReadiness::Ready)
            && self
                .node_router
                .connection_id_for_node(node_id)
                .and_then(|connection_id| self.ssh_registry.get(&connection_id))
                .is_some_and(|handle| {
                    matches!(
                        handle.state(),
                        ConnectionState::Active | ConnectionState::Idle
                    )
                })
    }

    pub(super) fn poll_forwarding_worker_results(&mut self, cx: &mut Context<Self>) {
        let mut results = Vec::new();
        while let Ok(result) = self.forwarding_worker_rx.try_recv() {
            results.push(result);
        }
        for result in results {
            match result {
                ForwardingWorkerResult::Operation {
                    tab_id,
                    message_key,
                    sync_saved_forwards_on_success,
                    binding,
                    result,
                } => {
                    self.remember_forwarding_binding(binding);
                    if Some(tab_id) == self.active_tab_id {
                        self.forwarding_view.pending = false;
                        match result {
                            Ok(()) => {
                                let _ = message_key;
                                self.forwarding_view.error = None;
                                self.forwarding_view.show_new_form = false;
                                self.forwarding_view.skip_health_check = false;
                                self.forwarding_view.editing_forward = None;
                                self.forwarding_view.focused_input = None;
                                if sync_saved_forwards_on_success {
                                    // Tauri emits saved-forwards:update only
                                    // for persisted mutations. A temporary stop
                                    // preserves the saved rule and must not mark
                                    // cloud sync dirty by itself.
                                    self.queue_cloud_sync_dirty_refresh(cx);
                                }
                            }
                            Err(error) => self.forwarding_view.error = Some(error),
                        }
                        cx.notify();
                    }
                }
                ForwardingWorkerResult::Binding { binding } => {
                    self.remember_forwarding_binding(binding);
                    cx.notify();
                }
                ForwardingWorkerResult::PortScan {
                    node_id,
                    connection_id,
                    binding,
                    result,
                } => {
                    self.remember_forwarding_binding(binding);
                    self.apply_port_detection_result(&node_id, connection_id, result);
                    if self.active_forwards_tab_matches_node(&node_id) {
                        self.sync_forwarding_view_port_detection(&node_id);
                        cx.notify();
                    }
                }
            }
        }
    }

    pub(super) fn poll_forwarding_events(&mut self, cx: &mut Context<Self>) {
        let mut events = Vec::new();
        while let Ok(event) = self.forwarding_event_rx.try_recv() {
            events.push(event);
        }

        for event in events {
            match event {
                ForwardEvent::StatusChanged {
                    session_id,
                    status,
                    error,
                    ..
                } => {
                    if !self.active_forwards_tab_matches_session(&session_id) {
                        continue;
                    }
                    match status {
                        ForwardStatus::Suspended => {
                            let description = self.i18n.t("forwards.toast.suspended_desc");
                            self.push_forward_status_notice(
                                self.i18n.t("forwards.toast.suspended_title"),
                                Some(description),
                                TerminalNoticeVariant::Warning,
                            );
                        }
                        ForwardStatus::Error => {
                            self.push_forward_status_notice(
                                self.i18n.t("forwards.toast.error_title"),
                                error,
                                TerminalNoticeVariant::Error,
                            );
                        }
                        _ => {}
                    }
                    cx.notify();
                }
                ForwardEvent::StatsUpdated { session_id, .. } => {
                    if self.active_forwards_tab_matches_session(&session_id) {
                        cx.notify();
                    }
                }
                ForwardEvent::SessionSuspended {
                    session_id,
                    forward_ids,
                } => {
                    if !self.active_forwards_tab_matches_session(&session_id) {
                        continue;
                    }
                    // Tauri handles sessionSuspended as a toast-only runtime
                    // event. Keep inline form errors reserved for create/edit
                    // validation and operation failures.
                    self.push_forward_status_notice(
                        self.i18n.t("forwards.toast.session_suspended_title"),
                        Some(
                            self.i18n
                                .t("forwards.toast.session_suspended_desc")
                                .replace("{{count}}", &forward_ids.len().to_string()),
                        ),
                        TerminalNoticeVariant::Warning,
                    );
                    cx.notify();
                }
                ForwardEvent::PortDetected {
                    connection_id,
                    new_ports,
                    closed_ports,
                    all_ports,
                } => {
                    let Some(node_id) = self.forwarding_node_for_connection_id(&connection_id)
                    else {
                        continue;
                    };
                    self.apply_port_detection_result(
                        &node_id,
                        Some(connection_id),
                        Ok(PortDetectionSnapshot {
                            new_ports,
                            closed_ports,
                            all_ports,
                            has_scanned: true,
                        }),
                    );
                    if self.active_forwards_tab_matches_node(&node_id) {
                        self.sync_forwarding_view_port_detection(&node_id);
                        cx.notify();
                    }
                }
            }
        }
    }

    fn push_forward_status_notice(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
    ) {
        // Tauri's ForwardsView emits toast() for suspended/error status events
        // while keeping create-form failures inline. Mirror that split so bind
        // and remote-open classes remain visible without turning every failed
        // form submission into a workspace toast.
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title,
            description,
            status_text: None,
            progress: None,
            variant,
        });
    }

    fn active_forwards_tab_matches_session(&self, session_id: &str) -> bool {
        let Some(tab_id) = self.active_tab_id else {
            return false;
        };
        let Some(node_id) = self.forward_tab_nodes.get(&tab_id) else {
            return false;
        };
        self.forwarding_session_id_for_node(node_id) == session_id
    }

    fn active_forwards_tab_matches_node(&self, node_id: &NodeId) -> bool {
        let Some(tab_id) = self.active_tab_id else {
            return false;
        };
        self.forward_tab_nodes
            .get(&tab_id)
            .is_some_and(|active_node_id| active_node_id == node_id)
    }

    pub(super) fn forwarding_connection_id_for_node(&self, node_id: &NodeId) -> Option<String> {
        let session_id = self.forwarding_session_id_for_node(node_id);
        self.forwarding_connection_consumers
            .get(&session_id)
            .map(|(connection_id, _)| connection_id.clone())
    }

    fn forwarding_node_for_connection_id(&self, connection_id: &str) -> Option<NodeId> {
        self.forwarding_connection_consumers
            .iter()
            .find_map(|(session_id, (candidate_connection_id, _))| {
                if candidate_connection_id != connection_id {
                    return None;
                }
                session_id
                    .strip_prefix(FORWARDS_NODE_SESSION_PREFIX)
                    .map(|raw_node_id| NodeId(raw_node_id.to_string()))
            })
    }

    pub(super) fn release_forwarding_binding_for_node(
        &mut self,
        node_id: &NodeId,
    ) -> Option<String> {
        let session_id = self.forwarding_session_id_for_node(node_id);
        self.release_forwarding_binding_for_session(&session_id, Some(node_id))
    }

    fn release_forwarding_binding_for_session(
        &mut self,
        session_id: &str,
        node_id: Option<&NodeId>,
    ) -> Option<String> {
        let consumer = ConnectionConsumer::PortForward(session_id.to_string());
        let connection_id = if let Some((connection_id, stored_consumer)) =
            self.forwarding_connection_consumers.remove(session_id)
        {
            self.ssh_registry.release(&connection_id, &stored_consumer);
            Some(connection_id)
        } else if let Some(manager) = self.forwarding_registry.get(session_id) {
            // Manual disconnect in Tauri tears down every affected forward
            // manager even if the Forwards view is already gone. Native can see
            // that same lifecycle while an async worker has registered the
            // manager but its Binding result has not yet been polled into
            // forwarding_connection_consumers, so release the known
            // PortForward consumer from the manager's current node-owned
            // connection as a fallback.
            let connection_id = manager.ssh_connection_handle().connection_id().to_string();
            self.ssh_registry.release(&connection_id, &consumer);
            Some(connection_id)
        } else if let Some(connection_id) =
            node_id.and_then(|node_id| self.node_router.connection_id_for_node(node_id))
        {
            self.ssh_registry.release(&connection_id, &consumer);
            Some(connection_id)
        } else {
            None
        };

        if let Some(connection_id) = connection_id.as_ref() {
            self.forwarding_registry.stop_port_profiler(connection_id);
        }
        connection_id
    }

    fn apply_port_detection_result(
        &mut self,
        node_id: &NodeId,
        connection_id: Option<String>,
        result: Result<PortDetectionSnapshot, String>,
    ) {
        let state = self
            .forwarding_port_detection_by_node
            .entry(node_id.clone())
            .or_default();
        if connection_id.is_some() && state.connection_id != connection_id {
            // Tauri's usePortDetection hook is keyed by connectionId and clears
            // dismissed/new/all port state when reconnect swaps the connection.
            state.connection_id = connection_id;
            state.detected_ports.clear();
            state.new_ports.clear();
            state.has_scanned_ports = false;
            state.port_scan_error = None;
        }
        state.port_scan_pending = false;
        match result {
            Ok(snapshot) => {
                // Tauri's first ResourceProfiler sample establishes a silent
                // baseline; only later `port-detected:{connectionId}` events
                // add visible new ports. Keep the same UI-facing merge rule.
                state.has_scanned_ports = snapshot.has_scanned;
                state.detected_ports = snapshot.all_ports;
                if !snapshot.new_ports.is_empty() {
                    let existing: std::collections::HashSet<u16> =
                        state.new_ports.iter().map(|port| port.port).collect();
                    state.new_ports.extend(
                        snapshot
                            .new_ports
                            .into_iter()
                            .filter(|port| !existing.contains(&port.port)),
                    );
                }
                if !snapshot.closed_ports.is_empty() {
                    let closed: std::collections::HashSet<u16> = snapshot
                        .closed_ports
                        .iter()
                        .map(|port| port.port)
                        .collect();
                    state.new_ports.retain(|port| !closed.contains(&port.port));
                }
                state.port_scan_error = None;
            }
            Err(error) => {
                let _ = error;
                state.port_scan_error = None;
            }
        }
    }

    fn sync_forwarding_view_port_detection(&mut self, node_id: &NodeId) {
        let Some(state) = self.forwarding_port_detection_by_node.get(node_id) else {
            self.forwarding_view.detected_ports.clear();
            self.forwarding_view.new_ports.clear();
            self.forwarding_view.has_scanned_ports = false;
            self.forwarding_view.port_scan_pending = false;
            self.forwarding_view.port_scan_error = None;
            self.forwarding_view.last_port_scan_started = None;
            return;
        };
        self.forwarding_view.detected_ports = state.detected_ports.clone();
        self.forwarding_view.new_ports = state.new_ports.clone();
        self.forwarding_view.has_scanned_ports = state.has_scanned_ports;
        self.forwarding_view.port_scan_pending = state.port_scan_pending;
        self.forwarding_view.port_scan_error = state.port_scan_error.clone();
        self.forwarding_view.last_port_scan_started = state.last_port_scan_started;
    }

    fn forwarding_manager_for_node_readonly(
        &self,
        node_id: &NodeId,
    ) -> Option<Arc<ForwardingManager>> {
        self.forwarding_registry
            .get(&self.forwarding_session_id_for_node(node_id))
    }

    pub(super) fn remember_forwarding_binding(
        &mut self,
        binding: Option<(String, String, ConnectionConsumer)>,
    ) {
        if let Some((session_id, connection_id, consumer)) = binding {
            if !self.forwarding_binding_is_current(&session_id, &connection_id) {
                // Forwarding workers run off-thread. A manual disconnect can
                // remove the node-owned manager before the worker result is
                // polled; stale Binding results must release their registry
                // consumer instead of re-populating forwarding_connection_consumers
                // after Tauri-equivalent node teardown has completed.
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
                return;
            }
            if let Some((previous_connection_id, previous_consumer)) =
                self.forwarding_connection_consumers.get(&session_id)
                && (previous_connection_id != &connection_id || previous_consumer != &consumer)
            {
                // Forwarding is node-owned in the same sense as Tauri's
                // NodeRouter path: reconnect/restore must swap to the fresh
                // registry handle and release the old logical consumer instead
                // of keeping a reference to the last terminal-era transport.
                self.forwarding_registry
                    .stop_port_profiler(previous_connection_id);
                self.ssh_registry
                    .release(previous_connection_id, previous_consumer);
            }
            self.forwarding_connection_consumers
                .insert(session_id, (connection_id, consumer));
        }
    }

    fn forwarding_binding_is_current(&self, session_id: &str, connection_id: &str) -> bool {
        let manager_matches_connection = self
            .forwarding_registry
            .get(session_id)
            .is_some_and(|manager| manager.ssh_connection_handle().connection_id() == connection_id);
        if !manager_matches_connection {
            return false;
        }

        if let Some(raw_node_id) = session_id.strip_prefix(FORWARDS_NODE_SESSION_PREFIX) {
            let node_id = NodeId(raw_node_id.to_string());
            if self
                .ssh_nodes
                .get(&node_id)
                .is_some_and(|node| node.readiness == NodeReadiness::Disconnected)
            {
                return false;
            }
            return self
                .node_router
                .connection_id_for_node(&node_id)
                .is_some_and(|current_connection_id| current_connection_id == connection_id);
        }

        true
    }

    async fn forwarding_manager_for_node_async(
        router: NodeRouter,
        registry: ForwardingRegistry,
        session_id: String,
        node_id: NodeId,
        owner_connection_id: Option<String>,
    ) -> Result<
        (
            Arc<ForwardingManager>,
            Option<(String, String, ConnectionConsumer)>,
        ),
        String,
    > {
        let manager_existed = registry.get(&session_id).is_some();
        let consumer = ConnectionConsumer::PortForward(session_id.clone());
        let resolved = router
            .acquire_connection_wait(&node_id, consumer.clone(), Duration::from_secs(15))
            .await
            .map_err(|error| error.to_string())?;
        let connection_id = resolved.connection_id.clone();
        let (manager, _restored) = registry
            .register_or_rebind(session_id.clone(), resolved.handle)
            .await;

        // Existing managers may outlive a terminal pane. Always reacquire the
        // node-owned handle before scanning or mutating rules, matching Tauri
        // node_forwarding's pool-first owner. register_or_rebind also
        // suspends/restores active runners when a reconnect swapped the
        // connection id, so no forward path keeps liveness through an old
        // terminal/session handle.
        if let Some(owner_connection_id) = owner_connection_id.as_ref() {
            let _ = registry
                .saved_store()
                .map(|store| store.bind_owned_forwards_to_session(owner_connection_id, &session_id));
        }
        if manager_existed {
            return Ok((manager, Some((session_id, connection_id, consumer))));
        }

        let saved_forwards = if let Some(owner_connection_id) = owner_connection_id.as_ref() {
            registry.load_owned_forwards(owner_connection_id)
        } else {
            registry.load_persisted_forwards(&session_id)
        };
        let auto_start_rules: Vec<ForwardRule> = saved_forwards
            .into_iter()
            .filter(|forward| forward.auto_start)
            .map(|forward| forward.rule)
            .collect();
        for mut rule in auto_start_rules {
            rule.status = ForwardStatus::Starting;
            let _ = manager.create_forward(rule).await;
        }

        Ok((manager, Some((session_id, connection_id, consumer))))
    }

    fn forward_persist_context_for_node(
        &self,
        node_id: &NodeId,
    ) -> Option<(String, Option<String>)> {
        let node = self.ssh_nodes.get(node_id)?;
        Some((
            self.forwarding_session_id_for_node(node_id),
            node.saved_connection_id.clone(),
        ))
    }

    pub(super) fn forwarding_session_id_for_node(&self, node_id: &NodeId) -> String {
        format!("{FORWARDS_NODE_SESSION_PREFIX}{}", node_id.0)
    }

    fn open_forward_edit_form(&mut self, rule: ForwardRule, cx: &mut Context<Self>) {
        self.forwarding_view.edit_bind_address = rule.bind_address.clone();
        self.forwarding_view.edit_bind_port = rule.bind_port.to_string();
        self.forwarding_view.edit_target_host = rule.target_host.clone();
        self.forwarding_view.edit_target_port = rule.target_port.to_string();
        self.forwarding_view.editing_forward = Some(rule);
        self.forwarding_view.error = None;
        self.forwarding_view.focused_input = None;
        cx.notify();
    }

    pub(super) fn handle_forwards_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.forwarding_view.focused_input else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        match key {
            "escape" => {
                self.forwarding_view.focused_input = None;
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            "backspace" => {
                self.forward_input_value_mut(input).pop();
                self.forwarding_view.error = None;
                cx.notify();
                true
            }
            _ => false,
        }
    }

    pub(super) fn forward_input_value(&self, input: ForwardInput) -> &str {
        match input {
            ForwardInput::CreateBindAddress => &self.forwarding_view.bind_address,
            ForwardInput::CreateBindPort => &self.forwarding_view.bind_port,
            ForwardInput::CreateTargetHost => &self.forwarding_view.target_host,
            ForwardInput::CreateTargetPort => &self.forwarding_view.target_port,
            ForwardInput::EditBindAddress => &self.forwarding_view.edit_bind_address,
            ForwardInput::EditBindPort => &self.forwarding_view.edit_bind_port,
            ForwardInput::EditTargetHost => &self.forwarding_view.edit_target_host,
            ForwardInput::EditTargetPort => &self.forwarding_view.edit_target_port,
        }
    }

    pub(super) fn forward_input_value_mut(&mut self, input: ForwardInput) -> &mut String {
        match input {
            ForwardInput::CreateBindAddress => &mut self.forwarding_view.bind_address,
            ForwardInput::CreateBindPort => &mut self.forwarding_view.bind_port,
            ForwardInput::CreateTargetHost => &mut self.forwarding_view.target_host,
            ForwardInput::CreateTargetPort => &mut self.forwarding_view.target_port,
            ForwardInput::EditBindAddress => &mut self.forwarding_view.edit_bind_address,
            ForwardInput::EditBindPort => &mut self.forwarding_view.edit_bind_port,
            ForwardInput::EditTargetHost => &mut self.forwarding_view.edit_target_host,
            ForwardInput::EditTargetPort => &mut self.forwarding_view.edit_target_port,
        }
    }
}
