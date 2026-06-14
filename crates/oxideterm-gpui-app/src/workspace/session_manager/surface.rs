impl WorkspaceApp {
    pub(super) fn handle_session_manager_basic_dialog_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.session_manager.show_new_group && !self.session_manager.show_import {
            return false;
        }
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }

        match browser_behavior::modal_footer_input_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
            self.session_manager.show_new_group,
            self.session_manager.focused_input == Some(SessionManagerInput::NewGroup),
            self.session_manager.focused_basic_dialog_footer_action,
            SessionManagerBasicDialogFooterAction::Cancel,
            None,
        ) {
            Some(browser_behavior::ModalFooterInputKeyAction::Cancel) => {
                self.close_session_manager_basic_dialog(cx);
                true
            }
            Some(browser_behavior::ModalFooterInputKeyAction::FocusInput) => {
                self.session_manager.focused_input = Some(SessionManagerInput::NewGroup);
                self.session_manager.focused_basic_dialog_footer_action = None;
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(action)) => {
                self.session_manager.focused_input = None;
                self.session_manager.focused_basic_dialog_footer_action = Some(action);
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterInputKeyAction::Activate(action)) => {
                self.activate_session_manager_basic_dialog_footer(action, cx);
                true
            }
            None => false,
        }
    }

    fn activate_session_manager_basic_dialog_footer(
        &mut self,
        action: SessionManagerBasicDialogFooterAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            SessionManagerBasicDialogFooterAction::Cancel => self.close_session_manager_basic_dialog(cx),
            SessionManagerBasicDialogFooterAction::Primary if self.session_manager.show_new_group => {
                if self.session_manager.new_group_name.trim().is_empty() {
                    // Match Tauri's disabled create button: keyboard activation
                    // cannot submit while the visible primary action is disabled.
                    return;
                }
                self.session_manager.focused_basic_dialog_footer_action = None;
                self.create_session_group(cx);
            }
            SessionManagerBasicDialogFooterAction::Primary if self.session_manager.show_import => {
                self.session_manager.focused_basic_dialog_footer_action = None;
                self.import_selected_ssh_hosts(cx);
            }
            SessionManagerBasicDialogFooterAction::Primary => {}
        }
    }

    fn close_session_manager_basic_dialog(&mut self, cx: &mut Context<Self>) {
        if self.session_manager.show_new_group {
            self.session_manager.show_new_group = false;
            self.session_manager.focused_input = None;
        }
        if self.session_manager.show_import {
            self.session_manager.show_import = false;
            self.session_manager.selected_import_aliases.clear();
        }
        self.session_manager.focused_basic_dialog_footer_action = None;
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn render_session_manager_surface(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self
            .terminal_background_preferences("session_manager")
            .is_some();
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .text_color(rgb(theme.text))
            .child(self.render_session_manager_toolbar(window, has_background, cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_row()
                    .child(self.render_session_manager_folder_tree(has_background, cx))
                    .child(self.render_session_manager_table(has_background, cx)),
            )
            .when_some(self.session_manager.status.clone(), |surface, status| {
                surface.child(
                    div()
                        .h(px(32.0))
                        .flex()
                        .items_center()
                        .px_4()
                        .border_t_1()
                        .border_color(theme_border(theme.border, has_background))
                        .bg(theme_bg(theme.bg, has_background))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.accent))
                        .child(status),
                )
            })
            .when(self.session_manager.show_new_group, |surface| {
                surface.child(self.render_new_group_dialog(cx))
            })
            .when(self.session_manager.show_import, |surface| {
                surface.child(self.render_ssh_config_import_dialog(cx))
            })
            .when_some(self.session_manager.delete_confirm.as_ref(), |surface, _| {
                surface.child(self.render_session_manager_delete_confirm(cx))
            })
            .when_some(self.session_manager.oxide_import_dialog.as_ref(), |surface, _| {
                surface.child(self.render_oxide_import_dialog(cx))
            })
            .when_some(self.session_manager.oxide_export_dialog.as_ref(), |surface, _| {
                surface.child(self.render_oxide_export_dialog(cx))
            })
            .when_some(
                self.session_manager
                    .row_menu_connection_id
                    .as_deref()
                    .and_then(|id| self.connection_info_by_id(id)),
                |surface, conn| {
                    surface.child(self.workspace_context_menu_backdrop(
                        self.render_row_more_menu(conn, window, has_background, cx),
                        cx,
                    ))
                },
            )
            .when_some(
                self.session_manager
                .row_context_menu_connection_id
                .as_deref()
                .and_then(|id| self.connection_info_by_id(id)),
                |surface, conn| {
                    surface.child(self.workspace_context_menu_backdrop(
                        self.render_row_context_menu(conn, window, has_background, cx),
                        cx,
                    ))
                },
            )
            .when(
                self.session_manager.folder_tree_context_menu_x.is_some()
                    && self.session_manager.folder_tree_context_menu_y.is_some(),
                |surface| {
                    surface.child(self.workspace_context_menu_backdrop(
                        self.render_folder_tree_context_menu(window, cx),
                        cx,
                    ))
                },
            )
            .into_any_element()
    }

    fn render_session_manager_delete_confirm(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(confirm) = self.session_manager.delete_confirm.as_ref() else {
            return div().into_any_element();
        };
        let (title, confirm_label) = match confirm {
            SessionManagerDeleteConfirm::Single { name, .. } => (
                confirm_delete_connection_label(&self.i18n, name),
                self.i18n.t("sessionManager.actions.delete"),
            ),
            SessionManagerDeleteConfirm::SerialProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.serial_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.serial_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::TelnetProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.telnet_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.telnet_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::Batch { ids } => (
                confirm_batch_delete_label(&self.i18n, ids.len()),
                self.i18n.t("common.actions.confirm"),
            ),
        };
        confirm_dialog(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div().child(title).into_any_element(),
                description: None,
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div().child(confirm_label).into_any_element(),
            },
            cx.listener(|this, _event, _window, cx| {
                this.cancel_session_manager_delete(cx);
                cx.stop_propagation();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.confirm_session_manager_delete(cx);
                cx.stop_propagation();
            }),
        )
    }

    pub(super) fn handle_session_manager_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.session_manager.focused_input else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        match key {
            "escape" => {
                match input {
                    SessionManagerInput::AutoRouteDisplayName => self.close_auto_route_modal(cx),
                    SessionManagerInput::OxideImportPassword
                    | SessionManagerInput::OxideExportPassword
                    | SessionManagerInput::OxideExportConfirmPassword
                    | SessionManagerInput::OxideExportDescription => {
                        self.session_manager.focused_input = None;
                    }
                    _ => {
                        self.session_manager.focused_input = None;
                    }
                }
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            "enter" if input == SessionManagerInput::AutoRouteDisplayName => {
                self.connect_auto_route(window, cx);
                true
            }
            "enter" if input == SessionManagerInput::NewGroup => {
                self.create_session_group(cx);
                true
            }
            "backspace" => {
                let changed = match input {
                    SessionManagerInput::Search => {
                        self.session_manager.search_query.pop().is_some()
                    }
                    SessionManagerInput::SavedSearch => {
                        self.session_manager.saved_search_query.pop().is_some()
                    }
                    SessionManagerInput::NewGroup => {
                        self.session_manager.new_group_name.pop().is_some()
                    }
                    SessionManagerInput::AutoRouteDisplayName => {
                        self.auto_route_modal.display_name.pop().is_some()
                    }
                    SessionManagerInput::OxideImportPassword => {
                        if let Some(dialog) = self.session_manager.oxide_import_dialog.as_mut() {
                            dialog.password.pop().is_some() || dialog.error.take().is_some()
                        } else {
                            false
                        }
                    }
                    SessionManagerInput::OxideExportPassword => {
                        if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                            dialog.password.pop().is_some() || dialog.error.take().is_some()
                        } else {
                            false
                        }
                    }
                    SessionManagerInput::OxideExportConfirmPassword => {
                        if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                            dialog.confirm_password.pop().is_some()
                                || dialog.error.take().is_some()
                        } else {
                            false
                        }
                    }
                    SessionManagerInput::OxideExportDescription => {
                        if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                            dialog.description.pop().is_some() || dialog.error.take().is_some()
                        } else {
                            false
                        }
                    }
                };
                if changed && input == SessionManagerInput::Search {
                    self.clear_session_selection_for_invisible_rows();
                }
                if changed {
                    // Empty Backspace should not repaint session-manager inputs
                    // unless it deletes text or clears a visible validation error.
                    cx.notify();
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn open_session_manager_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.kind == TabKind::SessionManager)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::SessionManager,
                title: self.i18n.t("sessionManager.title"),
                title_source: TabTitleSource::I18nKey("sessionManager.title"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.active_sidebar_section = SidebarSection::Connections;
        self.needs_active_pane_focus = false;
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
        window.focus(&self.focus_handle);
        self.reveal_active_tab(window);
        self.persist_sidebar_settings();
        cx.notify();
    }

}
