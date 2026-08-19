// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

const MAX_TAB_HISTORY: usize = 50;
const RECORDING_ELAPSED_TICK_INTERVAL: Duration = Duration::from_millis(530);

/// Owns workspace-wide tab identity, terminal mounts, navigation, and close lifecycle.
pub(in crate::workspace) struct WorkspaceTabHostEntity {
    tabs: Vec<Tab>,
    active_tab_id: Option<TabId>,
    active_tab_index_cache: Cell<Option<(TabId, usize)>>,
    next_tab_id: u64,
    next_pane_id: u64,
    next_session_id: u64,
    panes: HashMap<PaneId, Entity<TerminalPane>>,
    pane_subscriptions: HashMap<PaneId, Subscription>,
    pane_window_affinities: HashMap<PaneId, TerminalPaneWindowAffinity>,
    tab_mounts: HashMap<TabId, TabMount>,
    pending_detach_mounts: HashMap<TabId, TabMountId>,
    next_tab_mount_id: u64,
    terminal_locations: HashMap<TerminalSessionId, TerminalLocation>,
    terminal_output_highlight_enabled: bool,
    tabs_with_unread_terminal_output: HashSet<TabId>,
    navigation_history: Vec<TabId>,
    navigation_index: Option<usize>,
    navigation_replaying: bool,
    navigation_observed_tab: Option<TabId>,
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::workspace) struct TabMountId(u64);

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum WorkspaceTabHostEvent {
    CloseProcessCheckReady,
    RecordingElapsedTick {
        pane_id: PaneId,
    },
    TerminalOutputUnread,
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

impl WorkspaceTabHostEntity {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab_id: None,
            active_tab_index_cache: Cell::new(None),
            next_tab_id: 1,
            next_pane_id: 1,
            next_session_id: 1,
            panes: HashMap::new(),
            pane_subscriptions: HashMap::new(),
            pane_window_affinities: HashMap::new(),
            tab_mounts: HashMap::new(),
            pending_detach_mounts: HashMap::new(),
            next_tab_mount_id: 1,
            terminal_locations: HashMap::new(),
            terminal_output_highlight_enabled: true,
            tabs_with_unread_terminal_output: HashSet::new(),
            navigation_history: Vec::new(),
            navigation_index: None,
            navigation_replaying: false,
            navigation_observed_tab: None,
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

    pub(in crate::workspace) fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub(in crate::workspace) fn active_tab_id(&self) -> Option<TabId> {
        self.active_tab_id
    }

    pub(in crate::workspace) fn active_tab_index(&self) -> Option<usize> {
        let active_tab_id = self.active_tab_id?;
        if let Some((cached_tab_id, cached_index)) = self.active_tab_index_cache.get()
            && cached_tab_id == active_tab_id
            && self
                .tabs
                .get(cached_index)
                .is_some_and(|tab| tab.id == active_tab_id)
        {
            return Some(cached_index);
        }
        let index = self.tabs.iter().position(|tab| tab.id == active_tab_id)?;
        self.active_tab_index_cache
            .set(Some((active_tab_id, index)));
        Some(index)
    }

    pub(in crate::workspace) fn tab_index_by_id(&self, tab_id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == tab_id)
    }

    pub(in crate::workspace) fn tab_by_id(&self, tab_id: TabId) -> Option<&Tab> {
        self.tab_index_by_id(tab_id)
            .and_then(|index| self.tabs.get(index))
    }

    /// Renames only terminal display metadata without changing pane or session ownership.
    pub(in crate::workspace) fn rename_terminal_tab(&mut self, tab_id: TabId, title: &str) -> bool {
        let normalized_title = title.trim();
        if normalized_title.is_empty() {
            return false;
        }
        let Some(tab) = self.tab_mut_by_id(tab_id) else {
            return false;
        };
        if !matches!(
            tab.kind,
            TabKind::LocalTerminal | TabKind::SshTerminal | TabKind::MoshTerminal
        ) {
            return false;
        }
        tab.title.clear();
        tab.title.push_str(normalized_title);
        tab.title_source = TabTitleSource::Static;
        true
    }

    fn tab_mut_by_id(&mut self, tab_id: TabId) -> Option<&mut Tab> {
        let index = self.tab_index_by_id(tab_id)?;
        self.tabs.get_mut(index)
    }

    pub(in crate::workspace) fn active_tab(&self) -> Option<&Tab> {
        self.active_tab_index()
            .and_then(|index| self.tabs.get(index))
    }

    fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        let index = self.active_tab_index()?;
        self.tabs.get_mut(index)
    }

    /// Inserts a tab into the canonical collection and invalidates index projections.
    pub(in crate::workspace) fn insert_tab(&mut self, tab: Tab) {
        debug_assert!(
            self.tab_by_id(tab.id).is_none(),
            "tab identity must be unique inside one workspace"
        );
        self.tabs.push(tab);
        self.active_tab_index_cache.set(None);
    }

    pub(in crate::workspace) fn insert_and_select_main_tab(&mut self, tab: Tab) -> Option<TabId> {
        let tab_id = tab.id;
        self.insert_tab(tab);
        self.select_main_tab(Some(tab_id))
    }

    /// Removes one canonical tab and chooses the next main-window selection atomically.
    pub(in crate::workspace) fn remove_tab_at(
        &mut self,
        index: usize,
    ) -> Option<TabRemovalTransition> {
        let previous_active_tab_id = self.active_tab_id;
        let tab = self.tabs.get(index)?;
        let removed_was_active = Some(tab.id) == previous_active_tab_id;
        let tab = self.tabs.remove(index);
        self.tabs_with_unread_terminal_output.remove(&tab.id);
        let mount_cleanup = self.close_tab_mount(tab.id);
        let next_active_tab_id = if !removed_was_active
            && previous_active_tab_id.is_some_and(|tab_id| {
                self.tab_by_id(tab_id)
                    .is_some_and(|tab| !self.is_outside_main_window(tab.id))
            }) {
            previous_active_tab_id
        } else {
            self.tabs
                .iter()
                .enumerate()
                .skip(index.min(self.tabs.len().saturating_sub(1)))
                .find(|(_, tab)| !self.is_outside_main_window(tab.id))
                .or_else(|| {
                    self.tabs
                        .iter()
                        .enumerate()
                        .take(index)
                        .rev()
                        .find(|(_, tab)| !self.is_outside_main_window(tab.id))
                })
                .map(|(_, tab)| tab.id)
        };
        self.select_main_tab(next_active_tab_id);
        Some(TabRemovalTransition {
            tab,
            mount_cleanup,
            previous_active_tab_id,
            next_active_tab_id,
        })
    }

    /// Reorders a main-window tab among the visible main-window tabs.
    pub(in crate::workspace) fn move_main_tab_to_visible_index(
        &mut self,
        tab_id: TabId,
        target_visible_index: usize,
    ) -> bool {
        let Some(source_index) = self.tab_index_by_id(tab_id) else {
            return false;
        };
        let visible_tab_ids = self
            .tabs
            .iter()
            .filter(|tab| tab.id != tab_id && !self.is_outside_main_window(tab.id))
            .map(|tab| tab.id)
            .collect::<Vec<_>>();
        let target_visible_index = target_visible_index.min(visible_tab_ids.len());
        let anchor_tab_id = visible_tab_ids.get(target_visible_index).copied();
        let trailing_tab_id = target_visible_index
            .checked_sub(1)
            .and_then(|index| visible_tab_ids.get(index).copied());

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
        self.active_tab_index_cache.set(None);
        changed
    }

    pub(in crate::workspace) fn set_active_pane(
        &mut self,
        tab_id: Option<TabId>,
        pane_id: PaneId,
    ) -> bool {
        let tab = match tab_id {
            Some(tab_id) => self.tab_mut_by_id(tab_id),
            None => self.active_tab_mut(),
        };
        let Some(tab) = tab else {
            return false;
        };
        if tab.active_pane_id == Some(pane_id) {
            return false;
        }
        tab.active_pane_id = Some(pane_id);
        true
    }

    pub(in crate::workspace) fn split_pane(
        &mut self,
        tab_id: TabId,
        active_pane_id: PaneId,
        group_id: PaneId,
        direction: SplitDirection,
        pane_id: PaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        let Some(tab) = self.tab_mut_by_id(tab_id) else {
            return false;
        };
        let split = tab.root_pane.as_mut().is_some_and(|root_pane| {
            root_pane.split_active(active_pane_id, group_id, direction, pane_id, session_id)
        });
        if split {
            tab.active_pane_id = Some(pane_id);
        }
        split
    }

    pub(in crate::workspace) fn close_pane(
        &mut self,
        tab_id: TabId,
        pane_id: PaneId,
    ) -> Option<PaneId> {
        let tab = self.tab_mut_by_id(tab_id)?;
        let root_pane = tab.root_pane.as_mut()?;
        let next_active_pane_id = root_pane.close_pane(pane_id)?;
        if let Some(replacement) = root_pane.single_child_replacement() {
            tab.root_pane = Some(replacement);
        }
        tab.active_pane_id = Some(next_active_pane_id);
        Some(next_active_pane_id)
    }

    pub(in crate::workspace) fn reset_to_single_pane(
        &mut self,
        tab_id: TabId,
        pane_id: PaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        let Some(tab) = self.tab_mut_by_id(tab_id) else {
            return false;
        };
        tab.root_pane = Some(PaneNode::leaf(pane_id, session_id));
        tab.active_pane_id = Some(pane_id);
        true
    }

    pub(in crate::workspace) fn update_group_sizes(
        &mut self,
        tab_id: Option<TabId>,
        group_id: PaneId,
        sizes: &[f32],
    ) -> bool {
        let tab = match tab_id {
            Some(tab_id) => self.tab_mut_by_id(tab_id),
            None => self.active_tab_mut(),
        };
        tab.and_then(|tab| tab.root_pane.as_mut())
            .is_some_and(|root_pane| root_pane.update_group_sizes(group_id, sizes))
    }

    pub(in crate::workspace) fn reset_group_sizes(
        &mut self,
        tab_id: Option<TabId>,
        group_id: PaneId,
    ) -> bool {
        let tab = match tab_id {
            Some(tab_id) => self.tab_mut_by_id(tab_id),
            None => self.active_tab_mut(),
        };
        tab.and_then(|tab| tab.root_pane.as_mut())
            .is_some_and(|root_pane| root_pane.reset_group_sizes(group_id))
    }

    pub(in crate::workspace) fn replace_terminal_session(
        &mut self,
        tab_id: TabId,
        old_session_id: TerminalSessionId,
        old_pane_id: PaneId,
        new_pane_id: PaneId,
        new_session_id: TerminalSessionId,
    ) -> Option<PaneId> {
        let tab = self.tab_mut_by_id(tab_id)?;
        let replaced_pane_id =
            tab.root_pane
                .as_mut()?
                .replace_session(old_session_id, new_pane_id, new_session_id)?;
        if tab.active_pane_id == Some(old_pane_id) {
            tab.active_pane_id = Some(new_pane_id);
        }
        Some(replaced_pane_id)
    }

    pub(in crate::workspace) fn sync_tab_titles(
        &mut self,
        mut title_for_key: impl FnMut(&'static str) -> String,
    ) {
        for tab in &mut self.tabs {
            if let TabTitleSource::I18nKey(key) = tab.title_source {
                tab.title = title_for_key(key);
            }
        }
    }

    /// Updates the canonical main-window selection and navigation history together.
    pub(in crate::workspace) fn select_main_tab(
        &mut self,
        active_tab_id: Option<TabId>,
    ) -> Option<TabId> {
        debug_assert!(
            active_tab_id.is_none_or(|tab_id| self.tab_by_id(tab_id).is_some()),
            "main-window selection must reference a canonical tab"
        );
        let previous_active_tab_id = self.active_tab_id;
        self.active_tab_id = active_tab_id;
        if let Some(tab_id) = active_tab_id {
            self.tabs_with_unread_terminal_output.remove(&tab_id);
        }
        self.active_tab_index_cache.set(None);
        self.observe_active_tab(active_tab_id);
        previous_active_tab_id
    }

    pub(in crate::workspace) fn alloc_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        id
    }

    pub(in crate::workspace) fn alloc_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        id
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
            if *event == TerminalPaneEvent::OutputActivity {
                if tab_host.mark_terminal_output_unread(session_id).is_some() {
                    cx.emit(WorkspaceTabHostEvent::TerminalOutputUnread);
                }
                return;
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
                event: *event,
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
        self.unbind_terminal_location_for_pane(pane_id);
        self.panes.remove(&pane_id)
    }

    pub(in crate::workspace) fn panes(&self) -> &HashMap<PaneId, Entity<TerminalPane>> {
        &self.panes
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

    pub(in crate::workspace) fn unread_terminal_output_tab_ids(&self) -> HashSet<TabId> {
        // Rendering needs stable identities after releasing the Entity borrow.
        self.tabs_with_unread_terminal_output.clone()
    }

    pub(in crate::workspace) fn configure_terminal_output_highlight(&mut self, enabled: bool) {
        self.terminal_output_highlight_enabled = enabled;
        if !enabled {
            self.tabs_with_unread_terminal_output.clear();
        }
    }

    fn mark_terminal_output_unread(&mut self, session_id: TerminalSessionId) -> Option<TabId> {
        if !self.terminal_output_highlight_enabled {
            return None;
        }
        let tab_id = self.terminal_location(session_id)?.tab_id;
        if self.active_tab_id == Some(tab_id)
            || self.is_outside_main_window(tab_id)
            || self.tab_by_id(tab_id).is_none()
        {
            return None;
        }
        self.tabs_with_unread_terminal_output
            .insert(tab_id)
            .then_some(tab_id)
    }

    pub(in crate::workspace) fn begin_detach(&mut self, tab_id: TabId) -> Option<TabMountId> {
        if self.pending_detach_mounts.contains_key(&tab_id)
            || matches!(
                self.tab_mounts.get(&tab_id),
                Some(TabMount::Detached { .. })
            )
        {
            return None;
        }
        let mount_id = TabMountId(self.next_tab_mount_id);
        self.next_tab_mount_id = self.next_tab_mount_id.wrapping_add(1);
        self.pending_detach_mounts.insert(tab_id, mount_id);
        Some(mount_id)
    }

    pub(in crate::workspace) fn begin_detach_from_main(
        &mut self,
        tab_id: TabId,
    ) -> Option<TabDetachTransition> {
        let tab_index = self.tab_index_by_id(tab_id)?;
        let previous = self.active_tab_id;
        let mount_id = self.begin_detach(tab_id)?;
        self.tabs_with_unread_terminal_output.remove(&tab_id);
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
        if self.pending_detach_mounts.get(&tab_id).copied() != Some(mount_id)
            || matches!(
                self.tab_mounts.get(&tab_id),
                Some(TabMount::Detached { .. })
            )
        {
            return false;
        }
        self.pending_detach_mounts.remove(&tab_id);
        self.tab_mounts.insert(
            tab_id,
            TabMount::Detached {
                mount_id,
                window_id: window_handle.window_id(),
                handle: window_handle,
            },
        );
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

    pub(in crate::workspace) fn rollback_detach(
        &mut self,
        tab_id: TabId,
        mount_id: TabMountId,
    ) -> bool {
        if self.pending_detach_mounts.get(&tab_id).copied() != Some(mount_id) {
            return false;
        }
        self.pending_detach_mounts.remove(&tab_id);
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
        let previous = self.active_tab_id;
        let current = self.tab_by_id(tab_id).map(|tab| tab.id);
        self.select_main_tab(current);
        Some(MainTabSelectionChange { previous, current })
    }

    pub(in crate::workspace) fn return_to_main(
        &mut self,
        tab_id: TabId,
        reason: TabMountCloseReason,
    ) -> Option<TabMountCleanupPlan> {
        let Some(TabMount::Detached { handle, .. }) = self.tab_mounts.get(&tab_id).copied() else {
            return None;
        };
        self.tab_mounts.remove(&tab_id);
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
        let previous = self.active_tab_id;
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
        let tab_index = match self.tab_mounts.get(&tab_id).copied() {
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

    pub(in crate::workspace) fn close_tab_mount(&mut self, tab_id: TabId) -> TabMountCleanupPlan {
        self.pending_detach_mounts.remove(&tab_id);
        let detached_window = match self.tab_mounts.remove(&tab_id) {
            Some(TabMount::Detached { handle, .. }) => Some(handle),
            _ => None,
        };
        // Pane removal follows in the caller's tab-close transaction. Do not
        // release terminal or node consumers merely because the mount changed.
        TabMountCleanupPlan {
            tab_id,
            reason: TabMountCloseReason::TabClosed,
            detached_window,
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
        self.tab_mounts.get(&tab_id).copied()
    }

    pub(in crate::workspace) fn is_outside_main_window(&self, tab_id: TabId) -> bool {
        self.pending_detach_mounts.contains_key(&tab_id)
            || matches!(
                self.tab_mounts.get(&tab_id),
                Some(TabMount::Detached { .. })
            )
    }

    fn nearest_main_tab(&self, index: usize) -> Option<TabId> {
        self.tabs
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(_, tab)| !self.is_outside_main_window(tab.id))
            .or_else(|| {
                self.tabs
                    .iter()
                    .enumerate()
                    .take(index)
                    .rev()
                    .find(|(_, tab)| !self.is_outside_main_window(tab.id))
            })
            .map(|(_, tab)| tab.id)
    }

    pub(in crate::workspace) fn is_detached(&self, tab_id: TabId) -> bool {
        matches!(
            self.tab_mounts.get(&tab_id),
            Some(TabMount::Detached { .. })
        )
    }

    pub(in crate::workspace) fn outside_main_tab_ids(&self) -> HashSet<TabId> {
        // Renderers need to release the Entity borrow before mutating GPUI.
        // Copy only stable IDs, never tab contents, panes, or session state.
        self.tab_mounts
            .iter()
            .filter_map(|(tab_id, mount)| {
                matches!(mount, TabMount::Detached { .. }).then_some(*tab_id)
            })
            .chain(self.pending_detach_mounts.keys().copied())
            .collect()
    }

    pub(in crate::workspace) fn detached_window_handle(
        &self,
        tab_id: TabId,
    ) -> Option<AnyWindowHandle> {
        match self.tab_mounts.get(&tab_id).copied() {
            Some(TabMount::Detached { handle, .. }) => Some(handle),
            _ => None,
        }
    }

    pub(in crate::workspace) fn observe_active_tab(&mut self, active_tab_id: Option<TabId>) {
        if self.navigation_observed_tab == active_tab_id {
            return;
        }
        self.navigation_observed_tab = active_tab_id;

        let Some(tab_id) = active_tab_id else {
            return;
        };
        if self.navigation_replaying {
            self.navigation_replaying = false;
            return;
        }

        if let Some(index) = self.navigation_index {
            self.navigation_history.truncate(index.saturating_add(1));
        }
        if self.navigation_history.last().copied() != Some(tab_id) {
            self.navigation_history.push(tab_id);
        }
        if self.navigation_history.len() > MAX_TAB_HISTORY {
            let overflow = self.navigation_history.len() - MAX_TAB_HISTORY;
            self.navigation_history.drain(0..overflow);
        }
        self.navigation_index = self.navigation_history.len().checked_sub(1);
    }

    pub(in crate::workspace) fn navigate_history(
        &mut self,
        forward: bool,
        existing_tab_ids: &HashSet<TabId>,
    ) -> Option<TabId> {
        self.prune_navigation_history(existing_tab_ids);
        let mut index = self.navigation_index?;

        loop {
            if forward {
                if index + 1 >= self.navigation_history.len() {
                    return None;
                }
                index += 1;
            } else if index == 0 {
                return None;
            } else {
                index -= 1;
            }

            let tab_id = self.navigation_history[index];
            if existing_tab_ids.contains(&tab_id) {
                self.navigation_index = Some(index);
                self.navigation_replaying = true;
                return Some(tab_id);
            }
        }
    }

    fn prune_navigation_history(&mut self, existing_tab_ids: &HashSet<TabId>) {
        let current = self
            .navigation_index
            .and_then(|index| self.navigation_history.get(index).copied());
        self.navigation_history
            .retain(|tab_id| existing_tab_ids.contains(tab_id));
        self.navigation_index = current
            .and_then(|tab_id| {
                self.navigation_history
                    .iter()
                    .position(|candidate| *candidate == tab_id)
            })
            .or_else(|| self.navigation_history.len().checked_sub(1));
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
    }

    #[test]
    fn pane_tree_transitions_remain_inside_the_canonical_tab_owner() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let tab_id = TabId(1);
        let first_pane = PaneId(1);
        let first_session = TerminalSessionId(1);
        let split_group = PaneId(3);
        let second_pane = PaneId(2);
        let second_session = TerminalSessionId(2);
        tab_host.insert_and_select_main_tab(test_tab(
            tab_id,
            Some(PaneNode::leaf(first_pane, first_session)),
        ));

        // Split, focus, resize, close, and reconnect replacement mutate one tree.
        assert!(tab_host.split_pane(
            tab_id,
            first_pane,
            split_group,
            SplitDirection::Horizontal,
            second_pane,
            second_session,
        ));
        assert_eq!(
            tab_host.active_tab().and_then(|tab| tab.active_pane_id),
            Some(second_pane)
        );
        assert_eq!(
            tab_host
                .active_tab()
                .and_then(|tab| tab.root_pane.as_ref())
                .map(PaneNode::pane_count),
            Some(2)
        );
        assert!(tab_host.set_active_pane(Some(tab_id), first_pane));
        assert!(tab_host.update_group_sizes(Some(tab_id), split_group, &[35.0, 65.0]));
        assert!(tab_host.reset_group_sizes(Some(tab_id), split_group));
        assert_eq!(tab_host.close_pane(tab_id, second_pane), Some(first_pane));

        let replacement_pane = PaneId(4);
        let replacement_session = TerminalSessionId(4);
        assert_eq!(
            tab_host.replace_terminal_session(
                tab_id,
                first_session,
                first_pane,
                replacement_pane,
                replacement_session,
            ),
            Some(first_pane)
        );
        let tab = tab_host.active_tab().expect("active terminal tab");
        assert_eq!(tab.active_pane_id, Some(replacement_pane));
        assert_eq!(
            tab.root_pane
                .as_ref()
                .and_then(|root| root.session_id_for_pane(replacement_pane)),
            Some(replacement_session)
        );
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
    fn terminal_location_lifecycle_is_owned_by_tab_host() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let first_session = TerminalSessionId(1);
        let second_session = TerminalSessionId(2);
        let first_location = TerminalLocation {
            tab_id: TabId(3),
            pane_id: PaneId(4),
        };
        let second_location = TerminalLocation {
            tab_id: TabId(3),
            pane_id: PaneId(5),
        };

        tab_host.bind_terminal_location(first_session, first_location);
        tab_host.bind_terminal_location(second_session, second_location);
        assert_eq!(
            tab_host.terminal_location(first_session),
            Some(first_location)
        );
        assert_eq!(
            tab_host.unbind_terminal_location_for_pane(first_location.pane_id),
            Some(first_session)
        );
        assert!(tab_host.terminal_location(first_session).is_none());
        assert_eq!(
            tab_host.terminal_location(second_session),
            Some(second_location)
        );
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
    fn removing_pane_drops_delivery_subscription_and_terminal_location(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_window, _cx| TabHostTestRoot);
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
        let window_handle = cx.window_handle();
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let event_recorder = cx.new(|_| TabHostEventRecorder {
            events: Vec::new(),
            _subscription: None,
        });
        event_recorder.update(cx, |event_recorder, cx| {
            event_recorder._subscription = Some(cx.subscribe(
                &tab_host,
                |event_recorder, _tab_host, event, _cx| {
                    event_recorder.events.push(*event);
                },
            ));
        });
        let pane_id = PaneId(4);
        let session_id = TerminalSessionId(5);

        tab_host.update(cx, |tab_host, cx| {
            tab_host.register_terminal_pane(pane_id, session_id, pane.clone(), window_handle, cx);
            tab_host.bind_terminal_location(
                session_id,
                TerminalLocation {
                    tab_id: TabId(3),
                    pane_id,
                },
            );
            assert_eq!(tab_host.panes().len(), 1);
            assert_eq!(tab_host.pane_subscriptions.len(), 1);
            assert_eq!(tab_host.pane_window_affinities.len(), 1);
        });
        pane.update(cx, |_pane, cx| {
            cx.emit(TerminalPaneEvent::Exited { exit_code: Some(0) });
        });
        cx.run_until_parked();
        assert_eq!(
            event_recorder.read_with(cx, |event_recorder, _cx| event_recorder.events.clone()),
            vec![WorkspaceTabHostEvent::TerminalPaneDelivery {
                pane_id,
                session_id,
                window_handle,
                event: TerminalPaneEvent::Exited { exit_code: Some(0) },
            }]
        );

        tab_host.update(cx, |tab_host, _cx| {
            assert!(tab_host.remove_terminal_pane(pane_id).is_some());
            assert!(tab_host.panes().is_empty());
            assert!(tab_host.pane_subscriptions.is_empty());
            assert!(tab_host.pane_window_affinities.is_empty());
            assert!(tab_host.terminal_location(session_id).is_none());
        });
        pane.update(cx, |_pane, cx| {
            cx.emit(TerminalPaneEvent::ContextActionRequested);
        });
        cx.run_until_parked();
        assert_eq!(
            event_recorder.read_with(cx, |event_recorder, _cx| event_recorder.events.len()),
            1
        );
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
                    event_recorder.events.push(*event);
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
    }

    #[test]
    fn navigation_replay_does_not_create_a_new_history_branch() {
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
    }

    #[test]
    fn navigation_prunes_closed_tabs_and_new_selection_replaces_forward_history() {
        let mut tab_host = WorkspaceTabHostEntity::new();
        let first = TabId(1);
        let second = TabId(2);
        let third = TabId(3);
        let replacement = TabId(4);

        tab_host.observe_active_tab(Some(first));
        tab_host.observe_active_tab(Some(second));
        tab_host.observe_active_tab(Some(third));
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
    fn newer_close_process_check_cancels_and_replaces_the_previous_task(cx: &mut TestAppContext) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
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

        let completion = tab_host
            .update(cx, |tab_host, _| tab_host.take_close_process_completion())
            .expect("latest close process completion");
        assert_eq!(
            completion.request,
            LocalTerminalCloseCheck::Batch {
                tab_ids: vec![TabId(2), TabId(3)]
            }
        );
        assert!(completion.results.is_empty());
        assert!(!completion.has_foreground_child);
    }

    #[gpui::test]
    fn entity_release_cancels_close_process_check_without_completion(cx: &mut TestAppContext) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
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
    }

    #[gpui::test]
    fn current_close_process_check_completes_and_notifies_exactly_once(cx: &mut TestAppContext) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let event_recorder = cx.new(|_| TabHostEventRecorder {
            events: Vec::new(),
            _subscription: None,
        });
        event_recorder.update(cx, |event_recorder, cx| {
            event_recorder._subscription = Some(cx.subscribe(
                &tab_host,
                |event_recorder, _tab_host, event, _cx| {
                    event_recorder.events.push(*event);
                },
            ));
        });
        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
        tab_host.update(cx, |tab_host, cx| {
            tab_host.start_close_process_check_with_future(
                LocalTerminalCloseCheck::Single { tab_id: TabId(7) },
                async move {
                    result_receiver.await.expect("test result released");
                    Vec::new()
                },
                cx,
            );
        });

        result_sender.send(()).expect("current task retained");
        cx.run_until_parked();

        assert_eq!(
            event_recorder.read_with(cx, |event_recorder, _cx| event_recorder.events.clone()),
            vec![WorkspaceTabHostEvent::CloseProcessCheckReady]
        );
        tab_host.update(cx, |tab_host, _cx| {
            let completion = tab_host
                .take_close_process_completion()
                .expect("current completion");
            assert_eq!(
                completion.request,
                LocalTerminalCloseCheck::Single { tab_id: TabId(7) }
            );
            assert!(tab_host.take_close_process_completion().is_none());
        });
        cx.run_until_parked();
        assert_eq!(
            event_recorder.read_with(cx, |event_recorder, _cx| event_recorder.events.len()),
            1
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

    #[gpui::test]
    fn entity_release_cancels_retained_close_confirmation_exit(cx: &mut TestAppContext) {
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let (release_sender, release_receiver) = tokio::sync::oneshot::channel();
        tab_host.update(cx, |tab_host, cx| {
            // The confirmation owner retains the exit task and therefore
            // cancels it when its Entity is released.
            tab_host.close_confirm_exit_task = Some(cx.spawn(async move |_, _| {
                let _ = release_receiver.await;
            }));
        });
        cx.run_until_parked();

        drop(tab_host);
        cx.update(|_| {});
        cx.run_until_parked();

        assert!(release_sender.send(()).is_err());
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

    #[gpui::test]
    fn detached_window_release_closes_only_its_current_tab_mount(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_window, _cx| TabHostTestRoot);
        let first_window = cx.window_handle();
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let tab_id = tab_host.update(cx, |tab_host, _cx| {
            let tab_id = tab_host.alloc_tab_id();
            tab_host.insert_tab(test_tab(tab_id, None));
            tab_id
        });
        let first_mount_id = tab_host.update(cx, |tab_host, _cx| {
            let mount_id = tab_host.begin_detach(tab_id).expect("first reservation");
            assert!(tab_host.commit_detach(tab_id, mount_id, first_window));
            mount_id
        });

        let (_, cx) = cx.add_window_view(|_window, _cx| TabHostTestRoot);
        let second_window = cx.window_handle();
        let second_mount_id = tab_host.update(cx, |tab_host, _cx| {
            assert!(
                tab_host
                    .return_to_main(tab_id, TabMountCloseReason::ReturnToMain)
                    .is_some()
            );
            let mount_id = tab_host
                .begin_detach(tab_id)
                .expect("replacement reservation");
            assert!(tab_host.commit_detach(tab_id, mount_id, second_window));
            mount_id
        });

        tab_host.update(cx, |tab_host, _cx| {
            assert!(
                tab_host
                    .remove_tab_for_detached_window_release(
                        tab_id,
                        first_mount_id,
                        first_window.window_id(),
                    )
                    .is_none()
            );
            assert!(
                tab_host
                    .remove_tab_for_detached_window_release(
                        tab_id,
                        second_mount_id,
                        first_window.window_id(),
                    )
                    .is_none()
            );
            assert_eq!(
                tab_host.mount(tab_id),
                Some(TabMount::Detached {
                    mount_id: second_mount_id,
                    window_id: second_window.window_id(),
                    handle: second_window,
                })
            );

            let transition = tab_host
                .remove_tab_for_detached_window_release(
                    tab_id,
                    second_mount_id,
                    second_window.window_id(),
                )
                .expect("current release removes tab");
            assert_eq!(transition.tab.id, tab_id);
            assert_eq!(
                transition.mount_cleanup.reason,
                TabMountCloseReason::TabClosed
            );
            assert_eq!(transition.mount_cleanup.detached_window, None);
            assert_eq!(tab_host.mount(tab_id), None);
            assert!(tab_host.tab_by_id(tab_id).is_none());
        });
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
            let closed = tab_host.close_tab_mount(tab_id);
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

    #[gpui::test]
    fn detached_sftp_and_forward_mount_release_preserves_runtime_owners(cx: &mut TestAppContext) {
        let sftp_window: AnyWindowHandle = cx.add_window(|_window, _cx| TabHostTestRoot).into();
        let forwards_window: AnyWindowHandle = cx.add_window(|_window, _cx| TabHostTestRoot).into();
        let tab_host = cx.new(|_| WorkspaceTabHostEntity::new());
        let (sftp_tab_id, forwards_tab_id) = tab_host.update(cx, |tab_host, _cx| {
            let sftp_tab_id = tab_host.alloc_tab_id();
            let forwards_tab_id = tab_host.alloc_tab_id();
            tab_host.insert_tab(test_tab(sftp_tab_id, None));
            tab_host.insert_tab(test_tab(forwards_tab_id, None));
            (sftp_tab_id, forwards_tab_id)
        });

        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(ssh_registry.clone());
        let node_id = NodeId::new("shared-runtime-node");
        let config = SshConfig::default();
        node_router.upsert_node(node_id.clone(), config.clone());
        let node_consumer = ConnectionConsumer::NodeRouter(node_id.0.clone());
        let node_handle = ssh_registry.acquire(config.clone(), node_consumer.clone());
        node_router
            .bind_connection(&node_id, node_handle.connection_id().to_string())
            .expect("node binding");

        let sftp_consumer = ConnectionConsumer::Sftp(node_id.0.clone());
        let sftp_handle = ssh_registry.acquire(config.clone(), sftp_consumer.clone());
        let forwarding_session = forwards::ForwardingRuntimeService::session_id_for_node(&node_id);
        let forwarding_consumer = ConnectionConsumer::PortForward(forwarding_session.clone());
        let forwarding_handle = ssh_registry.acquire(config, forwarding_consumer.clone());
        let forwarding_registry = ForwardingRegistry::new();
        forwarding_registry.register(forwarding_session.clone(), forwarding_handle.clone());

        tab_host.update(cx, |tab_host, _cx| {
            let sftp_mount = tab_host
                .begin_detach(sftp_tab_id)
                .expect("SFTP detach reservation");
            assert!(tab_host.commit_detach(sftp_tab_id, sftp_mount, sftp_window));
            let forwards_mount = tab_host
                .begin_detach(forwards_tab_id)
                .expect("forwards detach reservation");
            assert!(tab_host.commit_detach(forwards_tab_id, forwards_mount, forwards_window));

            assert!(
                tab_host
                    .remove_tab_for_detached_window_release(
                        sftp_tab_id,
                        sftp_mount,
                        sftp_window.window_id(),
                    )
                    .is_some()
            );
            let forwards_cleanup = tab_host.close_tab_mount(forwards_tab_id);
            assert_eq!(forwards_cleanup.detached_window, Some(forwards_window));
        });

        // Native mount cleanup does not own node, transfer, or tunnel teardown.
        assert_eq!(
            node_router.connection_id_for_node(&node_id).as_deref(),
            Some(node_handle.connection_id())
        );
        let connection_info = ssh_registry
            .get(node_handle.connection_id())
            .expect("shared SSH connection remains registered")
            .info();
        assert!(connection_info.consumers.contains(&node_consumer));
        assert!(connection_info.consumers.contains(&sftp_consumer));
        assert!(connection_info.consumers.contains(&forwarding_consumer));
        assert_eq!(sftp_handle.connection_id(), node_handle.connection_id());
        assert_eq!(
            forwarding_handle.connection_id(),
            node_handle.connection_id()
        );
        assert_eq!(
            forwarding_registry
                .get(&forwarding_session)
                .expect("forwarding manager survives UI mount release")
                .ssh_connection_handle()
                .connection_id(),
            node_handle.connection_id()
        );
    }
}
