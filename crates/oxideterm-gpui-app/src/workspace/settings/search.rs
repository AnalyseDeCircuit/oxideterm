use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn toggle_settings_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_workspace.read(cx).settings_search_open() {
            self.close_settings_search(cx);
            return;
        }

        self.settings_workspace
            .update(cx, SettingsWorkspaceEntity::open_settings_search);
        self.focus_settings_input(SettingsInput::SettingsSearch, String::new(), cx);
        self.ime_marked_text = None;
        window.focus(&self.focus_handle, cx);
    }

    pub(in crate::workspace) fn close_settings_search(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.close_settings_search(true, cx);
        });
        self.clear_ime_selection();
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn activate_first_settings_search_result(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let query = self
            .settings_workspace
            .read(cx)
            .settings_search_query()
            .to_string();
        let Some(result) = settings_search_results(&self.i18n, &query)
            .into_iter()
            .find(|result| {
                result.section != SettingsSectionId::KnowledgeDocuments
                    || self.knowledge_has_selected_collection(cx)
            })
        else {
            return false;
        };
        self.activate_settings_search_result(result, cx);
        true
    }

    pub(super) fn activate_settings_search_result(
        &mut self,
        result: SettingsSearchResult,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity.update(cx, |ai, cx| {
            let section = match result.section {
                SettingsSectionId::AiProvidersSettings => {
                    Some(AiSettingsViewSection::ProviderSettings)
                }
                SettingsSectionId::AiToolsToolUse => Some(AiSettingsViewSection::ToolUse),
                SettingsSectionId::AiContextModelWindows => {
                    Some(AiSettingsViewSection::ContextWindows)
                }
                _ => None,
            };
            if let Some(section) = section {
                if !ai.settings_section_expanded(section) {
                    ai.toggle_settings_section(section, cx);
                }
                for provider in ai_provider_views(self.settings_store.settings()) {
                    if section == AiSettingsViewSection::ProviderSettings {
                        if !ai.settings_provider_expanded(&provider.id, false) {
                            ai.toggle_settings_provider_expanded(&provider.id, false, cx);
                        }
                        if !ai.settings_provider_models_expanded(&provider.id) {
                            ai.toggle_settings_provider_models(&provider.id, cx);
                        }
                    } else if section == AiSettingsViewSection::ContextWindows
                        && !ai.settings_context_provider_expanded(&provider.id)
                    {
                        ai.toggle_settings_context_provider(&provider.id, cx);
                    }
                }
            }
            if result.section == SettingsSectionId::KnowledgeEmbeddingConfig {
                ai.expand_knowledge_embedding_config();
                cx.notify();
            }
        });
        let tab = result.tab;
        if result.section == SettingsSectionId::AppearanceBackground {
            // Reveal collapsible background sub-sections so jumped matches remain visible.
            self.settings_workspace.update(cx, |entity, cx| {
                entity.background_effects_expanded = Some(true);
                entity.background_interactions_expanded = Some(true);
                cx.notify();
            });
        }
        self.settings_workspace.update(cx, |settings, cx| {
            if result.section == SettingsSectionId::TerminalAwarenessTriggers {
                settings.terminal_triggers.pane = None;
                cx.notify();
            }
            settings.set_active_tab(tab, cx);
            if let Some(page) = result.terminal_page {
                settings.set_terminal_page(page, cx);
            }
            if let Some(page) = result.ai_page {
                settings.set_ai_page(page, cx);
            }
            settings.close_settings_search(true, cx);
        });
        let dismissed_select = self.open_settings_select.is_some();
        self.close_settings_select();
        self.focused_settings_input = None;
        self.settings_slider_drag = None;
        self.clear_ime_selection();
        self.ime_marked_text = None;
        if tab == SettingsTab::General {
            self.refresh_cli_companion_status(cx);
            #[cfg(not(target_os = "macos"))]
            self.refresh_launch_at_login_status(cx);
        }
        if tab == SettingsTab::Portable {
            self.refresh_portable_settings_snapshot(true, cx);
        }
        self.sync_settings_section_list_state(cx);
        if let Some(item_ix) = settings_section_list_item_index(
            tab,
            result.section,
            self.settings_dynamic_section_counts(cx),
        ) {
            self.settings_workspace
                .read(cx)
                .section_list
                .state()
                .scroll_to(gpui::ListOffset {
                    item_ix,
                    offset_in_item: px(0.0),
                });
        }
        if dismissed_select {
            cx.notify();
        }
    }
}
