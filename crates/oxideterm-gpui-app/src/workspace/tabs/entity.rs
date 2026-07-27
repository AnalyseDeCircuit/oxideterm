// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

const MAX_TAB_HISTORY: usize = 50;

/// Owns workspace-wide tab, pane, and terminal identity allocation.
pub(in crate::workspace) struct WorkspaceTabHostEntity {
    next_tab_id: u64,
    next_pane_id: u64,
    next_session_id: u64,
    navigation_history: Vec<TabId>,
    navigation_index: Option<usize>,
    navigation_replaying: bool,
    navigation_observed_tab: Option<TabId>,
}

impl WorkspaceTabHostEntity {
    pub(in crate::workspace) fn new() -> Self {
        Self {
            next_tab_id: 1,
            next_pane_id: 1,
            next_session_id: 1,
            navigation_history: Vec::new(),
            navigation_index: None,
            navigation_replaying: false,
            navigation_observed_tab: None,
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_workspace_ids_are_monotonic_and_independent() {
        let mut tab_host = WorkspaceTabHostEntity::new();

        assert_eq!(tab_host.alloc_tab_id(), TabId(1));
        assert_eq!(tab_host.alloc_pane_id(), PaneId(1));
        assert_eq!(tab_host.alloc_session_id(), TerminalSessionId(1));
        assert_eq!(tab_host.alloc_tab_id(), TabId(2));
        assert_eq!(tab_host.alloc_pane_id(), PaneId(2));
        assert_eq!(tab_host.alloc_session_id(), TerminalSessionId(2));
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
}
