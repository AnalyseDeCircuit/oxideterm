// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Persistent page-model state for settings surfaces.
//!
//! This is intentionally free of GPUI handles, anchors, list state, focus
//! handles, and rendered element caches. The app owns those view concerns while
//! this model owns settings-page business state and drafts.

use crate::{AiSettingsPage, SettingsTab, TerminalSettingsPage};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CliCompanionStatus {
    pub bundled: bool,
    pub installed: bool,
    pub install_path: Option<String>,
    pub legacy_installed: bool,
    pub legacy_install_path: Option<String>,
    pub bundle_path: Option<String>,
    pub app_version: String,
    pub matches_bundled: Option<bool>,
    pub needs_reinstall: bool,
}

#[derive(Clone, Debug)]
pub struct SettingsPageModel {
    pub active_tab: SettingsTab,
    pub terminal_page: TerminalSettingsPage,
    pub previous_terminal_page: TerminalSettingsPage,
    pub ai_page: AiSettingsPage,
    pub previous_ai_page: AiSettingsPage,
    pub settings_reset_confirm_open: bool,
    pub legal_notice_open: bool,
    pub background_cache_poll_scheduled: bool,
}

impl Default for SettingsPageModel {
    fn default() -> Self {
        Self {
            active_tab: SettingsTab::General,
            terminal_page: TerminalSettingsPage::Display,
            previous_terminal_page: TerminalSettingsPage::Display,
            ai_page: AiSettingsPage::General,
            previous_ai_page: AiSettingsPage::General,
            settings_reset_confirm_open: false,
            legal_notice_open: false,
            background_cache_poll_scheduled: false,
        }
    }
}

impl SettingsPageModel {
    /// Selects the active settings tab without coupling tab routing to the app root.
    pub fn set_active_tab(&mut self, tab: SettingsTab) {
        self.active_tab = tab;
    }

    /// Selects the active terminal settings subpage.
    pub fn set_terminal_page(&mut self, page: TerminalSettingsPage) {
        if self.terminal_page != page {
            self.previous_terminal_page = self.terminal_page;
        }
        self.terminal_page = page;
    }

    /// Selects the active OxideSens settings subpage.
    pub fn set_ai_page(&mut self, page: AiSettingsPage) {
        if self.ai_page != page {
            self.previous_ai_page = self.ai_page;
        }
        self.ai_page = page;
    }

    /// Opens or closes the settings reset confirmation without exposing the flag layout.
    pub fn set_settings_reset_confirm_open(&mut self, is_open: bool) {
        self.settings_reset_confirm_open = is_open;
    }

    /// Marks whether the background image cache poll has already been scheduled.
    pub fn set_background_cache_poll_scheduled(&mut self, is_scheduled: bool) {
        self.background_cache_poll_scheduled = is_scheduled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_routing_state_lives_in_settings_model() {
        let mut model = SettingsPageModel::default();

        model.set_active_tab(SettingsTab::Keybindings);
        model.set_terminal_page(TerminalSettingsPage::Awareness);
        assert_eq!(model.active_tab, SettingsTab::Keybindings);
        assert_eq!(model.terminal_page, TerminalSettingsPage::Awareness);
        assert_eq!(model.previous_terminal_page, TerminalSettingsPage::Display);
    }
}
