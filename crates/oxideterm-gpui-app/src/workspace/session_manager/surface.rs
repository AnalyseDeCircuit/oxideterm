use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_session_manager_basic_dialog_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.session_manager.show_new_group {
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

    pub(super) fn activate_session_manager_basic_dialog_footer(
        &mut self,
        action: SessionManagerBasicDialogFooterAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            SessionManagerBasicDialogFooterAction::Cancel => {
                self.close_session_manager_basic_dialog(cx)
            }
            SessionManagerBasicDialogFooterAction::Primary
                if self.session_manager.show_new_group =>
            {
                if self.session_manager.new_group_name.trim().is_empty() {
                    // Match Tauri's disabled create button: keyboard activation
                    // cannot submit while the visible primary action is disabled.
                    return;
                }
                self.session_manager.focused_basic_dialog_footer_action = None;
                self.create_session_group(cx);
            }
            SessionManagerBasicDialogFooterAction::Primary => {}
        }
    }

    pub(super) fn close_session_manager_basic_dialog(&mut self, cx: &mut Context<Self>) {
        if self.session_manager.show_new_group {
            self.session_manager.show_new_group = false;
            self.session_manager.focused_input = None;
        }
        self.session_manager.focused_basic_dialog_footer_action = None;
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn render_session_manager_surface(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("session_manager");
        let view_mode = self.session_manager.view_mode;
        let content = self.render_session_manager_view_content(has_background, cx);
        let content = oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            ("session-manager-view", view_mode as usize),
            div().size_full().child(content),
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            // Session Manager is ordinary UI chrome, so it must explicitly
            // follow the configured Tauri UI font instead of inheriting a
            // terminal/mono font from surrounding tab content.
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .text_color(rgb(theme.text))
            .child(self.render_session_manager_toolbar(window, has_background, cx))
            .child(div().flex_1().min_h(px(0.0)).min_w(px(0.0)).child(content))
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
            .when_some(
                self.session_manager.row_action_menu.clone(),
                |surface, menu| {
                    surface.child(self.workspace_context_menu_backdrop(
                        self.render_session_manager_row_action_menu(
                            menu,
                            window,
                            has_background,
                            cx,
                        ),
                        cx,
                    ))
                },
            )
            .when(self.session_manager.view_mode_menu_open, |surface| {
                surface.child(self.workspace_context_menu_backdrop(
                    self.render_session_manager_view_mode_menu(window, has_background, cx),
                    cx,
                ))
            })
            .when(self.session_manager.sort_menu_open, |surface| {
                surface.child(self.workspace_context_menu_backdrop(
                    self.render_session_manager_sort_menu(window, has_background, cx),
                    cx,
                ))
            })
            .when(
                !self.session_manager.selected_ids.is_empty()
                    && self.session_manager.show_batch_move,
                |surface| {
                    surface.child(self.workspace_context_menu_backdrop(
                        self.render_batch_move_popover(window, cx),
                        cx,
                    ))
                },
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_session_manager_delete_confirm(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

    pub(in crate::workspace) fn handle_session_manager_key(
        &mut self,
        event: &KeyDownEvent,
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
                            dialog.confirm_password.pop().is_some() || dialog.error.take().is_some()
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

    pub(in crate::workspace) fn open_session_manager_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_session_manager_ssh_config_hosts(cx);
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
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.main_window_tabs.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.active_sidebar_section = SidebarSection::Connections;
        self.needs_active_pane_focus = false;
        if self.sidebar_collapsed {
            self.set_sidebar_collapsed_with_motion(false, cx);
        }
        window.focus(&self.focus_handle, cx);
        self.reveal_active_tab(window);
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(in crate::workspace) fn refresh_session_manager_ssh_config_hosts(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.settings_store.settings().ssh_config.auto_load_hosts {
            self.session_manager.ssh_config_hosts.clear();
            return;
        }

        let runtime = self.forwarding_runtime.clone();
        let existing_names = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.name.clone())
            .collect::<HashSet<_>>();
        // Reading nested Include files may touch slow network-mounted homes, so
        // discovery must never block GPUI's render thread.
        cx.spawn(async move |weak, cx| {
            let result = runtime
                .spawn_blocking(move || {
                    oxideterm_connections::list_ssh_config_hosts(&existing_names)
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(hosts) => {
                        this.session_manager.ssh_config_hosts = hosts;
                    }
                    Err(error) => {
                        this.session_manager.ssh_config_hosts.clear();
                        this.session_manager.status = Some(
                            this.i18n
                                .t("settings_view.connections.ssh_config.load_failed")
                                .replace("{{error}}", &error),
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn import_session_manager_ssh_config_host(
        &mut self,
        alias: String,
        cx: &mut Context<Self>,
    ) {
        match oxideterm_connections::import_ssh_config_alias(&mut self.connection_store, &alias) {
            Ok(true) => {
                self.session_manager
                    .ssh_config_hosts
                    .retain(|host| host.alias != alias);
                self.session_manager.status = Some(
                    self.i18n
                        .t("settings_view.errors.import_success")
                        .replace("{{name}}", &alias),
                );
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Ok(false) => {
                self.session_manager.status = Some(
                    self.i18n
                        .t("settings_view.connections.ssh_config.batch_import_skipped")
                        .replace("{{count}}", "1"),
                );
            }
            Err(error) => {
                self.session_manager.status = Some(
                    self.i18n
                        .t("settings_view.errors.import_failed")
                        .replace("{{error}}", &error.to_string()),
                );
            }
        }
        cx.notify();
    }
}
