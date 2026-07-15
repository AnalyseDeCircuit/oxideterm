use super::*;

fn tab_exit_visual_index(live_visual_index: usize, occupied_indices: &[usize]) -> usize {
    let mut visual_index = live_visual_index;
    for occupied in occupied_indices {
        if *occupied <= visual_index {
            visual_index += 1;
        }
    }
    visual_index
}

pub(super) const TAB_DRAG_THRESHOLD_PX: f32 = 10.0;

fn tab_drag_is_horizontal_reorder(delta_x: f32, delta_y: f32) -> bool {
    let horizontal = delta_x.abs();
    let vertical = delta_y.abs();
    horizontal > TAB_DRAG_THRESHOLD_PX && horizontal >= vertical
}

fn tab_drag_is_detach(delta_x: f32, delta_y: f32, tabbar_height: f32) -> bool {
    let threshold = (tabbar_height * 0.72).max(24.0);
    delta_y > threshold && delta_y.abs() >= delta_x.abs() * 0.85
}

// Pointer hit-testing returns a slot in the pre-removal strip. Moving right
// must discount the source tab after it is removed from that strip.
pub(super) fn tab_reorder_target_visible_index(
    source_visible_index: usize,
    insertion_slot: usize,
) -> usize {
    insertion_slot.saturating_sub(usize::from(source_visible_index < insertion_slot))
}

fn tabbar_tauri_wheel_scroll_delta(delta_x: f32, delta_y: f32) -> f32 {
    if delta_y != 0.0 { delta_y } else { delta_x }
}

// GPUI wheel deltas are applied to negative scroll offsets. The tab bar keeps a
// browser-like positive scrollLeft value, so advancing the strip subtracts delta.
fn tabbar_scroll_x_after_wheel(current_scroll_x: f32, wheel_delta: f32, max_scroll: f32) -> f32 {
    (current_scroll_x - wheel_delta).clamp(0.0, max_scroll)
}

fn attach_terminal_to_existing_ssh_node(
    node: &mut WorkspaceSshNode,
    saved_connection_id: Option<String>,
    config: SshConfig,
    session_id: TerminalSessionId,
) {
    node.config = config;
    // Terminal tab titles are per-tab state. Never let a Docker exec/logs tab,
    // quick command tab, or other one-off title rename the host node itself.
    if !matches!(node.readiness, NodeReadiness::Ready) {
        node.readiness = NodeReadiness::Connecting;
    }
    if !node.terminal_ids.contains(&session_id) {
        node.terminal_ids.push(session_id);
    }
    if node.saved_connection_id.is_none() {
        node.saved_connection_id = saved_connection_id;
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn observe_active_tab_for_history(&mut self) {
        let active_tab_id = self.main_window_tabs.active_tab_id;
        if self.main_window_tabs.navigation_observed_tab == active_tab_id {
            return;
        }
        self.main_window_tabs.navigation_observed_tab = active_tab_id;

        let Some(tab_id) = active_tab_id else {
            return;
        };
        if self.main_window_tabs.navigation_replaying {
            self.main_window_tabs.navigation_replaying = false;
            return;
        }

        if let Some(index) = self.main_window_tabs.navigation_index {
            self.main_window_tabs
                .navigation_history
                .truncate(index.saturating_add(1));
        }
        if self.main_window_tabs.navigation_history.last().copied() != Some(tab_id) {
            self.main_window_tabs.navigation_history.push(tab_id);
        }
        const MAX_TAB_HISTORY: usize = 50;
        if self.main_window_tabs.navigation_history.len() > MAX_TAB_HISTORY {
            let overflow = self.main_window_tabs.navigation_history.len() - MAX_TAB_HISTORY;
            self.main_window_tabs.navigation_history.drain(0..overflow);
        }
        self.main_window_tabs.navigation_index = self
            .main_window_tabs
            .navigation_history
            .len()
            .checked_sub(1);
    }

    pub(in crate::workspace) fn navigate_tab_history(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prune_tab_navigation_history();
        let Some(mut index) = self.main_window_tabs.navigation_index else {
            return;
        };

        loop {
            if forward {
                if index + 1 >= self.main_window_tabs.navigation_history.len() {
                    return;
                }
                index += 1;
            } else if index == 0 {
                return;
            } else {
                index -= 1;
            }

            let tab_id = self.main_window_tabs.navigation_history[index];
            if self
                .tabs
                .iter()
                .any(|tab| tab.id == tab_id && !self.detached_tabs.contains(&tab.id))
            {
                self.main_window_tabs.navigation_index = Some(index);
                self.main_window_tabs.navigation_replaying = true;
                self.main_window_tabs.active_tab_id = Some(tab_id);
                self.sync_active_tab_surface();
                self.needs_active_pane_focus = self.active_tab().is_some_and(|tab| {
                    matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal)
                });
                self.focus_active_tab_keyboard_owner(window, cx);
                self.reveal_active_tab(window);
                cx.notify();
                return;
            }
        }
    }

    fn prune_tab_navigation_history(&mut self) {
        let existing = self
            .tabs
            .iter()
            .filter(|tab| !self.detached_tabs.contains(&tab.id))
            .map(|tab| tab.id)
            .collect::<HashSet<_>>();
        let current = self
            .main_window_tabs
            .navigation_index
            .and_then(|index| self.main_window_tabs.navigation_history.get(index).copied());
        self.main_window_tabs
            .navigation_history
            .retain(|tab_id| existing.contains(tab_id));
        self.main_window_tabs.navigation_index = current
            .and_then(|tab_id| {
                self.main_window_tabs
                    .navigation_history
                    .iter()
                    .position(|candidate| *candidate == tab_id)
            })
            .or_else(|| {
                self.main_window_tabs
                    .navigation_history
                    .len()
                    .checked_sub(1)
            });
    }

    pub(in crate::workspace) fn set_active_tab(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .tabs
            .iter()
            .any(|tab| tab.id == tab_id && !self.detached_tabs.contains(&tab.id))
        {
            if self.main_window_tabs.active_tab_id != Some(tab_id)
                && let Some(previous_tab_id) = self.main_window_tabs.active_tab_id
            {
                // Remote desktops keep server-side input state. Release it when
                // the tab loses focus so modifiers or mouse buttons cannot stick
                // on the remote host while the user works elsewhere.
                self.release_remote_desktop_inputs_for_tab(previous_tab_id);
            }
            self.main_window_tabs.active_tab_id = Some(tab_id);
            self.sync_active_tab_surface();
            self.needs_active_pane_focus = self.active_tab().is_some_and(|tab| {
                matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal)
            });
            self.focus_active_tab_keyboard_owner(window, cx);
            self.reveal_active_tab(window);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn sync_active_tab_surface(&mut self) {
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
                if let Some(active_tab_id) = self.main_window_tabs.active_tab_id
                    && let Some(node_id) = self.forward_tab_nodes.get(&active_tab_id).cloned()
                {
                    self.active_ssh_node_id = Some(node_id.clone());
                    self.expanded_ssh_nodes.insert(node_id.clone());
                    self.start_port_profiler_for_node_without_notify(node_id);
                }
            }
            Some(TabKind::Sftp) => {
                self.active_surface = ActiveSurface::Terminal;
                if let Some(active_tab_id) = self.main_window_tabs.active_tab_id
                    && let Some(node_id) = self.sftp_tab_nodes.get(&active_tab_id).cloned()
                {
                    self.active_ssh_node_id = Some(node_id.clone());
                    self.expanded_ssh_nodes.insert(node_id.clone());
                    self.activate_sftp_view_for_node(&node_id);
                }
            }
            Some(TabKind::Ide) => {
                self.active_surface = ActiveSurface::Terminal;
                if let Some(active_tab_id) = self.main_window_tabs.active_tab_id
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
            Some(TabKind::Runtime) => {
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::ConnectionPool) => {
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::ConnectionMonitor) => {
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::Topology) => {
                self.active_surface = ActiveSurface::Terminal;
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
            Some(TabKind::RemoteDesktop) => {
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

    pub(in crate::workspace) fn focus_active_pane(&mut self, window: &mut Window, cx: &App) {
        self.clear_ai_sidebar_keyboard_focus();
        if self.terminal_command_bar_focused {
            // Focusing the pane must also release Workspace's synthetic command
            // input owner; otherwise root key capture can keep swallowing Tab
            // after the visual focus has returned to the terminal.
            self.terminal_command_bar_focused = false;
            self.terminal_command_suggestions_open = false;
            self.terminal_command_suggestion_highlighted = None;
            self.ime_marked_text = None;
        }
        if let Some(pane) = self.active_pane() {
            pane.read(cx).focus(window);
        } else {
            window.focus(&self.focus_handle);
        }
    }

    fn focus_active_tab_keyboard_owner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::RemoteDesktop)
        {
            // Remote desktop tabs are keyboard owners. Activating the tab must
            // release stale Workspace input fields even before the user clicks
            // inside the remote framebuffer.
            self.focus_remote_desktop_keyboard(window, cx);
        } else {
            self.focus_active_pane(window, cx);
        }
    }

    pub(super) fn register_ssh_terminal_session(
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
            .entry(node_id)
            .and_modify(|node| {
                attach_terminal_to_existing_ssh_node(
                    node,
                    saved_connection_id.clone(),
                    config.clone(),
                    session_id,
                );
            })
            .or_insert_with(|| WorkspaceSshNode {
                saved_connection_id,
                config,
                title,
                terminal_ids: vec![session_id],
                readiness: NodeReadiness::Connecting,
            });
    }

    pub(in crate::workspace) fn unregister_ssh_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
    ) {
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

    pub(in crate::workspace) fn focus_terminal_session(
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
        self.main_window_tabs.active_tab_id = Some(tab_id);
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

    pub(in crate::workspace) fn close_terminal_session(
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

    pub(in crate::workspace) fn request_disconnect_ssh_node(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
        let Some(node) = self.ssh_nodes.get(node_id) else {
            return;
        };
        let title = node.title.trim();
        let display_name = if title.is_empty() {
            format!("{}@{}", node.config.username, node.config.host)
        } else {
            title.to_string()
        };
        // Tauri opens the confirmation from the tree action entrypoint, while
        // disconnectNode itself remains the backend cleanup path.
        self.node_disconnect_confirm = Some(NodeDisconnectConfirm {
            node_id: node_id.clone(),
            display_name,
        });
        self.node_disconnect_confirm_presence.reopen();
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(in crate::workspace) fn cancel_node_disconnect_confirm(&mut self, cx: &mut Context<Self>) {
        if self.begin_node_disconnect_confirm_exit(cx) {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn confirm_node_disconnect_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(confirm) = self.node_disconnect_confirm.clone() else {
            return;
        };
        if self.begin_node_disconnect_confirm_exit(cx) {
            self.disconnect_ssh_node(&confirm.node_id, window, cx);
        }
    }

    pub(in crate::workspace) fn disconnect_ssh_node(
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
            self.forwarding_port_detection_by_node
                .remove(affected_node_id);
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

    pub(in crate::workspace) fn close_active_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.active_tab_index() else {
            return;
        };
        self.close_tab_at_index(index, window, cx);
    }

    pub(in crate::workspace) fn request_close_active_tab(
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
            self.main_window_tabs.close_confirm = Some(TabCloseConfirm::Single { tab_id });
            self.tab_close_confirm_presence.reopen();
            self.reset_standard_confirm_focus();
            cx.notify();
            return;
        }
        if self.tabs[index].kind == TabKind::LocalTerminal {
            self.request_local_terminal_close_check(
                LocalTerminalCloseCheck::Single { tab_id },
                window,
                cx,
            );
            return;
        }
        self.close_tab_at_index(index, window, cx);
    }

    pub(in crate::workspace) fn close_tab_by_id(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        self.close_tab_at_index(index, window, cx);
    }

    pub(in crate::workspace) fn request_close_other_tabs_or_active_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_tab_id) = self.main_window_tabs.active_tab_id else {
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
            self.main_window_tabs.close_confirm = Some(TabCloseConfirm::Other { tab_ids });
            self.tab_close_confirm_presence.reopen();
            self.reset_standard_confirm_focus();
            cx.notify();
            return;
        }
        self.request_local_terminal_close_check(
            LocalTerminalCloseCheck::Batch { tab_ids },
            window,
            cx,
        );
    }

    fn tab_close_ids_include_ssh_terminal(&self, tab_ids: &[TabId]) -> bool {
        tab_ids.iter().any(|tab_id| {
            self.tabs
                .iter()
                .any(|tab| tab.id == *tab_id && tab.kind == TabKind::SshTerminal)
        })
    }

    fn request_local_terminal_close_check(
        &mut self,
        request: LocalTerminalCloseCheck,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_ids = request.tab_ids();
        let mut seen_panes = HashSet::new();
        let probes = tab_ids
            .iter()
            .filter_map(|tab_id| {
                self.tabs
                    .iter()
                    .find(|tab| tab.id == *tab_id && tab.kind == TabKind::LocalTerminal)
            })
            .filter_map(|tab| tab.root_pane.as_ref())
            .flat_map(|root_pane| {
                let mut pane_ids = Vec::new();
                root_pane.collect_pane_ids(&mut pane_ids);
                pane_ids
            })
            .filter(|pane_id| seen_panes.insert(*pane_id))
            .filter_map(|pane_id| {
                let pane = self.panes.get(&pane_id)?.read(cx);
                Some((pane_id, pane.process_info_probe(), pane.process_info()))
            })
            .collect::<Vec<_>>();

        if probes.is_empty() {
            // Preserve immediate close behavior when the selected tabs do not own a live local
            // terminal pane; there is no process state that needs a background refresh.
            match request {
                LocalTerminalCloseCheck::Single { tab_id } => {
                    self.close_tab_by_id(tab_id, window, cx);
                }
                LocalTerminalCloseCheck::Batch { tab_ids } => {
                    for tab_id in tab_ids {
                        self.close_tab_by_id(tab_id, window, cx);
                    }
                }
            }
            return;
        }

        self.main_window_tabs.process_close_check_generation = self
            .main_window_tabs
            .process_close_check_generation
            .wrapping_add(1);
        let generation = self.main_window_tabs.process_close_check_generation;
        let window_handle = window.window_handle();
        let probe_task = cx.background_executor().spawn(async move {
            // Each probe owns its duplicated PTY descriptor, so no terminal mutex is held while
            // platform process and cwd commands run on the background executor.
            probes
                .into_iter()
                .map(|(pane_id, probe, cached)| {
                    let info = probe
                        .map(|probe| probe.collect_foreground_only())
                        .unwrap_or(cached);
                    (pane_id, info)
                })
                .collect::<Vec<_>>()
        });

        cx.spawn(async move |weak, cx| {
            let results = probe_task.await;
            let _ = cx.update_window(window_handle, |_root, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    if this.main_window_tabs.process_close_check_generation != generation {
                        return;
                    }
                    let has_foreground_child = results
                        .iter()
                        .any(|(_, info)| terminal_process_info_has_foreground_child_process(info));
                    for (pane_id, info) in results {
                        if let Some(pane) = this.panes.get(&pane_id) {
                            pane.update(cx, |pane, _cx| {
                                let _ = pane.apply_process_info(info);
                            });
                        }
                    }

                    match request {
                        LocalTerminalCloseCheck::Single { tab_id } => {
                            if has_foreground_child {
                                this.main_window_tabs.close_confirm =
                                    Some(TabCloseConfirm::LocalChildProcess { tab_id });
                                this.tab_close_confirm_presence.reopen();
                                this.reset_standard_confirm_focus();
                                cx.notify();
                            } else {
                                this.close_tab_by_id(tab_id, window, cx);
                            }
                        }
                        LocalTerminalCloseCheck::Batch { tab_ids } => {
                            if has_foreground_child {
                                this.main_window_tabs.close_confirm =
                                    Some(TabCloseConfirm::LocalChildProcessBatch { tab_ids });
                                this.tab_close_confirm_presence.reopen();
                                this.reset_standard_confirm_focus();
                                cx.notify();
                            } else {
                                for tab_id in tab_ids {
                                    this.close_tab_by_id(tab_id, window, cx);
                                }
                            }
                        }
                    }
                });
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn cancel_tab_close_confirm(&mut self, cx: &mut Context<Self>) {
        if self.begin_tab_close_confirm_exit(cx) {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn confirm_tab_close_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(confirm) = self.main_window_tabs.close_confirm.clone() else {
            return;
        };
        if !self.begin_tab_close_confirm_exit(cx) {
            return;
        }
        match confirm {
            TabCloseConfirm::Single { tab_id } => {
                self.close_tab_by_id(tab_id, window, cx);
            }
            TabCloseConfirm::LocalChildProcess { tab_id } => {
                self.close_tab_by_id(tab_id, window, cx);
            }
            TabCloseConfirm::Other { tab_ids } => {
                self.request_local_terminal_close_check(
                    LocalTerminalCloseCheck::Batch { tab_ids },
                    window,
                    cx,
                );
            }
            TabCloseConfirm::LocalChildProcessBatch { tab_ids } => {
                for tab_id in tab_ids {
                    self.close_tab_by_id(tab_id, window, cx);
                }
            }
        }
    }

    pub(in crate::workspace) fn focus_adjacent_pane(
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
        let Some(index) = pane_ids
            .iter()
            .position(|pane_id| *pane_id == active_pane_id)
        else {
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
        let old_active_tab_id = self.main_window_tabs.active_tab_id;
        let removed_was_active = self.tabs.get(index).map(|tab| tab.id) == old_active_tab_id;
        let exiting_visual = self.tab_exit_visual(index);
        let tab = self.tabs.remove(index);
        self.detached_tabs.remove(&tab.id);
        if self
            .main_window_tabs
            .context_menu
            .is_some_and(|menu| menu.tab_id == tab.id)
        {
            self.main_window_tabs.context_menu = None;
        }
        if tab.kind == TabKind::Graphics {
            self.shutdown_graphics_session();
        }
        if tab.kind == TabKind::RemoteDesktop {
            self.close_remote_desktop_tab(tab.id, window, cx);
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
            self.serial_terminal_configs.remove(&session_id);
            self.raw_tcp_terminal_configs.remove(&session_id);
            self.raw_udp_terminal_configs.remove(&session_id);
            self.unregister_ssh_terminal_session(session_id);
        }
        for pane_id in pane_ids {
            if let Some(pane) = self.remove_terminal_pane(&pane_id) {
                let _ = pane.update(cx, |pane, _cx| pane.shutdown());
            }
        }

        self.main_window_tabs.active_tab_id = if self.tabs.is_empty() {
            None
        } else if !removed_was_active
            && old_active_tab_id.is_some_and(|tab_id| {
                self.tabs
                    .iter()
                    .any(|tab| tab.id == tab_id && !self.detached_tabs.contains(&tab.id))
            })
        {
            old_active_tab_id
        } else {
            self.tabs
                .iter()
                .enumerate()
                .skip(index.min(self.tabs.len().saturating_sub(1)))
                .find(|(_, tab)| !self.detached_tabs.contains(&tab.id))
                .or_else(|| {
                    self.tabs
                        .iter()
                        .enumerate()
                        .take(index)
                        .rev()
                        .find(|(_, tab)| !self.detached_tabs.contains(&tab.id))
                })
                .map(|(_, tab)| tab.id)
        };
        self.sync_active_tab_surface();
        self.needs_active_pane_focus = self
            .active_tab()
            .is_some_and(|tab| matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal));
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window);
        if let Some(exiting_visual) = exiting_visual {
            self.begin_tab_visual_exit(exiting_visual, cx);
        }
        cx.notify();
    }

    pub(super) fn tab_exit_visual(&self, index: usize) -> Option<ExitingTabVisual> {
        let tab = self.tabs.get(index)?;
        if self.detached_tabs.contains(&tab.id) {
            return None;
        }
        let live_visual_index = self.tabs[..index]
            .iter()
            .filter(|candidate| !self.detached_tabs.contains(&candidate.id))
            .count();
        let mut occupied_indices = self
            .main_window_tabs
            .exiting_tabs
            .iter()
            .map(|exiting| exiting.visual_index)
            .collect::<Vec<_>>();
        occupied_indices.sort_unstable();
        let visual_index = tab_exit_visual_index(live_visual_index, &occupied_indices);
        Some(ExitingTabVisual {
            tab_id: tab.id,
            kind: tab.kind.clone(),
            title: self.tab_display_title(tab),
            width: self.tab_visual_width(tab),
            visual_index,
            was_active: Some(tab.id) == self.main_window_tabs.active_tab_id,
        })
    }

    pub(super) fn begin_tab_visual_exit(
        &mut self,
        exiting_visual: ExitingTabVisual,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        if delay.is_zero() {
            return;
        }
        let tab_id = exiting_visual.tab_id;
        self.main_window_tabs.exiting_tabs.push(exiting_visual);
        cx.spawn(async move |weak, cx| {
            Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                let Some(position) = this
                    .main_window_tabs
                    .exiting_tabs
                    .iter()
                    .position(|exiting| exiting.tab_id == tab_id)
                else {
                    return;
                };
                let removed_index = this.main_window_tabs.exiting_tabs[position].visual_index;
                this.main_window_tabs.exiting_tabs.remove(position);
                for exiting in &mut this.main_window_tabs.exiting_tabs {
                    if exiting.visual_index > removed_index {
                        exiting.visual_index -= 1;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn close_tabs_for_node(
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

    pub(in crate::workspace) fn next_tab(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let visible_tabs = self
            .tabs
            .iter()
            .filter(|tab| !self.detached_tabs.contains(&tab.id))
            .map(|tab| tab.id)
            .collect::<Vec<_>>();
        if visible_tabs.is_empty() {
            return;
        }
        let current = self
            .main_window_tabs
            .active_tab_id
            .and_then(|active| visible_tabs.iter().position(|tab_id| *tab_id == active))
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % visible_tabs.len()
        } else if current == 0 {
            visible_tabs.len() - 1
        } else {
            current - 1
        };
        self.main_window_tabs.active_tab_id = Some(visible_tabs[next]);
        self.sync_active_tab_surface();
        self.needs_active_pane_focus = self
            .active_tab()
            .is_some_and(|tab| matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal));
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window);
        cx.notify();
    }

    pub(in crate::workspace) fn go_to_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab_id) = self
            .tabs
            .iter()
            .filter(|tab| !self.detached_tabs.contains(&tab.id))
            .nth(index)
            .map(|tab| tab.id)
        {
            self.main_window_tabs.active_tab_id = Some(tab_id);
            self.sync_active_tab_surface();
            self.needs_active_pane_focus = self.active_tab().is_some_and(|tab| {
                matches!(tab.kind, TabKind::LocalTerminal | TabKind::SshTerminal)
            });
            self.focus_active_pane(window, cx);
            self.reveal_active_tab(window);
            cx.notify();
        }
    }

    fn tabbar_outer_width(&self, window: &Window) -> f32 {
        let window_width = f32::from(window.inner_window_bounds().get_bounds().size.width);
        let sidebar_width = if self.sidebar_collapsed {
            self.tokens.metrics.activity_bar_width
        } else {
            self.sidebar_width
        };
        let context_sidebar_width = if self.context_sidebar_visible() {
            self.ai.chat.sidebar_width
        } else {
            0.0
        };
        (window_width - sidebar_width - context_sidebar_width).max(0.0)
    }

    pub(in crate::workspace) fn tabbar_scroll_viewport_width(&self, window: &Window) -> f32 {
        let measured_width = f32::from(self.main_window_tabs.scroll_handle.bounds().size.width);
        if measured_width > 1.0 {
            return measured_width;
        }
        self.tabbar_outer_width(window)
    }

    pub(in crate::workspace) fn tabbar_left_x(&self) -> f32 {
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
                .filter(|tab| !self.detached_tabs.contains(&tab.id))
                .map(|tab| self.tab_visual_width(tab))
                .sum::<f32>()
    }

    pub(in crate::workspace) fn tabbar_max_scroll(&self, window: &Window) -> f32 {
        let measured_width = f32::from(self.main_window_tabs.scroll_handle.bounds().size.width);
        if measured_width > 1.0 {
            return f32::from(self.main_window_tabs.scroll_handle.max_offset().width);
        }
        (self.tabbar_content_width() - self.tabbar_scroll_viewport_width(window)).max(0.0)
    }

    fn clamp_tab_scroll(&mut self, window: &Window) {
        let scroll_x = self.tabbar_effective_scroll_x(window);
        self.set_tabbar_scroll_x(scroll_x, window);
    }

    fn tabbar_has_overflow(&self, window: &Window) -> bool {
        self.tabbar_max_scroll(window) > 1.0
    }

    pub(in crate::workspace) fn tabbar_effective_scroll_x(&self, window: &Window) -> f32 {
        if self.tabbar_has_overflow(window) {
            f32::from(-self.main_window_tabs.scroll_handle.offset().x)
                .clamp(0.0, self.tabbar_max_scroll(window))
        } else {
            0.0
        }
    }

    pub(in crate::workspace) fn set_tabbar_scroll_x(&mut self, scroll_x: f32, window: &Window) {
        let next = scroll_x.clamp(0.0, self.tabbar_max_scroll(window));
        self.main_window_tabs
            .scroll_handle
            .set_offset(Point::new(px(-next), px(0.0)));
    }

    pub(in crate::workspace) fn handle_tabbar_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let max_scroll = self.tabbar_max_scroll(window);
        if max_scroll <= 1.0 {
            let had_offset = self.main_window_tabs.scroll_handle.offset().x != px(0.0);
            self.set_tabbar_scroll_x(0.0, window);
            if had_offset {
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }

        let delta = event
            .delta
            .pixel_delta(px(self.tokens.metrics.tabbar_height));
        // Tauri TabBar intercepts vertical wheel movement and applies it to
        // scrollLeft. Keep ScrollHandle as the measured clamp, but make this
        // the only wheel adapter so GPUI's default listener cannot double-scroll.
        let scroll_delta = tabbar_tauri_wheel_scroll_delta(f32::from(delta.x), f32::from(delta.y));
        if scroll_delta == 0.0 {
            return;
        }

        let current_scroll_x = self.tabbar_effective_scroll_x(window);
        let next_scroll_x = tabbar_scroll_x_after_wheel(current_scroll_x, scroll_delta, max_scroll);
        if (next_scroll_x - current_scroll_x).abs() < 0.01 {
            cx.stop_propagation();
            return;
        }

        // Avoid calling set_tabbar_scroll_x here: max_scroll was already read
        // for this wheel event, and re-reading it on every trackpad frame causes
        // unnecessary work. The handle owns the measured clamp, so write the
        // matching negative GPUI offset directly.
        self.main_window_tabs
            .scroll_handle
            .set_offset(Point::new(px(-next_scroll_x), px(0.0)));
        cx.notify();
        cx.stop_propagation();
    }

    pub(in crate::workspace) fn reveal_active_tab(&mut self, window: &Window) {
        let Some(index) = self.active_tab_index() else {
            self.clamp_tab_scroll(window);
            return;
        };
        let tab_left = self.tokens.metrics.tabbar_leading_offset
            + self
                .tabs
                .iter()
                .take(index)
                .filter(|tab| !self.detached_tabs.contains(&tab.id))
                .map(|tab| self.tab_visual_width(tab))
                .sum::<f32>();
        let tab_right = tab_left + self.tab_visual_width(&self.tabs[index]);
        let viewport_width = self.tabbar_scroll_viewport_width(window);

        let current_scroll_x = self.tabbar_effective_scroll_x(window);
        let mut next_scroll_x = current_scroll_x;
        if tab_left < current_scroll_x {
            next_scroll_x = tab_left;
        } else if tab_right > current_scroll_x + viewport_width {
            next_scroll_x = tab_right - viewport_width;
        }
        self.set_tabbar_scroll_x(next_scroll_x, window);
    }

    pub(in crate::workspace) fn tab_display_title(&self, tab: &Tab) -> String {
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

    pub(super) fn tab_visual_width(&self, tab: &Tab) -> f32 {
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
        tab_widths.len()
    }

    pub(in crate::workspace) fn start_tab_drag_candidate(
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
        let start_y = f32::from(event.position.y);
        let tab_widths = self
            .tabs
            .iter()
            .filter(|tab| !self.detached_tabs.contains(&tab.id))
            .map(|tab| self.tab_visual_width(tab))
            .collect::<Vec<_>>();
        let Some(visible_index) = self
            .tabs
            .iter()
            .filter(|tab| !self.detached_tabs.contains(&tab.id))
            .position(|tab| tab.id == tab_id)
        else {
            return;
        };
        let drop_target_index = self.tab_drop_target_index_for_x(start_x, window, &tab_widths);
        self.main_window_tabs.drag = Some(TabDragState {
            tab_id,
            from_index: visible_index,
            start_x,
            start_y,
            current_x: start_x,
            current_y: start_y,
            tab_widths,
            active: false,
            mode: TabDragMode::Pending,
            drop_target_index,
        });
        cx.notify();
    }

    pub(in crate::workspace) fn update_tab_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut drag) = self.main_window_tabs.drag.clone() else {
            return;
        };
        let was_active = drag.active;
        let previous_mode = drag.mode.clone();
        let previous_drop_target_index = drag.drop_target_index;
        // Browser tab drags keep pointer capture after leaving the tab label;
        // the root mouse-up is responsible for finishing or cancelling.
        drag.current_x = f32::from(event.position.x);
        drag.current_y = f32::from(event.position.y);
        let delta_x = drag.current_x - drag.start_x;
        let delta_y = drag.current_y - drag.start_y;
        // Tauri uses a 10px pointer threshold for reorder. GPUI also needs the
        // browser strip axis check here so vertical drags do not become tab
        // reorders just because the root view is acting as pointer capture.
        if tab_drag_is_detach(delta_x, delta_y, self.tokens.metrics.tabbar_height) {
            drag.active = true;
            drag.mode = TabDragMode::Detach;
            drag.drop_target_index = drag.from_index;
        } else if tab_drag_is_horizontal_reorder(delta_x, delta_y) {
            drag.active = true;
            drag.mode = TabDragMode::Reorder;
            drag.drop_target_index =
                self.tab_drop_target_index_for_x(drag.current_x, window, &drag.tab_widths);
        } else {
            drag.active = false;
            drag.mode = TabDragMode::Pending;
            drag.drop_target_index = drag.from_index;
        }
        let changed = drag.active != was_active
            || drag.mode != previous_mode
            || drag.drop_target_index != previous_drop_target_index;
        self.main_window_tabs.drag = Some(drag);
        if changed {
            // The tab strip renders activation and drop-target changes, not raw
            // pointer coordinates. Avoid repainting every captured mouse move.
            cx.notify();
        }
    }

    pub(in crate::workspace) fn finish_tab_drag(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        let Some(drag) = self.main_window_tabs.drag.take() else {
            return;
        };
        match drag.mode {
            TabDragMode::Detach => {
                let handoff_origin = self.tab_detach_handoff_origin(&drag, window);
                self.detach_tab_to_window(drag.tab_id, handoff_origin, window, cx);
            }
            TabDragMode::Reorder if drag.active => {
                let target_visible_index =
                    tab_reorder_target_visible_index(drag.from_index, drag.drop_target_index);
                if self.move_tab_to_visible_index(drag.tab_id, target_visible_index) {
                    self.clamp_tab_scroll(window);
                    self.reveal_active_tab(window);
                    cx.notify();
                }
            }
            TabDragMode::Pending | TabDragMode::Reorder => {
                if self.tab_by_id(drag.tab_id).is_some() {
                    self.set_active_tab(drag.tab_id, window, cx);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn move_tab_to_visible_index(
        &mut self,
        tab_id: TabId,
        visible_index: usize,
    ) -> bool {
        let Some(source_index) = self.tab_index_by_id(tab_id) else {
            return false;
        };
        let current_visible_index = self
            .tabs
            .iter()
            .filter(|tab| !self.detached_tabs.contains(&tab.id))
            .position(|tab| tab.id == tab_id);
        if current_visible_index == Some(visible_index) {
            return false;
        }
        let visible_tab_ids = self
            .tabs
            .iter()
            .filter(|tab| tab.id != tab_id && !self.detached_tabs.contains(&tab.id))
            .map(|tab| tab.id)
            .collect::<Vec<_>>();
        let anchor_tab_id = visible_tab_ids.get(visible_index).copied();
        let trailing_tab_id = visible_tab_ids.last().copied();
        let moved_tab = self.tabs.remove(source_index);
        let insertion_index = anchor_tab_id
            .and_then(|anchor_id| self.tab_index_by_id(anchor_id))
            .or_else(|| {
                trailing_tab_id
                    .and_then(|trailing_id| self.tab_index_by_id(trailing_id))
                    .map(|index| index + 1)
            })
            .unwrap_or_else(|| source_index.min(self.tabs.len()))
            .min(self.tabs.len());
        let changed = insertion_index != source_index.min(self.tabs.len());
        self.tabs.insert(insertion_index, moved_tab);
        changed
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

    #[test]
    fn tab_drag_reorder_requires_horizontal_browser_axis() {
        assert!(!tab_drag_is_horizontal_reorder(9.0, 0.0));
        assert!(!tab_drag_is_horizontal_reorder(0.0, 18.0));
        assert!(!tab_drag_is_horizontal_reorder(12.0, 24.0));
        assert!(tab_drag_is_horizontal_reorder(12.0, 8.0));
        assert!(tab_drag_is_horizontal_reorder(-18.0, 4.0));
    }

    #[test]
    fn tab_drag_detach_requires_downward_browser_axis() {
        assert!(!tab_drag_is_detach(4.0, 10.0, 36.0));
        assert!(!tab_drag_is_detach(36.0, 30.0, 36.0));
        assert!(!tab_drag_is_detach(4.0, -36.0, 36.0));
        assert!(tab_drag_is_detach(4.0, 32.0, 36.0));
    }

    #[test]
    fn tab_reorder_converts_pre_removal_slots_to_final_visible_indices() {
        assert_eq!(tab_reorder_target_visible_index(0, 0), 0);
        assert_eq!(tab_reorder_target_visible_index(0, 2), 1);
        assert_eq!(tab_reorder_target_visible_index(0, 3), 2);
        assert_eq!(tab_reorder_target_visible_index(2, 0), 0);
        assert_eq!(tab_reorder_target_visible_index(1, 2), 1);
    }

    #[test]
    fn attaching_terminal_does_not_rename_existing_ssh_node() {
        let mut node = WorkspaceSshNode {
            saved_connection_id: Some("home".to_string()),
            config: SshConfig::default(),
            title: "Home Host".to_string(),
            terminal_ids: vec![TerminalSessionId(1)],
            readiness: NodeReadiness::Ready,
        };

        attach_terminal_to_existing_ssh_node(
            &mut node,
            Some("home".to_string()),
            SshConfig {
                host: "100.118.61.75".to_string(),
                ..SshConfig::default()
            },
            TerminalSessionId(2),
        );

        assert_eq!(node.title, "Home Host");
        assert_eq!(node.config.host, "100.118.61.75");
        assert_eq!(
            node.terminal_ids,
            vec![TerminalSessionId(1), TerminalSessionId(2)]
        );
        assert_eq!(node.readiness, NodeReadiness::Ready);
    }

    #[test]
    fn attaching_terminal_without_saved_id_keeps_existing_node_owner() {
        let mut node = WorkspaceSshNode {
            saved_connection_id: Some("prod".to_string()),
            config: SshConfig::default(),
            title: "Production".to_string(),
            terminal_ids: vec![TerminalSessionId(1)],
            readiness: NodeReadiness::Ready,
        };

        // A later terminal is a consumer of the existing node owner, not a new
        // privilege scope that can clear or replace that owner.
        attach_terminal_to_existing_ssh_node(
            &mut node,
            None,
            SshConfig::default(),
            TerminalSessionId(2),
        );

        assert_eq!(node.saved_connection_id.as_deref(), Some("prod"));
        assert_eq!(
            node.terminal_ids,
            vec![TerminalSessionId(1), TerminalSessionId(2)]
        );
    }

    #[test]
    fn tabbar_wheel_matches_tauri_delta_y_adapter() {
        assert_eq!(tabbar_tauri_wheel_scroll_delta(0.0, 24.0), 24.0);
        assert_eq!(tabbar_tauri_wheel_scroll_delta(18.0, 24.0), 24.0);
        assert_eq!(tabbar_tauri_wheel_scroll_delta(-18.0, 0.0), -18.0);
    }

    #[test]
    fn tabbar_wheel_delta_maps_to_gpui_negative_scroll_offset() {
        assert_eq!(tabbar_scroll_x_after_wheel(0.0, -24.0, 120.0), 24.0);
        assert_eq!(tabbar_scroll_x_after_wheel(0.0, 24.0, 120.0), 0.0);
        assert_eq!(tabbar_scroll_x_after_wheel(24.0, 24.0, 120.0), 0.0);
        assert_eq!(tabbar_scroll_x_after_wheel(110.0, -24.0, 120.0), 120.0);
        assert_eq!(tabbar_scroll_x_after_wheel(120.0, -24.0, 120.0), 120.0);
    }

    #[test]
    fn tabbar_scrollbar_is_hidden_without_horizontal_overflow() {
        assert!(calculate_tabbar_scrollbar_geometry(20.0, 600.0, 0.0, 0.0).is_none());
    }

    #[test]
    fn tabbar_scrollbar_thumb_tracks_horizontal_scroll() {
        let at_start = calculate_tabbar_scrollbar_geometry(20.0, 600.0, 600.0, 0.0)
            .expect("overflow should produce scrollbar geometry");
        let at_end = calculate_tabbar_scrollbar_geometry(20.0, 600.0, 600.0, 600.0)
            .expect("overflow should produce scrollbar geometry");

        assert_eq!(at_start.viewport_left, 20.0);
        assert_eq!(at_start.thumb_left, TABBAR_SCROLLBAR_HORIZONTAL_INSET);
        assert_eq!(at_start.thumb_width, TABBAR_SCROLLBAR_MAX_THUMB_WIDTH);
        assert_eq!(
            at_end.thumb_left,
            TABBAR_SCROLLBAR_HORIZONTAL_INSET + at_end.track_width - at_end.thumb_width
        );
    }

    #[test]
    fn tabbar_scrollbar_drag_maps_track_edges_to_scroll_edges() {
        let geometry = calculate_tabbar_scrollbar_geometry(20.0, 600.0, 600.0, 0.0)
            .expect("overflow should produce scrollbar geometry");
        let track_start = TABBAR_SCROLLBAR_HORIZONTAL_INSET;
        let track_end = track_start + geometry.track_width - geometry.thumb_width;

        assert_eq!(tabbar_scroll_x_for_thumb_left(track_start, geometry), 0.0);
        assert_eq!(
            tabbar_scroll_x_for_thumb_left(track_end, geometry),
            geometry.max_scroll
        );
    }

    #[test]
    fn tab_exit_visual_indices_preserve_parallel_batch_order() {
        assert_eq!(tab_exit_visual_index(1, &[]), 1);
        assert_eq!(tab_exit_visual_index(1, &[1]), 2);
        assert_eq!(tab_exit_visual_index(1, &[1, 2]), 3);
        assert_eq!(tab_exit_visual_index(0, &[2]), 0);
    }
}
