// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Settings page navigation and virtual-section model rules.
//!
//! The GPUI app supplies runtime counts for data-backed pages, while this
//! module owns the invariant section-count math shared by the settings page.

use crate::{AiSettingsPage, SettingsTab, TerminalSettingsPage};

pub const SETTINGS_SECTION_HEADER_ITEM_COUNT: usize = 1;

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
                4
            } else {
                3
            }
        }
        SettingsTab::Portable => 1,
        SettingsTab::Terminal => terminal_settings_section_count(dynamic.terminal_page),
        SettingsTab::Appearance => 4,
        // Reconnect controls share one card and therefore one virtual section.
        SettingsTab::Connections => 6,
        SettingsTab::Privilege => 1,
        SettingsTab::Network => 4,
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
        TerminalSettingsPage::Display => 2,
        TerminalSettingsPage::Input => 1,
        // The dedicated keybindings page owns shortcut discovery and editing.
        TerminalSettingsPage::Local => 4,
        TerminalSettingsPage::CommandBar => 3,
        TerminalSettingsPage::History => 2,
        TerminalSettingsPage::Transfer => 1,
        TerminalSettingsPage::Highlight => 1,
    };
    1 + page_cards
}

pub fn ai_settings_section_count(page: AiSettingsPage) -> usize {
    let page_cards = match page {
        AiSettingsPage::General => 1,
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
    keybinding_scope_key: &str,
    keybinding_query: &str,
) -> String {
    format!("{tab:?}:{terminal_page:?}:{ai_page:?}:{keybinding_scope_key}:{keybinding_query}")
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
    fn keybindings_page_keeps_an_empty_scope_section() {
        assert_eq!(keybinding_settings_section_count(0), 2);
        assert_eq!(keybinding_settings_section_count(3), 4);
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
            3
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
    fn ai_tools_page_counts_tool_policy_and_mcp_cards() {
        assert_eq!(ai_settings_section_count(AiSettingsPage::Tools), 3);
    }

    #[test]
    fn ai_context_page_counts_independent_policy_cards() {
        assert_eq!(ai_settings_section_count(AiSettingsPage::Context), 6);
    }
}
