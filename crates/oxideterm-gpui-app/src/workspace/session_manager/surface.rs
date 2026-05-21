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

        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_session_manager_basic_dialog(cx);
                true
            }
            "tab" => {
                // Browser focus traps walk visible dialog controls in order.
                // These small native dialogs model the same cycle explicitly:
                // optional text input, cancel, primary, then wrap.
                self.move_session_manager_basic_dialog_footer_focus(
                    !event.keystroke.modifiers.shift,
                    true,
                    cx,
                );
                true
            }
            "arrowleft" | "left" => {
                self.move_session_manager_basic_dialog_footer_focus(false, false, cx);
                true
            }
            "arrowright" | "right" => {
                self.move_session_manager_basic_dialog_footer_focus(true, false, cx);
                true
            }
            "enter" | "space" | " " => {
                if let Some(action) = self.session_manager.focused_basic_dialog_footer_action {
                    self.activate_session_manager_basic_dialog_footer(action, cx);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn move_session_manager_basic_dialog_footer_focus(
        &mut self,
        forward: bool,
        include_text_input: bool,
        cx: &mut Context<Self>,
    ) {
        let (focus_text_input, footer_action) = next_session_manager_basic_dialog_focus(
            include_text_input && self.session_manager.show_new_group,
            self.session_manager.focused_input == Some(SessionManagerInput::NewGroup),
            self.session_manager.focused_basic_dialog_footer_action,
            forward,
        );
        self.session_manager.focused_input =
            focus_text_input.then_some(SessionManagerInput::NewGroup);
        self.session_manager.focused_basic_dialog_footer_action = footer_action;
        self.ime_marked_text = None;
        cx.notify();
    }

    fn activate_session_manager_basic_dialog_footer(
        &mut self,
        action: SessionManagerBasicDialogFooterAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            SessionManagerBasicDialogFooterAction::Cancel => self.close_session_manager_basic_dialog(cx),
            SessionManagerBasicDialogFooterAction::Primary if self.session_manager.show_new_group => {
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
        &self,
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
            .when_some(self.session_manager.oxide_import_dialog.as_ref(), |surface, _| {
                surface.child(self.render_oxide_import_dialog(cx))
            })
            .when_some(self.session_manager.oxide_export_dialog.as_ref(), |surface, _| {
                surface.child(self.render_oxide_export_dialog(cx))
            })
            .when_some(
                self.session_manager
                .row_context_menu_connection_id
                .as_deref()
                .and_then(|id| self.connection_info_by_id(id)),
                |surface, conn| {
                    surface.child(
                        popover_backdrop()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, window, cx| {
                                    // Row menus are transient overlays; route
                                    // outside dismissal through the shared root
                                    // path so focus restoration stays uniform.
                                    this.dismiss_transient_workspace_overlays_from_outside_pointer(
                                        window, cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(|this, _event, window, cx| {
                                    // Secondary outside clicks close the current
                                    // menu before the next row can claim a menu.
                                    this.dismiss_transient_workspace_overlays_from_outside_pointer(
                                        window, cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .child(self.render_row_context_menu(conn, window, has_background, cx)),
                    )
                },
            )
            .into_any_element()
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
                match input {
                    SessionManagerInput::Search => {
                        self.session_manager.search_query.pop();
                    }
                    SessionManagerInput::SavedSearch => {
                        self.session_manager.saved_search_query.pop();
                    }
                    SessionManagerInput::NewGroup => {
                        self.session_manager.new_group_name.pop();
                    }
                    SessionManagerInput::AutoRouteDisplayName => {
                        self.auto_route_modal.display_name.pop();
                    }
                    SessionManagerInput::OxideImportPassword => {
                        if let Some(dialog) = self.session_manager.oxide_import_dialog.as_mut() {
                            dialog.password.pop();
                            dialog.error = None;
                        }
                    }
                    SessionManagerInput::OxideExportPassword => {
                        if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                            dialog.password.pop();
                            dialog.error = None;
                        }
                    }
                    SessionManagerInput::OxideExportConfirmPassword => {
                        if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                            dialog.confirm_password.pop();
                            dialog.error = None;
                        }
                    }
                    SessionManagerInput::OxideExportDescription => {
                        if let Some(dialog) = self.session_manager.oxide_export_dialog.as_mut() {
                            dialog.description.pop();
                            dialog.error = None;
                        }
                    }
                };
                if input == SessionManagerInput::Search {
                    self.clear_session_selection_for_invisible_rows();
                }
                cx.notify();
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
