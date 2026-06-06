impl WorkspaceApp {
    pub(super) fn create_local_terminal_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let tab_id = self.alloc_tab_id();
        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::LocalTerminal);
        let terminal_config = self.local_terminal_config();
        let pane = cx.new(|cx| {
            TerminalPane::new_local_with_config_and_preferences(
                terminal_config,
                preferences,
                window,
                cx,
            )
            .expect("failed to initialize terminal pane")
        });

        self.panes.insert(pane_id, pane.clone());
        self.refresh_native_plugin_terminal_hooks(cx);
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title: self.local_terminal_tab_title(),
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(pane_id, session_id)),
            active_pane_id: Some(pane_id),
        });
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.read(cx).focus(window);
        self.reveal_active_tab(window);
        cx.notify();
        Ok(())
    }

    pub(super) fn create_telnet_terminal_tab(
        &mut self,
        config: TelnetSessionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let tab_id = self.alloc_tab_id();
        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::LocalTerminal);
        let title = format!("Telnet {}", config.endpoint_label());
        let pane_config = config.clone();
        let pane = cx.new(|cx| {
            TerminalPane::new_telnet_with_preferences(
                pane_config,
                preferences,
                window,
                cx,
            )
            .expect("failed to initialize Telnet terminal pane")
        });

        // Telnet is a local transport in the plugin API: it owns no SSH node,
        // but it still participates in the normal tab/pane/session registry.
        self.panes.insert(pane_id, pane.clone());
        self.refresh_native_plugin_terminal_hooks(cx);
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title,
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(pane_id, session_id)),
            active_pane_id: Some(pane_id),
        });
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.read(cx).focus(window);
        self.reveal_active_tab(window);
        cx.notify();
        Ok(session_id)
    }

    pub(super) fn create_serial_terminal_tab(
        &mut self,
        config: SerialSessionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let tab_id = self.alloc_tab_id();
        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::LocalTerminal);
        let title = format!("Serial {}", config.port_path);
        let pane_config = config.clone();
        let pane = cx.new(|cx| {
            TerminalPane::new_serial_with_preferences(pane_config, preferences, window, cx)
                .expect("failed to initialize Serial terminal pane")
        });

        // Serial mirrors Tauri local-terminal transport semantics: it is not
        // an SSH node and must not expose SFTP, forwarding, or ProxyJump.
        self.panes.insert(pane_id, pane.clone());
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
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.read(cx).focus(window);
        self.reveal_active_tab(window);
        cx.notify();
        Ok(session_id)
    }

    pub(super) fn open_or_create_saved_ssh_terminal_tab(
        &mut self,
        saved_connection_id: String,
        config: SshConfig,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if let Some(node_id) = self.saved_ssh_nodes.get(&saved_connection_id).cloned()
            && let Some(session_id) = self
                .ssh_nodes
                .get(&node_id)
                .and_then(|node| node.terminal_ids.first().copied())
            && self.focus_terminal_session(session_id, window, cx)
        {
            let _ = self.connection_store.mark_used(&saved_connection_id);
            return Ok(());
        }
        if let Some(node_id) = self.saved_ssh_nodes.get(&saved_connection_id).cloned()
            && let Some(node) = self.ssh_nodes.get(&node_id).cloned()
        {
            let node_config = self
                .config_with_host_key_acceptance_for_node(&node_id, &config)
                .unwrap_or(node.config);
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
                .node_runtime_store
                .snapshot(&expansion.target_node_id)
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
                if let Some(session_id) = self
                    .ssh_nodes
                    .get(&existing_node_id)
                    .and_then(|node| node.terminal_ids.first().copied())
                    && self.focus_terminal_session(session_id, window, cx)
                {
                    let _ = self.connection_store.mark_used(&saved_connection_id);
                    return Ok(());
                }
                if let Some(node) = self.ssh_nodes.get(&existing_node_id).cloned() {
                    // Tauri's saved direct-open path reuses an existing root
                    // node by host/port/user without rewriting the node origin
                    // to the saved connection. Keep the same tree owner here.
                    let node_config = self
                        .config_with_host_key_acceptance_for_node(&existing_node_id, &config)
                        .unwrap_or(node.config);
                    // Tauri reuses the existing direct root node but still
                    // applies the saved connection's current terminal command.
                    let post_connect_command = config.post_connect_command.clone();
                    self.queue_ssh_terminal_tab_for_node_with_mark_used(
                        existing_node_id,
                        post_connect_command,
                        node_config,
                        node.title,
                        node.saved_connection_id,
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
                self.mark_pending_ssh_terminal_open_cleanup(
                    &cleanup_node_id,
                    cleanup_node_id.clone(),
                );
            }
            result
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
        let node = self.ssh_nodes.get_mut(node_id)?;
        let mut config = node.config.clone();
        // Tauri passes accepted host-key data as connectNode step options. A
        // reused native node connects from its stored config, so mirror those
        // one-step options onto the existing node before starting the worker.
        config.strict_host_key_checking = true;
        config.trust_host_key = Some(trust_host_key);
        config.expected_host_key_fingerprint = Some(expected_host_key_fingerprint);
        node.config = config.clone();
        let origin = self
            .node_runtime_store
            .snapshot(node_id)
            .map(|snapshot| snapshot.origin)
            .unwrap_or_default();
        self.node_runtime_store
            .upsert_node_with_origin(node_id.clone(), config.clone(), origin);
        Some(config)
    }

    pub(super) fn try_reuse_active_saved_connection_terminal(
        &mut self,
        saved_connection_id: &str,
        connection: &oxideterm_connections::SavedConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session_id) = self.ssh_nodes.iter().find_map(|(node_id, node)| {
            // Match Tauri connectToSaved: a saved connection with missing
            // credentials may still focus an already-active root node with the
            // same endpoint, but it must not create a new terminal.
            let matching_root = self
                .node_runtime_store
                .snapshot(node_id)
                .is_some_and(|snapshot| {
                    snapshot.depth == 0
                        && snapshot.config.host == connection.host
                        && snapshot.config.port == connection.port
                        && snapshot.config.username == connection.username
                });
            // `then_some` evaluates its argument eagerly. Use `first()` so a
            // ready node without attached terminals can be skipped instead of
            // indexing an empty terminal list during startup/event handling.
            (matching_root && node.readiness == NodeReadiness::Ready)
                .then(|| node.terminal_ids.first().copied())
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
        self.node_runtime_store
            .flatten()
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
            if self.node_runtime_store.snapshot(&node_id).is_none() {
                self.node_runtime_store.upsert_node_with_origin(
                    node_id.clone(),
                    config.clone(),
                    NodeOrigin::Restored {
                        saved_connection_id: saved_connection_id.clone(),
                    },
                );
            }
            self.ssh_nodes
                .entry(node_id.clone())
                .or_insert_with(|| WorkspaceSshNode {
                    saved_connection_id: Some(saved_connection_id.clone()),
                    config: config.clone(),
                    title: title.clone(),
                    terminal_ids: Vec::new(),
                    readiness: NodeReadiness::Disconnected,
                });
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
        self.node_runtime_store
            .upsert_node_with_origin(node_id.clone(), config.clone(), origin);
        self.ssh_nodes.insert(
            node_id.clone(),
            WorkspaceSshNode {
                saved_connection_id: saved_connection_id.clone(),
                config,
                title,
                terminal_ids: Vec::new(),
                readiness: NodeReadiness::Disconnected,
            },
        );
        if let Some(saved_connection_id) = saved_connection_id {
            self.saved_ssh_nodes
                .insert(saved_connection_id, node_id.clone());
        }
        self.persist_session_tree_snapshot();
        node_id
    }

    pub(super) fn create_ssh_terminal_tab_for_node(
        &mut self,
        post_connect_command: Option<String>,
        config: SshConfig,
        title: String,
        saved_connection_id: Option<String>,
        privilege_connection_id: Option<String>,
        node_id: Option<NodeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalSessionId> {
        let tab_id = self.alloc_tab_id();
        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let node_id = node_id.unwrap_or_else(|| {
            let id = NodeId::new(format!("ssh-{}", self.next_ssh_node_id));
            self.next_ssh_node_id += 1;
            id
        });

        let origin = self
            .node_runtime_store
            .snapshot(&node_id)
            .map(|snapshot| snapshot.origin)
            .or_else(|| {
                saved_connection_id.as_ref().map(|id| NodeOrigin::Restored {
                    saved_connection_id: id.clone(),
                })
            })
            .unwrap_or(NodeOrigin::Direct);
        if self.node_runtime_store.snapshot(&node_id).is_none() {
            self.node_runtime_store.upsert_node_with_origin(
                node_id.clone(),
                config.clone(),
                origin,
            );
        }
        let starting_node_connection = self.node_router.connection_id_for_node(&node_id).is_none();
        let trace_plan = starting_node_connection
            .then(|| self.connection_trace_plan_for_node(&node_id, ConnectionTraceMode::Connect))
            .flatten();
        let trace_parent_id = self
            .node_runtime_store
            .snapshot(&node_id)
            .and_then(|snapshot| snapshot.parent_id);
        if starting_node_connection {
            let node_consumer = ConnectionConsumer::NodeRouter(node_id.0.clone());
            let node_handle = self.ssh_registry.acquire(config.clone(), node_consumer);
            let connection_id = node_handle.connection_id().to_string();
            let _ = self
                .ssh_registry
                .mark_state(&connection_id, ConnectionState::Connecting);
            // Tauri owns SSH node liveness outside any terminal tab. Keep a
            // NodeRouter consumer in the pool so SFTP/forwards can resolve by
            // nodeId after the terminal pane that established the transport is
            // closed; the terminal below is only an additional consumer.
            if let Ok(event) = self.node_router.bind_connection(&node_id, connection_id) {
                self.emit_node_event(event);
            }
        }
        self.register_ssh_terminal_session(
            node_id.clone(),
            saved_connection_id,
            privilege_connection_id,
            config.clone(),
            title.clone(),
            session_id,
        );
        if starting_node_connection {
            self.begin_connection_trace_for_node(
                &node_id,
                trace_plan.as_ref(),
                trace_parent_id.as_ref(),
            );
        }
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::SshTerminal);
        let consumer = ConnectionConsumer::Terminal(session_id.0.to_string());
        let prompt_handler =
            std::sync::Arc::new(NativeSshPromptHandler::new(self.ssh_worker_tx.clone()));
        let managed_key_resolver =
            oxideterm_session_adapter::managed_key_resolver_from_store(&self.connection_store);
        // Tauri passes postConnectCommand as a createTerminalForNode option.
        // Keep it one-shot for this terminal instead of letting every future
        // terminal opened from the same node replay the saved command.
        let session_config = SshSessionConfig::from(config)
            .with_post_connect_command(post_connect_command)
            .with_registry(self.ssh_registry.clone(), consumer)
            .with_prompt_handler(prompt_handler)
            .with_managed_key_resolver(managed_key_resolver)
            .with_deferred_pty(true)
            .with_trzsz_policy(preferences.trzsz_policy.clone());
        let shared_session = TerminalPane::ssh_shared_session(session_config, &preferences);
        self.register_terminal_endpoint_session(&node_id, session_id, shared_session.clone());
        let pane = cx.new(|cx| {
            TerminalPane::from_shared_session(shared_session, preferences, window, cx)
                .expect("failed to initialize ssh terminal pane")
        });

        self.panes.insert(pane_id, pane.clone());
        self.refresh_native_plugin_terminal_hooks(cx);
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::SshTerminal,
            title,
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(pane_id, session_id)),
            active_pane_id: Some(pane_id),
        });
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
        self.needs_active_pane_focus = true;
        pane.read(cx).focus(window);
        self.reveal_active_tab(window);
        self.persist_session_tree_snapshot();
        cx.notify();
        Ok(session_id)
    }

    pub(super) fn expand_saved_connection_tree(
        &mut self,
        saved_connection_id: &str,
        mut config: SshConfig,
        target_title: String,
    ) -> Result<NodeTreeExpansion> {
        let proxy_chain = config.proxy_chain.take().unwrap_or_default();
        let hops = proxy_chain
            .iter()
            .map(ssh_config_from_proxy_hop)
            .collect::<Vec<_>>();
        let expansion = self
            .node_router
            .expand_manual_preset(saved_connection_id, hops, config)?;
        self.register_expanded_tree_nodes(saved_connection_id, &expansion, target_title);
        self.persist_session_tree_snapshot();
        Ok(expansion)
    }

    fn register_expanded_tree_nodes(
        &mut self,
        saved_connection_id: &str,
        expansion: &NodeTreeExpansion,
        target_title: String,
    ) {
        for node_id in &expansion.path_node_ids {
            let Some(snapshot) = self.node_runtime_store.snapshot(node_id) else {
                continue;
            };
            let title = if node_id == &expansion.target_node_id {
                target_title.clone()
            } else {
                format!("{}@{}", snapshot.config.username, snapshot.config.host)
            };
            self.ssh_nodes.insert(
                node_id.clone(),
                WorkspaceSshNode {
                    saved_connection_id: snapshot.origin.saved_connection_id().map(str::to_string),
                    config: snapshot.config,
                    title,
                    terminal_ids: Vec::new(),
                    readiness: NodeReadiness::Disconnected,
                },
            );
        }
        self.saved_ssh_nodes.insert(
            saved_connection_id.to_string(),
            expansion.target_node_id.clone(),
        );
    }

    fn create_ssh_terminal_pane_for_existing_node(
        &mut self,
        node_id: &NodeId,
        privilege_connection_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(PaneId, TerminalSessionId)> {
        let node = self
            .ssh_nodes
            .get(node_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("SSH node {} not found", node_id.0))?;
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
        if self.node_runtime_store.snapshot(node_id).is_none() {
            self.node_runtime_store.upsert_node_with_origin(
                node_id.clone(),
                node.config.clone(),
                origin,
            );
        }
        if self.node_router.connection_id_for_node(node_id).is_none() {
            self.ensure_node_connection_started(node_id);
        }

        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        self.register_ssh_terminal_session(
            node_id.clone(),
            node.saved_connection_id.clone(),
            privilege_connection_id.or_else(|| node.saved_connection_id.clone()),
            node.config.clone(),
            node.title.clone(),
            session_id,
        );

        // Tauri remounts terminal tabs by replacing the old session id in the
        // pane tree after reconnect. The new GPUI pane is only a consumer of
        // the node-owned SSH connection; node liveness stays with NodeRouter.
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::SshTerminal);
        let consumer = ConnectionConsumer::Terminal(session_id.0.to_string());
        let prompt_handler =
            std::sync::Arc::new(NativeSshPromptHandler::new(self.ssh_worker_tx.clone()));
        let managed_key_resolver =
            oxideterm_session_adapter::managed_key_resolver_from_store(&self.connection_store);
        // Opening another terminal for an already-connected node mirrors
        // Tauri's createTerminalForNode(nodeId) path: no post-connect command
        // is replayed unless the caller explicitly supplies one.
        let session_config = SshSessionConfig::from(node.config)
            .with_post_connect_command(None)
            .with_registry(self.ssh_registry.clone(), consumer)
            .with_prompt_handler(prompt_handler)
            .with_managed_key_resolver(managed_key_resolver)
            .with_deferred_pty(true)
            .with_trzsz_policy(preferences.trzsz_policy.clone());
        let shared_session = TerminalPane::ssh_shared_session(session_config, &preferences);
        self.register_terminal_endpoint_session(node_id, session_id, shared_session.clone());
        let pane = cx.new(|cx| {
            TerminalPane::from_shared_session(shared_session, preferences, window, cx)
                .expect("failed to remount ssh terminal pane")
        });
        self.panes.insert(pane_id, pane);
        self.refresh_native_plugin_terminal_hooks(cx);
        self.persist_session_tree_snapshot();
        Ok((pane_id, session_id))
    }

    pub(super) fn queue_ssh_terminal_tab_for_node(
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
        save_after_open: Option<SaveConnectionRequest>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.ssh_nodes
            .entry(node_id.clone())
            .or_insert_with(|| WorkspaceSshNode {
                saved_connection_id: saved_connection_id.clone(),
                config: config.clone(),
                title: title.clone(),
                terminal_ids: Vec::new(),
                readiness: NodeReadiness::Disconnected,
            });
        if self.node_is_ready_for_terminal(&node_id) {
            let privilege_connection_id = mark_used_connection_id
                .clone()
                .or_else(|| saved_connection_id.clone());
            self.create_ssh_terminal_tab_for_node(
                post_connect_command,
                config,
                title,
                saved_connection_id,
                privilege_connection_id,
                Some(node_id),
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
        if let Some(existing) = self
            .pending_ssh_terminal_opens
            .iter()
            .find(|pending| pending.node_id == node_id)
        {
            // Keep the first terminal-open request, but preserve the saved
            // connection side effects when a later action joins an already
            // pending node connection.
            if (existing.mark_used_connection_id.is_none() && mark_used_connection_id.is_some())
                || (existing.privilege_connection_id.is_none()
                    && (mark_used_connection_id.is_some() || saved_connection_id.is_some()))
                || (existing.save_after_open.is_none() && save_after_open.is_some())
            {
                if let Some(existing) = self
                    .pending_ssh_terminal_opens
                    .iter_mut()
                    .find(|pending| pending.node_id == node_id)
                {
                    if existing.privilege_connection_id.is_none() {
                        existing.privilege_connection_id = mark_used_connection_id
                            .clone()
                            .or_else(|| saved_connection_id.clone());
                    }
                    if existing.mark_used_connection_id.is_none() {
                        existing.mark_used_connection_id = mark_used_connection_id;
                    }
                    if existing.save_after_open.is_none() {
                        existing.save_after_open = save_after_open;
                    }
                    if existing.post_connect_command.is_none() {
                        existing.post_connect_command = post_connect_command;
                    }
                }
            }
        } else {
            let privilege_connection_id = mark_used_connection_id
                .clone()
                .or_else(|| saved_connection_id.clone());
            self.pending_ssh_terminal_opens
                .push_back(PendingSshTerminalOpen {
                    node_id: node_id.clone(),
                    post_connect_command,
                    saved_connection_id,
                    privilege_connection_id,
                    mark_used_connection_id,
                    save_after_open,
                    cleanup_node_id: None,
                    title,
                });
        }
        self.ensure_node_connection_started(&node_id);
        cx.notify();
        Ok(())
    }

    pub(super) fn mark_pending_ssh_terminal_open_cleanup(
        &mut self,
        node_id: &NodeId,
        cleanup_node_id: NodeId,
    ) {
        if let Some(pending) = self
            .pending_ssh_terminal_opens
            .iter_mut()
            .find(|pending| pending.node_id == *node_id)
        {
            // Tauri saved direct plans set cleanupNodeId only for freshly
            // created roots. Existing direct nodes are reused without cleanup.
            pending.cleanup_node_id = Some(cleanup_node_id);
        }
    }

    pub(super) fn pending_ssh_terminal_open_cleanup_for_node(
        &self,
        node_id: &NodeId,
    ) -> Option<NodeId> {
        self.pending_ssh_terminal_opens
            .iter()
            .find(|pending| pending.node_id == *node_id)
            .and_then(|pending| pending.cleanup_node_id.clone())
    }

    pub(super) fn drain_ready_pending_ssh_terminal_opens(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut pending = std::mem::take(&mut self.pending_ssh_terminal_opens);
        let mut remaining = VecDeque::new();
        let mut opened = false;
        while let Some(request) = pending.pop_front() {
            if !self.node_is_ready_for_terminal(&request.node_id) {
                remaining.push_back(request);
                continue;
            }
            let Some(node) = self.ssh_nodes.get(&request.node_id).cloned() else {
                continue;
            };
            if self
                .create_ssh_terminal_tab_for_node(
                    request.post_connect_command,
                    node.config,
                    request.title,
                    request.saved_connection_id,
                    request.privilege_connection_id,
                    Some(request.node_id),
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
        self.pending_ssh_terminal_opens = remaining;
        opened
    }

    pub(super) fn remove_pending_ssh_terminal_opens_for_node(&mut self, node_id: &NodeId) -> bool {
        let before = self.pending_ssh_terminal_opens.len();
        self.pending_ssh_terminal_opens
            .retain(|pending| pending.node_id != *node_id);
        self.pending_ssh_terminal_opens.len() != before
    }

    fn node_is_ready_for_terminal(&self, node_id: &NodeId) -> bool {
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

    fn register_terminal_endpoint_session(
        &mut self,
        node_id: &NodeId,
        session_id: TerminalSessionId,
        session: SharedTerminalSession,
    ) {
        let endpoint = TerminalEndpoint {
            // Native GPUI does not need a loopback WebSocket, but the owner
            // boundary mirrors Tauri: NodeRouter exposes a stable terminal
            // endpoint and GPUI panes consume the session by id instead of
            // being the authoritative terminal owner.
            ws_port: 0,
            ws_token: format!("native-terminal-{}", session_id.0),
            session_id: session_id.0.to_string(),
        };
        self.terminal_endpoint_sessions.insert(
            session_id,
            WorkspaceTerminalEndpointSession {
                endpoint: endpoint.clone(),
                session,
            },
        );
        let should_bind_primary = self
            .node_runtime_store
            .snapshot(node_id)
            .and_then(|snapshot| snapshot.terminal_session_id)
            .is_none();
        if should_bind_primary {
            if let Ok(event) = self.node_router.bind_terminal_endpoint(node_id, endpoint) {
                self.emit_node_event(event);
            }
        }
        self.persist_session_tree_snapshot();
    }

    pub(super) fn open_settings_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self.tabs.iter().find(|tab| tab.kind == TabKind::Settings) {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
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
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Settings;
        self.active_sidebar_section = SidebarSection::Settings;
        self.needs_active_pane_focus = false;
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
        self.persist_sidebar_settings();
        self.reveal_active_tab(window);
        if self.settings_page.active_tab == SettingsTab::General {
            self.refresh_cli_companion_status(cx);
        }
        cx.notify();
    }
}

fn ssh_config_from_proxy_hop(hop: &ProxyHopConfig) -> SshConfig {
    SshConfig {
        host: hop.host.clone(),
        port: hop.port,
        username: hop.username.clone(),
        auth: hop.auth.clone(),
        proxy_chain: None,
        agent_forwarding: hop.agent_forwarding,
        strict_host_key_checking: hop.strict_host_key_checking,
        trust_host_key: hop.trust_host_key,
        expected_host_key_fingerprint: hop.expected_host_key_fingerprint.clone(),
        ..SshConfig::default()
    }
}
