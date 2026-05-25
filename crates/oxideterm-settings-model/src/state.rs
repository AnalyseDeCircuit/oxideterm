// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Persistent page-model state for settings surfaces.
//!
//! This is intentionally free of GPUI handles, anchors, list state, focus
//! handles, and rendered element caches. The app owns those view concerns while
//! this model owns settings-page business state and drafts.

use std::collections::{HashMap, HashSet};

use crate::{
    KnowledgeDeleteConfirm, KnowledgeDeleteTarget, KnowledgeExternalEdit, SettingsInput,
    SettingsKeybindingScopeFilter, SettingsTab, TerminalSettingsPage, ThemeEditorState,
};

#[derive(Clone, Debug)]
pub struct SettingsPageModel {
    pub active_tab: SettingsTab,
    pub terminal_page: TerminalSettingsPage,
    pub keybinding_scope_filter: SettingsKeybindingScopeFilter,
    pub settings_reset_confirm_open: bool,
    pub ai_new_provider_type: String,
    pub ai_provider_settings_expanded: bool,
    pub ai_tool_use_expanded: bool,
    pub ai_context_windows_expanded: bool,
    pub ai_model_reasoning_expanded: bool,
    pub expanded_ai_providers: HashMap<String, bool>,
    pub expanded_ai_provider_models: HashSet<String>,
    pub expanded_ai_context_providers: HashSet<String>,
    pub expanded_ai_model_reasoning_providers: HashSet<String>,
    pub knowledge_selected_collection_id: Option<String>,
    pub knowledge_create_dialog_open: bool,
    pub knowledge_new_document_dialog_open: bool,
    pub knowledge_embedding_config_expanded: bool,
    pub knowledge_new_collection_name: String,
    pub knowledge_new_document_title: String,
    pub knowledge_new_document_format: String,
    pub knowledge_import_progress: Option<(usize, usize)>,
    pub knowledge_embedding_progress: Option<(usize, usize)>,
    pub knowledge_reindex_progress: Option<(usize, usize)>,
    pub knowledge_delete_confirm: Option<KnowledgeDeleteConfirm>,
    pub knowledge_external_edit: Option<KnowledgeExternalEdit>,
    pub knowledge_error: Option<String>,
    pub show_ai_enable_confirm: bool,
    pub ai_provider_key_remove_confirm: Option<(usize, String)>,
    pub ai_provider_remove_confirm: Option<(String, String)>,
    pub keybinding_recording_action_id: Option<String>,
    pub keybinding_conflict_action_ids: Vec<String>,
    pub keybinding_search_query: String,
    pub keybinding_reset_all_confirm_open: bool,
    pub theme_editor: Option<ThemeEditorState>,
    pub background_blur_preview: Option<i64>,
    pub background_blur_commit_generation: u64,
    pub background_cache_poll_scheduled: bool,
    pub settings_connection_new_group: String,
    pub settings_selected_ssh_hosts: HashSet<String>,
    pub settings_connection_status: Option<String>,
}

impl Default for SettingsPageModel {
    fn default() -> Self {
        Self {
            active_tab: SettingsTab::General,
            terminal_page: TerminalSettingsPage::Display,
            keybinding_scope_filter: SettingsKeybindingScopeFilter::All,
            settings_reset_confirm_open: false,
            ai_new_provider_type: "openai_compatible".to_string(),
            ai_provider_settings_expanded: true,
            ai_tool_use_expanded: true,
            ai_context_windows_expanded: true,
            ai_model_reasoning_expanded: false,
            expanded_ai_providers: HashMap::new(),
            expanded_ai_provider_models: HashSet::new(),
            expanded_ai_context_providers: HashSet::new(),
            expanded_ai_model_reasoning_providers: HashSet::new(),
            knowledge_selected_collection_id: None,
            knowledge_create_dialog_open: false,
            knowledge_new_document_dialog_open: false,
            knowledge_embedding_config_expanded: false,
            knowledge_new_collection_name: String::new(),
            knowledge_new_document_title: String::new(),
            knowledge_new_document_format: "markdown".to_string(),
            knowledge_import_progress: None,
            knowledge_embedding_progress: None,
            knowledge_reindex_progress: None,
            knowledge_delete_confirm: None,
            knowledge_external_edit: None,
            knowledge_error: None,
            show_ai_enable_confirm: false,
            ai_provider_key_remove_confirm: None,
            ai_provider_remove_confirm: None,
            keybinding_recording_action_id: None,
            keybinding_conflict_action_ids: Vec::new(),
            keybinding_search_query: String::new(),
            keybinding_reset_all_confirm_open: false,
            theme_editor: None,
            background_blur_preview: None,
            background_blur_commit_generation: 0,
            background_cache_poll_scheduled: false,
            settings_connection_new_group: String::new(),
            settings_selected_ssh_hosts: HashSet::new(),
            settings_connection_status: None,
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
        self.terminal_page = page;
    }

    /// Selects the keybinding scope filter used by the keybindings page.
    pub fn set_keybinding_scope_filter(&mut self, filter: SettingsKeybindingScopeFilter) {
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
            AiSettingsSection::ModelReasoning => {
                self.ai_model_reasoning_expanded = !self.ai_model_reasoning_expanded;
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
        self.expanded_ai_model_reasoning_providers
            .remove(provider_id);
    }

    /// Opens or closes the AI enable confirmation modal.
    pub fn set_ai_enable_confirm_open(&mut self, is_open: bool) {
        self.show_ai_enable_confirm = is_open;
    }

    /// Stores the pending provider key removal target.
    pub fn request_ai_provider_key_remove(
        &mut self,
        provider_index: usize,
        key_label: impl Into<String>,
    ) {
        self.ai_provider_key_remove_confirm = Some((provider_index, key_label.into()));
    }

    /// Clears the pending provider key removal target.
    pub fn clear_ai_provider_key_remove(&mut self) {
        self.ai_provider_key_remove_confirm = None;
    }

    /// Takes the pending provider key removal target for execution.
    pub fn take_ai_provider_key_remove(&mut self) -> Option<(usize, String)> {
        self.ai_provider_key_remove_confirm.take()
    }

    /// Stores the pending provider removal target.
    pub fn request_ai_provider_remove(
        &mut self,
        provider_id: impl Into<String>,
        provider_name: impl Into<String>,
    ) {
        self.ai_provider_remove_confirm = Some((provider_id.into(), provider_name.into()));
    }

    /// Clears the pending provider removal target.
    pub fn clear_ai_provider_remove(&mut self) {
        self.ai_provider_remove_confirm = None;
    }

    /// Takes the pending provider removal target for execution.
    pub fn take_ai_provider_remove(&mut self) -> Option<(String, String)> {
        self.ai_provider_remove_confirm.take()
    }

    /// Opens the create-collection dialog while preserving any draft name already typed.
    pub fn open_knowledge_create_dialog(&mut self) {
        self.knowledge_create_dialog_open = true;
    }

    /// Closes the create-collection dialog and clears its draft.
    pub fn close_knowledge_create_dialog(&mut self) {
        self.knowledge_create_dialog_open = false;
        self.knowledge_new_collection_name.clear();
    }

    /// Hides the create-collection dialog while preserving its draft for a later retry.
    pub fn hide_knowledge_create_dialog(&mut self) {
        self.knowledge_create_dialog_open = false;
    }

    /// Opens the new-document dialog while preserving any draft title already typed.
    pub fn open_knowledge_new_document_dialog(&mut self) {
        self.knowledge_new_document_dialog_open = true;
    }

    /// Closes the new-document dialog and clears its draft title.
    pub fn close_knowledge_new_document_dialog(&mut self) {
        self.knowledge_new_document_dialog_open = false;
        self.knowledge_new_document_title.clear();
    }

    /// Hides the new-document dialog while preserving its draft for a later retry.
    pub fn hide_knowledge_new_document_dialog(&mut self) {
        self.knowledge_new_document_dialog_open = false;
    }

    /// Records a successful collection creation and selects the created collection.
    pub fn finish_knowledge_collection_create(&mut self, collection_id: impl Into<String>) {
        self.knowledge_selected_collection_id = Some(collection_id.into());
        self.knowledge_new_collection_name.clear();
        self.knowledge_error = None;
    }

    /// Records a successful document creation and clears the document title draft.
    pub fn finish_knowledge_document_create(&mut self) {
        self.knowledge_new_document_title.clear();
        self.knowledge_error = None;
    }

    /// Selects a knowledge collection from the page list.
    pub fn select_knowledge_collection(&mut self, collection_id: impl Into<String>) {
        self.knowledge_selected_collection_id = Some(collection_id.into());
    }

    /// Updates the create-collection draft from the shared settings input.
    pub fn set_knowledge_collection_name(&mut self, name: impl Into<String>) {
        self.knowledge_new_collection_name = name.into();
    }

    /// Updates the new-document title draft from the shared settings input.
    pub fn set_knowledge_document_title(&mut self, title: impl Into<String>) {
        self.knowledge_new_document_title = title.into();
    }

    /// Selects the document format used when creating a blank knowledge document.
    pub fn set_knowledge_document_format(&mut self, format: impl Into<String>) {
        self.knowledge_new_document_format = format.into();
    }

    /// Removes a selected collection if it matches the deleted collection.
    pub fn clear_deleted_knowledge_collection(&mut self, collection_id: &str) {
        if self.knowledge_selected_collection_id.as_deref() == Some(collection_id) {
            self.knowledge_selected_collection_id = None;
        }
        self.knowledge_external_edit = None;
        self.knowledge_error = None;
    }

    /// Stores a translated or backend-provided knowledge page error.
    pub fn set_knowledge_error(&mut self, error: impl Into<String>) {
        self.knowledge_error = Some(error.into());
    }

    /// Clears the knowledge page error after a successful action.
    pub fn clear_knowledge_error(&mut self) {
        self.knowledge_error = None;
    }

    /// Builds and stores a delete confirmation for a knowledge collection.
    pub fn request_delete_collection(&mut self, id: impl Into<String>, name: impl Into<String>) {
        self.knowledge_delete_confirm = Some(KnowledgeDeleteConfirm {
            target: KnowledgeDeleteTarget::Collection,
            id: id.into(),
            name: name.into(),
        });
    }

    /// Builds and stores a delete confirmation for a knowledge document.
    pub fn request_delete_document(&mut self, id: impl Into<String>, name: impl Into<String>) {
        self.knowledge_delete_confirm = Some(KnowledgeDeleteConfirm {
            target: KnowledgeDeleteTarget::Document,
            id: id.into(),
            name: name.into(),
        });
    }

    /// Clears the active knowledge delete confirmation.
    pub fn clear_knowledge_delete_confirm(&mut self) {
        self.knowledge_delete_confirm = None;
    }

    /// Takes the active knowledge delete confirmation for command execution.
    pub fn take_knowledge_delete_confirm(&mut self) -> Option<KnowledgeDeleteConfirm> {
        self.knowledge_delete_confirm.take()
    }

    /// Records an external edit file currently being watched by the settings page.
    pub fn set_knowledge_external_edit(&mut self, edit: KnowledgeExternalEdit) {
        self.knowledge_external_edit = Some(edit);
        self.knowledge_error = None;
    }

    /// Clears the active external edit without touching other knowledge state.
    pub fn clear_knowledge_external_edit(&mut self) {
        self.knowledge_external_edit = None;
    }

    /// Starts a knowledge import progress counter and clears stale errors.
    pub fn start_knowledge_import(&mut self, total: usize) {
        self.knowledge_import_progress = Some((0, total));
        self.knowledge_error = None;
    }

    /// Updates the knowledge import progress counter.
    pub fn update_knowledge_import(&mut self, current: usize, total: usize) {
        self.knowledge_import_progress = Some((current, total));
    }

    /// Finishes a knowledge import and clears stale errors.
    pub fn finish_knowledge_import(&mut self) {
        self.knowledge_import_progress = None;
        self.knowledge_error = None;
    }

    /// Starts embedding progress and ensures the embedding controls are visible.
    pub fn start_knowledge_embedding(&mut self, total: usize) {
        self.knowledge_embedding_config_expanded = true;
        self.knowledge_embedding_progress = Some((0, total));
        self.knowledge_error = None;
    }

    /// Expands embedding configuration after a validation failure or user action.
    pub fn expand_knowledge_embedding_config(&mut self) {
        self.knowledge_embedding_config_expanded = true;
        self.knowledge_error = None;
    }

    /// Toggles whether embedding configuration details are visible.
    pub fn toggle_knowledge_embedding_config(&mut self) {
        self.knowledge_embedding_config_expanded = !self.knowledge_embedding_config_expanded;
    }

    /// Updates the embedding progress counter.
    pub fn update_knowledge_embedding(&mut self, current: usize, total: usize) {
        self.knowledge_embedding_progress = Some((current, total));
    }

    /// Finishes embedding progress and clears stale errors.
    pub fn finish_knowledge_embedding(&mut self) {
        self.knowledge_embedding_progress = None;
        self.knowledge_error = None;
    }

    /// Starts reindex progress and clears stale errors.
    pub fn start_knowledge_reindex(&mut self) {
        self.knowledge_reindex_progress = Some((0, 0));
        self.knowledge_error = None;
    }

    /// Updates the reindex progress counter.
    pub fn update_knowledge_reindex(&mut self, current: usize, total: usize) {
        self.knowledge_reindex_progress = Some((current, total));
    }

    /// Finishes reindex progress and clears stale errors.
    pub fn finish_knowledge_reindex(&mut self) {
        self.knowledge_reindex_progress = None;
        self.knowledge_error = None;
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
            SettingsInput::ConnectionNewGroup => self.settings_connection_new_group.clone(),
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
            SettingsInput::KnowledgeCollectionName => self.knowledge_new_collection_name.clone(),
            SettingsInput::KnowledgeDocumentTitle => self.knowledge_new_document_title.clone(),
            _ => return None,
        };
        Some(value)
    }

    /// Applies a draft to inputs whose state is page-local rather than persisted settings.
    pub fn apply_page_input_draft(&mut self, input: SettingsInput, draft: &str) -> bool {
        match input {
            SettingsInput::ConnectionNewGroup => {
                self.set_connection_new_group(draft.to_string());
                true
            }
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
            SettingsInput::KnowledgeCollectionName => {
                self.set_knowledge_collection_name(draft.to_string());
                true
            }
            SettingsInput::KnowledgeDocumentTitle => {
                self.set_knowledge_document_title(draft.to_string());
                true
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

    /// Updates the debounced background blur preview and returns the commit generation to schedule.
    pub fn update_background_blur_preview(
        &mut self,
        persisted_value: i64,
        preview_value: i64,
    ) -> Option<u64> {
        if self.background_blur_preview == Some(preview_value)
            || (self.background_blur_preview.is_none() && persisted_value == preview_value)
        {
            return None;
        }
        self.background_blur_preview = Some(preview_value);
        self.background_blur_commit_generation =
            self.background_blur_commit_generation.wrapping_add(1);
        Some(self.background_blur_commit_generation)
    }

    /// Takes a pending background blur preview only when its debounce generation is current.
    pub fn take_background_blur_preview(&mut self, generation: u64) -> Option<i64> {
        if self.background_blur_commit_generation != generation {
            return None;
        }
        self.background_blur_preview.take()
    }

    /// Marks whether the background image cache poll has already been scheduled.
    pub fn set_background_cache_poll_scheduled(&mut self, is_scheduled: bool) {
        self.background_cache_poll_scheduled = is_scheduled;
    }

    /// Toggles one SSH host selection and returns whether it is now selected.
    pub fn toggle_ssh_host_selection(&mut self, alias: impl Into<String>) -> bool {
        let alias = alias.into();
        if self.settings_selected_ssh_hosts.insert(alias.clone()) {
            true
        } else {
            self.settings_selected_ssh_hosts.remove(&alias);
            false
        }
    }

    /// Clears all selected SSH hosts.
    pub fn clear_ssh_host_selection(&mut self) {
        self.settings_selected_ssh_hosts.clear();
    }

    /// Replaces the selected SSH host set.
    pub fn set_selected_ssh_hosts(&mut self, hosts: HashSet<String>) {
        self.settings_selected_ssh_hosts = hosts;
    }

    /// Removes one selected SSH host after import or filtering.
    pub fn remove_selected_ssh_host(&mut self, alias: &str) {
        self.settings_selected_ssh_hosts.remove(alias);
    }

    /// Updates the connection import status shown on the settings page.
    pub fn set_connection_status(&mut self, status: Option<String>) {
        self.settings_connection_status = status;
    }

    /// Updates the connection group draft from the shared settings input.
    pub fn set_connection_new_group(&mut self, group: impl Into<String>) {
        self.settings_connection_new_group = group.into();
    }

    /// Clears the connection group draft after a successful group create.
    pub fn clear_connection_new_group(&mut self) {
        self.settings_connection_new_group.clear();
    }

    /// Toggles a model reasoning provider panel.
    pub fn toggle_ai_model_reasoning_provider(&mut self, provider_id: impl Into<String>) {
        let provider_id = provider_id.into();
        if !self
            .expanded_ai_model_reasoning_providers
            .insert(provider_id.clone())
        {
            self.expanded_ai_model_reasoning_providers
                .remove(&provider_id);
        }
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
    ModelReasoning,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_dialog_close_clears_drafts() {
        let mut model = SettingsPageModel::default();
        model.open_knowledge_create_dialog();
        model.knowledge_new_collection_name = "infra".to_string();
        model.close_knowledge_create_dialog();

        assert!(!model.knowledge_create_dialog_open);
        assert!(model.knowledge_new_collection_name.is_empty());
    }

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
        model
            .expanded_ai_model_reasoning_providers
            .insert(provider_id.clone());

        model.remove_ai_provider_page_state(&provider_id);

        assert!(!model.expanded_ai_providers.contains_key(&provider_id));
        assert!(!model.expanded_ai_provider_models.contains(&provider_id));
        assert!(!model.expanded_ai_context_providers.contains(&provider_id));
        assert!(
            !model
                .expanded_ai_model_reasoning_providers
                .contains(&provider_id)
        );
    }

    #[test]
    fn background_blur_preview_debounces_by_generation() {
        let mut model = SettingsPageModel::default();

        let generation = model.update_background_blur_preview(0, 8).unwrap();
        assert_eq!(model.update_background_blur_preview(0, 8), None);

        assert_eq!(model.take_background_blur_preview(generation + 1), None);
        assert_eq!(model.take_background_blur_preview(generation), Some(8));
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
        model.set_terminal_page(TerminalSettingsPage::History);
        model.set_keybinding_scope_filter(SettingsKeybindingScopeFilter::Terminal);

        assert_eq!(model.active_tab, SettingsTab::Keybindings);
        assert_eq!(model.terminal_page, TerminalSettingsPage::History);
        assert_eq!(
            model.keybinding_scope_filter,
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
