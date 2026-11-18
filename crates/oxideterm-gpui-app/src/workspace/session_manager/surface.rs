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
        if !self.session_manager_search_open {
            return;
        }
        self.session_manager_search_open = false;
        self.begin_disclosure_motion("session-manager-search".into(), false, cx);
        self.connections.session_manager.update(cx, |manager, cx| {
            manager.search_query.clear();
            if manager.focused_input == Some(SessionManagerInput::Search) {
                manager.focused_input = None;
            }
            cx.notify();
        });
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
        let Some(input) = self.connections.session_manager.read(cx).focused_input else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        match key {
            "escape" if input == SessionManagerInput::BatchTemplateName => {
                // Esc on the rename input cancels the pending rename instead of
                // only clearing focus.
                self.cancel_batch_template_name(cx);
                self.ime_marked_text = None;
                true
            }
            "escape"
                if input == SessionManagerInput::Search
                    && self.session_manager_search_open
                    // The sidebar disclosure owns this keystroke only while no
                    // Session Manager tab is the keyboard content; with the tab
                    // active a stale sidebar flag must not steal the collapse,
                    // which belongs to the tab's search field (generic arm).
                    && !self
                        .keyboard_content_tab(cx)
                        .is_some_and(|tab| tab.kind == TabKind::SessionManager) =>
            {
                // Escape collapses the sidebar disclosure and drops the draft,
                // matching the active-sessions search behavior.
                self.close_session_manager_sidebar_search(cx);
                true
            }
            "escape" => {
                self.connections
                    .session_manager
                    .update(cx, |session_manager, cx| {
                        // Escape clears focus without copying any secret input draft.
                        session_manager.focused_input = None;
                        cx.notify();
                    });
                self.ime_marked_text = None;
                true
            }
            "enter" if input == SessionManagerInput::GroupName => {
                self.submit_session_group_editor(cx);
                true
            }
            "enter" if input == SessionManagerInput::BatchTemplateName => {
                self.commit_batch_template_name(cx);
                true
            }
            "backspace" => {
                let changed = self
                    .connections
                    .session_manager
                    .update(cx, |session_manager, cx| {
                        let changed = match input {
                            SessionManagerInput::Search => {
                                session_manager.search_query.pop().is_some()
                            }
                            SessionManagerInput::GroupName => {
                                session_manager.group_name_draft.pop().is_some()
                            }
                            SessionManagerInput::OxideImportPassword => session_manager
                                .oxide_import_dialog
                                .as_mut()
                                .is_some_and(|dialog| {
                                    dialog.password.pop().is_some() || dialog.error.take().is_some()
                                }),
                            SessionManagerInput::OxideExportPassword => session_manager
                                .oxide_export_dialog
                                .as_mut()
                                .is_some_and(|dialog| {
                                    dialog.password.pop().is_some() || dialog.error.take().is_some()
                                }),
                            SessionManagerInput::OxideExportConfirmPassword => session_manager
                                .oxide_export_dialog
                                .as_mut()
                                .is_some_and(|dialog| {
                                    dialog.confirm_password.pop().is_some()
                                        || dialog.error.take().is_some()
                                }),
                            SessionManagerInput::OxideExportDescription => session_manager
                                .oxide_export_dialog
                                .as_mut()
                                .is_some_and(|dialog| {
                                    dialog.description.pop().is_some()
                                        || dialog.error.take().is_some()
                                }),
                            SessionManagerInput::BatchPassword => {
                                session_manager.batch_add.as_mut().is_some_and(|dialog| {
                                    dialog.password.pop().is_some() || dialog.error.take().is_some()
                                })
                            }
                            SessionManagerInput::BatchKeyPath => {
                                session_manager.batch_add.as_mut().is_some_and(|dialog| {
                                    dialog.key_path.pop().is_some() || dialog.error.take().is_some()
                                })
                            }
                            SessionManagerInput::BatchTemplateName => session_manager
                                .batch_add
                                .as_mut()
                                .is_some_and(|dialog| dialog.rename_draft.pop().is_some()),
                        };
                        if changed {
                            cx.notify();
                        }
                        changed
                    });
                if changed && input == SessionManagerInput::Search {
                    self.clear_session_selection_for_invisible_rows(cx);
                }
                true
            }
            _ => false,
        }
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
        if !self.settings_store.settings().ssh_config.auto_load_hosts {
            self.connections
                .session_manager
                .update(cx, |session_manager, cx| {
                    session_manager.clear_ssh_config_hosts(cx);
                });
            return;
        }

        let runtime = self.forwarding_runtime.clone();
        let existing_names = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.name.clone())
            .collect::<HashSet<_>>();
        let load_failed_template = self
            .i18n
            .t("settings_view.connections.ssh_config.load_failed");
        self.connections
            .session_manager
            .update(cx, |session_manager, cx| {
                // Nested Include files may touch slow network-mounted homes.
                session_manager.begin_ssh_config_host_load(
                    runtime,
                    existing_names,
                    load_failed_template,
                    cx,
                );
            });
    }

    pub(super) fn import_session_manager_ssh_config_host(
        &mut self,
        alias: String,
        cx: &mut Context<Self>,
    ) {
        match oxideterm_connections::import_ssh_config_alias(&mut self.connection_store, &alias) {
            Ok(true) => {
                let status = self
                    .i18n
                    .t("settings_view.errors.import_success")
                    .replace("{{name}}", &alias);
                self.connections
                    .session_manager
                    .update(cx, |session_manager, cx| {
                        session_manager.remove_ssh_config_host_alias(&alias, cx);
                        session_manager.set_status(Some(status), cx);
                    });
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Ok(false) => {
                let status = self
                    .i18n
                    .t("settings_view.connections.ssh_config.batch_import_skipped")
                    .replace("{{count}}", "1");
                self.connections
                    .session_manager
                    .update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(status), cx)
                    });
            }
            Err(error) => {
                let status = self
                    .i18n
                    .t("settings_view.errors.import_failed")
                    .replace("{{error}}", &error.to_string());
                self.connections
                    .session_manager
                    .update(cx, |session_manager, cx| {
                        session_manager.set_status(Some(status), cx)
                    });
            }
        }
    }
}
