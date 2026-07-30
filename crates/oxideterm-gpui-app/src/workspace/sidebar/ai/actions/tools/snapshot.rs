pub(in crate::workspace) const AI_CONNECT_TARGET_TIMEOUT_TICKS: usize = 900;
pub(in crate::workspace) const AI_CONNECT_TARGET_POLL_INTERVAL_MS: u64 = 100;

pub(in crate::workspace) fn ai_model_visible_settings_projection(
    settings: &oxideterm_settings::PersistedSettings,
) -> serde_json::Value {
    // Only explicitly approved, non-secret settings may cross the model boundary.
    serde_json::json!({
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
            "transferProtocol": settings.sftp.transfer_protocol,
        }
    })
}

fn ai_target_projection(target: &AiOrchestratorTarget) -> oxideterm_ai::AiTargetProjection {
    // Runtime snapshots stay app-owned; only their content-free DTO shape crosses
    // into the AI domain projection layer.
    oxideterm_ai::AiTargetProjection {
        id: target.id.clone(),
        kind: target.kind.clone(),
        label: target.label.clone(),
        state: target.state.clone(),
        capabilities: target.capabilities.clone(),
        refs: target.refs.clone(),
        metadata: target.metadata.clone(),
    }
}

fn ai_target_from_projection(projection: oxideterm_ai::AiTargetProjection) -> AiOrchestratorTarget {
    AiOrchestratorTarget {
        id: projection.id,
        kind: projection.kind,
        label: projection.label,
        state: projection.state,
        capabilities: projection.capabilities,
        refs: projection.refs,
        metadata: projection.metadata,
        terminal_buffer: None,
        terminal_screen: None,
    }
}

pub(in crate::workspace) fn ai_sftp_target_for_node(
    node_id: &NodeId,
    node: &WorkspaceSshNode,
    sftp_session_id: String,
) -> AiOrchestratorTarget {
    // Tauri exposes SFTP targets from node runtime state, not from the SFTP tab
    // itself, so keep the target shape node-scoped even when a tab is open.
    ai_target_from_projection(oxideterm_ai::sftp_target_projection(
        oxideterm_ai::AiSftpTargetInput {
            node_id: node_id.0.clone(),
            session_id: sftp_session_id,
            connection_id: node.saved_connection_id.clone(),
            host: node.endpoint.host.clone(),
        },
    ))
}


pub(in crate::workspace) fn ai_opened_local_terminal_target(
    target: &AiOrchestratorTarget,
) -> AiOrchestratorTarget {
    // Tauri returns a synthetic local-terminal target from open_app_surface,
    // not the richer target-discovery snapshot that carries tab metadata.
    ai_target_from_projection(oxideterm_ai::opened_local_terminal_projection(
        &ai_target_projection(target),
    ))
}

pub(in crate::workspace) fn ai_ide_workspace_target_for_node(
    tab_id: TabId,
    node_id: &NodeId,
    node: &WorkspaceSshNode,
    active_editor_tab_id: Option<String>,
    project_root_path: Option<String>,
    project_name: Option<String>,
) -> AiOrchestratorTarget {
    // Tauri's IDE target is keyed by node id and carries the active editor tab
    // separately; it never uses the outer app tab id as the workspace tab ref.
    let mut target = ai_target_from_projection(oxideterm_ai::ide_workspace_target_projection(
        oxideterm_ai::AiIdeTargetInput {
            node_id: node_id.0.clone(),
            connection_id: node.saved_connection_id.clone(),
            active_editor_tab_id,
            project_root_path,
            project_name,
        },
    ));
    // The internal target map must preserve surface identity when one node has
    // multiple IDE projects. The tab id is never emitted in the v2 projection.
    target.id = format!("ide-surface:{}", tab_id.0);
    target
        .refs
        .insert("surfaceTabId".to_string(), tab_id.0.to_string());
    target
}

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_orchestrator_snapshot(
        &self,
        cx: &mut Context<Self>,
    ) -> AiOrchestratorRuntimeSnapshot {
        self.ai_orchestrator_snapshot_for_tool_session(None, cx)
    }

    pub(in crate::workspace) fn ai_orchestrator_snapshot_for_tool_session(
        &self,
        tool_session_id: Option<&ToolSessionId>,
        cx: &mut Context<Self>,
    ) -> AiOrchestratorRuntimeSnapshot {
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

        for tab in self.tabs(cx) {
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
                state: if Some(tab.id) == self.active_tab_id(cx) {
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
                    "host": node.endpoint.host,
                    "port": node.endpoint.port,
                    "username": node.endpoint.username,
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
                        node.endpoint.username, node.endpoint.host, node.endpoint.port
                    ),
                    state: match node.readiness {
                        NodeReadiness::Ready => "connected",
                        NodeReadiness::Connecting => "opening",
                        NodeReadiness::Error => "stale",
                        NodeReadiness::Disconnected => "unavailable",
                    }
                    .to_string(),
                    capabilities: vec![
                        "node.inspect".to_string(),
                        "state.list".to_string(),
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

        for ide_target in self.ide_workspace.read(cx).target_snapshots(cx) {
            let Some(node) = self.ssh_nodes.get(&ide_target.node_id) else {
                continue;
            };
            targets.push(ai_ide_workspace_target_for_node(
                ide_target.tab_id,
                &ide_target.node_id,
                node,
                ide_target.active_editor_tab_id,
                ide_target.project_root_path,
                ide_target.project_name,
            ));
        }

        let tab_host = self.tab_host.read(cx);
        for tab in self.tabs(cx) {
            let Some(root) = tab.root_pane.as_ref() else {
                continue;
            };
            let mut pane_ids = Vec::new();
            root.collect_pane_ids(&mut pane_ids);
            for pane_id in pane_ids {
                let Some(session_id) = root.session_id_for_pane(pane_id) else {
                    continue;
                };
                let Some(pane) = tab_host.panes().get(&pane_id) else {
                    continue;
                };
                let serial_config = self.serial_terminal_configs.get(&session_id);
                let is_serial_terminal = serial_config.is_some();
                let is_local_terminal = tab.kind == TabKind::LocalTerminal;
                let terminal_type = if is_serial_terminal {
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
                let label = if let Some(config) = serial_config {
                    format!("Serial {}", config.port_path)
                } else if is_local_terminal {
                    format!("Local terminal {}", tab.title)
                } else {
                    format!("SSH terminal {}", ai_short_id(&session_id.0.to_string()))
                };
                let metadata = if let Some(config) = serial_config {
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
                        if terminal_running {
                            "connected"
                        } else {
                            "stale"
                        }
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
        let runtime_handles = tool_session_id
            .map(|tool_session_id| {
                targets
                    .iter()
                    .filter_map(|target| {
                        let ide_tab_id = (target.kind == "ide-workspace")
                            .then(|| {
                                target
                                    .refs
                                    .get("surfaceTabId")
                                    .and_then(|value| value.parse::<u64>().ok())
                                    .map(TabId)
                            })
                            .flatten();
                        self.ai_runtime_context
                            .update(cx, |runtime, _cx| {
                                match target.kind.as_str() {
                                    "terminal-session" => {
                                        let session_id = target
                                            .refs
                                            .get("sessionId")
                                            .and_then(|value| value.parse::<u64>().ok())
                                            .map(TerminalSessionId)?;
                                        runtime
                                            .issue_terminal_handle(tool_session_id, session_id)
                                            .ok()
                                    }
                                    "local-shell" => {
                                        runtime.issue_local_shell_handle(tool_session_id).ok()
                                    }
                                    "ssh-node" => {
                                        let node_id = target
                                            .refs
                                            .get("nodeId")
                                            .map(|value| NodeId::new(value.clone()))?;
                                        runtime.issue_node_handle(tool_session_id, &node_id).ok()
                                    }
                                    "sftp-session" => {
                                        let node_id = target
                                            .refs
                                            .get("nodeId")
                                            .map(|value| NodeId::new(value.clone()))?;
                                        runtime.issue_sftp_handle(tool_session_id, &node_id).ok()
                                    }
                                    "ide-workspace" => {
                                        ide_tab_id.and_then(|tab_id| {
                                            runtime.issue_ide_handle(tool_session_id, tab_id).ok()
                                        })
                                    }
                                    "app-surface" => {
                                        let tab_id = target
                                            .refs
                                            .get("tabId")
                                            .and_then(|value| value.parse::<u64>().ok())
                                            .map(TabId)?;
                                        runtime
                                            .issue_app_surface_handle(tool_session_id, tab_id)
                                            .ok()
                                    }
                                    _ => None,
                                }
                            })
                            .map(|handle| (target.id.clone(), handle))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let settings = self.settings_store.settings();
        let active_tab_ref = self.active_tab_id(cx)
            .and_then(|active_tab_id| self.tabs(cx).iter().find(|tab| tab.id == active_tab_id));
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
        let active_tab = self.active_tab_id(cx)
            .and_then(|active_tab_id| {
                self.tabs(cx)
                    .iter()
                    .find(|tab| tab.id == active_tab_id)
                    .map(|tab| {
                        serde_json::json!({
                            "type": ai_tab_kind_label(&tab.kind),
                            "title": tab.title,
                        })
                    })
            });
        let active_node = self.active_ssh_node_id.as_ref().and_then(|node_id| {
            self.ssh_nodes.get(node_id).map(|node| {
                serde_json::json!({
                    "host": node.endpoint.host,
                    "username": node.endpoint.username,
                    "status": match node.readiness {
                        NodeReadiness::Ready => "connected",
                        NodeReadiness::Connecting => "connecting",
                        NodeReadiness::Error => "error",
                        NodeReadiness::Disconnected => "disconnected",
                    },
                })
            })
        });
        let model_visible_settings = ai_model_visible_settings_projection(settings);
        let transfers = ai_transfers_state(&self.sftp_transfer_manager);
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
            "tabs": {
                "open": self.tabs(cx).len(),
                "hasActiveTab": self.active_tab_id(cx).is_some(),
            },
            "terminalRegistry": { "entries": self.tab_host.read(cx).panes().len() },
            "localTerminals": {
                "count": self.visible_local_terminal_session_count(cx) + self.detached_local_terminals.len(),
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
            runtime_handles,
            active_tab,
            active_node,
            active_session_id,
            active_tab_id: self.active_tab_id(cx)
                .map(|tab_id| tab_id.0.to_string()),
            active_node_id,
            memory: ai_memory_settings_json(
                settings.ai.memory.enabled,
                &settings.ai.memory.content,
            ),
            health_state,
            transfers_state: transfers,
            model_visible_settings,
        }
    }

    pub(in crate::workspace) fn ai_model_backend_services(
        &self,
        cx: &App,
    ) -> AiModelBackendServices {
        let ai = self.ai_entity.read(cx);
        let settings = self.settings_store.settings();
        AiModelBackendServices {
            rag_store: ai.rag_store(),
            ai_mcp_registry: ai.mcp_registry().clone(),
            ai_key_store: ai.key_store().clone(),
            ai_providers: settings.ai.providers.clone(),
            ai_embedding_config: settings.ai.embedding_config.clone(),
        }
    }

    pub(in crate::workspace) fn ai_live_tool_services(&self) -> AiLiveToolServices {
        // Application owners are copied only into broker-started tasks after a
        // live handle has passed its final GPUI-thread validation.
        AiLiveToolServices {
            node_router: self.node_router.clone(),
            sftp_transfer_manager: self.sftp_transfer_manager.clone(),
            backend_runtime: self.forwarding_runtime.clone(),
        }
    }

    /// Rebuilds the model-visible authority projection on the GPUI thread for
    /// every provider round. It deliberately owns no transport or pane state.
    pub(in crate::workspace) fn ai_runtime_context_prompt(
        &self,
        tool_session_id: &ToolSessionId,
        cx: &mut Context<Self>,
    ) -> String {
        // Issuing from authoritative owners here makes the first provider
        // round useful without trusting the internal target scan as authority.
        let snapshot =
            self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx);
        let mut stable_resources = ai_app_surface_stable_resources();
        for resource_ref in snapshot.targets.iter().filter_map(ai_stable_resource_ref_for_target) {
            if !stable_resources.contains(&resource_ref) {
                stable_resources.push(resource_ref);
            }
            if stable_resources.len() >= AI_RUNTIME_STABLE_RESOURCE_LIMIT {
                break;
            }
        }
        let live_handles = self
            .ai_runtime_context
            .read(cx)
            .current_handle_projections(tool_session_id);
        let registry_epoch = self.ai_runtime_context.read(cx).registry_epoch();
        let projection = oxideterm_ai::RuntimeContextSnapshot {
            protocol_version: 2,
            snapshot_id: format!("snap_{}", uuid::Uuid::new_v4().simple()),
            observed_at_ms: ai_now_ms(),
            registry_epoch,
            stable_resources,
            live_handles,
        };
        let value = serde_json::json!({
            "runtimeContext": projection,
            "instructions": [
                "Use stable resource_ref only for durable actions such as connecting a saved connection, reading settings or knowledge, and opening an application surface.",
                "Use handle_id only for the current live terminal, local shell, SFTP session, or IDE workspace.",
                "A stale handle must be rediscovered; never substitute a tab, session, node, or target id.",
            ],
        });
        serde_json::to_string_pretty(&value)
            .map(|text| ai_model_safe_runtime_text(&text))
            .unwrap_or_else(|_| "{\"runtimeContext\":{\"protocolVersion\":2}}".to_string())
    }

    pub(in crate::workspace) fn ai_acp_chat_launch(
        &self,
        config: &AiChatStreamConfig,
    ) -> Result<Option<AiAcpChatLaunch>, String> {
        if config.execution_backend != AiExecutionBackend::Acp {
            return Ok(None);
        }
        let agent_id = config
            .acp_agent_id
            .as_deref()
            .filter(|agent_id| !agent_id.trim().is_empty())
            .ok_or_else(|| "No ACP agent selected for this execution profile.".to_string())?;
        let agent = self
            .settings_store
            .settings()
            .ai
            .acp_agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .ok_or_else(|| format!("ACP agent `{agent_id}` is not configured."))?;
        if !agent.enabled {
            return Err(format!("ACP agent `{}` is disabled.", agent.id));
        }

        let agent_id = agent.id.clone();
        let display_name = if agent.display_name.trim().is_empty() {
            agent_id.clone()
        } else {
            agent.display_name.clone()
        };
        let session_cwd = std::env::current_dir().unwrap_or_else(|_| {
            agent
                .cwd
                .as_deref()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        });
        let host_policy = oxideterm_ai::AcpHostCapabilityPolicy {
            fs_read_text_file: agent.capability_policy.fs_read_text_file,
            fs_write_text_file: agent.capability_policy.fs_write_text_file,
            terminal: agent.capability_policy.terminal,
        };
        // Copy token-bearing args and environment values exactly once into the
        // zeroizing launch owner that is moved to the ACP worker.
        let launch_config = oxideterm_ai::AcpLaunchConfig {
            id: agent_id.clone(),
            display_name,
            command: agent.command.clone(),
            args: agent.args.clone(),
            env: agent.env.clone(),
            cwd: agent.cwd.as_deref().map(std::path::PathBuf::from),
        };
        Ok(Some(AiAcpChatLaunch {
            launch_config,
            session_cwd,
            host_policy,
        }))
    }

    pub(in crate::workspace) fn resolve_ai_tool_approval(
        &mut self,
        tool_call_id: String,
        approved: bool,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.resolve_tool_approval(&tool_call_id, approved);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn resolve_ai_acp_permission(
        &mut self,
        tool_call_id: String,
        option_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.resolve_acp_permission_choice(&tool_call_id, option_id);
        });
        self.acp_entity.update(cx, |entity, _cx| {
            entity.remove_file_write_preview(&tool_call_id);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn resolve_ai_tool_candidate_selection(
        &mut self,
        tool_call_id: String,
        selected_index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity.update(cx, |ai, _cx| {
            ai.resolve_tool_candidate_selection(&tool_call_id, selected_index);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn execute_ai_ui_orchestrator_tool(
        &mut self,
        tool_session_id: &ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiExecutedToolResult {
        let started = std::time::Instant::now();
        let current_snapshot =
            self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx);
        let result = match tool_name.as_str() {
            "list_targets" => current_snapshot.list_targets(&args),
            "select_target" => current_snapshot.select_target(&args),
            "connect_target" => self.execute_ai_connect_target(&args, window, cx),
            "run_command" => current_snapshot.fail(
                "Command execution requires the asynchronous runtime broker.",
                "runtime_capability_unavailable",
                "Retry the command through the current tool session.",
                "interactive",
            ),
            "observe_terminal" => {
                self.execute_ai_observe_terminal(tool_session_id, &args, cx)
            }
            "send_terminal_input" => {
                self.execute_ai_send_terminal_input(tool_session_id, &args, window, cx)
            }
            "read_resource" => self.execute_ai_read_stable_resource(&args, cx),
            "write_resource" => self.execute_ai_write_settings_resource(&args, window, cx),
            "transfer_resource" => current_snapshot.fail(
                "SFTP capability is unavailable.",
                "runtime_capability_unavailable",
                "Rediscover current resources after the SFTP capability owner is available.",
                "write",
            ),
            "open_app_surface" => {
                self.execute_ai_open_app_surface(tool_session_id, &args, window, cx)
            }
            "get_state" => self.execute_ai_get_state(tool_session_id, &args, cx),
            "remember_preference" => self.execute_ai_remember_preference(&args, cx),
            "recall_preferences" => {
                let memory_content = ai_memory_trimmed_content(&current_snapshot.memory);
                current_snapshot.ok(
                    if memory_content.is_empty() {
                        "No saved preferences."
                    } else {
                        "Preferences recalled."
                    },
                    if memory_content.is_empty() {
                        "No saved preferences.".to_string()
                    } else {
                        memory_content.to_string()
                    },
                    current_snapshot.memory.clone(),
                    "read",
                )
            }
            _ => current_snapshot.fail(
                "Unknown orchestrator tool.",
                "unknown_tool",
                format!("{tool_name} is not an OxideSens task tool."),
                "read",
            ),
        };
        self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx).to_executed_tool_result(
            tool_call_id,
            tool_name,
            result,
            started.elapsed().as_millis(),
        )
    }

    /// Executes only stable, non-live reads on the UI-owned broker. Live file
    /// and SFTP operations remain unavailable until their real owners expose
    /// capability adapters.
    fn execute_ai_read_stable_resource(
        &self,
        args: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        match ai_stable_resource_operation("read_resource", args) {
            Ok(AiStableResourceOperation::Settings) => {
                let Some(target) = snapshot
                    .targets
                    .iter()
                    .find(|target| target.kind == "settings")
                    .cloned()
                else {
                    return snapshot.fail(
                        "Settings resource is unavailable.",
                        "resource_not_found",
                        "Rediscover the application settings resource before reading it.",
                        "read",
                    );
                };
                let section = args.get("section").and_then(serde_json::Value::as_str);
                let data = section
                    .and_then(|section| snapshot.model_visible_settings.get(section).cloned())
                    .unwrap_or_else(|| snapshot.model_visible_settings.clone());
                snapshot
                    .ok(
                        section
                            .map(|section| format!("Read settings section {section}."))
                            .unwrap_or_else(|| "Read settings.".to_string()),
                        serde_json::to_string_pretty(&data).unwrap_or_default(),
                        data,
                        "read",
                    )
                    .with_target(target)
            }
            Ok(AiStableResourceOperation::Rag) => {
                let Some(target) = snapshot
                    .targets
                    .iter()
                    .find(|target| target.kind == "rag-index")
                    .cloned()
                else {
                    return snapshot.fail(
                        "Knowledge resource is unavailable.",
                        "resource_not_found",
                        "Rediscover the knowledge resource before searching it.",
                        "read",
                    );
                };
                let rag_store = self.ai_entity.read(cx).rag_store();
                let results = oxideterm_ai::rag_search(
                    &rag_store,
                    oxideterm_ai::RagSearchRequest {
                        query: ai_rag_query_arg(args).to_string(),
                        collection_ids: Vec::new(),
                        query_vector: None,
                        top_k: Some(8),
                    },
                );
                match results {
                    Ok(results) => {
                        let data = serde_json::to_value(results).unwrap_or_else(|_| serde_json::json!([]));
                        snapshot
                            .ok(
                                format!(
                                    "Found {} knowledge results.",
                                    data.as_array().map(Vec::len).unwrap_or(0)
                                ),
                                serde_json::to_string_pretty(&data).unwrap_or_default(),
                                data,
                                "read",
                            )
                            .with_target(target)
                    }
                    Err(error) => snapshot
                        .fail(
                            "Knowledge search failed.",
                            "rag_search_error",
                            error,
                            "read",
                        )
                        .with_target(target),
                }
            }
            _ => snapshot.fail(
                "Resource is unavailable.",
                "resource_not_found",
                "Rediscover the resource through the current v2 runtime context.",
                "read",
            ),
        }
    }

    fn execute_ai_get_state(
        &self,
        tool_session_id: &ToolSessionId,
        args: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        if args.get("scope").and_then(serde_json::Value::as_str) != Some("target") {
            return snapshot.get_state(args);
        }
        if args.get("handle_id").is_some() {
            return match self.ai_runtime_context.read(cx).validate_state_handle(
                tool_session_id,
                args.get("handle_id").and_then(serde_json::Value::as_str),
            ) {
                Ok(handle) => snapshot.ok(
                    "Read current target state.",
                    serde_json::to_string_pretty(&handle).unwrap_or_default(),
                    serde_json::to_value(handle).unwrap_or(serde_json::Value::Null),
                    "read",
                ),
                Err(error) => snapshot.fail(
                    "Runtime target is unavailable.",
                    error.public_code(),
                    "Rediscover current targets before retrying.",
                    "read",
                ),
            };
        }

        let Some(resource_ref) = args
            .get("resource_ref")
            .cloned()
            .and_then(|value| {
                serde_json::from_value::<oxideterm_ai::StableResourceRef>(value).ok()
            })
        else {
            return snapshot.fail(
                "Target authority is required.",
                "runtime_handle_missing",
                "Provide one current handle or durable resource reference.",
                "read",
            );
        };
        if resource_ref.kind() == oxideterm_ai::StableResourceKind::SavedConnection
            && !self
                .connection_store
                .connections()
                .iter()
                .any(|connection| connection.id == resource_ref.id())
        {
            return snapshot.fail(
                "Saved resource no longer exists.",
                "resource_removed",
                "Rediscover saved connections before retrying.",
                "read",
            );
        }
        let state = snapshot
            .targets
            .iter()
            .find(|target| {
                ai_stable_resource_ref_for_target(target).as_ref() == Some(&resource_ref)
            })
            .and_then(|target| snapshot.model_target_json(target))
            .unwrap_or_else(|| {
                serde_json::json!({
                    "authority": {
                        "kind": "stable_resource",
                        "resource_ref": resource_ref,
                    },
                    "state": "available",
                })
            });
        snapshot.ok(
            "Read durable target state.",
            serde_json::to_string_pretty(&state).unwrap_or_default(),
            state,
            "read",
        )
    }

    pub(in crate::workspace) fn start_ai_ui_orchestrator_tool_execution(
        &mut self,
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        post_user_approval: bool,
        dangerous_command_approved: bool,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let args = match self.prepare_ai_runtime_authority(&tool_session_id, &tool_name, args, cx) {
            Ok(args) => args,
            Err(error) => {
                let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
                let result = snapshot.to_executed_tool_result(
                    tool_call_id,
                    tool_name,
                    snapshot.fail(
                        "Runtime target is unavailable.",
                        ai_runtime_validation_public_code(&error, post_user_approval),
                        ai_runtime_validation_recovery_message(post_user_approval),
                        "interactive",
                    ),
                    0,
                );
                let _ = sender.send(result);
                return;
            }
        };
        if tool_name == "connect_target" {
            self.start_ai_connect_target_execution(
                tool_session_id,
                tool_call_id,
                tool_name,
                args,
                sender,
                window,
                cx,
            );
            return;
        }
        if matches!(
            tool_name.as_str(),
            "read_resource" | "write_resource" | "transfer_resource"
        ) && args.get("handle_id").is_some()
        {
            self.start_ai_live_resource_execution(
                tool_session_id,
                tool_call_id,
                tool_name,
                args,
                post_user_approval,
                sender,
                cx,
            );
            return;
        }
        if tool_name == "run_command" {
            self.start_ai_terminal_run_command_execution(
                tool_session_id,
                tool_call_id,
                tool_name,
                args,
                post_user_approval,
                dangerous_command_approved,
                sender,
                window,
                cx,
            );
            return;
        }
        let result = self.execute_ai_ui_orchestrator_tool(
            &tool_session_id,
            tool_call_id,
            tool_name,
            args,
            window,
            cx,
        );
        let _ = sender.send(result);
    }

    /// Validates a live resource handle once more after approval, then hands
    /// the real owner adapter to the backend task without exposing internals.
    fn start_ai_live_resource_execution(
        &mut self,
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        post_user_approval: bool,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        cx: &mut Context<Self>,
    ) {
        let started = std::time::Instant::now();
        let operation = match ai_live_resource_operation(&tool_name, &args) {
            Ok(operation) => operation,
            Err(error) => {
                self.send_ai_live_resource_validation_failure(
                    &tool_session_id,
                    tool_call_id,
                    tool_name,
                    sender,
                    error,
                    post_user_approval,
                    started.elapsed().as_millis(),
                    cx,
                );
                return;
            }
        };
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        let (node_id, sftp_owner, ide_file_system) = if operation.requires_ide_owner() {
            let (tab_id, node_id) = match self.ai_runtime_context.read(cx).validate_ide_handle(
                &tool_session_id,
                raw_handle_id,
                operation.capability(),
            ) {
                Ok(owner) => owner,
                Err(error) => {
                    self.send_ai_live_resource_validation_failure(
                        &tool_session_id,
                        tool_call_id,
                        tool_name,
                        sender,
                        error,
                        post_user_approval,
                        started.elapsed().as_millis(),
                        cx,
                    );
                    return;
                }
            };
            let Some(file_system) = self.ide_workspace.read(cx).ai_owner_file_system(tab_id, cx)
            else {
                self.send_ai_live_resource_validation_failure(
                    &tool_session_id,
                    tool_call_id,
                    tool_name,
                    sender,
                    oxideterm_ai::RuntimeValidationError::new(
                        oxideterm_ai::RuntimeValidationFailure::OwnerClosed,
                    ),
                    post_user_approval,
                    started.elapsed().as_millis(),
                    cx,
                );
                return;
            };
            (node_id, None, Some(file_system))
        } else {
            let owner = match self.ai_runtime_context.read(cx).validate_sftp_handle(
                &tool_session_id,
                raw_handle_id,
                operation.capability(),
            ) {
                Ok(node_id) => node_id,
                Err(error) => {
                    self.send_ai_live_resource_validation_failure(
                        &tool_session_id,
                        tool_call_id,
                        tool_name,
                        sender,
                        error,
                        post_user_approval,
                        started.elapsed().as_millis(),
                        cx,
                    );
                    return;
                }
            };
            (owner.node_id.clone(), Some(owner), None)
        };
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
        let services = self.ai_live_tool_services();
        self.forwarding_runtime.spawn(async move {
            let mut sender = sender;
            let operation = async {
                match tool_name.as_str() {
                    "read_resource" => {
                        snapshot
                            .read_live_resource(
                                &services,
                                node_id,
                                sftp_owner,
                                &args,
                                ide_file_system,
                                post_user_approval,
                            )
                            .await
                    }
                    "write_resource" => {
                        snapshot
                            .write_live_resource(
                                &services,
                                node_id,
                                sftp_owner,
                                &args,
                                ide_file_system,
                                post_user_approval,
                            )
                            .await
                    }
                    "transfer_resource" => match sftp_owner {
                        Some(owner) => {
                            snapshot
                                .transfer_live_resource(
                                    &services,
                                    owner,
                                    &args,
                                    post_user_approval,
                                )
                                .await
                        }
                        None => snapshot.fail(
                            "SFTP capability is unavailable.",
                            "runtime_capability_unavailable",
                            "Rediscover the current SFTP session before retrying.",
                            "write",
                        ),
                    },
                    _ => snapshot.fail(
                        "Resource operation is unavailable.",
                        "runtime_capability_unavailable",
                        "Rediscover the current runtime resource before retrying.",
                        "write",
                    ),
                }
            };
            let action = tokio::select! {
                action = operation => Some(action),
                _ = sender.closed() => None,
            };
            if let Some(action) = action {
                let _ = sender.send(snapshot.to_executed_tool_result(
                    tool_call_id,
                    tool_name,
                    action,
                    started.elapsed().as_millis(),
                ));
            }
        });
    }

    fn send_ai_live_resource_validation_failure(
        &mut self,
        tool_session_id: &ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        error: oxideterm_ai::RuntimeValidationError,
        post_user_approval: bool,
        duration_ms: u128,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx);
        let result = snapshot.to_executed_tool_result(
            tool_call_id,
            tool_name,
            snapshot.fail(
                "Runtime resource is unavailable.",
                ai_runtime_validation_public_code(&error, post_user_approval),
                ai_runtime_validation_recovery_message(post_user_approval),
                "write",
            ),
            duration_ms,
        );
        let _ = sender.send(result);
    }

    /// Validates typed stable references or opaque live handles without
    /// translating either authority form back into a legacy target identifier.
    fn prepare_ai_runtime_authority(
        &self,
        tool_session_id: &ToolSessionId,
        tool_name: &str,
        args: serde_json::Value,
        cx: &App,
    ) -> Result<serde_json::Value, oxideterm_ai::RuntimeValidationError> {
        if ai_rejects_legacy_live_target_argument(tool_name, &args) {
            return Err(oxideterm_ai::RuntimeValidationError::new(
                oxideterm_ai::RuntimeValidationFailure::CapabilityUnavailable,
            ));
        }
        if matches!(tool_name, "read_resource" | "write_resource") {
            if args.get("resource_ref").is_some() {
                ai_stable_resource_operation(tool_name, &args)?;
            } else {
                let operation = ai_live_resource_operation(tool_name, &args)?;
                let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
                if operation.requires_ide_owner() {
                    self.ai_runtime_context.read(cx).validate_ide_handle(
                        tool_session_id,
                        raw_handle_id,
                        operation.capability(),
                    )?;
                } else {
                    self.ai_runtime_context.read(cx).validate_sftp_handle(
                        tool_session_id,
                        raw_handle_id,
                        operation.capability(),
                    )?;
                }
            }
            return Ok(args);
        }
        if tool_name == "transfer_resource" {
            let operation = ai_live_resource_operation(tool_name, &args)?;
            let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
            self.ai_runtime_context.read(cx).validate_sftp_handle(
                tool_session_id,
                raw_handle_id,
                operation.capability(),
            )?;
            return Ok(args);
        }
        if tool_name == "open_app_surface" {
            if args.get("handle_id").is_some() {
                self.ai_runtime_context
                    .read(cx)
                    .validate_app_surface_handle(
                        tool_session_id,
                        args.get("handle_id").and_then(serde_json::Value::as_str),
                    )?;
            } else {
                ai_stable_resource_operation(tool_name, &args)?;
            }
            return Ok(args);
        }
        if tool_name == "connect_target" {
            let operation = ai_stable_resource_operation(tool_name, &args)?;
            let AiStableResourceOperation::SavedConnection(resource_ref) = operation else {
                return Err(oxideterm_ai::RuntimeValidationError::new(
                    oxideterm_ai::RuntimeValidationFailure::CapabilityUnavailable,
                ));
            };
            if !self
                .connection_store
                .connections()
                .iter()
                .any(|connection| connection.id == resource_ref.id())
            {
                return Err(oxideterm_ai::RuntimeValidationError::new(
                    oxideterm_ai::RuntimeValidationFailure::CapabilityUnavailable,
                ));
            }
            return Ok(args);
        }
        if tool_name == "get_state"
            && args.get("scope").and_then(serde_json::Value::as_str) == Some("target")
        {
            if args.get("handle_id").is_some() {
                self.ai_runtime_context.read(cx).validate_state_handle(
                    tool_session_id,
                    args.get("handle_id").and_then(serde_json::Value::as_str),
                )?;
            } else {
                let resource_ref = args
                    .get("resource_ref")
                    .cloned()
                    .and_then(|value| {
                        serde_json::from_value::<oxideterm_ai::StableResourceRef>(value).ok()
                    })
                    .ok_or_else(|| {
                        oxideterm_ai::RuntimeValidationError::new(
                            oxideterm_ai::RuntimeValidationFailure::CapabilityUnavailable,
                        )
                    })?;
                if resource_ref.kind() == oxideterm_ai::StableResourceKind::SavedConnection
                    && !self
                        .connection_store
                        .connections()
                        .iter()
                        .any(|connection| connection.id == resource_ref.id())
                {
                    return Err(oxideterm_ai::RuntimeValidationError::new(
                        oxideterm_ai::RuntimeValidationFailure::CapabilityUnavailable,
                    ));
                }
            }
            return Ok(args);
        }
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        match tool_name {
            "run_command" => {
                self.ai_runtime_context
                    .read(cx)
                    .validate_run_command_handle(tool_session_id, raw_handle_id)?;
            }
            "observe_terminal" => {
                self.ai_runtime_context.read(cx).validate_terminal_handle(
                    tool_session_id,
                    raw_handle_id,
                    oxideterm_ai::RuntimeCapability::TerminalObserve,
                )?;
            }
            "send_terminal_input" => {
                self.ai_runtime_context.read(cx).validate_terminal_handle(
                    tool_session_id,
                    raw_handle_id,
                    oxideterm_ai::RuntimeCapability::TerminalSendInput,
                )?;
            }
            _ => {}
        }
        Ok(args)
    }

    /// Validates authority before policy approval. Execution repeats validation
    /// immediately before dispatch to close the approval-time state-change gap.
    pub(in crate::workspace) fn preflight_ai_ui_orchestrator_tool(
        &self,
        tool_session_id: &ToolSessionId,
        tool_name: &str,
        args: &serde_json::Value,
        cx: &App,
    ) -> Result<(), oxideterm_ai::RuntimeValidationError> {
        if matches!(
            tool_name,
            "connect_target"
                | "run_command"
                | "observe_terminal"
                | "send_terminal_input"
                | "read_resource"
                | "write_resource"
                | "transfer_resource"
                | "open_app_surface"
                | "get_state"
        ) {
            self.prepare_ai_runtime_authority(tool_session_id, tool_name, args.clone(), cx)
                .map(|_| ())
        } else {
            Ok(())
        }
    }

    pub(in crate::workspace) fn start_ai_connect_target_execution(
        &mut self,
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let started = std::time::Instant::now();
        let base = self.execute_ai_ui_orchestrator_tool(
            &tool_session_id,
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
        if base
            .envelope
            .get("summary")
            .and_then(serde_json::Value::as_str)
            == Some("Target is already live.")
        {
            let _ = sender.send(base);
            return;
        }
        if let Some(ready) = self.ai_connect_target_ready_result(
            &tool_session_id,
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
                        &tool_session_id,
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
                        &tool_session_id,
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

    pub(in crate::workspace) fn execute_ai_connect_target(
        &mut self,
        args: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let resource_ref = match ai_stable_resource_operation("connect_target", args) {
            Ok(AiStableResourceOperation::SavedConnection(resource_ref)) => resource_ref,
            _ => {
                return snapshot.fail(
                    "Saved connection is unavailable.",
                    "resource_not_found",
                    "Rediscover the saved connection before connecting.",
                    "write",
                );
            }
        };
        let target = snapshot
            .targets
            .iter()
            .find(|target| {
                target.kind == "saved-connection"
                    && target.refs.get("connectionId").is_some_and(|id| id == resource_ref.id())
            })
            .cloned();
        let Some(connection) = self.connection_store.get(resource_ref.id()).cloned() else {
            return snapshot.fail(
                "Saved connection was removed.",
                "resource_removed",
                "The saved connection no longer exists. Rediscover available connections.",
                "write",
            );
        };
        let Some(config) = oxideterm_session_adapter::ssh_config_from_saved_connection(
            &self.connection_store,
            self.settings_store.settings(),
            &connection,
        ) else {
            if self.try_reuse_active_saved_connection_terminal(
                resource_ref.id(),
                &connection,
                window,
                cx,
            ) {
                return snapshot
                    .ok(
                        "Target is already live.",
                        "Focused the existing SSH terminal.",
                        serde_json::json!({ "resourceRef": resource_ref }),
                        "write",
                    )
                    .with_optional_target(target);
            }
            return snapshot
                .fail(
                    "Saved connection cannot be opened.",
                    "credential_interaction_required",
                    "The saved connection needs valid SSH configuration or credentials.",
                    "write",
                )
                .with_optional_target(target);
        };
        let title = if connection.name.trim().is_empty() {
            format!("{}@{}", connection.username, connection.host)
        } else {
            connection.name.clone()
        };
        self.start_saved_connection_flow(
            resource_ref.id().to_string(),
            config,
            title,
            window,
            cx,
        );
        snapshot
            .ok(
                "Connection requested.",
                "The saved connection flow has started.",
                serde_json::json!({ "resourceRef": resource_ref }),
                "write",
            )
            .with_optional_target(target)
    }

    pub(in crate::workspace) fn execute_ai_observe_terminal(
        &self,
        tool_session_id: &ToolSessionId,
        args: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        let session_id = match self.ai_runtime_context.read(cx).validate_terminal_handle(
            tool_session_id,
            raw_handle_id,
            oxideterm_ai::RuntimeCapability::TerminalObserve,
        ) {
            Ok(session_id) => session_id,
            Err(error) => {
                return snapshot.fail(
                    "Runtime terminal is unavailable.",
                    error.public_code(),
                    "Rediscover the current terminal before observing it.",
                    "read",
                );
            }
        };
        let target = snapshot
            .targets
            .iter()
            .find(|target| {
                target.kind == "terminal-session"
                    && target
                        .refs
                        .get("sessionId")
                        .is_some_and(|value| value == &session_id.0.to_string())
            })
            .cloned();
        let Some(target_snapshot) = target.as_ref() else {
            return snapshot.fail(
                "Runtime terminal is unavailable.",
                "runtime_owner_closed",
                "The terminal pane is no longer registered.",
                "read",
            );
        };
        let max_chars = args
            .get("max_chars")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(4000) as usize;
        let output = trim_tail_chars(
            target_snapshot.terminal_buffer.as_deref().unwrap_or_default(),
            max_chars,
        );
        snapshot
            .ok(
                "Terminal observed.",
                output.clone(),
                serde_json::json!({
                    "buffer": output,
                    "screen": target_snapshot.terminal_screen.clone().unwrap_or_else(|| serde_json::json!({ "lines": [] })),
                    "waitingForInput": looks_waiting_for_input(target_snapshot.terminal_buffer.as_deref().unwrap_or_default()),
                }),
                "read",
            )
            .with_optional_target(target)
    }

    pub(in crate::workspace) fn execute_ai_send_terminal_input(
        &mut self,
        tool_session_id: &ToolSessionId,
        args: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        let session_id = match self.ai_runtime_context.read(cx).validate_terminal_handle(
            tool_session_id,
            raw_handle_id,
            oxideterm_ai::RuntimeCapability::TerminalSendInput,
        ) {
            Ok(session_id) => session_id,
            Err(error) => {
                return snapshot.fail(
                    "Runtime terminal is unavailable.",
                    error.public_code(),
                    "Rediscover the current terminal before sending input.",
                    "interactive",
                );
            }
        };
        let target = snapshot
            .targets
            .iter()
            .find(|target| {
                target.kind == "terminal-session"
                    && target
                        .refs
                        .get("sessionId")
                        .is_some_and(|value| value == &session_id.0.to_string())
            })
            .cloned();
        if args.get("control").and_then(serde_json::Value::as_str).is_some() {
            return snapshot
                .fail(
                    "Terminal control input is not available through this tool.",
                    "terminal_control_disabled",
                    "Use run_command for shell commands. This tool sends only literal interactive input or Enter.",
                    "interactive",
                )
                .with_optional_target(target);
        }
        let Some((_pane_id, pane)) = self.reveal_ai_terminal_session(session_id, window, cx) else {
            return snapshot
                .fail(
                    "Runtime terminal is unavailable.",
                    "runtime_owner_closed",
                    "The terminal pane is no longer registered.",
                    "interactive",
                )
                .with_optional_target(target);
        };
        // Interactive input can contain passwords; wipe the assembled payload
        // immediately after it crosses into the terminal owner.
        let payload = zeroize::Zeroizing::new(ai_terminal_input_payload(args));
        if payload.is_empty() {
            return snapshot
                .fail(
                    "No terminal input specified.",
                    "missing_terminal_input",
                    "Provide text or request Enter with append_enter.",
                    "interactive",
                )
                .with_optional_target(target);
        }
        if !pane.read(cx).ai_accepts_input() {
            return snapshot
                .fail(
                    "Failed to send terminal input.",
                    "terminal_send_failed",
                    "The terminal writer is no longer available.",
                    "interactive",
                )
                .with_optional_target(target);
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
            .with_optional_target(target)
    }


    pub(in crate::workspace) fn start_ai_terminal_run_command_execution(
        &mut self,
        tool_session_id: ToolSessionId,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        post_user_approval: bool,
        dangerous_command_approved: bool,
        sender: tokio::sync::oneshot::Sender<AiExecutedToolResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let started = std::time::Instant::now();
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        let handle_id = raw_handle_id
            .and_then(|value| oxideterm_ai::RuntimeHandleId::parse(value.to_string()).ok());
        let owner = match self
            .ai_runtime_context
            .read(cx)
            .validate_run_command_handle(
                &tool_session_id,
                handle_id.as_ref().map(oxideterm_ai::RuntimeHandleId::as_str),
            )
        {
            Ok(owner) => owner,
            Err(error) => {
                let result = snapshot.to_executed_tool_result(
                    tool_call_id,
                    tool_name,
                    snapshot.fail(
                        "Runtime command target is unavailable.",
                        ai_runtime_validation_public_code(&error, post_user_approval),
                        ai_runtime_validation_recovery_message(post_user_approval),
                        ai_run_command_preflight_risk(),
                    ),
                    started.elapsed().as_millis(),
                );
                let _ = sender.send(result);
                return;
            }
        };
        let Some(command) = args
            .get("command")
            .and_then(serde_json::Value::as_str)
            .filter(|command| !command.trim().is_empty())
            .map(|command| zeroize::Zeroizing::new(command.to_string()))
        else {
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
        if owner == crate::workspace::ai_runtime_context::AiRunCommandOwner::LocalShell {
            let cwd = args
                .get("cwd")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let timeout_secs = args
                .get("timeout_secs")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(30);
            self.forwarding_runtime.spawn(async move {
                let mut sender = sender;
                let action = tokio::select! {
                    action = run_local_ai_command(
                        &command,
                        cwd.as_deref(),
                        timeout_secs,
                        dangerous_command_approved,
                    ) => Some(action),
                    _ = sender.closed() => None,
                };
                if let Some(action) = action {
                    let _ = sender.send(snapshot.to_executed_tool_result(
                        tool_call_id,
                        tool_name,
                        action,
                        started.elapsed().as_millis(),
                    ));
                }
            });
            return;
        }
        let crate::workspace::ai_runtime_context::AiRunCommandOwner::Terminal(session_id) = owner
        else {
            unreachable!("local shell command returned before terminal dispatch");
        };
        let target = snapshot
            .targets
            .iter()
            .find(|target| {
                target.kind == "terminal-session"
                    && target
                        .refs
                        .get("sessionId")
                        .is_some_and(|value| value == &session_id.0.to_string())
            })
            .cloned();
        let command = zeroize::Zeroizing::new(ai_command_with_cwd(
            &command,
            args.get("cwd").and_then(serde_json::Value::as_str),
        ));
        let Some((_pane_id, pane)) = self.reveal_ai_terminal_session(session_id, window, cx) else {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot
                    .fail(
                        "Runtime terminal is unavailable.",
                        "runtime_owner_closed",
                        "The terminal pane is no longer registered.",
                        "interactive",
                    )
                    .with_optional_target(target),
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
                        "The terminal writer is no longer available.",
                        "interactive",
                    )
                    .with_optional_target(target),
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
        if args.get("await_output").and_then(serde_json::Value::as_bool) == Some(false) {
            let result = snapshot.to_executed_tool_result(
                tool_call_id,
                tool_name,
                snapshot
                    .ok(
                        "Command sent to terminal.",
                        "Command sent to the visible terminal.",
                        serde_json::json!({
                            "executionState": "sent",
                            "visibleInTerminal": true,
                        }),
                        "interactive",
                    )
                    .with_optional_target(target),
                started.elapsed().as_millis(),
            );
            let _ = sender.send(result);
            return;
        }

        cx.spawn(async move |weak, cx| {
            let mut sender = Some(sender);
            let mut last = before.clone();
            let mut changed_at = std::time::Instant::now();
            let mut owner_closed = false;
            for _ in 0..300 {
                if sender.as_ref().is_none_or(tokio::sync::oneshot::Sender::is_closed) {
                    return;
                }
                Timer::after(Duration::from_millis(100)).await;
                let current = weak.update(cx, |this, cx| {
                    // Do not let the retained pane entity become authority
                    // after its terminal owner or tool session is revoked.
                    let current_session = this
                        .ai_runtime_context
                        .read(cx)
                        .validate_terminal_handle(
                            &tool_session_id,
                            handle_id
                                .as_ref()
                                .map(oxideterm_ai::RuntimeHandleId::as_str),
                            oxideterm_ai::RuntimeCapability::TerminalRunCommand,
                        )
                        .ok();
                    (current_session == Some(session_id))
                        .then(|| pane.read(cx).ai_buffer_snapshot())
                });
                let current = match current {
                    Ok(Some(current)) => current,
                    Ok(None) => {
                        owner_closed = true;
                        break;
                    }
                    Err(_) => break,
                };
                if current != last {
                    last = current.clone();
                    changed_at = std::time::Instant::now();
                }
                if current != before && changed_at.elapsed() >= Duration::from_millis(400) {
                    let result = weak.update(cx, |this, cx| {
                        let current_snapshot = this
                            .ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
                        current_snapshot.to_executed_tool_result(
                            tool_call_id.clone(),
                            tool_name.clone(),
                            current_snapshot
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
                                .with_optional_target(target.clone()),
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
                let current_snapshot =
                    this.ai_orchestrator_snapshot_for_tool_session(Some(&tool_session_id), cx);
                let output = terminal_delta_output(&before, &last);
                let output_empty = output.trim().is_empty();
                if owner_closed {
                    return current_snapshot.to_executed_tool_result(
                        tool_call_id,
                        tool_name,
                        current_snapshot
                            .fail(
                                "Runtime terminal changed while waiting for output.",
                                if post_user_approval {
                                    "runtime_state_changed_after_approval"
                                } else {
                                    "runtime_owner_closed"
                                },
                                ai_runtime_validation_recovery_message(post_user_approval),
                                "interactive",
                            )
                            .with_optional_target(target),
                        started.elapsed().as_millis(),
                    );
                }
                current_snapshot.to_executed_tool_result(
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
                        error_code: output_empty
                            .then(|| "terminal_command_wait_timeout".to_string()),
                        error_message: output_empty.then(|| {
                            "No new output was captured before the command wait timed out.".to_string()
                        }),
                        risk: "interactive",
                        target,
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

    pub(in crate::workspace) fn execute_ai_write_settings_resource(
        &mut self,
        args: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        if !matches!(
            ai_stable_resource_operation("write_resource", args),
            Ok(AiStableResourceOperation::Settings)
        ) {
            return snapshot.fail(
                "Settings resource is unavailable.",
                "resource_not_found",
                "Rediscover the application settings resource before updating it.",
                "write",
            );
        }
        let Some(target) = snapshot
            .targets
            .iter()
            .find(|target| target.kind == "settings")
            .cloned()
        else {
            return snapshot.fail(
                "Settings resource is unavailable.",
                "resource_not_found",
                "Rediscover the application settings resource before updating it.",
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
        let value = args
            .get("value")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if args
            .get("dry_run")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            return snapshot
                .ok(
                    format!("Dry-run settings write {section}.{key}."),
                    "Dry-run only; settings were not changed.",
                    serde_json::json!({ "section": section, "key": key, "value": value }),
                    "write",
                )
                .with_target(target)
                .with_verified(false);
        }
        match settings_with_json_patch(self.settings_store.settings(), section, key, value.clone())
        {
            Ok(next_settings) => {
                self.edit_settings(|settings| *settings = next_settings, cx);
                let target_tab =
                    oxideterm_gpui_settings_view::settings_tab_from_ai_section(section);
                let target_terminal_page =
                    oxideterm_gpui_settings_view::terminal_settings_page_from_ai_section(section);
                self.settings_workspace.update(cx, |settings, cx| {
                    if let Some(tab) = target_tab {
                        settings.set_active_tab(tab, cx);
                    }
                    if let Some(page) = target_terminal_page {
                        settings.set_terminal_page(page, cx);
                    }
                });
                self.open_settings_tab(window, cx);
                snapshot
                    .ok(
                        format!("Updated settings {section}.{key}."),
                        format!("{section}.{key} updated."),
                        serde_json::json!({
                            "section": section,
                            "key": key,
                            "value": value,
                            "visibleSurface": "settings",
                        }),
                        "write",
                    )
                    .with_target(target)
            }
            Err(error) => snapshot
                .fail(
                    "Settings section cannot be updated.",
                    "unsupported_settings_section",
                    error,
                    "write",
                )
                .with_target(target),
        }
    }

    pub(in crate::workspace) fn execute_ai_remember_preference(
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
        if !oxideterm_ai::preference_is_safe_to_persist(preference) {
            return snapshot.fail(
                "Preference was not saved.",
                "memory_content_rejected",
                "Long-term memory cannot store credentials or one-time task instructions.",
                "write",
            );
        }
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



    pub(in crate::workspace) fn execute_ai_open_app_surface(
        &mut self,
        tool_session_id: &ToolSessionId,
        args: &serde_json::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AiActionResultLite {
        let snapshot = self.ai_orchestrator_snapshot(cx);
        if let Some(raw_handle_id) = args
            .get("handle_id")
            .and_then(serde_json::Value::as_str)
        {
            let tab_id = match self
                .ai_runtime_context
                .read(cx)
                .validate_app_surface_handle(tool_session_id, Some(raw_handle_id))
            {
                Ok(tab_id) => tab_id,
                Err(error) => {
                    return snapshot.fail(
                        "Application surface is unavailable.",
                        error.public_code(),
                        "Rediscover current application surfaces before retrying.",
                        "write",
                    );
                }
            };
            if !self.tabs(cx).iter().any(|tab| tab.id == tab_id) {
                return snapshot.fail(
                    "Application surface is unavailable.",
                    "runtime_owner_closed",
                    "The selected surface is no longer open.",
                    "write",
                );
            }
            if self.tab_host.read(cx).is_outside_main_window(tab_id) {
                self.focus_detached_tab_window(tab_id, cx);
            } else {
                self.set_main_window_active_tab(Some(tab_id), cx);
                self.sync_active_tab_surface(cx);
                self.focus_active_pane(window, cx);
                self.reveal_active_tab(window, cx);
            }
            cx.notify();
            return snapshot.ok(
                "Focused application surface.",
                "Focused the selected application surface.",
                serde_json::Value::Null,
                "write",
            );
        }
        let surface = match ai_stable_resource_operation("open_app_surface", args) {
            Ok(AiStableResourceOperation::AppSurface(surface)) => surface,
            _ => {
                return snapshot.fail(
                    "Application surface is unavailable.",
                    "resource_not_found",
                    "Rediscover the application surface before opening it.",
                    "write",
                );
            }
        };
        // Opening a durable surface is not the same as focusing an existing live tab.
        let target: Option<AiOrchestratorTarget> = None;

        match surface.as_str() {
            "local_terminal" | "terminal" => match self.create_local_terminal_tab(window, cx) {
                Ok(()) => {
                    let active_tab_id = self.active_tab_id(cx)
                        .map(|tab_id| tab_id.0.to_string());
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
                            "Opened local terminal.",
                            serde_json::json!({ "surface": surface }),
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
                if let Some(section) = args.get("section").and_then(serde_json::Value::as_str) {
                    let target_tab =
                        oxideterm_gpui_settings_view::settings_tab_from_ai_section(section);
                    let target_terminal_page =
                        oxideterm_gpui_settings_view::terminal_settings_page_from_ai_section(
                            section,
                        );
                    self.settings_workspace.update(cx, |settings, cx| {
                        if let Some(tab) = target_tab {
                            settings.set_active_tab(tab, cx);
                        }
                        if let Some(page) = target_terminal_page {
                            settings.set_terminal_page(page, cx);
                        }
                    });
                }
                self.open_settings_tab(window, cx);
                snapshot
                    .ok(
                        "Opened settings.",
                        "Opened settings.",
                        serde_json::Value::Null,
                        "write",
                    )
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
                        "Opened runtime overview.",
                        "Opened runtime overview.",
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
                            "Connect an SSH target first, then rediscover the current SFTP surface.",
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
                    .ok(
                        "Opened sftp.",
                        "Opened sftp.",
                        serde_json::Value::Null,
                        "write",
                    )
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
                            "Connect an SSH target first, then rediscover the current IDE surface.",
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
                    .ok(
                        "Opened ide.",
                        "Opened ide.",
                        serde_json::Value::Null,
                        "write",
                    )
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


    pub(in crate::workspace) fn reveal_ai_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(PaneId, gpui::Entity<oxideterm_gpui_terminal::TerminalPane>)> {
        let location = self.tab_host.read(cx).terminal_location(session_id)?;
        let tab_id = location.tab_id;
        let pane_id = location.pane_id;
        let pane = self.tab_host.read(cx).panes().get(&pane_id)?.clone();

        if self.tab_host.read(cx).is_outside_main_window(tab_id) {
            // The detached window already owns this pane entity. Focus that
            // owner without mounting the same terminal into the main window.
            self.focus_detached_tab_window(tab_id, cx);
            return Some((pane_id, pane));
        }

        // AI terminal tools must act on the same pane the user can see. The
        // model may target a non-active session from context, so make that tab
        // and pane visible before writing input or reading command output.
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.tab_host.update(cx, |tab_host, _| {
            tab_host.set_active_pane(Some(tab_id), pane_id);
        });
        self.sync_active_tab_surface(cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window, cx);
        cx.notify();

        Some((pane_id, pane))
    }

    pub(in crate::workspace) fn ai_connect_target_ready_result(
        &mut self,
        tool_session_id: &ToolSessionId,
        tool_call_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        duration_ms: u128,
        cx: &mut Context<Self>,
    ) -> Option<AiExecutedToolResult> {
        let AiStableResourceOperation::SavedConnection(resource_ref) =
            ai_stable_resource_operation("connect_target", args).ok()?
        else {
            return None;
        };
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx);
        let ready_targets = snapshot
            .targets
            .iter()
            .filter(|target| {
                matches!(target.kind.as_str(), "ssh-node" | "terminal-session")
                    && target.state == "connected"
                    && target
                        .refs
                        .get("connectionId")
                        .is_some_and(|id| id == resource_ref.id())
            })
            .cloned()
            .collect::<Vec<_>>();
        let primary = ready_targets
            .iter()
            .find(|target| target.kind == "terminal-session")
            .or_else(|| ready_targets.first())?
            .clone();
        let label = self
            .connection_store
            .get(resource_ref.id())
            .map(|connection| connection.name.trim())
            .filter(|label| !label.is_empty())
            .unwrap_or("saved connection");
        Some(snapshot.to_executed_tool_result(
            tool_call_id.to_string(),
            tool_name.to_string(),
            snapshot
                .ok(
                    format!("Connected {label}."),
                    "A live terminal is ready.",
                    serde_json::json!({ "resourceRef": resource_ref }),
                    "write",
                )
                .with_target(primary)
                .with_targets(ready_targets),
            duration_ms,
        ))
    }

    pub(in crate::workspace) fn ai_connect_target_timeout_result(
        &mut self,
        tool_session_id: &ToolSessionId,
        tool_call_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        _base: &AiExecutedToolResult,
        duration_ms: u128,
        cx: &mut Context<Self>,
    ) -> AiExecutedToolResult {
        let snapshot = self.ai_orchestrator_snapshot_for_tool_session(Some(tool_session_id), cx);
        let target = match ai_stable_resource_operation("connect_target", args) {
            Ok(AiStableResourceOperation::SavedConnection(resource_ref)) => snapshot
                .targets
                .iter()
                .find(|target| {
                    target.kind == "saved-connection"
                        && target
                            .refs
                            .get("connectionId")
                            .is_some_and(|id| id == resource_ref.id())
                })
                .cloned(),
            _ => None,
        };
        let next_actions = target
            .as_ref()
            .map(|target| {
                vec![serde_json::json!({
                    "action": "select_target",
                    "args": { "query": target.label },
                    "reason": "Re-select the saved connection after credentials are updated."
                })]
            })
            .unwrap_or_default();
        snapshot.to_executed_tool_result(
            tool_call_id.to_string(),
            tool_name.to_string(),
            snapshot
                .fail(
                    "Connection did not complete.",
                    "connect_failed",
                    "The saved connection flow did not return a live terminal.",
                    "write",
                )
                .with_optional_target(target)
                .with_next_actions(next_actions),
            duration_ms,
        )
    }
}

fn ai_runtime_validation_public_code(
    error: &oxideterm_ai::RuntimeValidationError,
    post_user_approval: bool,
) -> &'static str {
    if post_user_approval {
        "runtime_state_changed_after_approval"
    } else {
        error.public_code()
    }
}

fn ai_runtime_validation_recovery_message(post_user_approval: bool) -> &'static str {
    if post_user_approval {
        "Nothing was executed because the live target changed while approval was open. Rediscover it before retrying."
    } else {
        "Rediscover the current runtime target before retrying."
    }
}
