use super::*;
use oxideterm_gpui_ui::button::{ButtonRadius, IconButtonOptions};
use oxideterm_terminal_triggers::SavedConnectionKind;

const SPLIT_HANDLE_LINE_ALPHA: u32 = 0xd9;
const SPLIT_HANDLE_HOVER_BG_ALPHA: u32 = 0x12;
const SPLIT_HANDLE_ACTIVE_BG_ALPHA: u32 = 0x1f;
const SPLIT_HANDLE_HOVER_LINE_ALPHA: u32 = 0xcc;
const SPLIT_HANDLE_ACTIVE_LINE_ALPHA: u32 = 0xff;
const SPLIT_HANDLE_LINE_WIDTH: f32 = 3.0;
const SPLIT_HANDLE_HOVER_LINE_WIDTH: f32 = 4.0;
const SPLIT_HANDLE_ACTIVE_LINE_WIDTH: f32 = 5.0;

#[derive(Clone)]
pub(super) struct SplitDrag {
    tab_id: Option<TabId>,
    group_id: PaneId,
    handle_index: usize,
    direction: SplitDirection,
    start_position: gpui::Point<Pixels>,
    start_sizes: Vec<f32>,
    start_extent: f32,
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
    kind: oxideterm_terminal::TerminalSessionKind,
    ssh_node_ready: bool,
) -> bool {
    use oxideterm_terminal::TerminalSessionKind;
    match kind {
        TerminalSessionKind::LocalPty => true,
        TerminalSessionKind::SshPty => ssh_node_ready,
        _ => false,
    }
}

fn serial_profile_line_ending(
    line_ending: oxideterm_terminal::SerialLineEnding,
) -> oxideterm_connections::SerialLineEnding {
    match line_ending {
        oxideterm_terminal::SerialLineEnding::Lf => oxideterm_connections::SerialLineEnding::Lf,
        oxideterm_terminal::SerialLineEnding::CrLf => oxideterm_connections::SerialLineEnding::CrLf,
        oxideterm_terminal::SerialLineEnding::Cr => oxideterm_connections::SerialLineEnding::Cr,
        oxideterm_terminal::SerialLineEnding::None => oxideterm_connections::SerialLineEnding::None,
    }
}

#[derive(Clone)]
struct TerminalInputBroadcastRoute {
    source_pane_id: PaneId,
    session_id: TerminalSessionId,
    ai_runtime: gpui::WeakEntity<crate::workspace::ai_runtime_context::AiRuntimeContextEntity>,
    agent_resources: oxideterm_ai::agent::AgentResourceCoordinator,
    tab_host: gpui::WeakEntity<tabs::WorkspaceTabHostEntity>,
    terminal: gpui::WeakEntity<WorkspaceTerminalEntity>,
}

impl TerminalInputBroadcastRoute {
    fn broadcaster(self) -> TerminalInputBroadcaster {
        Rc::new(move |kind, bytes, cx| self.deliver(kind, bytes, cx))
    }

    fn deliver(&self, kind: TerminalBroadcastInputKind, bytes: &[u8], cx: &mut App) {
        if let Some(runtime) = self.ai_runtime.upgrade() {
            if let Some(key) = runtime.read(cx).terminal_resource_key(self.session_id) {
                if self.agent_resources.has_owner(&key) {
                    self.agent_resources.invalidate(&key);
                }
            }
        }
        let Some(tab_host) = self.tab_host.upgrade() else {
            return;
        };
        let Some(terminal) = self.terminal.upgrade() else {
            return;
        };

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
            terminal.filter_broadcast_targets(self.source_pane_id, candidates)
        });
        for pane_id in targets {
            let session_id = tab_host
                .read(cx)
                .tabs()
                .iter()
                .find_map(|tab| tab.root_pane.as_ref()?.session_id_for_pane(pane_id));
            if let Some(runtime) = self.ai_runtime.upgrade() {
                if let Some(key) =
                    session_id.and_then(|id| runtime.read(cx).terminal_resource_key(id))
                {
                    if self.agent_resources.has_owner(&key) {
                        self.agent_resources.invalidate(&key);
                    }
                }
            }
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
            session_id,
            ai_runtime: self.ai_runtime_context.downgrade(),
            agent_resources: self.ai_entity.read(cx).agents.services.resources.clone(),
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
                if let Some(search) = self.search.panes.get_mut(&pane_id)
                    && search.visible
                    && let Some(pane) = self.tab_host.read(cx).panes().get(&pane_id)
                {
                    search.sync_from_terminal(pane.read(cx).search_status());
                    cx.notify();
                }
            }
            TerminalPaneEvent::SerialLineEndingsChanged { input, output } => {
                self.persist_serial_line_endings(session_id, input, output, cx);
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

    fn persist_serial_line_endings(
        &mut self,
        session_id: TerminalSessionId,
        input: Option<oxideterm_terminal::SerialLineEnding>,
        output: Option<oxideterm_terminal::SerialLineEnding>,
        cx: &mut Context<Self>,
    ) {
        let Some(saved_connection) = self.terminal_saved_connection_refs.get(&session_id) else {
            return;
        };
        if saved_connection.kind != SavedConnectionKind::Serial {
            return;
        }
        let profile_id = saved_connection.id.clone();
        let input_line_ending = input.map(serial_profile_line_ending);
        let output_line_ending = output.map(serial_profile_line_ending);
        match self.connection_store.set_serial_profile_line_endings(
            &profile_id,
            input_line_ending,
            output_line_ending,
        ) {
            Ok(true) => self.queue_cloud_sync_dirty_refresh(cx),
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(%profile_id, %error, "failed to persist serial line endings");
            }
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
        if self
            .ai_entity
            .read(cx)
            .terminal_inline_panel()
            .target
            .is_some_and(|(id, _)| id == *pane_id)
        {
            self.ai_entity
                .update(cx, |ai, _| ai.close_terminal_inline_panel());
        }
        if self.search.focused == Some(*pane_id) {
            self.ime_marked_text = None;
            self.clear_ime_selection();
        }
        self.search.remove(*pane_id);
        self.terminal.update(cx, |terminal, _| {
            terminal.sync_groups_mut().remove(*pane_id)
        });
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

    pub(super) fn can_split_active_pane(&self, cx: &App) -> bool {
        self.active_pane_split_target(cx).is_some()
    }

    fn active_pane_split_target(&self, cx: &App) -> Option<ActivePaneSplitTarget> {
        let (tab_id, active_pane_id, pane_count) = self.active_tab(cx).and_then(|tab| {
            Some((
                tab.id,
                tab.active_pane_id?,
                tab.root_pane.as_ref()?.pane_count(),
            ))
        })?;
        if pane_count >= MAX_PANES_PER_TAB {
            return None;
        }

        let kind = self.terminal_kind_for_pane(active_pane_id, cx)?;
        let node = self.active_ssh_terminal_node_id(cx);
        if !terminal_split_supported(
            kind,
            node.as_ref()
                .is_some_and(|id| self.node_is_ready_for_terminal(id)),
        ) {
            return None;
        }
        let source = match kind {
            oxideterm_terminal::TerminalSessionKind::LocalPty => TerminalSplitSource::Local,
            oxideterm_terminal::TerminalSessionKind::SshPty => {
                let node = self.active_ssh_terminal_node_id(cx)?;
                if !self.node_is_ready_for_terminal(&node) {
                    return None;
                }
                TerminalSplitSource::Ssh(node)
            }
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
        let host = self.tab_host.read(cx);
        let source_instance = host
            .tab_by_id(target.tab_id)
            .and_then(|tab| tab.root_pane.as_ref())
            .and_then(|root| root.session_id_for_pane(target.active_pane_id))
            .and_then(|session_id| host.local_sessions.get(&session_id))
            .cloned();
        let mut local_config = self.local_terminal_config();
        if let Some(instance) = &source_instance {
            local_config.shell = instance.shell.clone();
            local_config.cwd = instance.cwd.clone();
        }
        if let Some(pane) = self.tab_host.read(cx).panes().get(&target.active_pane_id) {
            if let Some(snapshot) = terminal_cwd::terminal_cwd_snapshot_from_pane(
                oxideterm_environment::CurrentDirectoryScope::Local,
                pane.read(cx),
            ) {
                local_config.cwd = Some(std::path::PathBuf::from(snapshot.path()));
            }
        }
        let instance = source_instance.unwrap_or_else(|| {
            local_sessions::LocalTerminalInstance::new(
                &local_config,
                self.local_terminal_tab_title(),
            )
        });
        let local_preference_overrides =
            self.terminal_preference_overrides_for_local_shell(local_config.shell.as_ref());
        local_preference_overrides.apply_to(&mut preferences);
        let shared_session = match TerminalPane::local_shared_session(local_config, &preferences) {
            Ok(session) => session,
            Err(error) => {
                self.session_manager.update(cx, |manager, cx| {
                    manager.set_status(Some(error.to_string()), cx)
                });
                return;
            }
        };
        let pane = cx.new(|cx| {
            TerminalPane::from_shared_session(shared_session, preferences, window, cx)
                .expect("failed to initialize split terminal view")
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
            self.tab_host.update(cx, |host, _| {
                host.local_sessions.insert(session_id, instance);
            });
            self.register_terminal_pane(pane_id, session_id, pane.clone(), window, cx);
            self.bind_terminal_location(target.tab_id, pane_id, session_id, cx);
            self.activate_embedded_sftp_sidebar_if_visible(cx);
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
            self.activate_embedded_sftp_sidebar_if_visible(cx);
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
        let Some((tab_id, active_pane_id, pane_count)) = self.active_tab(cx).and_then(|tab| {
            let active_pane_id = tab.active_pane_id?;
            let root_pane = tab.root_pane.as_ref()?;
            Some((tab.id, active_pane_id, root_pane.pane_count()))
        }) else {
            return;
        };
        if pane_count <= 1 {
            return;
        }

        if let Some(page) = self
            .tab_by_id(tab_id, cx)
            .and_then(|tab| tab.root_pane.as_ref()?.page_id_for_pane(active_pane_id))
        {
            self.request_close_tab_by_id(page, window, cx);
        } else {
            self.close_terminal_pane_in_tab(tab_id, active_pane_id, window, cx);
        }
    }

    pub(in crate::workspace) fn close_terminal_pane_in_tab(
        &mut self,
        tab_id: TabId,
        active_pane_id: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self
            .tab_by_id(tab_id, cx)
            .and_then(|tab| tab.root_pane.as_ref())
        else {
            return;
        };
        let Some(session_id) = root.session_id_for_pane(active_pane_id) else {
            return;
        };
        if root.pane_count() == 1 {
            self.close_tab_by_id(tab_id, window, cx);
            return;
        }
        let session_id = Some(session_id);
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
            if self.active_tab_id(cx) == Some(tab_id) {
                self.sync_active_tab_surface(cx);
            }
            if self.active_tab_id(cx) == Some(tab_id) || self.tab_host.read(cx).is_detached(tab_id)
            {
                self.focus_tab_terminal(tab_id, window, cx);
            }
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
        let mut pane_ids = Vec::new();
        root_pane.collect_pane_ids(&mut pane_ids);
        for pane in pane_ids.into_iter().filter(|id| *id != active_pane_id) {
            if let Some(page) = root_pane.page_id_for_pane(pane) {
                self.close_tab_by_id(page, window, cx);
            } else {
                self.close_terminal_pane_in_tab(tab_id, pane, window, cx);
            }
        }
        self.sync_active_tab_surface(cx);
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
        extent: f32,
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
            start_extent: extent,
        });
        cx.notify();
    }

    pub(super) fn update_split_drag(
        &mut self,
        event: &MouseMoveEvent,
        _window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.split_drag.clone() else {
            return;
        };
        // Splitters use root-level pointer capture. While dragging outside the
        // splitter element, the stored drag state owns motion until mouse-up.
        let delta_fraction = match drag.direction {
            SplitDirection::Horizontal => {
                f32::from(event.position.x - drag.start_position.x) / drag.start_extent.max(1.0)
                    * 100.0
            }
            SplitDirection::Vertical => {
                f32::from(event.position.y - drag.start_position.y) / drag.start_extent.max(1.0)
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

    pub(in crate::workspace) fn split_drag_belongs_to_tab(&self, tab_id: TabId) -> bool {
        self.split_drag
            .as_ref()
            .is_some_and(|drag| drag.tab_id == Some(tab_id))
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

    fn pane_header_action(
        &self,
        icon: LucideIcon,
        label_key: &'static str,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        let tokens = self.tokens;
        self.workspace_icon_action_button(
            icon,
            13.0,
            rgb(tokens.ui.text_muted),
            IconButtonOptions::opaque_toolbar(22.0, ButtonRadius::Md),
            listener,
            cx,
        )
        .id(label_key)
        .flex_none()
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .tooltip(move |_, cx| {
            oxideterm_gpui_ui::tooltip::tooltip_view(tokens, label.clone(), None, cx)
        })
        .into_any_element()
    }

    fn render_terminal_pane_header(
        &self,
        tab_id: TabId,
        pane_id: PaneId,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(28.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .bg(self.workspace_chrome_background(theme.bg))
            .border_b_1()
            .border_color(rgb(if active { theme.accent } else { theme.border }))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(self.terminal_pane_label(pane_id, cx)),
            )
            .child(format!("#{}", pane_id.0))
            .child(self.pane_header_action(
                LucideIcon::AppWindow,
                "tabbar.move_pane_to_tab",
                move |this, _, window, cx| {
                    this.move_terminal_pane_out(tab_id, pane_id, false, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .child(self.pane_header_action(
                LucideIcon::ExternalLink,
                "tabbar.move_pane_to_window",
                move |this, _, window, cx| {
                    this.move_terminal_pane_out(tab_id, pane_id, true, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .child(self.pane_header_action(
                LucideIcon::X,
                "command_palette.cmd_close_pane",
                move |this, _, window, cx| {
                    this.close_terminal_pane_in_tab(tab_id, pane_id, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_pane_tree(
        &mut self,
        node: &PaneNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_pane_tree_for_tab(self.active_tab_id(cx), node, window, cx)
    }

    fn render_workspace_page_header(
        &self,
        page: TabId,
        title: &str,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(28.0))
            .flex_none()
            .px(px(6.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .bg(self.workspace_chrome_background(theme.bg))
            .border_b_1()
            .border_color(rgb(if active { theme.accent } else { theme.border }))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(theme.text_muted))
            .child(div().flex_1().min_w_0().truncate().child(title.to_owned()))
            .child(self.pane_header_action(
                LucideIcon::AppWindow,
                "tabbar.move_pane_to_tab",
                move |this, _, window, cx| {
                    this.move_workspace_page_out(page, false, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .child(self.pane_header_action(
                LucideIcon::ExternalLink,
                "tabbar.move_pane_to_window",
                move |this, _, window, cx| {
                    this.move_workspace_page_out(page, true, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .child(self.pane_header_action(
                LucideIcon::X,
                "command_palette.cmd_close_pane",
                move |this, _, window, cx| {
                    this.request_close_tab_by_id(page, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ))
            .into_any_element()
    }

    fn move_workspace_page_out(
        &mut self,
        page: TabId,
        new_window: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(container) = self.tab_host.update(cx, |host, _| host.unembed_page(page)) else {
            return;
        };
        if self
            .tab_by_id(container, cx)
            .is_some_and(|tab| tab.root_pane.is_none())
        {
            self.close_tab_by_id(container, window, cx);
        }
        self.set_main_window_active_tab(Some(page), cx);
        self.sync_active_tab_surface(cx);
        self.sync_ide_surface_mount(page, cx);
        if new_window {
            self.detach_tab_to_window(page, None, window, cx);
        } else if let Some(main) = self
            .window_registry
            .handle_for_role(window_registry::WindowRole::Main)
        {
            if main.window_id() != window.window_handle().window_id() {
                let _ = main.update(cx, |_, window, _| window.activate_window());
            }
        }
        cx.notify();
    }

    pub(super) fn render_pane_tree_for_tab(
        &mut self,
        tab_id: Option<TabId>,
        node: &PaneNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_pane_id = tab_id
            .and_then(|tab_id| self.tab_by_id(tab_id, cx))
            .and_then(|tab| tab.active_pane_id);
        let content = match node {
            PaneNode::Page {
                pane_id,
                tab_id: page_id,
            } => {
                let Some(page) = self.tab_by_id(*page_id, cx).cloned() else {
                    return div().into_any_element();
                };
                let (pane_id, page_id) = (*pane_id, *page_id);
                let container = tab_id.unwrap_or(page_id);
                let content = match page.kind {
                    TabKind::Sftp => self.render_sftp_surface_for_tab(page_id, window, cx),
                    TabKind::Ide => self.render_ide_surface_for_tab(page_id, cx),
                    TabKind::Forwards => self.render_forwards_surface_for_tab(page_id, window, cx),
                    _ => div().into_any_element(),
                };
                div()
                    .id(("workspace-page", pane_id.0))
                    .size_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        if this
                            .tab_host
                            .update(cx, |host, _| host.set_active_pane(Some(container), pane_id))
                        {
                            this.blur_text_inputs(cx);
                            if !this.tab_host.read(cx).is_detached(container) {
                                this.sync_active_tab_surface(cx);
                            }
                        }
                        cx.notify();
                    }))
                    .child(self.render_workspace_page_header(
                        page_id,
                        &page.title,
                        Some(pane_id) == active_pane_id,
                        cx,
                    ))
                    .child(div().flex_1().min_h_0().relative().child(content))
                    .into_any_element()
            }
            PaneNode::Leaf { pane_id, .. } => {
                let active = Some(*pane_id) == active_pane_id;
                let Some(pane) = self.tab_host.read(cx).panes().get(pane_id).cloned() else {
                    return div().size_full().into_any_element();
                };
                let sync_header = self.render_terminal_sync_member_header(*pane_id, cx);
                let split = tab_id
                    .and_then(|id| self.tab_by_id(id, cx))
                    .and_then(|tab| tab.root_pane.as_ref())
                    .is_some_and(|root| root.pane_count() > 1);
                let terminal_top = if sync_header.is_some() {
                    terminal_command_bar::TERMINAL_SYNC_HEADER_HEIGHT
                } else {
                    0.0
                } + if split { 28.0 } else { 0.0 };
                let header = div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .flex()
                    .flex_col()
                    .when(split, |header| {
                        header.child(self.render_terminal_pane_header(
                            tab_id.unwrap(),
                            *pane_id,
                            active,
                            cx,
                        ))
                    })
                    .children(sync_header);
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
                            move |this, _event, window, cx| {
                                // Release logical input ownership before focusing the native pane.
                                this.blur_text_inputs(cx);
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
                                if tab_id.is_none_or(|id| !this.tab_host.read(cx).is_detached(id)) {
                                    this.sync_active_tab_surface(cx);
                                    this.sync_active_terminal_metadata_context(cx);
                                    this.sync_active_terminal_recording_elapsed_tick(cx);
                                    this.sync_active_privilege_prompt_inline_hint(cx);
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
                    .child(header)
                    .child(
                        div()
                            .absolute()
                            .top(px(terminal_top))
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(pane),
                    )
                    .when(
                        self.ai_entity.read(cx).terminal_inline_panel().open
                            && self
                                .ai_entity
                                .read(cx)
                                .terminal_inline_panel()
                                .target
                                .is_some_and(|(id, _)| id == *pane_id),
                        |pane_frame| pane_frame.child(self.render_terminal_ai_inline_panel(cx)),
                    )
                    .when(
                        self.search
                            .panes
                            .get(pane_id)
                            .is_some_and(|search| search.visible),
                        |frame| frame.child(self.render_search_bar(*pane_id, cx)),
                    )
                    .into_any_element()
            }
            PaneNode::Group {
                id,
                direction,
                children,
            } => {
                let sizes = node.split_sizes();
                let extent = Rc::new(Cell::new(1.0_f32));
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
                                    .child(self.render_pane_tree_for_tab(
                                        tab_id,
                                        &child.node,
                                        window,
                                        cx,
                                    )),
                            ),
                    );
                    if index + 1 < children.len() {
                        let group_id = *id;
                        let direction = *direction;
                        let start_sizes = sizes.clone();
                        let drag_extent = extent.clone();
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
                                        drag_extent.get(),
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

                let direction = *direction;
                div()
                    .size_full()
                    .child(group)
                    .on_children_prepainted(move |bounds, _, _| {
                        if let Some(bounds) = bounds.first() {
                            extent.set(f32::from(match direction {
                                SplitDirection::Horizontal => bounds.size.width,
                                SplitDirection::Vertical => bounds.size.height,
                            }));
                        }
                    })
                    .into_any_element()
            }
        };
        match (tab_id, node) {
            (Some(tab), PaneNode::Leaf { pane_id, .. } | PaneNode::Page { pane_id, .. }) => {
                self.wrap_split_drop_region(tab, Some(*pane_id), content, window, cx)
            }
            _ => content,
        }
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;

    #[test]
    fn terminal_split_support_matches_transport_ownership() {
        use oxideterm_terminal::TerminalSessionKind::*;
        for (kind, ready, supported) in [
            (LocalPty, false, true),
            (SshPty, true, true),
            (SshPty, false, false),
            (Serial, true, false),
            (Telnet, true, false),
            (Mosh, true, false),
        ] {
            assert_eq!(
                terminal_split_supported(kind, ready),
                supported,
                "{kind:?}, ready={ready}"
            );
        }
    }
}
