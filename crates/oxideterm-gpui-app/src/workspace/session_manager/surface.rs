use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_session_manager_delete_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .connections
            .session_manager
            .read(cx)
            .delete_confirm
            .is_none()
        {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.cancel_session_manager_delete(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_session_manager_delete(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(in crate::workspace) fn handle_session_manager_basic_dialog_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let (handled, request) = self
            .connections
            .session_manager
            .update(cx, |manager, cx| manager.handle_group_editor_key(event, cx));
        if let Some(request) = request {
            self.dispatch_connections_request(request, None, cx);
        }
        handled
    }

    pub(super) fn cancel_session_group_editor(&mut self, cx: &mut Context<Self>) {
        self.connections
            .session_manager
            .update(cx, SessionManagerState::cancel_group_editor);
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn render_session_manager_surface(
        &mut self,
        _window: &Window,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        self.connections.session_manager_catalog()
    }

    pub(in crate::workspace) fn close_session_manager_sidebar_search(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self
            .connections
            .session_manager
            .update(cx, SessionManagerState::close_sidebar_search)
        {
            return;
        }
        self.begin_disclosure_motion("session-manager-search".into(), false, cx);
        if self.selected_ime_target
            == Some(ime::WorkspaceImeTarget::SessionManager(
                SessionManagerInput::Search,
            ))
        {
            self.selected_ime_target = None;
            self.clear_ime_selection();
            self.ime_marked_text = None;
        }
    }

    pub(in crate::workspace) fn handle_session_manager_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        use oxideterm_gpui_connections::session_manager::CatalogInputAction;
        let tab_active = self
            .keyboard_content_tab(cx)
            .is_some_and(|tab| tab.kind == TabKind::SessionManager);
        let action = self.connections.session_manager.update(cx, |manager, cx| {
            manager.handle_input_key(event, tab_active, cx)
        });
        match action {
            CatalogInputAction::Pass => return false,
            CatalogInputAction::Handled => {}
            CatalogInputAction::ClearComposition => self.ime_marked_text = None,
            CatalogInputAction::CloseSidebarSearch => self.close_session_manager_sidebar_search(cx),
            CatalogInputAction::PruneSelection => self.clear_session_selection_for_invisible_rows(cx),
            CatalogInputAction::SubmitGroup => self.submit_session_group_editor(cx),
            CatalogInputAction::CommitTemplateName => self.commit_batch_template_name(cx),
            CatalogInputAction::CancelTemplateName => {
                self.cancel_batch_template_name(cx);
                self.ime_marked_text = None;
            }
        }
        true
    }

    pub(in crate::workspace) fn open_session_manager_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The two forms stay mutually exclusive: callers may reach here while
        // the sidebar still shows the session-manager panel (welcome link,
        // command palette, plugin lifecycle), so the tab owns moving the
        // sidebar off it, mirroring finish_tab_removal's close path.
        if self.active_sidebar_section == SidebarSection::Connections {
            self.set_sidebar_section(SidebarSection::Sessions, cx);
        }
        self.refresh_session_manager_ssh_config_hosts(cx);
        let tab_id = if let Some(tab) = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.kind == TabKind::SessionManager)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::SessionManager,
                    title: self.i18n.t("sessionManager.title"),
                    title_source: TabTitleSource::I18nKey("sessionManager.title"),
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            tab_id
        };
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        self.reveal_active_tab(window, cx);
        self.persist_sidebar_settings(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn refresh_session_manager_ssh_config_hosts(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let enabled = self.settings_store.settings().ssh_config.auto_load_hosts;
        self.connections.session_manager.update(cx, |manager, cx| {
            manager.refresh_ssh_config_hosts(
                enabled, self.forwarding_runtime.clone(), &self.connection_store, &self.i18n, cx,
            );
        });
    }

    pub(super) fn import_session_manager_ssh_config_host(
        &mut self,
        alias: String,
        cx: &mut Context<Self>,
    ) {
        if self.connections.session_manager.update(cx, |manager, cx| {
            manager.import_ssh_config_host(&mut self.connection_store, &alias, &self.i18n, cx)
        }) {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }
}
