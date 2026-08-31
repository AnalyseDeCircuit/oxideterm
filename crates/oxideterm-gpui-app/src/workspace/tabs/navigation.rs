use super::*;
use crate::workspace::sftp::{SftpRemoteId, SftpSurfaceId};

fn is_terminal_tab_kind(kind: &TabKind) -> bool {
    // Terminal focus and display behavior is transport-neutral.
    matches!(
        kind,
        TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal
    )
}

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

fn focus_terminal_node_projection(
    node_id: &NodeId,
    active_node_id: &mut Option<NodeId>,
    expanded_node_ids: &mut HashSet<NodeId>,
) {
    // Focusing a terminal changes only navigation state. Transport readiness
    // remains exclusively driven by registry and NodeRouter events.
    *active_node_id = Some(node_id.clone());
    expanded_node_ids.insert(node_id.clone());
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_tab_host_event(
        &mut self,
        event: &WorkspaceTabHostEvent,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        match event {
            WorkspaceTabHostEvent::CloseProcessCheckReady => {
                cx.spawn(async move |weak, cx| {
                    let _ = cx.update_window(window_handle, |_root, window, cx| {
                        weak.update(cx, |workspace, cx| {
                            workspace.apply_tab_close_process_completion(window, cx);
                        })
                    });
                })
                .detach();
            }
            WorkspaceTabHostEvent::RecordingElapsedTick { pane_id } => {
                if self.active_pane_id(cx) == Some(*pane_id)
                    && self.active_terminal_recording_status(cx).state
                        == TerminalRecordingState::Recording
                {
                    cx.notify();
                } else {
                    self.sync_active_terminal_recording_elapsed_tick(cx);
                }
            }
            WorkspaceTabHostEvent::TerminalOutputUnread => cx.notify(),
            WorkspaceTabHostEvent::TerminalPaneDelivery {
                pane_id,
                session_id,
                window_handle,
                event,
            } => {
                self.handle_terminal_pane_delivery(
                    *pane_id,
                    *session_id,
                    *window_handle,
                    *event,
                    cx,
                );
            }
        }
    }

    pub(in crate::workspace) fn navigate_tab_history(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let outside_main_tabs = self.tab_host.read(cx).outside_main_tab_ids();
        let existing_tab_ids = self
            .tabs(cx)
            .iter()
            .filter(|tab| !outside_main_tabs.contains(&tab.id))
            .map(|tab| tab.id)
            .collect::<HashSet<_>>();
        let Some(tab_id) = self.tab_host.update(cx, |tab_host, _| {
            tab_host.navigate_history(forward, &existing_tab_ids)
        }) else {
            return;
        };

        self.set_main_window_active_tab(Some(tab_id), cx);
        self.sync_active_tab_surface(cx);
        self.needs_active_pane_focus = self
            .active_tab(cx)
            .is_some_and(|tab| is_terminal_tab_kind(&tab.kind));
        self.focus_active_tab_keyboard_owner(window, cx);
        self.reveal_active_tab(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn set_active_tab(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        if self
            .tabs(cx)
            .iter()
            .any(|tab| tab.id == tab_id && !self.tab_host.read(cx).is_outside_main_window(tab.id))
        {
            if self.active_tab_id(cx) != Some(tab_id)
                && let Some(previous_tab_id) = self.active_tab_id(cx)
            {
                // Remote desktops keep server-side input state. Release it when
                // the tab loses focus so modifiers or mouse buttons cannot stick
                // on the remote host while the user works elsewhere.
                self.release_remote_desktop_inputs_for_tab(previous_tab_id, cx);
            }
            self.set_main_window_active_tab(Some(tab_id), cx);
            self.resume_remote_desktop_frame_delivery(tab_id, cx);
            self.sync_active_tab_surface(cx);
            self.needs_active_pane_focus = self
                .active_tab(cx)
                .is_some_and(|tab| is_terminal_tab_kind(&tab.kind));
            self.focus_active_tab_keyboard_owner(window, cx);
            self.reveal_active_tab(window, cx);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn sync_active_tab_surface(&mut self, cx: &mut Context<Self>) {
        // Tauri keeps the SSH session tree independent from terminal tab focus,
        // but app-level utility tabs still light up their owning activity icon.
        // Keep terminal/SFTP/IDE ownership separate while syncing these sidebar
        // entry tabs so the selected icon frame follows the visible surface.
        match self.active_tab(cx).map(|tab| &tab.kind) {
            Some(TabKind::Settings) => {
                self.active_surface = ActiveSurface::Settings;
            }
            Some(TabKind::Forwards) => {
                self.active_surface = ActiveSurface::Terminal;
                if let Some(active_tab_id) = self.active_tab_id(cx)
                    && let Some(node_id) = self.forwarding.read(cx).node_for_tab(active_tab_id)
                {
                    self.active_ssh_node_id = Some(node_id.clone());
                    self.expanded_ssh_nodes.insert(node_id.clone());
                    self.start_port_profiler_for_node_without_notify(node_id, cx);
                }
            }
            Some(TabKind::Sftp) => {
                self.active_surface = ActiveSurface::Terminal;
                if let Some(active_tab_id) = self.active_tab_id(cx) {
                    if let Some(node_id) = self.sftp_tab_nodes.get(&active_tab_id).cloned() {
                        self.active_ssh_node_id = Some(node_id.clone());
                        self.expanded_ssh_nodes.insert(node_id.clone());
                        self.activate_sftp_view_for_node(active_tab_id, &node_id, cx);
                    } else if let Some(binding) =
                        self.standalone_sftp_tabs.get(&active_tab_id).cloned()
                    {
                        self.active_ssh_node_id = None;
                        let pair_mode = binding.secondary_endpoint_id.is_some();
                        self.sftp_view.update(cx, |sftp, cx| {
                            if let Some(secondary_endpoint_id) = binding.secondary_endpoint_id {
                                sftp.activate_pair_view(
                                    SftpSurfaceId::Tab(active_tab_id),
                                    SftpRemoteId::Standalone(binding.primary_endpoint_id),
                                    SftpRemoteId::Standalone(secondary_endpoint_id),
                                    None,
                                );
                            } else {
                                sftp.activate_view(
                                    SftpSurfaceId::Tab(active_tab_id),
                                    SftpRemoteId::Standalone(binding.primary_endpoint_id),
                                );
                            }
                            cx.notify();
                        });
                        if pair_mode {
                            self.request_sftp_pair_primary_load(cx);
                        }
                    }
                }
            }
            Some(TabKind::Ide) => {
                self.active_surface = ActiveSurface::Terminal;
                if let Some(active_tab_id) = self.active_tab_id(cx)
                    && let Some(node_id) = self.ide_workspace.read(cx).node_for_tab(active_tab_id)
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
            Some(TabKind::MoshTerminal) => {
                // Mosh tabs have no SSH node ownership and must not retain a stale host highlight.
                self.active_surface = ActiveSurface::Terminal;
                self.active_ssh_node_id = None;
            }
            _ => {
                self.active_surface = ActiveSurface::Terminal;
            }
        }
        if let Some(session_id) = self.active_terminal_session_id(cx)
            && let Some(node_id) = self
                .workspace_runtime
                .read(cx)
                .ssh_terminal_node_id(session_id)
        {
            self.active_ssh_node_id = Some(node_id.clone());
            self.expanded_ssh_nodes.insert(node_id.clone());
        }
        self.activate_embedded_sftp_sidebar_if_visible(cx);
    }

    pub(in crate::workspace) fn focus_active_pane(&mut self, window: &mut Window, cx: &mut App) {
        self.clear_ai_sidebar_keyboard_focus(cx);
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.set_compact_focused(false, cx);
        });
        if let Some(pane) = self.active_pane(cx) {
            // A hidden terminal can retain paint operations that reference atlas slots later
            // reused by another surface. Force one fresh frame when the pane becomes active so
            // its unchanged snapshot is reshaped without reconnecting or rebuilding the session.
            window.refresh();
            pane.update(cx, |pane, cx| pane.focus(window, cx));
        } else {
            window.focus(&self.focus_handle, cx);
        }
        self.sync_active_terminal_metadata_context(cx);
        self.sync_active_terminal_recording_elapsed_tick(cx);
        self.sync_active_privilege_prompt_inline_hint(cx);
    }

    fn focus_active_tab_keyboard_owner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .active_tab(cx)
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

    pub(super) fn register_existing_ssh_terminal_session(
        &mut self,
        node_id: &NodeId,
        session_id: TerminalSessionId,
        cx: &mut App,
    ) -> Result<()> {
        let saved_connection_id = self
            .ssh_nodes
            .get(node_id)
            .map(|node| node.saved_connection_id.clone())
            .ok_or_else(|| anyhow::anyhow!("SSH node {} not found", node_id.0))?;

        // Existing terminals register only their consumer identity. The node
        // and registry keep owning the authentication config and transport.
        let registered = self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.register_ssh_terminal_session(session_id, node_id.clone())
        });
        if !registered {
            return Err(anyhow::anyhow!("workspace runtime is shutting down"));
        }
        if let Some(node) = self.ssh_nodes.get_mut(node_id)
            && !node.terminal_ids.contains(&session_id)
        {
            node.terminal_ids.push(session_id);
        }
        self.expanded_ssh_nodes.insert(node_id.clone());
        self.active_ssh_node_id = Some(node_id.clone());
        if let Some(saved_connection_id) = saved_connection_id {
            self.saved_ssh_nodes
                .insert(saved_connection_id, node_id.clone());
        }
        Ok(())
    }

    pub(in crate::workspace) fn unregister_ssh_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        // This method is also the shared terminal-session close path for local
        // panes. Revoke only this session; NodeRouter remains the SSH owner.
        self.ai_runtime_context.update(cx, |runtime, _cx| {
            runtime.revoke_terminal_session(session_id);
        });
        let forwarding_registry = self.forwarding_service.registry().clone();
        let forwarding_runtime = self.forwarding_runtime.clone();
        let forwarding_session_id = session_id.0.to_string();
        self.forwarding_service
            .release_binding_for_session(&forwarding_session_id);
        forwarding_runtime.spawn(async move {
            let _ = forwarding_registry.remove(&forwarding_session_id).await;
        });

        let node_id = self.workspace_runtime.update(cx, |runtime, _cx| {
            runtime.unregister_ssh_terminal_session(session_id)
        });
        // Tauri terminal close only removes the terminal/session mapping.
        // Do not health-probe here: a closed shell channel is not evidence
        // that the node-owned SSH transport died, and probing on the last
        // terminal close can incorrectly drive the node into LinkDown.
        let mut projection_changed = false;
        for (projected_node_id, node) in &mut self.ssh_nodes {
            if node_id
                .as_ref()
                .is_some_and(|runtime_node_id| runtime_node_id != projected_node_id)
            {
                continue;
            }
            let terminal_count = node.terminal_ids.len();
            node.terminal_ids.retain(|id| *id != session_id);
            projection_changed |= node.terminal_ids.len() != terminal_count;
        }
        if projection_changed {
            self.persist_session_tree_snapshot();
        }
        if let Some(node_id) = node_id
            && self
                .workspace_runtime
                .read(cx)
                .ssh_terminal_session_ids_for_node(&node_id)
                .is_empty()
        {
            self.close_embedded_sftp_for_node(&node_id, cx);
        }
    }

    pub(in crate::workspace) fn focus_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(location) = self.tab_host.read(cx).terminal_location(session_id) else {
            return false;
        };
        if self
            .tab_host
            .read(cx)
            .is_outside_main_window(location.tab_id)
        {
            // A detached terminal already has a native window owner. Do not
            // mount the same terminal entity into the main window as well;
            // focus its existing owner so session-tree activation still works.
            return self.focus_detached_tab_window(location.tab_id, cx);
        }
        self.set_main_window_active_tab(Some(location.tab_id), cx);
        self.tab_host.update(cx, |tab_host, _| {
            tab_host.set_active_pane(Some(location.tab_id), location.pane_id);
        });
        if let Some(node_id) = self
            .workspace_runtime
            .read(cx)
            .ssh_terminal_node_id(session_id)
        {
            focus_terminal_node_projection(
                &node_id,
                &mut self.active_ssh_node_id,
                &mut self.expanded_ssh_nodes,
            );
        }
        self.sync_active_tab_surface(cx);
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window, cx);
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
            .active_tab(cx)
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node) = self.ssh_nodes.get(node_id) else {
            return;
        };
        if !self.ssh_close_confirmation_enabled() {
            self.disconnect_ssh_node(node_id, window, cx);
            return;
        }
        let title = node.title.trim();
        let display_name = if title.is_empty() {
            format!("{}@{}", node.endpoint.username, node.endpoint.host)
        } else {
            title.to_string()
        };
        // Keep the transient choice scoped to the confirmation that owns it.
        self.skip_future_ssh_close_confirmations = false;
        self.overlay.update(cx, |overlay, cx| {
            overlay.open_confirm(
                WorkspaceOverlayConfirmKind::NodeDisconnect {
                    node_id: node_id.clone(),
                    display_name: Arc::from(display_name),
                },
                cx,
            );
        });
    }

    pub(in crate::workspace) fn cancel_node_disconnect_confirm(&mut self, cx: &mut Context<Self>) {
        if self.begin_node_disconnect_confirm_exit(false, cx).0 {
            self.skip_future_ssh_close_confirmations = false;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn confirm_node_disconnect_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (started, effect) = self.begin_node_disconnect_confirm_exit(true, cx);
        if !started {
            return;
        }
        let skip_future_confirmations = self.skip_future_ssh_close_confirmations;
        self.skip_future_ssh_close_confirmations = false;
        if let Some(WorkspaceOverlayConfirmEffect::DisconnectNode { node_id }) = effect {
            if skip_future_confirmations {
                self.disable_future_ssh_close_confirmations(cx);
            }
            self.disconnect_ssh_node(&node_id, window, cx);
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

        let mut nodes_to_disconnect = self.node_router.subtree_postorder(node_id);
        if nodes_to_disconnect.is_empty() {
            nodes_to_disconnect.push(node_id.clone());
        }
        for affected_node_id in &nodes_to_disconnect {
            // Dropping the node-scoped slot releases the dedicated browsing
            // transport without affecting transfers that still own their lease.
            self.dedicated_sftp_connections
                .lock()
                .remove(affected_node_id);
            let _ = self.interrupt_sftp_transfers_by_node(
                affected_node_id,
                "Connection closed".to_string(),
                cx,
            );
            self.close_embedded_sftp_for_node(affected_node_id, cx);
        }
        for affected_node_id in &nodes_to_disconnect {
            self.forwarding.update(cx, |forwarding, _cx| {
                forwarding.untrack_port_profiler(affected_node_id);
            });
            let forwarding_registry = self.forwarding_service.registry().clone();
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
        }
        let disconnected_nodes = self.workspace_runtime.update(cx, |runtime, cx| {
            runtime.disconnect_node_runtime_subtree(node_id, cx)
        });
        for affected_node_id in disconnected_nodes {
            if let Some(node) = self.ssh_nodes.get_mut(&affected_node_id) {
                // Tauri disconnect_tree_node marks every affected subtree node
                // as Disconnected. Link-down propagation uses Error elsewhere;
                // explicit user disconnect should not look like a failure.
                node.readiness = NodeReadiness::Disconnected;
                node.terminal_ids.clear();
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
        let Some(index) = self.active_tab_index(cx) else {
            return;
        };
        self.close_tab_at_index(index, window, cx);
    }

    pub(in crate::workspace) fn request_close_active_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_id) = self.active_tab_id(cx) else {
            return;
        };
        self.request_close_tab_by_id(tab_id, window, cx);
    }

    /// Applies the same user-facing close checks to close buttons, shortcuts, and middle-clicks.
    pub(in crate::workspace) fn request_close_tab_by_id(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.tabs(cx).iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        if self.tabs(cx)[index].kind == TabKind::SshTerminal {
            if !self.ssh_close_confirmation_enabled() {
                self.close_tab_at_index(index, window, cx);
                return;
            }
            // Keep the transient choice scoped to the confirmation that owns it.
            self.skip_future_ssh_close_confirmations = false;
            self.tab_host.update(cx, |tab_host, cx| {
                tab_host.open_close_confirm(TabCloseConfirm::Single { tab_id }, cx);
            });
            cx.notify();
            return;
        }
        if self.tabs(cx)[index].kind == TabKind::LocalTerminal {
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
        let Some(index) = self.tabs(cx).iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        self.close_tab_at_index(index, window, cx);
    }

    pub(in crate::workspace) fn request_close_other_tabs_or_active_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_tab_id) = self.active_tab_id(cx) else {
            return;
        };
        if self
            .active_tab(cx)
            .is_some_and(|tab| is_terminal_tab_kind(&tab.kind))
        {
            if self
                .active_tab(cx)
                .and_then(|tab| tab.root_pane.as_ref())
                .is_some_and(|root| root.pane_count() > 1)
            {
                self.close_active_pane(window, cx);
            }
            return;
        }

        let tab_ids = self
            .tabs(cx)
            .iter()
            .filter(|tab| tab.id != active_tab_id)
            .map(|tab| tab.id)
            .collect::<Vec<_>>();
        if tab_ids.is_empty() {
            return;
        }
        if self.tab_close_ids_include_ssh_terminal(&tab_ids, cx) {
            if !self.ssh_close_confirmation_enabled() {
                self.request_local_terminal_close_check(
                    LocalTerminalCloseCheck::Batch { tab_ids },
                    window,
                    cx,
                );
                return;
            }
            // Keep the transient choice scoped to the confirmation that owns it.
            self.skip_future_ssh_close_confirmations = false;
            self.tab_host.update(cx, |tab_host, cx| {
                tab_host.open_close_confirm(TabCloseConfirm::Other { tab_ids }, cx);
            });
            cx.notify();
            return;
        }
        self.request_local_terminal_close_check(
            LocalTerminalCloseCheck::Batch { tab_ids },
            window,
            cx,
        );
    }

    fn tab_close_ids_include_ssh_terminal(&self, tab_ids: &[TabId], cx: &App) -> bool {
        tab_ids.iter().any(|tab_id| {
            self.tabs(cx)
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
        let probes = {
            let tab_host = self.tab_host.read(cx);
            tab_ids
                .iter()
                .filter_map(|tab_id| {
                    self.tabs(cx)
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
                    let pane = tab_host.panes().get(&pane_id)?.read(cx);
                    Some(TabCloseProcessProbe {
                        pane_id,
                        probe: pane.process_info_probe(),
                        cached: pane.process_info(),
                    })
                })
                .collect::<Vec<_>>()
        };

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

        self.tab_host.update(cx, |tab_host, cx| {
            tab_host.start_close_process_check(request, probes, cx);
        });
    }

    fn apply_tab_close_process_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(completion) = self
            .tab_host
            .update(cx, |tab_host, _| tab_host.take_close_process_completion())
        else {
            return;
        };
        for (pane_id, info) in completion.results {
            if let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() {
                pane.update(cx, |pane, _cx| {
                    let _ = pane.apply_process_info(info);
                });
            }
        }

        match completion.request {
            LocalTerminalCloseCheck::Single { tab_id } => {
                if completion.has_foreground_child {
                    self.tab_host.update(cx, |tab_host, cx| {
                        tab_host
                            .open_close_confirm(TabCloseConfirm::LocalChildProcess { tab_id }, cx);
                    });
                    cx.notify();
                } else {
                    self.close_tab_by_id(tab_id, window, cx);
                }
            }
            LocalTerminalCloseCheck::Batch { tab_ids } => {
                if completion.has_foreground_child {
                    self.tab_host.update(cx, |tab_host, cx| {
                        tab_host.open_close_confirm(
                            TabCloseConfirm::LocalChildProcessBatch { tab_ids },
                            cx,
                        );
                    });
                    cx.notify();
                } else {
                    for tab_id in tab_ids {
                        self.close_tab_by_id(tab_id, window, cx);
                    }
                }
            }
        }
    }

    pub(in crate::workspace) fn cancel_tab_close_confirm(&mut self, cx: &mut Context<Self>) {
        if self.begin_tab_close_confirm_exit(false, cx).0 {
            self.skip_future_ssh_close_confirmations = false;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn confirm_tab_close_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (started, effect) = self.begin_tab_close_confirm_exit(true, cx);
        if !started {
            return;
        }
        if let Some(confirm) = effect {
            if self.skip_future_ssh_close_confirmations
                && matches!(
                    &confirm,
                    TabCloseConfirm::Single { .. } | TabCloseConfirm::Other { .. }
                )
            {
                self.disable_future_ssh_close_confirmations(cx);
            }
            self.skip_future_ssh_close_confirmations = false;
            self.apply_tab_close_confirm_effect(confirm, window, cx);
        }
    }

    pub(in crate::workspace) fn toggle_skip_future_ssh_close_confirmations(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.skip_future_ssh_close_confirmations = !self.skip_future_ssh_close_confirmations;
        cx.notify();
    }

    fn ssh_close_confirmation_enabled(&self) -> bool {
        self.settings_store
            .settings()
            .terminal
            .confirm_before_closing_ssh
    }

    fn disable_future_ssh_close_confirmations(&mut self, cx: &mut Context<Self>) {
        if !self.ssh_close_confirmation_enabled() {
            return;
        }
        // Persist only after the user confirms the destructive action.
        self.settings_store
            .settings_mut()
            .terminal
            .confirm_before_closing_ssh = false;
        if self.settings_store.save().is_ok() {
            self.settings_workspace.update(cx, |settings, _cx| {
                settings.acknowledge_external_store_state()
            });
        }
    }

    fn apply_tab_close_confirm_effect(
        &mut self,
        confirm: TabCloseConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let Some(active_pane_id) = self.active_pane_id(cx) else {
            return;
        };
        let mut pane_ids = Vec::new();
        if let Some(root) = self.active_tab(cx).and_then(|tab| tab.root_pane.as_ref()) {
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
        self.tab_host.update(cx, |tab_host, _| {
            tab_host.set_active_pane(None, next_pane_id);
        });
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    fn close_tab_at_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let exiting_visual = self.tab_exit_visual(index, cx);
        let Some(transition) = self
            .tab_host
            .update(cx, |tab_host, _cx| tab_host.remove_tab_at(index))
        else {
            return;
        };
        self.finish_tab_removal(transition, exiting_visual, window, cx);
    }

    pub(super) fn finish_tab_removal(
        &mut self,
        transition: TabRemovalTransition,
        exiting_visual: Option<ExitingTabVisual>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let TabRemovalTransition {
            tab,
            mount_cleanup,
            previous_active_tab_id,
            next_active_tab_id,
        } = transition;
        // Final tab removal revokes focus authority before any deferred UI work
        // can observe a replacement tab with the same presentation kind.
        self.ai_runtime_context
            .update(cx, |runtime, _cx| runtime.revoke_app_surface(tab.id));
        self.apply_tab_mount_cleanup(mount_cleanup, Some(window), cx);
        self.sync_host_tools_lifecycle(false, cx);
        if self
            .main_window_tabs
            .context_menu
            .is_some_and(|menu| menu.tab_id == tab.id)
        {
            self.main_window_tabs.context_menu = None;
        }
        if let TabKind::Plugin { plugin_id, tab_id } = &tab.kind {
            self.plugin_entity.update(cx, |plugins, _cx| {
                plugins
                    .ui_state_mut()
                    .remove_surface(plugin_id, "tab", tab_id);
            });
        }
        if tab.kind == TabKind::Graphics {
            self.graphics.update(cx, |graphics, cx| {
                // Closing the graphics page stops only its WSL graphics session.
                graphics.shutdown_graphics_session(cx);
            });
        }
        if tab.kind == TabKind::RemoteDesktop {
            self.standalone_connections.release_surface(
                standalone_connections::StandaloneConnectionSurface::RemoteDesktop(tab.id),
            );
            self.close_remote_desktop_tab(tab.id, window, cx);
        }
        // Tauri keeps node SFTP alive when the SFTP tab is closed; the tab is
        // only a view over the node-owned ConnectionEntry session.
        self.sftp_tab_nodes.remove(&tab.id);
        if let Some(binding) = self.standalone_sftp_tabs.remove(&tab.id) {
            let endpoint_ids =
                std::iter::once(binding.primary_endpoint_id).chain(binding.secondary_endpoint_id);
            for endpoint_id in endpoint_ids {
                if self
                    .standalone_sftp_tabs
                    .values()
                    .any(|open_binding| open_binding.contains_endpoint(&endpoint_id))
                {
                    continue;
                }
                if let Some(runtime) = self.standalone_sftp_sessions.remove(&endpoint_id) {
                    // The tab releases only its consumer; active transfers retain separate leases.
                    self.ssh_registry
                        .release(&runtime.connection_id, &runtime.consumer);
                }
            }
        }
        self.ide_workspace.update(cx, |workspace, cx| {
            // The IDE owner records a real project close and releases only this
            // surface's node consumer; shared node users remain registered.
            workspace.close_surface(tab.id, ide::IdeSurfaceCloseReason::UserProjectClose, cx);
        });
        self.forwarding
            .update(cx, |forwarding, _cx| forwarding.unmap_tab(tab.id));
        let mut pane_ids = Vec::new();
        let mut session_ids = Vec::new();
        if let Some(root_pane) = &tab.root_pane {
            root_pane.collect_pane_ids(&mut pane_ids);
            root_pane.collect_session_ids(&mut session_ids);
        }
        for session_id in session_ids {
            self.standalone_connections.release_surface(
                standalone_connections::StandaloneConnectionSurface::Terminal(session_id),
            );
            self.release_public_mcp_terminal_for_closed_session(session_id, cx);
            self.serial_terminal_configs.remove(&session_id);
            self.telnet_terminal_profile_ids.remove(&session_id);
            self.terminal_saved_connection_refs.remove(&session_id);
            self.clear_terminal_trigger_session_overrides(session_id);
            self.unregister_ssh_terminal_session(session_id, cx);
        }
        for pane_id in pane_ids {
            if let Some(pane) = self.remove_terminal_pane(&pane_id, cx) {
                let _ = pane.update(cx, |pane, _cx| pane.shutdown());
            }
        }

        self.apply_main_window_active_tab_change(previous_active_tab_id, next_active_tab_id, cx);
        self.sync_active_tab_surface(cx);
        self.needs_active_pane_focus = self
            .active_tab(cx)
            .is_some_and(|tab| is_terminal_tab_kind(&tab.kind));
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window, cx);
        if let Some(exiting_visual) = exiting_visual {
            self.begin_tab_visual_exit(exiting_visual, cx);
        }
        cx.notify();
    }

    pub(super) fn tab_exit_visual(&self, index: usize, cx: &App) -> Option<ExitingTabVisual> {
        let tab = self.tabs(cx).get(index)?;
        let outside_main_tabs = self.tab_host.read(cx).outside_main_tab_ids();
        if outside_main_tabs.contains(&tab.id) {
            return None;
        }
        let live_visual_index = self.tabs(cx)[..index]
            .iter()
            .filter(|candidate| !outside_main_tabs.contains(&candidate.id))
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
            was_active: Some(tab.id) == self.active_tab_id(cx),
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
            .tabs(cx)
            .iter()
            .filter(|tab| self.tab_belongs_to_node(tab, node_id, cx))
            .map(|tab| tab.id)
            .collect::<Vec<_>>();
        for tab_id in tab_ids {
            self.close_tab_by_id(tab_id, window, cx);
        }
    }

    fn tab_belongs_to_node(&self, tab: &Tab, node_id: &NodeId, cx: &App) -> bool {
        if self.sftp_tab_nodes.get(&tab.id) == Some(node_id) {
            return true;
        }
        if self.ide_workspace.read(cx).node_for_tab(tab.id) == Some(node_id) {
            return true;
        }
        if self.forwarding.read(cx).tab_matches_node(tab.id, node_id) {
            return true;
        }
        let mut session_ids = Vec::new();
        if let Some(root_pane) = &tab.root_pane {
            root_pane.collect_session_ids(&mut session_ids);
        }
        session_ids.into_iter().any(|session_id| {
            self.workspace_runtime
                .read(cx)
                .ssh_terminal_session_belongs_to_node(session_id, node_id)
        })
    }

    pub(in crate::workspace) fn next_tab(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let outside_main_tabs = self.tab_host.read(cx).outside_main_tab_ids();
        let visible_tabs = self
            .tabs(cx)
            .iter()
            .filter(|tab| !outside_main_tabs.contains(&tab.id))
            .map(|tab| tab.id)
            .collect::<Vec<_>>();
        if visible_tabs.is_empty() {
            return;
        }
        let current = self
            .active_tab_id(cx)
            .and_then(|active| visible_tabs.iter().position(|tab_id| *tab_id == active))
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % visible_tabs.len()
        } else if current == 0 {
            visible_tabs.len() - 1
        } else {
            current - 1
        };
        self.set_main_window_active_tab(Some(visible_tabs[next]), cx);
        self.sync_active_tab_surface(cx);
        self.needs_active_pane_focus = self
            .active_tab(cx)
            .is_some_and(|tab| is_terminal_tab_kind(&tab.kind));
        self.focus_active_pane(window, cx);
        self.reveal_active_tab(window, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn go_to_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let outside_main_tabs = self.tab_host.read(cx).outside_main_tab_ids();
        if let Some(tab_id) = self
            .tabs(cx)
            .iter()
            .filter(|tab| !outside_main_tabs.contains(&tab.id))
            .nth(index)
            .map(|tab| tab.id)
        {
            self.set_main_window_active_tab(Some(tab_id), cx);
            self.sync_active_tab_surface(cx);
            self.needs_active_pane_focus = self
                .active_tab(cx)
                .is_some_and(|tab| is_terminal_tab_kind(&tab.kind));
            self.focus_active_pane(window, cx);
            self.reveal_active_tab(window, cx);
            cx.notify();
        }
    }

    fn tabbar_outer_width(&self, window: &Window, cx: &App) -> f32 {
        let window_width = f32::from(window.inner_window_bounds().get_bounds().size.width);
        let sidebar_width = if self.sidebar_collapsed {
            self.tokens.metrics.activity_bar_width
        } else {
            self.sidebar_width
        };
        let context_sidebar_width = if self.context_sidebar_visible() {
            self.ai_entity.read(cx).chat_ui().sidebar_width
        } else {
            0.0
        };
        (window_width - sidebar_width - context_sidebar_width).max(0.0)
    }

    pub(in crate::workspace) fn tabbar_scroll_viewport_width(
        &self,
        window: &Window,
        cx: &App,
    ) -> f32 {
        let measured_width = f32::from(self.main_window_tabs.scroll_handle.bounds().size.width);
        if measured_width > 1.0 {
            return measured_width;
        }
        self.tabbar_outer_width(window, cx)
    }

    pub(in crate::workspace) fn tabbar_left_x(&self) -> f32 {
        if self.sidebar_collapsed {
            self.tokens.metrics.activity_bar_width
        } else {
            self.sidebar_width
        }
    }

    fn tabbar_content_width(&self, cx: &App) -> f32 {
        let outside_main_tabs = self.tab_host.read(cx).outside_main_tab_ids();
        self.tokens.metrics.tabbar_leading_offset
            + self
                .tabs(cx)
                .iter()
                .filter(|tab| !outside_main_tabs.contains(&tab.id))
                .map(|tab| self.tab_visual_width(tab))
                .sum::<f32>()
    }

    pub(in crate::workspace) fn tabbar_max_scroll(&self, window: &Window, cx: &App) -> f32 {
        let measured_width = f32::from(self.main_window_tabs.scroll_handle.bounds().size.width);
        if measured_width > 1.0 {
            return f32::from(self.main_window_tabs.scroll_handle.max_offset().x);
        }
        (self.tabbar_content_width(cx) - self.tabbar_scroll_viewport_width(window, cx)).max(0.0)
    }

    fn clamp_tab_scroll(&mut self, window: &Window, cx: &App) {
        let scroll_x = self.tabbar_effective_scroll_x(window, cx);
        self.set_tabbar_scroll_x(scroll_x, window, cx);
    }

    fn tabbar_has_overflow(&self, window: &Window, cx: &App) -> bool {
        self.tabbar_max_scroll(window, cx) > 1.0
    }

    pub(in crate::workspace) fn tabbar_effective_scroll_x(&self, window: &Window, cx: &App) -> f32 {
        if self.tabbar_has_overflow(window, cx) {
            f32::from(-self.main_window_tabs.scroll_handle.offset().x)
                .clamp(0.0, self.tabbar_max_scroll(window, cx))
        } else {
            0.0
        }
    }

    pub(in crate::workspace) fn set_tabbar_scroll_x(
        &mut self,
        scroll_x: f32,
        window: &Window,
        cx: &App,
    ) {
        let next = scroll_x.clamp(0.0, self.tabbar_max_scroll(window, cx));
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
        let max_scroll = self.tabbar_max_scroll(window, cx);
        if max_scroll <= 1.0 {
            let had_offset = self.main_window_tabs.scroll_handle.offset().x != px(0.0);
            self.set_tabbar_scroll_x(0.0, window, cx);
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

        let current_scroll_x = self.tabbar_effective_scroll_x(window, cx);
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

    pub(in crate::workspace) fn reveal_active_tab(&mut self, window: &Window, cx: &App) {
        let Some(index) = self.active_tab_index(cx) else {
            self.clamp_tab_scroll(window, cx);
            return;
        };
        let outside_main_tabs = self.tab_host.read(cx).outside_main_tab_ids();
        let tab_left = self.tokens.metrics.tabbar_leading_offset
            + self
                .tabs(cx)
                .iter()
                .take(index)
                .filter(|tab| !outside_main_tabs.contains(&tab.id))
                .map(|tab| self.tab_visual_width(tab))
                .sum::<f32>();
        let tab_right = tab_left + self.tab_visual_width(&self.tabs(cx)[index]);
        let viewport_width = self.tabbar_scroll_viewport_width(window, cx);

        let current_scroll_x = self.tabbar_effective_scroll_x(window, cx);
        let mut next_scroll_x = current_scroll_x;
        if tab_left < current_scroll_x {
            next_scroll_x = tab_left;
        } else if tab_right > current_scroll_x + viewport_width {
            next_scroll_x = tab_right - viewport_width;
        }
        self.set_tabbar_scroll_x(next_scroll_x, window, cx);
    }

    pub(in crate::workspace) fn tab_display_title(&self, tab: &Tab) -> String {
        let title = match tab.title_source {
            TabTitleSource::Static => tab.title.clone(),
            TabTitleSource::I18nKey(key) => self.i18n.t(key),
        };
        if is_terminal_tab_kind(&tab.kind) {
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
        cx: &mut App,
    ) -> usize {
        if tab_widths.is_empty() {
            return 0;
        }
        let tabbar_x = client_x - self.tabbar_left_x() + self.tabbar_effective_scroll_x(window, cx)
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
        if index >= self.tabs(cx).len()
            || self.tabs(cx).get(index).is_none_or(|tab| tab.id != tab_id)
        {
            return;
        }
        let start_x = f32::from(event.position.x);
        let start_y = f32::from(event.position.y);
        let outside_main_tabs = self.tab_host.read(cx).outside_main_tab_ids();
        let tab_widths = self
            .tabs(cx)
            .iter()
            .filter(|tab| !outside_main_tabs.contains(&tab.id))
            .map(|tab| self.tab_visual_width(tab))
            .collect::<Vec<_>>();
        let Some(visible_index) = self
            .tabs(cx)
            .iter()
            .filter(|tab| !outside_main_tabs.contains(&tab.id))
            .position(|tab| tab.id == tab_id)
        else {
            return;
        };
        let drop_target_index = self.tab_drop_target_index_for_x(start_x, window, &tab_widths, cx);
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
        if event.pressed_button != Some(MouseButton::Left) {
            // Win32 can lose the matching mouse-up during a re-entrant native callback.
            // A buttonless move is authoritative and must release the logical tab capture.
            self.main_window_tabs.drag = None;
            cx.notify();
            return;
        }
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
                self.tab_drop_target_index_for_x(drag.current_x, window, &drag.tab_widths, cx);
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
                if self.move_tab_to_visible_index(drag.tab_id, target_visible_index, cx) {
                    self.clamp_tab_scroll(window, cx);
                    self.reveal_active_tab(window, cx);
                    cx.notify();
                }
            }
            TabDragMode::Pending | TabDragMode::Reorder => {
                if self.tab_by_id(drag.tab_id, cx).is_some() {
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
        cx: &mut App,
    ) -> bool {
        self.tab_host.update(cx, |tab_host, _| {
            tab_host.move_main_tab_to_visible_index(tab_id, visible_index)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_drag_axes_distinguish_reorder_and_detach() {
        assert!(!tab_drag_is_horizontal_reorder(9.0, 0.0));
        assert!(!tab_drag_is_horizontal_reorder(0.0, 18.0));
        assert!(!tab_drag_is_horizontal_reorder(12.0, 24.0));
        assert!(tab_drag_is_horizontal_reorder(12.0, 8.0));
        assert!(tab_drag_is_horizontal_reorder(-18.0, 4.0));
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
    fn focusing_terminal_does_not_mark_node_ready() {
        let node_id = NodeId("focus-only".to_string());
        let node = WorkspaceSshNode::new(
            None,
            &SshConfig::default(),
            "Focus only".to_string(),
            vec![TerminalSessionId(1)],
            NodeReadiness::Disconnected,
        );
        let mut active_node_id = None;
        let mut expanded_node_ids = HashSet::new();

        focus_terminal_node_projection(&node_id, &mut active_node_id, &mut expanded_node_ids);

        assert_eq!(node.readiness, NodeReadiness::Disconnected);
        assert_eq!(active_node_id, Some(node_id.clone()));
        assert!(expanded_node_ids.contains(&node_id));
    }

    #[test]
    fn tab_exit_visual_indices_preserve_parallel_batch_order() {
        assert_eq!(tab_exit_visual_index(1, &[]), 1);
        assert_eq!(tab_exit_visual_index(1, &[1]), 2);
        assert_eq!(tab_exit_visual_index(1, &[1, 2]), 3);
        assert_eq!(tab_exit_visual_index(0, &[2]), 0);
    }
}
