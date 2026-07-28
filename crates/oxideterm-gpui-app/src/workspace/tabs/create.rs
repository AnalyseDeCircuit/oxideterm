use super::*;

fn attach_saved_owner_to_reused_ssh_node(
    node: &mut WorkspaceSshNode,
    saved_connection_id: &str,
) -> bool {
    // Saved SSH credentials are node-owned. Attach an owner only when the user
    // reached this node through an explicit saved-connection action.
    let saved_connection_id = saved_connection_id.trim();
    if saved_connection_id.is_empty() || node.saved_connection_id.is_some() {
        return false;
    }
    node.saved_connection_id = Some(saved_connection_id.to_string());
    true
}

fn saved_node_route_matches_config(
    node_router: &NodeRouter,
    node_id: &NodeId,
    requested_config: &SshConfig,
) -> bool {
    let Ok(path) = node_router.path_to_node(node_id) else {
        return false;
    };
    let proxy_hops = requested_config.proxy_chain.as_deref().unwrap_or_default();
    if path.len() != proxy_hops.len() + 1 {
        return false;
    }

    // A saved-connection index is reusable only while every routed endpoint
    // still matches the current saved profile. Authentication and host-key
    // overrides may change without changing which remote node the path owns.
    path.iter().enumerate().all(|(index, node_id)| {
        let Some(actual) = node_router.node_metadata(node_id) else {
            return false;
        };
        let (expected_host, expected_port, expected_username) = proxy_hops
            .get(index)
            .map(|hop| (hop.host.as_str(), hop.port, hop.username.as_str()))
            .unwrap_or((
                requested_config.host.as_str(),
                requested_config.port,
                requested_config.username.as_str(),
            ));
        actual.host == expected_host
            && actual.port == expected_port
            && actual.username == expected_username
    })
}

impl WorkspaceApp {
    pub(in crate::workspace) fn create_local_terminal_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let terminal_config = self.local_terminal_config();
        let title = self.local_terminal_tab_title();
        self.create_local_terminal_tab_with_config(terminal_config, title, window, cx)
    }

    pub(in crate::workspace) fn create_local_terminal_tab_with_config(
        &mut self,
        terminal_config: LocalPtyConfig,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        // Shell Launcher selections must use the same tab and pane lifecycle as
        // the default-shell shortcut; only the PTY configuration differs.
        let tab_id = self.alloc_tab_id(cx);
        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        let preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        let pane = cx.new(|cx| {
            TerminalPane::new_local_with_config_and_preferences(
                terminal_config,
                preferences,
                window,
                cx,
            )
            .expect("failed to initialize terminal pane")
        });

        self.register_terminal_pane(pane_id, session_id, pane.clone(), window, cx);
        self.refresh_native_plugin_terminal_hooks(cx);
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title,
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(pane_id, session_id)),
            active_pane_id: Some(pane_id),
        });
        self.bind_terminal_location(tab_id, pane_id, session_id, cx);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.update(cx, |pane, cx| pane.focus(window, cx));
        self.reveal_active_tab(window, cx);
        cx.notify();
        Ok(())
    }

    pub(in crate::workspace) fn create_telnet_terminal_tab(
        &mut self,
        config: TelnetSessionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let tab_id = self.alloc_tab_id(cx);
        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        let preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        let title = format!("Telnet {}", config.endpoint_label());
        let pane_config = config;
        let pane = cx.new(|cx| {
            TerminalPane::new_telnet_with_preferences(pane_config, preferences, window, cx)
                .expect("failed to initialize Telnet terminal pane")
        });

        // Telnet is a local transport in the plugin API: it owns no SSH node,
        // but it still participates in the normal tab/pane/session registry.
        self.register_terminal_pane(pane_id, session_id, pane.clone(), window, cx);
        self.refresh_native_plugin_terminal_hooks(cx);
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title,
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(pane_id, session_id)),
            active_pane_id: Some(pane_id),
        });
        self.bind_terminal_location(tab_id, pane_id, session_id, cx);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.update(cx, |pane, cx| pane.focus(window, cx));
        self.reveal_active_tab(window, cx);
        cx.notify();
        Ok(session_id)
    }

    pub(in crate::workspace) fn create_serial_terminal_tab(
        &mut self,
        config: SerialSessionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let tab_id = self.alloc_tab_id(cx);
        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        let preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        let title = format!("Serial {}", config.port_path);
        let pane_config = config.clone();
        let pane = cx.new(|cx| {
            TerminalPane::new_serial_with_preferences(pane_config, preferences, window, cx)
                .expect("failed to initialize Serial terminal pane")
        });

        // Serial mirrors Tauri local-terminal transport semantics: it is not
        // an SSH node and must not expose SFTP, forwarding, or ProxyJump.
        self.register_terminal_pane(pane_id, session_id, pane.clone(), window, cx);
        self.serial_terminal_configs.insert(session_id, config);
        self.refresh_native_plugin_terminal_hooks(cx);
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title,
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(pane_id, session_id)),
            active_pane_id: Some(pane_id),
        });
        self.bind_terminal_location(tab_id, pane_id, session_id, cx);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.update(cx, |pane, cx| pane.focus(window, cx));
        self.reveal_active_tab(window, cx);
        cx.notify();
        Ok(session_id)
    }

    pub(in crate::workspace) fn open_or_create_saved_ssh_terminal_tab(
        &mut self,
        saved_connection_id: String,
        config: SshConfig,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let indexed_node_id = self.saved_ssh_nodes.get(&saved_connection_id).cloned();
        if let Some(node_id) = indexed_node_id.clone().filter(|node_id| {
            saved_node_route_matches_config(&self.node_router, node_id, &config)
                && self.ssh_nodes.get(node_id).is_some_and(|node| {
                    node.endpoint.host == config.host
                        && node.endpoint.port == config.port
                        && node.endpoint.username == config.username
                })
        }) {
            self.associate_existing_node_with_saved_connection(&node_id, &saved_connection_id);
            if let Some(session_id) = self
                .workspace_runtime
                .read(cx)
                .ssh_terminal_session_ids_for_node(&node_id)
                .first()
                .copied()
                && self.focus_terminal_session(session_id, window, cx)
            {
                let _ = self.connection_store.mark_used(&saved_connection_id);
                return Ok(());
            }
            if self.ssh_nodes.contains_key(&node_id) {
                let node_config = self
                    .config_with_host_key_acceptance_for_node(&node_id, &config)
                    .or_else(|| {
                        self.node_router
                            .node_runtime_snapshot(&node_id)
                            .map(|snapshot| snapshot.config)
                    })
                    .ok_or_else(|| {
                        anyhow::anyhow!("SSH node {} has no runtime config", node_id.0)
                    })?;
                // Tauri passes the saved connection's current post-connect command
                // to createTerminalForNode even when the live node already exists.
                let post_connect_command = config.post_connect_command.clone();
                self.queue_ssh_terminal_tab_for_node_with_mark_used(
                    node_id,
                    post_connect_command,
                    node_config,
                    title,
                    Some(saved_connection_id.clone()),
                    Some(saved_connection_id.clone()),
                    None,
                    window,
                    cx,
                )?;
                return Ok(());
            }
        }
        if indexed_node_id.is_some() {
            // Keep an already-running historical node alive for its existing
            // consumers, but never let a stale saved-profile index route a new tab.
            self.saved_ssh_nodes.remove(&saved_connection_id);
        }

        if config
            .proxy_chain
            .as_ref()
            .is_some_and(|chain| !chain.is_empty())
        {
            // Tauri does not represent a saved proxy chain as one target node
            // with an embedded proxy_chain. It expands each hop into the
            // SessionTree and then connects the target through its ancestors.
            let expansion =
                self.expand_saved_connection_tree(&saved_connection_id, config, title.clone())?;
            let target_config = self
                .node_router
                .node_runtime_snapshot(&expansion.target_node_id)
                .map(|snapshot| snapshot.config)
                .ok_or_else(|| anyhow::anyhow!("target node was not materialized"))?;
            let target_node_id = expansion.target_node_id;
            let post_connect_command = target_config.post_connect_command.clone();
            self.queue_ssh_terminal_tab_for_node_with_mark_used(
                target_node_id,
                post_connect_command,
                target_config,
                title,
                Some(saved_connection_id.clone()),
                Some(saved_connection_id.clone()),
                None,
                window,
                cx,
            )?;
            return Ok(());
        } else {
            if let Some(existing_node_id) = self.existing_direct_root_node_for_saved_config(&config)
            {
                self.ensure_workspace_ssh_node_from_runtime(&existing_node_id);
                self.associate_existing_node_with_saved_connection(
                    &existing_node_id,
                    &saved_connection_id,
                );
                if let Some(session_id) = self
                    .workspace_runtime
                    .read(cx)
                    .ssh_terminal_session_ids_for_node(&existing_node_id)
                    .first()
                    .copied()
                    && self.focus_terminal_session(session_id, window, cx)
                {
                    let _ = self.connection_store.mark_used(&saved_connection_id);
                    return Ok(());
                }
                if let Some((node_title, node_saved_connection_id)) = self
                    .ssh_nodes
                    .get(&existing_node_id)
                    .map(|node| (node.title.clone(), node.saved_connection_id.clone()))
                {
                    // This branch reuses a root node only because the user
                    // explicitly opened this saved connection. The association
                    // above gives every terminal on that reused node one owner
                    // without letting sudo helper code infer one from host text.
                    let node_config = self
                        .config_with_host_key_acceptance_for_node(&existing_node_id, &config)
                        .or_else(|| {
                            self.node_router
                                .node_runtime_snapshot(&existing_node_id)
                                .map(|snapshot| snapshot.config)
                        })
                        .ok_or_else(|| {
                            anyhow::anyhow!("SSH node {} has no runtime config", existing_node_id.0)
                        })?;
                    // Tauri reuses the existing direct root node but still
                    // applies the saved connection's current terminal command.
                    let post_connect_command = config.post_connect_command.clone();
                    self.queue_ssh_terminal_tab_for_node_with_mark_used(
                        existing_node_id,
                        post_connect_command,
                        node_config,
                        node_title,
                        node_saved_connection_id,
                        Some(saved_connection_id.clone()),
                        None,
                        window,
                        cx,
                    )?;
                    return Ok(());
                }
            }
            let node_id = self.materialize_ssh_root_node(
                config.clone(),
                title.clone(),
                Some(saved_connection_id.clone()),
            );
            let cleanup_node_id = node_id.clone();
            let post_connect_command = config.post_connect_command.clone();
            let result = self.queue_ssh_terminal_tab_for_node_with_mark_used(
                node_id,
                post_connect_command,
                config,
                title,
                Some(saved_connection_id.clone()),
                Some(saved_connection_id.clone()),
                None,
                window,
                cx,
            );
            if result.is_ok() {
                self.workspace_runtime.update(cx, |runtime, _cx| {
                    runtime.mark_pending_ssh_terminal_open_cleanup(
                        &cleanup_node_id,
                        cleanup_node_id.clone(),
                    );
                });
            }
            result
        }
    }

    fn associate_existing_node_with_saved_connection(
        &mut self,
        node_id: &NodeId,
        saved_connection_id: &str,
    ) {
        let saved_connection_id = saved_connection_id.trim();
        if saved_connection_id.is_empty() {
            return;
        }
        let attached = self
            .ssh_nodes
            .get_mut(node_id)
            .is_some_and(|node| attach_saved_owner_to_reused_ssh_node(node, saved_connection_id));
        let owned_by_saved_connection = self
            .ssh_nodes
            .get(node_id)
            .is_some_and(|node| node.saved_connection_id.as_deref() == Some(saved_connection_id));
        if !attached && !owned_by_saved_connection {
            return;
        }

        let saved_connection_id = saved_connection_id.trim().to_string();
        self.saved_ssh_nodes
            .insert(saved_connection_id.clone(), node_id.clone());
        let runtime_origin_needs_owner =
            self.node_router
                .node_metadata(node_id)
                .is_some_and(|snapshot| {
                    snapshot.origin.saved_connection_id() != Some(saved_connection_id.as_str())
                });
        if attached || runtime_origin_needs_owner {
            // This is an explicit saved-connection open that reaches an
            // existing node. Promote runtime origin so persisted node ownership
            // and SSH privilege scope agree after restart.
            let _ = self.node_router.update_node_origin(
                node_id,
                NodeOrigin::Restored {
                    saved_connection_id,
                },
            );
        }
        if attached || runtime_origin_needs_owner {
            self.persist_session_tree_snapshot();
        }
    }

    fn config_with_host_key_acceptance_for_node(
        &mut self,
        node_id: &NodeId,
        accepted_config: &SshConfig,
    ) -> Option<SshConfig> {
        let trust_host_key = accepted_config.trust_host_key?;
        let expected_host_key_fingerprint =
            accepted_config.expected_host_key_fingerprint.clone()?;
        let runtime_snapshot = self.node_router.node_runtime_snapshot(node_id)?;
        let mut config = runtime_snapshot.config;
        // Tauri passes accepted host-key data as connectNode step options. A
        // reused native node connects from its runtime-owned config, so update
        // that owner before starting the worker.
        config.strict_host_key_checking = true;
        config.trust_host_key = Some(trust_host_key);
        config.expected_host_key_fingerprint = Some(expected_host_key_fingerprint);
        let action_config = config.clone();
        self.node_router
            .upsert_node_with_origin(node_id.clone(), config, runtime_snapshot.origin);
        Some(action_config)
    }

    pub(in crate::workspace) fn try_reuse_active_saved_connection_terminal(
        &mut self,
        saved_connection_id: &str,
        connection: &oxideterm_connections::SavedConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session_id) = self.ssh_nodes.iter().find_map(|(node_id, _node)| {
            // Match Tauri connectToSaved: a saved connection with missing
            // credentials may still focus an already-active root node with the
            // same endpoint, but it must not create a new terminal.
            let matching_root = self
                .node_router
                .node_metadata(node_id)
                .is_some_and(|snapshot| {
                    snapshot.depth == 0
                        && snapshot.host == connection.host
                        && snapshot.port == connection.port
                        && snapshot.username == connection.username
                });
            // `then_some` evaluates its argument eagerly. Use `first()` so a
            // ready node without attached terminals can be skipped instead of
            // indexing an empty terminal list during startup/event handling.
            let runtime_ready = self
                .node_router
                .node_state(node_id)
                .is_ok_and(|snapshot| snapshot.state.readiness == NodeReadiness::Ready);
            let session_id = self
                .workspace_runtime
                .read(cx)
                .ssh_terminal_session_ids_for_node(node_id)
                .first()
                .copied();
            (matching_root && runtime_ready)
                .then_some(session_id)
                .flatten()
        }) else {
            return false;
        };

        if !self.focus_terminal_session(session_id, window, cx) {
            return false;
        }
        let _ = self.connection_store.mark_used(saved_connection_id);
        true
    }

    fn existing_direct_root_node_for_saved_config(&self, config: &SshConfig) -> Option<NodeId> {
        self.node_router
            .flatten_tree()
            .into_iter()
            .find(|node| {
                node.depth == 0
                    && node.host == config.host
                    && node.port == config.port
                    && node.username == config.username
            })
            .map(|node| NodeId::new(node.id))
    }

    pub(in crate::workspace) fn materialize_ssh_root_node(
        &mut self,
        config: SshConfig,
        title: String,
        saved_connection_id: Option<String>,
    ) -> NodeId {
        if let Some(saved_connection_id) = saved_connection_id.as_ref()
            && let Some(node_id) = self.saved_ssh_nodes.get(saved_connection_id).cloned()
        {
            let ui_node = WorkspaceSshNode::new(
                Some(saved_connection_id.clone()),
                &config,
                title.clone(),
                Vec::new(),
                NodeReadiness::Disconnected,
            );
            if !self.node_router.contains_node(&node_id) {
                self.node_router.upsert_node_with_origin(
                    node_id.clone(),
                    config,
                    NodeOrigin::Restored {
                        saved_connection_id: saved_connection_id.clone(),
                    },
                );
            }
            self.ssh_nodes.entry(node_id.clone()).or_insert(ui_node);
            self.associate_existing_node_with_saved_connection(&node_id, saved_connection_id);
            return node_id;
        }

        let node_id = NodeId::new(format!("ssh-{}", self.next_ssh_node_id));
        self.next_ssh_node_id += 1;
        let origin = saved_connection_id
            .as_ref()
            .map(|id| NodeOrigin::Restored {
                saved_connection_id: id.clone(),
            })
            .unwrap_or(NodeOrigin::Direct);
        let ui_node = WorkspaceSshNode::new(
            saved_connection_id.clone(),
            &config,
            title,
            Vec::new(),
            NodeReadiness::Disconnected,
        );
        self.node_router
            .upsert_node_with_origin(node_id.clone(), config, origin);
        self.ssh_nodes.insert(node_id.clone(), ui_node);
        if let Some(saved_connection_id) = saved_connection_id {
            self.saved_ssh_nodes
                .insert(saved_connection_id, node_id.clone());
        }
        self.persist_session_tree_snapshot();
        node_id
    }

    pub(crate) fn open_temporary_ssh_launch(
        &mut self,
        launch: TemporarySshLaunch,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let title = launch.title();
        let auth = match launch.password {
            Some(password) => AuthMethod::password_secret(password),
            None => AuthMethod::Agent,
        };
        let config = SshConfig {
            host: launch.host,
            port: launch.port,
            username: launch.username,
            auth,
            strict_host_key_checking: true,
            ..SshConfig::default()
        };
        let node_id = self.materialize_ssh_root_node(config, title.clone(), None);
        let queue_outcome = self.workspace_runtime.update(cx, |runtime, runtime_cx| {
            runtime.queue_ssh_terminal_open(
                runtime_entity::PendingSshTerminalOpen {
                    node_id: node_id.clone(),
                    post_connect_command: None,
                    mark_used_connection_id: None,
                    save_after_open: None,
                    cleanup_node_id: Some(node_id.clone()),
                    title,
                },
                runtime_cx,
            )
        });
        if queue_outcome == runtime_entity::QueueSshTerminalOpenOutcome::WorkspaceShuttingDown {
            return Err(anyhow::anyhow!("workspace runtime is shutting down"));
        }
        // The temporary launch now shares the same node-owned transport attempt
        // and reliable completion delivery as every other first terminal.
        self.ensure_node_connection_started(&node_id, cx);
        cx.notify();
        Ok(())
    }

    pub(in crate::workspace) fn expand_saved_connection_tree(
        &mut self,
        saved_connection_id: &str,
        mut config: SshConfig,
        target_title: String,
    ) -> Result<NodeTreeExpansion> {
        let proxy_chain = config.proxy_chain.take().unwrap_or_default();
        // Consume the detached chain so runtime authentication material is not cloned.
        let hops = proxy_chain
            .into_iter()
            .map(ssh_config_from_proxy_hop)
            .collect::<Vec<_>>();
        let expansion = self
            .node_router
            .expand_manual_preset(saved_connection_id, hops, config)?;
        self.register_expanded_tree_nodes(saved_connection_id, &expansion, target_title, true);
        self.persist_session_tree_snapshot();
        Ok(expansion)
    }

    pub(in crate::workspace) fn expand_saved_connection_tree_under_parent(
        &mut self,
        parent_node_id: NodeId,
        saved_connection_id: &str,
        mut config: SshConfig,
        target_title: String,
    ) -> Result<NodeTreeExpansion> {
        let proxy_chain = config.proxy_chain.take().unwrap_or_default();
        // Consume the detached chain so runtime authentication material is not cloned.
        let hops = proxy_chain
            .into_iter()
            .map(ssh_config_from_proxy_hop)
            .collect::<Vec<_>>();
        let expansion = self.node_router.expand_manual_preset_under_parent(
            parent_node_id.clone(),
            saved_connection_id,
            hops,
            config,
        )?;
        self.register_expanded_tree_nodes(saved_connection_id, &expansion, target_title, false);
        self.expanded_ssh_nodes.insert(parent_node_id);
        for node_id in &expansion.path_node_ids {
            self.expanded_ssh_nodes.insert(node_id.clone());
        }
        // A saved next hop is scoped to its current parent node. Keep the
        // per-node saved id, but do not replace the root saved-connection
        // reuse index with a context-specific child path.
        self.persist_session_tree_snapshot();
        Ok(expansion)
    }

    fn register_expanded_tree_nodes(
        &mut self,
        saved_connection_id: &str,
        expansion: &NodeTreeExpansion,
        target_title: String,
        update_saved_node_index: bool,
    ) {
        for node_id in &expansion.path_node_ids {
            let Some(snapshot) = self.node_router.node_metadata(node_id) else {
                continue;
            };
            let title = if node_id == &expansion.target_node_id {
                target_title.clone()
            } else {
                format!("{}@{}", snapshot.username, snapshot.host)
            };
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
                    terminal_ids: Vec::new(),
                    readiness: NodeReadiness::Disconnected,
                },
            );
        }
        if update_saved_node_index {
            self.saved_ssh_nodes.insert(
                saved_connection_id.to_string(),
                expansion.target_node_id.clone(),
            );
        }
    }

    pub(super) fn create_ssh_terminal_pane_for_existing_node(
        &mut self,
        node_id: &NodeId,
        post_connect_command: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(PaneId, TerminalSessionId)> {
        let (host, port, username) = self
            .ssh_nodes
            .get(node_id)
            .map(|node| {
                (
                    node.endpoint.host.clone(),
                    node.endpoint.port,
                    node.endpoint.username.clone(),
                )
            })
            .ok_or_else(|| anyhow::anyhow!("SSH node {} not found", node_id.0))?;
        let connection_id = self
            .node_router
            .connection_id_for_node(node_id)
            .ok_or_else(|| anyhow::anyhow!("SSH node {} is not connected", node_id.0))?;
        if !self.node_router.contains_node(node_id) {
            return Err(anyhow::anyhow!(
                "SSH node {} has no runtime owner",
                node_id.0
            ));
        }

        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        self.register_existing_ssh_terminal_session(node_id, session_id, cx)?;

        // Tauri remounts terminal tabs by replacing the old session id in the
        // pane tree after reconnect. The new GPUI pane is only a consumer of
        // the node-owned SSH connection; node liveness stays with NodeRouter.
        let preferences = self.prepare_terminal_preferences_for_tab_kind(&TabKind::SshTerminal, cx);
        let consumer = ConnectionConsumer::Terminal(session_id.0.to_string());
        // Opening another terminal for an already-connected node mirrors
        // Tauri's createTerminalForNode(nodeId) path: no post-connect command
        // is replayed unless the caller explicitly supplies one.
        let session_config =
            SshSessionConfig::for_existing_connection(connection_id, host, port, username)
                .with_post_connect_command(post_connect_command)
                .with_registry(self.ssh_registry.clone(), consumer)
                // Reopened node terminals are consumers of the same backend runtime
                // as the node-owned SSH transport.
                // Keep remounted tabs on the same deferred PTY boundary as fresh
                // SSH terminals so reconnects do not briefly start at fallback size.
                .with_deferred_pty(true)
                .with_runtime_handle(self.forwarding_runtime.handle().clone())
                .with_trzsz_policy(preferences.trzsz_policy.clone());
        let shared_session = TerminalPane::ssh_shared_session(session_config, &preferences);
        self.register_terminal_endpoint_session(node_id, session_id, shared_session.clone(), cx);
        let pane = cx.new(|cx| {
            TerminalPane::from_shared_session(shared_session, preferences, window, cx)
                .expect("failed to remount ssh terminal pane")
        });
        self.register_terminal_pane(pane_id, session_id, pane, window, cx);
        self.refresh_native_plugin_terminal_hooks(cx);
        self.persist_session_tree_snapshot();
        Ok((pane_id, session_id))
    }

    pub(in crate::workspace) fn create_ssh_terminal_tab_for_existing_node(
        &mut self,
        node_id: &NodeId,
        post_connect_command: Option<String>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let tab_id = self.alloc_tab_id(cx);
        let (pane_id, session_id) = self.create_ssh_terminal_pane_for_existing_node(
            node_id,
            post_connect_command,
            window,
            cx,
        )?;
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::SshTerminal,
            title,
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(pane_id, session_id)),
            active_pane_id: Some(pane_id),
        });
        self.bind_terminal_location(tab_id, pane_id, session_id, cx);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        if self.sidebar_collapsed {
            self.set_sidebar_collapsed_with_motion(false, cx);
        }
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window, cx);
        self.persist_session_tree_snapshot();
        // The existing node remains the physical transport owner. This tab
        // owns only the terminal consumer registered by the pane helper.
        self.start_remote_shell_integration_terminal_gate(node_id.clone(), false, cx);
        cx.notify();
        Ok(session_id)
    }

    pub(in crate::workspace) fn queue_ssh_terminal_tab_for_existing_node(
        &mut self,
        node_id: NodeId,
        post_connect_command: Option<String>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if !self.node_is_ready_for_terminal(&node_id) {
            return Err(anyhow::anyhow!("SSH node {} is not ready", node_id.0));
        }
        self.create_ssh_terminal_tab_for_existing_node(
            &node_id,
            post_connect_command,
            title,
            window,
            cx,
        )?;
        Ok(())
    }

    pub(in crate::workspace) fn queue_ssh_terminal_tab_for_node(
        &mut self,
        node_id: NodeId,
        config: SshConfig,
        title: String,
        saved_connection_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.queue_ssh_terminal_tab_for_node_with_mark_used(
            node_id,
            None,
            config,
            title,
            saved_connection_id,
            None,
            None,
            window,
            cx,
        )
    }

    fn save_connection_after_terminal_open(
        &mut self,
        request: SaveConnectionRequest,
        cx: &mut Context<Self>,
    ) {
        // Tauri opens the terminal first and treats save failures as a toast,
        // not as a failed SSH connection attempt.
        if let Err(error) = self.connection_store.upsert(request) {
            self.push_command_palette_toast(
                self.i18n.t("modals.new_connection.save_failed"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
                cx,
            );
            cx.notify();
            return;
        }
        self.queue_cloud_sync_dirty_refresh(cx);
    }

    pub(in crate::workspace) fn queue_ssh_terminal_tab_for_node_with_mark_used(
        &mut self,
        node_id: NodeId,
        post_connect_command: Option<String>,
        config: SshConfig,
        title: String,
        saved_connection_id: Option<String>,
        mark_used_connection_id: Option<String>,
        mut save_after_open: Option<SaveConnectionRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.ssh_nodes.entry(node_id.clone()).or_insert_with(|| {
            WorkspaceSshNode::new(
                saved_connection_id.clone(),
                &config,
                title.clone(),
                Vec::new(),
                NodeReadiness::Disconnected,
            )
        });
        if let Some(saved_connection_id) = saved_connection_id.as_deref() {
            self.associate_existing_node_with_saved_connection(&node_id, saved_connection_id);
        }
        if self.node_is_ready_for_terminal(&node_id) {
            self.create_ssh_terminal_tab_for_existing_node(
                &node_id,
                post_connect_command,
                title,
                window,
                cx,
            )?;
            if let Some(request) = save_after_open {
                self.save_connection_after_terminal_open(request, cx);
            }
            if let Some(connection_id) = mark_used_connection_id.as_deref() {
                let _ = self.connection_store.mark_used(connection_id);
            }
            return Ok(());
        }
        let target_has_parent = self
            .node_router
            .node_metadata(&node_id)
            .and_then(|snapshot| snapshot.parent_id)
            .is_some();
        if target_has_parent && self.node_router.connection_id_for_node(&node_id).is_none() {
            let intent = mark_used_connection_id
                .clone()
                .or_else(|| saved_connection_id.clone())
                .map(SshConnectionIntent::ConnectSaved)
                .unwrap_or(SshConnectionIntent::Connect);
            if self.start_existing_session_tree_connect(
                node_id.clone(),
                title.clone(),
                intent,
                &mut save_after_open,
                window,
                cx,
            ) {
                cx.notify();
                return Ok(());
            }
        }
        let queue_outcome = self.workspace_runtime.update(cx, |runtime, runtime_cx| {
            runtime.queue_ssh_terminal_open(
                runtime_entity::PendingSshTerminalOpen {
                    node_id: node_id.clone(),
                    post_connect_command,
                    mark_used_connection_id,
                    save_after_open,
                    cleanup_node_id: None,
                    title,
                },
                runtime_cx,
            )
        });
        if queue_outcome == runtime_entity::QueueSshTerminalOpenOutcome::WorkspaceShuttingDown {
            return Err(anyhow::anyhow!("workspace runtime is shutting down"));
        }
        if queue_outcome == runtime_entity::QueueSshTerminalOpenOutcome::Ready {
            cx.notify();
            return Ok(());
        }
        self.ensure_node_connection_started(&node_id, cx);
        cx.notify();
        Ok(())
    }

    pub(in crate::workspace) fn open_ready_ssh_terminal_requests(
        &mut self,
        requests: Vec<runtime_entity::PendingSshTerminalOpen>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut opened = false;
        for request in requests {
            if self
                .create_ssh_terminal_tab_for_existing_node(
                    &request.node_id,
                    request.post_connect_command,
                    request.title,
                    window,
                    cx,
                )
                .is_ok()
            {
                let mark_used_connection_id = request.mark_used_connection_id.clone();
                if let Some(save_request) = request.save_after_open {
                    self.save_connection_after_terminal_open(save_request, cx);
                }
                if let Some(connection_id) = mark_used_connection_id.as_deref() {
                    let _ = self.connection_store.mark_used(connection_id);
                }
                opened = true;
            }
        }
        opened
    }

    pub(in crate::workspace) fn node_is_ready_for_terminal(&self, node_id: &NodeId) -> bool {
        self.node_router
            .node_state(node_id)
            .is_ok_and(|snapshot| snapshot.state.readiness == NodeReadiness::Ready)
    }

    fn register_terminal_endpoint_session(
        &mut self,
        node_id: &NodeId,
        session_id: TerminalSessionId,
        session: SharedTerminalSession,
        cx: &mut Context<Self>,
    ) {
        let endpoint = TerminalEndpoint {
            // Native GPUI does not need a loopback WebSocket, but the owner
            // boundary mirrors Tauri: NodeRouter exposes a stable terminal
            // endpoint and GPUI panes consume the session by id instead of
            // being the authoritative terminal owner.
            ws_port: 0,
            ws_token: zeroize::Zeroizing::new(format!("native-terminal-{}", session_id.0)),
            session_id: session_id.0.to_string(),
        };
        self.terminal_endpoint_sessions
            .insert(session_id, WorkspaceTerminalEndpointSession { session });
        // Register every endpoint. NodeRouter keeps the first endpoint primary
        // and can elect another live endpoint when that terminal closes.
        self.workspace_runtime
            .read(cx)
            .bind_ssh_terminal_endpoint(node_id, endpoint);
        self.persist_session_tree_snapshot();
    }

    pub(in crate::workspace) fn open_settings_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = if let Some(tab) = self.tabs.iter().find(|tab| tab.kind == TabKind::Settings) {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Settings,
                title: self.i18n.t("settings_view.title"),
                title_source: TabTitleSource::I18nKey("settings_view.title"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Settings;
        self.needs_active_pane_focus = false;
        if self.sidebar_collapsed {
            self.set_sidebar_collapsed_with_motion(false, cx);
        }
        self.persist_sidebar_settings(cx);
        self.reveal_active_tab(window, cx);
        if self.settings_workspace.read(cx).route_snapshot().active_tab == SettingsTab::General {
            self.refresh_cli_companion_status(cx);
            #[cfg(not(target_os = "macos"))]
            self.refresh_launch_at_login_status(cx);
        }
        cx.notify();
    }
}

fn ssh_config_from_proxy_hop(hop: ProxyHopConfig) -> SshConfig {
    let ProxyHopConfig {
        host,
        port,
        username,
        auth,
        agent_forwarding,
        identity_agent,
        agent_forwarding_socket,
        legacy_ssh_compatibility,
        strict_host_key_checking,
        trust_host_key,
        expected_host_key_fingerprint,
    } = hop;
    SshConfig {
        host,
        port,
        username,
        auth,
        proxy_chain: None,
        agent_forwarding,
        identity_agent,
        agent_forwarding_socket,
        legacy_ssh_compatibility,
        strict_host_key_checking,
        trust_host_key,
        expected_host_key_fingerprint,
        ..SshConfig::default()
    }
}

#[cfg(test)]
mod create_tests {
    use super::*;

    #[test]
    fn proxy_hop_conversion_moves_auth_and_preserves_transport_options() {
        let config = ssh_config_from_proxy_hop(ProxyHopConfig {
            host: "jump.example.com".to_string(),
            port: 2202,
            username: "operator".to_string(),
            auth: AuthMethod::password("runtime-secret"),
            agent_forwarding: true,
            identity_agent: Some("/tmp/identity-agent.sock".to_string()),
            agent_forwarding_socket: Some("/tmp/forward-agent.sock".to_string()),
            legacy_ssh_compatibility: true,
            strict_host_key_checking: true,
            trust_host_key: Some(false),
            expected_host_key_fingerprint: Some("SHA256:test".to_string()),
        });

        assert_eq!(config.host, "jump.example.com");
        assert_eq!(config.port, 2202);
        assert_eq!(config.username, "operator");
        assert!(config.agent_forwarding);
        assert_eq!(
            config.identity_agent.as_deref(),
            Some("/tmp/identity-agent.sock")
        );
        assert_eq!(
            config.agent_forwarding_socket.as_deref(),
            Some("/tmp/forward-agent.sock")
        );
        assert!(config.legacy_ssh_compatibility);
        assert!(config.strict_host_key_checking);
        assert_eq!(config.trust_host_key, Some(false));
        assert_eq!(
            config.expected_host_key_fingerprint.as_deref(),
            Some("SHA256:test")
        );
        match config.auth {
            AuthMethod::Password { password } => {
                assert_eq!(password.as_str(), "runtime-secret");
            }
            _ => panic!("proxy hop password authentication was not preserved"),
        }
    }

    #[test]
    fn reused_ssh_node_without_owner_accepts_explicit_saved_owner() {
        let mut node = WorkspaceSshNode::new(
            None,
            &SshConfig::default(),
            "lipsc@100.118.61.75".to_string(),
            vec![TerminalSessionId(1)],
            NodeReadiness::Ready,
        );

        assert!(attach_saved_owner_to_reused_ssh_node(&mut node, "home-100"));
        assert_eq!(node.saved_connection_id.as_deref(), Some("home-100"));
    }

    #[test]
    fn reused_ssh_node_keeps_existing_saved_owner() {
        let mut node = WorkspaceSshNode::new(
            Some("existing-owner".to_string()),
            &SshConfig::default(),
            "Production".to_string(),
            vec![TerminalSessionId(1)],
            NodeReadiness::Ready,
        );

        assert!(!attach_saved_owner_to_reused_ssh_node(
            &mut node,
            "other-owner"
        ));
        assert_eq!(node.saved_connection_id.as_deref(), Some("existing-owner"));
    }

    #[test]
    fn saved_node_route_rejects_a_stale_direct_target() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let node_id = NodeId::new("saved-node");
        router.upsert_node(
            node_id.clone(),
            SshConfig::password("old.example.com", 22, "ops", "old-secret"),
        );
        let requested = SshConfig::password("new.example.com", 22, "ops", "new-secret");

        assert!(!saved_node_route_matches_config(
            &router, &node_id, &requested
        ));
    }

    #[test]
    fn saved_node_route_accepts_the_same_direct_target_with_new_auth() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let node_id = NodeId::new("saved-node");
        router.upsert_node(
            node_id.clone(),
            SshConfig::password("target.example.com", 22, "ops", "old-secret"),
        );
        let requested = SshConfig::password("target.example.com", 22, "ops", "new-secret");

        assert!(saved_node_route_matches_config(
            &router, &node_id, &requested
        ));
    }

    #[test]
    fn saved_node_route_rejects_a_different_proxy_chain() {
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        let expansion = router
            .expand_manual_preset(
                "saved-a",
                vec![SshConfig::password("old-jump.example.com", 22, "ops", "pw")],
                SshConfig::password("target.example.com", 22, "ops", "pw"),
            )
            .unwrap();
        let requested = SshConfig {
            proxy_chain: Some(vec![ProxyHopConfig {
                host: "new-jump.example.com".to_string(),
                port: 22,
                username: "ops".to_string(),
                auth: AuthMethod::password("pw"),
                agent_forwarding: false,
                identity_agent: None,
                agent_forwarding_socket: None,
                legacy_ssh_compatibility: false,
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            }]),
            ..SshConfig::password("target.example.com", 22, "ops", "pw")
        };

        assert!(!saved_node_route_matches_config(
            &router,
            &expansion.target_node_id,
            &requested,
        ));
    }
}
