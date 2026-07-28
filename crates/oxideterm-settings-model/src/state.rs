// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Persistent page-model state for settings surfaces.
//!
//! This is intentionally free of GPUI handles, anchors, list state, focus
//! handles, and rendered element caches. The app owns those view concerns while
//! this model owns settings-page business state and drafts.

use std::collections::{HashMap, HashSet};

use crate::{
    AiSettingsPage, SettingsInput, SettingsKeybindingScopeFilter, SettingsTab,
    TerminalSettingsPage, ThemeEditorState,
};

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
    pub keybinding_scope_filter: SettingsKeybindingScopeFilter,
    pub previous_keybinding_scope_filter: SettingsKeybindingScopeFilter,
    pub settings_reset_confirm_open: bool,
    pub ai_new_provider_type: String,
    pub ai_provider_settings_expanded: bool,
    pub ai_tool_use_expanded: bool,
    pub ai_context_windows_expanded: bool,
    pub expanded_ai_providers: HashMap<String, bool>,
    pub expanded_ai_provider_models: HashSet<String>,
    pub expanded_ai_context_providers: HashSet<String>,
    pub keybinding_recording_action_id: Option<String>,
    pub keybinding_conflict_action_ids: Vec<String>,
    pub keybinding_search_query: String,
    pub keybinding_reset_all_confirm_open: bool,
    pub legal_notice_open: bool,
    pub theme_editor: Option<ThemeEditorState>,
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
            keybinding_scope_filter: SettingsKeybindingScopeFilter::All,
            previous_keybinding_scope_filter: SettingsKeybindingScopeFilter::All,
            settings_reset_confirm_open: false,
            ai_new_provider_type: "openai_compatible".to_string(),
            ai_provider_settings_expanded: true,
            ai_tool_use_expanded: true,
            ai_context_windows_expanded: true,
            expanded_ai_providers: HashMap::new(),
            expanded_ai_provider_models: HashSet::new(),
            expanded_ai_context_providers: HashSet::new(),
            keybinding_recording_action_id: None,
            keybinding_conflict_action_ids: Vec::new(),
            keybinding_search_query: String::new(),
            keybinding_reset_all_confirm_open: false,
            legal_notice_open: false,
            theme_editor: None,
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

    /// Selects the keybinding scope filter used by the keybindings page.
    pub fn set_keybinding_scope_filter(&mut self, filter: SettingsKeybindingScopeFilter) {
        if self.keybinding_scope_filter != filter {
            self.previous_keybinding_scope_filter = self.keybinding_scope_filter;
        }
        self.keybinding_scope_filter = filter;
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

    /// Starts recording a keybinding and clears stale conflict hints.
    pub fn start_keybinding_recording(&mut self, action_id: impl Into<String>) {
        self.keybinding_recording_action_id = Some(action_id.into());
        self.keybinding_conflict_action_ids.clear();
    }

    /// Stops recording a keybinding and clears conflict hints.
    pub fn stop_keybinding_recording(&mut self) {
        self.keybinding_recording_action_id = None;
        self.keybinding_conflict_action_ids.clear();
    }

    /// Replaces the current keybinding conflict list.
    pub fn set_keybinding_conflicts(&mut self, conflicts: Vec<String>) {
        self.keybinding_conflict_action_ids = conflicts;
    }

    /// Updates the keybinding search draft.
    pub fn set_keybinding_search_query(&mut self, query: impl Into<String>) {
        self.keybinding_search_query = query.into();
    }

    /// Opens or closes the reset-all keybindings confirmation.
    pub fn set_keybinding_reset_all_confirm_open(&mut self, is_open: bool) {
        self.keybinding_reset_all_confirm_open = is_open;
    }

    /// Installs a new theme editor model.
    pub fn open_theme_editor(&mut self, editor: ThemeEditorState) {
        self.theme_editor = Some(editor);
    }

    /// Closes the active theme editor.
    pub fn close_theme_editor(&mut self) {
        self.theme_editor = None;
    }

    /// Mutates the active theme editor when it exists.
    pub fn update_theme_editor(&mut self, update: impl FnOnce(&mut ThemeEditorState)) {
        if let Some(editor) = self.theme_editor.as_mut() {
            update(editor);
        }
    }

    /// Returns the draft text for inputs whose state is owned by the settings page model.
    pub fn page_input_value(&self, input: SettingsInput) -> Option<String> {
        let value = match input {
            SettingsInput::KeybindingSearch => self.keybinding_search_query.clone(),
            SettingsInput::CustomThemeName => self
                .theme_editor
                .as_ref()
                .map(|editor| editor.name.clone())
                .unwrap_or_default(),
            SettingsInput::CustomThemeTerminalColor(index) => self
                .theme_editor
                .as_ref()
                .and_then(|editor| editor.terminal_colors.get(index).cloned())
                .unwrap_or_default(),
            SettingsInput::CustomThemeUiColor(index) => self
                .theme_editor
                .as_ref()
                .and_then(|editor| editor.ui_colors.get(index).cloned())
                .unwrap_or_default(),
            _ => return None,
        };
        Some(value)
    }

    /// Applies a draft to inputs whose state is page-local rather than persisted settings.
    pub fn apply_page_input_draft(&mut self, input: SettingsInput, draft: &str) -> bool {
        match input {
            SettingsInput::KeybindingSearch => {
                self.set_keybinding_search_query(draft.to_string());
                true
            }
            SettingsInput::CustomThemeName => {
                self.update_theme_editor(|editor| editor.name = draft.to_string());
                true
            }
            SettingsInput::CustomThemeTerminalColor(index) => {
                self.apply_theme_editor_color_slot(index, draft, true)
            }
            SettingsInput::CustomThemeUiColor(index) => {
                self.apply_theme_editor_color_slot(index, draft, false)
            }
            _ => false,
        }
    }

    fn apply_theme_editor_color_slot(
        &mut self,
        index: usize,
        draft: &str,
        is_terminal_color: bool,
    ) -> bool {
        let Some(editor) = self.theme_editor.as_mut() else {
            return true;
        };
        // Color text remains intentionally unvalidated during typing so partial
        // hex or rgb() values can be edited without the view fighting the user.
        let colors = if is_terminal_color {
            &mut editor.terminal_colors
        } else {
            &mut editor.ui_colors
        };
        if let Some(slot) = colors.get_mut(index) {
            *slot = draft.trim().to_string();
        }
        true
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
    fn keybinding_recording_resets_conflicts() {
        let mut model = SettingsPageModel::default();
        model
            .keybinding_conflict_action_ids
            .push("copy".to_string());

        model.start_keybinding_recording("paste");

        assert_eq!(
            model.keybinding_recording_action_id.as_deref(),
            Some("paste")
        );
        assert!(model.keybinding_conflict_action_ids.is_empty());
    }

    #[test]
    fn page_routing_state_lives_in_settings_model() {
        let mut model = SettingsPageModel::default();

        model.set_active_tab(SettingsTab::Keybindings);
        model.set_terminal_page(TerminalSettingsPage::Awareness);
        model.set_keybinding_scope_filter(SettingsKeybindingScopeFilter::Terminal);

        assert_eq!(model.active_tab, SettingsTab::Keybindings);
        assert_eq!(model.terminal_page, TerminalSettingsPage::Awareness);
        assert_eq!(model.previous_terminal_page, TerminalSettingsPage::Display);
        assert_eq!(
            model.keybinding_scope_filter,
            SettingsKeybindingScopeFilter::Terminal
        );
        assert_eq!(
            model.previous_keybinding_scope_filter,
            SettingsKeybindingScopeFilter::All
        );

        model.set_keybinding_scope_filter(SettingsKeybindingScopeFilter::Terminal);
        assert_eq!(
            model.previous_keybinding_scope_filter,
            SettingsKeybindingScopeFilter::All
        );

        model.set_keybinding_scope_filter(SettingsKeybindingScopeFilter::Split);
        assert_eq!(
            model.previous_keybinding_scope_filter,
            SettingsKeybindingScopeFilter::Terminal
        );
    }

    #[test]
    fn page_owned_input_drafts_apply_inside_settings_model() {
        let mut model = SettingsPageModel::default();

        assert!(model.apply_page_input_draft(SettingsInput::KeybindingSearch, "terminal"));

        assert_eq!(model.keybinding_search_query, "terminal");
        assert_eq!(
            model
                .page_input_value(SettingsInput::KeybindingSearch)
                .as_deref(),
            Some("terminal")
        );
    }
}
