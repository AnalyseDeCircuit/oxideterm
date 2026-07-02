#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandSpecsAction {
    Format,
    Example,
    Save,
}

const SETTINGS_TERMINAL_CUSTOM_FONT_INPUT_WIDTH: f32 = 300.0; // Tauri TerminalTab custom font input w-[300px].

impl WorkspaceApp {
    fn settings_general_section(&self, section_index: usize, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings_store.settings();
        match section_index {
            0 => self.settings_card(
                "settings_view.general.language",
                "settings_view.general.language_hint",
                vec![self.language_select_row(settings.general.language, cx)],
            ),
            1 => {
                let data_dir_info = self.settings_data_directory_info();
                let data_dir = data_dir_info.path.display().to_string();
                self.plain_settings_card(vec![
                    self.card_title("settings_view.general.data_directory"),
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(self.tokens.ui.text))
                                .child(self.i18n.t("settings_view.general.data_directory")),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("settings_view.general.data_directory_hint")),
                        )
                        .into_any_element(),
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(16.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .text_size(px(self.tokens.metrics.ui_text_base))
                                .text_color(rgb(self.tokens.ui.text))
                                .font_family(settings_mono_font_family(
                                    self.settings_store.settings(),
                                ))
                                .truncate()
                                .child(data_dir),
                        )
                        .when(data_dir_info.can_change, |row| {
                            row.child(self.settings_data_directory_change_button(cx))
                        })
                        .when(data_dir_info.can_change && data_dir_info.is_custom, |row| {
                            row.child(self.settings_data_directory_reset_button(cx))
                        })
                        .into_any_element(),
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.warning))
                        .child(
                            self.i18n
                                .t("settings_view.general.data_directory_restart_notice"),
                        )
                        .into_any_element(),
                ])
            }
            2 => {
                let cli_status = self.settings_page.cli_companion_status.as_ref();
                let cli_loading = self.settings_page.cli_companion_loading;
                let cli_installed = cli_status.is_some_and(|status| status.installed);
                let cli_bundled = cli_status.is_some_and(|status| status.bundled);
                let cli_needs_reinstall = cli_status.is_some_and(|status| status.needs_reinstall);
                let cli_path = cli_status
                    .and_then(|status| status.install_path.clone())
                    .unwrap_or_else(|| cli_install_path().display().to_string());
                let (badge_label, badge_color) =
                    if self.settings_page.cli_companion_error.is_some() {
                        (
                            self.i18n.t("settings_view.general.cli_status_error"),
                            self.tokens.ui.error,
                        )
                    } else if cli_loading {
                        (
                            self.i18n.t("settings_view.general.cli_checking"),
                            self.tokens.ui.warning,
                        )
                    } else if cli_installed && cli_needs_reinstall {
                        (
                            self.i18n.t("settings_view.general.cli_reinstall_required"),
                            self.tokens.ui.warning,
                        )
                    } else if cli_installed {
                        (
                            self.i18n.t("settings_view.general.cli_installed"),
                            self.tokens.ui.success,
                        )
                    } else {
                        (
                            self.i18n.t("settings_view.general.cli_not_installed"),
                            self.tokens.ui.text_muted,
                        )
                    };
                let reinstall_hint = cli_status
                    .filter(|status| status.installed && status.needs_reinstall)
                    .map(|status| {
                        self.i18n_with(
                            "settings_view.general.cli_reinstall_hint",
                            &[("version", status.app_version.clone())],
                        )
                    });
                self.plain_settings_card(vec![
                    self.card_title("settings_view.general.cli_companion"),
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(self.tokens.ui.text))
                                .child(self.i18n.t("settings_view.general.cli_tool")),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("settings_view.general.cli_tool_hint")),
                        )
                        .into_any_element(),
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_end()
                        .justify_between()
                        .gap(px(16.0))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(10.0))
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Terminal,
                                            16.0,
                                            rgb(self.tokens.ui.text_muted),
                                        ))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_family(settings_mono_font_family(
                                                    self.settings_store.settings(),
                                                ))
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child(CLI_COMPANION_COMMAND_NAME),
                                        )
                                        .child(self.text_badge(badge_label, badge_color)),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .min_w(px(0.0))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .font_family(settings_mono_font_family(
                                            self.settings_store.settings(),
                                        ))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .truncate()
                                        .child(cli_path),
                                )
                                .when_some(reinstall_hint, |column, hint| {
                                    column.child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .text_color(rgb(self.tokens.ui.warning))
                                            .child(hint),
                                    )
                                })
                                .when_some(
                                    self.settings_page.cli_companion_error.clone(),
                                    |column, error| {
                                        column.child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .text_color(rgb(self.tokens.ui.error))
                                                .child(error),
                                        )
                                    },
                                )
                                .when(!cli_loading && cli_status.is_some() && !cli_bundled, |column| {
                                    column.child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .text_color(rgb(self.tokens.ui.text_muted))
                                            .child(
                                                self.i18n
                                                    .t("settings_view.general.cli_not_bundled"),
                                            ),
                                    )
                                }),
                        )
                        .when(cli_bundled && (!cli_installed || cli_needs_reinstall), |row| {
                            row.child(self.cli_companion_action_button(
                                if cli_needs_reinstall {
                                    self.i18n.t("settings_view.general.cli_reinstall")
                                } else {
                                    self.i18n.t("settings_view.general.cli_install")
                                },
                                LucideIcon::Download,
                                ButtonVariant::Outline,
                                cli_loading,
                                |this, _event, _window, cx| this.install_cli_companion(cx),
                                cx,
                            ))
                        })
                        .when(cli_installed, |row| {
                            row.child(self.cli_companion_action_button(
                                self.i18n.t("settings_view.general.cli_uninstall"),
                                LucideIcon::Trash2,
                                ButtonVariant::Ghost,
                                cli_loading,
                                |this, _event, _window, cx| this.uninstall_cli_companion(cx),
                                cx,
                            ))
                        })
                        .when(!cli_loading && self.settings_page.cli_companion_error.is_some(), |row| {
                            row.child(self.cli_companion_action_button(
                                self.i18n.t("settings_view.help.retry"),
                                LucideIcon::RefreshCw,
                                ButtonVariant::Ghost,
                                false,
                                |this, _event, _window, cx| this.refresh_cli_companion_status(cx),
                                cx,
                            ))
                        })
                        .into_any_element(),
                ])
            }
            3 if cfg!(any(target_os = "windows", target_os = "macos")) => {
                let (label_key, hint_key) = close_to_background_label_keys();
                self.settings_card(
                    "settings_view.general.window_behavior",
                    "settings_view.general.window_behavior_hint",
                    vec![self.general_checkbox_row(
                        label_key,
                        hint_key,
                        settings.general.minimize_to_tray_on_close,
                        |settings, enabled| settings.general.minimize_to_tray_on_close = enabled,
                        cx,
                    )],
                )
            }
            _ => div().into_any_element(),
        }
    }

    fn general_checkbox_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(
                div().flex_none().child(
                    checkbox(&self.tokens, String::new(), checked).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(|settings| setter(settings, !checked), cx);
                            cx.stop_propagation();
                        }),
                    ),
                ),
            )
            .into_any_element()
    }

    fn settings_data_directory_info(&self) -> oxideterm_settings::DataDirectoryInfo {
        oxideterm_settings::data_directory_info().unwrap_or_else(|_| {
            let path = self
                .settings_store
                .path()
                .parent()
                .unwrap_or_else(|| self.settings_store.path())
                .to_path_buf();
            oxideterm_settings::DataDirectoryInfo {
                default_path: path.clone(),
                path,
                is_custom: false,
                is_portable: false,
                can_change: false,
            }
        })
    }

    fn settings_data_directory_change_button(&self, cx: &mut Context<Self>) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.general.change"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.pick_settings_data_directory(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn settings_data_directory_reset_button(&self, cx: &mut Context<Self>) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.general.reset_to_default"),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.settings_data_directory_confirm = Some(DataDirectoryConfirm::Reset);
                this.reset_standard_confirm_focus();
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn pick_settings_data_directory(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.general.select_data_directory"),
            )),
        });
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = weak.update(cx, |this, cx| {
                match oxideterm_settings::check_data_directory(&path) {
                    Ok(check) if check.has_existing_data => {
                        // Tauri asks for a second confirmation before writing
                        // bootstrap.json when known OxideTerm data already
                        // exists in the target directory.
                        this.settings_data_directory_confirm =
                            Some(DataDirectoryConfirm::Conflict {
                                path,
                                files_found: check.files_found,
                            });
                        this.reset_standard_confirm_focus();
                        cx.notify();
                    }
                    Ok(_) => this.apply_settings_data_directory(path, cx),
                    Err(error) => {
                        this.push_ai_settings_toast(
                            error.to_string(),
                            TerminalNoticeVariant::Error,
                        );
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn apply_settings_data_directory(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match oxideterm_settings::set_data_directory(&path) {
            Ok(()) => {
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.general.data_directory_changed"),
                    TerminalNoticeVariant::Success,
                );
            }
            Err(error) => {
                self.push_ai_settings_toast(error.to_string(), TerminalNoticeVariant::Error);
            }
        }
        cx.notify();
    }

    fn reset_settings_data_directory(&mut self, cx: &mut Context<Self>) {
        match oxideterm_settings::reset_data_directory() {
            Ok(()) => {
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.general.data_directory_reset"),
                    TerminalNoticeVariant::Success,
                );
            }
            Err(error) => {
                self.push_ai_settings_toast(error.to_string(), TerminalNoticeVariant::Error);
            }
        }
        cx.notify();
    }

    pub(super) fn cancel_settings_data_directory_confirm(&mut self, cx: &mut Context<Self>) {
        self.settings_data_directory_confirm = None;
        self.clear_standard_confirm_focus();
        cx.notify();
    }

    pub(super) fn confirm_settings_data_directory(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.settings_data_directory_confirm.take() else {
            return;
        };
        self.clear_standard_confirm_focus();
        match confirm {
            DataDirectoryConfirm::Conflict { path, .. } => {
                self.apply_settings_data_directory(path, cx);
            }
            DataDirectoryConfirm::Reset => {
                self.reset_settings_data_directory(cx);
            }
        }
    }

    pub(super) fn render_settings_data_directory_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let confirm = self.settings_data_directory_confirm.as_ref()?;
        let (title_key, description) = match confirm {
            DataDirectoryConfirm::Conflict { files_found, .. } => (
                "settings_view.general.data_directory_conflict",
                self.i18n
                    .t("settings_view.general.data_directory_conflict_detail")
                    .replace("{{files}}", &files_found.join(", ")),
            ),
            DataDirectoryConfirm::Reset => (
                "settings_view.general.reset_data_directory",
                self.i18n
                    .t("settings_view.general.reset_data_directory_confirm"),
            ),
        };
        Some(confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Default,
                title: div().child(self.i18n.t(title_key)).into_any_element(),
                description: Some(div().child(description).into_any_element()),
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("common.actions.confirm"))
                    .into_any_element(),
            },
            self.standard_confirm_focus(),
            cx.listener(|this, _event, _window, cx| {
                this.cancel_settings_data_directory_confirm(cx);
                cx.stop_propagation();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.confirm_settings_data_directory(cx);
                cx.stop_propagation();
            }),
        ))
    }

    fn settings_terminal_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        if section_index == 0 {
            return self.terminal_page_switcher(cx);
        }

        match (self.settings_page.terminal_page, section_index - 1) {
            (TerminalSettingsPage::Display, 0) => {
                let mut rows = vec![self.select_setting_row(
                    "settings_view.terminal.font_family",
                    "settings_view.terminal.font_family_hint",
                    SettingsSelect::TerminalFontFamily,
                    font_family_label(settings.terminal.font_family),
                    self.tokens.metrics.settings_select_width,
                    cx,
                )];
                if settings.terminal.font_family == oxideterm_settings::FontFamily::Custom {
                    rows.push(self.setting_row(
                        "settings_view.terminal.custom_font_stack",
                        "settings_view.terminal.custom_font_stack_hint",
                        self.settings_text_input_control(
                            SettingsInput::TerminalCustomFontFamily,
                            settings.terminal.custom_font_family.clone(),
                            "'Sarasa Fixed SC', 'Fira Code', monospace".to_string(),
                            SETTINGS_TERMINAL_CUSTOM_FONT_INPUT_WIDTH,
                            cx,
                        ),
                        cx,
                    ));
                }
                rows.push(self.card_separator());
                rows.push(self.select_setting_row(
                    "settings_view.terminal.cjk_font_family",
                    "settings_view.terminal.cjk_font_family_hint",
                    SettingsSelect::TerminalCjkFontFamily,
                    terminal_cjk_font_label(&settings.terminal.cjk_font_family, &self.i18n),
                    self.tokens.metrics.settings_select_width,
                    cx,
                ));
                rows.extend([
                    self.terminal_preview(settings),
                    self.card_separator(),
                    self.font_size_row(settings, cx),
                    self.card_separator(),
                    self.decimal_row(
                        "settings_view.terminal.line_height",
                        "settings_view.terminal.line_height_hint",
                        SettingsInput::TerminalLineHeight,
                        compact_decimal(settings.terminal.line_height),
                        cx,
                    ),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.smooth_scroll",
                        "settings_view.terminal.smooth_scroll_hint",
                        settings.terminal.smooth_scroll,
                        set_terminal_smooth_scroll,
                        cx,
                    ),
                    self.card_separator(),
                    self.select_setting_row(
                        "settings_view.terminal.encoding",
                        "settings_view.terminal.encoding_hint",
                        SettingsSelect::TerminalEncoding,
                        terminal_encoding_label(settings.terminal.terminal_encoding),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.show_fps_overlay",
                        "settings_view.terminal.show_fps_overlay_hint",
                        settings.terminal.show_fps_overlay,
                        set_show_fps_overlay,
                        cx,
                    ),
                ]);
                self.settings_card(
                    "settings_view.terminal.font",
                    "settings_view.terminal.font_family_hint",
                    rows,
                )
            }
            (TerminalSettingsPage::Display, 1) => self.settings_card(
                "settings_view.terminal.cursor",
                "settings_view.terminal.cursor_style_hint",
                vec![
                    self.select_setting_row(
                        "settings_view.terminal.cursor_style",
                        "settings_view.terminal.cursor_style_hint",
                        SettingsSelect::TerminalCursorStyle,
                        cursor_style_label(settings.terminal.cursor_style, &self.i18n),
                        self.tokens.metrics.settings_select_narrow_width,
                        cx,
                    ),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.cursor_blink",
                        "settings_view.terminal.cursor_blink_hint",
                        settings.terminal.cursor_blink,
                        set_terminal_cursor_blink,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::Input, 0) => self.terminal_input_settings_card(settings, cx),
            (TerminalSettingsPage::CommandBar, 0) => self.settings_card(
                "settings_view.terminal.command_bar",
                "settings_view.terminal.command_bar_hint",
                vec![
                    self.bool_row(
                        "settings_view.terminal.command_bar",
                        "settings_view.terminal.command_bar_hint",
                        settings.terminal.command_bar.enabled,
                        set_command_bar_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.command_bar_git_status",
                        "settings_view.terminal.command_bar_git_status_hint",
                        settings.terminal.command_bar.git_status,
                        set_command_bar_git_status,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.command_bar_project_tasks",
                        "settings_view.terminal.command_bar_project_tasks_hint",
                        settings.terminal.command_bar.project_tasks,
                        set_command_bar_project_tasks,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.command_bar_current_directory_awareness",
                        "settings_view.terminal.command_bar_current_directory_awareness_hint",
                        settings.terminal.command_bar.current_directory_awareness,
                        set_command_bar_current_directory_awareness,
                        cx,
                    ),
                    self.card_separator(),
                    self.focus_handoff_commands_row(settings, cx),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_commands",
                        "settings_view.terminal.quick_commands_hint",
                        settings.terminal.command_bar.quick_commands_enabled,
                        set_quick_commands_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_commands_confirm",
                        "settings_view.terminal.quick_commands_confirm_hint",
                        settings
                            .terminal
                            .command_bar
                            .quick_commands_confirm_before_run,
                        set_quick_commands_confirm,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_commands_toast",
                        "settings_view.terminal.quick_commands_toast_hint",
                        settings.terminal.command_bar.quick_commands_show_toast,
                        set_quick_commands_toast,
                        cx,
                    ),
                    self.card_separator(),
                    self.terminal_command_specs_editor_row(cx),
                ],
            ),
            (TerminalSettingsPage::History, 0) => self.settings_card(
                "settings_view.terminal.command_marks",
                "settings_view.terminal.command_marks_hint",
                vec![
                    self.bool_row(
                        "settings_view.terminal.command_marks",
                        "settings_view.terminal.command_marks_hint",
                        settings.terminal.command_marks.enabled,
                        set_command_marks_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.command_marks_hover_actions",
                        "settings_view.terminal.command_marks_hover_actions_hint",
                        settings.terminal.command_marks.show_hover_actions,
                        set_command_marks_hover_actions,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::History, 1) => self.settings_card(
                "settings_view.terminal.buffer",
                "settings_view.terminal.scrollback_hint",
                vec![
                    self.number_row(
                        "settings_view.terminal.scrollback",
                        "settings_view.terminal.scrollback_hint",
                        settings.terminal.scrollback,
                        500,
                        500,
                        20000,
                        set_terminal_scrollback,
                        cx,
                    ),
                    self.card_separator(),
                    self.number_row(
                        "settings_view.terminal.backend_buffer_lines",
                        "settings_view.terminal.backend_buffer_lines_hint",
                        settings.buffer.max_lines,
                        500,
                        5000,
                        12000,
                        set_buffer_max_lines,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::Transfer, 0) => self.settings_card(
                "settings_view.terminal.in_band_transfer.title",
                "settings_view.terminal.in_band_transfer.runtime_note",
                vec![
                    self.bool_row(
                        "settings_view.terminal.in_band_transfer.enabled",
                        "settings_view.terminal.in_band_transfer.enabled_hint",
                        settings.terminal.in_band_transfer.enabled,
                        set_in_band_transfer_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.in_band_transfer.allow_directory",
                        "settings_view.terminal.in_band_transfer.allow_directory_hint",
                        settings.terminal.in_band_transfer.allow_directory,
                        set_in_band_transfer_allow_directory,
                        cx,
                    ),
                    self.card_separator(),
                    self.in_band_transfer_number_row(
                        "settings_view.terminal.in_band_transfer.max_chunk_bytes",
                        "settings_view.terminal.in_band_transfer.max_chunk_bytes_hint",
                        SettingsInput::InBandTransferMaxChunkBytes,
                        settings.terminal.in_band_transfer.max_chunk_bytes,
                        128.0,
                        cx,
                    ),
                    self.card_separator(),
                    self.in_band_transfer_number_row(
                        "settings_view.terminal.in_band_transfer.max_file_count",
                        "settings_view.terminal.in_band_transfer.max_file_count_hint",
                        SettingsInput::InBandTransferMaxFileCount,
                        settings.terminal.in_band_transfer.max_file_count,
                        128.0,
                        cx,
                    ),
                    self.card_separator(),
                    self.in_band_transfer_number_row(
                        "settings_view.terminal.in_band_transfer.max_total_bytes",
                        "settings_view.terminal.in_band_transfer.max_total_bytes_hint",
                        SettingsInput::InBandTransferMaxTotalBytes,
                        settings.terminal.in_band_transfer.max_total_bytes,
                        160.0,
                        cx,
                    ),
                    self.in_band_transfer_runtime_note(),
                ],
            ),
            (TerminalSettingsPage::Highlight, 0) => self.highlight_rules_card(settings, cx),
            _ => div().into_any_element(),
        }
    }

    fn focus_handoff_commands_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = SettingsInput::TerminalCommandBarFocusHandoff;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            settings
                .terminal
                .command_bar
                .focus_handoff_commands
                .join("\n")
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut textarea = div()
            .w_full()
            .min_h(px(96.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(0.0))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(line_height))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_color(rgb(theme.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );

        if value.is_empty() {
            textarea = self.render_settings_multiline_textarea_lines(
                textarea,
                target,
                "vim\nnvim\nlazygit",
                true,
                line_height,
            );
        } else {
            textarea = self.render_settings_multiline_textarea_lines(
                textarea,
                target,
                &value,
                false,
                line_height,
            );
        }

        if let Some(marked) = self.marked_text_for_target(target) {
            textarea = textarea.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }

        let control = text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        });

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("settings_view.terminal.command_bar_focus_handoff")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.command_bar_focus_handoff_hint"),
                            ),
                    ),
            )
            .child(control)
            .into_any_element()
    }

    fn terminal_command_specs_editor_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let input = SettingsInput::TerminalCommandSpecsJson;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.terminal_command_specs_editor_initial_value()
        };
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut textarea = div()
            .w_full()
            .min_h(px(220.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(0.0))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(line_height))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_color(rgb(theme.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );

        textarea =
            self.render_settings_multiline_textarea_lines(textarea, target, &value, false, line_height);
        if let Some(marked) = self.marked_text_for_target(target) {
            textarea = textarea.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }
        let control = text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        });

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("settings_view.terminal.command_specs")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("settings_view.terminal.command_specs_hint")),
                    ),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgb(theme.text_muted))
                    .truncate()
                    .child(path.display().to_string()),
            )
            .child(control)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.terminal_command_specs_summary()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_format",
                                TerminalCommandSpecsAction::Format,
                                cx,
                            ))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_example",
                                TerminalCommandSpecsAction::Example,
                                cx,
                            ))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_save",
                                TerminalCommandSpecsAction::Save,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_settings_multiline_textarea_lines(
        &self,
        mut textarea: Div,
        target: WorkspaceImeTarget,
        value: &str,
        placeholder: bool,
        line_height: f32,
    ) -> Div {
        let selection = self.ime_selected_range_for_target(target);
        let theme = self.tokens.ui;
        for (line_range, line_text) in settings_multiline_line_ranges(value) {
            let (selection_range, caret_offset) =
                settings_multiline_line_selection(selection.as_ref(), &line_range);
            // Browser textareas hit-test contiguous line boxes. Keep the
            // manually rendered GPUI lines at the same height used by IME
            // y-to-line mapping so pointer selection cannot drift vertically.
            let mut line = div().h(px(line_height)).min_h(px(line_height));
            if placeholder {
                // Browser placeholder text is not part of the editable value;
                // keep it muted and do not feed it through selection segments.
                line = line.text_color(rgb(theme.text_muted)).child(line_text);
            } else {
                // Tauri uses a real textarea, so caret/selection sit inside the
                // current visual line. Native renders line elements manually and
                // must split the shared UTF-16 IME selection per line.
                line = line.child(text_input_value_segments(
                    &self.tokens,
                    &line_text,
                    false,
                    selection_range,
                    caret_offset,
                    self.new_connection_caret_visible,
                ));
            }
            textarea = textarea.child(line);
        }
        textarea
    }

    fn terminal_command_specs_button(
        &self,
        label_key: &'static str,
        action: TerminalCommandSpecsAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Command-spec editor actions behave like Tauri Button onClick handlers:
        // disabled/loading guards live at the shared workspace boundary, not in
        // each feature listener.
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: if action == TerminalCommandSpecsAction::Save {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Outline
                    },
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| {
                this.handle_terminal_command_specs_action(action, window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn load_terminal_command_specs_editor_value(&self) -> String {
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        std::fs::read_to_string(path).unwrap_or_default()
    }

    fn terminal_command_specs_editor_initial_value(&self) -> String {
        super::terminal_command_bar::completion::terminal_command_specs_editor_initial_json(
            &self.load_terminal_command_specs_editor_value(),
        )
    }

    fn terminal_command_specs_summary(&self) -> String {
        let built_in_count =
            super::terminal_command_bar::completion::built_in_terminal_fig_specs().len();
        let custom_count = super::terminal_command_bar::completion::user_terminal_fig_specs_count(
            self.settings_store.path(),
        );
        self.i18n
            .t("settings_view.terminal.command_specs_summary")
            .replace("{builtIn}", &built_in_count.to_string())
            .replace("{custom}", &custom_count.to_string())
    }

    fn current_terminal_command_specs_editor_value(&self) -> String {
        if self.focused_settings_input == Some(SettingsInput::TerminalCommandSpecsJson) {
            self.settings_input_draft.clone()
        } else {
            self.terminal_command_specs_editor_initial_value()
        }
    }

    fn handle_terminal_command_specs_action(
        &mut self,
        action: TerminalCommandSpecsAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = SettingsInput::TerminalCommandSpecsJson;
        match action {
            TerminalCommandSpecsAction::Example => {
                let example =
                    super::terminal_command_bar::completion::terminal_command_specs_example_json();
                self.focus_settings_input(input, example, cx);
                window.focus(&self.focus_handle);
            }
            TerminalCommandSpecsAction::Format => {
                let value = self.current_terminal_command_specs_editor_value();
                match super::terminal_command_bar::completion::normalize_terminal_command_specs_json(&value) {
                    Ok(pretty) => {
                        self.focus_settings_input(input, pretty, cx);
                        window.focus(&self.focus_handle);
                    }
                    Err(error) => self.push_ai_settings_toast(
                        format!(
                            "{} {}",
                            self.i18n
                                .t("settings_view.terminal.command_specs_invalid"),
                            error
                        ),
                        TerminalNoticeVariant::Error,
                    ),
                }
            }
            TerminalCommandSpecsAction::Save => {
                let value = self.current_terminal_command_specs_editor_value();
                match super::terminal_command_bar::completion::normalize_terminal_command_specs_json(&value) {
                    Ok(pretty) => {
                        let path =
                            super::terminal_command_bar::completion::terminal_command_specs_path(
                                self.settings_store.path(),
                            );
                        let result = path
                            .parent()
                            .map(std::fs::create_dir_all)
                            .transpose()
                            .and_then(|_| std::fs::write(&path, pretty.as_bytes()));
                        match result {
                            Ok(()) => {
                                if self.focused_settings_input == Some(input) {
                                    self.settings_input_draft = pretty;
                                }
                                self.push_ai_settings_toast(
                                    self.i18n.t("settings_view.terminal.command_specs_saved"),
                                    TerminalNoticeVariant::Success,
                                );
                                cx.notify();
                            }
                            Err(error) => self.push_ai_settings_toast(
                                error.to_string(),
                                TerminalNoticeVariant::Error,
                            ),
                        }
                    }
                    Err(error) => self.push_ai_settings_toast(
                        format!(
                            "{} {}",
                            self.i18n
                                .t("settings_view.terminal.command_specs_invalid"),
                            error
                        ),
                        TerminalNoticeVariant::Error,
                    ),
                }
            }
        }
    }

    fn in_band_transfer_number_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: i64,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            self.number_input(input, value.to_string(), width, cx),
            cx,
        )
    }

    fn in_band_transfer_runtime_note(&self) -> AnyElement {
        const TAURI_RUNTIME_NOTE_BORDER_ALPHA: f32 = 0.30;
        const TAURI_RUNTIME_NOTE_BACKGROUND_ALPHA: f32 = 0.10;

        // Tauri renders this as `border-amber-500/30 bg-amber-500/10 p-3 text-xs`;
        // keep the amber opacity mapping explicit instead of folding it into
        // the generic settings card row style.
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.warning << 8) | alpha_byte(TAURI_RUNTIME_NOTE_BORDER_ALPHA),
            ))
            .bg(rgba(
                (self.tokens.ui.warning << 8) | alpha_byte(TAURI_RUNTIME_NOTE_BACKGROUND_ALPHA),
            ))
            .p(px(12.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                self.i18n
                    .t("settings_view.terminal.in_band_transfer.runtime_note"),
            )
            .into_any_element()
    }
}

fn close_to_background_label_keys() -> (&'static str, &'static str) {
    if cfg!(target_os = "macos") {
        (
            "settings_view.general.keep_in_menu_bar_on_close",
            "settings_view.general.keep_in_menu_bar_on_close_hint",
        )
    } else {
        (
            "settings_view.general.minimize_to_tray_on_close",
            "settings_view.general.minimize_to_tray_on_close_hint",
        )
    }
}
