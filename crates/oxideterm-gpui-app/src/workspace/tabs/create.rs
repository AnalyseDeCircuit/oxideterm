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

    pub(super) fn create_ssh_terminal_tab(
        &mut self,
        config: SshConfig,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.create_ssh_terminal_tab_for_node(config, title, None, None, window, cx)
            .map(|_| ())
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
            return Ok(());
        }

        let node_id = self.saved_ssh_nodes.get(&saved_connection_id).cloned();
        self.create_ssh_terminal_tab_for_node(
            config,
            title,
            Some(saved_connection_id),
            node_id,
            window,
            cx,
        )
        .map(|_| ())
    }

    pub(super) fn create_ssh_terminal_tab_for_node(
        &mut self,
        config: SshConfig,
        title: String,
        saved_connection_id: Option<String>,
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

        self.node_runtime_store
            .upsert_node(node_id.clone(), config.clone());
        if self.node_router.connection_id_for_node(&node_id).is_none() {
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
        let _ = self
            .node_router
            .bind_terminal_session(&node_id, session_id.0.to_string());
        self.register_ssh_terminal_session(
            node_id.clone(),
            saved_connection_id,
            config.clone(),
            title.clone(),
            session_id,
        );
        let consumer = ConnectionConsumer::Terminal(session_id.0.to_string());
        let prompt_handler =
            std::sync::Arc::new(NativeSshPromptHandler::new(self.ssh_worker_tx.clone()));
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::SshTerminal);
        let session_config = SshSessionConfig::from(config)
            .with_registry(self.ssh_registry.clone(), consumer)
            .with_prompt_handler(prompt_handler)
            .with_trzsz_policy(preferences.trzsz_policy.clone());
        let pane = cx.new(|cx| {
            TerminalPane::new_ssh_with_preferences(session_config, preferences, window, cx)
                .expect("failed to initialize ssh terminal pane")
        });

        self.panes.insert(pane_id, pane.clone());
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
        cx.notify();
        Ok(session_id)
    }

    fn create_ssh_terminal_pane_for_existing_node(
        &mut self,
        node_id: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(PaneId, TerminalSessionId)> {
        let node = self
            .ssh_nodes
            .get(node_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("SSH node {} not found", node_id.0))?;
        self.node_runtime_store
            .upsert_node(node_id.clone(), node.config.clone());
        if self.node_router.connection_id_for_node(node_id).is_none() {
            self.ensure_node_connection_started(node_id);
        }

        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let _ = self
            .node_router
            .bind_terminal_session(node_id, session_id.0.to_string());
        self.register_ssh_terminal_session(
            node_id.clone(),
            node.saved_connection_id.clone(),
            node.config.clone(),
            node.title.clone(),
            session_id,
        );

        // Tauri remounts terminal tabs by replacing the old session id in the
        // pane tree after reconnect. The new GPUI pane is only a consumer of
        // the node-owned SSH connection; node liveness stays with NodeRouter.
        let consumer = ConnectionConsumer::Terminal(session_id.0.to_string());
        let prompt_handler =
            std::sync::Arc::new(NativeSshPromptHandler::new(self.ssh_worker_tx.clone()));
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::SshTerminal);
        let session_config = SshSessionConfig::from(node.config)
            .with_registry(self.ssh_registry.clone(), consumer)
            .with_prompt_handler(prompt_handler)
            .with_trzsz_policy(preferences.trzsz_policy.clone());
        let pane = cx.new(|cx| {
            TerminalPane::new_ssh_with_preferences(session_config, preferences, window, cx)
                .expect("failed to remount ssh terminal pane")
        });
        self.panes.insert(pane_id, pane);
        Ok((pane_id, session_id))
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
        cx.notify();
    }
}
