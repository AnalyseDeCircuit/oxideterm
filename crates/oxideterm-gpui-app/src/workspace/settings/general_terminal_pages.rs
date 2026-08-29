use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum TerminalCommandSpecsAction {
    Format,
    Example,
    Save,
}

pub(in crate::workspace) const SETTINGS_TERMINAL_CUSTOM_FONT_INPUT_WIDTH: f32 = 300.0; // Tauri TerminalTab custom font input w-[300px].
// The command-spec document needs enough room for structured editing while
// remaining bounded by the workspace viewport.
const TERMINAL_COMMAND_SPECS_MODAL_WIDTH: f32 = 880.0;
const TERMINAL_COMMAND_SPECS_MODAL_HEIGHT: f32 = 720.0;
const TERMINAL_COMMAND_SPECS_EDITOR_MIN_HEIGHT: f32 = 520.0;
const TERMINAL_COMMAND_SPECS_ACTION_ICON_SIZE: f32 = 12.0;

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_general_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                let cli = self.settings_workspace.read(cx).cli_companion_snapshot();
                let cli_status = cli.status.as_ref();
                let cli_loading = cli.loading;
                let cli_installed = cli_status.is_some_and(|status| status.installed);
                let cli_bundled = cli_status.is_some_and(|status| status.bundled);
                let cli_needs_reinstall = cli_status.is_some_and(|status| status.needs_reinstall);
                let legacy_cli_installed = cli_status.is_some_and(|status| status.legacy_installed);
                let legacy_cli_path = cli_status
                    .and_then(|status| status.legacy_install_path.clone())
                    .unwrap_or_default();
                let cli_path = cli_status
                    .and_then(|status| status.install_path.clone())
                    .unwrap_or_else(|| cli_install_path().display().to_string());
                let (badge_label, badge_color) = if cli.error.is_some() {
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
                                    cli.error.clone(),
                                    |column, error| {
                                        column.child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .text_color(rgb(self.tokens.ui.error))
                                                .child(error),
                                        )
                                    },
                                )
                                .when(
                                    !cli_loading && cli_status.is_some() && !cli_bundled,
                                    |column| {
                                        column.child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .text_color(rgb(self.tokens.ui.text_muted))
                                                .child(
                                                    self.i18n
                                                        .t("settings_view.general.cli_not_bundled"),
                                                ),
                                        )
                                    },
                                ),
                        )
                        .when(
                            cli_bundled && (!cli_installed || cli_needs_reinstall),
                            |row| {
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
                            },
                        )
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
                        .when(
                            !cli_loading && cli.error.is_some(),
                            |row| {
                                row.child(self.cli_companion_action_button(
                                    self.i18n.t("settings_view.help.retry"),
                                    LucideIcon::RefreshCw,
                                    ButtonVariant::Ghost,
                                    false,
                                    |this, _event, _window, cx| {
                                        this.refresh_cli_companion_status(cx)
                                    },
                                    cx,
                                ))
                            },
                        )
                        .into_any_element(),
                    if legacy_cli_installed {
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .p(px(12.0))
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgba((self.tokens.ui.warning << 8) | 0x80))
                            .bg(rgba((self.tokens.ui.warning << 8) | 0x12))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(220.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::AlertTriangle,
                                                16.0,
                                                rgb(self.tokens.ui.warning),
                                            ))
                                            .child(
                                                div()
                                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                                    .font_weight(gpui::FontWeight::MEDIUM)
                                                    .text_color(rgb(self.tokens.ui.text))
                                                    .child(self.i18n.t("migration.cli_legacy_found")),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .text_color(rgb(self.tokens.ui.text_muted))
                                            .child(self.i18n.t("migration.cli_legacy_settings_hint")),
                                    )
                                    .child(
                                        div()
                                            .min_w(px(0.0))
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .font_family(settings_mono_font_family(
                                                self.settings_store.settings(),
                                            ))
                                            .text_color(rgb(self.tokens.ui.text_muted))
                                            .truncate()
                                            .child(format!(
                                                "{LEGACY_CLI_COMPANION_COMMAND_NAME} · {legacy_cli_path}"
                                            )),
                                    ),
                            )
                            .child(self.cli_companion_action_button(
                                self.i18n.t("migration.cli_uninstall_legacy"),
                                LucideIcon::Trash2,
                                ButtonVariant::Ghost,
                                cli_loading,
                                |this, _event, _window, cx| {
                                    this.uninstall_legacy_cli_companion(cx)
                                },
                                cx,
                            ))
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    },
                ])
            }
            3 => self.launch_at_login_settings_card(cx),
            4 => self.settings_card(
                "settings_view.general.connection_uri_integration",
                "settings_view.general.connection_uri_integration_hint",
                vec![self.general_checkbox_row(
                    "settings_view.general.external_connection_uris",
                    "settings_view.general.external_connection_uris_hint",
                    settings.general.external_connection_uris_enabled,
                    |settings, enabled| settings.general.external_connection_uris_enabled = enabled,
                    cx,
                )],
            ),
            5 if cfg!(any(target_os = "windows", target_os = "macos")) => {
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
            6 if cfg!(any(target_os = "windows", target_os = "macos")) => {
                self.render_app_lock_settings_card(cx)
            }
            5 => self.render_app_lock_settings_card(cx),
            _ => div().into_any_element(),
        }
    }

    #[cfg(target_os = "macos")]
    pub(in crate::workspace) fn launch_at_login_settings_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let launch_at_login = self.settings_workspace.read(cx).launch_at_login_snapshot();
        // TODO(signing): Once every macOS artifact is Developer ID signed and
        // notarized, replace this system-settings handoff with an in-app
        // SMAppService register/unregister toggle and remove the manual copy.
        let mut description = div()
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
                    .child(self.i18n.t("settings_view.general.launch_at_login")),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.general.launch_at_login_macos_hint"),
                    ),
            );
        if let Some(error) = launch_at_login.error {
            let error = match error {
                LaunchAtLoginError::OperationFailed(error) => error,
            };
            description = description.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(
                        self.i18n
                            .t("settings_view.general.launch_at_login_failed")
                            .replace("{{error}}", error.as_ref()),
                    ),
            );
        }
        let manage_button = self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.general.manage_login_items"),
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
                let result = oxideterm_gpui_platform::autostart::open_login_items_settings()
                    .map_err(|error| error.to_string());
                this.settings_workspace.update(cx, |settings, cx| {
                    settings.finish_launch_at_login_settings_handoff(result, cx);
                });
                cx.stop_propagation();
            }),
        );

        self.settings_card(
            "settings_view.general.startup",
            "settings_view.general.startup_hint",
            vec![
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .child(description)
                    .child(manage_button)
                    .into_any_element(),
            ],
        )
    }

    #[cfg(not(target_os = "macos"))]
    pub(in crate::workspace) fn launch_at_login_settings_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let launch_at_login = self.settings_workspace.read(cx).launch_at_login_snapshot();
        let loading = launch_at_login.pending;
        let enabled = launch_at_login.enabled;
        let error = launch_at_login.error;
        let control = div()
            .flex_none()
            .opacity(if loading { 0.55 } else { 1.0 })
            .child(
                checkbox(&self.tokens, String::new(), enabled).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if !this
                            .settings_workspace
                            .read(cx)
                            .launch_at_login_snapshot()
                            .pending
                        {
                            this.set_launch_at_login_enabled(!enabled, cx);
                        }
                        cx.stop_propagation();
                    }),
                ),
            );
        let mut label = div()
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
                    .child(self.i18n.t("settings_view.general.launch_at_login")),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.general.launch_at_login_hint")),
            );
        if loading {
            label = label.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.general.launch_at_login_updating"),
                    ),
            );
        }
        if let Some(error) = error {
            let error = match error {
                LaunchAtLoginError::ApprovalRequired => self
                    .i18n
                    .t("settings_view.general.launch_at_login_approval_required"),
                LaunchAtLoginError::OperationFailed(error)
                | LaunchAtLoginError::TaskFailed(error) => error.to_string(),
            };
            label = label.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(
                        self.i18n
                            .t("settings_view.general.launch_at_login_failed")
                            .replace("{{error}}", error.as_ref()),
                    ),
            );
        }

        self.settings_card(
            "settings_view.general.startup",
            "settings_view.general.startup_hint",
            vec![
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .child(label)
                    .child(control)
                    .into_any_element(),
            ],
        )
    }

    #[cfg(not(target_os = "macos"))]
    pub(in crate::workspace) fn refresh_launch_at_login_status(&mut self, cx: &mut Context<Self>) {
        let runtime = self.forwarding_runtime.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_launch_at_login_operation(
                async move {
                    let result = runtime
                        .spawn_blocking(oxideterm_gpui_platform::autostart::is_enabled)
                        .await
                        .map_err(|error| {
                            LaunchAtLoginError::TaskFailed(error.to_string().into())
                        })?;
                    result.map_err(|error| {
                        LaunchAtLoginError::OperationFailed(error.to_string().into())
                    })
                },
                cx,
            );
        });
    }

    #[cfg(not(target_os = "macos"))]
    pub(in crate::workspace) fn set_launch_at_login_enabled(
        &mut self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.forwarding_runtime.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_launch_at_login_operation(
                async move {
                    let result = runtime
                        .spawn_blocking(move || {
                            oxideterm_gpui_platform::autostart::set_enabled(enabled)?;
                            let actual = oxideterm_gpui_platform::autostart::is_enabled()?;
                            if actual != enabled {
                                return Err(std::io::Error::other(
                                    "the operating system did not retain the startup setting",
                                ));
                            }
                            Ok(actual)
                        })
                        .await
                        .map_err(|error| {
                            LaunchAtLoginError::TaskFailed(error.to_string().into())
                        })?;
                    result.map_err(|error| {
                        if error.kind() == std::io::ErrorKind::PermissionDenied {
                            LaunchAtLoginError::ApprovalRequired
                        } else {
                            LaunchAtLoginError::OperationFailed(error.to_string().into())
                        }
                    })
                },
                cx,
            );
        });
    }

    pub(in crate::workspace) fn general_checkbox_row(
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
            .child(div().flex_none().child(
                checkbox(&self.tokens, String::new(), checked).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(|settings| setter(settings, !checked), cx);
                        cx.stop_propagation();
                    }),
                ),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_data_directory_info(
        &self,
    ) -> oxideterm_settings::DataDirectoryInfo {
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

    pub(in crate::workspace) fn settings_data_directory_change_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

    pub(in crate::workspace) fn settings_data_directory_reset_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                this.settings_workspace.update(cx, |settings, cx| {
                    settings.open_data_directory_reset_confirm(cx);
                });
                this.reset_standard_confirm_focus();
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn pick_settings_data_directory(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.general.select_data_directory"),
            )),
        });
        let selection = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            paths.into_iter().next()
        };
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_data_directory_picker(selection, cx);
        });
    }

    pub(in crate::workspace) fn cancel_settings_data_directory_confirm(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.clear_standard_confirm_focus();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.begin_data_directory_confirm_exit(false, delay, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn confirm_settings_data_directory(&mut self, cx: &mut Context<Self>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.clear_standard_confirm_focus();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.begin_data_directory_confirm_exit(true, delay, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn render_settings_data_directory_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let settings_workspace = self.settings_workspace.read(cx);
        let confirm = settings_workspace.data_directory_confirm()?;
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
        Some(
            oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
                &self.tokens,
                "settings-data-directory-confirm-motion",
                settings_workspace.data_directory_confirm_phase(),
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
            ),
        )
    }

    pub(in crate::workspace) fn settings_terminal_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        if section_index == 0 {
            return self.terminal_page_switcher(cx);
        }

        let terminal_page = self
            .settings_workspace
            .read(cx)
            .route_snapshot()
            .terminal_page;
        match (terminal_page, section_index - 1) {
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
                    self.card_separator(),
                    self.decimal_row(
                        "settings_view.terminal.font_weight",
                        "settings_view.terminal.font_weight_hint",
                        SettingsInput::TerminalFontWeight,
                        settings.terminal.font_weight.to_string(),
                        cx,
                    ),
                    self.terminal_preview(settings),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.font_ligatures",
                        "settings_view.terminal.font_ligatures_hint",
                        settings.terminal.font_ligatures,
                        set_font_ligatures,
                        cx,
                    ),
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
                        "settings_view.terminal.show_performance_overlay",
                        "settings_view.terminal.show_performance_overlay_hint",
                        settings.terminal.show_fps_overlay,
                        set_show_terminal_performance_overlay,
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
            (TerminalSettingsPage::Display, 2) => self.settings_card(
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
            (TerminalSettingsPage::Display, 3) => self.settings_card(
                "settings_view.terminal.buffer",
                "settings_view.terminal.scrollback_hint",
                vec![
                    self.number_row(
                        "settings_view.terminal.scrollback",
                        "settings_view.terminal.scrollback_hint",
                        SettingsInput::TerminalScrollback,
                        settings.terminal.scrollback,
                        cx,
                    ),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.highlight_tab_on_new_output",
                        "settings_view.terminal.highlight_tab_on_new_output_hint",
                        settings.terminal.highlight_tab_on_new_output,
                        set_highlight_tab_on_new_output,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::Input, 0) => self.terminal_input_settings_card(settings, cx),
            (TerminalSettingsPage::Local, local_section_index) => {
                self.settings_local_section(local_section_index, cx)
            }
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
                        settings.terminal.command_bar.show_current_directory,
                        set_command_bar_show_current_directory,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::CommandBar, 1) => self.settings_card(
                "settings_view.terminal.command_bar_focus_handoff",
                "settings_view.terminal.command_bar_focus_handoff_hint",
                vec![self.focus_handoff_commands_row(settings, cx)],
            ),
            (TerminalSettingsPage::CommandBar, 2) => self.settings_card(
                "settings_view.terminal.quick_commands",
                "settings_view.terminal.quick_commands_hint",
                vec![
                    self.bool_row(
                        "settings_view.terminal.quick_commands",
                        "settings_view.terminal.quick_commands_hint",
                        settings.terminal.command_bar.quick_commands_enabled,
                        set_quick_commands_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_bar",
                        "settings_view.terminal.quick_bar_hint",
                        settings.terminal.command_bar.quick_bar_enabled,
                        set_quick_bar_enabled,
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
            (TerminalSettingsPage::Awareness, 0) => self.settings_card(
                "settings_view.terminal.awareness_title",
                "settings_view.terminal.awareness_description",
                vec![
                    self.bool_row(
                        "settings_view.terminal.awareness_enabled",
                        "settings_view.terminal.awareness_enabled_hint",
                        settings.terminal.command_bar.current_directory_awareness,
                        set_command_bar_current_directory_awareness,
                        cx,
                    ),
                    self.card_separator(),
                    self.select_setting_row(
                        "settings_view.connections.shell_integration.mode_label",
                        "settings_view.connections.shell_integration.mode_hint",
                        SettingsSelect::RemoteShellIntegrationMode,
                        remote_shell_integration_mode_label(
                            settings.terminal.remote_shell_integration_mode,
                            &self.i18n,
                        ),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::Awareness, 1) => self.remote_shell_integration_card(cx),
            (TerminalSettingsPage::Awareness, 2) => self.terminal_triggers_settings_card(cx),
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
            (TerminalSettingsPage::Logging, 0) => {
                let directory = self
                    .settings_store
                    .path()
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("logs")
                    .join("terminal");
                self.settings_card(
                    "settings_view.terminal.session_log_title",
                    "settings_view.terminal.session_log_description",
                    vec![
                        self.checkbox_row(
                            "settings_view.terminal.session_log_automatic",
                            "settings_view.terminal.session_log_automatic_hint",
                            settings.terminal.session_log.automatic,
                            set_terminal_session_log_automatic,
                            cx,
                        ),
                        self.card_separator(),
                        self.setting_row(
                            "settings_view.terminal.session_log_file_name_template",
                            "settings_view.terminal.session_log_file_name_template_hint",
                            self.settings_text_input_control(
                                SettingsInput::TerminalSessionLogFileNameTemplate,
                                &settings.terminal.session_log.file_name_template,
                                "{date}_{time}_{protocol}_{session}.log".to_string(),
                                420.0,
                                cx,
                            ),
                            cx,
                        ),
                        self.card_separator(),
                        self.select_setting_row(
                            "settings_view.terminal.session_log_file_mode",
                            "settings_view.terminal.session_log_file_mode_hint",
                            SettingsSelect::TerminalSessionLogFileMode,
                            terminal_session_log_file_mode_label(
                                settings.terminal.session_log.file_mode,
                                &self.i18n,
                            ),
                            self.tokens.metrics.settings_select_width,
                            cx,
                        ),
                        self.card_separator(),
                        self.setting_row(
                            "settings_view.terminal.session_log_content_template",
                            "settings_view.terminal.session_log_content_template_hint",
                            self.settings_text_input_control(
                                SettingsInput::TerminalSessionLogContentTemplate,
                                &settings.terminal.session_log.content_template,
                                "[{timestamp}] {text}".to_string(),
                                420.0,
                                cx,
                            ),
                            cx,
                        ),
                        self.card_separator(),
                        self.checkbox_row(
                            "settings_view.terminal.session_log_control_sequences",
                            "settings_view.terminal.session_log_control_sequences_hint",
                            settings.terminal.session_log.include_control_sequences,
                            set_terminal_session_log_include_control_sequences,
                            cx,
                        ),
                        self.card_separator(),
                        self.number_row(
                            "settings_view.terminal.session_log_retention_days",
                            "settings_view.terminal.session_log_retention_days_hint",
                            SettingsInput::TerminalSessionLogRetentionDays,
                            settings.terminal.session_log.retention_days,
                            cx,
                        ),
                        self.card_separator(),
                        self.number_row(
                            "settings_view.terminal.session_log_max_file_size",
                            "settings_view.terminal.session_log_max_file_size_hint",
                            SettingsInput::TerminalSessionLogMaxFileSizeMib,
                            settings.terminal.session_log.max_file_size_mib,
                            cx,
                        ),
                        self.card_separator(),
                        self.setting_row(
                            "settings_view.terminal.session_log_directory",
                            "settings_view.terminal.session_log_directory_hint",
                            div()
                                .max_w(px(420.0))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(directory.to_string_lossy().to_string())
                                .into_any_element(),
                            cx,
                        ),
                    ],
                )
            }
            (TerminalSettingsPage::Highlight, 0) => self.highlight_rules_card(settings, cx),
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn focus_handoff_commands_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = SettingsInput::TerminalCommandBarFocusHandoff;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.current_settings_input_value(input, cx)
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut command_editor = div()
            .w_full()
            .h(px(44.0))
            .overflow_hidden()
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
                    let current = this.current_settings_input_value(input, cx);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle, cx);
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
            command_editor = self.render_settings_multiline_textarea_lines(
                command_editor,
                target,
                "aider, custom-tui",
                true,
                line_height,
                cx,
            );
        } else {
            command_editor = self.render_settings_multiline_textarea_lines(
                command_editor,
                target,
                &value,
                false,
                line_height,
                cx,
            );
        }

        if let Some(marked) = self.marked_text_for_target(target, cx) {
            command_editor = command_editor.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }

        let control = text_input_anchor_probe(
            target.anchor_id(),
            command_editor,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        );

        let mut presets = div().w_full().flex().flex_row().flex_wrap().gap(px(8.0));
        for command in RECOMMENDED_FOCUS_HANDOFF_COMMANDS {
            let enabled = settings
                .terminal
                .command_bar
                .focus_handoff_commands
                .iter()
                .any(|candidate| candidate == command);
            let command_name = (*command).to_string();
            let theme = self.tokens.ui;
            let chip = div()
                .h(px(32.0))
                .px(px(12.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(if enabled {
                    rgba((theme.accent << 8) | 0x99)
                } else {
                    rgba((theme.border << 8) | 0x99)
                })
                .bg(if enabled {
                    rgba((theme.accent << 8) | 0x20)
                } else {
                    rgba(0x00000000)
                })
                .flex()
                .items_center()
                .gap(px(6.0))
                .cursor_pointer()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .font_family(settings_mono_font_family(self.settings_store.settings()))
                .text_color(if enabled {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                })
                .hover(move |style| {
                    style
                        .border_color(rgba((theme.accent << 8) | 0x88))
                        .bg(rgb(theme.bg_hover))
                        .text_color(rgb(theme.text))
                })
                .when(enabled, |chip| chip.child("✓"))
                .child(command_name.clone())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(input) = this.focused_settings_input.take() {
                            this.clear_settings_input_draft(input);
                        }
                        let command_name = command_name.clone();
                        this.edit_settings(
                            move |settings| {
                                let commands =
                                    &mut settings.terminal.command_bar.focus_handoff_commands;
                                if let Some(index) = commands
                                    .iter()
                                    .position(|candidate| candidate == &command_name)
                                {
                                    commands.remove(index);
                                } else {
                                    commands.push(command_name);
                                }
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                );
            presets = presets.child(chip);
        }

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
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.command_bar_focus_handoff_presets"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t(
                                "settings_view.terminal.command_bar_focus_handoff_presets_hint",
                            )),
                    ),
            )
            .child(presets)
            .child(self.card_separator())
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
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.command_bar_focus_handoff_advanced"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t(
                                "settings_view.terminal.command_bar_focus_handoff_advanced_hint",
                            )),
                    ),
            )
            .child(control)
            .into_any_element()
    }

    pub(in crate::workspace) fn terminal_command_specs_editor_row(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        let theme = self.tokens.ui;

        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.metrics.modal_section_gap))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.one))
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
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(self.tokens.metrics.modal_field_gap))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    ))
                                    .truncate()
                                    .child(path.display().to_string()),
                            )
                            .child(self.terminal_command_specs_summary()),
                    ),
            )
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("settings_view.terminal.command_specs_edit"),
                Some(Self::render_lucide_icon(
                    LucideIcon::Pencil,
                    TERMINAL_COMMAND_SPECS_ACTION_ICON_SIZE,
                    rgb(theme.text),
                )),
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                    ..ToolbarButtonOptions::default()
                },
                cx.listener(|this, _event, window, cx| {
                    this.open_terminal_command_specs_editor(window, cx);
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_terminal_command_specs_editor_modal(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        let theme = self.tokens.ui;
        let dialog = oxideterm_gpui_ui::modal_container(&self.tokens)
            .w(px(TERMINAL_COMMAND_SPECS_MODAL_WIDTH))
            .max_w_full()
            .h(px(TERMINAL_COMMAND_SPECS_MODAL_HEIGHT))
            .max_h_full()
            .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&self.tokens))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(
                        &self.tokens,
                        self.i18n.t("settings_view.terminal.command_specs"),
                    ))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n.t("settings_view.terminal.command_specs_hint"),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_color(rgb(theme.text_muted))
                            .truncate()
                            .child(path.display().to_string()),
                    ),
            )
            .child(
                // The modal body is the sole scroll owner. The editor grows with
                // its JSON document instead of introducing a nested scroll view.
                oxideterm_gpui_ui::modal::modal_body(&self.tokens)
                    .id("terminal-command-specs-editor-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .selectable_overflow_y_scrollbar(
                        &self.selectable_text_scroll_handle("terminal-command-specs-editor-scroll"),
                    )
                    .bg(rgb(theme.bg_elevated))
                    .child(self.terminal_command_specs_editor_control(cx)),
            )
            .child(
                dialog_footer(&self.tokens)
                    .justify_between()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.terminal_command_specs_summary()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(self.tokens.metrics.modal_field_gap))
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
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.terminal.command_specs_close"),
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
                                    this.close_terminal_command_specs_editor(cx);
                                    cx.stop_propagation();
                                }),
                            ))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_save",
                                TerminalCommandSpecsAction::Save,
                                cx,
                            )),
                    ),
            );

        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_terminal_command_specs_editor(cx);
                    cx.stop_propagation();
                }),
            )
            .child(overlay_content_boundary(dialog))
            .into_any_element()
    }

    fn terminal_command_specs_editor_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let input = SettingsInput::TerminalCommandSpecsJson;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.terminal_command_specs_editor_initial_value()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut textarea = div()
            .w_full()
            .min_h(px(TERMINAL_COMMAND_SPECS_EDITOR_MIN_HEIGHT))
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
                    let current = this.current_settings_input_value(input, cx);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle, cx);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );

        textarea = self.render_settings_multiline_textarea_lines(
            textarea,
            target,
            &value,
            false,
            line_height,
            cx,
        );
        if let Some(marked) = self.marked_text_for_target(target, cx) {
            textarea = textarea.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }
        let control =
            text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            });

        control.into_any_element()
    }

    pub(in crate::workspace) fn open_terminal_command_specs_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self.terminal_command_specs_editor_initial_value();
        self.prepare_modal_interaction_boundary(cx);
        self.terminal_command_specs_editor_open = true;
        self.focus_settings_input(SettingsInput::TerminalCommandSpecsJson, value, cx);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_terminal_command_specs_editor(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.terminal_command_specs_editor_open = false;
        if self.focused_settings_input == Some(SettingsInput::TerminalCommandSpecsJson) {
            self.focused_settings_input = None;
            self.clear_settings_input_draft(SettingsInput::TerminalCommandSpecsJson);
            self.clear_ime_selection();
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(in crate::workspace) fn render_settings_multiline_textarea_lines(
        &self,
        mut textarea: Div,
        target: WorkspaceImeTarget,
        value: &str,
        placeholder: bool,
        line_height: f32,
        cx: &App,
    ) -> Div {
        let selection = self.ime_selected_range_for_target(target, cx);
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
                line = line
                    .text_color(rgb(theme.text_muted))
                    .child(line_text.as_str().to_string());
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
                    self.input_caret.visible(),
                ));
            }
            textarea = textarea.child(line);
        }
        textarea
    }

    pub(in crate::workspace) fn terminal_command_specs_button(
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

    pub(in crate::workspace) fn load_terminal_command_specs_editor_value(&self) -> String {
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        std::fs::read_to_string(path).unwrap_or_default()
    }

    pub(in crate::workspace) fn terminal_command_specs_editor_initial_value(&self) -> String {
        super::terminal_command_bar::completion::terminal_command_specs_editor_initial_json(
            &self.load_terminal_command_specs_editor_value(),
        )
    }

    pub(in crate::workspace) fn terminal_command_specs_summary(&self) -> String {
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

    pub(in crate::workspace) fn current_terminal_command_specs_editor_value(&self) -> String {
        if self.focused_settings_input == Some(SettingsInput::TerminalCommandSpecsJson) {
            self.settings_input_draft.clone()
        } else {
            self.terminal_command_specs_editor_initial_value()
        }
    }

    pub(in crate::workspace) fn handle_terminal_command_specs_action(
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
                window.focus(&self.focus_handle, cx);
            }
            TerminalCommandSpecsAction::Format => {
                let value = self.current_terminal_command_specs_editor_value();
                match super::terminal_command_bar::completion::normalize_terminal_command_specs_json(
                    &value,
                ) {
                    Ok(pretty) => {
                        self.focus_settings_input(input, pretty, cx);
                        window.focus(&self.focus_handle, cx);
                    }
                    Err(error) => self.push_ai_settings_toast(
                        format!(
                            "{} {}",
                            self.i18n.t("settings_view.terminal.command_specs_invalid"),
                            error
                        ),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
            }
            TerminalCommandSpecsAction::Save => {
                let value = self.current_terminal_command_specs_editor_value();
                match super::terminal_command_bar::completion::normalize_terminal_command_specs_json(
                    &value,
                ) {
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
                                    cx,
                                );
                                cx.notify();
                            }
                            Err(error) => self.push_ai_settings_toast(
                                error.to_string(),
                                TerminalNoticeVariant::Error,
                                cx,
                            ),
                        }
                    }
                    Err(error) => self.push_ai_settings_toast(
                        format!(
                            "{} {}",
                            self.i18n.t("settings_view.terminal.command_specs_invalid"),
                            error
                        ),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
            }
        }
    }

    pub(in crate::workspace) fn in_band_transfer_number_row(
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

    pub(in crate::workspace) fn in_band_transfer_runtime_note(&self) -> AnyElement {
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

pub(in crate::workspace) fn close_to_background_label_keys() -> (&'static str, &'static str) {
    if cfg!(target_os = "macos") {
        (
            "settings_view.general.keep_running_on_close",
            "settings_view.general.keep_running_on_close_hint",
        )
    } else {
        (
            "settings_view.general.minimize_to_tray_on_close",
            "settings_view.general.minimize_to_tray_on_close_hint",
        )
    }
}
