// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_gpui_shell::tabs::TabLayout;
pub(in crate::workspace) use oxideterm_gpui_shell::tabs::TabMountId;

const RECORDING_ELAPSED_TICK_INTERVAL: Duration = Duration::from_millis(530);

/// Owns workspace-wide tab identity, terminal mounts, navigation, and close lifecycle.
pub(in crate::workspace) struct WorkspaceTabHostEntity {
    layout: TabLayout<TabAffinity, TabMount>,
    next_session_id: u64,
    panes: HashMap<PaneId, Entity<TerminalPane>>,
    pane_subscriptions: HashMap<PaneId, Subscription>,
    pane_window_affinities: HashMap<PaneId, TerminalPaneWindowAffinity>,
    terminal_locations: HashMap<TerminalSessionId, TerminalLocation>,
    pub(in crate::workspace) local_sessions:
        HashMap<TerminalSessionId, super::super::local_sessions::LocalTerminalInstance>,
    process_close_check_generation: u64,
    process_close_check_task: Option<gpui::Task<()>>,
    process_close_completion: Option<TabCloseProcessCompletion>,
    close_confirm: Option<TabCloseConfirm>,
    close_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence,
    close_confirm_focused_action: Option<ConfirmDialogAction>,
    close_confirm_exit_task: Option<gpui::Task<()>>,
    recording_elapsed_pane_id: Option<PaneId>,
    recording_elapsed_generation: u64,
    recording_elapsed_task: Option<gpui::Task<()>>,
}

/// Identifies the single tab and pane currently mounting one terminal session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct TerminalLocation {
    pub(in crate::workspace) tab_id: TabId,
    pub(in crate::workspace) pane_id: PaneId,
}

/// Identifies the native window currently mounting a workspace tab.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TabMount {
    Detached {
        mount_id: TabMountId,
        window_id: gpui::WindowId,
        handle: AnyWindowHandle,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TabMountCloseReason {
    ReturnToMain,
    TabClosed,
}

/// Describes the native-window cleanup that follows an Entity-owned mount transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct TabMountCleanupPlan {
    pub(in crate::workspace) tab_id: TabId,
    pub(in crate::workspace) reason: TabMountCloseReason,
    pub(in crate::workspace) detached_window: Option<AnyWindowHandle>,
}

pub(in crate::workspace) struct TabRemovalTransition {
    pub(in crate::workspace) tab: Tab,
    pub(in crate::workspace) mount_cleanup: TabMountCleanupPlan,
    pub(in crate::workspace) previous_active_tab_id: Option<TabId>,
    pub(in crate::workspace) next_active_tab_id: Option<TabId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct MainTabSelectionChange {
    pub(in crate::workspace) previous: Option<TabId>,
    pub(in crate::workspace) current: Option<TabId>,
}

pub(in crate::workspace) struct TabDetachTransition {
    pub(in crate::workspace) mount_id: TabMountId,
    pub(in crate::workspace) selection: MainTabSelectionChange,
}

pub(in crate::workspace) struct TabReturnTransition {
    pub(in crate::workspace) cleanup: TabMountCleanupPlan,
    pub(in crate::workspace) selection: MainTabSelectionChange,
}

#[derive(Clone, Copy)]
struct TerminalPaneWindowAffinity {
    home: AnyWindowHandle,
    current: AnyWindowHandle,
}

// TerminalPaneDelivery forwards the pane event payload, which is Clone but
// not Copy (ToolbarActionInvoked carries an owned id).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum WorkspaceTabHostEvent {
    CloseProcessCheckReady,
    RecordingElapsedTick {
        pane_id: PaneId,
    },
    TerminalOutputUnread,
    TerminalBellTriggered,
    TerminalPaneDelivery {
        pane_id: PaneId,
        session_id: TerminalSessionId,
        window_handle: AnyWindowHandle,
        event: TerminalPaneEvent,
    },
}

pub(in crate::workspace) struct TabCloseProcessProbe {
    pub(in crate::workspace) pane_id: PaneId,
    pub(in crate::workspace) probe: Option<oxideterm_terminal::TerminalProcessProbe>,
    pub(in crate::workspace) cached: oxideterm_terminal::TerminalProcessInfo,
}

pub(in crate::workspace) struct TabCloseProcessCompletion {
    pub(in crate::workspace) request: LocalTerminalCloseCheck,
    pub(in crate::workspace) results: Vec<(PaneId, oxideterm_terminal::TerminalProcessInfo)>,
    pub(in crate::workspace) has_foreground_child: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct TabCloseConfirmSnapshot {
    pub(in crate::workspace) confirm: TabCloseConfirm,
    pub(in crate::workspace) phase: oxideterm_gpui_ui::motion::ExitPhase,
    pub(in crate::workspace) focused_action: Option<ConfirmDialogAction>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TabCloseConfirmKeyAction {
    Cancel,
    Confirm,
    Handled,
}

impl std::ops::Deref for WorkspaceTabHostEntity {
    type Target = TabLayout<TabAffinity, TabMount>;
    fn deref(&self) -> &Self::Target {
        &self.layout
    }
}
impl std::ops::DerefMut for WorkspaceTabHostEntity {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.layout
    }
}
impl WorkspaceTabHostEntity {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            layout: TabLayout::default(),
            next_session_id: 1,
            panes: HashMap::new(),
            pane_subscriptions: HashMap::new(),
            pane_window_affinities: HashMap::new(),
            terminal_locations: HashMap::new(),
            local_sessions: HashMap::new(),
            process_close_check_generation: 0,
            process_close_check_task: None,
            process_close_completion: None,
            close_confirm: None,
            close_confirm_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            close_confirm_focused_action: None,
            close_confirm_exit_task: None,
            recording_elapsed_pane_id: None,
            recording_elapsed_generation: 0,
            recording_elapsed_task: None,
        }
    }

    /// Removes one canonical tab and chooses the next main-window selection atomically.
    pub(in crate::workspace) fn remove_tab_at(
        &mut self,
        index: usize,
    ) -> Option<TabRemovalTransition> {
        let removed = self.layout.remove_tab_at(index)?;
        Some(TabRemovalTransition {
            mount_cleanup: TabMountCleanupPlan {
                tab_id: removed.tab.id,
                reason: TabMountCloseReason::TabClosed,
                detached_window: removed
                    .mount
                    .map(|TabMount::Detached { handle, .. }| handle),
            },
            tab: removed.tab,
            previous_active_tab_id: removed.previous_active_tab_id,
            next_active_tab_id: removed.next_active_tab_id,
        })
    }
    pub(in crate::workspace) fn combine_pages(
        &mut self,
        source: TabId,
        target: TabId,
        direction: SplitDirection,
    ) -> Option<(Tab, Vec<TabId>)> {
        self.combine_pages_at(source, target, None, direction, false)
    }

    pub(in crate::workspace) fn combine_pages_at(
        &mut self,
        source: TabId,
        target: TabId,
        target_pane: Option<PaneId>,
        direction: SplitDirection,
        before: bool,
    ) -> Option<(Tab, Vec<TabId>)> {
        let (tab, removed) =
            self.layout
                .combine_pages_at(source, target, target_pane, direction, before)?;
        self.rebind_layout_sessions(&tab);
        Some((tab, removed))
    }

    fn rebind_layout_sessions(&mut self, tab: &Tab) {
        let mut sessions = Vec::new();
        if let Some(root) = &tab.root_pane {
            root.collect_session_ids(&mut sessions);
        }
        for session in sessions {
            if let Some(location) = self.terminal_locations.get_mut(&session) {
                location.tab_id = tab.id;
            }
        }
    }
    pub(in crate::workspace) fn can_tile_terminal_panes(&self, panes: &[PaneId]) -> bool {
        let unique = panes.iter().copied().collect::<HashSet<_>>();
        (2..=MAX_PANES_PER_TAB).contains(&panes.len())
            && unique.len() == panes.len()
            && panes.iter().all(|pane| {
                self.panes.contains_key(pane)
                    && self.terminal_locations.iter().any(|(session, location)| {
                        location.pane_id == *pane
                            && !self.is_outside_main_window(location.tab_id)
                            && self
                                .tab_by_id(location.tab_id)
                                .and_then(|tab| tab.root_pane.as_ref())
                                .and_then(|root| root.session_id_for_pane(*pane))
                                == Some(*session)
                    })
            })
    }

    /// Validate and prepare every source tree before committing the layout change.
    /// Terminal entities, subscriptions, and connection consumers stay owned here.
    pub(in crate::workspace) fn tile_terminal_panes(
        &mut self,
        panes: &[PaneId],
        title: String,
        landscape: bool,
    ) -> Option<(Tab, Vec<TabId>)> {
        if !self.can_tile_terminal_panes(panes) {
            return None;
        }
        let result = self.layout.tile_terminal_panes(panes, title, landscape)?;
        self.rebind_layout_sessions(&result.0);
        Some(result)
    }
    /// Moving a pane preserves its Entity, session, subscriptions, and runtime consumers.
    pub(in crate::workspace) fn move_terminal_pane_to_tab(
        &mut self,
        source_id: TabId,
        pane_id: PaneId,
        new_tab: Tab,
        main_window: AnyWindowHandle,
    ) -> bool {
        let id = new_tab.id;
        let Some(session_id) = new_tab
            .root_pane
            .as_ref()
            .and_then(|root| root.session_id_for_pane(pane_id))
        else {
            return false;
        };
        if !self
            .layout
            .move_terminal_pane_to_tab(source_id, pane_id, new_tab)
        {
            return false;
        }
        if let Some(location) = self.terminal_locations.get_mut(&session_id) {
            location.tab_id = id;
        }
        // A pane moved from a detached window is mounted in the main window until detached again.
        if let Some(affinity) = self.pane_window_affinities.get_mut(&pane_id) {
            affinity.home = main_window;
            affinity.current = main_window;
        }
        true
    }

    pub(in crate::workspace) fn terminal_pane_for_target(
        &self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
    ) -> Option<Entity<TerminalPane>> {
        let location = self.terminal_location(session_id)?;
        (location.pane_id == pane_id)
            .then(|| self.panes.get(&pane_id).cloned())
            .flatten()
    }

    pub(in crate::workspace) fn alloc_session_id(&mut self) -> TerminalSessionId {
        let id = TerminalSessionId(self.next_session_id);
        self.next_session_id += 1;
        id
    }

    pub(in crate::workspace) fn sync_recording_elapsed_tick(
        &mut self,
        pane_id: Option<PaneId>,
        recording: bool,
        cx: &mut Context<Self>,
    ) {
        let pane_id = pane_id.filter(|_| recording);
        if self.recording_elapsed_pane_id == pane_id
            && (pane_id.is_none() || self.recording_elapsed_task.is_some())
        {
            return;
        }
        self.recording_elapsed_pane_id = pane_id;
        self.recording_elapsed_generation = self.recording_elapsed_generation.wrapping_add(1);
        self.recording_elapsed_task = None;
        let Some(pane_id) = pane_id else {
            return;
        };
        let generation = self.recording_elapsed_generation;
        self.recording_elapsed_task = Some(cx.spawn(async move |tab_host, cx| {
            loop {
                Timer::after(RECORDING_ELAPSED_TICK_INTERVAL).await;
                let should_continue = tab_host
                    .update(cx, |tab_host, cx| {
                        if tab_host.recording_elapsed_generation != generation
                            || tab_host.recording_elapsed_pane_id != Some(pane_id)
                        {
                            return false;
                        }
                        cx.emit(WorkspaceTabHostEvent::RecordingElapsedTick { pane_id });
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }));
    }

    pub(in crate::workspace) fn bind_terminal_location(
        &mut self,
        session_id: TerminalSessionId,
        location: TerminalLocation,
    ) {
        // A session may be registered repeatedly at the same mount boundary,
        // but moving it requires the previous pane lifecycle to unbind first.
        let previous = self.terminal_locations.insert(session_id, location);
        debug_assert!(
            previous.is_none_or(|previous| previous == location),
            "terminal session was rebound without removing its previous location"
        );
        if let Some(window_handle) = self.detached_window_handle(location.tab_id)
            && let Some(affinity) = self.pane_window_affinities.get_mut(&location.pane_id)
        {
            // Reconnect remounts can register a replacement pane after the tab
            // moved, so location binding reapplies the current window owner.
            affinity.current = window_handle;
        }
    }

    pub(in crate::workspace) fn register_terminal_pane(
        &mut self,
        pane_id: PaneId,
        session_id: TerminalSessionId,
        pane: Entity<TerminalPane>,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        // TabHost owns pane delivery and its cancellation together with the
        // registered Entity. Delivery resolves the current mount dynamically
        // so detach and return cannot retain the creation window.
        self.pane_window_affinities.insert(
            pane_id,
            TerminalPaneWindowAffinity {
                home: window_handle,
                current: window_handle,
            },
        );
        let subscription = cx.subscribe(&pane, move |tab_host, _pane, event, cx| {
            if *event == TerminalPaneEvent::OutputActivity
                && tab_host.mark_terminal_output_unread(session_id).is_some()
            {
                cx.emit(WorkspaceTabHostEvent::TerminalOutputUnread);
            } else if *event == TerminalPaneEvent::BellActivity
                && tab_host.mark_terminal_bell(session_id).is_some()
            {
                cx.emit(WorkspaceTabHostEvent::TerminalBellTriggered);
            }
            let Some(window_handle) = tab_host
                .pane_window_affinities
                .get(&pane_id)
                .map(|affinity| affinity.current)
            else {
                return;
            };
            cx.emit(WorkspaceTabHostEvent::TerminalPaneDelivery {
                pane_id,
                session_id,
                window_handle,
                event: event.clone(),
            });
        });
        self.pane_subscriptions.insert(pane_id, subscription);
        self.panes.insert(pane_id, pane);
    }

    pub(in crate::workspace) fn remove_terminal_pane(
        &mut self,
        pane_id: PaneId,
    ) -> Option<Entity<TerminalPane>> {
        self.pane_subscriptions.remove(&pane_id);
        self.pane_window_affinities.remove(&pane_id);
        if let Some(session_id) = self.unbind_terminal_location_for_pane(pane_id) {
            self.local_sessions.remove(&session_id);
        }
        self.panes.remove(&pane_id)
    }

    pub(in crate::workspace) fn panes(&self) -> &HashMap<PaneId, Entity<TerminalPane>> {
        &self.panes
    }

    pub(in crate::workspace) fn sync_terminal_render_caches(&self, unlocked: bool, cx: &mut App) {
        let mut visible = Vec::new();
        if unlocked {
            for tab in self.tabs() {
                if self.surface_is_visible(tab.id)
                    && let Some(root) = &tab.root_pane
                {
                    if let Some(pane) = self.zoomed_pane(tab.id) {
                        visible.push(pane);
                    } else {
                        root.collect_pane_ids(&mut visible);
                    }
                }
            }
        }
        // Zoom hides sibling rendering without releasing their terminal sessions.
        for (id, pane) in &self.panes {
            let active = visible.contains(id);
            if pane.read(cx).render_cache_visible() != active {
                pane.update(cx, |pane, cx| pane.set_render_cache_visible(active, cx));
            }
        }
    }

    pub(in crate::workspace) fn terminal_session_id_for_pane(
        &self,
        pane_id: PaneId,
    ) -> Option<TerminalSessionId> {
        self.terminal_locations
            .iter()
            .find_map(|(session_id, location)| (location.pane_id == pane_id).then_some(*session_id))
    }

    pub(in crate::workspace) fn unbind_terminal_location_for_pane(
        &mut self,
        pane_id: PaneId,
    ) -> Option<TerminalSessionId> {
        let session_id = self
            .terminal_locations
            .iter()
            .find_map(|(session_id, location)| {
                (location.pane_id == pane_id).then_some(*session_id)
            })?;
        self.terminal_locations.remove(&session_id);
        Some(session_id)
    }

    pub(in crate::workspace) fn terminal_location(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<TerminalLocation> {
        self.terminal_locations.get(&session_id).copied()
    }

    fn mark_terminal_bell(&mut self, session: TerminalSessionId) -> Option<TabId> {
        let tab = self.terminal_location(session)?.tab_id;
        self.layout.mark_bell(tab)
    }
    fn mark_terminal_output_unread(&mut self, session: TerminalSessionId) -> Option<TabId> {
        let tab = self.terminal_location(session)?.tab_id;
        self.layout.mark_unread(tab)
    }
    pub(in crate::workspace) fn begin_detach_from_main(
        &mut self,
        tab_id: TabId,
    ) -> Option<TabDetachTransition> {
        let tab_index = self.tab_index_by_id(tab_id)?;
        let previous = self.active_tab_id();
        let mount_id = self.begin_detach(tab_id)?;
        self.clear_highlights(tab_id);
        let current = if previous == Some(tab_id) {
            self.nearest_main_tab(tab_index)
        } else {
            previous
        };
        self.select_main_tab(current);
        Some(TabDetachTransition {
            mount_id,
            selection: MainTabSelectionChange { previous, current },
        })
    }

    pub(in crate::workspace) fn commit_detach(
        &mut self,
        tab_id: TabId,
        mount_id: TabMountId,
        window_handle: AnyWindowHandle,
    ) -> bool {
        if !self.layout.commit_mount(
            tab_id,
            mount_id,
            TabMount::Detached {
                mount_id,
                window_id: window_handle.window_id(),
                handle: window_handle,
            },
        ) {
            return false;
        }
        for location in self
            .terminal_locations
            .values()
            .filter(|location| location.tab_id == tab_id)
        {
            if let Some(affinity) = self.pane_window_affinities.get_mut(&location.pane_id) {
                affinity.current = window_handle;
            }
        }
        true
    }

    pub(in crate::workspace) fn rollback_detach_to_main(
        &mut self,
        tab_id: TabId,
        mount_id: TabMountId,
    ) -> Option<MainTabSelectionChange> {
        if !self.rollback_detach(tab_id, mount_id) {
            return None;
        }
        let previous = self.active_tab_id();
        let current = self.tab_by_id(tab_id).map(|tab| tab.id);
        self.select_main_tab(current);
        Some(MainTabSelectionChange { previous, current })
    }

    pub(in crate::workspace) fn return_to_main(
        &mut self,
        tab_id: TabId,
        reason: TabMountCloseReason,
    ) -> Option<TabMountCleanupPlan> {
        let TabMount::Detached { handle, .. } = self.layout.return_mount(tab_id)?;
        self.restore_tab_panes_to_home_window(tab_id);
        Some(TabMountCleanupPlan {
            tab_id,
            reason,
            detached_window: Some(handle),
        })
    }

    pub(in crate::workspace) fn return_to_main_and_select(
        &mut self,
        tab_id: TabId,
        reason: TabMountCloseReason,
    ) -> Option<TabReturnTransition> {
        let cleanup = self.return_to_main(tab_id, reason)?;
        let previous = self.active_tab_id();
        let current = self.tab_by_id(tab_id).map(|tab| tab.id);
        self.select_main_tab(current);
        Some(TabReturnTransition {
            cleanup,
            selection: MainTabSelectionChange { previous, current },
        })
    }

    pub(in crate::workspace) fn remove_tab_for_detached_window_release(
        &mut self,
        tab_id: TabId,
        mount_id: TabMountId,
        window_id: gpui::WindowId,
    ) -> Option<TabRemovalTransition> {
        let tab_index = match self.layout.detached_mount(tab_id).copied() {
            Some(TabMount::Detached {
                mount_id: current_mount_id,
                window_id: current_window_id,
                ..
            }) if current_mount_id == mount_id && current_window_id == window_id => {
                self.tab_index_by_id(tab_id)?
            }
            _ => return None,
        };
        let mut transition = self.remove_tab_at(tab_index)?;
        // The native window is already releasing, so final tab cleanup must not
        // re-enter its handle. Pane and terminal cleanup still follows normally.
        transition.mount_cleanup.detached_window = None;
        Some(transition)
    }

    pub(in crate::workspace) fn detached_close_dialog(&self) -> Option<TabId> {
        match self.close_confirm.as_ref()? {
            TabCloseConfirm::SshPane { tab_id, .. }
            | TabCloseConfirm::LocalChildPane { tab_id, .. }
                if self.is_detached(*tab_id) =>
            {
                Some(*tab_id)
            }
            _ => None,
        }
    }

    fn restore_tab_panes_to_home_window(&mut self, tab_id: TabId) {
        for location in self
            .terminal_locations
            .values()
            .filter(|location| location.tab_id == tab_id)
        {
            if let Some(affinity) = self.pane_window_affinities.get_mut(&location.pane_id) {
                affinity.current = affinity.home;
            }
        }
    }

    #[cfg(test)]
    fn mount(&self, tab_id: TabId) -> Option<TabMount> {
        self.layout.detached_mount(tab_id).copied()
    }

    pub(in crate::workspace) fn detached_window_handle(
        &self,
        tab_id: TabId,
    ) -> Option<AnyWindowHandle> {
        let tab_id = self.container_tab_id(tab_id);
        match self.layout.detached_mount(tab_id).copied() {
            Some(TabMount::Detached { handle, .. }) => Some(handle),
            _ => None,
        }
    }

    pub(in crate::workspace) fn start_close_process_check(
        &mut self,
        request: LocalTerminalCloseCheck,
        probes: Vec<TabCloseProcessProbe>,
        cx: &mut Context<Self>,
    ) {
        let probe_task = cx.background_executor().spawn(async move {
            // Each probe owns its duplicated PTY descriptor, so no terminal mutex is held while
            // platform process and cwd commands run on the background executor.
            probes
                .into_iter()
                .map(|probe| {
                    let info = probe
                        .probe
                        .map(|probe| probe.collect_foreground_only())
                        .unwrap_or(probe.cached);
                    (probe.pane_id, info)
                })
                .collect::<Vec<_>>()
        });

        self.start_close_process_check_with_future(request, probe_task, cx);
    }

    fn start_close_process_check_with_future(
        &mut self,
        request: LocalTerminalCloseCheck,
        probe_task: impl std::future::Future<
            Output = Vec<(PaneId, oxideterm_terminal::TerminalProcessInfo)>,
        > + 'static,
        cx: &mut Context<Self>,
    ) {
        self.process_close_check_generation = self.process_close_check_generation.wrapping_add(1);
        // A newer user request invalidates a completion that has not reached the window adapter.
        self.process_close_completion = None;
        let generation = self.process_close_check_generation;
        // The Entity is the sole task owner. Replacing this handle cancels the
        // older check, and releasing the Entity cancels the current check.
        self.process_close_check_task = Some(cx.spawn(async move |entity, cx| {
            let results = probe_task.await;
            let _ = entity.update(cx, |entity, cx| {
                if entity.process_close_check_generation != generation {
                    return;
                }
                let has_foreground_child = results
                    .iter()
                    .any(|(_, info)| terminal_process_info_has_foreground_child_process(info));
                entity.process_close_completion = Some(TabCloseProcessCompletion {
                    request,
                    results,
                    has_foreground_child,
                });
                cx.emit(WorkspaceTabHostEvent::CloseProcessCheckReady);
            });
        }));
    }

    pub(in crate::workspace) fn take_close_process_completion(
        &mut self,
    ) -> Option<TabCloseProcessCompletion> {
        self.process_close_completion.take()
    }

    pub(in crate::workspace) fn open_close_confirm(
        &mut self,
        confirm: TabCloseConfirm,
        cx: &mut Context<Self>,
    ) {
        // A replacement confirmation owns a fresh generation and cancels the
        // retained exit task for the previous payload.
        self.close_confirm_exit_task = None;
        self.close_confirm = Some(confirm);
        self.close_confirm_presence.reopen();
        self.close_confirm_focused_action = None;
        cx.notify();
    }

    pub(in crate::workspace) fn close_confirm(&self) -> Option<&TabCloseConfirm> {
        self.close_confirm.as_ref()
    }

    pub(in crate::workspace) fn close_confirm_phase(
        &self,
    ) -> Option<oxideterm_gpui_ui::motion::ExitPhase> {
        self.close_confirm
            .as_ref()
            .map(|_| self.close_confirm_presence.phase())
    }

    pub(in crate::workspace) fn close_confirm_snapshot(&self) -> Option<TabCloseConfirmSnapshot> {
        self.close_confirm
            .as_ref()
            .cloned()
            .map(|confirm| TabCloseConfirmSnapshot {
                confirm,
                phase: self.close_confirm_presence.phase(),
                focused_action: self.close_confirm_focused_action,
            })
    }

    pub(in crate::workspace) fn handle_close_confirm_key(
        &mut self,
        key: &str,
        shift: bool,
        blocked_by_primary_modifier: bool,
        cx: &mut Context<Self>,
    ) -> Option<TabCloseConfirmKeyAction> {
        if blocked_by_primary_modifier
            || self.close_confirm.is_none()
            || self.close_confirm_presence.phase() != oxideterm_gpui_ui::motion::ExitPhase::Visible
        {
            return None;
        }
        if key == "escape" {
            self.close_confirm_focused_action = None;
            return Some(TabCloseConfirmKeyAction::Cancel);
        }
        if key == "enter" {
            self.close_confirm_focused_action = None;
            return Some(TabCloseConfirmKeyAction::Confirm);
        }
        const ACTIONS: [ConfirmDialogAction; 2] =
            [ConfirmDialogAction::Cancel, ConfirmDialogAction::Confirm];
        match browser_behavior::modal_footer_key_action(
            key,
            shift,
            &ACTIONS,
            self.close_confirm_focused_action,
            ConfirmDialogAction::Cancel,
        ) {
            Some(browser_behavior::ModalFooterKeyAction::Cancel) => {
                self.close_confirm_focused_action = None;
                Some(TabCloseConfirmKeyAction::Cancel)
            }
            Some(browser_behavior::ModalFooterKeyAction::Focus(action)) => {
                self.close_confirm_focused_action = Some(action);
                cx.notify();
                Some(TabCloseConfirmKeyAction::Handled)
            }
            Some(browser_behavior::ModalFooterKeyAction::Activate(action)) => {
                self.close_confirm_focused_action = None;
                Some(match action {
                    ConfirmDialogAction::Cancel => TabCloseConfirmKeyAction::Cancel,
                    ConfirmDialogAction::Confirm => TabCloseConfirmKeyAction::Confirm,
                })
            }
            None => None,
        }
    }

    pub(in crate::workspace) fn begin_close_confirm_exit(
        &mut self,
        confirmed: bool,
        delay: Duration,
        cx: &mut Context<Self>,
    ) -> (bool, Option<TabCloseConfirm>) {
        let Some(confirm) = self.close_confirm.as_ref() else {
            return (false, None);
        };
        let Some(generation) = self.close_confirm_presence.begin_exit() else {
            return (false, None);
        };
        self.close_confirm_focused_action = None;
        let effect = confirmed.then(|| confirm.clone());
        self.close_confirm_exit_task = None;
        if delay.is_zero() {
            self.finish_close_confirm_exit(generation, cx);
            return (true, effect);
        }
        self.close_confirm_exit_task = Some(cx.spawn(async move |tab_host, cx| {
            Timer::after(delay).await;
            let _ = tab_host.update(cx, |tab_host, cx| {
                tab_host.finish_close_confirm_exit(generation, cx);
            });
        }));
        cx.notify();
        (true, effect)
    }

    fn finish_close_confirm_exit(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.close_confirm_exit_task = None;
        if self.close_confirm.is_some() && self.close_confirm_presence.finish_exit(generation) {
            self.close_confirm = None;
            self.close_confirm_presence.reopen();
            self.close_confirm_focused_action = None;
            cx.notify();
        }
    }
}

fn terminal_process_info_has_foreground_child_process(
    info: &oxideterm_terminal::TerminalProcessInfo,
) -> bool {
    let Some(shell_pid) = info.shell_pid else {
        return false;
    };
    info.foreground_process_group_id
        .is_some_and(|foreground_group| foreground_group != shell_pid)
        || info
            .foreground_pid
            .is_some_and(|foreground_pid| foreground_pid != shell_pid)
}

impl gpui::EventEmitter<WorkspaceTabHostEvent> for WorkspaceTabHostEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, div};
    use oxideterm_gpui_shell::tabs::TabContent;

    struct TabHostTestRoot;

    impl Render for TabHostTestRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn test_tab(tab_id: TabId, root_pane: Option<PaneNode>) -> Tab {
        Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title: format!("tab-{}", tab_id.0),
            title_source: TabTitleSource::Static,
            active_pane_id: root_pane.as_ref().map(PaneNode::first_pane_id),
            root_pane,
        }
    }

    struct TabHostEventRecorder {
        events: Vec<WorkspaceTabHostEvent>,
        _subscription: Option<Subscription>,
    }

    #[gpui::test]
    fn tiling_live_terminals_preserves_content_and_supports_zoom_and_extraction(
        cx: &mut TestAppContext,
    ) {
        let main = cx.add_window(|_, _| TabHostTestRoot);
        let panes = main
            .update(cx, |_, window, cx| {
                (0..17)
                    .map(|index| {
                        cx.new(|cx| {
                            let mut pane = TerminalPane::new_recording_playback(
                                80,
                                24,
                                Default::default(),
                                window,
                                cx,
                            )
                            .unwrap();
                            pane.feed_recording_output(format!("server-{index}").as_bytes(), cx);
                            pane
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap();
        let host = cx.new(|_| WorkspaceTabHostEntity::new());
        host.update(cx, |host, cx| {
            let pane_ids = (0..17).map(|_| host.alloc_pane_id()).collect::<Vec<_>>();
            let sessions = (0..17).map(|_| host.alloc_session_id()).collect::<Vec<_>>();
            let tabs = (0..17).map(|_| host.alloc_tab_id()).collect::<Vec<_>>();
            for index in 0..17 {
                if index == 10 {
                    continue;
                }
                host.insert_tab(test_tab(
                    tabs[index],
                    Some(PaneNode::leaf(pane_ids[index], sessions[index])),
                ));
            }
            let group = host.alloc_pane_id();
            assert!(host.split_pane(
                tabs[0],
                pane_ids[0],
                group,
                SplitDirection::Horizontal,
                pane_ids[10],
                sessions[10]
            ));
            for index in 0..17 {
                host.register_terminal_pane(
                    pane_ids[index],
                    sessions[index],
                    panes[index].clone(),
                    main.into(),
                    cx,
                );
                host.bind_terminal_location(
                    sessions[index],
                    TerminalLocation {
                        tab_id: if index == 10 { tabs[0] } else { tabs[index] },
                        pane_id: pane_ids[index],
                    },
                );
            }
            host.select_main_tab(Some(tabs[0]));
            host.set_active_pane(Some(tabs[0]), pane_ids[0]);
            let original_roots = host
                .tabs()
                .iter()
                .map(|tab| (tab.id, tab.root_pane.clone()))
                .collect::<Vec<_>>();
            for selection in [
                vec![pane_ids[0]],
                vec![pane_ids[0], pane_ids[0]],
                vec![pane_ids[0], PaneId(9999)],
                pane_ids.clone(),
            ] {
                assert!(
                    host.tile_terminal_panes(&selection, "Tiled".into(), true)
                        .is_none()
                );
                assert_eq!(
                    host.tabs()
                        .iter()
                        .map(|tab| (tab.id, tab.root_pane.clone()))
                        .collect::<Vec<_>>(),
                    original_roots
                );
            }
            let mount = host.begin_detach(tabs[9]).unwrap();
            assert!(
                host.tile_terminal_panes(&pane_ids[..10], "Tiled".into(), true)
                    .is_none()
            );
            assert_eq!(
                host.tabs()
                    .iter()
                    .map(|tab| (tab.id, tab.root_pane.clone()))
                    .collect::<Vec<_>>(),
                original_roots
            );
            assert!(host.rollback_detach(tabs[9], mount));

            let (tiled, removed) = host
                .tile_terminal_panes(&pane_ids[..10], "Tiled".into(), true)
                .unwrap();
            assert_eq!(removed, tabs[1..10]);
            assert_eq!(tiled.active_pane_id, Some(pane_ids[0]));
            assert_eq!(
                host.tab_by_id(tabs[0]).unwrap().root_pane,
                Some(PaneNode::leaf(pane_ids[10], sessions[10]))
            );
            let mut tiled_ids = Vec::new();
            tiled
                .root_pane
                .as_ref()
                .unwrap()
                .collect_pane_ids(&mut tiled_ids);
            assert_eq!(tiled_ids, pane_ids[..10]);
            for index in 0..17 {
                assert_eq!(
                    host.terminal_pane_for_target(pane_ids[index], sessions[index]),
                    Some(panes[index].clone())
                );
                assert_eq!(
                    panes[index].read(cx).ai_buffer_snapshot().trim(),
                    format!("server-{index}")
                );
                assert_eq!(
                    host.terminal_location(sessions[index]).unwrap().tab_id,
                    if index < 10 {
                        tiled.id
                    } else if index == 10 {
                        tabs[0]
                    } else {
                        tabs[index]
                    }
                );
            }

            assert!(host.toggle_pane_zoom(tiled.id, pane_ids[0]));
            host.sync_terminal_render_caches(true, cx);
            assert_eq!(
                panes
                    .iter()
                    .map(|pane| pane.read(cx).render_cache_visible())
                    .collect::<Vec<_>>(),
                (0..17).map(|index| index == 0).collect::<Vec<_>>()
            );
            assert_eq!(host.tab_by_id(tiled.id).unwrap().root_pane, tiled.root_pane);
            assert!(host.set_active_pane(Some(tiled.id), pane_ids[1]));
            assert_eq!(host.zoomed_pane(tiled.id), Some(pane_ids[1]));
            assert!(host.toggle_pane_zoom(tiled.id, pane_ids[1]));
            host.sync_terminal_render_caches(true, cx);
            assert_eq!(
                panes
                    .iter()
                    .map(|pane| pane.read(cx).render_cache_visible())
                    .collect::<Vec<_>>(),
                (0..17).map(|index| index < 10).collect::<Vec<_>>()
            );

            let extracted = host.alloc_tab_id();
            assert!(host.move_terminal_pane_to_tab(
                tiled.id,
                pane_ids[2],
                test_tab(extracted, Some(PaneNode::leaf(pane_ids[2], sessions[2]))),
                main.into()
            ));
            assert_eq!(
                host.terminal_location(sessions[2]),
                Some(TerminalLocation {
                    tab_id: extracted,
                    pane_id: pane_ids[2]
                })
            );
            assert_eq!(
                host.terminal_pane_for_target(pane_ids[2], sessions[2]),
                Some(panes[2].clone())
            );
            assert!(host.toggle_pane_zoom(tiled.id, pane_ids[3]));
            host.close_pane(tiled.id, pane_ids[3]).unwrap();
            host.remove_terminal_pane(pane_ids[3]);
            assert_eq!(host.zoomed_pane(tiled.id), None);
            let mut remaining = Vec::new();
            host.tab_by_id(tiled.id)
                .unwrap()
                .root_pane
                .as_ref()
                .unwrap()
                .collect_pane_ids(&mut remaining);
            assert_eq!(
                remaining,
                [0, 1, 4, 5, 6, 7, 8, 9].map(|index| pane_ids[index])
            );
            for index in [0, 1, 4, 5, 6, 7, 8, 9] {
                assert_eq!(
                    host.terminal_pane_for_target(pane_ids[index], sessions[index]),
                    Some(panes[index].clone())
                );
            }
        });
    }

    #[gpui::test]
    fn moved_terminal_keeps_ai_target_and_content_across_window_handoffs(cx: &mut TestAppContext) {
        let main = cx.add_window(|_, _| TabHostTestRoot);
        let detached = cx.add_window(|_, _| TabHostTestRoot);
        let panes = main
            .update(cx, |_, window, cx| {
                ["local-a", "remote-b"].map(|text| {
                    cx.new(|cx| {
                        let mut pane = TerminalPane::new_recording_playback(
                            80,
                            24,
                            Default::default(),
                            window,
                            cx,
                        )
                        .unwrap();
                        pane.feed_recording_output(text.as_bytes(), cx);
                        pane
                    })
                })
            })
            .unwrap();
        let host = cx.new(|_| WorkspaceTabHostEntity::new());
        let [a, b] = [PaneId(1), PaneId(2)];
        let [sa, sb] = [TerminalSessionId(1), TerminalSessionId(2)];
        let mut ai = crate::workspace::ai_runtime_context::AiRuntimeContextEntity::new();
        ai.register_terminal_session(sb, "remote-b".into());
        let tools = ai.begin_tool_session(1);
        let handle = ai.issue_terminal_handle(&tools, sb).unwrap();
        host.update(cx, |host, cx| {
            host.insert_and_select_main_tab(test_tab(TabId(1), Some(PaneNode::leaf(a, sa))));
            assert!(host.split_pane(TabId(1), a, PaneId(3), SplitDirection::Horizontal, b, sb));
            for ((id, session), pane) in [(a, sa), (b, sb)].into_iter().zip(&panes) {
                host.register_terminal_pane(id, session, pane.clone(), main.into(), cx);
                host.bind_terminal_location(
                    session,
                    TerminalLocation {
                        tab_id: TabId(1),
                        pane_id: id,
                    },
                );
            }
            host.sync_terminal_render_caches(true, cx);
            assert!(
                panes
                    .iter()
                    .all(|pane| pane.read(cx).render_cache_visible())
            );
            for session in [sa, sb] {
                let mut instance = crate::workspace::local_sessions::LocalTerminalInstance::new(
                    &LocalPtyConfig::default(),
                    "Project".into(),
                );
                instance.profile_id = Some("same-saved-profile".into());
                host.local_sessions.insert(session, instance);
            }
            assert!(host.move_terminal_pane_to_tab(
                TabId(1),
                b,
                test_tab(TabId(2), Some(PaneNode::leaf(b, sb))),
                main.into()
            ));
            assert_eq!(
                host.tab_by_id(TabId(1)).unwrap().root_pane,
                Some(PaneNode::leaf(a, sa))
            );
            assert_eq!(
                host.terminal_location(sb),
                Some(TerminalLocation {
                    tab_id: TabId(2),
                    pane_id: b
                })
            );
            // Selection changes cannot redirect an already-issued terminal handle.
            host.select_main_tab(Some(TabId(1)));
            host.sync_terminal_render_caches(true, cx);
            assert!(panes[0].read(cx).render_cache_visible());
            assert!(!panes[1].read(cx).render_cache_visible());
            let mount = host.begin_detach(TabId(2)).unwrap();
            assert!(host.commit_detach(TabId(2), mount, detached.into()));
            host.sync_terminal_render_caches(true, cx);
            assert!(
                panes
                    .iter()
                    .all(|pane| pane.read(cx).render_cache_visible())
            );
            assert_eq!(host.pane_window_affinities[&b].current, detached.into());
            assert_eq!(host.terminal_pane_for_target(b, sb), Some(panes[1].clone()));
            assert!(host.terminal_pane_for_target(a, sb).is_none());
            let target = host.terminal_pane_for_target(b, sb).unwrap();
            assert_eq!(target.read(cx).ai_buffer_snapshot().trim(), "remote-b");
            for capability in [
                oxideterm_ai::RuntimeCapability::TerminalObserve,
                oxideterm_ai::RuntimeCapability::TerminalRunCommand,
                oxideterm_ai::RuntimeCapability::TerminalSendInput,
            ] {
                assert_eq!(
                    ai.validate_terminal_handle(
                        &tools,
                        Some(handle.handle_id.as_str()),
                        capability
                    )
                    .unwrap(),
                    sb
                );
            }
            host.return_to_main(TabId(2), TabMountCloseReason::ReturnToMain)
                .unwrap();
            host.sync_terminal_render_caches(true, cx);
            assert!(!panes[1].read(cx).render_cache_visible());
            host.sync_terminal_render_caches(false, cx);
            assert!(
                panes
                    .iter()
                    .all(|pane| !pane.read(cx).render_cache_visible())
            );
            assert_eq!(host.pane_window_affinities[&b].current, main.into());
            assert_eq!(host.terminal_pane_for_target(b, sb), Some(panes[1].clone()));
            assert_eq!(
                host.local_sessions[&sb].profile_id.as_deref(),
                Some("same-saved-profile")
            );
            host.remove_terminal_pane(b);
            assert_eq!(host.terminal_location(sb), None);
            assert_eq!(
                host.terminal_location(sa),
                Some(TerminalLocation {
                    tab_id: TabId(1),
                    pane_id: a
                })
            );
            assert!(!host.local_sessions.contains_key(&sb));
            assert_eq!(
                host.local_sessions[&sa].profile_id.as_deref(),
                Some("same-saved-profile")
            );
            assert!(host.terminal_pane_for_target(b, sb).is_none());
            assert_eq!(host.terminal_pane_for_target(a, sa), Some(panes[0].clone()));
        });
    }

    #[test]
    fn clearing_tab_scope_preserves_the_current_page() {
        let mut host = WorkspaceTabHostEntity::new();
        for id in 1..=3 {
            host.insert_tab(test_tab(TabId(id), None));
        }
        let connection = TabAffinity::Ssh(NodeId::new("server"));
        host.update_affinities(HashMap::from([
            (TabId(1), vec![TabAffinity::Global]),
            (TabId(2), vec![connection.clone()]),
            (TabId(3), vec![connection.clone()]),
        ]));
        host.select_main_tab(Some(TabId(1)));
        host.switch_scope(TabScope::Affinity(connection));
        host.select_main_tab(Some(TabId(3)));
        host.switch_scope(TabScope::All);
        assert_eq!(host.active_tab_id(), Some(TabId(3)));
        assert_eq!(
            host.main_tabs().map(|(_, tab)| tab.id).collect::<Vec<_>>(),
            vec![TabId(1), TabId(2), TabId(3)]
        );
    }

    #[test]
    fn tab_scopes_remember_selection_and_keep_empty_scopes_empty() {
        let mut host = WorkspaceTabHostEntity::new();
        for id in 1..=4 {
            host.insert_tab(test_tab(TabId(id), None));
        }
        let first = TabAffinity::Ssh(NodeId::new("first"));
        let second = TabAffinity::Ssh(NodeId::new("second"));
        host.update_affinities(HashMap::from([
            (TabId(1), vec![first.clone()]),
            (TabId(2), vec![second]),
            (TabId(3), vec![first.clone(), TabAffinity::Global]),
            (TabId(4), vec![TabAffinity::Global]),
        ]));
        host.select_main_tab(Some(TabId(2)));
        host.switch_scope(TabScope::Affinity(first.clone()));
        assert_eq!(
            host.main_tabs().map(|(_, tab)| tab.id).collect::<Vec<_>>(),
            vec![TabId(1), TabId(3)]
        );
        host.select_main_tab(Some(TabId(3)));
        host.switch_scope(TabScope::Affinity(TabAffinity::Global));
        assert_eq!(
            host.main_tabs().map(|(_, tab)| tab.id).collect::<Vec<_>>(),
            vec![TabId(3), TabId(4)]
        );
        host.select_main_tab(Some(TabId(4)));
        host.switch_scope(TabScope::Affinity(first.clone()));
        assert_eq!(host.active_tab_id(), Some(TabId(3)));
        let index = host.tab_index_by_id(TabId(3)).unwrap();
        let removed = host.remove_tab_at(index).unwrap();
        assert_eq!(removed.next_active_tab_id, Some(TabId(1)));
        host.switch_scope(TabScope::Affinity(TabAffinity::LocalTerminal));
        assert_eq!(host.active_tab_id(), None);
        assert_eq!(
            host.scope(),
            &TabScope::Affinity(TabAffinity::LocalTerminal)
        );
        host.switch_scope(TabScope::All);
        assert_eq!(host.active_tab_id(), Some(TabId(2)));
        assert_eq!(
            host.tabs().iter().map(|tab| tab.id).collect::<Vec<_>>(),
            vec![TabId(1), TabId(2), TabId(4)]
        );
    }

    #[test]
    fn tab_scopes_reorder_only_visible_slots_and_exclude_detached_tabs() {
        let mut host = WorkspaceTabHostEntity::new();
        for id in 1..=6 {
            host.insert_tab(test_tab(TabId(id), None));
        }
        host.update_affinities(
            (1..=6)
                .map(|id| {
                    (
                        TabId(id),
                        vec![if id % 2 == 1 {
                            TabAffinity::LocalTerminal
                        } else {
                            TabAffinity::Global
                        }],
                    )
                })
                .collect(),
        );
        host.switch_scope(TabScope::Affinity(TabAffinity::LocalTerminal));
        host.select_main_tab(Some(TabId(1)));
        assert!(host.move_main_tab_to_visible_index(TabId(1), 2));
        assert_eq!(
            host.tabs().iter().map(|tab| tab.id).collect::<Vec<_>>(),
            vec![TabId(3), TabId(2), TabId(5), TabId(4), TabId(1), TabId(6)]
        );
        assert_eq!(host.active_tab_id(), Some(TabId(1)));
        let reservation = host.begin_detach(TabId(5)).unwrap();
        assert_eq!(host.other_main_tab_ids(TabId(1)), vec![TabId(3)]);
        assert!(host.rollback_detach(TabId(5), reservation));
        assert_eq!(host.other_main_tab_ids(TabId(1)), vec![TabId(3), TabId(5)]);
        host.switch_scope(TabScope::Affinity(TabAffinity::Global));
        assert_eq!(
            host.main_tabs().map(|(_, tab)| tab.id).collect::<Vec<_>>(),
            vec![TabId(2), TabId(4), TabId(6)]
        );
    }

    #[test]
    fn canonical_tabs_keep_selection_reorder_and_removal_atomic() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let first = TabId(1);
        let second = TabId(2);
        let third = TabId(3);

        // Collection changes and main-window selection share one write owner.
        assert_eq!(
            tab_host.insert_and_select_main_tab(test_tab(first, None)),
            None
        );
        assert_eq!(
            tab_host.insert_and_select_main_tab(test_tab(second, None)),
            Some(first)
        );
        tab_host.insert_tab(test_tab(third, None));
        assert_eq!(tab_host.active_tab_id(), Some(second));
        assert!(tab_host.move_main_tab_to_visible_index(third, 0));
        assert_eq!(
            tab_host.tabs().iter().map(|tab| tab.id).collect::<Vec<_>>(),
            vec![third, first, second]
        );

        let removed = tab_host
            .remove_tab_at(2)
            .expect("active tab removal transition");
        assert_eq!(removed.tab.id, second);
        assert_eq!(removed.previous_active_tab_id, Some(second));
        assert_eq!(removed.next_active_tab_id, Some(first));
        assert_eq!(removed.mount_cleanup.reason, TabMountCloseReason::TabClosed);
        assert_eq!(tab_host.active_tab_id(), Some(first));
        assert!(tab_host.tab_by_id(second).is_none());

        let pending = TabId(4);
        let last = TabId(5);
        tab_host.insert_tab(test_tab(pending, None));
        tab_host.insert_tab(test_tab(last, None));
        let mount = tab_host.begin_detach(pending).unwrap();
        assert_eq!(tab_host.other_main_tab_ids(first), vec![third, last]);
        assert!(!tab_host.move_main_tab_to_visible_index(pending, 0));
        assert!(tab_host.move_main_tab_to_visible_index(last, 0));
        assert_eq!(
            tab_host
                .main_tabs()
                .map(|(_, tab)| tab.id)
                .collect::<Vec<_>>(),
            vec![last, third, first]
        );
        tab_host.select_main_tab(Some(first));
        let removed = tab_host
            .remove_tab_at(tab_host.tab_index_by_id(first).unwrap())
            .unwrap();
        assert_eq!(removed.next_active_tab_id, Some(third));
        assert!(tab_host.rollback_detach(pending, mount));
        assert_eq!(
            tab_host
                .main_tabs()
                .map(|(_, tab)| tab.id)
                .collect::<Vec<_>>(),
            vec![last, third, pending]
        );
    }

    #[test]
    fn merging_terminal_tabs_preserves_sessions_and_rebinds_locations() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let target_tab_id = tab_host.alloc_tab_id();
        let source_tab_id = tab_host.alloc_tab_id();
        let target_pane_id = tab_host.alloc_pane_id();
        let source_pane_id = tab_host.alloc_pane_id();
        let target_session_id = TerminalSessionId(1);
        let source_session_id = TerminalSessionId(2);
        tab_host.insert_and_select_main_tab(test_tab(
            target_tab_id,
            Some(PaneNode::leaf(target_pane_id, target_session_id)),
        ));
        let mut source = test_tab(
            source_tab_id,
            Some(PaneNode::leaf(source_pane_id, source_session_id)),
        );
        source.kind = TabKind::SshTerminal;
        tab_host.insert_tab(source);
        tab_host.bind_terminal_location(
            target_session_id,
            TerminalLocation {
                tab_id: target_tab_id,
                pane_id: target_pane_id,
            },
        );
        tab_host.bind_terminal_location(
            source_session_id,
            TerminalLocation {
                tab_id: source_tab_id,
                pane_id: source_pane_id,
            },
        );

        assert!(tab_host.can_combine_pages(source_tab_id, target_tab_id));
        let (combined, removed) = tab_host
            .combine_pages(source_tab_id, target_tab_id, SplitDirection::Horizontal)
            .unwrap();
        assert_eq!(removed, vec![source_tab_id, target_tab_id]);
        assert!(tab_host.tab_by_id(source_tab_id).is_none());
        let target = tab_host.tab_by_id(combined.id).expect("combined tab");
        assert_eq!(target.active_pane_id, Some(source_pane_id));
        assert_eq!(target.root_pane.as_ref().map(PaneNode::pane_count), Some(2));
        assert_eq!(
            tab_host.terminal_location(source_session_id),
            Some(TerminalLocation {
                tab_id: combined.id,
                pane_id: source_pane_id,
            })
        );
    }

    #[gpui::test]
    fn mixed_pages_keep_identity_focus_and_window_ownership_when_recombined(
        cx: &mut TestAppContext,
    ) {
        let detached = cx.add_window(|_, _| TabHostTestRoot);
        let mut host = WorkspaceTabHostEntity::new();
        let ids = [TabKind::Sftp, TabKind::Ide, TabKind::Forwards].map(|kind| {
            let id = host.alloc_tab_id();
            let mut tab = test_tab(id, None);
            tab.kind = kind;
            host.insert_tab(tab);
            id
        });
        let [sftp, ide, forwards] = ids;
        let mut ai = crate::workspace::ai_runtime_context::AiRuntimeContextEntity::new();
        let tools = ai.begin_tool_session(1);
        let handles = ids.map(|id| {
            ai.register_app_surface(id, format!("page-{}", id.0), None);
            ai.issue_app_surface_handle(&tools, id).unwrap()
        });
        let (first, _) = host
            .combine_pages(sftp, ide, SplitDirection::Horizontal)
            .unwrap();
        let (combined, removed) = host
            .combine_pages(forwards, first.id, SplitDirection::Vertical)
            .unwrap();
        assert_eq!(removed, vec![first.id]);
        let terminal = host.alloc_tab_id();
        let pane = host.alloc_pane_id();
        let session = host.alloc_session_id();
        host.insert_tab(test_tab(terminal, Some(PaneNode::leaf(pane, session))));
        host.bind_terminal_location(
            session,
            TerminalLocation {
                tab_id: terminal,
                pane_id: pane,
            },
        );
        let (combined, _) = host
            .combine_pages(terminal, combined.id, SplitDirection::Horizontal)
            .unwrap();
        assert_eq!(
            host.tab_contents(combined.id),
            vec![
                TabContent::Page(ide),
                TabContent::Page(sftp),
                TabContent::Page(forwards),
                TabContent::Terminal(session),
            ]
        );
        assert_eq!(host.tab_contents(sftp), vec![TabContent::Page(sftp)]);
        assert_eq!(
            host.main_tabs().map(|(_, tab)| tab.id).collect::<Vec<_>>(),
            vec![combined.id]
        );
        for id in ids {
            assert_eq!(host.container_tab_id(id), combined.id);
            assert!(host.surface_is_visible(id));
        }
        assert_eq!(host.tab_by_id(sftp).unwrap().kind, TabKind::Sftp);
        assert_eq!(host.tab_by_id(ide).unwrap().kind, TabKind::Ide);
        host.select_main_tab(Some(ide));
        assert_eq!(host.active_tab_id(), Some(combined.id));
        assert_eq!(host.focused_page_id(combined.id), ide);
        let mount = host.begin_detach(combined.id).unwrap();
        assert!(host.commit_detach(combined.id, mount, detached.into()));
        host.select_main_tab(None);
        assert_eq!(host.other_main_tab_ids(sftp), Vec::<TabId>::new());
        for (id, handle) in ids.into_iter().zip(&handles) {
            let target = ai
                .validate_app_surface_handle(&tools, Some(handle.handle_id.as_str()))
                .unwrap();
            assert_eq!(target, id);
            host.focus_content_page(target);
            assert_eq!(host.focused_page_id(combined.id), id);
            assert_eq!(host.detached_window_handle(target), Some(detached.into()));
            assert!(host.surface_is_visible(target));
        }
        host.return_to_main(combined.id, TabMountCloseReason::ReturnToMain)
            .unwrap();
        host.select_main_tab(Some(forwards));
        assert_eq!(host.focused_page_id(combined.id), forwards);
        assert_eq!(host.unembed_page(sftp), Some(combined.id));
        assert_eq!(host.container_tab_id(sftp), sftp);
        assert!(!host.is_outside_main_window(sftp));
        let removed = host
            .remove_tab_at(host.tab_index_by_id(ide).unwrap())
            .unwrap();
        assert_eq!(removed.tab.id, ide);
        let mut remaining = Vec::new();
        host.tab_by_id(combined.id)
            .unwrap()
            .root_pane
            .as_ref()
            .unwrap()
            .collect_page_ids(&mut remaining);
        assert_eq!(remaining, vec![forwards]);
        assert_eq!(host.focused_page_id(combined.id), forwards);
        assert_eq!(host.tab_by_id(sftp).unwrap().kind, TabKind::Sftp);
        assert_eq!(
            host.tab_contents(combined.id),
            vec![TabContent::Page(forwards), TabContent::Terminal(session)]
        );
        assert_eq!(host.other_main_tab_ids(combined.id), vec![sftp]);
        assert_eq!(
            host.terminal_location(session),
            Some(TerminalLocation {
                tab_id: combined.id,
                pane_id: pane
            })
        );
        assert_eq!(
            host.tab_by_id(combined.id)
                .unwrap()
                .root_pane
                .as_ref()
                .unwrap()
                .session_id_for_pane(pane),
            Some(session)
        );
    }

    #[test]
    fn utility_pages_keep_their_identity_and_title_when_groups_shrink() {
        for kind in [
            TabKind::Settings,
            TabKind::Knowledge,
            TabKind::CloudSync,
            TabKind::NotificationCenter,
            TabKind::SessionManager,
            TabKind::FileManager,
            TabKind::Graphics,
            TabKind::Runtime,
            TabKind::ConnectionPool,
            TabKind::Topology,
            TabKind::PluginManager,
            TabKind::RemoteDesktop,
            TabKind::Plugin {
                plugin_id: "example".into(),
                tab_id: "tools".into(),
            },
        ] {
            let mut host = WorkspaceTabHostEntity::new();
            let source = host.alloc_tab_id();
            let target = host.alloc_tab_id();
            let mut source_tab = test_tab(source, None);
            source_tab.kind = kind.clone();
            source_tab.title = "Remaining page".into();
            let mut target_tab = test_tab(target, None);
            target_tab.kind = TabKind::Knowledge;
            host.insert_tab(source_tab);
            host.insert_and_select_main_tab(target_tab);
            assert!(host.can_receive_tab_drop(source, target), "{kind:?}");
            let (combined, removed) = host
                .combine_pages(source, target, SplitDirection::Horizontal)
                .unwrap();
            assert_eq!(removed, Vec::<TabId>::new());
            let mut pages = Vec::new();
            combined
                .root_pane
                .as_ref()
                .unwrap()
                .collect_page_ids(&mut pages);
            assert_eq!(pages, vec![target, source]);
            assert_eq!(host.tab_by_id(source).unwrap().kind, kind);
            host.select_main_tab(Some(source));
            assert_eq!(host.active_tab_id(), Some(combined.id));
            assert_eq!(host.focused_page_id(combined.id), source);
            assert_eq!(host.unembed_page(source), Some(combined.id));
            assert_eq!(host.container_tab_id(source), source);
            assert_eq!(host.focused_page_id(combined.id), target);
            assert_eq!(host.tab_by_id(source).unwrap().kind, kind);

            let (combined, _) = host
                .combine_pages(source, combined.id, SplitDirection::Vertical)
                .unwrap();
            host.remove_tab_at(host.tab_index_by_id(target).unwrap())
                .unwrap();
            let remaining = host.tab_by_id(combined.id).unwrap();
            assert!(
                matches!(remaining.root_pane, Some(PaneNode::Page { tab_id, .. }) if tab_id == source)
            );
            assert_eq!(remaining.title, "Remaining page");
            assert_eq!(host.focused_page_id(combined.id), source);
            assert_eq!(host.tab_by_id(source).unwrap().kind, kind);
        }
    }

    #[gpui::test]
    fn restore_single_terminal_workspace_tab_updates_only_collapsed_terminal_workspaces(
        cx: &mut TestAppContext,
    ) {
        let main = cx.add_window(|_, _| TabHostTestRoot);
        let mut host = WorkspaceTabHostEntity::new();
        let ciallo_tab = host.alloc_tab_id();
        let dero_tab = host.alloc_tab_id();
        let ciallo_pane = host.alloc_pane_id();
        let dero_pane = host.alloc_pane_id();
        let ciallo_session = host.alloc_session_id();
        let dero_session = host.alloc_session_id();

        host.insert_and_select_main_tab(Tab {
            id: ciallo_tab,
            kind: TabKind::SshTerminal,
            title: "Ciallo | DE | 1C1G".into(),
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(ciallo_pane, ciallo_session)),
            active_pane_id: Some(ciallo_pane),
        });
        host.insert_and_select_main_tab(Tab {
            id: dero_tab,
            kind: TabKind::SshTerminal,
            title: "DERO | DE | 4C12G".into(),
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(dero_pane, dero_session)),
            active_pane_id: Some(dero_pane),
        });

        let (combined, _) = host
            .combine_pages(dero_tab, ciallo_tab, SplitDirection::Horizontal)
            .unwrap();
        assert_eq!(combined.kind, TabKind::Workspace);
        assert_eq!(combined.title, "Ciallo | DE | 1C1G");

        // Multi-pane workspaces stay Workspace containers and keep their title.
        assert!(
            host.restore_single_terminal_workspace_tab(
                combined.id,
                TabKind::SshTerminal,
                "DERO | DE | 4C12G".into(),
            )
            .is_none()
        );
        assert_eq!(
            host.tab_by_id(combined.id).unwrap().kind,
            TabKind::Workspace
        );
        assert_eq!(
            host.tab_by_id(combined.id).unwrap().title,
            "Ciallo | DE | 1C1G"
        );

        let detached_tab = host.alloc_tab_id();
        assert!(host.move_terminal_pane_to_tab(
            combined.id,
            ciallo_pane,
            Tab {
                id: detached_tab,
                kind: TabKind::SshTerminal,
                title: "Ciallo | DE | 1C1G".into(),
                title_source: TabTitleSource::Static,
                root_pane: Some(PaneNode::leaf(ciallo_pane, ciallo_session)),
                active_pane_id: Some(ciallo_pane),
            },
            main.into(),
        ));

        let restored = host
            .restore_single_terminal_workspace_tab(
                combined.id,
                TabKind::SshTerminal,
                "DERO | DE | 4C12G".into(),
            )
            .unwrap();
        assert_eq!(restored.kind, TabKind::SshTerminal);
        assert_eq!(restored.title, "DERO | DE | 4C12G");

        // Regular non-Workspace terminal tabs are not overwritten.
        assert!(
            host.restore_single_terminal_workspace_tab(
                combined.id,
                TabKind::LocalTerminal,
                "Overwritten".into(),
            )
            .is_none()
        );
        assert_eq!(
            host.tab_by_id(combined.id).unwrap().kind,
            TabKind::SshTerminal
        );
        assert_eq!(
            host.tab_by_id(combined.id).unwrap().title,
            "DERO | DE | 4C12G"
        );

        // Workspace tabs wrapping an embedded page keep Workspace container semantics.
        let sftp_tab = host.alloc_tab_id();
        let settings_tab = host.alloc_tab_id();
        host.insert_and_select_main_tab(Tab {
            id: sftp_tab,
            kind: TabKind::Sftp,
            title: "SFTP".into(),
            title_source: TabTitleSource::Static,
            root_pane: None,
            active_pane_id: None,
        });
        host.insert_and_select_main_tab(Tab {
            id: settings_tab,
            kind: TabKind::Settings,
            title: "Settings".into(),
            title_source: TabTitleSource::Static,
            root_pane: None,
            active_pane_id: None,
        });
        let (page_workspace, _) = host
            .combine_pages(settings_tab, sftp_tab, SplitDirection::Horizontal)
            .unwrap();
        assert_eq!(host.unembed_page(settings_tab), Some(page_workspace.id));
        assert!(
            host.restore_single_terminal_workspace_tab(
                page_workspace.id,
                TabKind::LocalTerminal,
                "Wrong".into(),
            )
            .is_none()
        );
        assert_eq!(
            host.tab_by_id(page_workspace.id).unwrap().kind,
            TabKind::Workspace
        );
        assert_eq!(host.tab_by_id(page_workspace.id).unwrap().title, "SFTP");
    }

    #[gpui::test]
    fn detached_page_drop_uses_the_destination_leaf_and_preserves_owners(cx: &mut TestAppContext) {
        let window = cx.add_window(|_, _| TabHostTestRoot);
        let mut host = WorkspaceTabHostEntity::new();
        let target = host.alloc_tab_id();
        let source = host.alloc_tab_id();
        let a = host.alloc_pane_id();
        let b = host.alloc_pane_id();
        let group = host.alloc_pane_id();
        host.insert_and_select_main_tab(test_tab(
            target,
            Some(PaneNode::leaf(a, TerminalSessionId(1))),
        ));
        host.split_pane(
            target,
            a,
            group,
            SplitDirection::Horizontal,
            b,
            TerminalSessionId(2),
        );
        host.set_active_pane(Some(target), a);
        for (session_id, pane_id) in [(TerminalSessionId(1), a), (TerminalSessionId(2), b)] {
            host.bind_terminal_location(
                session_id,
                TerminalLocation {
                    tab_id: target,
                    pane_id,
                },
            );
        }
        let mut page = test_tab(source, None);
        page.kind = TabKind::Sftp;
        host.insert_tab(page);
        let mut ai = crate::workspace::ai_runtime_context::AiRuntimeContextEntity::new();
        ai.register_app_surface(source, "Files".into(), None);
        let tools = ai.begin_tool_session(1);
        let handle = ai.issue_app_surface_handle(&tools, source).unwrap();
        let mount = host.begin_detach(source).unwrap();
        assert!(!host.can_receive_tab_drop(source, target));
        host.commit_detach(source, mount, window.into());
        assert!(host.can_receive_tab_drop(source, target));
        assert!(!host.can_combine_pages(source, target));
        host.return_to_main(source, TabMountCloseReason::ReturnToMain)
            .unwrap();
        assert!(
            host.combine_pages_at(
                source,
                target,
                Some(PaneId(999)),
                SplitDirection::Vertical,
                true
            )
            .is_none()
        );
        assert_eq!(host.container_tab_id(source), source);
        assert_eq!(
            host.terminal_location(TerminalSessionId(2)),
            Some(TerminalLocation {
                tab_id: target,
                pane_id: b
            })
        );
        let (combined, _) = host
            .combine_pages_at(source, target, Some(b), SplitDirection::Vertical, true)
            .unwrap();
        let root = host
            .tab_by_id(combined.id)
            .unwrap()
            .root_pane
            .as_ref()
            .unwrap();
        let source_pane = root.page_pane(source).unwrap();
        let mut panes = Vec::new();
        root.collect_pane_ids(&mut panes);
        assert_eq!(panes, vec![a, source_pane, b]);
        assert_eq!(
            host.terminal_location(TerminalSessionId(1)),
            Some(TerminalLocation {
                tab_id: combined.id,
                pane_id: a
            })
        );
        assert_eq!(
            host.terminal_location(TerminalSessionId(2)),
            Some(TerminalLocation {
                tab_id: combined.id,
                pane_id: b
            })
        );
        assert_eq!(
            ai.validate_app_surface_handle(&tools, Some(handle.handle_id.as_str()))
                .unwrap(),
            source
        );
        assert_eq!(host.container_tab_id(source), combined.id);
        assert!(!host.can_receive_tab_drop(combined.id, combined.id));
        assert!(!host.can_receive_tab_drop(source, combined.id));
        let extra = host.alloc_tab_id();
        let c = host.alloc_pane_id();
        let d = host.alloc_pane_id();
        let extra_group = host.alloc_pane_id();
        host.insert_tab(test_tab(
            extra,
            Some(PaneNode::leaf(c, TerminalSessionId(3))),
        ));
        assert!(host.split_pane(
            extra,
            c,
            extra_group,
            SplitDirection::Horizontal,
            d,
            TerminalSessionId(4),
        ));
        // Exercise the configured boundary rather than assuming the former four-pane limit.
        for index in 2..MAX_PANES_PER_TAB - panes.len() {
            let pane = host.alloc_pane_id();
            let group = host.alloc_pane_id();
            assert!(host.split_pane(
                extra,
                c,
                group,
                SplitDirection::Horizontal,
                pane,
                TerminalSessionId(index as u64 + 3),
            ));
        }
        assert!(host.can_receive_tab_drop(extra, combined.id));
        let overflow = host.alloc_pane_id();
        let overflow_group = host.alloc_pane_id();
        assert!(host.split_pane(
            extra,
            c,
            overflow_group,
            SplitDirection::Horizontal,
            overflow,
            TerminalSessionId(MAX_PANES_PER_TAB as u64 + 1),
        ));
        let roots_before_drop =
            [extra, combined.id].map(|id| host.tab_by_id(id).unwrap().root_pane.clone());
        assert!(!host.can_receive_tab_drop(extra, combined.id));
        assert!(
            host.combine_pages_at(
                extra,
                combined.id,
                Some(a),
                SplitDirection::Horizontal,
                false
            )
            .is_none()
        );
        assert_eq!(host.active_tab_id(), Some(combined.id));
        assert_eq!(
            [extra, combined.id].map(|id| host.tab_by_id(id).unwrap().root_pane.clone()),
            roots_before_drop
        );
        assert_eq!(host.container_tab_id(source), combined.id);
        for (session, pane) in [(TerminalSessionId(1), a), (TerminalSessionId(2), b)] {
            assert_eq!(
                host.terminal_location(session),
                Some(TerminalLocation {
                    tab_id: combined.id,
                    pane_id: pane
                })
            );
        }
        host.close_pane(extra, overflow).unwrap();
        let (at_limit, _) = host
            .combine_pages_at(
                extra,
                combined.id,
                Some(a),
                SplitDirection::Horizontal,
                false,
            )
            .unwrap();
        assert_eq!(
            at_limit.root_pane.as_ref().unwrap().pane_count(),
            MAX_PANES_PER_TAB
        );
        assert_eq!(host.container_tab_id(source), at_limit.id);
        for (session, pane) in [(TerminalSessionId(1), a), (TerminalSessionId(2), b)] {
            assert_eq!(
                host.terminal_location(session),
                Some(TerminalLocation {
                    tab_id: at_limit.id,
                    pane_id: pane
                })
            );
        }
    }

    #[test]
    fn terminal_tab_rename_preserves_pane_and_session_ownership() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let tab_id = TabId(1);
        let pane_id = PaneId(2);
        let session_id = TerminalSessionId(3);
        let location = TerminalLocation { tab_id, pane_id };
        tab_host.insert_and_select_main_tab(test_tab(
            tab_id,
            Some(PaneNode::leaf(pane_id, session_id)),
        ));
        tab_host.bind_terminal_location(session_id, location);

        assert!(tab_host.rename_terminal_tab(tab_id, "  Production logs  "));
        let tab = tab_host.tab_by_id(tab_id).expect("renamed terminal tab");
        assert_eq!(tab.title, "Production logs");
        assert_eq!(tab.title_source, TabTitleSource::Static);
        assert_eq!(tab.active_pane_id, Some(pane_id));
        assert_eq!(
            tab.root_pane
                .as_ref()
                .and_then(|root| root.session_id_for_pane(pane_id)),
            Some(session_id)
        );
        assert_eq!(tab_host.terminal_location(session_id), Some(location));
    }

    #[test]
    fn background_terminal_output_stays_unread_until_the_tab_becomes_visible() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let active_tab_id = TabId(1);
        let background_tab_id = TabId(2);
        let background_session_id = TerminalSessionId(3);
        tab_host.insert_and_select_main_tab(test_tab(active_tab_id, None));
        tab_host.insert_tab(test_tab(background_tab_id, None));
        tab_host.bind_terminal_location(
            background_session_id,
            TerminalLocation {
                tab_id: background_tab_id,
                pane_id: PaneId(4),
            },
        );

        assert_eq!(
            tab_host.mark_terminal_output_unread(background_session_id),
            Some(background_tab_id)
        );
        assert_eq!(
            tab_host.mark_terminal_output_unread(background_session_id),
            None
        );
        assert!(
            tab_host
                .unread_terminal_output_tab_ids()
                .contains(&background_tab_id)
        );

        tab_host.select_main_tab(Some(background_tab_id));
        assert!(tab_host.unread_terminal_output_tab_ids().is_empty());
        assert_eq!(
            tab_host.mark_terminal_output_unread(background_session_id),
            None
        );

        tab_host.select_main_tab(Some(active_tab_id));
        tab_host.configure_terminal_output_highlight(false);
        assert_eq!(
            tab_host.mark_terminal_output_unread(background_session_id),
            None
        );
        assert!(tab_host.unread_terminal_output_tab_ids().is_empty());

        tab_host.configure_terminal_output_highlight(true);
        assert_eq!(
            tab_host.mark_terminal_output_unread(background_session_id),
            Some(background_tab_id)
        );
        tab_host
            .begin_detach_from_main(background_tab_id)
            .expect("background tab detach transition");
        assert!(tab_host.unread_terminal_output_tab_ids().is_empty());
    }

    #[gpui::test]
    fn pane_delivery_tracks_detach_reconnect_and_return_window(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_window, _cx| TabHostTestRoot);
        let first_pane = cx.update(|window, cx| {
            cx.new(|cx| {
                TerminalPane::new_recording_playback(
                    80,
                    24,
                    oxideterm_gpui_terminal::TerminalUiPreferences::default(),
                    window,
                    cx,
                )
                .expect("first recording pane")
            })
        });
        let replacement_pane = cx.update(|window, cx| {
            cx.new(|cx| {
                TerminalPane::new_recording_playback(
                    80,
                    24,
                    oxideterm_gpui_terminal::TerminalUiPreferences::default(),
                    window,
                    cx,
                )
                .expect("replacement recording pane")
            })
        });
        let main_window_handle = cx.window_handle();
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let event_recorder = cx.new(|_| TabHostEventRecorder {
            events: Vec::new(),
            _subscription: None,
        });
        event_recorder.update(cx, |event_recorder, cx| {
            event_recorder._subscription = Some(cx.subscribe(
                &tab_host,
                |event_recorder, _tab_host, event, _cx| {
                    event_recorder.events.push(event.clone());
                },
            ));
        });
        let tab_id = tab_host.update(cx, |tab_host, _cx| tab_host.alloc_tab_id());
        let first_pane_id = PaneId(4);
        let first_session_id = TerminalSessionId(5);
        let first_location = TerminalLocation {
            tab_id,
            pane_id: first_pane_id,
        };

        tab_host.update(cx, |tab_host, cx| {
            tab_host.register_terminal_pane(
                first_pane_id,
                first_session_id,
                first_pane.clone(),
                main_window_handle,
                cx,
            );
            tab_host.bind_terminal_location(first_session_id, first_location);
        });

        let (_, cx) = cx.add_window_view(|_window, _cx| TabHostTestRoot);
        let detached_window_handle = cx.window_handle();
        assert_ne!(main_window_handle, detached_window_handle);

        tab_host.update(cx, |tab_host, _cx| {
            let mount_id = tab_host.begin_detach(tab_id).expect("detach reservation");
            assert!(tab_host.commit_detach(tab_id, mount_id, detached_window_handle));
            assert_eq!(
                tab_host
                    .panes()
                    .get(&first_pane_id)
                    .expect("first pane remains registered")
                    .entity_id(),
                first_pane.entity_id()
            );
            assert_eq!(
                tab_host.terminal_location(first_session_id),
                Some(first_location)
            );
            assert_eq!(
                tab_host
                    .pane_window_affinities
                    .get(&first_pane_id)
                    .expect("first pane affinity")
                    .current,
                detached_window_handle
            );
        });
        first_pane.update(cx, |_pane, cx| {
            cx.emit(TerminalPaneEvent::ContextActionRequested);
        });
        cx.run_until_parked();

        tab_host.update(cx, |tab_host, _cx| {
            let cleanup = tab_host
                .return_to_main(tab_id, TabMountCloseReason::ReturnToMain)
                .expect("return cleanup");
            assert_eq!(cleanup.reason, TabMountCloseReason::ReturnToMain);
            assert_eq!(
                tab_host
                    .panes()
                    .get(&first_pane_id)
                    .expect("first pane remains registered")
                    .entity_id(),
                first_pane.entity_id()
            );
            assert_eq!(
                tab_host.terminal_location(first_session_id),
                Some(first_location)
            );
        });
        first_pane.update(cx, |_pane, cx| {
            cx.emit(TerminalPaneEvent::PrivilegePromptStateChanged);
        });
        cx.run_until_parked();

        let replacement_pane_id = PaneId(6);
        let replacement_session_id = TerminalSessionId(7);
        tab_host.update(cx, |tab_host, cx| {
            let mount_id = tab_host.begin_detach(tab_id).expect("replacement detach");
            assert!(tab_host.commit_detach(tab_id, mount_id, detached_window_handle));
            tab_host.register_terminal_pane(
                replacement_pane_id,
                replacement_session_id,
                replacement_pane.clone(),
                main_window_handle,
                cx,
            );
            assert!(tab_host.remove_terminal_pane(first_pane_id).is_some());
            assert!(!tab_host.pane_subscriptions.contains_key(&first_pane_id));
            assert!(!tab_host.pane_window_affinities.contains_key(&first_pane_id));
            assert_eq!(tab_host.terminal_location(first_session_id), None);
            tab_host.bind_terminal_location(
                replacement_session_id,
                TerminalLocation {
                    tab_id,
                    pane_id: replacement_pane_id,
                },
            );
            assert_eq!(
                tab_host
                    .pane_window_affinities
                    .get(&replacement_pane_id)
                    .expect("replacement pane affinity")
                    .current,
                detached_window_handle
            );
        });
        first_pane.update(cx, |_pane, cx| {
            cx.emit(TerminalPaneEvent::Exited { exit_code: Some(0) });
        });
        replacement_pane.update(cx, |_pane, cx| {
            cx.emit(TerminalPaneEvent::CurrentDirectoryChanged);
        });
        cx.run_until_parked();

        tab_host.update(cx, |tab_host, _cx| {
            assert!(
                tab_host
                    .return_to_main(tab_id, TabMountCloseReason::ReturnToMain)
                    .is_some()
            );
            assert_eq!(tab_host.mount(tab_id), None);
        });
        replacement_pane.update(cx, |_pane, cx| {
            cx.emit(TerminalPaneEvent::RecordingStatusChanged);
        });
        cx.run_until_parked();

        assert_eq!(
            event_recorder.read_with(cx, |event_recorder, _cx| event_recorder.events.clone()),
            vec![
                WorkspaceTabHostEvent::TerminalPaneDelivery {
                    pane_id: first_pane_id,
                    session_id: first_session_id,
                    window_handle: detached_window_handle,
                    event: TerminalPaneEvent::ContextActionRequested,
                },
                WorkspaceTabHostEvent::TerminalPaneDelivery {
                    pane_id: first_pane_id,
                    session_id: first_session_id,
                    window_handle: main_window_handle,
                    event: TerminalPaneEvent::PrivilegePromptStateChanged,
                },
                WorkspaceTabHostEvent::TerminalPaneDelivery {
                    pane_id: replacement_pane_id,
                    session_id: replacement_session_id,
                    window_handle: detached_window_handle,
                    event: TerminalPaneEvent::CurrentDirectoryChanged,
                },
                WorkspaceTabHostEvent::TerminalPaneDelivery {
                    pane_id: replacement_pane_id,
                    session_id: replacement_session_id,
                    window_handle: main_window_handle,
                    event: TerminalPaneEvent::RecordingStatusChanged,
                },
            ]
        );
        tab_host.update(cx, |tab_host, _cx| {
            assert!(tab_host.remove_terminal_pane(replacement_pane_id).is_some());
            assert!(tab_host.panes().is_empty());
            assert!(tab_host.pane_subscriptions.is_empty());
            assert!(tab_host.pane_window_affinities.is_empty());
            assert_eq!(tab_host.terminal_location(replacement_session_id), None);
        });
        replacement_pane.update(cx, |_pane, cx| {
            cx.emit(TerminalPaneEvent::ContextActionRequested);
        });
        cx.run_until_parked();
        assert_eq!(
            event_recorder.read_with(cx, |recorder, _cx| recorder.events.len()),
            4
        );
    }

    #[test]
    fn navigation_replays_prunes_closed_tabs_and_branches_only_on_new_selection() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let first = TabId(1);
        let second = TabId(2);
        let third = TabId(3);
        let existing = HashSet::from([first, second, third]);

        tab_host.observe_active_tab(Some(first));
        tab_host.observe_active_tab(Some(second));
        tab_host.observe_active_tab(Some(third));
        assert_eq!(tab_host.navigate_history(false, &existing), Some(second));
        tab_host.observe_active_tab(Some(second));
        assert_eq!(tab_host.navigate_history(false, &existing), Some(first));
        tab_host.observe_active_tab(Some(first));
        assert_eq!(tab_host.navigate_history(true, &existing), Some(second));
        tab_host.observe_active_tab(Some(second));
        assert_eq!(tab_host.navigate_history(true, &existing), Some(third));
        tab_host.observe_active_tab(Some(third));
        let replacement = TabId(4);
        assert_eq!(
            tab_host.navigate_history(false, &HashSet::from([first, third])),
            Some(first)
        );
        tab_host.observe_active_tab(Some(first));
        tab_host.observe_active_tab(Some(replacement));

        let existing = HashSet::from([first, third, replacement]);
        assert_eq!(tab_host.navigate_history(true, &existing), None);
        assert_eq!(tab_host.navigate_history(false, &existing), Some(first));
    }

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

    #[gpui::test]
    fn recording_elapsed_task_follows_only_the_visible_recording_pane(cx: &mut TestAppContext) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        tab_host.update(cx, |tab_host, cx| {
            tab_host.sync_recording_elapsed_tick(Some(PaneId(1)), true, cx);
            let first_generation = tab_host.recording_elapsed_generation;
            assert_eq!(tab_host.recording_elapsed_pane_id, Some(PaneId(1)));
            assert!(tab_host.recording_elapsed_task.is_some());

            tab_host.sync_recording_elapsed_tick(Some(PaneId(2)), true, cx);
            assert_ne!(tab_host.recording_elapsed_generation, first_generation);
            assert_eq!(tab_host.recording_elapsed_pane_id, Some(PaneId(2)));
            assert!(tab_host.recording_elapsed_task.is_some());

            tab_host.sync_recording_elapsed_tick(Some(PaneId(2)), false, cx);
            assert_eq!(tab_host.recording_elapsed_pane_id, None);
            assert!(tab_host.recording_elapsed_task.is_none());
        });
    }

    #[gpui::test]
    fn close_process_check_replacement_completion_and_release_preserve_delivery(
        cx: &mut TestAppContext,
    ) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let event_recorder = cx.new(|_| TabHostEventRecorder {
            events: Vec::new(),
            _subscription: None,
        });
        event_recorder.update(cx, |event_recorder, cx| {
            event_recorder._subscription = Some(cx.subscribe(
                &tab_host,
                |event_recorder, _tab_host, event, _cx| {
                    event_recorder.events.push(event.clone());
                },
            ));
        });
        let (first_sender, first_receiver) = tokio::sync::oneshot::channel();
        let (replacement_sender, replacement_receiver) = tokio::sync::oneshot::channel();
        tab_host.update(cx, |tab_host, cx| {
            tab_host.start_close_process_check_with_future(
                LocalTerminalCloseCheck::Single { tab_id: TabId(1) },
                async move {
                    first_receiver.await.expect("first result released");
                    Vec::new()
                },
                cx,
            );
            tab_host.start_close_process_check_with_future(
                LocalTerminalCloseCheck::Batch {
                    tab_ids: vec![TabId(2), TabId(3)],
                },
                async move {
                    replacement_receiver
                        .await
                        .expect("replacement result released");
                    Vec::new()
                },
                cx,
            );
        });
        cx.run_until_parked();
        assert!(
            first_sender.send(()).is_err(),
            "replacing the retained task must cancel the older future"
        );
        replacement_sender
            .send(())
            .expect("current task remains retained");
        cx.run_until_parked();
        tab_host.update(cx, |tab_host, _| {
            let completion = tab_host
                .take_close_process_completion()
                .expect("latest close process completion");
            assert_eq!(
                completion.request,
                LocalTerminalCloseCheck::Batch {
                    tab_ids: vec![TabId(2), TabId(3)]
                }
            );
            assert!(completion.results.is_empty());
            assert!(!completion.has_foreground_child);
            assert!(tab_host.take_close_process_completion().is_none());
        });
        assert_eq!(
            event_recorder.read_with(cx, |recorder, _| recorder.events.clone()),
            vec![WorkspaceTabHostEvent::CloseProcessCheckReady]
        );

        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
        tab_host.update(cx, |tab_host, cx| {
            tab_host.start_close_process_check_with_future(
                LocalTerminalCloseCheck::Single { tab_id: TabId(7) },
                async move {
                    result_receiver.await.expect("single result released");
                    Vec::new()
                },
                cx,
            );
        });
        result_sender.send(()).expect("current task retained");
        cx.run_until_parked();
        tab_host.update(cx, |tab_host, _| {
            let completion = tab_host
                .take_close_process_completion()
                .expect("single completion");
            assert_eq!(
                completion.request,
                LocalTerminalCloseCheck::Single { tab_id: TabId(7) }
            );
            assert!(tab_host.take_close_process_completion().is_none());
        });
        cx.run_until_parked();
        assert_eq!(
            event_recorder.read_with(cx, |recorder, _| recorder.events.clone()),
            vec![
                WorkspaceTabHostEvent::CloseProcessCheckReady,
                WorkspaceTabHostEvent::CloseProcessCheckReady,
            ]
        );

        let completion_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let completion_count_for_task = completion_count.clone();
        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
        tab_host.update(cx, |tab_host, cx| {
            tab_host.start_close_process_check_with_future(
                LocalTerminalCloseCheck::Single { tab_id: TabId(1) },
                async move {
                    result_receiver.await.expect("test result released");
                    completion_count_for_task.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Vec::new()
                },
                cx,
            );
        });
        drop(tab_host);
        cx.update(|_cx| {});
        cx.run_until_parked();
        assert!(
            result_sender.send(()).is_err(),
            "releasing the Entity must cancel its retained check"
        );
        assert_eq!(
            completion_count.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        assert_eq!(
            event_recorder.read_with(cx, |recorder, _| recorder.events.len()),
            2
        );
    }

    #[gpui::test]
    fn close_confirmation_reopen_cancels_stale_exit(cx: &mut TestAppContext) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let stale_confirm = TabCloseConfirm::Other {
            tab_ids: vec![TabId(2), TabId(3)],
        };
        let replacement_confirm = TabCloseConfirm::Single { tab_id: TabId(7) };

        tab_host.update(cx, |tab_host, cx| {
            tab_host.open_close_confirm(stale_confirm, cx);
            assert_eq!(
                tab_host.begin_close_confirm_exit(false, Duration::from_secs(60), cx),
                (true, None)
            );
            assert!(tab_host.close_confirm_exit_task.is_some());

            tab_host.open_close_confirm(replacement_confirm.clone(), cx);
            assert!(tab_host.close_confirm_exit_task.is_none());
            assert_eq!(
                tab_host.close_confirm_snapshot(),
                Some(TabCloseConfirmSnapshot {
                    confirm: replacement_confirm,
                    phase: oxideterm_gpui_ui::motion::ExitPhase::Visible,
                    focused_action: None,
                })
            );
        });
    }

    #[gpui::test]
    fn close_confirmation_keys_publish_the_payload_at_most_once(cx: &mut TestAppContext) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let confirm = TabCloseConfirm::LocalChildProcessBatch {
            tab_ids: vec![TabId(4), TabId(5)],
        };

        tab_host.update(cx, |tab_host, cx| {
            tab_host.open_close_confirm(confirm.clone(), cx);
            assert_eq!(
                tab_host.handle_close_confirm_key("escape", false, false, cx),
                Some(TabCloseConfirmKeyAction::Cancel)
            );
            assert_eq!(
                tab_host.begin_close_confirm_exit(false, Duration::ZERO, cx),
                (true, None)
            );

            tab_host.open_close_confirm(confirm.clone(), cx);
            assert_eq!(
                tab_host.handle_close_confirm_key("enter", false, false, cx),
                Some(TabCloseConfirmKeyAction::Confirm)
            );
            assert_eq!(
                tab_host.begin_close_confirm_exit(true, Duration::ZERO, cx),
                (true, Some(confirm))
            );
            assert_eq!(
                tab_host.begin_close_confirm_exit(true, Duration::ZERO, cx),
                (false, None)
            );
        });
    }

    #[test]
    fn failed_detach_rolls_back_only_the_current_reservation() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let tab_id = tab_host.alloc_tab_id();
        let mount_id = tab_host.begin_detach(tab_id).expect("detach reservation");

        assert!(tab_host.is_outside_main_window(tab_id));
        assert!(tab_host.rollback_detach(tab_id, mount_id));
        assert_eq!(tab_host.mount(tab_id), None);
        assert!(!tab_host.is_outside_main_window(tab_id));
        let replacement_mount_id = tab_host
            .begin_detach(tab_id)
            .expect("replacement reservation");
        assert!(!tab_host.rollback_detach(tab_id, mount_id));
        assert!(tab_host.is_outside_main_window(tab_id));
        assert!(tab_host.rollback_detach(tab_id, replacement_mount_id));
    }

    struct ClosingTabWindow {
        _close: Subscription,
    }

    impl Render for ClosingTabWindow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[gpui::test]
    fn detached_window_release_closes_only_its_current_tab_mount(cx: &mut TestAppContext) {
        for kind in [
            TabKind::LocalTerminal,
            TabKind::Settings,
            TabKind::Runtime,
            TabKind::PluginManager,
        ] {
            let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
            let tab_id = tab_host.update(cx, |tabs, _| {
                let id = tabs.alloc_tab_id();
                let mut tab = test_tab(id, None);
                tab.kind = kind.clone();
                tabs.insert_tab(tab);
                id
            });
            let open = |cx: &mut TestAppContext| {
                let mount = tab_host.update(cx, |tabs, _| tabs.begin_detach(tab_id).unwrap());
                let owner = tab_host.clone();
                let handle: AnyWindowHandle = cx
                    .add_window(move |window, cx| ClosingTabWindow {
                        _close: window_shell::observe_window_close(window, cx, move |id, cx| {
                            owner.update(cx, |tabs, _| {
                                if let Some(transition) =
                                    tabs.remove_tab_for_detached_window_release(tab_id, mount, id)
                                {
                                    assert_eq!(transition.mount_cleanup.detached_window, None);
                                }
                            });
                        }),
                    })
                    .into();
                tab_host.update(cx, |tabs, _| {
                    assert!(tabs.commit_detach(tab_id, mount, handle))
                });
                (handle, mount)
            };
            let (first, _) = open(cx);
            tab_host.update(cx, |tabs, _| {
                tabs.return_to_main(tab_id, TabMountCloseReason::ReturnToMain)
                    .unwrap();
            });
            let (second, mount) = open(cx);
            tab_host.update(cx, |tabs, _| {
                assert!(
                    tabs.remove_tab_for_detached_window_release(tab_id, mount, first.window_id())
                        .is_none()
                );
            });
            // Closing a retired shell during its owner's update must not re-enter
            // that owner or remove the replacement mount.
            tab_host.update(cx, |_, cx| {
                first
                    .update(cx, |_, window, _| window.remove_window())
                    .unwrap();
            });
            cx.run_until_parked();
            tab_host.read_with(cx, |tabs, _| {
                assert_eq!(
                    tabs.mount(tab_id),
                    Some(TabMount::Detached {
                        mount_id: mount,
                        window_id: second.window_id(),
                        handle: second,
                    })
                );
                assert_eq!(tabs.tab_by_id(tab_id).unwrap().kind, kind);
            });
            second
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
            cx.run_until_parked();
            tab_host.update(cx, |tabs, _| {
                assert!(
                    tabs.tab_by_id(tab_id).is_none(),
                    "closed {kind:?} tab remains registered"
                );
                assert_eq!(tabs.mount(tab_id), None);
            });
        }
    }

    #[gpui::test]
    fn mount_return_and_tab_close_preserve_consumers_until_pane_cleanup(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_window, _cx| TabHostTestRoot);
        let main_window = cx.window_handle();
        let pane = cx.update(|window, cx| {
            cx.new(|cx| {
                TerminalPane::new_recording_playback(
                    80,
                    24,
                    oxideterm_gpui_terminal::TerminalUiPreferences::default(),
                    window,
                    cx,
                )
                .expect("recording pane")
            })
        });
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let tab_id = tab_host.update(cx, |tab_host, _cx| tab_host.alloc_tab_id());
        let pane_id = PaneId(8);
        let session_id = TerminalSessionId(9);
        tab_host.update(cx, |tab_host, cx| {
            tab_host.insert_tab(test_tab(tab_id, Some(PaneNode::leaf(pane_id, session_id))));
            tab_host.register_terminal_pane(pane_id, session_id, pane.clone(), main_window, cx);
            tab_host.bind_terminal_location(session_id, TerminalLocation { tab_id, pane_id });
        });

        let (_, cx) = cx.add_window_view(|_window, _cx| TabHostTestRoot);
        let detached_window = cx.window_handle();
        tab_host.update(cx, |tab_host, _cx| {
            let mount_id = tab_host.begin_detach(tab_id).expect("detach reservation");
            assert!(tab_host.commit_detach(tab_id, mount_id, detached_window));

            let returned = tab_host
                .return_to_main(tab_id, TabMountCloseReason::ReturnToMain)
                .expect("return cleanup");
            assert_eq!(returned.reason, TabMountCloseReason::ReturnToMain);
            assert_eq!(returned.detached_window, Some(detached_window));
            assert!(tab_host.panes().contains_key(&pane_id));
            assert_eq!(
                tab_host.terminal_location(session_id),
                Some(TerminalLocation { tab_id, pane_id })
            );

            let replacement_mount_id = tab_host.begin_detach(tab_id).expect("replacement detach");
            assert!(tab_host.commit_detach(tab_id, replacement_mount_id, detached_window));
            let closed = tab_host.remove_tab_at(0).unwrap().mount_cleanup;
            assert_eq!(closed.reason, TabMountCloseReason::TabClosed);
            assert_eq!(closed.detached_window, Some(detached_window));
            assert!(tab_host.panes().contains_key(&pane_id));
            assert_eq!(
                tab_host.terminal_location(session_id),
                Some(TerminalLocation { tab_id, pane_id })
            );
            assert_eq!(
                tab_host
                    .pane_window_affinities
                    .get(&pane_id)
                    .expect("pane affinity survives mount cleanup")
                    .current,
                detached_window
            );
        });
    }
}
