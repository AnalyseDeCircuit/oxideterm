// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Settings page navigation and virtual-section model rules.
//!
//! The GPUI app supplies runtime counts for data-backed pages, while this
//! module owns the invariant section-count math shared by the settings page.

use crate::{SettingsTab, TerminalSettingsPage};

pub const SETTINGS_SECTION_HEADER_ITEM_COUNT: usize = 1;
pub const AI_SETTINGS_FIXED_SECTION_COUNT: usize = 6;

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
        SettingsTab::General => 3,
        SettingsTab::Portable => 1,
        SettingsTab::Terminal => terminal_settings_section_count(dynamic.terminal_page),
        SettingsTab::Appearance => 3,
        SettingsTab::Local => 6,
        SettingsTab::Connections => 5,
        SettingsTab::Ssh => 1,
        SettingsTab::Reconnect => 3,
        SettingsTab::Sftp => 3,
        SettingsTab::Ide => 5,
        SettingsTab::Ai => AI_SETTINGS_FIXED_SECTION_COUNT,
        SettingsTab::Knowledge => knowledge_settings_section_count(
            dynamic.knowledge_has_error,
            dynamic.knowledge_has_selected_collection,
        ),
        SettingsTab::Keybindings => {
            keybinding_settings_section_count(dynamic.visible_keybinding_scope_count)
        }
        SettingsTab::Help => 5,
    }
}

pub fn terminal_settings_section_count(page: TerminalSettingsPage) -> usize {
    let page_cards = match page {
        TerminalSettingsPage::Display => 2,
        TerminalSettingsPage::Input => 1,
        TerminalSettingsPage::CommandBar => 1,
        TerminalSettingsPage::History => 2,
        TerminalSettingsPage::Transfer => 1,
        TerminalSettingsPage::Highlight => 1,
    };
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
    keybinding_scope_key: &str,
    keybinding_query: &str,
) -> String {
    format!("{tab:?}:{terminal_page:?}:{keybinding_scope_key}:{keybinding_query}")
}

#[derive(Clone, Copy, Debug)]
pub struct SettingsDynamicSectionCounts {
    pub terminal_page: TerminalSettingsPage,
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
    }

    #[test]
    fn help_page_count_omits_shortcuts_and_memory_diagnostics_sections() {
        let dynamic = SettingsDynamicSectionCounts {
            terminal_page: TerminalSettingsPage::Display,
            visible_keybinding_scope_count: 0,
            knowledge_has_error: false,
            knowledge_has_selected_collection: false,
        };

        assert_eq!(settings_tab_section_count(SettingsTab::Help, dynamic), 5);
    }
}
