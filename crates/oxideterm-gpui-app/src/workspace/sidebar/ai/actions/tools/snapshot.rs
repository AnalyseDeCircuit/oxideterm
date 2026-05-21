impl WorkspaceApp {
    fn ai_orchestrator_snapshot(&self, cx: &mut Context<Self>) -> AiOrchestratorRuntimeSnapshot {
        let mut targets = Vec::new();
        for connection in self.connection_store.connections() {
            let mut refs = BTreeMap::new();
            refs.insert("connectionId".to_string(), connection.id.clone());
            targets.push(AiOrchestratorTarget {
                id: format!("saved-connection:{}", connection.id),
                kind: "saved-connection".to_string(),
                label: format!(
                    "{} ({}@{}:{})",
                    connection.name, connection.username, connection.host, connection.port
                ),
                state: "available".to_string(),
                capabilities: vec!["navigation.open".to_string(), "state.list".to_string()],
                refs,
                metadata: serde_json::json!({
                    "host": connection.host,
                    "port": connection.port,
                    "username": connection.username,
                    "name": connection.name,
                    "group": connection.group,
                }),
                terminal_buffer: None,
                ssh_handle: None,
            });
        }

        for tab in &self.tabs {
            let mut refs = BTreeMap::new();
            refs.insert("tabId".to_string(), tab.id.0.to_string());
            targets.push(AiOrchestratorTarget {
                id: format!("app-surface:{}:{}", ai_tab_kind_label(&tab.kind), tab.id.0),
                kind: "app-surface".to_string(),
                label: tab.title.clone(),
                state: if Some(tab.id) == self.active_tab_id {
                    "connected"
                } else {
                    "available"
                }
                .to_string(),
                capabilities: vec!["navigation.open".to_string(), "state.list".to_string()],
                refs,
                metadata: serde_json::json!({ "tabType": ai_tab_kind_label(&tab.kind) }),
                terminal_buffer: None,
                ssh_handle: None,
            });
        }

        for (node_id, node) in &self.ssh_nodes {
            let terminal_id = node.terminal_ids.first().copied();
            let ssh_handle = self
                .node_router
                .resolve_connection_now(node_id)
                .ok()
                .map(|resolved| resolved.handle)
                .or_else(|| {
                    terminal_id
                        .and_then(|session_id| self.terminal_endpoint_sessions.get(&session_id))
                        .and_then(|endpoint| endpoint.session.lock().ssh_connection_handle())
                });
            let mut refs = BTreeMap::new();
            refs.insert("nodeId".to_string(), node_id.0.clone());
            if let Some(saved_connection_id) = node.saved_connection_id.as_ref() {
                refs.insert("connectionId".to_string(), saved_connection_id.clone());
            }
            if let Some(session_id) = terminal_id {
                refs.insert("sessionId".to_string(), session_id.0.to_string());
            }
            targets.push(AiOrchestratorTarget {
                id: format!("ssh-node:{}", node_id.0),
                kind: "ssh-node".to_string(),
                label: format!(
                    "{}@{}:{}",
                    node.config.username, node.config.host, node.config.port
                ),
                state: match node.readiness {
                    NodeReadiness::Ready => "connected",
                    NodeReadiness::Connecting => "opening",
                    NodeReadiness::Error => "stale",
                    NodeReadiness::Disconnected => "unavailable",
                }
                .to_string(),
                capabilities: vec![
                    "command.run".to_string(),
                    "filesystem.read".to_string(),
                    "filesystem.write".to_string(),
                    "state.list".to_string(),
                    "navigation.open".to_string(),
                ],
                refs,
                metadata: serde_json::json!({
                    "host": node.config.host,
                    "port": node.config.port,
                    "username": node.config.username,
                    "terminalIds": node.terminal_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
                    "title": node.title,
                }),
                terminal_buffer: None,
                ssh_handle,
            });
        }

        for tab in &self.tabs {
            let Some(root) = tab.root_pane.as_ref() else {
                continue;
            };
            let mut pane_ids = Vec::new();
            root.collect_pane_ids(&mut pane_ids);
            for pane_id in pane_ids {
                let Some(session_id) = root.session_id_for_pane(pane_id) else {
                    continue;
                };
                let Some(pane) = self.panes.get(&pane_id) else {
                    continue;
                };
                let terminal_type = if tab.kind == TabKind::LocalTerminal {
                    "local_terminal"
                } else {
                    "ssh_terminal"
                };
                let mut refs = BTreeMap::new();
                refs.insert("sessionId".to_string(), session_id.0.to_string());
                refs.insert("tabId".to_string(), tab.id.0.to_string());
                if let Some(node_id) = self.terminal_ssh_nodes.get(&session_id) {
                    refs.insert("nodeId".to_string(), node_id.0.clone());
                    if let Some(connection_id) = self
                        .ssh_nodes
                        .get(node_id)
                        .and_then(|node| node.saved_connection_id.clone())
                    {
                        refs.insert("connectionId".to_string(), connection_id);
                    }
                }
                let terminal_buffer = pane.read(cx).visible_text_snapshot();
                targets.push(AiOrchestratorTarget {
                    id: format!("terminal-session:{}", session_id.0),
                    kind: "terminal-session".to_string(),
                    label: format!("{} {}", if tab.kind == TabKind::LocalTerminal { "Local terminal" } else { "SSH terminal" }, session_id.0),
                    state: "connected".to_string(),
                    capabilities: vec![
                        "terminal.observe".to_string(),
                        "terminal.send".to_string(),
                        "terminal.wait".to_string(),
                        "state.list".to_string(),
                    ],
                    refs,
                    metadata: serde_json::json!({
                        "paneId": pane_id.0,
                        "terminalType": terminal_type,
                    }),
                    terminal_buffer: Some(terminal_buffer),
                    ssh_handle: None,
                });
            }
        }

        targets.push(AiOrchestratorTarget {
            id: "local-shell:default".to_string(),
            kind: "local-shell".to_string(),
            label: "Local shell".to_string(),
            state: "available".to_string(),
            capabilities: vec![
                "command.run".to_string(),
                "navigation.open".to_string(),
                "state.list".to_string(),
            ],
            refs: BTreeMap::new(),
            metadata: serde_json::json!({}),
            terminal_buffer: None,
            ssh_handle: None,
        });
        targets.push(AiOrchestratorTarget {
            id: "settings:app".to_string(),
            kind: "settings".to_string(),
            label: "Settings".to_string(),
            state: "available".to_string(),
            capabilities: vec![
                "settings.read".to_string(),
                "settings.write".to_string(),
                "navigation.open".to_string(),
                "state.list".to_string(),
            ],
            refs: BTreeMap::new(),
            metadata: serde_json::json!({}),
            terminal_buffer: None,
            ssh_handle: None,
        });
        targets.push(AiOrchestratorTarget {
            id: "rag-index:default".to_string(),
            kind: "rag-index".to_string(),
            label: "Knowledge base".to_string(),
            state: "available".to_string(),
            capabilities: vec!["state.list".to_string(), "filesystem.search".to_string()],
            refs: BTreeMap::new(),
            metadata: serde_json::json!({}),
            terminal_buffer: None,
            ssh_handle: None,
        });

        let settings = self.settings_store.settings();
        AiOrchestratorRuntimeSnapshot {
            targets,
            memory: settings.ai.memory.content.clone(),
            node_router: self.node_router.clone(),
            sftp_transfer_manager: self.sftp_transfer_manager.clone(),
            agent_fs: self.ai_agent_fs.clone(),
            backend_runtime: self.forwarding_runtime.clone(),
            mcp_registry: self.ai_mcp_registry.clone(),
            rag_store: self.ai_rag_store.clone(),
            ai_key_store: self.ai_key_store.clone(),
            ai_providers: settings.ai.providers.clone(),
            ai_embedding_config: settings.ai.embedding_config.clone(),
            ai_context_window: AI_COMPACTION_DEFAULT_CONTEXT_WINDOW,
            runtime_epoch: self.ai_runtime_epoch.clone(),
            settings_summary: serde_json::json!({
                "ai": {
                    "enabled": settings.ai.enabled,
                    "activeProviderId": settings.ai.active_provider_id,
                    "activeModel": settings.ai.active_model,
                    "toolUse": {
                        "enabled": settings.ai.tool_use.enabled,
                        "maxRounds": settings.ai.tool_use.max_rounds,
                        "maxCallsPerRound": settings.ai.tool_use.max_calls_per_round,
                        "autoApproveTools": settings.ai.tool_use.auto_approve_tools,
                        "disabledTools": settings.ai.tool_use.disabled_tools,
                    },
                    "memory": {
                        "enabled": settings.ai.memory.enabled,
                        "contentLength": settings.ai.memory.content.len(),
                    },
                    "mcpServers": self.ai_mcp_registry.snapshots(),
                },
                "tabs": self.tabs.len(),
                "terminalSessions": self.panes.len(),
            }),
        }
    }

    fn ai_chat_orchestrator_snapshot(
        &self,
        config: &AiChatStreamConfig,
        cx: &mut Context<Self>,
    ) -> AiOrchestratorRuntimeSnapshot {
        let mut snapshot = self.ai_orchestrator_snapshot(cx);
        snapshot.ai_context_window = self.ai_active_model_context_window(config);
        snapshot
    }

    fn resolve_ai_tool_approval(&mut self, tool_call_id: String, approved: bool, cx: &mut Context<Self>) {
        if let Some(sender) = self.ai_pending_tool_approvals.remove(&tool_call_id) {
            let _ = sender.send(approved);
        }
        cx.notify();
    }

    fn execute_ai_ui_orchestrator_tool(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiExecutedToolResult {
        let started = std::time::Instant::now();
        let result = match tool_name.as_str() {
            "connect_target" => self.execute_ai_connect_target(&args, window, cx),
            "run_command" => self.execute_ai_terminal_run_command(&args, cx),
            "send_terminal_input" => self.execute_ai_send_terminal_input(&args, cx),
            "write_resource" => self.execute_ai_write_settings_resource(&args, cx),
            "open_app_surface" => self.execute_ai_open_app_surface(&args, window, cx),
            "remember_preference" => self.execute_ai_remember_preference(&args, cx),
            _ => self.ai_orchestrator_snapshot(cx).fail(
                "Unknown tool.",
                "unknown_tool",
                format!("Tool {tool_name} is not available."),
                "read",
            ),
        };
        self.ai_orchestrator_snapshot(cx).to_executed_tool_result(
            tool_call_id,
            tool_name,
            result,
            started.elapsed().as_millis(),
        )
    }

    fn start_ai_ui_orchestrator_tool_execution(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if tool_name == "connect_target" {
            self.start_ai_connect_target_execution(tool_call_id, tool_name, args, sender, window, cx);
            return;
        }
        if tool_name == "run_command"
            && self
                .ai_orchestrator_snapshot(cx)
                .target_kind_for_args(&args)
                .as_deref()
                == Some("terminal-session")
        {
            self.start_ai_terminal_run_command_execution(tool_call_id, tool_name, args, sender, cx);
            return;
        }
        let result = self.execute_ai_ui_orchestrator_tool(tool_call_id, tool_name, args, window, cx);
        let _ = sender.send(result);
    }

    fn start_ai_connect_target_execution(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let started = std::time::Instant::now();
        let base = self.execute_ai_ui_orchestrator_tool(
            tool_call_id.clone(),
            tool_name.clone(),
            args.clone(),
            window,
            cx,
        );
        if !base.success {
            let _ = sender.send(base);
            return;
        }
        if let Some(ready) = self.ai_connect_target_ready_result(
            &tool_call_id,
            &tool_name,
            &args,
            started.elapsed().as_millis(),
            cx,
        ) {
            let _ = sender.send(ready);
            return;
        }
        cx.spawn(async move |weak, cx| {
            let mut sender = Some(sender);
            for _ in 0..50 {
                Timer::after(Duration::from_millis(100)).await;
                let ready = weak.update(cx, |this, cx| {
                    this.ai_connect_target_ready_result(
                        &tool_call_id,
                        &tool_name,
                        &args,
                        started.elapsed().as_millis(),
                        cx,
                    )
                });
                match ready {
                    Ok(Some(result)) => {
                        if let Some(sender) = sender.take() {
                            let _ = sender.send(result);
                        }
                        return;
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
            if let Some(sender) = sender.take() {
                let result = weak.update(cx, |this, cx| {
                    this.ai_connect_target_timeout_result(
                        &tool_call_id,
                        &tool_name,
                        &args,
                        &base,
                        started.elapsed().as_millis(),
                        cx,
                    )
                });
                let _ = sender.send(result.unwrap_or(base));
            }
        })
        .detach();
    }

    fn execute_ai_connect_target(
        &mut self,
        args: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        if args.get("control").and_then(serde_json::Value::as_str).is_some() {
            return snapshot.fail(
                "Terminal control input is not available through this tool.",
                "terminal_control_disabled",
                "Use run_command to execute shell commands. send_terminal_input only sends literal interactive text or Enter after observing a prompt.",
                "interactive",
            );
        }
        let Some(target_id) = args.get("target_id").and_then(serde_json::Value::as_str) else {
            return snapshot.fail(
                "Target is required.",
                "missing_target_id",
                "connect_target requires target_id.",
                "write",
            );
        };
        let Some(target) = snapshot.targets.iter().find(|target| target.id == target_id).cloned()
        else {
            return snapshot.fail(
                "Target not found.",
                "target_not_found",
                format!("Target not found: {target_id}"),
                "write",
            );
        };

        match target.kind.as_str() {
            "terminal-session" => {
                if let Some(session_id) = target
                    .refs
                    .get("sessionId")
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(TerminalSessionId)
                    && self.focus_terminal_session(session_id, window, cx)
                {
                    return snapshot
                        .ok("Target is already live.", "", target_json(&target), "write")
                        .with_target(target);
                }
                snapshot
                    .fail(
                        "Terminal target is not ready.",
                        "terminal_pane_missing",
                        "No visible pane is registered for this terminal session.",
                        "write",
                    )
                    .with_target(target)
            }
            "ssh-node" => {
                let Some(node_id) = target.refs.get("nodeId").map(|value| NodeId::new(value.clone()))
                else {
                    return snapshot
                        .fail(
                            "SSH target is missing nodeId.",
                            "missing_node_id",
                            "The selected SSH target cannot be reconnected without a node id.",
                            "write",
                        )
                        .with_target(target);
                };
                if let Some(session_id) = self
                    .ssh_nodes
                    .get(&node_id)
                    .and_then(|node| node.terminal_ids.first().copied())
                    && self.focus_terminal_session(session_id, window, cx)
                {
                    let data = serde_json::json!({
                        "nodeId": node_id.0,
                        "sessionId": session_id.0.to_string(),
                        "connectionId": target.refs.get("connectionId").cloned().unwrap_or_default(),
                    });
                    return self
                        .ai_orchestrator_snapshot(cx)
                        .ok(
                            "Target is already live.",
                            "",
                            data,
                            "write",
                        )
                        .with_target(target);
                }
                let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
                    return snapshot
                        .fail(
                            "SSH target is missing.",
                            "missing_node",
                            format!("No SSH node exists for {}.", node_id.0),
                            "write",
                        )
                        .with_target(target);
                };
                let saved_connection_id = node.saved_connection_id.clone();
                match self.queue_ssh_terminal_tab_for_node(
                    node_id.clone(),
                    node.config,
                    node.title,
                    saved_connection_id,
                    window,
                    cx,
                ) {
                    Ok(()) => {
                        let refreshed = self.ai_orchestrator_snapshot(cx);
                        let targets = refreshed
                            .targets
                            .iter()
                            .filter(|candidate| {
                                candidate.id == target.id
                                    || candidate.refs.get("nodeId") == Some(&node_id.0)
                            })
                            .cloned()
                            .collect::<Vec<_>>();
                        refreshed
                            .ok(
                                format!("Connection requested for {}.", target.label),
                                format!("Connection requested for {}.", target.id),
                                serde_json::json!({ "nodeId": node_id.0 }),
                                "write",
                            )
                            .with_target(target)
                            .with_targets(targets)
                    }
                    Err(error) => snapshot
                        .fail(
                            "SSH target reconnect failed.",
                            "ssh_reconnect_failed",
                            error.to_string(),
                            "write",
                        )
                        .with_target(target),
                }
            }
            "saved-connection" => {
                let Some(connection_id) = target.refs.get("connectionId").cloned() else {
                    return snapshot
                        .fail(
                            "Saved connection is missing connectionId.",
                            "missing_connection_id",
                            "The selected saved connection has no connection id.",
                            "write",
                        )
                        .with_target(target);
                };
                let Some(connection) = self.connection_store.get(&connection_id).cloned() else {
                    return snapshot
                        .fail(
                            "Saved connection not found.",
                            "saved_connection_not_found",
                            format!("No saved connection exists for {connection_id}."),
                            "write",
                        )
                        .with_target(target);
                };
                let Some(config) = crate::workspace::session_manager::ssh_config_from_saved_connection(
                    &self.connection_store,
                    &connection,
                ) else {
                    return snapshot
                        .fail(
                            "Saved connection cannot be materialized.",
                            "saved_connection_invalid",
                            "The saved connection is missing SSH configuration or credentials.",
                            "write",
                        )
                        .with_target(target);
                };
                let title = if connection.name.trim().is_empty() {
                    format!("{}@{}", connection.username, connection.host)
                } else {
                    connection.name.clone()
                };
                match self.open_or_create_saved_ssh_terminal_tab(
                    connection_id.clone(),
                    config,
                    title,
                    window,
                    cx,
                ) {
                    Ok(()) => {
                        let refreshed = self.ai_orchestrator_snapshot(cx);
                        let targets = refreshed
                            .targets
                            .iter()
                            .filter(|candidate| {
                                candidate.refs.get("connectionId") == Some(&connection_id)
                            })
                            .cloned()
                            .collect::<Vec<_>>();
                        let output = targets
                            .iter()
                            .map(|target| format!("{} — {}", target.id, target.label))
                            .collect::<Vec<_>>()
                            .join("\n");
                        refreshed
                            .ok(
                                format!("Connected {}.", target.label),
                                if output.is_empty() {
                                    format!("Connection requested for {connection_id}.")
                                } else {
                                    output
                                },
                                serde_json::json!({ "connectionId": connection_id }),
                                "write",
                            )
                            .with_target(target)
                            .with_targets(targets)
                    }
                    Err(error) => snapshot
                        .fail("Connection failed.", "connect_error", error.to_string(), "write")
                        .with_target(target),
                }
            }
            _ => snapshot
                .fail(
                    "Target cannot be connected as SSH.",
                    "unsupported_connect_target",
                    format!("{} is not a saved SSH connection.", target.kind),
                    "write",
                )
                .with_target(target),
        }
    }

    fn execute_ai_send_terminal_input(
        &mut self,
        args: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let Some(target_id) = args.get("target_id").and_then(serde_json::Value::as_str) else {
            return snapshot.fail(
                "Target is required.",
                "missing_target_id",
                "send_terminal_input requires target_id.",
                "interactive",
            );
        };
        let Some(target) = snapshot.targets.iter().find(|target| target.id == target_id).cloned()
        else {
            return snapshot.fail(
                "Target not found.",
                "target_not_found",
                format!("Target not found: {target_id}"),
                "interactive",
            );
        };
        let Some(session_id) = target
            .refs
            .get("sessionId")
            .and_then(|value| value.parse::<u64>().ok())
            .map(TerminalSessionId)
        else {
            return snapshot
                .fail(
                    "Terminal target is missing sessionId.",
                    "missing_session_id",
                    "send_terminal_input requires a terminal-session target.",
                    "interactive",
                )
                .with_target(target);
        };
        let Some((pane_id, pane)) = self.pane_for_terminal_session(session_id) else {
            return snapshot
                .fail(
                    "Terminal pane is not registered.",
                    "terminal_pane_missing",
                    "No visible pane is registered for this terminal session.",
                    "interactive",
                )
                .with_target(target);
        };
        let payload = ai_terminal_input_payload(args);
        if payload.is_empty() {
            return snapshot
                .fail(
                    "No terminal input specified.",
                    "missing_terminal_input",
                    "Provide text or a supported control sequence.",
                    "interactive",
                )
                .with_target(target);
        }
        pane.update(cx, |pane, cx| {
            pane.send_ai_input_bytes(payload.as_bytes(), cx);
        });
        snapshot
            .ok(
                "Terminal input sent.",
                "Input sent.",
                serde_json::json!({ "sessionId": session_id.0, "paneId": pane_id.0 }),
                "interactive",
            )
            .with_target(target)
    }

    fn execute_ai_terminal_run_command(
        &mut self,
        args: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let Some(target_id) = args.get("target_id").and_then(serde_json::Value::as_str) else {
            return snapshot.fail(
                "Target is required.",
                "missing_target_id",
                "run_command requires target_id.",
                "interactive",
            );
        };
        let Some(command) = args
            .get("command")
            .and_then(serde_json::Value::as_str)
            .filter(|command| !command.trim().is_empty())
        else {
            return snapshot.fail(
                "Command is required.",
                "missing_command",
                "run_command requires a command.",
                "interactive",
            );
        };
        let Some(target) = snapshot.targets.iter().find(|target| target.id == target_id).cloned()
        else {
            return snapshot.fail(
                "Target not found.",
                "target_not_found",
                format!("Target not found: {target_id}"),
                "interactive",
            );
        };
        let Some(session_id) = target
            .refs
            .get("sessionId")
            .and_then(|value| value.parse::<u64>().ok())
            .map(TerminalSessionId)
        else {
            return snapshot
                .fail(
                    "Terminal target is missing sessionId.",
                    "missing_session_id",
                    "Target cannot receive terminal input without sessionId.",
                    "interactive",
                )
                .with_target(target);
        };
        let Some((pane_id, pane)) = self.pane_for_terminal_session(session_id) else {
            return snapshot
                .fail(
                    "Terminal pane is not ready.",
                    "terminal_pane_missing",
                    "The visible terminal pane is not registered yet.",
                    "interactive",
                )
                .with_target(target);
        };
        let before = pane.read(cx).visible_text_snapshot();
        pane.update(cx, |pane, cx| {
            pane.begin_command_mark(command, TerminalCommandMarkDetectionSource::Ai, cx);
            pane.send_command_line(command, cx);
        });
        if args
            .get("await_output")
            .and_then(serde_json::Value::as_bool)
            == Some(false)
        {
            return snapshot
                .ok(
                    "Command sent to terminal.",
                    format!("Command sent: {command}"),
                    serde_json::json!({ "sessionId": session_id.0, "paneId": pane_id.0 }),
                    "interactive",
                )
                .with_target(target);
        }
        let after = pane.read(cx).visible_text_snapshot();
        let output = terminal_delta_output(&before, &after);
        snapshot
            .ok(
                "Command sent to terminal.",
                if output.trim().is_empty() {
                    format!("Command sent: {command}")
                } else {
                    output
                },
                serde_json::json!({
                    "sessionId": session_id.0,
                    "paneId": pane_id.0,
                    "waitingForInput": looks_waiting_for_input(&after),
                }),
                "interactive",
            )
            .with_target(target)
    }

    fn start_ai_terminal_run_command_execution(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        cx: &mut Context<Self>,
    ) {
        let started = std::time::Instant::now();
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let Some(target_id) = args.get("target_id").and_then(serde_json::Value::as_str) else {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot.fail(
                    "Target is required.",
                    "missing_target_id",
                    "run_command requires target_id.",
                    "interactive",
                ),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        };
        let Some(command) = args
            .get("command")
            .and_then(serde_json::Value::as_str)
            .filter(|command| !command.trim().is_empty())
            .map(str::to_string)
        else {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot.fail(
                    "Command is required.",
                    "missing_command",
                    "run_command requires a command.",
                    "interactive",
                ),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        };
        let Some(target) = snapshot.targets.iter().find(|target| target.id == target_id).cloned()
        else {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot.fail(
                    "Target not found.",
                    "target_not_found",
                    format!("Target not found: {target_id}"),
                    "interactive",
                ),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        };
        let Some(session_id) = target
            .refs
            .get("sessionId")
            .and_then(|value| value.parse::<u64>().ok())
            .map(TerminalSessionId)
        else {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot
                    .fail(
                        "Terminal target is missing sessionId.",
                        "missing_session_id",
                        "Target cannot receive terminal input without sessionId.",
                        "interactive",
                    )
                    .with_target(target),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        };
        let Some((pane_id, pane)) = self.pane_for_terminal_session(session_id) else {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot
                    .fail(
                        "Terminal pane is not ready.",
                        "terminal_pane_missing",
                        "The visible terminal pane is not registered yet.",
                        "interactive",
                    )
                    .with_target(target),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        };
        let before = pane.read(cx).visible_text_snapshot();
        pane.update(cx, |pane, cx| {
            pane.begin_command_mark(&command, TerminalCommandMarkDetectionSource::Ai, cx);
            pane.send_command_line(&command, cx);
        });
        if args
            .get("await_output")
            .and_then(serde_json::Value::as_bool)
            == Some(false)
        {
            let result = self.ai_orchestrator_snapshot(cx).to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot
                    .ok(
                        "Command sent to terminal.",
                        format!("Command sent: {command}"),
                        serde_json::json!({ "sessionId": session_id.0, "paneId": pane_id.0 }),
                        "interactive",
                    )
                    .with_target(target),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        }

        cx.spawn(async move |weak, cx| {
            let mut sender = Some(sender);
            let mut last = before.clone();
            let mut changed_at = std::time::Instant::now();
            for _ in 0..300 {
                Timer::after(Duration::from_millis(100)).await;
                let current = weak.update(cx, |_this, cx| pane.read(cx).visible_text_snapshot());
                let current = match current {
                    Ok(current) => current,
                    Err(_) => break,
                };
                if current != last {
                    last = current.clone();
                    changed_at = std::time::Instant::now();
                }
                if current != before && changed_at.elapsed() >= Duration::from_millis(400) {
                    let result = weak.update(cx, |this, cx| {
                        let snapshot = this.ai_orchestrator_snapshot(cx);
                        snapshot.to_executed_tool_result(
                            tool_call_id.clone(),
                            tool_name.clone(),
                            snapshot
                                .ok(
                                    "Terminal command output captured.",
                                    terminal_delta_output(&before, &current),
                                    serde_json::json!({
                                        "sessionId": session_id.0,
                                        "paneId": pane_id.0,
                                        "waitingForInput": looks_waiting_for_input(&current),
                                    }),
                                    "interactive",
                                )
                                .with_target(target.clone()),
                            started.elapsed().as_millis(),
                        )
                    });
                    if let (Some(sender), Ok(result)) = (sender.take(), result) {
                        let _ = sender.send(result);
                    }
                    return;
                }
            }
            let result = weak.update(cx, |this, cx| {
                let snapshot = this.ai_orchestrator_snapshot(cx);
                let output = terminal_delta_output(&before, &last);
                let output_empty = output.trim().is_empty();
                snapshot.to_executed_tool_result(
                    tool_call_id,
                    tool_name,
                    AiActionResultLite {
                        ok: !output_empty,
                        summary: if output_empty {
                            "Terminal command did not produce completed output.".to_string()
                        } else {
                            "Terminal command output captured.".to_string()
                        },
                        output: if output_empty {
                            "No new output after 30s. The command may be waiting for input or still running.".to_string()
                        } else {
                            output
                        },
                        data: serde_json::json!({
                            "sessionId": session_id.0,
                            "paneId": pane_id.0,
                            "waitingForInput": looks_waiting_for_input(&last),
                            "timedOut": true,
                        }),
                        error_code: output_empty.then(|| "terminal_command_wait_timeout".to_string()),
                        error_message: output_empty.then(|| "No new output after 30s. The command may be waiting for input or still running.".to_string()),
                        risk: "interactive",
                        target: Some(target),
                        targets: Vec::new(),
                        state_version: None,
                    },
                    started.elapsed().as_millis(),
                )
            });
            if let (Some(sender), Ok(result)) = (sender.take(), result) {
                let _ = sender.send(result);
            }
        })
        .detach();
    }

    fn execute_ai_write_settings_resource(
        &mut self,
        args: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        if args.get("resource").and_then(serde_json::Value::as_str) != Some("settings") {
            return snapshot.fail(
                "Unsupported resource write.",
                "unsupported_resource_write",
                "The UI settings executor only handles write_resource(settings).",
                "write",
            );
        }
        let Some(target_id) = args.get("target_id").and_then(serde_json::Value::as_str) else {
            return snapshot.fail(
                "Target is required.",
                "missing_target_id",
                "write_resource(settings) requires target_id.",
                "write",
            );
        };
        let Some(target) = snapshot.targets.iter().find(|target| target.id == target_id).cloned()
        else {
            return snapshot.fail(
                "Target not found.",
                "target_not_found",
                format!("Target not found: {target_id}"),
                "write",
            );
        };
        let Some(section) = args.get("section").and_then(serde_json::Value::as_str) else {
            return snapshot.fail(
                "Settings section and key are required.",
                "missing_settings_key",
                "write_resource(settings) requires section and key.",
                "write",
            );
        };
        let Some(key) = args.get("key").and_then(serde_json::Value::as_str) else {
            return snapshot.fail(
                "Settings section and key are required.",
                "missing_settings_key",
                "write_resource(settings) requires section and key.",
                "write",
            );
        };
        let value = args.get("value").cloned().unwrap_or(serde_json::Value::Null);
        if args.get("dry_run").and_then(serde_json::Value::as_bool).unwrap_or(false) {
            return snapshot.ok(
                format!("Dry-run settings write {section}.{key}."),
                "Dry-run only; settings were not changed.",
                serde_json::json!({ "section": section, "key": key, "value": value }),
                "write",
            ).with_target(target);
        }
        match settings_with_json_patch(self.settings_store.settings(), section, key, value.clone()) {
            Ok(next_settings) => {
                self.edit_settings(|settings| *settings = next_settings, cx);
                snapshot.ok(
                    format!("Updated settings {section}.{key}."),
                    format!("{section}.{key} updated."),
                    serde_json::json!({ "section": section, "key": key, "value": value }),
                    "write",
                ).with_target(target)
            }
            Err(error) => snapshot.fail(
                "Settings section cannot be updated.",
                "unsupported_settings_section",
                error,
                "write",
            ).with_target(target),
        }
    }

    fn execute_ai_remember_preference(
        &mut self,
        args: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let Some(preference) = args
            .get("preference")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return snapshot.fail(
                "Preference is required.",
                "missing_preference",
                "remember_preference requires preference text.",
                "write",
            );
        };
        let preference = preference.to_string();
        self.edit_settings(
            |settings| {
                let current = settings.ai.memory.content.trim();
                settings.ai.memory.content = [current, &format!("- {preference}")]
                    .into_iter()
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
            },
            cx,
        );
        snapshot.ok(
            "Preference remembered.",
            preference.clone(),
            serde_json::json!({ "preference": preference, "persisted": true }),
            "write",
        )
    }

    fn execute_ai_open_app_surface(
        &mut self,
        args: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let surface = args
            .get("surface")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let target = args
            .get("target_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|target_id| snapshot.targets.iter().find(|target| target.id == target_id))
            .cloned();

        match surface {
            "local_terminal" | "terminal" => match self.create_local_terminal_tab(window, cx) {
                Ok(()) => {
                    let active_tab_id = self.active_tab_id.map(|tab_id| tab_id.0.to_string());
                    let refreshed = self.ai_orchestrator_snapshot(cx);
                    let target = refreshed
                        .targets
                        .iter()
                        .find(|target| {
                            target.kind == "terminal-session"
                                && active_tab_id
                                    .as_ref()
                                    .is_some_and(|tab_id| target.refs.get("tabId") == Some(tab_id))
                                && target
                                    .metadata
                                    .get("terminalType")
                                    .and_then(serde_json::Value::as_str)
                                    == Some("local_terminal")
                        })
                        .cloned();
                    refreshed
                        .ok(
                            "Opened local terminal.",
                            target
                                .as_ref()
                                .map(|target| {
                                    serde_json::to_string_pretty(&target_json(target))
                                        .unwrap_or_default()
                                })
                                .unwrap_or_else(|| "Opened local terminal.".to_string()),
                            target
                                .as_ref()
                                .map(target_json)
                                .unwrap_or_else(|| serde_json::json!({ "surface": surface })),
                            "write",
                        )
                        .with_optional_target(target)
                }
                Err(error) => snapshot.fail(
                    "Failed to open local terminal.",
                    "open_local_terminal_failed",
                    error.to_string(),
                    "write",
                ),
            },
            "settings" => {
                if let Some(section) = args.get("section").and_then(serde_json::Value::as_str)
                    && let Some(tab) = settings_tab_for_ai_section(section)
                {
                    self.active_settings_tab = tab;
                }
                self.open_settings_tab(window, cx);
                snapshot
                    .ok("Opened settings.", "Opened settings.", serde_json::json!({ "surface": surface }), "write")
                    .with_optional_target(target)
            }
            "connection_manager" => {
                self.open_session_manager_tab(window, cx);
                snapshot
                    .ok("Opened connection_manager.", "Opened connection_manager.", serde_json::json!({ "surface": surface }), "write")
                    .with_optional_target(target)
            }
            "connection_pool" => {
                self.open_connection_pool_tab(window, cx);
                snapshot
                    .ok("Opened connection_pool.", "Opened connection_pool.", serde_json::json!({ "surface": surface }), "write")
                    .with_optional_target(target)
            }
            "connection_monitor" => {
                self.open_connection_monitor_tab(window, cx);
                snapshot
                    .ok("Opened connection_monitor.", "Opened connection_monitor.", serde_json::json!({ "surface": surface }), "write")
                    .with_optional_target(target)
            }
            "file_manager" => {
                self.open_file_manager_tab(window, cx);
                snapshot
                    .ok("Opened file_manager.", "Opened file_manager.", serde_json::json!({ "surface": surface }), "write")
                    .with_optional_target(target)
            }
            "sftp" => {
                let Some(node_id) = target
                    .as_ref()
                    .and_then(|target| target.refs.get("nodeId"))
                    .map(|value| NodeId::new(value.clone()))
                    .or_else(|| self.active_ssh_node_id.clone())
                else {
                    return snapshot.fail(
                        "SFTP surface requires an SSH target.",
                        "missing_node_id",
                        "open_app_surface(sftp) requires a target with nodeId.",
                        "write",
                    );
                };
                self.open_sftp_tab(node_id.clone(), window, cx);
                snapshot
                    .ok("Opened sftp.", format!("Opened sftp for {}.", node_id.0), serde_json::json!({ "surface": surface, "nodeId": node_id.0 }), "write")
                    .with_optional_target(target)
            }
            "ide" => {
                let Some(node_id) = target
                    .as_ref()
                    .and_then(|target| target.refs.get("nodeId"))
                    .map(|value| NodeId::new(value.clone()))
                    .or_else(|| self.active_ssh_node_id.clone())
                else {
                    return snapshot.fail(
                        "IDE surface requires an SSH target.",
                        "missing_node_id",
                        "open_app_surface(ide) requires a target with nodeId.",
                        "write",
                    );
                };
                self.open_ide_folder_picker_tab(node_id.clone(), cx);
                snapshot
                    .ok("Opened ide.", format!("Opened ide for {}.", node_id.0), serde_json::json!({ "surface": surface, "nodeId": node_id.0 }), "write")
                    .with_optional_target(target)
            }
            _ => snapshot
                .fail(
                    "Unknown app surface.",
                    "unknown_app_surface",
                    format!("Unknown app surface: {surface}"),
                    "write",
                )
                .with_optional_target(target),
        }
    }

    fn pane_for_terminal_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<(PaneId, gpui::Entity<oxideterm_gpui_terminal::TerminalPane>)> {
        let pane_id = self.tabs.iter().find_map(|tab| {
            tab.root_pane
                .as_ref()
                .and_then(|root| root.pane_id_for_session(session_id))
        })?;
        let pane = self.panes.get(&pane_id)?.clone();
        Some((pane_id, pane))
    }

    fn ai_connect_target_ready_result(
        &mut self,
        tool_call_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        duration_ms: u128,
        cx: &mut Context<Self>,
    ) -> Option<AiExecutedToolResult> {
        let target_id = args.get("target_id").and_then(serde_json::Value::as_str)?;
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let original = snapshot.targets.iter().find(|target| target.id == target_id)?;
        let connection_id = original.refs.get("connectionId").cloned();
        let node_id = original.refs.get("nodeId").cloned();
        let ready_targets = snapshot
            .targets
            .iter()
            .filter(|target| {
                if (target.kind == "ssh-node" || target.kind == "terminal-session")
                    && target.state == "connected"
                {
                    let connection_matches = connection_id
                        .as_ref()
                        .is_some_and(|id| target.refs.get("connectionId") == Some(id));
                    let node_matches = node_id
                        .as_ref()
                        .is_some_and(|id| target.refs.get("nodeId") == Some(id));
                    target.id == target_id || connection_matches || node_matches
                } else {
                    false
                }
            })
            .cloned()
            .collect::<Vec<_>>();
        if ready_targets.is_empty() {
            return None;
        }
        let primary = snapshot
            .targets
            .iter()
            .find(|target| {
                (target.kind == "ssh-node" && target.state == "connected")
                    && (node_id
                        .as_ref()
                        .is_some_and(|id| target.refs.get("nodeId") == Some(id))
                        || connection_id
                            .as_ref()
                            .is_some_and(|id| target.refs.get("connectionId") == Some(id)))
            })
            .cloned()
            .or_else(|| ready_targets.first().cloned())?;
        let session_id = ready_targets
            .iter()
            .find(|target| target.kind == "terminal-session")
            .and_then(|target| target.refs.get("sessionId"))
            .cloned()
            .or_else(|| primary.refs.get("sessionId").cloned())
            .unwrap_or_default();
        let node_id = primary
            .refs
            .get("nodeId")
            .cloned()
            .or_else(|| node_id.clone())
            .unwrap_or_default();
        let connection_id = primary
            .refs
            .get("connectionId")
            .cloned()
            .or_else(|| connection_id.clone())
            .unwrap_or_default();
        let returned_targets = std::iter::once(primary.clone())
            .chain(
                ready_targets
                    .into_iter()
                    .filter(|target| target.id != primary.id),
            )
            .collect::<Vec<_>>();
        let output = returned_targets
            .iter()
            .map(|target| format!("{} — {}", target.id, target.label))
            .collect::<Vec<_>>()
            .join("\n");
        Some(snapshot.to_executed_tool_result(
            tool_call_id.to_string(),
            tool_name.to_string(),
            snapshot
                .ok(
                    format!("Connected {}.", primary.label),
                    output,
                    serde_json::json!({
                        "nodeId": node_id,
                        "sessionId": session_id,
                        "connectionId": connection_id,
                    }),
                    "write",
                )
                .with_target(primary)
                .with_targets(returned_targets),
            duration_ms,
        ))
    }

    fn ai_connect_target_timeout_result(
        &mut self,
        tool_call_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        base: &AiExecutedToolResult,
        duration_ms: u128,
        cx: &mut Context<Self>,
    ) -> AiExecutedToolResult {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let target = args
            .get("target_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|target_id| {
                snapshot
                    .targets
                    .iter()
                    .find(|target| target.id == target_id)
                    .cloned()
            });
        let detail = match target.as_ref().map(|target| target.kind.as_str()) {
            Some("saved-connection") => {
                "The saved connection flow did not return a live terminal."
            }
            Some("ssh-node") => {
                "The SSH target did not return a live terminal before the executor timeout."
            }
            Some("terminal-session") => {
                "The terminal target did not become available before the executor timeout."
            }
            _ => "The connection request did not return a live OxideTerm target.",
        };
        let data = serde_json::json!({
            "requestedArgs": args,
            "initialResult": base.envelope,
        });
        snapshot.to_executed_tool_result(
            tool_call_id.to_string(),
            tool_name.to_string(),
            snapshot
                .fail("Connection did not complete.", "connect_failed", detail, "write")
                .with_data(data)
                .with_optional_target(target),
            duration_ms,
        )
    }
}
