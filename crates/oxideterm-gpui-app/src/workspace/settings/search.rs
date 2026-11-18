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
        if result.section == SettingsSectionId::AiToolsToolUse {
            self.settings_workspace.update(cx, |state, cx| {
                state.ai_tool_policy_expanded = true;
                cx.notify();
            });
        }
        if result.section == SettingsSectionId::AiContextModelWindows {
            let providers = ai_provider_views(self.settings_store.settings());
            self.settings_workspace.update(cx, |state, cx| {
                state.ai_context_windows.expanded = true;
                state.ai_context_windows.expanded_providers.extend(providers.into_iter().map(|provider| provider.id));
                cx.notify();
            });
        }
        if result.section == SettingsSectionId::AiProvidersSettings {
            let ids = ai_provider_views(self.settings_store.settings()).into_iter().map(|provider| provider.id);
            self.settings_views.ai_providers.update(cx, |view, cx| view.expand_for_search(ids, cx));
        }
        if result.section == SettingsSectionId::KnowledgeEmbeddingConfig {
            self.knowledge_page(cx).update(cx, |ai, cx| {
                ai.expand_knowledge_embedding_config();
                cx.notify();
            });
        }
        let tab = result.tab;
        self.settings_workspace.update(cx, |settings, cx| settings.navigate_to_search_result(&result, cx));
        let dismissed_select = self.open_settings_select.is_some();
        self.close_settings_select();
        self.focused_settings_input = None;
        self.clear_ime_selection();
        self.ime_marked_text = None;
        self.refresh_settings_tab_services(tab, cx);
        self.sync_settings_section_list_state(cx);
        self.settings_workspace.read(cx).section_list.scroll_to_section(
            tab, result.section, self.settings_dynamic_section_counts(cx),
        );
        if dismissed_select {
            cx.notify();
        }
    }
}
