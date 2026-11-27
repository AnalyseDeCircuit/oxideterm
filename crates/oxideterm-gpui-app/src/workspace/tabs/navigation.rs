use super::*;
use crate::workspace::sftp::{SftpRemoteId, SftpSurfaceId};

fn is_terminal_tab_kind(kind: &TabKind) -> bool {
    // Terminal focus and display behavior is transport-neutral.
    matches!(
        kind,
        TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal | TabKind::Workspace
    )
}

pub(super) use oxideterm_gpui_shell::tabs::geometry::TAB_DRAG_THRESHOLD_PX;
use oxideterm_gpui_shell::tabs::geometry::tab_exit_visual_index;
#[cfg(test)]
use oxideterm_gpui_shell::tabs::geometry::{
    tab_drag_is_detach, tab_drag_is_horizontal_reorder, tab_reorder_target_visible_index,
    tabbar_scroll_x_after_wheel, tabbar_wheel_scroll_delta,
};
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
            WorkspaceTabHostEvent::TerminalBellTriggered => cx.notify(),
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
                    event.clone(),
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
        let existing_tab_ids = self
            .main_tabs(cx)
            .map(|(_, tab)| tab.id)
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
        if self.onboarding_disclaimer_locked() {
            return;
        }
        self.audit
            .update(cx, |audit, cx| audit.dismiss_transients(cx));
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        if self.tabs(cx).iter().any(|tab| {
            tab.id == tab_id
                && !self
                    .tab_host
                    .read(cx)
                    .is_outside_main_window(self.tab_host.read(cx).container_tab_id(tab.id))
        }) {
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
        if !self.tabs(cx).iter().any(|tab| {
            tab.kind == TabKind::NotificationCenter
                && self.tab_host.read(cx).surface_is_visible(tab.id)
        }) {
            self.audit.update(cx, |audit, cx| audit.hide_audit_page(cx));
        }
        // Tauri keeps the SSH session tree independent from terminal tab focus,
        // but app-level utility tabs still light up their owning activity icon.
        // Keep terminal/SFTP/IDE ownership separate while syncing these sidebar
        // entry tabs so the selected icon frame follows the visible surface.
        match self.active_content_tab(cx).map(|tab| &tab.kind) {
            Some(TabKind::Settings) => {
                // These surfaces own no SSH node; put the previous highlight
                // out instead of leaving it stale.
                self.active_ssh_node_id = None;
                self.active_surface = ActiveSurface::Settings;
            }
            Some(TabKind::Forwards) => {
                if let Some(id) = self.active_content_tab_id(cx) {
                    self.forwarding
                        .update(cx, |state, cx| state.select_page(id, cx));
                }
                self.active_surface = ActiveSurface::Terminal;
                if let Some(active_tab_id) = self.active_content_tab_id(cx)
                    && let Some(node_id) = self.forwarding.read(cx).node_for_tab(active_tab_id)
                {
                    self.active_ssh_node_id = Some(node_id.clone());
                    self.expanded_ssh_nodes.insert(node_id.clone());
                    self.start_port_profiler_for_node_without_notify(node_id, cx);
                }
            }
            Some(TabKind::Sftp) => {
                if let Some(id) = self.active_content_tab_id(cx) {
                    self.sftp_focused_surface = SftpSurfaceId::Tab(id);
                }
                self.active_surface = ActiveSurface::Terminal;
                if let Some(active_tab_id) = self.active_content_tab_id(cx) {
                    if let Some(node_id) = self.sftp_tab_nodes.get(&active_tab_id).cloned() {
                        self.active_ssh_node_id = Some(node_id.clone());
                        self.expanded_ssh_nodes.insert(node_id.clone());
                        let _scope = self.enter_sftp_surface(SftpSurfaceId::Tab(active_tab_id));
                        if self.sftp_view().read(cx).current_surface_id
                            != Some(SftpSurfaceId::Tab(active_tab_id))
                        {
                            self.activate_sftp_view_for_node(active_tab_id, &node_id, cx);
                        } else {
                            self.maybe_start_sftp_remote_load(cx);
                        }
                    } else if let Some(binding) =
                        self.standalone_sftp_tabs.get(&active_tab_id).cloned()
                    {
                        self.active_ssh_node_id = None;
                        let _scope = self.enter_sftp_surface(SftpSurfaceId::Tab(active_tab_id));
                        if self.sftp_view().read(cx).current_surface_id
                            == Some(SftpSurfaceId::Tab(active_tab_id))
                        {
                            self.maybe_start_sftp_remote_load(cx);
                            return;
                        }
                        let pair_mode = binding.secondary_endpoint_id.is_some();
                        self.sftp_view().update(cx, |sftp, cx| {
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
                if let Some(active_tab_id) = self.active_content_tab_id(cx)
                    && let Some(node_id) = self.ide_workspace.read(cx).node_for_tab(active_tab_id)
                {
                    self.active_ssh_node_id = Some(node_id.clone());
                    self.expanded_ssh_nodes.insert(node_id.clone());
                }
            }
            Some(TabKind::SessionManager) => {
                self.discovery.ensure_visible();
                self.active_ssh_node_id = None;
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::Runtime) => {
                self.active_ssh_node_id = None;
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::ConnectionPool) => {
                self.active_ssh_node_id = None;
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::Topology) => {
                self.active_ssh_node_id = None;
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::NotificationCenter) => {
                self.active_ssh_node_id = None;
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::PluginManager) => {
                self.active_ssh_node_id = None;
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::CloudSync) => {
                self.active_ssh_node_id = None;
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::RemoteDesktop) => {
                self.active_ssh_node_id = None;
                self.active_surface = ActiveSurface::Terminal;
            }
            Some(TabKind::MoshTerminal) => {
                // Mosh tabs have no SSH node ownership and must not retain a stale host highlight.
                self.active_surface = ActiveSurface::Terminal;
                self.active_ssh_node_id = None;
            }
            _ => {
                // Plain terminal tabs and any other surface re-derive node
                // ownership below; clear first so local terminals and unknown
                // surfaces put the previous highlight out.
                self.active_ssh_node_id = None;
                self.active_surface = ActiveSurface::Terminal;
            }
        }
        self.sync_active_ssh_node_from_terminal_session(cx);
        self.activate_embedded_sftp_sidebar_if_visible(cx);
    }

    /// Re-derives SSH node ownership for the active surface, putting the
    /// previous host highlight out when the surface owns no node. Tool tabs
    /// resolve their node through their page registries (mirroring
    /// `sync_active_tab_surface`); only terminal-bearing surfaces re-derive
    /// the node from the active terminal session, so a tool page without
    /// terminal panes never loses its host. Tab creation paths bypass
    /// `sync_active_tab_surface` and call this directly so the sidebar host
    /// row follows the newly active surface.
    pub(in crate::workspace) fn sync_active_ssh_node_from_terminal_session(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.active_ssh_node_id = match self.active_content_tab(cx).map(|tab| &tab.kind) {
            Some(TabKind::Sftp) => self
                .active_content_tab_id(cx)
                .and_then(|id| self.sftp_tab_nodes.get(&id).cloned()),
            Some(TabKind::Ide) => self
                .active_content_tab_id(cx)
                .and_then(|id| self.ide_workspace.read(cx).node_for_tab(id))
                .cloned(),
            Some(TabKind::Forwards) => self
                .active_content_tab_id(cx)
                .and_then(|id| self.forwarding.read(cx).node_for_tab(id)),
            Some(kind) if is_terminal_tab_kind(kind) => {
                self.active_terminal_session_id(cx).and_then(|session_id| {
                    self.workspace_runtime
                        .read(cx)
                        .ssh_terminal_node_id(session_id)
                })
            }
            _ => None,
        };
        if let Some(node_id) = self.active_ssh_node_id.as_ref() {
            self.expanded_ssh_nodes.insert(node_id.clone());
        }
    }

    pub(in crate::workspace) fn focus_active_pane(&mut self, window: &mut Window, cx: &mut App) {
        self.blur_terminal_quick_commands_input(cx);
        self.blur_host_tools_input(cx);
        if self.search.blur() {
            self.ime_marked_text = None;
            self.clear_ime_selection();
        }
        self.clear_ai_sidebar_keyboard_focus(cx);
        self.terminal_command_sender.update(cx, |sender, cx| {
            sender.set_compact_focused(false, cx);
        });
        if let Some(surface) = self.active_ide_surface(cx) {
            window.focus(&surface.read(cx).focus_handle(cx), cx);
        } else if let Some(pane) = self.active_pane(cx) {
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

    pub(super) fn focus_active_tab_keyboard_owner(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab_id) = self.active_remote_desktop_tab_id(cx) {
            // Remote desktop tabs are keyboard owners. Activating the tab must
            // release stale Workspace input fields even before the user clicks
            // inside the remote framebuffer.
            self.focus_remote_desktop_keyboard(tab_id, window, cx);
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
        if let Some(origin) = self.node_router.runtime_store().discovery_origin(node_id) {
            origin.finish_open_request();
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
        self.ai.authority.update(cx, |runtime, _cx| {
            runtime.revoke_terminal_session(session_id);
        });
        let forwarding_registry = self.forwarding_service.registry().clone();
        let forwarding_runtime = self.forwarding_runtime.clone();
        let forwarding_session_id = session_id.0.to_string();
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
        self.tab_host.update(cx, |host, _| {
            host.set_active_pane(Some(location.tab_id), location.pane_id);
        });
        if self
            .tab_host
            .read(cx)
            .is_outside_main_window(location.tab_id)
        {
            // A detached terminal already has a native window owner. Do not
            // mount the same terminal entity into the main window as well;
            // focus its existing owner so session-tree activation still works.
            self.focus_tab_terminal(location.tab_id, window, cx);
            cx.notify();
            return self.focus_detached_tab_window(location.tab_id, cx);
        }
        self.set_main_window_active_tab(Some(location.tab_id), cx);

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
        let Some(location) = self.tab_host.read(cx).terminal_location(session_id) else {
            return;
        };
        self.close_terminal_pane_in_tab(location.tab_id, location.pane_id, window, cx);
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
    ) -> Option<usize> {
        if !self.ssh_nodes.contains_key(node_id) {
            return None;
        }

        let mut nodes_to_disconnect = self.node_router.subtree_postorder(node_id);
        if nodes_to_disconnect.is_empty() {
            nodes_to_disconnect.push(node_id.clone());
        }
        for affected_node_id in &nodes_to_disconnect {
            self.cancel_discovery_open_for_node(affected_node_id);
            // A later connection must not reuse an in-flight operation's old browsing slot.
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
            self.forwarding.update(cx, |forwarding, cx| {
                forwarding.untrack_port_profiler(affected_node_id, cx);
            });
            let forwarding_registry = self.forwarding_service.registry().clone();
            let forwarding_runtime = self.forwarding_runtime.clone();
            let forwarding_session_id = self.forwarding_session_id_for_node(affected_node_id);
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
        let invalidated_handles = self
            .public_mcp
            .invalidate_disconnected_nodes(&disconnected_nodes);
        for affected_node_id in disconnected_nodes {
            self.note_closed_ssh_session(&affected_node_id);
            if let Some(node) = self.ssh_nodes.get_mut(&affected_node_id) {
                // Tauri disconnect_tree_node marks every affected subtree node
                // as Disconnected. Link-down propagation uses Error elsewhere;
                // explicit user disconnect should not look like a failure.
                node.readiness = NodeReadiness::Disconnected;
                node.terminal_ids.clear();
            }
        }
        let snapshot_requested = self.sync_session_retention(cx);
        if !snapshot_requested {
            self.persist_session_tree_snapshot();
        }
        cx.notify();
        Some(invalidated_handles)
    }

    pub(in crate::workspace) fn request_close_active_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab_shortcut(
            oxideterm_gpui_shell::shortcuts::TabShortcut::Close,
            window,
            cx,
        );
    }
    pub(in crate::workspace) fn tab_shortcut(
        &mut self,
        shortcut: oxideterm_gpui_shell::shortcuts::TabShortcut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.onboarding_disclaimer_locked() {
            return;
        }
        let host = self.tab_host.read(cx);
        let container = self
            .keyboard_content_tab(cx)
            .map(|tab| host.container_tab_id(tab.id))
            .or(host.active_tab_id());
        if let Some(request) = shortcut.request(host, container) {
            self.handle_tab_request(request, window, cx);
        }
    }

    /// Applies the same user-facing close checks to close buttons, shortcuts, and middle-clicks.
    pub(in crate::workspace) fn request_close_tab_by_id(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.onboarding_disclaimer_locked() {
            return;
        }
        let Some(index) = self.tabs(cx).iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        if self.tabs(cx)[index].kind == TabKind::Onboarding {
            self.complete_onboarding(cx);
            return;
        }
        if self.terminal_tab_has_kind(tab_id, oxideterm_terminal::TerminalSessionKind::SshPty, cx) {
            if !self.ssh_close_confirmation_enabled() {
                self.request_local_terminal_close_check(
                    LocalTerminalCloseCheck::Single { tab_id },
                    window,
                    cx,
                );
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
        if self.terminal_tab_has_kind(
            tab_id,
            oxideterm_terminal::TerminalSessionKind::LocalPty,
            cx,
        ) {
            self.request_local_terminal_close_check(
                LocalTerminalCloseCheck::Single { tab_id },
                window,
                cx,
            );
            return;
        }
        if self.guard_dirty_knowledge_tab_close(tab_id, window, cx) {
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
        if self.onboarding_disclaimer_locked() {
            return;
        }
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

        let tab_ids = self.tab_host.read(cx).other_main_tab_ids(active_tab_id);
        self.request_close_tab_batch(tab_ids, window, cx);
    }
    pub(in crate::workspace) fn request_close_tab_batch(
        &mut self,
        tab_ids: Vec<TabId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if tab_ids.is_empty() || self.onboarding_disclaimer_locked() {
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
            self.terminal_tab_has_kind(*tab_id, oxideterm_terminal::TerminalSessionKind::SshPty, cx)
        })
    }

    pub(in crate::workspace) fn request_close_terminal_pane(
        &mut self,
        tab_id: TabId,
        pane_id: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .ide_workspace
            .read(cx)
            .shell_surface_for_pane(pane_id)
            .is_some()
        {
            self.close_terminal_pane_in_tab(tab_id, pane_id, window, cx);
            return;
        }
        match self.terminal_kind_for_pane(pane_id, cx) {
            Some(oxideterm_terminal::TerminalSessionKind::SshPty)
                if self.ssh_close_confirmation_enabled() =>
            {
                self.skip_future_ssh_close_confirmations = false;
                self.tab_host.update(cx, |host, cx| {
                    host.open_close_confirm(TabCloseConfirm::SshPane { tab_id, pane_id }, cx)
                });
                cx.notify()
            }
            Some(oxideterm_terminal::TerminalSessionKind::LocalPty) => self
                .request_local_terminal_close_check(
                    LocalTerminalCloseCheck::Pane { tab_id, pane_id },
                    window,
                    cx,
                ),
            _ => self.close_terminal_pane_in_tab(tab_id, pane_id, window, cx),
        }
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
                .filter_map(|tab_id| self.tabs(cx).iter().find(|tab| tab.id == *tab_id))
                .filter_map(|tab| tab.root_pane.as_ref())
                .flat_map(|root_pane| {
                    let mut pane_ids = Vec::new();
                    root_pane.collect_pane_ids(&mut pane_ids);
                    pane_ids
                })
                .filter(|pane| !matches!(request, LocalTerminalCloseCheck::Pane {pane_id,..} if pane_id != *pane) && seen_panes.insert(*pane))
                .filter_map(|pane_id| {
                    let pane = tab_host.panes().get(&pane_id)?.read(cx);
                    if pane.session_kind() != oxideterm_terminal::TerminalSessionKind::LocalPty {
                        return None;
                    }
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
                LocalTerminalCloseCheck::Pane { tab_id, pane_id } => {
                    self.close_terminal_pane_in_tab(tab_id, pane_id, window, cx)
                }
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
            LocalTerminalCloseCheck::Pane { tab_id, pane_id } => {
                if completion.has_foreground_child {
                    self.tab_host.update(cx, |host, cx| {
                        host.open_close_confirm(
                            TabCloseConfirm::LocalChildPane { tab_id, pane_id },
                            cx,
                        )
                    });
                    cx.notify()
                } else {
                    self.close_terminal_pane_in_tab(tab_id, pane_id, window, cx)
                }
            }
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
                    TabCloseConfirm::Single { .. }
                        | TabCloseConfirm::SshPane { .. }
                        | TabCloseConfirm::Other { .. }
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
            TabCloseConfirm::SshPane { tab_id, pane_id }
            | TabCloseConfirm::LocalChildPane { tab_id, pane_id } => {
                self.close_terminal_pane_in_tab(tab_id, pane_id, window, cx)
            }
            TabCloseConfirm::ApplicationQuit => {
                self.quit_after_confirmation(cx);
            }
            TabCloseConfirm::Single { tab_id } => {
                self.request_local_terminal_close_check(
                    LocalTerminalCloseCheck::Single { tab_id },
                    window,
                    cx,
                );
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
        self.blur_text_inputs(cx);
        self.tab_host.update(cx, |tab_host, _| {
            tab_host.set_active_pane(None, next_pane_id);
        });
        self.sync_active_tab_surface(cx);
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    fn close_tab_at_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs(cx).get(index)
            && tab.kind == TabKind::Onboarding
        {
            if self.onboarding_disclaimer_locked() {
                return;
            }
            if self.onboarding.open {
                self.complete_onboarding(cx);
                return;
            }
        }
        if let Some(tab_id) = self.tabs(cx).get(index).map(|tab| tab.id)
            && self.guard_shell_ide_close(tab_id, cx)
        {
            return;
        }
        if let Some(tab) = self.tabs(cx).get(index)
            && self.guard_dirty_knowledge_tab_close(tab.id, window, cx)
        {
            return;
        }
        if self.settings_store.settings().terminal.crt_effect
            && let Some(tab) = self.tabs(cx).get(index)
            && Some(tab.id) == self.active_tab_id(cx)
            && let Some(root_pane) = tab.root_pane.as_ref()
        {
            let tab_id = tab.id;
            let mut pane_ids = Vec::new();
            root_pane.collect_pane_ids(&mut pane_ids);
            let panes = {
                let tab_host = self.tab_host.read(cx);
                pane_ids
                    .into_iter()
                    .filter_map(|pane_id| tab_host.panes().get(&pane_id).cloned())
                    .collect::<Vec<_>>()
            };
            let mut started_any = false;
            for pane in panes {
                if pane.update(cx, |pane, cx| pane.start_crt_power_off(cx)) {
                    started_any = true;
                }
            }
            if started_any {
                // Let the 260ms CRT beam-collapse animation play before detaching the tab.
                // A second close action skips this branch because collapse is already running.
                cx.spawn_in(window, async move |workspace, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(260))
                        .await;
                    let _ = workspace.update_in(cx, |this, window, cx| {
                        this.close_tab_by_id(tab_id, window, cx);
                    });
                })
                .detach();
                return;
            }
        }
        let exiting_visual = self.tab_exit_visual(index, window, cx);
        let Some(transition) = self
            .tab_host
            .update(cx, |tab_host, _cx| tab_host.remove_tab_at(index))
        else {
            return;
        };
        self.finish_tab_removal(transition, exiting_visual, Some(window), cx);
    }

    pub(in crate::workspace) fn finish_tab_removal(
        &mut self,
        transition: TabRemovalTransition,
        exiting_visual: Option<ExitingTabVisual>,
        mut window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let TabRemovalTransition {
            tab,
            mount_cleanup,
            previous_active_tab_id,
            next_active_tab_id,
        } = transition;
        self.close_plugin_connection_tab(tab.id, cx);
        if tab.kind == TabKind::NotificationCenter {
            self.audit.update(cx, |audit, cx| audit.hide_audit_page(cx));
        }
        if tab.kind == TabKind::SessionManager && self.rails.section == SidebarSection::Connections
        {
            if self.session_manager_handoff_to_sidebar {
                // The tabbar's "show in sidebar" handoff closes this tab while
                // the sidebar form takes over, so keep the panel in place.
                self.session_manager_handoff_to_sidebar = false;
            } else {
                // Closing the tab form ends the Session Manager entirely; hand the
                // sidebar back to its previous panel instead of revealing the
                // sidebar form.
                self.set_sidebar_section(SidebarSection::Sessions, cx);
            }
        }
        let mut pages = Vec::new();
        if let Some(root) = &tab.root_pane {
            root.collect_page_ids(&mut pages);
        }
        for page in pages {
            let transition = self.tab_host.update(cx, |host, _| {
                host.tab_index_by_id(page)
                    .and_then(|index| host.remove_tab_at(index))
            });
            if let Some(transition) = transition {
                self.finish_tab_removal(transition, None, window.as_deref_mut(), cx);
            }
        }
        // Final tab removal revokes focus authority before any deferred UI work
        // can observe a replacement tab with the same presentation kind.
        self.ai
            .authority
            .update(cx, |runtime, _cx| runtime.revoke_app_surface(tab.id));
        self.apply_tab_mount_cleanup(mount_cleanup, window.as_deref_mut(), cx);
        self.sync_host_tools_lifecycle(false, cx);
        if self
            .tab_context_menu
            .is_some_and(|menu| menu.tab_id == tab.id)
        {
            self.tab_context_menu = None;
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
            self.close_remote_desktop_tab(tab.id, window.as_deref_mut(), cx);
        }
        // Tauri keeps node SFTP alive when the SFTP tab is closed; the tab is
        // only a view over the node-owned ConnectionEntry session.
        self.close_sftp_page(tab.id);
        self.sftp_surface_leases
            .remove(&sftp::SftpSurfaceId::Tab(tab.id));
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
                self.ftp_sessions.remove(&endpoint_id);
            }
        }
        self.ide_workspace.update(cx, |workspace, cx| {
            // The IDE owner records a real project close and releases only this
            // surface's node consumer; shared node users remain registered.
            workspace.close_surface(tab.id, ide::IdeSurfaceCloseReason::UserProjectClose, cx);
        });
        self.knowledge_state()
            .update(cx, |workspace, _cx| workspace.close_tab(tab.id));
        if let Some(node) = self
            .forwarding
            .update(cx, |forwarding, cx| forwarding.unmap_tab(tab.id, cx))
            && let Some(connection_id) = self.forwarding_connection_id_for_node(&node)
        {
            self.stop_hidden_port_profiler(&connection_id, cx);
        }
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
            self.close_terminal_pane(&pane_id, cx);
        }

        self.sync_collapsed_workspace_tabs(cx);
        self.sync_session_retention(cx);
        self.apply_main_window_active_tab_change(previous_active_tab_id, next_active_tab_id, cx);
        self.sync_active_tab_surface(cx);
        self.needs_active_pane_focus = self
            .active_tab(cx)
            .is_some_and(|tab| is_terminal_tab_kind(&tab.kind));
        // A closed shell has no window to focus. The surviving main window
        // observes the session change and consumes needs_active_pane_focus.
        if let Some(window) = window.as_deref_mut() {
            self.focus_active_pane(window, cx);
            self.reveal_active_tab(window, cx);
        }
        if let Some(exiting_visual) = exiting_visual {
            self.begin_tab_visual_exit(exiting_visual, cx);
        }
        let empty = self
            .tabs(cx)
            .iter()
            .filter(|tab| tab.kind == TabKind::Workspace && tab.root_pane.is_none())
            .map(|tab| tab.id)
            .collect::<Vec<_>>();
        for id in empty {
            let transition = self.tab_host.update(cx, |host, _| {
                host.tab_index_by_id(id)
                    .and_then(|index| host.remove_tab_at(index))
            });
            if let Some(transition) = transition {
                self.finish_tab_removal(transition, None, window.as_deref_mut(), cx);
            }
        }
        cx.notify();
    }

    pub(super) fn tab_exit_visual(
        &self,
        index: usize,
        window: &Window,
        cx: &App,
    ) -> Option<ExitingTabVisual> {
        let tab = self.tabs(cx).get(index)?;
        let live_visual_index = self
            .main_tabs(cx)
            .position(|(_, candidate)| candidate.id == tab.id)?;
        let mut occupied_indices = self
            .main_window_tabs
            .read(cx)
            .state
            .exiting_tabs
            .iter()
            .map(|exiting| exiting.visual_index)
            .collect::<Vec<_>>();
        occupied_indices.sort_unstable();
        let visual_index = tab_exit_visual_index(live_visual_index, &occupied_indices);
        let bounds = self.tab_strip_layout(window, cx).bounds(tab.id)?;
        Some(ExitingTabVisual {
            tab_id: tab.id,
            kind: tab.kind.clone(),
            title: self.tab_display_title(tab),
            width: bounds.end - bounds.start,
            visual_index,
            was_active: Some(tab.id) == self.active_tab_id(cx),
            started_at: cx.background_executor().now(),
            duration: oxideterm_gpui_ui::motion::exit_duration(
                &self.tokens,
                oxideterm_gpui_ui::motion::MotionDuration::Control,
            ),
        })
    }

    pub(super) fn begin_tab_visual_exit(
        &mut self,
        exiting_visual: ExitingTabVisual,
        cx: &mut Context<Self>,
    ) {
        if self.tokens.motion.enabled && !exiting_visual.duration.is_zero() {
            self.main_window_tabs
                .update(cx, |strip, cx| strip.begin_exit(exiting_visual, cx));
        }
    }
    pub(super) fn close_tabs_for_node(
        &mut self,
        node_id: &NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let targets = self
            .tabs(cx)
            .iter()
            .filter(|tab| self.tab_belongs_to_node(tab, node_id, cx))
            .map(|tab| {
                let panes = tab.root_pane.as_ref().map(|root| {
                    let mut ids = Vec::new();
                    root.collect_pane_ids(&mut ids);
                    ids.into_iter()
                        .filter(|pane| {
                            root.session_id_for_pane(*pane).is_some_and(|session| {
                                self.workspace_runtime
                                    .read(cx)
                                    .ssh_terminal_session_belongs_to_node(session, node_id)
                            })
                        })
                        .collect::<Vec<_>>()
                });
                (tab.id, panes)
            })
            .collect::<Vec<_>>();
        for (tab_id, panes) in targets {
            if let Some(panes) = panes {
                for pane in panes {
                    self.close_terminal_pane_in_tab(tab_id, pane, window, cx);
                }
            } else {
                self.close_tab_by_id(tab_id, window, cx);
            }
        }
    }

    fn tab_belongs_to_node(&self, tab: &Tab, node_id: &NodeId, cx: &App) -> bool {
        self.tab_affinities(tab.id, cx)
            .contains(&TabAffinity::Ssh(node_id.clone()))
    }

    pub(in crate::workspace) fn next_tab(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab_shortcut(
            oxideterm_gpui_shell::shortcuts::TabShortcut::Cycle(forward),
            window,
            cx,
        );
    }

    pub(in crate::workspace) fn go_to_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab_shortcut(
            oxideterm_gpui_shell::shortcuts::TabShortcut::Index(index),
            window,
            cx,
        );
    }

    pub(super) fn zen_exit_tabbar_width(&self) -> f32 {
        let settings = self.settings_store.settings();
        if settings.sidebar_ui.zen_mode && settings.appearance.zen_mode_keep_tabbar {
            self.tokens.metrics.tabbar_height
        } else {
            0.0
        }
    }

    fn tabbar_outer_width(&self, window: &Window) -> f32 {
        let window_width = f32::from(window.inner_window_bounds().get_bounds().size.width);
        let context_sidebar_width = self.context_sidebar_occupied_width();
        (window_width
            - self.tabbar_left_x(window)
            - context_sidebar_width
            - self.zen_exit_tabbar_width()
            - if self.onboarding_disclaimer_locked() {
                0.
            } else {
                self.tokens.metrics.tabbar_height
            })
        .max(0.0)
    }

    pub(in crate::workspace) fn tabbar_scroll_viewport_width(
        &self,
        window: &Window,
        cx: &App,
    ) -> f32 {
        let measured_width = f32::from(
            self.main_window_tabs
                .read(cx)
                .state
                .scroll_handle
                .bounds()
                .size
                .width,
        );
        if measured_width > 1.0 {
            return measured_width;
        }
        self.tabbar_outer_width(window)
    }

    pub(in crate::workspace) fn tabbar_left_x(&self, window: &Window) -> f32 {
        if self.settings_store.settings().sidebar_ui.zen_mode {
            // AppKit keeps its native window buttons above the first row in windowed mode.
            if cfg!(target_os = "macos") && !window.is_fullscreen() {
                self.tokens.metrics.titlebar_label_x()
            } else {
                0.0
            }
        } else if self.rails.left.collapsed() {
            self.tokens.metrics.activity_bar_width
        } else {
            self.rails.left.width()
        }
    }

    pub(super) fn tabbar_effective_scroll_x(&self, window: &Window, cx: &App) -> f32 {
        self.main_window_tabs.read(cx).state.offset(
            (self.tab_strip_layout(window, cx).content_width()
                - self.tabbar_scroll_viewport_width(window, cx))
            .max(0.),
        )
    }
    pub(in crate::workspace) fn reveal_active_tab(&mut self, window: &Window, cx: &App) {
        self.main_window_tabs.read(cx).state.reveal(
            &self.tab_strip_layout(window, cx),
            self.active_tab_id(cx),
            self.tabbar_scroll_viewport_width(window, cx),
        );
    }
    pub(in crate::workspace) fn tab_display_title(&self, tab: &Tab) -> String {
        let title = match tab.title_source {
            TabTitleSource::Static => tab.title.clone(),
            TabTitleSource::I18nKey(key) => self.i18n.t(key),
        }
        .replace(['\r', '\n'], " ");
        if is_terminal_tab_kind(&tab.kind) {
            let pane_count = tab.root_pane.as_ref().map_or(1, PaneNode::pane_count);
            if pane_count > 1 {
                return format!("{title} ({pane_count})");
            }
        }
        title
    }

    pub(in crate::workspace) fn handle_tab_request(
        &mut self,
        request: ShellTabRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            ShellTabRequest::CancelDrag(id) => {
                self.split_drop_target = None;
                if self.tab_by_id(id, cx).is_some()
                    && !self.tab_host.read(cx).is_outside_main_window(id)
                {
                    self.set_main_window_active_tab(Some(id), cx);
                    self.sync_active_tab_surface(cx)
                }
                cx.notify()
            }
            ShellTabRequest::DragChanged => {
                if self.main_window_tabs.read(cx).state.drag.is_none() {
                    self.split_drop_target = None
                }
                cx.notify()
            }
            ShellTabRequest::Select(id) => {
                self.split_drop_target = None;
                self.set_active_tab(id, window, cx)
            }
            ShellTabRequest::DoubleClick(id) => {
                if !self.onboarding_disclaimer_locked() {
                    self.begin_tab_rename(id, window, cx);
                }
            }
            ShellTabRequest::ContextMenu(id, position) => {
                self.open_tab_context_menu(id, position, cx)
            }
            ShellTabRequest::CloseTab { tab, select_first } => {
                self.clear_workspace_tooltip(&format!("workspace-tab-title-{}", tab.0), cx);
                if select_first {
                    self.set_active_tab(tab, window, cx);
                }
                self.request_close_tab_by_id(tab, window, cx)
            }
            ShellTabRequest::NewTab => {
                window.dispatch_action(Box::new(NewTerminal), cx);
            }
            ShellTabRequest::ShowTitle(tab, title, point) => self.queue_workspace_tooltip(
                format!("workspace-tab-title-{}", tab.0),
                title,
                f32::from(point.x) + 12.,
                f32::from(point.y) + 16.,
                cx,
            ),
            ShellTabRequest::HideTitle(tab) => {
                self.clear_workspace_tooltip(&format!("workspace-tab-title-{}", tab.0), cx)
            }
            ShellTabRequest::Detach(drag) => {
                let origin = self.tab_detach_handoff_origin(&drag, window);
                self.detach_tab_to_window(drag.tab_id, origin, window, cx);
            }
            ShellTabRequest::Reorder(tab, index) => {
                self.set_active_tab(tab, window, cx);
                if self.move_tab_to_visible_index(tab, index, cx) {
                    self.reveal_active_tab(window, cx);
                    cx.notify();
                }
            }
        }
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
    fn tabbar_wheel_scroll_ignores_minor_cross_axis_movement() {
        for (direction, expected) in [
            (-1.0, [112.0, 124.0, 136.0, 148.0]),
            (1.0, [88.0, 76.0, 64.0, 52.0]),
        ] {
            let mut scroll_x = 100.0;
            for (minor_delta, expected_scroll) in [0.0, 0.4, -0.3, 0.0].into_iter().zip(expected) {
                let delta = tabbar_wheel_scroll_delta(direction * 12.0, minor_delta);
                scroll_x = tabbar_scroll_x_after_wheel(scroll_x, delta, 500.0);
                assert_eq!(scroll_x, expected_scroll);
            }
        }
    }

    #[test]
    fn tabbar_vertical_wheel_scroll_preserves_direction_and_bounds() {
        for (delta_x, delta_y, current, expected) in [
            (0.0, -12.0, 100.0, 112.0),
            (0.4, -12.0, 100.0, 112.0),
            (-0.3, 12.0, 100.0, 88.0),
            (0.0, -12.0, 495.0, 500.0),
            (0.0, 12.0, 5.0, 0.0),
            (0.0, 0.0, 100.0, 100.0),
        ] {
            let delta = tabbar_wheel_scroll_delta(delta_x, delta_y);
            assert_eq!(tabbar_scroll_x_after_wheel(current, delta, 500.0), expected);
        }
    }

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
    fn focusing_terminal_selects_and_expands_its_node() {
        let node_id = NodeId("focus-only".to_string());
        let mut active_node_id = None;
        let mut expanded_node_ids = HashSet::new();

        focus_terminal_node_projection(&node_id, &mut active_node_id, &mut expanded_node_ids);

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
