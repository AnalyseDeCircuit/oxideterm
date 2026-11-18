use super::*;
use oxideterm_gpui_settings_view::{
    appearance_page::AppearancePageData,
    components::SettingsControls,
    knowledge_page::KnowledgeSection,
    mcp_page::McpSettingsFrame,
    surface::{
        KnowledgePageData, McpTargetLabel, SettingsDocumentText, SettingsPageAction,
        SettingsPageWindowAction, SettingsSurfaceBindings, SettingsSurfaceHost,
        SettingsSurfaceRequest, TriggerPageRuntime,
    },
};

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_surface_bindings(
        cx: &Context<Self>,
    ) -> SettingsSurfaceBindings {
        SettingsSurfaceBindings::for_host(cx.entity().downgrade())
    }
}

impl SettingsSurfaceHost for WorkspaceApp {
    fn ai_mcp(&self) -> Entity<oxideterm_gpui_settings_view::ai::mcp::AiMcpSettingsView> {
        self.settings_views.ai_mcp.clone()
    }
    fn ai_providers(&self) -> Entity<oxideterm_gpui_settings_view::ai::providers::AiProviderSettingsView> {
        self.settings_views.ai_providers.clone()
    }
    fn ai_skills(&self) -> oxideterm_gpui_settings_view::ai::AiSkillData {
        let registry = self.ai.skills.registry.read();
        oxideterm_gpui_settings_view::ai::AiSkillData {
            records: registry.records().cloned().collect(),
            diagnostics: registry.diagnostics().to_vec(),
        }
    }
    fn settings_state(&self) -> &Entity<SettingsWorkspaceEntity> {
        &self.settings_workspace
    }
    fn controls(&self, owner: Entity<Self>, cx: &App) -> SettingsControls<'_> {
        self.settings_controls_for(owner, cx)
    }
    fn document_text(&self, owner: Entity<Self>, cx: &App) -> SettingsDocumentText {
        self.settings_document_text(owner, cx)
    }
    fn scroll_handle(&self, key: &str) -> ScrollHandle {
        self.selectable_text_scroll_handle(key)
    }
    fn connections(&self) -> &ConnectionStore {
        &self.connection_store
    }
    fn settings_path(&self) -> &Path {
        self.settings_store.path()
    }
    fn footer_focus(&self) -> Option<ConfirmDialogAction> {
        self.standard_confirm_focus()
    }
    fn layout(&self, cx: &App) -> (SettingsDynamicSectionCounts, Vec<u64>, Rgba) {
        (
            self.settings_dynamic_section_counts(cx),
            self.settings_section_list_signatures(cx),
            self.workspace_chrome_divider(),
        )
    }
    fn data_directory(&self) -> oxideterm_settings::DataDirectoryInfo {
        self.settings_data_directory_info()
    }
    fn history_failed(&self) -> bool {
        self.terminal_history_store.failed()
    }
    fn command_specs(&self) -> (String, String) {
        (
            super::terminal_command_bar::completion::terminal_command_specs_path(
                self.settings_store.path(),
            )
            .display()
            .to_string(),
            self.terminal_command_specs_summary(),
        )
    }
    fn local_shells(&self) -> Vec<ShellInfo> {
        self.effective_local_shells_for_settings(self.settings_store.settings())
    }
    fn focused_input(&self) -> Option<(SettingsInput, &str)> {
        self.focused_settings_input
            .map(|input| (input, self.settings_input_draft.as_str()))
    }
    fn keybindings(&self, cx: &App) -> Vec<crate::keybindings::ActionDefinition> {
        self.keybinding_definitions(cx)
    }
    fn app_lock(&self) -> (bool, bool, Option<String>) {
        self.settings_app_lock_state()
    }
    fn appearance(&self, cx: &App) -> AppearancePageData {
        self.appearance_page_data(cx)
    }
    fn background_is_bundled(&self) -> Rc<dyn Fn(&Path) -> bool> {
        let path = self.settings_store.path().to_path_buf();
        Rc::new(move |candidate| is_bundled_workspace_background(&path, candidate))
    }
    fn remote_shell(&self, cx: &App) -> (Option<String>, RemoteShellIntegrationCardSnapshot) {
        let node = self.active_ssh_node_id.as_ref();
        (
            node.and_then(|node| self.ssh_nodes.get(node))
                .map(|node| node.title.clone()),
            self.workspace_runtime
                .read(cx)
                .remote_shell_integration_card_snapshot(node),
        )
    }
    fn triggers(&self, owner: gpui::WeakEntity<Self>) -> TriggerPageRuntime<'_> {
        TriggerPageRuntime {
            store: &self.connection_store,
            pane_exists: Rc::new({
                let host = self.tab_host.clone();
                move |pane, cx| host.read(cx).panes().contains_key(&pane)
            }),
            applies: Rc::new({
                let weak = owner.clone();
                move |pane, trigger, cx| {
                    weak.upgrade().is_some_and(|app| {
                        app.read(cx)
                            .terminal_trigger_applies_to_pane(pane, trigger, cx)
                    })
                }
            }),
            enabled: Rc::new({
                let weak = owner.clone();
                move |pane, trigger, cx| {
                    weak.upgrade().is_some_and(|app| {
                        app.read(cx)
                            .terminal_trigger_effective_enabled_for_pane(pane, trigger, cx)
                    })
                }
            }),
            command_label: Rc::new({
                let terminal = self.terminal.clone();
                move |id, cx| {
                    terminal
                        .read(cx)
                        .quick_commands
                        .store
                        .commands
                        .iter()
                        .find(|command| command.id == id)
                        .map(|command| command.name.clone())
                }
            }),
        }
    }
    fn public_mcp(&self, owner: gpui::WeakEntity<Self>) -> (McpSettingsFrame<'_>, McpTargetLabel) {
        (
            oxideterm_gpui_settings_view::mcp_page::McpSettingsFrame {
                endpoint: self.public_mcp.endpoint_url(),
                startup_error: self.public_mcp.startup_error(),
                clients: self.public_mcp.clients(),
                approvals: self.public_mcp.approvals(),
                port_draft: self.public_mcp.port_draft(),
                credential_ready: self.public_mcp.revealed_credential().is_some(),
            },
            Rc::new({
                let weak = owner;
                move |client, target, cx| {
                    weak.upgrade()
                        .map(|owner| owner.read(cx).public_mcp_target_label(client, target, cx))
                        .unwrap_or_else(|| target.to_owned())
                }
            }),
        )
    }
    fn knowledge<'a>(
        &'a self,
        owner: gpui::WeakEntity<Self>,
        cx: &'a App,
    ) -> KnowledgePageData<'a> {
        self.knowledge_settings_page_data(owner, cx)
    }
    fn knowledge_available(&self, cx: &App) -> bool {
        self.knowledge_has_selected_collection(cx)
    }
    fn terminal_font(
        &self,
        cx: &App,
    ) -> oxideterm_gpui_settings_view::components::TerminalPreviewFont {
        Self::settings_terminal_preview_font(self.settings_store.settings(), cx)
    }
    fn prepare_portable(&mut self, cx: &mut Context<Self>) {
        self.ensure_portable_settings_snapshot(cx);
    }
    fn prepare_knowledge(
        &mut self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> KnowledgeSection {
        self.prepare_knowledge_settings(section, cx)
    }
    fn active_theme_preview(&self, cx: &App) -> Option<String> {
        self.settings_views
            .theme_picker
            .read(cx)
            .preview()
            .map(str::to_owned)
    }
    fn keybinding_note(&mut self, note: String, color: u32, cx: &mut Context<Self>) -> AnyElement {
        self.render_selectable_text_scoped(
            "settings-keybindings-note",
            "keybindings",
            note,
            color,
            cx,
        )
    }
    fn on_scroll(&mut self, cx: &mut Context<Self>) {
        self.pause_settings_caret_blink_during_scroll(cx);
        if self.open_settings_select.is_some() {
            self.close_settings_select();
            self.clear_settings_select_anchors();
            cx.notify();
        }
    }
    fn page_action(&mut self, action: SettingsPageAction<'_>, cx: &mut Context<Self>) {
        match action {
            SettingsPageAction::AiSkills(request) => {
                use oxideterm_gpui_settings_view::ai::AiSkillAction;
                if let AiSkillAction::EditAndRefresh(edit) = request {
                    self.edit_settings(edit, cx);
                }
                self.refresh_ai_skill_registry();
                self.settings_views.surface.update(cx, |_, cx| cx.notify());
            }
            SettingsPageAction::Connection(request) => {
                self.handle_connection_page_request(request, cx)
            }
            SettingsPageAction::Credential(request) => self.handle_credential_request(request, cx),
            SettingsPageAction::Proxy(request) => self.handle_proxy_request(request, cx),
            SettingsPageAction::Help(request) => self.handle_help_request(request, cx),
            SettingsPageAction::Appearance(request) => self.handle_appearance_request(request, cx),
            SettingsPageAction::EditBackground(edit) => self.edit_background_style(edit, cx),
            SettingsPageAction::Keybindings(request) => self.handle_keybinding_request(request, cx),
            SettingsPageAction::Portable(request) => {
                self.handle_portable_settings_action(request, cx)
            }
            SettingsPageAction::RemoteShell(request) => {
                self.handle_remote_shell_request(request, cx)
            }
            SettingsPageAction::Mcp(request) => self.handle_mcp_settings_request(request, cx),
            SettingsPageAction::ToggleFocusHandoff(name) => {
                self.toggle_focus_handoff_command(name, cx)
            }
            SettingsPageAction::OpenKnownHostsFile(path) => {
                if let Ok(url) = url::Url::from_file_path(path) {
                    cx.open_url(url.as_str());
                }
            }
        }
    }
    fn window_action(
        &mut self,
        action: SettingsPageWindowAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            SettingsPageWindowAction::General(request) => {
                self.handle_general_settings_request(request, window, cx)
            }
            SettingsPageWindowAction::Highlight(request) => {
                self.handle_highlight_request(request, window, cx)
            }
            SettingsPageWindowAction::Knowledge(request, pos) => {
                self.handle_knowledge_settings_request(request, pos, window, cx)
            }
            SettingsPageWindowAction::Background(request) => {
                self.handle_background_request(request, window, cx)
            }
            SettingsPageWindowAction::AppLock(dialog) => {
                self.open_app_lock_dialog(dialog, false, window, cx)
            }
            SettingsPageWindowAction::Trigger(request) => {
                self.handle_terminal_trigger_request(request, window, cx)
            }
            SettingsPageWindowAction::LocalTerminal(request) => {
                self.handle_local_terminal_settings_request(request, window, cx)
            }
        }
    }
    fn surface_request(
        &mut self,
        request: SettingsSurfaceRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            SettingsSurfaceRequest::ToggleSearch => self.toggle_settings_search(window, cx),
            SettingsSurfaceRequest::ClearSearch => {
                self.settings_workspace
                    .update(cx, |settings, cx| settings.clear_settings_search_query(cx));
                self.clear_ime_selection();
                self.show_active_input_caret(cx);
            }
            SettingsSurfaceRequest::SearchResult(result) => {
                self.activate_settings_search_result(result, cx)
            }
            SettingsSurfaceRequest::OpenNavigationEditor => {
                self.open_settings_navigation_editor(cx)
            }
            SettingsSurfaceRequest::SelectedTab(tab) => {
                let had_select = self.open_settings_select.is_some();
                self.close_settings_select();
                self.focused_settings_input = None;
                self.settings_workspace
                    .update(cx, |settings, _| settings.set_slider_drag(None));
                self.clear_ime_selection();
                self.refresh_settings_tab_services(tab, cx);
                if had_select {
                    cx.notify();
                }
            }
            SettingsSurfaceRequest::Tooltip(text, Some(position)) => {
                self.queue_workspace_tooltip(
                    text.clone(),
                    text,
                    f32::from(position.x) + 12.0,
                    f32::from(position.y) + 16.0,
                    cx,
                );
            }
            SettingsSurfaceRequest::Tooltip(text, None) => self.clear_workspace_tooltip(&text, cx),
        }
    }
}
