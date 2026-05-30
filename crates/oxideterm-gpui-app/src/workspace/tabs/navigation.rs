const TAB_DRAG_THRESHOLD_PX: f32 = 10.0;

impl WorkspaceApp {
    pub(super) fn observe_active_tab_for_history(&mut self) {
        let active_tab_id = self.active_tab_id;
        if self.tab_navigation_observed_tab == active_tab_id {
            return;
        }
        self.tab_navigation_observed_tab = active_tab_id;

        let Some(tab_id) = active_tab_id else {
            return;
        };
        if self.tab_navigation_replaying {
            self.tab_navigation_replaying = false;
            return;
        }

        if let Some(index) = self.tab_navigation_index {
            self.tab_navigation_history.truncate(index.saturating_add(1));
        }
        if self.tab_navigation_history.last().copied() != Some(tab_id) {
            self.tab_navigation_history.push(tab_id);
        }
        const MAX_TAB_HISTORY: usize = 50;
        if self.tab_navigation_history.len() > MAX_TAB_HISTORY {
            let overflow = self.tab_navigation_history.len() - MAX_TAB_HISTORY;
            self.tab_navigation_history.drain(0..overflow);
        }
        self.tab_navigation_index = self.tab_navigation_history.len().checked_sub(1);
    }

    pub(super) fn navigate_tab_history(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prune_tab_navigation_history();
        let Some(mut index) = self.tab_navigation_index else {
            return;
        };

        loop {
            if forward {
                if index + 1 >= self.tab_navigation_history.len() {
                    return;
                }
                index += 1;
            } else if index == 0 {
                return;
            } else {
                index -= 1;
            }

            let tab_id = self.tab_navigation_history[index];
            if self.tabs.iter().any(|tab| tab.id == tab_id) {
                self.tab_navigation_index = Some(index);
                self.tab_navigation_replaying = true;
                self.active_tab_id = Some(tab_id);
                self.sync_active_tab_surface();
                self.needs_active_pane_focus = self.active_tab().is_some_and(|tab| {
                    matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal)
                });
                self.focus_active_pane(window, cx);
                self.reveal_active_tab(window);
                cx.notify();
                return;
            }
        }
    }

    fn prune_tab_navigation_history(&mut self) {
        let existing = self.tabs.iter().map(|tab| tab.id).collect::<HashSet<_>>();
        let current = self
            .tab_navigation_index
            .and_then(|index| self.tab_navigation_history.get(index).copied());
        self.tab_navigation_history
            .retain(|tab_id| existing.contains(tab_id));
        self.tab_navigation_index = current
            .and_then(|tab_id| {
                self.tab_navigation_history
                    .iter()
                    .position(|candidate| *candidate == tab_id)
            })
            .or_else(|| self.tab_navigation_history.len().checked_sub(1));
    }

    pub(super) fn set_active_tab(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.iter().any(|tab| tab.id == tab_id) {
            self.active_tab_id = Some(tab_id);
            self.sync_active_tab_surface();
            self.needs_active_pane_focus = self.active_tab().is_some_and(|tab| {
                matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal)
            });
            self.focus_active_pane(window, cx);
            self.reveal_active_tab(window);
            cx.notify();
        }
    }

    pub(super) fn sync_active_tab_surface(&mut self) {
        // Tauri keeps the SSH session tree independent from terminal tab focus,
        // but app-level utility tabs still light up their owning activity icon.
        // Keep terminal/SFTP/IDE ownership separate while syncing these sidebar
        // entry tabs so the selected icon frame follows the visible surface.
        match self.active_tab().map(|tab| &tab.kind) {
            Some(TabKind::Settings) => {
                self.active_surface = ActiveSurface::Settings;
            }
            Some(TabKind::Forwards) => {
                self.active_surface = ActiveSurface::Terminal;
                if let Some(active_tab_id) = self.active_tab_id
                    && let Some(node_id) = self.forward_tab_nodes.get(&active_tab_id).cloned()
                {
                    self.active_ssh_node_id = Some(node_id.clone());
                    self.expanded_ssh_nodes.insert(node_id.clone());
                    self.start_port_profiler_for_node_without_notify(node_id);
                }
            }
            Some(TabKind::Sftp) => {
                self.active_surface = ActiveSurface::Terminal;
                if let Some(active_tab_id) = self.active_tab_id
                    && let Some(node_id) = self.sftp_tab_nodes.get(&active_tab_id).cloned()
                {
                    self.active_ssh_node_id = Some(node_id.clone());
                    self.expanded_ssh_nodes.insert(node_id.clone());
                    self.activate_sftp_view_for_node(&node_id);
                }
            }
            Some(TabKind::Ide) => {
                self.active_surface = ActiveSurface::Terminal;
                if let Some(active_tab_id) = self.active_tab_id
                    && let Some(node_id) = self.ide_tab_nodes.get(&active_tab_id)
                {
                    self.active_ssh_node_id = Some(node_id.clone());
                    self.expanded_ssh_nodes.insert(node_id.clone());
                }
            }
            Some(TabKind::SessionManager) => {
                self.active_surface = ActiveSurface::Terminal;
                self.active_sidebar_section = SidebarSection::Connections;
            }
            Some(TabKind::ConnectionPool) => {
                self.active_surface = ActiveSurface::Terminal;
                self.active_sidebar_section = SidebarSection::Terminal;
            }
            Some(TabKind::ConnectionMonitor) => {
                self.active_surface = ActiveSurface::Terminal;
                self.active_sidebar_section = SidebarSection::Activity;
            }
            Some(TabKind::Topology) => {
                self.active_surface = ActiveSurface::Terminal;
                self.active_sidebar_section = SidebarSection::Network;
            }
            Some(TabKind::NotificationCenter) => {
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::PluginManager) => {
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::CloudSync) => {
                self.active_surface = ActiveSurface::Terminal;
            }
            _ => {
                self.active_surface = ActiveSurface::Terminal;
            }
        }
        if let Some(session_id) = self.active_terminal_session_id()
            && let Some(node_id) = self.terminal_ssh_nodes.get(&session_id)
        {
            self.active_ssh_node_id = Some(node_id.clone());
            self.expanded_ssh_nodes.insert(node_id.clone());
        }
    }

    pub(super) fn focus_active_pane(&mut self, window: &mut Window, cx: &App) {
        self.clear_ai_sidebar_keyboard_focus();
        if let Some(pane) = self.active_pane() {
            pane.read(cx).focus(window);
        } else {
            window.focus(&self.focus_handle);
        }
    }

    fn register_ssh_terminal_session(
        &mut self,
        node_id: NodeId,
        saved_connection_id: Option<String>,
        config: SshConfig,
        title: String,
        session_id: TerminalSessionId,
    ) {
        self.terminal_ssh_nodes.insert(session_id, node_id.clone());
        self.expanded_ssh_nodes.insert(node_id.clone());
        self.active_ssh_node_id = Some(node_id.clone());
        if let Some(saved_connection_id) = saved_connection_id.as_ref() {
            self.saved_ssh_nodes
                .insert(saved_connection_id.clone(), node_id.clone());
        }

        self.ssh_nodes
            .entry(node_id.clone())
            .and_modify(|node| {
                node.config = config.clone();
                node.title = title.clone();
                // Adding another terminal opens a new shell channel on the
                // node-owned SSH connection. Tauri does not downgrade an
                // already-connected node to Connecting for that session-level
                // operation, so preserve Ready here to avoid tree-wide status
                // churn when users open terminal #2/#3/#4.
                if !matches!(node.readiness, NodeReadiness::Ready) {
                    node.readiness = NodeReadiness::Connecting;
                }
                if !node.terminal_ids.contains(&session_id) {
                    node.terminal_ids.push(session_id);
                }
                if node.saved_connection_id.is_none() {
                    node.saved_connection_id = saved_connection_id.clone();
                }
            })
            .or_insert_with(|| WorkspaceSshNode {
                saved_connection_id,
                config,
                title,
                terminal_ids: vec![session_id],
                readiness: NodeReadiness::Connecting,
            });
    }

    pub(super) fn unregister_ssh_terminal_session(&mut self, session_id: TerminalSessionId) {
        let forwarding_registry = self.forwarding_registry.clone();
        let forwarding_runtime = self.forwarding_runtime.clone();
        let forwarding_session_id = session_id.0.to_string();
        if let Some((connection_id, consumer)) = self
            .forwarding_connection_consumers
            .remove(&forwarding_session_id)
        {
            self.ssh_registry.release(&connection_id, &consumer);
        }
        forwarding_runtime.spawn(async move {
            let _ = forwarding_registry.remove(&forwarding_session_id).await;
        });

        // Drop the SessionRegistry-shaped owner before unbinding the node
        // endpoint, matching Tauri's terminal close cleanup order: endpoint
        // metadata is removed without touching the node's SSH connection.
        let endpoint_session = self.terminal_endpoint_sessions.remove(&session_id);
        let Some(node_id) = self.terminal_ssh_nodes.remove(&session_id) else {
            return;
        };
        // Tauri terminal close only removes the terminal/session mapping.
        // Do not health-probe here: a closed shell channel is not evidence
        // that the node-owned SSH transport died, and probing on the last
        // terminal close can incorrectly drive the node into LinkDown.
        let Some(node) = self.ssh_nodes.get_mut(&node_id) else {
            return;
        };
        node.terminal_ids.retain(|id| *id != session_id);
        if node.terminal_ids.is_empty() {
            let endpoint_session_id = endpoint_session
                .as_ref()
                .map(|owner| owner.endpoint.session_id.clone())
                .unwrap_or_else(|| session_id.0.to_string());
            let _ = self
                .node_router
                .unbind_terminal_session(&node_id, &endpoint_session_id);
        }
        self.persist_session_tree_snapshot();
    }

    pub(super) fn focus_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((tab_id, pane_id)) = self.tabs.iter().find_map(|tab| {
            tab.root_pane
                .as_ref()
                .and_then(|root| root.pane_id_for_session(session_id))
                .map(|pane_id| (tab.id, pane_id))
        }) else {
            return false;
        };
        self.active_tab_id = Some(tab_id);
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) {
            tab.active_pane_id = Some(pane_id);
        }
        if let Some(node_id) = self.terminal_ssh_nodes.get(&session_id)
            && let Some(node) = self.ssh_nodes.get_mut(node_id)
        {
            node.readiness = NodeReadiness::Ready;
            self.active_ssh_node_id = Some(node_id.clone());
            self.expanded_ssh_nodes.insert(node_id.clone());
        }
        self.sync_active_tab_surface();
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window);
        cx.notify();
        true
    }

    pub(super) fn close_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_terminal_session(session_id, window, cx) {
            return;
        }
        let single_pane_tab = self
            .active_tab()
            .and_then(|tab| tab.root_pane.as_ref())
            .is_none_or(|root| root.pane_count() <= 1);
        if single_pane_tab {
            self.close_active_tab(window, cx);
        } else {
            self.close_active_pane(window, cx);
        }
    }

    pub(super) fn disconnect_ssh_node(
        &mut self,
        node_id: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ssh_nodes.contains_key(node_id) {
            return;
        }

        let mut nodes_to_disconnect = self.node_runtime_store.subtree_postorder(node_id);
        if nodes_to_disconnect.is_empty() {
            nodes_to_disconnect.push(node_id.clone());
        }
        for affected_node_id in &nodes_to_disconnect {
            self.cancel_connection_trace_for_node(affected_node_id);
            self.abort_connection_chain_for_node(affected_node_id);
            self.reconnect_orchestrator.cancel(&affected_node_id.0);
            self.cancel_forward_restore_token(affected_node_id);
            self.pending_reconnect_node_ids.remove(affected_node_id);
            self.reconnect_requeue_counts.remove(affected_node_id);
            self.pending_reconnect_cascade_nodes
                .retain(|pending_node_id| pending_node_id != affected_node_id);
            if self
                .reconnect_pipeline_active_node
                .as_ref()
                .is_some_and(|active_node_id| active_node_id == affected_node_id)
            {
                self.reconnect_pipeline_active_node = None;
            }
            let _ = self.interrupt_sftp_transfers_by_node(
                affected_node_id,
                "Connection closed".to_string(),
            );
        }
        for affected_node_id in &nodes_to_disconnect {
            self.forwarding_port_profiler_nodes.remove(affected_node_id);
            self.forwarding_port_detection_by_node.remove(affected_node_id);
            let forwarding_registry = self.forwarding_registry.clone();
            let forwarding_runtime = self.forwarding_runtime.clone();
            let forwarding_session_id = self.forwarding_session_id_for_node(affected_node_id);
            self.release_forwarding_binding_for_node(affected_node_id);
            forwarding_runtime.spawn(async move {
                let _ = forwarding_registry.remove(&forwarding_session_id).await;
            });
        }

        // Tauri's `disconnectNode` closes tabs by affected nodeId, not just by
        // terminal session id. Keep SFTP/forwards tabs from surviving as orphaned
        // node-scoped surfaces after an explicit disconnect.
        for affected_node_id in &nodes_to_disconnect {
            self.close_tabs_for_node(affected_node_id, window, cx);

            if let Some(connection_id) = self.node_router.connection_id_for_node(affected_node_id) {
                let node_consumer = ConnectionConsumer::NodeRouter(affected_node_id.0.clone());
                self.ssh_registry.release(&connection_id, &node_consumer);
                self.release_parent_ref_for_child_connection(affected_node_id, &connection_id);
                if let Some(handle) = self.ssh_registry.get(&connection_id) {
                    let runtime = self.forwarding_runtime.clone();
                    runtime.spawn(async move {
                        handle.clear_physical().await;
                    });
                }
                if let Some(info) = self
                    .ssh_registry
                    .mark_state(&connection_id, ConnectionState::Disconnected)
                    && let Some(event) = self
                        .node_router
                        .sync_connection_state_by_connection_id(&info, "explicit disconnect")
                {
                    self.emit_node_event(event);
                }
                self.node_router.emitter().unregister(&connection_id);
                let _ = self.ssh_registry.retire_connection(&connection_id);
            }

            if let Some(node) = self.ssh_nodes.get_mut(affected_node_id) {
                // Tauri disconnect_tree_node marks every affected subtree node
                // as Disconnected. Link-down propagation uses Error elsewhere;
                // explicit user disconnect should not look like a failure.
                node.readiness = NodeReadiness::Disconnected;
                node.terminal_ids.clear();
            }
            if let Ok(event) = self
                .node_router
                .disconnect_node_runtime(affected_node_id, "explicit disconnect")
            {
                self.emit_node_event(event);
            }
        }
        self.persist_session_tree_snapshot();
        cx.notify();
    }

    pub(super) fn close_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.active_tab_index() else {
            return;
        };
        self.close_tab_at_index(index, window, cx);
    }

    pub(super) fn request_close_active_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.active_tab_index() else {
            return;
        };
        let tab_id = self.tabs[index].id;
        if self.tabs[index].kind == TabKind::SshTerminal {
            // Tauri confirms user-initiated SSH terminal tab closes while
            // still allowing backend/session cleanup paths to close directly.
            self.tab_close_confirm = Some(TabCloseConfirm::Single { tab_id });
            self.reset_standard_confirm_focus();
            cx.notify();
            return;
        }
        if self.tabs[index].kind == TabKind::LocalTerminal
            && self.local_terminal_tab_has_foreground_child_process(index, cx)
        {
            // Tauri warns before closing a local terminal whose foreground
            // process is not the original shell. Native derives the same guard
            // from the PTY process snapshot refreshed by the terminal tick.
            self.tab_close_confirm = Some(TabCloseConfirm::LocalChildProcess { tab_id });
            self.reset_standard_confirm_focus();
            cx.notify();
            return;
        }
        self.close_tab_at_index(index, window, cx);
    }

    pub(super) fn close_tab_by_id(&mut self, tab_id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        self.close_tab_at_index(index, window, cx);
    }

    pub(super) fn close_other_tabs_or_active_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_tab_id) = self.active_tab_id else {
            return;
        };
        if self
            .active_tab()
            .is_some_and(|tab| matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal))
        {
            if self
                .active_tab()
                .and_then(|tab| tab.root_pane.as_ref())
                .is_some_and(|root| root.pane_count() > 1)
            {
                self.close_active_pane(window, cx);
            }
            return;
        }

        let tab_ids = self
            .tabs
            .iter()
            .filter(|tab| tab.id != active_tab_id)
            .map(|tab| tab.id)
            .collect::<Vec<_>>();
        for tab_id in tab_ids {
            self.close_tab_by_id(tab_id, window, cx);
        }
    }

    pub(super) fn request_close_other_tabs_or_active_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_tab_id) = self.active_tab_id else {
            return;
        };
        if self
            .active_tab()
            .is_some_and(|tab| matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal))
        {
            if self
                .active_tab()
                .and_then(|tab| tab.root_pane.as_ref())
                .is_some_and(|root| root.pane_count() > 1)
            {
                self.close_active_pane(window, cx);
            }
            return;
        }

        let tab_ids = self
            .tabs
            .iter()
            .filter(|tab| tab.id != active_tab_id)
            .map(|tab| tab.id)
            .collect::<Vec<_>>();
        if tab_ids.is_empty() {
            return;
        }
        if self.tab_close_ids_include_ssh_terminal(&tab_ids) {
            self.tab_close_confirm = Some(TabCloseConfirm::Other { tab_ids });
            self.reset_standard_confirm_focus();
            cx.notify();
            return;
        }
        if self.tab_close_ids_include_local_foreground_child_process(&tab_ids, cx) {
            self.tab_close_confirm = Some(TabCloseConfirm::LocalChildProcessBatch { tab_ids });
            self.reset_standard_confirm_focus();
            cx.notify();
            return;
        }
        self.close_other_tabs_or_active_pane(window, cx);
    }

    fn tab_close_ids_include_ssh_terminal(&self, tab_ids: &[TabId]) -> bool {
        tab_ids.iter().any(|tab_id| {
            self.tabs
                .iter()
                .any(|tab| tab.id == *tab_id && tab.kind == TabKind::SshTerminal)
        })
    }

    fn tab_close_ids_include_local_foreground_child_process(
        &self,
        tab_ids: &[TabId],
        cx: &mut Context<Self>,
    ) -> bool {
        tab_ids.iter().any(|tab_id| {
            self.tabs
                .iter()
                .position(|tab| tab.id == *tab_id && tab.kind == TabKind::LocalTerminal)
                .is_some_and(|index| self.local_terminal_tab_has_foreground_child_process(index, cx))
        })
    }

    fn local_terminal_tab_has_foreground_child_process(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(root_pane) = self.tabs.get(index).and_then(|tab| tab.root_pane.as_ref()) else {
            return false;
        };
        let mut pane_ids = Vec::new();
        root_pane.collect_pane_ids(&mut pane_ids);
        pane_ids.into_iter().any(|pane_id| {
            let Some(pane) = self.panes.get(&pane_id) else {
                return false;
            };
            let process = pane.read(cx).process_info();
            terminal_process_info_has_foreground_child_process(&process)
        })
    }

    pub(super) fn cancel_tab_close_confirm(&mut self, cx: &mut Context<Self>) {
        self.tab_close_confirm = None;
        self.clear_standard_confirm_focus();
        cx.notify();
    }

    pub(super) fn confirm_tab_close_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(confirm) = self.tab_close_confirm.take() else {
            return;
        };
        self.clear_standard_confirm_focus();
        match confirm {
            TabCloseConfirm::Single { tab_id } => {
                self.close_tab_by_id(tab_id, window, cx);
            }
            TabCloseConfirm::LocalChildProcess { tab_id } => {
                self.close_tab_by_id(tab_id, window, cx);
            }
            TabCloseConfirm::Other { tab_ids } => {
                if self.tab_close_ids_include_local_foreground_child_process(&tab_ids, cx) {
                    self.tab_close_confirm =
                        Some(TabCloseConfirm::LocalChildProcessBatch { tab_ids });
                    self.reset_standard_confirm_focus();
                    cx.notify();
                    return;
                }
                for tab_id in tab_ids {
                    self.close_tab_by_id(tab_id, window, cx);
                }
            }
            TabCloseConfirm::LocalChildProcessBatch { tab_ids } => {
                for tab_id in tab_ids {
                    self.close_tab_by_id(tab_id, window, cx);
                }
            }
        }
    }

    pub(super) fn focus_adjacent_pane(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_pane_id) = self.active_pane_id() else {
            return;
        };
        let mut pane_ids = Vec::new();
        if let Some(root) = self.active_tab().and_then(|tab| tab.root_pane.as_ref()) {
            root.collect_pane_ids(&mut pane_ids);
        }
        if pane_ids.len() < 2 {
            return;
        }
        let Some(index) = pane_ids.iter().position(|pane_id| *pane_id == active_pane_id) else {
            return;
        };
        let next_index = if forward {
            (index + 1) % pane_ids.len()
        } else if index == 0 {
            pane_ids.len() - 1
        } else {
            index - 1
        };
        let next_pane_id = pane_ids[next_index];
        if let Some(tab) = self.active_tab_mut() {
            tab.active_pane_id = Some(next_pane_id);
        }
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    fn close_tab_at_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let old_active_tab_id = self.active_tab_id;
        let removed_was_active = self.tabs.get(index).map(|tab| tab.id) == old_active_tab_id;
        let tab = self.tabs.remove(index);
        if tab.kind == TabKind::Graphics {
            self.shutdown_graphics_session();
        }
        // Tauri keeps node SFTP alive when the SFTP tab is closed; the tab is
        // only a view over the node-owned ConnectionEntry session.
        self.sftp_tab_nodes.remove(&tab.id);
        if let Some(surface) = self.ide_tab_surfaces.remove(&tab.id) {
            surface.update(cx, |surface, cx| {
                surface.release_remote_session(cx);
            });
        }
        self.ide_surface_subscriptions.remove(&tab.id);
        if let Some(node_id) = self.ide_tab_nodes.remove(&tab.id) {
            // Tauri appStore.closeTab() calls ideStore.closeProject(true) when
            // the IDE tab goes away, and closeProject records lastClosedAt so
            // reconnect does not resurrect a project the user intentionally
            // closed after the snapshot.
            self.ide_last_closed_at_by_node
                .insert(node_id, SystemTime::now());
        }
        self.forward_tab_nodes.remove(&tab.id);
        let mut pane_ids = Vec::new();
        let mut session_ids = Vec::new();
        if let Some(root_pane) = &tab.root_pane {
            root_pane.collect_pane_ids(&mut pane_ids);
            root_pane.collect_session_ids(&mut session_ids);
        }
        for session_id in session_ids {
            self.unregister_ssh_terminal_session(session_id);
        }
        for pane_id in pane_ids {
            if let Some(pane) = self.panes.remove(&pane_id) {
                let _ = pane.update(cx, |pane, _cx| pane.shutdown());
            }
        }

        self.active_tab_id = if self.tabs.is_empty() {
            None
        } else if !removed_was_active
            && old_active_tab_id.is_some_and(|tab_id| self.tabs.iter().any(|tab| tab.id == tab_id))
        {
            old_active_tab_id
        } else {
            Some(self.tabs[index.min(self.tabs.len() - 1)].id)
        };
        self.sync_active_tab_surface();
        self.needs_active_pane_focus = self
            .active_tab()
            .is_some_and(|tab| matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal));
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window);
        cx.notify();
    }

    fn close_tabs_for_node(
        &mut self,
        node_id: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_ids = self
            .tabs
            .iter()
            .filter(|tab| self.tab_belongs_to_node(tab, node_id))
            .map(|tab| tab.id)
            .collect::<Vec<_>>();
        for tab_id in tab_ids {
            self.close_tab_by_id(tab_id, window, cx);
        }
    }

    fn tab_belongs_to_node(&self, tab: &Tab, node_id: &NodeId) -> bool {
        if self.sftp_tab_nodes.get(&tab.id) == Some(node_id) {
            return true;
        }
        if self.ide_tab_nodes.get(&tab.id) == Some(node_id) {
            return true;
        }
        if self.forward_tab_nodes.get(&tab.id) == Some(node_id) {
            return true;
        }
        let mut session_ids = Vec::new();
        if let Some(root_pane) = &tab.root_pane {
            root_pane.collect_session_ids(&mut session_ids);
        }
        session_ids
            .into_iter()
            .any(|session_id| self.terminal_ssh_nodes.get(&session_id) == Some(node_id))
    }

    pub(super) fn next_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let current = self.active_tab_index().unwrap_or(0);
        let next = if forward {
            (current + 1) % self.tabs.len()
        } else if current == 0 {
            self.tabs.len() - 1
        } else {
            current - 1
        };
        self.active_tab_id = Some(self.tabs[next].id);
        self.sync_active_tab_surface();
        self.needs_active_pane_focus = self
            .active_tab()
            .is_some_and(|tab| matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal));
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window);
        cx.notify();
    }

    pub(super) fn go_to_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(index) {
            self.active_tab_id = Some(tab.id);
            self.sync_active_tab_surface();
            self.needs_active_pane_focus = self.active_tab().is_some_and(|tab| {
                matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal)
            });
            self.focus_active_pane(window, cx);
            self.reveal_active_tab(window);
            cx.notify();
        }
    }

    fn tabbar_viewport_width(&self, window: &Window) -> f32 {
        let window_width = f32::from(window.inner_window_bounds().get_bounds().size.width);
        let sidebar_width = if self.sidebar_collapsed {
            self.tokens.metrics.activity_bar_width
        } else {
            self.sidebar_width
        };
        let ai_sidebar_width = if self.ai_sidebar_visible() {
            self.ai_sidebar_width
        } else {
            0.0
        };
        (window_width - sidebar_width - ai_sidebar_width).max(0.0)
    }

    fn tabbar_left_x(&self) -> f32 {
        if self.sidebar_collapsed {
            self.tokens.metrics.activity_bar_width
        } else {
            self.sidebar_width
        }
    }

    fn tabbar_content_width(&self) -> f32 {
        self.tokens.metrics.tabbar_leading_offset
            + self
                .tabs
                .iter()
                .map(|tab| self.tab_visual_width(tab))
                .sum::<f32>()
    }

    fn tabbar_max_scroll(&self, window: &Window) -> f32 {
        (self.tabbar_content_width() - self.tabbar_viewport_width(window)).max(0.0)
    }

    fn clamp_tab_scroll(&mut self, window: &Window) {
        self.tab_scroll_x = self.tab_scroll_x.clamp(0.0, self.tabbar_max_scroll(window));
    }

    fn tabbar_has_overflow(&self, window: &Window) -> bool {
        self.tabbar_max_scroll(window) > 1.0
    }

    pub(super) fn tabbar_effective_scroll_x(&self, window: &Window) -> f32 {
        if self.tabbar_has_overflow(window) {
            self.tab_scroll_x.clamp(0.0, self.tabbar_max_scroll(window))
        } else {
            0.0
        }
    }

    pub(super) fn reveal_active_tab(&mut self, window: &Window) {
        let Some(index) = self.active_tab_index() else {
            self.clamp_tab_scroll(window);
            return;
        };
        let tab_left = self.tokens.metrics.tabbar_leading_offset
            + self
                .tabs
                .iter()
                .take(index)
                .map(|tab| self.tab_visual_width(tab))
                .sum::<f32>();
        let tab_right = tab_left + self.tab_visual_width(&self.tabs[index]);
        let viewport_width = self.tabbar_viewport_width(window);

        if tab_left < self.tab_scroll_x {
            self.tab_scroll_x = tab_left;
        } else if tab_right > self.tab_scroll_x + viewport_width {
            self.tab_scroll_x = tab_right - viewport_width;
        }
        self.clamp_tab_scroll(window);
    }

    pub(super) fn tab_display_title(&self, tab: &Tab) -> String {
        let title = match tab.title_source {
            TabTitleSource::Static => tab.title.clone(),
            TabTitleSource::I18nKey(key) => self.i18n.t(key),
        };
        if matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal) {
            let pane_count = tab.root_pane.as_ref().map_or(1, PaneNode::pane_count);
            if pane_count > 1 {
                return format!("{title} ({pane_count})");
            }
        }
        title
    }

    fn tab_visual_width(&self, tab: &Tab) -> f32 {
        let metrics = self.tokens.metrics;
        let title = self.tab_display_title(tab);
        let title_width = title
            .chars()
            .map(|ch| {
                if ch.is_ascii() {
                    metrics.tab_font_size * metrics.tab_title_width_ratio
                } else {
                    metrics.tab_font_size
                }
            })
            .sum::<f32>();
        let fixed_width = metrics.tab_padding_x * 2.0
            + metrics.tab_icon_size
            + metrics.tab_gap * 2.0
            + metrics.tab_close_button_size;

        (title_width + fixed_width).clamp(metrics.tab_min_width, metrics.tab_max_width)
    }

    fn tab_drop_target_index_for_x(
        &self,
        client_x: f32,
        window: &Window,
        tab_widths: &[f32],
    ) -> usize {
        if tab_widths.is_empty() {
            return 0;
        }
        let tabbar_x = client_x - self.tabbar_left_x() + self.tabbar_effective_scroll_x(window)
            - self.tokens.metrics.tabbar_leading_offset;
        let mut left = 0.0;
        for (index, width) in tab_widths.iter().copied().enumerate() {
            let midpoint = left + width / 2.0;
            if tabbar_x < midpoint {
                return index;
            }
            left += width;
        }
        tab_widths.len() - 1
    }

    pub(super) fn start_tab_drag_candidate(
        &mut self,
        tab_id: TabId,
        index: usize,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.tabs.len() || self.tabs.get(index).is_none_or(|tab| tab.id != tab_id) {
            return;
        }
        let start_x = f32::from(event.position.x);
        let tab_widths = self
            .tabs
            .iter()
            .map(|tab| self.tab_visual_width(tab))
            .collect::<Vec<_>>();
        let drop_target_index = self.tab_drop_target_index_for_x(start_x, window, &tab_widths);
        self.tab_drag = Some(TabDragState {
            tab_id,
            from_index: index,
            start_x,
            current_x: start_x,
            tab_widths,
            active: false,
            drop_target_index,
        });
        cx.notify();
    }

    pub(super) fn update_tab_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut drag) = self.tab_drag.clone() else {
            return;
        };
        // Browser tab drags keep pointer capture after leaving the tab label;
        // the root mouse-up is responsible for finishing or cancelling.
        drag.current_x = f32::from(event.position.x);
        let delta = (drag.current_x - drag.start_x).abs();
        // Tauri TabBar uses a 10px pointer threshold before a pointer gesture
        // becomes a reorder; below that threshold the later mouse-up is a click.
        if delta > TAB_DRAG_THRESHOLD_PX {
            drag.active = true;
            drag.drop_target_index =
                self.tab_drop_target_index_for_x(drag.current_x, window, &drag.tab_widths);
        } else {
            drag.drop_target_index = drag.from_index;
        }
        self.tab_drag = Some(drag);
        cx.notify();
    }

    pub(super) fn finish_tab_drag(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        let Some(drag) = self.tab_drag.take() else {
            return;
        };
        if drag.active {
            self.move_tab(drag.from_index, drag.drop_target_index, window, cx);
        } else if self.tabs.get(drag.from_index).is_some_and(|tab| tab.id == drag.tab_id) {
            self.set_active_tab(drag.tab_id, window, cx);
        }
        cx.notify();
    }

    fn move_tab(
        &mut self,
        from_index: usize,
        to_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if from_index == to_index || from_index >= self.tabs.len() || to_index >= self.tabs.len() {
            return;
        }
        let moved = self.tabs.remove(from_index);
        self.tabs.insert(to_index, moved);
        self.clamp_tab_scroll(window);
        self.reveal_active_tab(window);
        cx.notify();
    }

    pub(super) fn handle_tabbar_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.tabbar_has_overflow(window) {
            if self.tab_scroll_x != 0.0 {
                self.tab_scroll_x = 0.0;
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        let delta = event
            .delta
            .pixel_delta(px(self.tokens.metrics.tabbar_height));
        let horizontal = if f32::from(delta.x).abs() > f32::from(delta.y).abs() {
            f32::from(delta.x)
        } else {
            f32::from(delta.y)
        };
        if horizontal == 0.0 {
            return;
        }

        self.tab_scroll_x += horizontal;
        self.clamp_tab_scroll(window);
        cx.stop_propagation();
        cx.notify();
    }
}

fn terminal_process_info_has_foreground_child_process(
    process: &oxideterm_terminal::TerminalProcessInfo,
) -> bool {
    let Some(shell_pid) = process.shell_pid else {
        return false;
    };
    process
        .foreground_process_group_id
        .is_some_and(|foreground_group| foreground_group != shell_pid)
        || process
            .foreground_pid
            .is_some_and(|foreground_pid| foreground_pid != shell_pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_close_warning_detects_foreground_child_process() {
        let shell_only = oxideterm_terminal::TerminalProcessInfo {
            shell_pid: Some(10),
            foreground_pid: Some(10),
            foreground_process_group_id: Some(10),
            ..Default::default()
        };
        assert!(!terminal_process_info_has_foreground_child_process(
            &shell_only
        ));

        let foreground_child = oxideterm_terminal::TerminalProcessInfo {
            shell_pid: Some(10),
            foreground_pid: Some(42),
            foreground_process_group_id: Some(42),
            ..Default::default()
        };
        assert!(terminal_process_info_has_foreground_child_process(
            &foreground_child
        ));
    }
}
