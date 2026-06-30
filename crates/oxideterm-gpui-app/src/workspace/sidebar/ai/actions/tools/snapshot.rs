const AI_CONNECT_TARGET_TIMEOUT_TICKS: usize = 900;
const AI_CONNECT_TARGET_POLL_INTERVAL_MS: u64 = 100;

fn ai_sftp_target_for_node(
    node_id: &NodeId,
    node: &WorkspaceSshNode,
    sftp_session_id: String,
) -> AiOrchestratorTarget {
    let mut refs = BTreeMap::new();
    refs.insert("nodeId".to_string(), node_id.0.clone());
    refs.insert("sessionId".to_string(), sftp_session_id.clone());
    if let Some(saved_connection_id) = node.saved_connection_id.as_ref() {
        refs.insert("connectionId".to_string(), saved_connection_id.clone());
    }
    // Tauri exposes SFTP targets from node runtime state, not from the SFTP tab
    // itself, so keep the target shape node-scoped even when a tab is open.
    AiOrchestratorTarget {
        id: format!("sftp-session:{sftp_session_id}"),
        kind: "sftp-session".to_string(),
        label: format!("SFTP {}", node.config.host),
        state: "connected".to_string(),
        capabilities: vec![
            "filesystem.read".to_string(),
            "filesystem.write".to_string(),
            "state.list".to_string(),
        ],
        refs,
        metadata: serde_json::json!({
            "host": node.config.host,
        }),
        terminal_buffer: None,
        terminal_screen: None,
    }
}

fn ai_connect_result_terminal_target(
    target: &AiOrchestratorTarget,
    original_label: &str,
    node_id: Option<&str>,
    connection_id: Option<&str>,
) -> AiOrchestratorTarget {
    let mut refs = BTreeMap::new();
    if let Some(node_id) = node_id {
        refs.insert("nodeId".to_string(), node_id.to_string());
    }
    if let Some(session_id) = target.refs.get("sessionId") {
        refs.insert("sessionId".to_string(), session_id.clone());
    }
    if let Some(connection_id) = connection_id {
        refs.insert("connectionId".to_string(), connection_id.to_string());
    }
    // Tauri connect_target synthesizes a terminal target for the connection
    // result; regular discovery keeps terminal refs limited to session/tab.
    AiOrchestratorTarget {
        id: target.id.clone(),
        kind: target.kind.clone(),
        label: format!("{original_label} terminal"),
        state: target.state.clone(),
        capabilities: target.capabilities.clone(),
        refs,
        metadata: serde_json::json!({ "terminalType": "terminal" }),
        terminal_buffer: target.terminal_buffer.clone(),
        terminal_screen: target.terminal_screen.clone(),
    }
}

fn ai_opened_local_terminal_target(target: &AiOrchestratorTarget) -> AiOrchestratorTarget {
    let mut refs = BTreeMap::new();
    if let Some(session_id) = target.refs.get("sessionId") {
        refs.insert("sessionId".to_string(), session_id.clone());
    }
    // Tauri returns a synthetic local-terminal target from open_app_surface,
    // not the richer target-discovery snapshot that carries tab metadata.
    AiOrchestratorTarget {
        id: target.id.clone(),
        kind: target.kind.clone(),
        label: target.label.clone(),
        state: target.state.clone(),
        capabilities: target.capabilities.clone(),
        refs,
        metadata: serde_json::json!({ "terminalType": "local_terminal" }),
        terminal_buffer: None,
        terminal_screen: None,
    }
}

fn ai_raw_tcp_terminal_label(config: &RawTcpSessionConfig) -> String {
    let scheme = if config.tls.enabled { "TLS" } else { "TCP" };
    format!("{scheme} {}", config.endpoint_label())
}

fn ai_raw_tcp_terminal_metadata(config: &RawTcpSessionConfig) -> serde_json::Value {
    // Keep the AI target schema explicit so local socket sessions do not inherit
    // shell-oriented behavior from ordinary local terminals.
    serde_json::json!({
        "terminalType": "raw_tcp",
        "terminalTransport": "raw_tcp",
        "host": config.host,
        "port": config.port,
        "lineEnding": format!("{:?}", config.line_ending).to_lowercase(),
        "displayMode": format!("{:?}", config.display_mode).to_lowercase(),
        "sendMode": format!("{:?}", config.send_mode).to_lowercase(),
        "tls": {
            "enabled": config.tls.enabled,
            "verification": format!("{:?}", config.tls.verification).to_lowercase(),
            "serverName": config.tls.server_name,
        },
    })
}

fn ai_raw_udp_terminal_label(config: &RawUdpSessionConfig) -> String {
    format!("UDP {}", config.remote_endpoint_label())
}

fn ai_raw_udp_terminal_metadata(config: &RawUdpSessionConfig) -> serde_json::Value {
    // UDP targets are datagram-oriented local sockets, so expose enough shape
    // for tools to avoid stream-only assumptions.
    serde_json::json!({
        "terminalType": "raw_udp",
        "terminalTransport": "raw_udp",
        "remoteHost": config.remote_host,
        "remotePort": config.remote_port,
        "localBindHost": config.local_bind_host,
        "localBindPort": config.local_bind_port,
        "lineEnding": format!("{:?}", config.line_ending).to_lowercase(),
        "displayMode": format!("{:?}", config.display_mode).to_lowercase(),
        "sendMode": format!("{:?}", config.send_mode).to_lowercase(),
    })
}

fn ai_ide_workspace_target_for_node(
    node_id: &NodeId,
    node: &WorkspaceSshNode,
    active_editor_tab_id: Option<String>,
    project_root_path: Option<String>,
    project_name: Option<String>,
) -> AiOrchestratorTarget {
    let mut refs = BTreeMap::new();
    refs.insert("nodeId".to_string(), node_id.0.clone());
    if let Some(active_editor_tab_id) = active_editor_tab_id.as_ref() {
        refs.insert("tabId".to_string(), active_editor_tab_id.clone());
    }
    if let Some(saved_connection_id) = node.saved_connection_id.as_ref() {
        refs.insert("connectionId".to_string(), saved_connection_id.clone());
    }
    let mut metadata = serde_json::Map::new();
    if let Some(project_root_path) = project_root_path {
        metadata.insert("rootPath".to_string(), serde_json::json!(project_root_path));
    }
    metadata.insert(
        "activeTabId".to_string(),
        active_editor_tab_id
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    // Tauri's IDE target is keyed by node id and carries the active editor tab
    // separately; it never uses the outer app tab id as the workspace tab ref.
    AiOrchestratorTarget {
        id: format!("ide-workspace:{}", node_id.0),
        kind: "ide-workspace".to_string(),
        label: project_name.unwrap_or_else(|| "IDE workspace".to_string()),
        state: "connected".to_string(),
        capabilities: vec![
            "filesystem.read".to_string(),
            "filesystem.write".to_string(),
            "navigation.open".to_string(),
            "state.list".to_string(),
        ],
        refs,
        metadata: serde_json::Value::Object(metadata),
        terminal_buffer: None,
        terminal_screen: None,
    }
}

impl WorkspaceApp {
    fn ai_orchestrator_snapshot(&self, cx: &mut Context<Self>) -> AiOrchestratorRuntimeSnapshot {
        let mut targets = Vec::new();
        for connection in self.connection_store.connections() {
            let mut refs = BTreeMap::new();
            refs.insert("connectionId".to_string(), connection.id.clone());
            let connection_label = if connection.name.trim().is_empty() {
                connection.host.as_str()
            } else {
                connection.name.as_str()
            };
            targets.push(AiOrchestratorTarget {
                id: format!("saved-connection:{}", connection.id),
                kind: "saved-connection".to_string(),
                label: format!(
                    "{} ({}@{}:{})",
                    connection_label, connection.username, connection.host, connection.port
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
                terminal_screen: None,
            });
        }

        for tab in &self.tabs {
            let mut refs = BTreeMap::new();
            refs.insert("tabId".to_string(), tab.id.0.to_string());
            if let Some(session_id) = tab.root_pane.as_ref().and_then(|root| {
                let mut pane_ids = Vec::new();
                root.collect_pane_ids(&mut pane_ids);
                pane_ids
                    .into_iter()
                    .find_map(|pane_id| root.session_id_for_pane(pane_id))
            }) {
                refs.insert("sessionId".to_string(), session_id.0.to_string());
            }
            targets.push(AiOrchestratorTarget {
                id: format!("app-surface:{}:{}", ai_tab_kind_label(&tab.kind), tab.id.0),
                kind: "app-surface".to_string(),
                label: if tab.title.is_empty() {
                    ai_tab_kind_label(&tab.kind).to_string()
                } else {
                    tab.title.clone()
                },
                state: if Some(tab.id) == self.main_window_tabs.active_tab_id {
                    "connected"
                } else {
                    "available"
                }
                .to_string(),
                capabilities: vec!["navigation.open".to_string(), "state.list".to_string()],
                refs,
                metadata: serde_json::json!({ "tabType": ai_tab_kind_label(&tab.kind) }),
                terminal_buffer: None,
                terminal_screen: None,
            });
        }

        for (node_id, node) in &self.ssh_nodes {
            let terminal_id = node.terminal_ids.first().copied();
            let resolved_connection = self.node_router.resolve_connection_now(node_id).ok();
            let sftp_session_id = resolved_connection
                .as_ref()
                .and_then(|resolved| resolved.sftp_session_id.clone());
            if node.saved_connection_id.is_some() || node.readiness == NodeReadiness::Ready {
                let runtime_status = match node.readiness {
                    NodeReadiness::Ready => "connected",
                    NodeReadiness::Connecting => "connecting",
                    NodeReadiness::Error => "error",
                    NodeReadiness::Disconnected => "disconnected",
                };
                let mut refs = BTreeMap::new();
                refs.insert("nodeId".to_string(), node_id.0.clone());
                if let Some(saved_connection_id) = node.saved_connection_id.as_ref() {
                    refs.insert("connectionId".to_string(), saved_connection_id.clone());
                }
                if let Some(session_id) = terminal_id {
                    refs.insert("sessionId".to_string(), session_id.0.to_string());
                }
                let mut metadata = serde_json::json!({
                    "host": node.config.host,
                    "port": node.config.port,
                    "username": node.config.username,
                    "status": runtime_status,
                    "terminalIds": node.terminal_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
                    "title": node.title,
                });
                if let Some(sftp_session_id) = sftp_session_id.as_ref()
                    && let Some(object) = metadata.as_object_mut()
                {
                    object.insert(
                        "sftpSessionId".to_string(),
                        serde_json::json!(sftp_session_id),
                    );
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
                    metadata,
                    terminal_buffer: None,
                    terminal_screen: None,
                });
            }
            if let Some(sftp_session_id) = sftp_session_id {
                targets.push(ai_sftp_target_for_node(node_id, node, sftp_session_id));
            }
        }

        for node_id in self.sftp_tab_nodes.values() {
            let Some(node) = self.ssh_nodes.get(node_id) else {
                continue;
            };
            let Some(sftp_session_id) = self
                .node_router
                .resolve_connection_now(node_id)
                .ok()
                .and_then(|resolved| resolved.sftp_session_id)
            else {
                continue;
            };
            targets.push(ai_sftp_target_for_node(node_id, node, sftp_session_id));
        }

        for (tab_id, node_id) in &self.ide_tab_nodes {
            let Some(node) = self.ssh_nodes.get(node_id) else {
                continue;
            };
            let (project_root_path, project_name, active_editor_tab_id) = self
                .ide_tab_surfaces
                .get(tab_id)
                .map(|surface| {
                    surface.update(cx, |surface, _cx| {
                        let context = surface.ai_context_snapshot();
                        (
                            surface.project_root_path(),
                            context.map(|snapshot| snapshot.project_name),
                            surface.active_editor_tab_id(),
                        )
                    })
                })
                .unwrap_or((None, None, None));
            targets.push(ai_ide_workspace_target_for_node(
                node_id,
                node,
                active_editor_tab_id,
                project_root_path,
                project_name,
            ));
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
                let raw_tcp_config = self.raw_tcp_terminal_configs.get(&session_id);
                let raw_udp_config = self.raw_udp_terminal_configs.get(&session_id);
                let serial_config = self.serial_terminal_configs.get(&session_id);
                let is_raw_tcp_terminal = raw_tcp_config.is_some();
                let is_raw_udp_terminal = raw_udp_config.is_some();
                let is_serial_terminal = serial_config.is_some();
                let is_local_terminal = tab.kind == TabKind::LocalTerminal;
                let terminal_type = if is_raw_tcp_terminal {
                    "raw_tcp"
                } else if is_raw_udp_terminal {
                    "raw_udp"
                } else if is_serial_terminal {
                    "serial"
                } else if is_local_terminal {
                    "local_terminal"
                } else {
                    "terminal"
                };
                let mut refs = BTreeMap::new();
                refs.insert("sessionId".to_string(), session_id.0.to_string());
                refs.insert("tabId".to_string(), tab.id.0.to_string());
                let (terminal_buffer, terminal_screen, accepts_input, terminal_running) = {
                    let pane = pane.read(cx);
                    let screen = pane.ai_screen_snapshot();
                    let is_alternate_buffer = pane.ai_screen_is_alternate_buffer();
                    (
                        pane.ai_buffer_snapshot(),
                        ai_terminal_screen_snapshot_json(&screen, is_alternate_buffer),
                        pane.ai_accepts_input(),
                        pane.lifecycle().is_running(),
                    )
                };
                let label = if let Some(config) = raw_tcp_config {
                    ai_raw_tcp_terminal_label(config)
                } else if let Some(config) = raw_udp_config {
                    ai_raw_udp_terminal_label(config)
                } else if let Some(config) = serial_config {
                    format!("Serial {}", config.port_path)
                } else if is_local_terminal {
                    format!("Local terminal {}", tab.title)
                } else {
                    format!("SSH terminal {}", ai_short_id(&session_id.0.to_string()))
                };
                let metadata = if let Some(config) = raw_tcp_config {
                    ai_raw_tcp_terminal_metadata(config)
                } else if let Some(config) = raw_udp_config {
                    ai_raw_udp_terminal_metadata(config)
                } else if let Some(config) = serial_config {
                    serde_json::json!({
                        "terminalType": terminal_type,
                        "terminalTransport": "serial",
                        "portPath": config.port_path,
                        "baudRate": config.baud_rate,
                        "dataBits": config.data_bits,
                        "stopBits": config.stop_bits,
                        "parity": format!("{:?}", config.parity).to_lowercase(),
                        "flowControl": format!("{:?}", config.flow_control).to_lowercase(),
                    })
                } else if is_local_terminal {
                    // Tauri's local terminal store overwrites registry metadata
                    // with shell-oriented metadata instead of pane internals.
                    serde_json::json!({
                        "terminalType": terminal_type,
                        "shell": {
                            "label": tab.title.clone(),
                        },
                    })
                } else {
                    serde_json::json!({
                        "paneId": pane_id.0,
                        "terminalType": terminal_type,
                    })
                };
                targets.push(AiOrchestratorTarget {
                    id: format!("terminal-session:{}", session_id.0),
                    kind: "terminal-session".to_string(),
                    label,
                    state: if is_local_terminal {
                        if terminal_running { "connected" } else { "stale" }
                    } else if accepts_input {
                        "connected"
                    } else {
                        "opening"
                    }
                    .to_string(),
                    capabilities: vec![
                        "terminal.observe".to_string(),
                        "terminal.send".to_string(),
                        "terminal.wait".to_string(),
                        "state.list".to_string(),
                    ],
                    refs,
                    metadata,
                    terminal_buffer: Some(terminal_buffer),
                    terminal_screen: Some(terminal_screen),
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
            terminal_screen: None,
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
            terminal_screen: None,
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
            terminal_screen: None,
        });

        // Tauri deduplicates targets by id after discovery; keep the first
        // discovery order while replacing duplicate values with the latest
        // runtime snapshot.
        let mut target_indexes = std::collections::HashMap::<String, usize>::new();
        let mut deduped_targets = Vec::<AiOrchestratorTarget>::new();
        for target in targets {
            if let Some(index) = target_indexes.get(&target.id).copied() {
                deduped_targets[index] = target;
            } else {
                target_indexes.insert(target.id.clone(), deduped_targets.len());
                deduped_targets.push(target);
            }
        }
        let targets = deduped_targets;

        let settings = self.settings_store.settings();
        let active_tab_ref = self
            .main_window_tabs
            .active_tab_id
            .and_then(|active_tab_id| self.tabs.iter().find(|tab| tab.id == active_tab_id));
        let active_node_id = self
            .active_ssh_node_id
            .as_ref()
            .map(|node_id| node_id.0.clone());
        let active_session_id = active_tab_ref
            .and_then(|tab| tab.root_pane.as_ref())
            .and_then(|root| {
                let mut pane_ids = Vec::new();
                root.collect_pane_ids(&mut pane_ids);
                pane_ids
                    .into_iter()
                    .find_map(|pane_id| root.session_id_for_pane(pane_id))
            })
            .map(|session_id| session_id.0.to_string())
            .or_else(|| {
                self.active_ssh_node_id
                    .as_ref()
                    .and_then(|node_id| self.ssh_nodes.get(node_id))
                    .and_then(|node| node.terminal_ids.first().copied())
                    .map(|session_id| session_id.0.to_string())
            });
        let active_tab = self.main_window_tabs.active_tab_id.and_then(|active_tab_id| {
            self.tabs
                .iter()
                .find(|tab| tab.id == active_tab_id)
                .map(|tab| {
                    serde_json::json!({
                        "id": tab.id.0.to_string(),
                        "type": ai_tab_kind_label(&tab.kind),
                        "title": tab.title,
                        "sessionId": active_session_id.clone(),
                    })
                })
        });
        let active_node = self.active_ssh_node_id.as_ref().and_then(|node_id| {
            self.ssh_nodes.get(node_id).map(|node| {
                serde_json::json!({
                    "id": node_id.0,
                    "host": node.config.host,
                    "username": node.config.username,
                    "status": match node.readiness {
                        NodeReadiness::Ready => "connected",
                        NodeReadiness::Connecting => "connecting",
                        NodeReadiness::Error => "error",
                        NodeReadiness::Disconnected => "disconnected",
                    },
                    "terminalIds": node.terminal_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
                })
            })
        });
        let settings_summary = serde_json::json!({
            "ai": {
                "enabled": settings.ai.enabled,
                "toolUse": {
                    "enabled": settings.ai.tool_use.enabled,
                    "maxRounds": settings.ai.tool_use.max_rounds,
                    "maxCallsPerRound": settings.ai.tool_use.max_calls_per_round,
                    "autoApproveTools": settings.ai.tool_use.auto_approve_tools,
                    "disabledTools": settings.ai.tool_use.disabled_tools,
                },
            },
            "terminal": {
                "renderer": settings.terminal.renderer,
                "encoding": settings.terminal.terminal_encoding,
            },
            "sftp": {
                "directoryParallelism": settings.sftp.directory_parallelism,
            }
        });
        let transfers = ai_transfers_state(&self.sftp_transfer_manager, &self.ai_runtime_epoch);
        let mut ssh_node_states = std::collections::BTreeMap::<String, usize>::new();
        for node in self.ssh_nodes.values() {
            let state = match node.readiness {
                NodeReadiness::Ready => "connected",
                NodeReadiness::Connecting => "connecting",
                NodeReadiness::Error => "error",
                NodeReadiness::Disconnected => "disconnected",
            };
            *ssh_node_states.entry(state.to_string()).or_default() += 1;
        }
        let recent_event_cutoff =
            std::time::SystemTime::now() - std::time::Duration::from_secs(10 * 60);
        let recent_events = self
            .notification_center
            .event_log
            .entries
            .iter()
            .filter(|entry| entry.timestamp >= recent_event_cutoff)
            .collect::<Vec<_>>();
        let recent_event_warnings = recent_events
            .iter()
            .filter(|entry| entry.severity == WorkspaceEventSeverity::Warn)
            .count();
        let recent_event_errors = recent_events
            .iter()
            .filter(|entry| entry.severity == WorkspaceEventSeverity::Error)
            .count();
        // Keep get_state(health) on the same public shape as Tauri even though
        // native derives the values from GPUI-owned stores instead of Zustand.
        let health_state = serde_json::json!({
            "runtimeEpoch": self.ai_runtime_epoch,
            "tabs": {
                "open": self.tabs.len(),
                "activeTabId": self.main_window_tabs.active_tab_id.map(|id| id.0.to_string()),
            },
            "terminalRegistry": { "entries": self.panes.len() },
            "localTerminals": {
                "count": self.visible_local_terminal_session_count() + self.detached_local_terminals.len(),
            },
            "sshNodes": {
                "total": self.ssh_nodes.len(),
                "states": ssh_node_states,
            },
            "transfers": {
                "total": transfers.get("total").and_then(serde_json::Value::as_u64).unwrap_or(0),
                "counts": transfers.get("counts").cloned().unwrap_or_else(|| serde_json::json!({})),
            },
            "recentEvents": {
                "total": recent_events.len(),
                "warnings": recent_event_warnings,
                "errors": recent_event_errors,
            },
        });
        AiOrchestratorRuntimeSnapshot {
            targets,
            active_tab,
            active_node,
            active_session_id,
            active_tab_id: self.main_window_tabs.active_tab_id.map(|tab_id| tab_id.0.to_string()),
            active_node_id,
            memory: ai_memory_settings_json(settings.ai.memory.enabled, &settings.ai.memory.content),
            health_state,
            node_router: self.node_router.clone(),
            sftp_transfer_manager: self.sftp_transfer_manager.clone(),
            agent_fs: self.ai_agent_fs.clone(),
            backend_runtime: self.forwarding_runtime.clone(),
            rag_store: self.ai_rag_store.get(),
            ai_mcp_registry: self.ai_mcp_registry.clone(),
            ai_acp_runtime_registry: self.ai_acp_runtime_registry.clone(),
            ai_key_store: self.ai_key_store.clone(),
            ai_providers: settings.ai.providers.clone(),
            ai_embedding_config: settings.ai.embedding_config.clone(),
            ai_context_window: AI_COMPACTION_DEFAULT_CONTEXT_WINDOW,
            runtime_epoch: self.ai_runtime_epoch.clone(),
            // Tauri read_resource(settings) exposes the settings object, while
            // get_state(settings) returns a compact diagnostic summary.
            settings_state: serde_json::to_value(settings).unwrap_or_else(|_| settings_summary.clone()),
            settings_summary,
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
            "run_command" => self.execute_ai_terminal_run_command(&args, window, cx),
            "send_terminal_input" => self.execute_ai_send_terminal_input(&args, window, cx),
            "write_resource" => self.execute_ai_write_settings_resource(&args, window, cx),
            "open_app_surface" => self.execute_ai_open_app_surface(&args, window, cx),
            "remember_preference" => self.execute_ai_remember_preference(&args, cx),
            _ => self.ai_orchestrator_snapshot(cx).fail(
                "Unknown orchestrator tool.",
                "unknown_tool",
                format!("{tool_name} is not an OxideSens task tool."),
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
                .is_some_and(|kind| matches!(kind, "terminal-session" | "ssh-node" | "local-shell"))
        {
            self.start_ai_terminal_run_command_execution(
                tool_call_id,
                tool_name,
                args,
                sender,
                window,
                cx,
            );
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
        if base.envelope.get("summary").and_then(serde_json::Value::as_str)
            == Some("Target is already live.")
        {
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
            for _ in 0..AI_CONNECT_TARGET_TIMEOUT_TICKS {
                // Tauri waits for connectToSaved to finish before returning
                // connect_target. Keep native's UI-thread bridge patient enough
                // for slow SSH/proxy chains, while still polling the snapshot.
                Timer::after(Duration::from_millis(AI_CONNECT_TARGET_POLL_INTERVAL_MS)).await;
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
        let Some(target_id) = args.get("target_id").and_then(serde_json::Value::as_str) else {
            return snapshot.fail(
                "Target not found.",
                "target_not_found",
                "Target not found: ",
                "write",
            )
            .with_next_actions(vec![serde_json::json!({
                "action": "list_targets",
                "reason": "Refresh available targets before connecting."
            })]);
        };
        let Some(target) = snapshot.targets.iter().find(|target| target.id == target_id).cloned()
        else {
            return snapshot.fail(
                "Target not found.",
                "target_not_found",
                format!("Target not found: {target_id}"),
                "write",
            )
            .with_next_actions(vec![serde_json::json!({
                "action": "list_targets",
                "reason": "Refresh available targets before connecting."
            })]);
        };

        match target.kind.as_str() {
            "terminal-session" => {
                if target.state != "connected" {
                    return snapshot
                        .fail(
                            "Target is not ready.",
                            "target_not_ready",
                            format!(
                                "{} is {}; wait for it to become connected before continuing.",
                                target.id, target.state
                            ),
                            "write",
                        )
                        .with_target(target)
                        .with_next_actions(vec![serde_json::json!({
                            "action": "list_targets",
                            "reason": "Refresh available targets before retrying."
                        })]);
                }
                self.reveal_ai_target_if_visible(&target, window, cx);
                snapshot
                    .ok(
                        "Target is already live.",
                        "Target is already live.",
                        serde_json::json!({
                            "nodeId": target.refs.get("nodeId").cloned().unwrap_or_default(),
                            "sessionId": target.refs.get("sessionId").cloned().unwrap_or_default(),
                        }),
                        "write",
                    )
                    .with_target(target)
            }
            "ssh-node" => {
                if target.state == "connected" {
                    if self.reveal_ai_target_if_visible(&target, window, cx) {
                        return snapshot
                            .ok(
                                "Target is already live.",
                                "Target is already live.",
                                serde_json::json!({
                                    "nodeId": target.refs.get("nodeId").cloned().unwrap_or_default(),
                                    "sessionId": target.refs.get("sessionId").cloned().unwrap_or_default(),
                                }),
                                "write",
                            )
                            .with_target(target);
                    }
                }
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
                // Tauri reconnects stale ssh-node targets and creates a fresh terminal;
                // stale pane metadata must not be reported as an already-live target.
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
                        .with_target(target)
                        .with_next_actions(ai_ssh_reconnect_failed_next_actions()),
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
                let Some(config) = oxideterm_session_adapter::ssh_config_from_saved_connection(
                    &self.connection_store,
                    self.settings_store.settings(),
                    &connection,
                ) else {
                    if self.try_reuse_active_saved_connection_terminal(
                        &connection_id,
                        &connection,
                        window,
                        cx,
                    ) {
                        let refreshed = self.ai_orchestrator_snapshot(cx);
                        return refreshed
                            .ok(
                                "Focused existing SSH terminal.",
                                "Focused existing SSH terminal.",
                                serde_json::json!({ "connectionId": connection_id }),
                                "write",
                            )
                            .with_target(target);
                    }
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
                // Use the same saved-connection flow as the GUI. Proxy-chain
                // saved targets must pass through the resumable SessionTree
                // preflight plan before a terminal is created.
                self.start_saved_connection_flow(
                    connection_id.clone(),
                    config,
                    title,
                    window,
                    cx,
                );
                let refreshed = self.ai_orchestrator_snapshot(cx);
                let targets = refreshed
                    .targets
                    .iter()
                    .filter(|candidate| candidate.refs.get("connectionId") == Some(&connection_id))
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
            return snapshot.fail_missing_target_id("interactive");
        };
        let Some(target) = snapshot.targets.iter().find(|target| target.id == target_id).cloned()
        else {
            return snapshot.fail_target_not_found(target_id, "interactive");
        };
        if target_requires_live_state(&target) && target.state != "connected" {
            // Tauri gates interactive terminal tools with requireLive before validating session refs.
            return snapshot
                .fail(
                    "Target is not ready.",
                    "target_not_ready",
                    format!(
                        "{target_id} is {}; send_terminal_input requires a connected target.",
                        target.state
                    ),
                    "interactive",
                )
                .with_target(target.clone())
                .with_next_actions(recovery_actions_for_target(&target));
        }
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
        let Some((_pane_id, pane)) = self.reveal_ai_terminal_session(session_id, window, cx) else {
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
                    "Provide text or request Enter with append_enter.",
                    "interactive",
                )
                .with_target(target.clone())
                .with_next_actions(recovery_actions_for_target(&target));
        }
        if !pane.read(cx).ai_accepts_input() {
            return snapshot
                .fail(
                    "Failed to send terminal input.",
                    "terminal_send_failed",
                    "No terminal writer is registered.",
                    "interactive",
                )
                .with_target(target.clone())
                .with_next_actions(recovery_actions_for_target(&target));
        }
        pane.update(cx, |pane, cx| {
            pane.send_ai_input_bytes(payload.as_bytes(), cx);
        });
        snapshot
            .ok(
                "Terminal input sent.",
                "Input sent.",
                serde_json::Value::Null,
                "interactive",
            )
            .with_target(target)
    }

    fn execute_ai_terminal_run_command(
        &mut self,
        args: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let Some(target_id) = args.get("target_id").and_then(serde_json::Value::as_str) else {
            return snapshot.fail_missing_target_id(ai_run_command_preflight_risk());
        };
        let Some(target) = snapshot.targets.iter().find(|target| target.id == target_id).cloned()
        else {
            return snapshot.fail_target_not_found(target_id, ai_run_command_preflight_risk());
        };
        if target_requires_live_state(&target) && target.state != "connected" {
            // Match Tauri's live-target guard before touching terminal session metadata.
            return snapshot
                .fail(
                    "Target is not ready.",
                    "target_not_ready",
                    format!(
                        "{target_id} is {}; run_command requires a connected target.",
                        target.state
                    ),
                    ai_run_command_preflight_risk(),
                )
                .with_target(target);
        }
        let Some(command) = args
            .get("command")
            .and_then(serde_json::Value::as_str)
            .filter(|command| !command.trim().is_empty())
        else {
            // Match Tauri's executor order: requireTarget runs before the
            // terminal capability validates command text.
            return snapshot.fail(
                "Command is required.",
                "missing_command",
                "run_command requires a command.",
                ai_run_command_preflight_risk(),
            );
        };
        let target = match self.resolve_ai_run_command_terminal_target(target, window, cx) {
            Ok(target) => target,
            Err(result) => return result,
        };
        let command = ai_command_with_cwd(command, args.get("cwd").and_then(serde_json::Value::as_str));
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
        let Some((_pane_id, pane)) = self.reveal_ai_terminal_session(session_id, window, cx) else {
            return snapshot
                .fail(
                    "Terminal pane is not ready.",
                    "terminal_pane_missing",
                    "The visible terminal pane is not registered yet.",
                    "interactive",
                )
                .with_target(target);
        };
        if !pane.read(cx).ai_accepts_input() {
            return snapshot
                .fail(
                    "Terminal is not ready.",
                    "terminal_not_ready",
                    "Terminal writer/listener is not ready.",
                    "interactive",
                )
                .with_target(target);
        }
        let before = pane.read(cx).ai_buffer_snapshot();
        pane.update(cx, |pane, cx| {
            pane.begin_command_mark(&command, TerminalCommandMarkDetectionSource::Ai, cx);
            pane.send_command_line(&command, cx);
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
                    serde_json::json!({
                        "executionState": "sent",
                        "visibleInTerminal": true,
                    }),
                    "interactive",
                )
                .with_target(target);
        }
        let after = pane.read(cx).ai_buffer_snapshot();
        let output = terminal_delta_output(&before, &after);
        let output_empty = output.trim().is_empty();
        snapshot
            .ok(
                "Command sent to terminal.",
                if output_empty {
                    format!("Command sent: {command}")
                } else {
                    output
                },
                serde_json::json!({
                    "executionState": if output_empty { "sent" } else { "output_captured" },
                    "visibleInTerminal": true,
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let started = std::time::Instant::now();
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let Some(target_id) = args.get("target_id").and_then(serde_json::Value::as_str) else {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot.fail_missing_target_id(ai_run_command_preflight_risk()),
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
                snapshot.fail_target_not_found(target_id, ai_run_command_preflight_risk()),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        };
        if target_requires_live_state(&target) && target.state != "connected" {
            // Keep the deferred UI execution path on the same live-target contract as Tauri.
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot
                    .fail(
                        "Target is not ready.",
                        "target_not_ready",
                        format!(
                            "{target_id} is {}; run_command requires a connected target.",
                            target.state
                        ),
                        ai_run_command_preflight_risk(),
                    )
                    .with_target(target.clone())
                    .with_next_actions(recovery_actions_for_target(&target)),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        }
        let Some(command) = args
            .get("command")
            .and_then(serde_json::Value::as_str)
            .filter(|command| !command.trim().is_empty())
            .map(str::to_string)
        else {
            // Keep the async UI executor on Tauri's target-first validation path.
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot.fail(
                    "Command is required.",
                    "missing_command",
                    "run_command requires a command.",
                    ai_run_command_preflight_risk(),
                ),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        };
        let target = match self.resolve_ai_run_command_terminal_target(target, window, cx) {
            Ok(target) => target,
            Err(action_result) => {
                let result = snapshot.to_executed_tool_result(
                    tool_call_id,
                    tool_name,
                    action_result,
                    started.elapsed().as_millis(),
                );
                let _ = sender.send(result);
                return;
            }
        };
        let command = ai_command_with_cwd(
            &command,
            args.get("cwd").and_then(serde_json::Value::as_str),
        );
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
        let Some((_pane_id, pane)) = self.reveal_ai_terminal_session(session_id, window, cx) else {
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
        if !pane.read(cx).ai_accepts_input() {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot
                    .fail(
                        "Terminal is not ready.",
                        "terminal_not_ready",
                        "Terminal writer/listener is not ready.",
                        "interactive",
                    )
                    .with_target(target),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        }
        let before = pane.read(cx).ai_buffer_snapshot();
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
                        serde_json::json!({
                            "executionState": "sent",
                            "visibleInTerminal": true,
                        }),
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
                let current = weak.update(cx, |_this, cx| pane.read(cx).ai_buffer_snapshot());
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
                                        "executionState": "output_captured",
                                        "visibleInTerminal": true,
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
                        summary: "Terminal command did not produce completed output.".to_string(),
                        output: if output_empty {
                            "No new output captured.".to_string()
                        } else {
                            output
                        },
                        data: serde_json::json!({
                            "executionState": if output_empty { "timeout" } else { "output_captured" },
                            "visibleInTerminal": true,
                            "waitingForInput": looks_waiting_for_input(&last),
                        }),
                        error_code: output_empty.then(|| "terminal_command_wait_timeout".to_string()),
                        error_message: output_empty.then(|| "No new output after 30s. The command may be waiting for input or still running.".to_string()),
                        risk: "interactive",
                        target: Some(target),
                        targets: Vec::new(),
                        next_actions: Vec::new(),
                        observations: Vec::new(),
                        verified: None,
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
        window: &mut Window,
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
            return snapshot.fail_missing_target_id("write");
        };
        let Some(target) = snapshot.targets.iter().find(|target| target.id == target_id).cloned()
        else {
            return snapshot.fail_target_not_found(target_id, "write");
        };
        if target_requires_live_state(&target) && target.state != "connected" {
            // Tauri resolves and live-checks the target before validating the settings payload.
            return snapshot
                .fail(
                    "Target is not ready.",
                    "target_not_ready",
                    format!(
                        "{target_id} is {}; write_resource requires a connected target.",
                        target.state
                    ),
                    "write",
                )
                .with_target(target.clone())
                .with_next_actions(recovery_actions_for_target(&target));
        }
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
            ).with_target(target)
            .with_verified(false);
        }
        match settings_with_json_patch(self.settings_store.settings(), section, key, value.clone()) {
            Ok(next_settings) => {
                self.edit_settings(|settings| *settings = next_settings, cx);
                if let Some(tab) =
                    oxideterm_gpui_settings_view::settings_tab_from_ai_section(section)
                {
                    self.settings_page.set_active_tab(tab);
                }
                self.open_settings_tab(window, cx);
                snapshot.ok(
                    format!("Updated settings {section}.{key}."),
                    format!("{section}.{key} updated."),
                    serde_json::json!({
                        "section": section,
                        "key": key,
                        "value": value,
                        "visibleSurface": "settings",
                    }),
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
            serde_json::Value::Null,
            "write",
        )
    }

    fn resolve_ai_run_command_terminal_target(
        &mut self,
        target: AiOrchestratorTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<AiOrchestratorTarget, AiActionResultLite> {
        match target.kind.as_str() {
            "local-shell" => self.resolve_ai_local_shell_terminal_target(&target, window, cx),
            "ssh-node" if target.refs.contains_key("sessionId") => Ok(target),
            "ssh-node" => {
                let snapshot = self.ai_orchestrator_snapshot(cx);
                Err(snapshot
                    .fail(
                        "Visible terminal is required.",
                        "missing_visible_terminal",
                        "run_command for ssh-node must use a visible terminal session. Connect or open the target terminal, then retry.",
                        "interactive",
                    )
                    .with_target(target.clone())
                    .with_next_actions(vec![serde_json::json!({
                        "action": "connect_target",
                        "args": { "target_id": target.id },
                        "reason": "Create or reveal a visible terminal before running the command."
                    })]))
            }
            _ => Ok(target),
        }
    }

    fn resolve_ai_local_shell_terminal_target(
        &mut self,
        requested_target: &AiOrchestratorTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<AiOrchestratorTarget, AiActionResultLite> {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        if let Some(target) = local_terminal_run_target(&snapshot) {
            if let Some(session_id) = target
                .refs
                .get("sessionId")
                .and_then(|value| value.parse::<u64>().ok())
                .map(TerminalSessionId)
            {
                self.reveal_ai_terminal_session(session_id, window, cx);
            }
            return Ok(target);
        }

        if let Err(error) = self.create_local_terminal_tab(window, cx) {
            return Err(snapshot
                .fail(
                    "Failed to open local terminal.",
                    "open_local_terminal_failed",
                    error.to_string(),
                    "interactive",
                )
                .with_target(requested_target.clone()));
        }

        let active_tab_id = self.main_window_tabs.active_tab_id.map(|tab_id| tab_id.0.to_string());
        let refreshed = self.ai_orchestrator_snapshot(cx);
        refreshed
            .targets
            .iter()
            .find(|target| {
                target.kind == "terminal-session"
                    && active_tab_id
                        .as_ref()
                        .is_some_and(|tab_id| target.refs.get("tabId") == Some(tab_id))
                    && ai_target_is_local_terminal(target)
            })
            .cloned()
            .ok_or_else(|| {
                refreshed
                    .fail(
                        "Local terminal is not ready.",
                        "local_terminal_missing",
                        "A local terminal was opened, but no visible terminal-session target was registered yet.",
                        "interactive",
                    )
                    .with_target(requested_target.clone())
            })
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
                    let active_tab_id = self.main_window_tabs.active_tab_id.map(|tab_id| tab_id.0.to_string());
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
                        .map(ai_opened_local_terminal_target);
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
                    && let Some(tab) =
                        oxideterm_gpui_settings_view::settings_tab_from_ai_section(section)
                {
                    self.settings_page.set_active_tab(tab);
                }
                self.open_settings_tab(window, cx);
                snapshot
                    .ok("Opened settings.", "Opened settings.", serde_json::Value::Null, "write")
                    .with_optional_target(target)
            }
            "connection_manager" => {
                self.open_session_manager_tab(window, cx);
                snapshot
                    .ok(
                        "Opened connection_manager.",
                        "Opened connection_manager.",
                        serde_json::Value::Null,
                        "write",
                    )
                    .with_optional_target(target)
            }
            "connection_pool" => {
                self.open_connection_pool_tab(window, cx);
                snapshot
                    .ok(
                        "Opened connection_pool.",
                        "Opened connection_pool.",
                        serde_json::Value::Null,
                        "write",
                    )
                    .with_optional_target(target)
            }
            "connection_monitor" => {
                self.open_connection_monitor_tab(window, cx);
                snapshot
                    .ok(
                        "Opened connection_monitor.",
                        "Opened connection_monitor.",
                        serde_json::Value::Null,
                        "write",
                    )
                    .with_optional_target(target)
            }
            "file_manager" => {
                self.open_file_manager_tab(window, cx);
                snapshot
                    .ok(
                        "Opened file_manager.",
                        "Opened file_manager.",
                        serde_json::Value::Null,
                        "write",
                    )
                    .with_optional_target(target)
            }
            "sftp" => {
                let node_id = target
                    .as_ref()
                    .and_then(|target| target.refs.get("nodeId"))
                    .map(|value| NodeId::new(value.clone()))
                    .or_else(|| self.active_ssh_node_id.clone());
                let Some(node_id) = node_id else {
                    return snapshot
                        .fail(
                            "SFTP requires a connected SSH target.",
                            "missing_node_context",
                            "Open SFTP with a target_id that carries nodeId, or connect an SSH target first.",
                            "write",
                        )
                        .with_optional_target(target)
                        .with_next_actions(vec![serde_json::json!({
                            "action": "list_targets",
                            "args": { "view": "files" },
                            "reason": "Find a connected SFTP or SSH target before opening SFTP."
                        })]);
                };
                self.open_sftp_tab(node_id, window, cx);
                snapshot
                    .ok("Opened sftp.", "Opened sftp.", serde_json::Value::Null, "write")
                    .with_optional_target(target)
            }
            "ide" => {
                let node_id = target
                    .as_ref()
                    .and_then(|target| target.refs.get("nodeId"))
                    .map(|value| NodeId::new(value.clone()))
                    .or_else(|| self.active_ssh_node_id.clone());
                let Some(node_id) = node_id else {
                    return snapshot
                        .fail(
                            "IDE requires a connected SSH target.",
                            "missing_node_context",
                            "Open IDE with a target_id that carries nodeId, or connect an SSH target first.",
                            "write",
                        )
                        .with_optional_target(target)
                        .with_next_actions(vec![serde_json::json!({
                            "action": "list_targets",
                            "args": { "view": "files" },
                            "reason": "Find a connected IDE or SSH target before opening IDE."
                        })]);
                };
                self.open_ide_folder_picker_tab(node_id, cx);
                snapshot
                    .ok("Opened ide.", "Opened ide.", serde_json::Value::Null, "write")
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

    fn reveal_ai_target_if_visible(
        &mut self,
        target: &AiOrchestratorTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Tool results are only trustworthy when the affected app surface is
        // visible to the user. Prefer a terminal session because it is the
        // only target that represents concrete shell state.
        if let Some(session_id) = target
            .refs
            .get("sessionId")
            .and_then(|value| value.parse::<u64>().ok())
            .map(TerminalSessionId)
        {
            return self
                .reveal_ai_terminal_session(session_id, window, cx)
                .is_some();
        }

        // Node-only targets may point at non-terminal surfaces. Reveal an
        // already-open SFTP tab, but never create a new surface from a generic
        // connect_target call because that would overstate the requested action.
        if let Some(node_id) = target.refs.get("nodeId") {
            let node_id = NodeId::new(node_id.clone());
            if self.sftp_tab_nodes.values().any(|existing| existing == &node_id) {
                self.open_sftp_tab(node_id, window, cx);
                return true;
            }
        }

        false
    }

    fn reveal_ai_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(PaneId, gpui::Entity<oxideterm_gpui_terminal::TerminalPane>)> {
        let (tab_index, tab_id, pane_id) =
            self.tabs.iter().enumerate().find_map(|(index, tab)| {
                let pane_id = tab
                    .root_pane
                    .as_ref()
                    .and_then(|root| root.pane_id_for_session(session_id))?;
                Some((index, tab.id, pane_id))
            })?;
        let pane = self.panes.get(&pane_id)?.clone();

        // AI terminal tools must act on the same pane the user can see. The
        // model may target a non-active session from context, so make that tab
        // and pane visible before writing input or reading command output.
        self.main_window_tabs.active_tab_id = Some(tab_id);
        if let Some(tab) = self.tabs.get_mut(tab_index) {
            tab.active_pane_id = Some(pane_id);
        }
        self.sync_active_tab_surface();
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window);
        cx.notify();

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
        let primary_session_id = primary.refs.get("sessionId").cloned();
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
        let summary = match original.kind.as_str() {
            "ssh-node" => format!("Reconnected {}.", original.label),
            _ => format!("Connected {}.", original.label),
        };
        let mut returned_targets = std::iter::once(primary.clone())
            .chain(ready_targets.into_iter().filter(|target| target.id != primary.id))
            .collect::<Vec<_>>();
        if let Some(primary_session_id) = primary_session_id.as_ref() {
            let mut returned_target_ids = returned_targets
                .iter()
                .map(|target| target.id.clone())
                .collect::<std::collections::HashSet<_>>();
            for terminal in &snapshot.targets {
                if terminal.kind != "terminal-session"
                    || terminal.state != "connected"
                    || terminal.refs.get("sessionId") != Some(primary_session_id)
                    || returned_target_ids.contains(&terminal.id)
                {
                    continue;
                }
                returned_target_ids.insert(terminal.id.clone());
                returned_targets.push(ai_connect_result_terminal_target(
                    terminal,
                    &original.label,
                    (!node_id.is_empty()).then_some(node_id.as_str()),
                    connection_id.as_deref(),
                ));
            }
        }
        let output = returned_targets
            .iter()
            .find(|target| target.kind == "terminal-session")
            .filter(|_| primary.kind == "ssh-node")
            .map(|terminal| {
                format!(
                    "Connected target {}; visible terminal {}.",
                    primary.id, terminal.id
                )
            })
            .unwrap_or_else(|| {
                returned_targets
                    .iter()
                    .map(|target| format!("{} — {}", target.id, target.label))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        Some(snapshot.to_executed_tool_result(
            tool_call_id.to_string(),
            tool_name.to_string(),
            snapshot
                .ok(
                    summary,
                    output,
                    serde_json::json!({
                        "nodeId": node_id,
                        "sessionId": session_id,
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
        _base: &AiExecutedToolResult,
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
        let next_actions = match target.as_ref().map(|target| target.kind.as_str()) {
            Some("saved-connection") => target
                .as_ref()
                .map(|target| {
                    vec![serde_json::json!({
                        "action": "select_target",
                        "args": { "query": target.label },
                        "reason": "Re-select the target and retry if credentials were updated."
                    })]
                })
                .unwrap_or_default(),
            Some("ssh-node") => vec![serde_json::json!({
                "action": "list_targets",
                "reason": "Refresh target state before retrying."
            })],
            _ => Vec::new(),
        };
        snapshot.to_executed_tool_result(
            tool_call_id.to_string(),
            tool_name.to_string(),
            snapshot
                .fail("Connection did not complete.", "connect_failed", detail, "write")
                .with_optional_target(target)
                .with_next_actions(next_actions),
            duration_ms,
        )
    }
}

#[cfg(test)]
mod raw_tcp_snapshot_tests {
    use super::*;
    use oxideterm_terminal::{
        RawTcpDisplayMode, RawTcpLineEnding, RawTcpSendMode, RawTcpSessionConfig, RawTcpTlsConfig,
        RawTcpTlsVerification, RawUdpDisplayMode, RawUdpLineEnding, RawUdpSendMode,
        RawUdpSessionConfig,
    };

    #[test]
    fn raw_tcp_target_metadata_identifies_local_socket_transport() {
        let config = RawTcpSessionConfig {
            host: "socket.internal".to_string(),
            port: 9000,
            line_ending: RawTcpLineEnding::Lf,
            display_mode: RawTcpDisplayMode::Text,
            send_mode: RawTcpSendMode::Text,
            tls: RawTcpTlsConfig {
                enabled: false,
                verification: RawTcpTlsVerification::System,
                server_name: None,
            },
        };

        let metadata = ai_raw_tcp_terminal_metadata(&config);

        assert_eq!(ai_raw_tcp_terminal_label(&config), "TCP socket.internal:9000");
        assert_eq!(metadata["terminalType"], "raw_tcp");
        assert_eq!(metadata["terminalTransport"], "raw_tcp");
        assert_eq!(metadata["host"], "socket.internal");
        assert_eq!(metadata["port"], 9000);
        assert_eq!(metadata["lineEnding"], "lf");
        assert_eq!(metadata["displayMode"], "text");
        assert_eq!(metadata["sendMode"], "text");
        assert_eq!(metadata["tls"]["enabled"], false);
        assert_eq!(metadata["tls"]["verification"], "system");
        assert!(metadata["tls"]["serverName"].is_null());
    }

    #[test]
    fn raw_tcp_label_marks_tls_sessions() {
        let config = RawTcpSessionConfig {
            host: "secure.internal".to_string(),
            port: 443,
            line_ending: RawTcpLineEnding::None,
            display_mode: RawTcpDisplayMode::Mixed,
            send_mode: RawTcpSendMode::Hex,
            tls: RawTcpTlsConfig {
                enabled: true,
                verification: RawTcpTlsVerification::AllowInvalidCertificates,
                server_name: Some("secure.internal".to_string()),
            },
        };

        let metadata = ai_raw_tcp_terminal_metadata(&config);

        assert_eq!(ai_raw_tcp_terminal_label(&config), "TLS secure.internal:443");
        assert_eq!(metadata["tls"]["enabled"], true);
        assert_eq!(metadata["tls"]["serverName"], "secure.internal");
    }

    #[test]
    fn raw_udp_target_metadata_identifies_local_datagram_transport() {
        let config = RawUdpSessionConfig {
            remote_host: "udp.internal".to_string(),
            remote_port: 8125,
            local_bind_host: Some("127.0.0.1".to_string()),
            local_bind_port: 0,
            line_ending: RawUdpLineEnding::None,
            display_mode: RawUdpDisplayMode::Mixed,
            send_mode: RawUdpSendMode::Hex,
        };

        let metadata = ai_raw_udp_terminal_metadata(&config);

        assert_eq!(ai_raw_udp_terminal_label(&config), "UDP udp.internal:8125");
        assert_eq!(metadata["terminalType"], "raw_udp");
        assert_eq!(metadata["terminalTransport"], "raw_udp");
        assert_eq!(metadata["remoteHost"], "udp.internal");
        assert_eq!(metadata["remotePort"], 8125);
        assert_eq!(metadata["localBindHost"], "127.0.0.1");
        assert_eq!(metadata["localBindPort"], 0);
        assert_eq!(metadata["lineEnding"], "none");
        assert_eq!(metadata["displayMode"], "mixed");
        assert_eq!(metadata["sendMode"], "hex");
    }
}
