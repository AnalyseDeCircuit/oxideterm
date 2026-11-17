use std::sync::Arc;

use oxideterm_settings_model::{
    AiSettingsPage, SettingsNavigationLayout, SettingsTab, TerminalSettingsPage,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingsRouteSnapshot {
    pub active_tab: SettingsTab,
    pub terminal_page: TerminalSettingsPage,
    pub previous_terminal_page: TerminalSettingsPage,
    pub ai_page: AiSettingsPage,
    pub previous_ai_page: AiSettingsPage,
}

/// Keeps settings route history and its navigation editor draft under one writer.
pub struct SettingsRouteState {
    active_tab: SettingsTab,
    terminal_page: TerminalSettingsPage,
    previous_terminal_page: TerminalSettingsPage,
    ai_page: AiSettingsPage,
    previous_ai_page: AiSettingsPage,
    navigation_draft: Option<Arc<SettingsNavigationLayout>>,
}

impl Default for SettingsRouteState {
    fn default() -> Self {
        Self {
            active_tab: SettingsTab::General,
            terminal_page: TerminalSettingsPage::Display,
            previous_terminal_page: TerminalSettingsPage::Display,
            ai_page: AiSettingsPage::General,
            previous_ai_page: AiSettingsPage::General,
            navigation_draft: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsNavigationDraftAction {
    MoveTabBefore {
        tab: SettingsTab,
        target: SettingsTab,
    },
    MoveTabToGroupStart {
        tab: SettingsTab,
        group_index: usize,
    },
    MoveTabToGroupEnd {
        tab: SettingsTab,
        group_index: usize,
    },
    MoveGroupToPosition {
        source_index: usize,
        target_index: usize,
    },
    MoveGroupToEnd {
        source_index: usize,
    },
    RemoveEmptyGroup {
        group_index: usize,
    },
    AddGroup,
    AddGroupWithTab(SettingsTab),
    RestoreDefault,
}

impl SettingsRouteState {
    pub fn route_snapshot(&self) -> SettingsRouteSnapshot {
        SettingsRouteSnapshot {
            active_tab: self.active_tab,
            terminal_page: self.terminal_page,
            previous_terminal_page: self.previous_terminal_page,
            ai_page: self.ai_page,
            previous_ai_page: self.previous_ai_page,
        }
    }

    pub fn set_active_tab(&mut self, tab: SettingsTab) -> bool {
        if self.active_tab == tab {
            return false;
        }
        self.active_tab = tab;
        true
    }

    pub fn set_terminal_page(&mut self, page: TerminalSettingsPage) -> bool {
        if self.terminal_page == page {
            return false;
        }
        self.previous_terminal_page = self.terminal_page;
        self.terminal_page = page;
        true
    }

    pub fn set_ai_page(&mut self, page: AiSettingsPage) -> bool {
        if self.ai_page == page {
            return false;
        }
        self.previous_ai_page = self.ai_page;
        self.ai_page = page;
        true
    }

    pub fn open_navigation_editor(&mut self, persisted_groups: &[Vec<String>]) {
        self.navigation_draft = Some(Arc::new(SettingsNavigationLayout::from_persisted_groups(
            persisted_groups,
        )));
    }

    pub fn close_navigation_editor(&mut self) -> bool {
        self.navigation_draft.take().is_some()
    }

    /// Returns a shallow immutable render snapshot of the navigation draft.
    pub fn navigation_draft_snapshot(&self) -> Option<Arc<SettingsNavigationLayout>> {
        self.navigation_draft.as_ref().map(Arc::clone)
    }

    pub fn navigation_editor_open(&self) -> bool {
        self.navigation_draft.is_some()
    }

    pub fn apply_navigation_draft_action(&mut self, action: SettingsNavigationDraftAction) -> bool {
        let Some(layout) = self.navigation_draft.as_mut() else {
            return false;
        };
        let layout = Arc::make_mut(layout);
        match action {
            SettingsNavigationDraftAction::MoveTabBefore { tab, target } => {
                layout.move_tab_to_position(tab, target)
            }
            SettingsNavigationDraftAction::MoveTabToGroupStart { tab, group_index } => {
                layout.move_tab_to_group_start(tab, group_index)
            }
            SettingsNavigationDraftAction::MoveTabToGroupEnd { tab, group_index } => {
                layout.move_tab_to_group_end(tab, group_index)
            }
            SettingsNavigationDraftAction::MoveGroupToPosition {
                source_index,
                target_index,
            } => layout.move_group_to_position(source_index, target_index),
            SettingsNavigationDraftAction::MoveGroupToEnd { source_index } => {
                layout.move_group_to_end(source_index)
            }
            SettingsNavigationDraftAction::RemoveEmptyGroup { group_index } => {
                layout.remove_empty_group(group_index)
            }
            SettingsNavigationDraftAction::AddGroup => {
                layout.add_group();
                true
            }
            SettingsNavigationDraftAction::AddGroupWithTab(tab) => {
                layout.add_group();
                let destination_group = layout.group_count() - 1;
                // The group addition is itself a state change even if a
                // malformed draft no longer contains the requested page.
                let _ = layout.move_tab_to_group_end(tab, destination_group);
                true
            }
            SettingsNavigationDraftAction::RestoreDefault => {
                if layout.is_default() {
                    false
                } else {
                    *layout = SettingsNavigationLayout::default();
                    true
                }
            }
        }
    }

    /// Closes the editor and serializes only the persistence payload.
    pub fn take_navigation_editor_persisted_groups(&mut self) -> Option<Vec<Vec<String>>> {
        let layout = self.navigation_draft.take()?;
        let serialized_groups = layout.to_persisted_groups();
        let default_groups = SettingsNavigationLayout::default().to_persisted_groups();
        // Empty groups are not persisted, so compare serialized forms before
        // choosing the "follow future product defaults" empty payload.
        let persisted_groups = if serialized_groups == default_groups {
            Vec::new()
        } else {
            serialized_groups
        };
        Some(persisted_groups)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_draft_persists_reordered_group_but_not_restored_defaults() {
        let mut route = SettingsRouteState::default();
        route.open_navigation_editor(&[]);
        assert!(route.apply_navigation_draft_action(
            SettingsNavigationDraftAction::AddGroupWithTab(SettingsTab::General),
        ));
        let groups = route.take_navigation_editor_persisted_groups().unwrap();
        assert_eq!(
            groups.last(),
            Some(&vec![SettingsTab::General.id().to_owned()])
        );
        assert_eq!(
            groups
                .iter()
                .flatten()
                .filter(|id| *id == SettingsTab::General.id())
                .count(),
            1
        );

        route.open_navigation_editor(&groups);
        assert!(route.apply_navigation_draft_action(SettingsNavigationDraftAction::RestoreDefault));
        assert_eq!(
            route.take_navigation_editor_persisted_groups(),
            Some(Vec::new())
        );
    }
}
