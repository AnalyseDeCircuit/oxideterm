use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_session_manager_delete_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.session_manager.read(cx).delete_confirm.is_none() {
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
        let (group_editor_active, focused_input, focused_footer_action) = {
            let session_manager = self.session_manager.read(cx);
            (
                session_manager.show_group_manager && session_manager.group_editor.is_some(),
                session_manager.focused_input,
                session_manager.focused_basic_dialog_footer_action,
            )
        };
        if !group_editor_active {
            return false;
        }
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }

        match browser_behavior::modal_footer_input_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
            group_editor_active,
            focused_input == Some(SessionManagerInput::GroupName),
            focused_footer_action,
            SessionManagerBasicDialogFooterAction::Cancel,
            None,
        ) {
            Some(browser_behavior::ModalFooterInputKeyAction::Cancel) => {
                self.cancel_session_group_editor(cx);
                true
            }
            Some(browser_behavior::ModalFooterInputKeyAction::FocusInput) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.focused_input = Some(SessionManagerInput::GroupName);
                    session_manager.focused_basic_dialog_footer_action = None;
                    cx.notify();
                });
                self.ime_marked_text = None;
                true
            }
            Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(action)) => {
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.focused_input = None;
                    session_manager.focused_basic_dialog_footer_action = Some(action);
                    cx.notify();
                });
                self.ime_marked_text = None;
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
            SessionManagerBasicDialogFooterAction::Cancel => self.cancel_session_group_editor(cx),
            SessionManagerBasicDialogFooterAction::Primary => {
                let (group_editor_active, group_name_invalid) = {
                    let session_manager = self.session_manager.read(cx);
                    let group_name = session_manager.group_name_draft.trim();
                    let candidate_path = session_manager.group_editor.as_ref().and_then(|editor| {
                        let parent_path = match editor {
                            SessionManagerGroupEditor::Create { parent_path }
                            | SessionManagerGroupEditor::Rename { parent_path, .. } => {
                                parent_path.as_deref()
                            }
                        };
                        session_group_path_from_leaf(parent_path, group_name)
                    });
                    (
                        session_manager.show_group_manager
                            && session_manager.group_editor.is_some(),
                        candidate_path.is_none()
                            || matches!(
                                (
                                    session_manager.group_editor.as_ref(),
                                    candidate_path.as_deref()
                                ),
                                (
                                    Some(SessionManagerGroupEditor::Rename { old_path, .. }),
                                    Some(candidate)
                                ) if old_path == candidate
                            ),
                    )
                };
                if !group_editor_active || group_name_invalid {
                    // Keyboard activation cannot submit while the visible primary
                    // action is disabled for an empty or unchanged group name.
                    return;
                }
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.focused_basic_dialog_footer_action = None;
                    cx.notify();
                });
                self.submit_session_group_editor(cx);
            }
        }
    }

    pub(super) fn cancel_session_group_editor(&mut self, cx: &mut Context<Self>) {
        self.session_manager.update(cx, |session_manager, cx| {
            session_manager.group_editor = None;
            session_manager.group_name_draft.clear();
            session_manager.group_editor_error = None;
            session_manager.focused_input = None;
            session_manager.focused_basic_dialog_footer_action = None;
            cx.notify();
        });
        self.ime_marked_text = None;
    }

    pub(in crate::workspace) fn render_session_manager_surface(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("session_manager");
        let (
            view_mode,
            status,
            row_action_menu,
            view_mode_menu_open,
            sort_menu_open,
            show_batch_move,
            has_selection,
        ) = {
            let session_manager = self.session_manager.read(cx);
            (
                session_manager.view_mode,
                session_manager.status.clone(),
                session_manager.row_action_menu.clone(),
                session_manager.view_mode_menu_open,
                session_manager.sort_menu_open,
                session_manager.show_batch_move,
                !session_manager.selected_items.is_empty(),
            )
        };
        let content = self.render_session_manager_view_content(window, has_background, cx);
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
            .when_some(status, |surface, status| {
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
            .when_some(row_action_menu, |surface, menu| {
                let menu =
                    self.render_session_manager_row_action_menu(menu, window, has_background, cx);
                let backdrop = self
                    .workspace_context_menu_backdrop(menu, cx)
                    .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                        // Pointer-positioned menus are stale as soon as their list scrolls.
                        this.close_session_row_menus(cx);
                        cx.stop_propagation();
                    }));
                surface.child(backdrop)
            })
            .when(view_mode_menu_open, |surface| {
                surface.child(self.workspace_context_menu_backdrop(
                    self.render_session_manager_view_mode_menu(window, has_background, cx),
                    cx,
                ))
            })
            .when(sort_menu_open, |surface| {
                surface.child(self.workspace_context_menu_backdrop(
                    self.render_session_manager_sort_menu(window, has_background, cx),
                    cx,
                ))
            })
            .when(has_selection && show_batch_move, |surface| {
                surface.child(self.workspace_context_menu_backdrop(
                    self.render_batch_move_popover(window, cx),
                    cx,
                ))
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn render_session_manager_delete_confirm(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(confirm) = self.session_manager.read(cx).delete_confirm.clone() else {
            return div().into_any_element();
        };
        let (title, confirm_label) = match &confirm {
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
            SessionManagerDeleteConfirm::MoshProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.mosh_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.mosh_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::StandaloneSftpProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.standalone_sftp_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n
                    .t("sessionManager.standalone_sftp_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::RemoteDesktopProfile { name, .. } => (
                self.i18n
                    .t("sessionManager.remote_desktop_profiles.confirm_delete")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.remote_desktop_profiles.delete"),
            ),
            SessionManagerDeleteConfirm::Batch { targets } => (
                confirm_batch_delete_label(&self.i18n, targets.len()),
                self.i18n.t("common.actions.confirm"),
            ),
            SessionManagerDeleteConfirm::Group { name } => (
                self.i18n
                    .t("sessionManager.folder_tree.confirm_delete_group")
                    .replace("{{name}}", name),
                self.i18n.t("sessionManager.folder_tree.delete_group"),
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
        let Some(input) = self.session_manager.read(cx).focused_input else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        match key {
            "escape" => {
                self.session_manager.update(cx, |session_manager, cx| {
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
            "backspace" => {
                let changed = self.session_manager.update(cx, |session_manager, cx| {
                    let changed = match input {
                        SessionManagerInput::Search => session_manager.search_query.pop().is_some(),
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
                                dialog.description.pop().is_some() || dialog.error.take().is_some()
                            }),
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
        self.active_sidebar_section = SidebarSection::Connections;
        self.needs_active_pane_focus = false;
        if self.sidebar_collapsed {
            self.set_sidebar_collapsed_with_motion(false, cx);
        }
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
            self.session_manager.update(cx, |session_manager, cx| {
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
        self.session_manager.update(cx, |session_manager, cx| {
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
                self.session_manager.update(cx, |session_manager, cx| {
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
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(status), cx)
                });
            }
            Err(error) => {
                let status = self
                    .i18n
                    .t("settings_view.errors.import_failed")
                    .replace("{{error}}", &error.to_string());
                self.session_manager.update(cx, |session_manager, cx| {
                    session_manager.set_status(Some(status), cx)
                });
            }
        }
    }
}
