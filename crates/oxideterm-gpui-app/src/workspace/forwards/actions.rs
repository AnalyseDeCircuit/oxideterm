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
            ForwardType::Dynamic => {
                ForwardRule::dynamic(self.forwarding_view.bind_address.clone(), bind_port)
            }
        };
        let check_health = !self.forwarding_view.skip_health_check;
        let persist = self.forward_persist_context_for_node(&node_id);
        let registry = self.forwarding_registry.clone();
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.created",
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
            && let Some(manager) = self.forwarding_manager_for_node_readonly(node_id)
        {
            manager.ignore_detected_port(port);
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
        if !self.ensure_node_connection_started(&node_id) {
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
                node_id,
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
                binding,
                result,
            });
        });
        cx.notify();
    }

    fn stop_all_forward_rules(&mut self, tab_id: TabId, node_id: NodeId, cx: &mut Context<Self>) {
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.stopped_all",
            move |manager| {
                Box::pin(async move {
                    manager.stop_all().await;
                    Ok(())
                })
            },
            cx,
        );
    }

    fn suspend_all_forward_rules(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.suspended_all",
            move |manager| {
                Box::pin(async move {
                    // Mirrors Tauri pause_port_forwards: stopping listeners is
                    // not deletion. Rules move to Suspended so reconnect/restore
                    // can recreate them from the same manager owner.
                    manager.suspend_all_and_save_rules().await;
                    Ok(())
                })
            },
            cx,
        );
    }

    fn restore_suspended_forward_rules(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let session_id = self.forwarding_session_id_for_node(&node_id);
        let router = self.node_router.clone();
        self.start_forward_operation(
            tab_id,
            node_id.clone(),
            "forwards.messages.restored",
            move |manager| {
                Box::pin(async move {
                    let consumer = ConnectionConsumer::PortForward(session_id);
                    let handle = router
                        .acquire_connection_wait(&node_id, consumer, Duration::from_secs(15))
                        .await
                        .map_err(|error| oxideterm_forwarding::ForwardingError::Ssh(error.to_string()))?;
                    let results = manager.restore_saved_forwards(handle.handle).await;
                    if let Some(error) = results.into_iter().find_map(Result::err) {
                        return Err(error);
                    }
                    Ok(())
                })
            },
            cx,
        );
    }

    fn start_port_scan_for_forwards_tab(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        let Some(node_id) = self.forward_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        self.start_port_scan(tab_id, node_id, cx);
    }

    pub(super) fn maybe_start_forwards_port_scan(&mut self, cx: &mut Context<Self>) {
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
        if self.forwarding_view.port_scan_pending {
            return;
        }
        let due = self
            .forwarding_view
            .last_port_scan_started
            .is_none_or(|last| last.elapsed() >= FORWARDS_PORT_SCAN_INTERVAL);
        if due {
            self.start_port_scan_for_forwards_tab(tab_id, cx);
        }
    }

    fn start_port_scan(&mut self, tab_id: TabId, node_id: NodeId, cx: &mut Context<Self>) {
        if self.forwarding_view.port_scan_pending {
            return;
        }
        if !self.ensure_node_connection_started(&node_id) {
            self.forwarding_view.port_scan_error =
                Some(self.i18n.t("forwards.messages.node_not_ready"));
            self.forwarding_view.has_scanned_ports = true;
            cx.notify();
            return;
        }

        self.forwarding_view.port_scan_pending = true;
        self.forwarding_view.port_scan_error = None;
        self.forwarding_view.last_port_scan_started = Some(Instant::now());
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
                node_id,
                owner_connection_id,
            )) {
                Ok((manager, binding)) => {
                    let result = runtime
                        .block_on(manager.scan_remote_ports())
                        .map_err(|error| error.to_string());
                    (binding, result)
                }
                Err(error) => (None, Err(error)),
            };
            let _ = tx.send(ForwardingWorkerResult::PortScan {
                tab_id,
                binding,
                result,
            });
        });
        cx.notify();
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
                    tab_id,
                    binding,
                    result,
                } => {
                    self.remember_forwarding_binding(binding);
                    if Some(tab_id) == self.active_tab_id {
                        self.forwarding_view.port_scan_pending = false;
                        match result {
                            Ok(snapshot) => {
                                self.forwarding_view.has_scanned_ports = snapshot.has_scanned;
                                self.forwarding_view.detected_ports = snapshot.all_ports;
                                if !snapshot.new_ports.is_empty() {
                                    let existing: std::collections::HashSet<u16> = self
                                        .forwarding_view
                                        .new_ports
                                        .iter()
                                        .map(|port| port.port)
                                        .collect();
                                    self.forwarding_view.new_ports.extend(
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
                                    self.forwarding_view
                                        .new_ports
                                        .retain(|port| !closed.contains(&port.port));
                                }
                                self.forwarding_view.port_scan_error = None;
                            }
                            Err(error) => {
                                self.forwarding_view.has_scanned_ports = true;
                                self.forwarding_view.port_scan_error = Some(error);
                            }
                        }
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
                            self.forwarding_view.error =
                                Some(self.i18n.t("forwards.toast.suspended_desc"));
                        }
                        ForwardStatus::Error => {
                            self.forwarding_view.error = error;
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
                    self.forwarding_view.error = Some(
                        self.i18n
                            .t("forwards.toast.session_suspended_desc")
                            .replace("{{count}}", &forward_ids.len().to_string()),
                    );
                    cx.notify();
                }
            }
        }
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

    fn forwarding_manager_for_node_readonly(
        &self,
        node_id: &NodeId,
    ) -> Option<Arc<ForwardingManager>> {
        self.forwarding_registry
            .get(&self.forwarding_session_id_for_node(node_id))
    }

    fn remember_forwarding_binding(
        &mut self,
        binding: Option<(String, String, ConnectionConsumer)>,
    ) {
        if let Some((session_id, connection_id, consumer)) = binding {
            self.forwarding_connection_consumers
                .insert(session_id, (connection_id, consumer));
        }
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
        let manager = registry.register(session_id.clone(), resolved.handle);

        // Existing managers may outlive a terminal pane. Always reacquire the
        // node-owned handle and replace the manager connection before scanning
        // or mutating rules, matching Tauri node_forwarding's pool-first owner.
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
