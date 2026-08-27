use super::*;

const SPLIT_HANDLE_LINE_ALPHA: u32 = 0xb3;
const SPLIT_HANDLE_HOVER_BG_ALPHA: u32 = 0x12;
const SPLIT_HANDLE_ACTIVE_BG_ALPHA: u32 = 0x1f;
const SPLIT_HANDLE_HOVER_LINE_ALPHA: u32 = 0xcc;
const SPLIT_HANDLE_ACTIVE_LINE_ALPHA: u32 = 0xff;
const SPLIT_HANDLE_LINE_WIDTH: f32 = 1.0;
const SPLIT_HANDLE_HOVER_LINE_WIDTH: f32 = 3.0;
const SPLIT_HANDLE_ACTIVE_LINE_WIDTH: f32 = 4.0;

#[derive(Clone)]
pub(super) struct SplitDrag {
    tab_id: Option<TabId>,
    group_id: PaneId,
    handle_index: usize,
    direction: SplitDirection,
    start_position: gpui::Point<Pixels>,
    start_sizes: Vec<f32>,
}

#[derive(Clone, Copy)]
enum TerminalPaneInteraction {
    PrivilegePromptSubmit,
    ContextAction,
}

struct ActivePaneSplitTarget {
    tab_id: TabId,
    active_pane_id: PaneId,
    source: TerminalSplitSource,
}

enum TerminalSplitSource {
    Local,
    Ssh(NodeId),
}

fn terminal_split_supported(
    tab_kind: &TabKind,
    contains_serial_terminal: bool,
    ssh_node_ready: bool,
) -> bool {
    match tab_kind {
        TabKind::LocalTerminal => !contains_serial_terminal,
        TabKind::SshTerminal => ssh_node_ready,
        _ => false,
    }
}

#[derive(Clone)]
struct TerminalInputBroadcastRoute {
    source_pane_id: PaneId,
    tab_host: gpui::WeakEntity<tabs::WorkspaceTabHostEntity>,
    terminal: gpui::WeakEntity<WorkspaceTerminalEntity>,
}

impl TerminalInputBroadcastRoute {
    fn broadcaster(self) -> TerminalInputBroadcaster {
        Rc::new(move |kind, bytes, cx| self.deliver(kind, bytes, cx))
    }

    fn deliver(&self, kind: TerminalBroadcastInputKind, bytes: &[u8], cx: &mut App) {
        let Some(tab_host) = self.tab_host.upgrade() else {
            return;
        };
        let Some(terminal) = self.terminal.upgrade() else {
            return;
        };
        if !terminal.read(cx).broadcast_enabled() {
            return;
        }

        let (live_panes, mut candidates) = {
            let tab_host = tab_host.read(cx);
            let live_panes = tab_host.panes().keys().copied().collect::<HashSet<_>>();
            let mut candidates = Vec::new();
            for tab in tab_host.tabs() {
                if let Some(root) = tab.root_pane.as_ref() {
                    root.collect_pane_ids(&mut candidates);
                }
            }
            (live_panes, candidates)
        };
        candidates
            .retain(|pane_id| *pane_id != self.source_pane_id && live_panes.contains(pane_id));

        let targets = terminal.update(cx, |terminal, _cx| {
            terminal.retain_live_broadcast_targets(&live_panes);
            if !terminal.broadcast_enabled() {
                return Vec::new();
            }
            terminal.filter_broadcast_targets(candidates)
        });
        for pane_id in targets {
            let Some(pane) = tab_host.read(cx).panes().get(&pane_id).cloned() else {
                continue;
            };
            let _ = pane.update(cx, |pane, cx| {
                // Borrowed input is delivered synchronously and never retained
                // outside the target pane's existing zeroizing write path.
                pane.send_broadcast_input(kind, bytes, cx);
            });
        }
    }
}

impl WorkspaceApp {
    pub(super) fn register_terminal_pane(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        pane: gpui::Entity<TerminalPane>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let window_handle = window.window_handle();
        let terminal_label = pane.read(cx).title().to_string();
        let broadcaster = TerminalInputBroadcastRoute {
            source_pane_id: pane_id,
            tab_host: self.tab_host.downgrade(),
            terminal: self.terminal.downgrade(),
        }
        .broadcaster();
        pane.update(cx, |pane, _cx| {
            // Weak routing endpoints follow pane remounts without taking
            // ownership of the pane, its SSH channel, or the physical node.
            pane.set_input_broadcaster(Some(broadcaster));
        });
        self.tab_host.update(cx, |tab_host, cx| {
            tab_host.register_terminal_pane(pane_id, session_id, pane, window_handle, cx);
        });
        // The live terminal session is the capability owner. A later tab move
        // reuses this registration instead of minting another owner identity.
        self.ai_runtime_context.update(cx, |runtime, _cx| {
            runtime.register_terminal_session(session_id, terminal_label);
        });
    }

    pub(super) fn handle_terminal_pane_delivery(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        window_handle: AnyWindowHandle,
        event: TerminalPaneEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TerminalPaneEvent::Exited { .. } => {
                self.queue_auto_close_terminal_session(session_id, cx);
            }
            // TabHost consumes this signal before ordinary pane delivery.
            TerminalPaneEvent::OutputActivity => {}
            TerminalPaneEvent::TriggerMatchesAvailable => {
                let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id).cloned() else {
                    return;
                };
                let pane_owner = pane.downgrade();
                let matches = pane.update(cx, |pane, _cx| pane.take_trigger_matches());
                self.handle_terminal_trigger_matches(pane_id, session_id, pane_owner, matches, cx);
            }
            TerminalPaneEvent::CurrentDirectoryChanged => {
                if self.active_pane_id(cx) == Some(pane_id) {
                    self.sync_active_terminal_metadata_context(cx);
                }
            }
            TerminalPaneEvent::RecordingStatusChanged => {
                if self.active_pane_id(cx) == Some(pane_id) {
                    self.sync_active_terminal_recording_elapsed_tick(cx);
                }
            }
            TerminalPaneEvent::SessionLogStatusChanged => {
                if self.active_pane_id(cx) == Some(pane_id) {
                    cx.notify();
                }
            }
            TerminalPaneEvent::SearchStatusChanged => {
                if self.active_pane_id(cx) == Some(pane_id)
                    && self.search.visible
                    && let Some(pane) = self.active_pane(cx)
                {
                    self.search
                        .sync_from_terminal(pane.read(cx).search_status());
                    cx.notify();
                }
            }
            TerminalPaneEvent::PrivilegePromptStateChanged => {
                if self.active_pane_id(cx) == Some(pane_id)
                    && self.sync_active_privilege_prompt_inline_hint(cx)
                {
                    cx.notify();
                }
            }
            TerminalPaneEvent::PrivilegePromptSubmitRequested => self
                .deliver_terminal_pane_interaction(
                    pane_id,
                    window_handle,
                    TerminalPaneInteraction::PrivilegePromptSubmit,
                    cx,
                ),
            TerminalPaneEvent::ContextActionRequested => self.deliver_terminal_pane_interaction(
                pane_id,
                window_handle,
                TerminalPaneInteraction::ContextAction,
                cx,
            ),
        }
    }

    fn deliver_terminal_pane_interaction(
        &mut self,
        pane_id: PaneId,
        window_handle: AnyWindowHandle,
        interaction: TerminalPaneInteraction,
        cx: &mut Context<Self>,
    ) {
        // Defer the window-scoped action without putting secrets or selected text in the event.
        cx.spawn(async move |weak, cx| {
            let _ = cx.update_window(window_handle, |_, window, cx| {
                weak.update(cx, |workspace, cx| {
                    if workspace.active_pane_id(cx) != Some(pane_id) {
                        // A request cannot follow focus into another pane.
                        if let Some(pane) =
                            workspace.tab_host.read(cx).panes().get(&pane_id).cloned()
                        {
                            pane.update(cx, |pane, _cx| match interaction {
                                TerminalPaneInteraction::PrivilegePromptSubmit => {
                                    pane.take_privilege_prompt_submit_request();
                                }
                                TerminalPaneInteraction::ContextAction => {
                                    pane.take_context_action_request();
                                }
                            });
                        }
                        return;
                    }

                    let handled = match interaction {
                        TerminalPaneInteraction::PrivilegePromptSubmit => {
                            workspace.handle_active_privilege_prompt_submit_request(window, cx)
                        }
                        TerminalPaneInteraction::ContextAction => workspace
                            .handle_terminal_context_action_request_for_pane(pane_id, window, cx),
                    };
                    if handled {
                        cx.notify();
                    }
                })
            });
        })
        .detach();
    }

    pub(super) fn bind_terminal_location(
        &mut self,
        tab_id: TabId,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        self.tab_host.update(cx, |tab_host, _cx| {
            tab_host.bind_terminal_location(session_id, TerminalLocation { tab_id, pane_id });
        });
        self.refresh_terminal_trigger_pane(pane_id, cx);
        self.debug_assert_terminal_location(session_id, cx);
    }

    fn debug_assert_terminal_location(&self, session_id: TerminalSessionId, cx: &App) {
        // Release builds compile out the invariant checks; consume the ID so
        // release packaging remains warning-free.
        #[cfg(not(debug_assertions))]
        let _ = (session_id, cx);
        #[cfg(debug_assertions)]
        if let Some(location) = self.tab_host.read(cx).terminal_location(session_id) {
            let tree_location = self.tab_by_id(location.tab_id, cx).and_then(|tab| {
                tab.root_pane
                    .as_ref()
                    .and_then(|root| root.pane_id_for_session(session_id))
            });
            debug_assert_eq!(tree_location, Some(location.pane_id));
            debug_assert!(
                self.tab_host
                    .read(cx)
                    .panes()
                    .contains_key(&location.pane_id)
            );
        }
    }

    pub(super) fn remove_terminal_pane(
        &mut self,
        pane_id: &PaneId,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Entity<TerminalPane>> {
        self.tab_host
            .update(cx, |tab_host, _cx| tab_host.remove_terminal_pane(*pane_id))
    }

    pub(super) fn queue_auto_close_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        // Serial sessions report port failures through the same terminal event;
        // keep local transport panes visible so users can inspect the error
        // text and reconnect without recreating the whole tab.
        if self.serial_terminal_configs.contains_key(&session_id) {
            return;
        }
        if self.pending_auto_close_terminal_sessions.insert(session_id) {
            cx.notify();
        }
    }

    pub(super) fn schedule_pending_auto_close_terminal_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_auto_close_terminal_sessions.is_empty()
            || self.auto_close_terminal_sessions_scheduled
        {
            return;
        }
        self.auto_close_terminal_sessions_scheduled = true;
        let workspace = cx.entity();
        window.on_next_frame(move |window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.auto_close_terminal_sessions_scheduled = false;
                this.drain_pending_auto_close_terminal_sessions(window, cx);
            });
        });
    }

    fn drain_pending_auto_close_terminal_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session_ids: Vec<_> = self.pending_auto_close_terminal_sessions.drain().collect();
        for session_id in session_ids {
            if self.serial_terminal_configs.contains_key(&session_id) {
                continue;
            }
            self.close_terminal_session(session_id, window, cx);
        }
    }

    pub(super) fn active_tab_has_serial_terminal(&self, cx: &App) -> bool {
        let Some(tab) = self.active_tab(cx) else {
            return false;
        };
        let Some(root_pane) = tab.root_pane.as_ref() else {
            return false;
        };

        let mut session_ids = Vec::new();
        root_pane.collect_session_ids(&mut session_ids);
        session_ids
            .iter()
            .any(|session_id| self.serial_terminal_configs.contains_key(session_id))
    }

    pub(super) fn can_split_active_pane(&self, cx: &App) -> bool {
        self.active_pane_split_target(cx).is_some()
    }

    fn active_pane_split_target(&self, cx: &App) -> Option<ActivePaneSplitTarget> {
        let (tab_id, active_pane_id, pane_count, tab_kind) =
            self.active_tab(cx).and_then(|tab| {
                Some((
                    tab.id,
                    tab.active_pane_id?,
                    tab.root_pane.as_ref()?.pane_count(),
                    tab.kind.clone(),
                ))
            })?;
        if pane_count >= MAX_PANES_PER_TAB {
            return None;
        }

        let ssh_node_id = (tab_kind == TabKind::SshTerminal)
            .then(|| self.active_ssh_terminal_node_id(cx))
            .flatten();
        let ssh_node_ready = ssh_node_id
            .as_ref()
            .is_some_and(|node_id| self.node_is_ready_for_terminal(node_id));
        if !terminal_split_supported(
            &tab_kind,
            self.active_tab_has_serial_terminal(cx),
            ssh_node_ready,
        ) {
            return None;
        }

        let source = match tab_kind {
            TabKind::LocalTerminal => TerminalSplitSource::Local,
            TabKind::SshTerminal => TerminalSplitSource::Ssh(ssh_node_id?),
            _ => return None,
        };
        Some(ActivePaneSplitTarget {
            tab_id,
            active_pane_id,
            source,
        })
    }

    pub(super) fn split_active_pane(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.active_pane_split_target(cx) else {
            return;
        };
        if let TerminalSplitSource::Ssh(node_id) = &target.source {
            let node_id = node_id.clone();
            self.split_ssh_terminal_pane(target, node_id, direction, window, cx);
            return;
        }

        let group_id = self.alloc_pane_id(cx);
        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        let mut preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        let local_config = self.local_terminal_config();
        let local_preference_overrides =
            self.terminal_preference_overrides_for_local_shell(local_config.shell.as_ref());
        local_preference_overrides.apply_to(&mut preferences);
        let pane = cx.new(|cx| {
            TerminalPane::new_local_with_config_and_preferences(
                local_config,
                preferences,
                window,
                cx,
            )
            .expect("failed to initialize split terminal pane")
            .with_preference_overrides(local_preference_overrides)
        });

        if self.tab_host.update(cx, |tab_host, _| {
            tab_host.split_pane(
                target.tab_id,
                target.active_pane_id,
                group_id,
                direction,
                pane_id,
                session_id,
            )
        }) {
            self.register_terminal_pane(pane_id, session_id, pane.clone(), window, cx);
            self.bind_terminal_location(target.tab_id, pane_id, session_id, cx);
            self.needs_active_pane_focus = true;
            pane.update(cx, |pane, cx| pane.focus(window, cx));
            cx.notify();
        } else {
            let _ = pane.update(cx, |pane, _cx| pane.shutdown());
        }
    }

    fn split_ssh_terminal_pane(
        &mut self,
        target: ActivePaneSplitTarget,
        node_id: NodeId,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let group_id = self.alloc_pane_id(cx);
        let Ok((pane_id, session_id)) =
            self.create_ssh_terminal_pane_for_existing_node(&node_id, None, true, window, cx)
        else {
            return;
        };
        let mounted = self.tab_host.update(cx, |tab_host, _| {
            tab_host.split_pane(
                target.tab_id,
                target.active_pane_id,
                group_id,
                direction,
                pane_id,
                session_id,
            )
        });
        if mounted {
            self.bind_terminal_location(target.tab_id, pane_id, session_id, cx);
            self.needs_active_pane_focus = true;
            self.focus_active_pane(window, cx);
            cx.notify();
        } else {
            self.rollback_unmounted_ssh_terminal_pane(pane_id, session_id, cx);
        }
    }

    fn rollback_unmounted_ssh_terminal_pane(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) {
        // The node remains owned by NodeRouter; only the failed pane's projections and consumer
        // are revoked before its session is shut down.
        self.pending_auto_close_terminal_sessions
            .remove(&session_id);
        self.terminal_saved_connection_refs.remove(&session_id);
        self.clear_terminal_trigger_session_overrides(session_id);
        self.unregister_ssh_terminal_session(session_id, cx);
        if let Some(pane) = self.remove_terminal_pane(&pane_id, cx) {
            let _ = pane.update(cx, |pane, _cx| pane.shutdown());
        }
    }

    pub(super) fn close_active_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((tab_id, active_pane_id, pane_count, session_id)) =
            self.active_tab(cx).and_then(|tab| {
                let active_pane_id = tab.active_pane_id?;
                let root_pane = tab.root_pane.as_ref()?;
                Some((
                    tab.id,
                    active_pane_id,
                    root_pane.pane_count(),
                    root_pane.session_id_for_pane(active_pane_id),
                ))
            })
        else {
            return;
        };
        if pane_count <= 1 {
            return;
        }

        if let Some(session_id) = session_id {
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

        if let Some(pane) = self.remove_terminal_pane(&active_pane_id, cx) {
            let _ = pane.update(cx, |pane, _cx| pane.shutdown());
        }

        if self
            .tab_host
            .update(cx, |tab_host, _| {
                tab_host.close_pane(tab_id, active_pane_id)
            })
            .is_some()
        {
            self.needs_active_pane_focus = true;
            self.focus_active_pane(window, cx);
            cx.notify();
        }
    }

    pub(super) fn reset_active_tab_to_single_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((tab_id, active_pane_id, root_pane)) = self
            .active_tab(cx)
            .and_then(|tab| Some((tab.id, tab.active_pane_id?, tab.root_pane.as_ref()?.clone())))
        else {
            return;
        };
        if root_pane.pane_count() <= 1 {
            return;
        }
        let Some(active_session_id) = root_pane.session_id_for_pane(active_pane_id) else {
            return;
        };

        let mut pane_ids = Vec::new();
        root_pane.collect_pane_ids(&mut pane_ids);
        let mut session_ids = Vec::new();
        root_pane.collect_session_ids(&mut session_ids);

        for session_id in session_ids
            .into_iter()
            .filter(|session_id| *session_id != active_session_id)
        {
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
        for pane_id in pane_ids
            .into_iter()
            .filter(|pane_id| *pane_id != active_pane_id)
        {
            if let Some(pane) = self.remove_terminal_pane(&pane_id, cx) {
                let _ = pane.update(cx, |pane, _cx| pane.shutdown());
            }
        }

        self.tab_host.update(cx, |tab_host, _| {
            tab_host.reset_to_single_pane(tab_id, active_pane_id, active_session_id);
        });
        self.needs_active_pane_focus = true;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(super) fn start_split_drag(
        &mut self,
        tab_id: Option<TabId>,
        group_id: PaneId,
        handle_index: usize,
        direction: SplitDirection,
        sizes: &[f32],
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        self.split_drag = Some(SplitDrag {
            tab_id,
            group_id,
            handle_index,
            direction,
            start_position: event.position,
            start_sizes: sizes.to_vec(),
        });
        cx.notify();
    }

    pub(super) fn update_split_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.split_drag.clone() else {
            return;
        };
        // Splitters use root-level pointer capture. While dragging outside the
        // splitter element, the stored drag state owns motion until mouse-up.
        let viewport = window.viewport_size();
        let delta_fraction = match drag.direction {
            SplitDirection::Horizontal => {
                f32::from(event.position.x - drag.start_position.x)
                    / f32::from(viewport.width).max(1.0)
                    * 100.0
            }
            SplitDirection::Vertical => {
                f32::from(event.position.y - drag.start_position.y)
                    / f32::from(viewport.height).max(1.0)
                    * 100.0
            }
        };
        let next_sizes = adjusted_split_sizes(&drag.start_sizes, drag.handle_index, delta_fraction);
        let updated = self.tab_host.update(cx, |tab_host, _| {
            tab_host.update_group_sizes(drag.tab_id, drag.group_id, &next_sizes)
        });
        if updated {
            cx.notify();
        }
    }

    pub(super) fn finish_split_drag(&mut self, cx: &mut Context<Self>) {
        if self.split_drag.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn reset_split_group_sizes(
        &mut self,
        tab_id: Option<TabId>,
        group_id: PaneId,
        cx: &mut Context<Self>,
    ) {
        let updated = self.tab_host.update(cx, |tab_host, _| {
            tab_host.reset_group_sizes(tab_id, group_id)
        });
        if updated {
            cx.notify();
        }
    }

    pub(super) fn render_pane_tree(&self, node: &PaneNode, cx: &mut Context<Self>) -> AnyElement {
        self.render_pane_tree_for_tab(self.active_tab_id(cx), node, cx)
    }

    pub(super) fn render_pane_tree_for_tab(
        &self,
        tab_id: Option<TabId>,
        node: &PaneNode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_pane_id = tab_id
            .and_then(|tab_id| self.tab_by_id(tab_id, cx))
            .and_then(|tab| tab.active_pane_id);
        match node {
            PaneNode::Leaf { pane_id, .. } => {
                let active = Some(*pane_id) == active_pane_id;
                let Some(pane) = self.tab_host.read(cx).panes().get(pane_id).cloned() else {
                    return div().size_full().into_any_element();
                };
                div()
                    .id(("workspace-pane", pane_id.0))
                    .size_full()
                    .relative()
                    .min_w(px(self.tokens.metrics.min_pane_width))
                    .min_h(px(self.tokens.metrics.min_pane_height))
                    .overflow_hidden()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let pane_id = *pane_id;
                            let tab_id = tab_id;
                            move |this, _event, window, cx| {
                                if let Some(tab_id) = tab_id {
                                    this.tab_host.update(cx, |tab_host, _| {
                                        tab_host.set_active_pane(Some(tab_id), pane_id);
                                    });
                                    if !this.tab_host.read(cx).is_outside_main_window(tab_id) {
                                        this.set_main_window_active_tab(Some(tab_id), cx);
                                    }
                                } else {
                                    this.tab_host.update(cx, |tab_host, _| {
                                        tab_host.set_active_pane(None, pane_id);
                                    });
                                }
                                if let Some(pane) =
                                    this.tab_host.read(cx).panes().get(&pane_id).cloned()
                                {
                                    pane.update(cx, |pane, cx| pane.focus(window, cx));
                                }
                                cx.notify();
                            }
                        }),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(pane),
                    )
                    .when(
                        active && self.ai_entity.read(cx).terminal_inline_panel().open,
                        |pane_frame| pane_frame.child(self.render_terminal_ai_inline_panel(cx)),
                    )
                    .into_any_element()
            }
            PaneNode::Group {
                id,
                direction,
                children,
            } => {
                let sizes = node.split_sizes();
                let mut group = div()
                    .id(("workspace-pane-group", id.0))
                    .size_full()
                    .flex()
                    .overflow_hidden();
                group = match direction {
                    SplitDirection::Horizontal => group.flex_row(),
                    SplitDirection::Vertical => group.flex_col(),
                };

                for (index, child) in children.iter().enumerate() {
                    let basis = relative(sizes.get(index).copied().unwrap_or(0.0) / 100.0);
                    group = group.child(
                        div()
                            .flex_none()
                            .flex_basis(basis)
                            .relative()
                            .min_w(px(self.tokens.metrics.min_pane_width))
                            .min_h(px(self.tokens.metrics.min_pane_height))
                            .overflow_hidden()
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .bottom_0()
                                    .child(self.render_pane_tree_for_tab(tab_id, &child.node, cx)),
                            ),
                    );
                    if index + 1 < children.len() {
                        let group_id = *id;
                        let direction = *direction;
                        let start_sizes = sizes.clone();
                        let active_drag = self.split_drag.as_ref().is_some_and(|drag| {
                            drag.tab_id == tab_id
                                && drag.group_id == group_id
                                && drag.handle_index == index
                                && drag.direction == direction
                        });
                        let handle_bg = if active_drag {
                            rgba((self.tokens.ui.accent << 8) | SPLIT_HANDLE_ACTIVE_BG_ALPHA)
                        } else {
                            rgba(0x00000000)
                        };
                        let line_color = if active_drag {
                            rgba((self.tokens.ui.accent << 8) | SPLIT_HANDLE_ACTIVE_LINE_ALPHA)
                        } else {
                            rgba((self.tokens.ui.divider << 8) | SPLIT_HANDLE_LINE_ALPHA)
                        };
                        let line_width = if active_drag {
                            SPLIT_HANDLE_ACTIVE_LINE_WIDTH
                        } else {
                            SPLIT_HANDLE_LINE_WIDTH
                        };
                        let highlighted_line_width = if active_drag {
                            SPLIT_HANDLE_ACTIVE_LINE_WIDTH
                        } else {
                            SPLIT_HANDLE_HOVER_LINE_WIDTH
                        };
                        let highlighted_line_alpha = if active_drag {
                            SPLIT_HANDLE_ACTIVE_LINE_ALPHA
                        } else {
                            SPLIT_HANDLE_HOVER_LINE_ALPHA
                        };
                        let highlighted_line_color =
                            rgba((self.tokens.ui.accent << 8) | highlighted_line_alpha);
                        let split_handle_size = self.tokens.metrics.split_handle_size;
                        let handle_group = SharedString::from(format!(
                            "workspace-split-handle-{}-{index}",
                            group_id.0
                        ));
                        // The full handle remains easy to acquire while only the
                        // centered visual line grows on hover and during drag.
                        let line = div()
                            .absolute()
                            .bg(line_color)
                            .when(direction == SplitDirection::Horizontal, |line| {
                                line.top_0()
                                    .bottom_0()
                                    .left(px((split_handle_size - line_width) / 2.0))
                                    .w(px(line_width))
                            })
                            .when(direction == SplitDirection::Vertical, |line| {
                                line.left_0()
                                    .right_0()
                                    .top(px((split_handle_size - line_width) / 2.0))
                                    .h(px(line_width))
                            })
                            .group_hover(handle_group.clone(), move |style| {
                                let style = style.bg(highlighted_line_color);
                                match direction {
                                    SplitDirection::Horizontal => style
                                        .left(
                                            px((split_handle_size - highlighted_line_width) / 2.0),
                                        )
                                        .w(px(highlighted_line_width)),
                                    SplitDirection::Vertical => style
                                        .top(px((split_handle_size - highlighted_line_width) / 2.0))
                                        .h(px(highlighted_line_width)),
                                }
                            });
                        let mut handle = div()
                            .flex_none()
                            .relative()
                            .group(handle_group)
                            .bg(handle_bg)
                            .hover({
                                let accent = self.tokens.ui.accent;
                                let hover_bg_alpha = if active_drag {
                                    SPLIT_HANDLE_ACTIVE_BG_ALPHA
                                } else {
                                    SPLIT_HANDLE_HOVER_BG_ALPHA
                                };
                                move |style| style.bg(rgba((accent << 8) | hover_bg_alpha))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                    if event.click_count >= 2 {
                                        this.reset_split_group_sizes(tab_id, group_id, cx);
                                        return;
                                    }
                                    this.start_split_drag(
                                        tab_id,
                                        group_id,
                                        index,
                                        direction,
                                        &start_sizes,
                                        event,
                                        cx,
                                    );
                                }),
                            )
                            .child(line);
                        handle = match direction {
                            SplitDirection::Horizontal => handle
                                .w(px(self.tokens.metrics.split_handle_size))
                                .h_full()
                                .cursor(CursorStyle::ResizeColumn),
                            SplitDirection::Vertical => handle
                                .h(px(self.tokens.metrics.split_handle_size))
                                .w_full()
                                .cursor(CursorStyle::ResizeRow),
                        };
                        group = group.child(handle);
                    }
                }

                group.into_any_element()
            }
        }
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;

    #[test]
    fn terminal_split_support_matches_transport_ownership() {
        assert!(terminal_split_supported(
            &TabKind::LocalTerminal,
            false,
            false
        ));
        assert!(!terminal_split_supported(
            &TabKind::LocalTerminal,
            true,
            false
        ));
        assert!(terminal_split_supported(&TabKind::SshTerminal, false, true));
        assert!(!terminal_split_supported(
            &TabKind::SshTerminal,
            false,
            false
        ));
        assert!(!terminal_split_supported(
            &TabKind::MoshTerminal,
            false,
            true
        ));
    }
}
