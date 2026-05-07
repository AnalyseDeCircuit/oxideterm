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

        self.node_router
            .upsert_node(node_id.clone(), config.clone());
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
        let session_config = SshSessionConfig::from(config)
            .with_registry(self.ssh_registry.clone(), consumer)
            .with_prompt_handler(prompt_handler);
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::SshTerminal);
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
        self.active_sidebar_section = SidebarSection::Sessions;
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
        self.persist_sidebar_settings();
        self.needs_active_pane_focus = true;
        pane.read(cx).focus(window);
        self.reveal_active_tab(window);
        cx.notify();
        Ok(session_id)
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
