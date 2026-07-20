// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Settings page navigation and virtual-section model rules.
//!
//! The GPUI app supplies runtime counts for data-backed pages, while this
//! module owns the invariant section-count math shared by the settings page.

use std::collections::HashSet;

use crate::{AiSettingsPage, SettingsTab, TerminalSettingsPage};

pub const SETTINGS_SECTION_HEADER_ITEM_COUNT: usize = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsNavigationLayout {
    groups: Vec<Vec<SettingsTab>>,
}

impl Default for SettingsNavigationLayout {
    fn default() -> Self {
        Self::from_groups(SettingsTab::groups())
    }
}

impl SettingsNavigationLayout {
    pub fn from_persisted_groups(persisted_groups: &[Vec<String>]) -> Self {
        if persisted_groups.is_empty() {
            return Self::default();
        }

        let mut groups = Vec::with_capacity(persisted_groups.len());
        let mut seen = HashSet::new();

        for persisted_group in persisted_groups {
            let mut group = Vec::with_capacity(persisted_group.len());
            for id in persisted_group {
                if let Some(tab) = SettingsTab::from_id(id)
                    && seen.insert(tab)
                {
                    group.push(tab);
                }
            }
            if !group.is_empty() {
                groups.push(group);
            }
        }

        if groups.is_empty() {
            return Self::default();
        }

        // New settings pages remain reachable when an older custom layout is loaded.
        for tab in SettingsTab::all() {
            if seen.insert(*tab) {
                groups
                    .last_mut()
                    .expect("validated navigation layout has a group")
                    .push(*tab);
            }
        }

        Self { groups }
    }

    pub fn groups(&self) -> &[Vec<SettingsTab>] {
        &self.groups
    }

    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    pub fn add_group(&mut self) {
        self.groups.push(Vec::new());
    }

    pub fn remove_empty_group(&mut self, group_index: usize) -> bool {
        if self.groups.len() <= 1
            || self
                .groups
                .get(group_index)
                .is_none_or(|group| !group.is_empty())
        {
            return false;
        }
        self.groups.remove(group_index);
        true
    }

    pub fn move_tab_to_position(&mut self, tab: SettingsTab, target: SettingsTab) -> bool {
        if tab == target {
            return false;
        }
        let Some((target_group_index, target_index)) = self.tab_position(target) else {
            return false;
        };
        let Some(tab) = self.remove_tab(tab) else {
            return false;
        };
        let insertion_index = target_index.min(self.groups[target_group_index].len());
        self.groups[target_group_index].insert(insertion_index, tab);
        true
    }

    pub fn move_tab_to_group_start(&mut self, tab: SettingsTab, group_index: usize) -> bool {
        if group_index >= self.groups.len() {
            return false;
        }
        let Some(tab) = self.remove_tab(tab) else {
            return false;
        };
        self.groups[group_index].insert(0, tab);
        true
    }

    pub fn move_tab_to_group_end(&mut self, tab: SettingsTab, group_index: usize) -> bool {
        if group_index >= self.groups.len() {
            return false;
        }
        let Some(tab) = self.remove_tab(tab) else {
            return false;
        };
        self.groups[group_index].push(tab);
        true
    }

    pub fn move_group_to_position(&mut self, source_index: usize, target_index: usize) -> bool {
        if source_index == target_index
            || source_index >= self.groups.len()
            || target_index >= self.groups.len()
        {
            return false;
        }
        let group = self.groups.remove(source_index);
        let insertion_index = target_index.min(self.groups.len());
        self.groups.insert(insertion_index, group);
        true
    }

    pub fn move_group_to_end(&mut self, source_index: usize) -> bool {
        if source_index + 1 >= self.groups.len() {
            return false;
        }
        let group = self.groups.remove(source_index);
        self.groups.push(group);
        true
    }

    pub fn to_persisted_groups(&self) -> Vec<Vec<String>> {
        self.groups
            .iter()
            .filter(|group| !group.is_empty())
            .map(|group| group.iter().map(|tab| tab.id().to_string()).collect())
            .collect()
    }

    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn from_groups(groups: &[&[SettingsTab]]) -> Self {
        Self {
            groups: groups.iter().map(|group| group.to_vec()).collect(),
        }
    }

    fn tab_position(&self, tab: SettingsTab) -> Option<(usize, usize)> {
        self.groups
            .iter()
            .enumerate()
            .find_map(|(group_index, group)| {
                group
                    .iter()
                    .position(|candidate| *candidate == tab)
                    .map(|tab_index| (group_index, tab_index))
            })
    }

    fn remove_tab(&mut self, tab: SettingsTab) -> Option<SettingsTab> {
        let (group_index, tab_index) = self.tab_position(tab)?;
        Some(self.groups[group_index].remove(tab_index))
    }
}

pub fn settings_section_list_item_count(
    tab: SettingsTab,
    dynamic: SettingsDynamicSectionCounts,
) -> usize {
    SETTINGS_SECTION_HEADER_ITEM_COUNT + settings_tab_section_count(tab, dynamic)
}

pub fn settings_tab_section_count(
    tab: SettingsTab,
    dynamic: SettingsDynamicSectionCounts,
) -> usize {
    match tab {
        SettingsTab::General => {
            if cfg!(any(target_os = "windows", target_os = "macos")) {
                6
            } else {
                5
            }
        }
        SettingsTab::Portable => 1,
        SettingsTab::Terminal => terminal_settings_section_count(dynamic.terminal_page),
        SettingsTab::Appearance => 4,
        // Reconnect controls share one card and therefore one virtual section.
        SettingsTab::Connections => 6,
        SettingsTab::Privilege => 1,
        SettingsTab::Network => 2,
        SettingsTab::Sftp => 3,
        SettingsTab::Ide => 5,
        SettingsTab::Ai => ai_settings_section_count(dynamic.ai_page),
        SettingsTab::Knowledge => knowledge_settings_section_count(
            dynamic.knowledge_has_error,
            dynamic.knowledge_has_selected_collection,
        ),
        SettingsTab::Keybindings => {
            keybinding_settings_section_count(dynamic.visible_keybinding_scope_count)
        }
        SettingsTab::Help => 6,
    }
}

pub fn terminal_settings_section_count(page: TerminalSettingsPage) -> usize {
    let page_cards = match page {
        TerminalSettingsPage::Display => 4,
        TerminalSettingsPage::Input => 1,
        // The dedicated keybindings page owns shortcut discovery and editing.
        TerminalSettingsPage::Local => 4,
        TerminalSettingsPage::CommandBar => 3,
        TerminalSettingsPage::Awareness => 2,
        TerminalSettingsPage::Transfer => 1,
        TerminalSettingsPage::Highlight => 1,
    };
    1 + page_cards
}

pub fn ai_settings_section_count(page: AiSettingsPage) -> usize {
    let page_cards = match page {
        // Feature activation and privacy guidance are independent cards.
        AiSettingsPage::General => 2,
        AiSettingsPage::Providers => 1,
        AiSettingsPage::Agents => 1,
        // Context controls, prompt, memory, reasoning, and model windows are
        // separate cards so each virtual row owns one stable responsibility.
        AiSettingsPage::Context => 5,
        // Tool approval and MCP servers are independent top-level cards.
        AiSettingsPage::Tools => 2,
    };
    // The first section is the subpage picker, matching terminal settings.
    1 + page_cards
}

pub fn keybinding_settings_section_count(visible_scope_count: usize) -> usize {
    1 + visible_scope_count.max(1)
}

pub fn knowledge_settings_section_count(has_error: bool, has_selected_collection: bool) -> usize {
    1 + usize::from(has_error) + usize::from(has_selected_collection)
}

pub fn settings_section_list_identity(
    tab: SettingsTab,
    terminal_page: TerminalSettingsPage,
    ai_page: AiSettingsPage,
) -> String {
    // Keybinding filters update row signatures rather than replacing the list;
    // this keeps the toolbar-mounted selection animation alive.
    format!("{tab:?}:{terminal_page:?}:{ai_page:?}")
}

#[derive(Clone, Copy, Debug)]
pub struct SettingsDynamicSectionCounts {
    pub terminal_page: TerminalSettingsPage,
    pub ai_page: AiSettingsPage,
    pub visible_keybinding_scope_count: usize,
    pub knowledge_has_error: bool,
    pub knowledge_has_selected_collection: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_navigation_layout_matches_declared_groups() {
        assert_eq!(
            SettingsNavigationLayout::default().groups(),
            SettingsTab::groups()
        );
    }

    #[test]
    fn persisted_navigation_layout_ignores_invalid_entries_and_appends_new_tabs() {
        let persisted = vec![
            vec!["terminal".to_string(), "unknown".to_string()],
            vec!["general".to_string(), "terminal".to_string()],
        ];

        let layout = SettingsNavigationLayout::from_persisted_groups(&persisted);

        assert_eq!(layout.groups()[0], [SettingsTab::Terminal]);
        assert_eq!(layout.groups()[1][..1], [SettingsTab::General]);
        assert_eq!(
            layout.groups().iter().map(Vec::len).sum::<usize>(),
            SettingsTab::all().len()
        );
        assert_eq!(layout.groups().len(), 2);
    }

    #[test]
    fn navigation_pages_can_move_within_and_across_groups() {
        let mut layout = SettingsNavigationLayout::default();

        assert!(layout.move_tab_to_position(SettingsTab::Terminal, SettingsTab::Appearance));
        assert_eq!(
            layout.groups()[0][..3],
            [
                SettingsTab::General,
                SettingsTab::Terminal,
                SettingsTab::Appearance,
            ]
        );
        assert!(layout.move_tab_to_group_end(SettingsTab::General, 1));
        assert_eq!(layout.groups()[1].last(), Some(&SettingsTab::General));
    }

    #[test]
    fn navigation_groups_can_be_added_removed_and_reordered() {
        let mut layout = SettingsNavigationLayout::default();
        let original_first_group = layout.groups()[0].clone();

        layout.add_group();
        assert_eq!(layout.group_count(), 6);
        assert!(layout.move_group_to_end(0));
        assert_eq!(layout.groups().last(), Some(&original_first_group));
        assert!(layout.remove_empty_group(4));
        assert_eq!(layout.group_count(), 5);
    }

    #[test]
    fn settings_tab_ids_round_trip() {
        for tab in SettingsTab::all() {
            assert_eq!(SettingsTab::from_id(tab.id()), Some(*tab));
        }
    }

    #[test]
    fn keybindings_page_keeps_an_empty_scope_section() {
        assert_eq!(keybinding_settings_section_count(0), 2);
        assert_eq!(keybinding_settings_section_count(3), 4);
    }

    #[test]
    fn general_page_includes_application_lock_card() {
        let dynamic = SettingsDynamicSectionCounts {
            terminal_page: TerminalSettingsPage::Display,
            ai_page: AiSettingsPage::General,
            visible_keybinding_scope_count: 0,
            knowledge_has_error: false,
            knowledge_has_selected_collection: false,
        };
        let expected = if cfg!(any(target_os = "windows", target_os = "macos")) {
            6
        } else {
            5
        };

        assert_eq!(
            settings_tab_section_count(SettingsTab::General, dynamic),
            expected
        );
    }

    #[test]
    fn knowledge_page_counts_error_and_selected_collection_sections() {
        assert_eq!(knowledge_settings_section_count(false, false), 1);
        assert_eq!(knowledge_settings_section_count(true, true), 3);
    }

    #[test]
    fn terminal_page_count_includes_subtab_picker() {
        assert_eq!(
            terminal_settings_section_count(TerminalSettingsPage::Display),
            5
        );
        assert_eq!(
            terminal_settings_section_count(TerminalSettingsPage::Input),
            2
        );
        assert_eq!(
            terminal_settings_section_count(TerminalSettingsPage::CommandBar),
            4
        );
        assert_eq!(
            terminal_settings_section_count(TerminalSettingsPage::Awareness),
            3
        );
        assert_eq!(
            terminal_settings_section_count(TerminalSettingsPage::Local),
            5
        );
    }

    #[test]
    fn help_page_count_includes_safety_guidance() {
        let dynamic = SettingsDynamicSectionCounts {
            terminal_page: TerminalSettingsPage::Display,
            ai_page: AiSettingsPage::General,
            visible_keybinding_scope_count: 0,
            knowledge_has_error: false,
            knowledge_has_selected_collection: false,
        };

        assert_eq!(settings_tab_section_count(SettingsTab::Help, dynamic), 6);
    }

    #[test]
    fn connections_page_counts_each_top_level_card_once() {
        let dynamic = SettingsDynamicSectionCounts {
            terminal_page: TerminalSettingsPage::Display,
            ai_page: AiSettingsPage::General,
            visible_keybinding_scope_count: 0,
            knowledge_has_error: false,
            knowledge_has_selected_collection: false,
        };

        assert_eq!(
            settings_tab_section_count(SettingsTab::Connections, dynamic),
            6
        );
    }

    #[test]
    fn network_page_groups_proxy_definition_and_routing_policy() {
        let dynamic = SettingsDynamicSectionCounts {
            terminal_page: TerminalSettingsPage::Display,
            ai_page: AiSettingsPage::General,
            visible_keybinding_scope_count: 0,
            knowledge_has_error: false,
            knowledge_has_selected_collection: false,
        };

        assert_eq!(settings_tab_section_count(SettingsTab::Network, dynamic), 2);
    }

    #[test]
    fn ai_tools_page_counts_tool_policy_and_mcp_cards() {
        assert_eq!(ai_settings_section_count(AiSettingsPage::Tools), 3);
    }

    #[test]
    fn ai_general_page_counts_toggle_and_privacy_cards() {
        assert_eq!(ai_settings_section_count(AiSettingsPage::General), 3);
    }

    #[test]
    fn ai_context_page_counts_independent_policy_cards() {
        assert_eq!(ai_settings_section_count(AiSettingsPage::Context), 6);
    }
}
