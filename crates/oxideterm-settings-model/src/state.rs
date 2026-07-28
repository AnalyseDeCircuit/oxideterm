// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Persistent page-model state for settings surfaces.
//!
//! This is intentionally free of GPUI handles, anchors, list state, focus
//! handles, and rendered element caches. The app owns those view concerns while
//! this model owns settings-page business state and drafts.

use std::collections::{HashMap, HashSet};

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
    pub ai_new_provider_type: String,
    pub ai_provider_settings_expanded: bool,
    pub ai_tool_use_expanded: bool,
    pub ai_context_windows_expanded: bool,
    pub expanded_ai_providers: HashMap<String, bool>,
    pub expanded_ai_provider_models: HashSet<String>,
    pub expanded_ai_context_providers: HashSet<String>,
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
            ai_new_provider_type: "openai_compatible".to_string(),
            ai_provider_settings_expanded: true,
            ai_tool_use_expanded: true,
            ai_context_windows_expanded: true,
            expanded_ai_providers: HashMap::new(),
            expanded_ai_provider_models: HashSet::new(),
            expanded_ai_context_providers: HashSet::new(),
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

    /// Selects the AI provider template used by the add-provider controls.
    pub fn select_ai_provider_type(&mut self, provider_type: impl Into<String>) {
        self.ai_new_provider_type = provider_type.into();
    }

    /// Toggles one of the top-level AI settings sections owned by the page model.
    pub fn toggle_ai_section(&mut self, section: AiSettingsSection) {
        match section {
            AiSettingsSection::ProviderSettings => {
                self.ai_provider_settings_expanded = !self.ai_provider_settings_expanded;
            }
            AiSettingsSection::ToolUse => {
                self.ai_tool_use_expanded = !self.ai_tool_use_expanded;
            }
            AiSettingsSection::ContextWindows => {
                self.ai_context_windows_expanded = !self.ai_context_windows_expanded;
            }
        }
    }

    /// Flips the per-provider expansion state and returns the new value for callers that render immediately.
    pub fn toggle_ai_provider_expanded(&mut self, provider_id: impl Into<String>) -> bool {
        let provider_id = provider_id.into();
        let is_expanded = !self
            .expanded_ai_providers
            .get(&provider_id)
            .copied()
            .unwrap_or(true);
        self.expanded_ai_providers.insert(provider_id, is_expanded);
        is_expanded
    }

    /// Clears all AI settings expansion state for a provider that was removed.
    pub fn remove_ai_provider_page_state(&mut self, provider_id: &str) {
        self.expanded_ai_providers.remove(provider_id);
        self.expanded_ai_provider_models.remove(provider_id);
        self.expanded_ai_context_providers.remove(provider_id);
    }

    /// Marks whether the background image cache poll has already been scheduled.
    pub fn set_background_cache_poll_scheduled(&mut self, is_scheduled: bool) {
        self.background_cache_poll_scheduled = is_scheduled;
    }

    /// Toggles a context-window provider panel.
    pub fn toggle_ai_context_provider(&mut self, provider_id: impl Into<String>) {
        let provider_id = provider_id.into();
        if !self
            .expanded_ai_context_providers
            .insert(provider_id.clone())
        {
            self.expanded_ai_context_providers.remove(&provider_id);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiSettingsSection {
    ProviderSettings,
    ToolUse,
    ContextWindows,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_removal_clears_related_expansion_state() {
        let mut model = SettingsPageModel::default();
        let provider_id = "provider-a".to_string();
        model
            .expanded_ai_providers
            .insert(provider_id.clone(), false);
        model
            .expanded_ai_provider_models
            .insert(provider_id.clone());
        model
            .expanded_ai_context_providers
            .insert(provider_id.clone());
        model.remove_ai_provider_page_state(&provider_id);

        assert!(!model.expanded_ai_providers.contains_key(&provider_id));
        assert!(!model.expanded_ai_provider_models.contains(&provider_id));
        assert!(!model.expanded_ai_context_providers.contains(&provider_id));
    }

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
