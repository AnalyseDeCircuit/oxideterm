use super::super::local_terminal::local_shell_supports_oh_my_posh;
use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_local_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        match section_index {
            0 => {
                let mut shell_rows = vec![self.local_shell_select_row(settings, cx)];
                if let Some(path_hint) = self.local_shell_path_hint(settings) {
                    shell_rows.push(path_hint);
                }
                shell_rows.push(self.card_separator());
                shell_rows.push(
                    self.setting_row(
                        "settings_view.local_terminal.git_bash_path",
                        "settings_view.local_terminal.git_bash_path_hint",
                        self.settings_text_input_control(
                            SettingsInput::LocalGitBashPath,
                            settings
                                .local_terminal
                                .git_bash_path
                                .clone()
                                .unwrap_or_default(),
                            self.i18n
                                .t("settings_view.local_terminal.git_bash_path_placeholder"),
                            300.0,
                            cx,
                        ),
                        cx,
                    ),
                );
                shell_rows.push(self.card_separator());
                shell_rows.push(
                    self.setting_row(
                        "settings_view.local_terminal.default_cwd",
                        "settings_view.local_terminal.default_cwd_hint",
                        self.settings_text_input_control(
                            SettingsInput::LocalDefaultCwd,
                            settings
                                .local_terminal
                                .default_cwd
                                .clone()
                                .unwrap_or_default(),
                            "~".to_string(),
                            self.tokens.metrics.settings_select_width,
                            cx,
                        ),
                        cx,
                    ),
                );
                if cfg!(target_os = "windows")
                    && local_shell_supports_oh_my_posh(
                        settings.local_terminal.default_shell_id.as_deref(),
                    )
                {
                    shell_rows.push(self.card_separator());
                    shell_rows.extend(self.local_oh_my_posh_rows(settings, cx));
                }
                self.settings_card(
                    "settings_view.local_terminal.shell",
                    "settings_view.local_terminal.default_shell_hint",
                    shell_rows,
                )
            }
            1 => self.settings_card(
                "settings_view.local_terminal.shell_profile",
                "settings_view.local_terminal.load_shell_profile_hint",
                vec![self.checkbox_row(
                    "settings_view.local_terminal.load_shell_profile",
                    "settings_view.local_terminal.load_shell_profile_hint",
                    settings.local_terminal.load_shell_profile,
                    set_load_shell_profile,
                    cx,
                )],
            ),
            2 => self.local_privilege_credentials_card(cx),
            3 => {
                let effective_shells = self.effective_local_shells_for_settings(settings);
                let shell_list = if effective_shells.is_empty() {
                    div()
                        .text_align(gpui::TextAlign::Center)
                        .py(px(32.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.local_terminal.loading_shells"))
                        .into_any_element()
                } else {
                    let shell_count = effective_shells.len();
                    let mut list = div().w_full().min_w(px(0.0)).flex().flex_col();
                    for (index, shell) in effective_shells.iter().enumerate() {
                        list = list.child(self.available_shell_row(
                            shell,
                            settings.local_terminal.default_shell_id.as_deref(),
                        ));
                        if index + 1 < shell_count {
                            list = list.child(self.card_separator());
                        }
                    }
                    list.into_any_element()
                };
                self.settings_card(
                    "settings_view.local_terminal.available_shells",
                    "settings_view.local_terminal.select_shell",
                    vec![shell_list],
                )
            }
            _ => div().into_any_element(),
        }
    }

    fn local_oh_my_posh_rows(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = vec![
            div()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(self.tokens.ui.text))
                .child(self.i18n.t("settings_view.local_terminal.oh_my_posh"))
                .into_any_element(),
            self.checkbox_row(
                "settings_view.local_terminal.oh_my_posh_enable",
                "settings_view.local_terminal.oh_my_posh_enable_hint",
                settings.local_terminal.oh_my_posh_enabled,
                set_oh_my_posh,
                cx,
            ),
        ];
        if settings.local_terminal.oh_my_posh_enabled {
            rows.push(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.info << 8) | 0x33))
                    .bg(rgba((self.tokens.ui.info << 8) | 0x1a))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.info))
                            .child(format!(
                                "💡 {}",
                                self.i18n.t("settings_view.local_terminal.oh_my_posh_note")
                            )),
                    )
                    .into_any_element(),
            );
            rows.push(self.card_separator());
            rows.push(
                self.setting_row(
                    "settings_view.local_terminal.oh_my_posh_theme",
                    "settings_view.local_terminal.oh_my_posh_theme_hint",
                    self.settings_text_input_control(
                        SettingsInput::LocalOhMyPoshTheme,
                        settings
                            .local_terminal
                            .oh_my_posh_theme
                            .clone()
                            .unwrap_or_default(),
                        self.i18n
                            .t("settings_view.local_terminal.oh_my_posh_theme_placeholder"),
                        300.0,
                        cx,
                    ),
                    cx,
                ),
            );
        }
        rows
    }

    pub(in crate::workspace) fn local_privilege_credentials_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let credential_count = self
            .connection_store
            .list_privilege_credentials(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
            .map(|credentials| credentials.len())
            .unwrap_or_default();
        let scope_id = LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string();
        let summary = div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(rgba((theme.bg << 8) | 0x80))
            .p(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(Self::render_lucide_icon(
                                LucideIcon::KeyRound,
                                16.0,
                                rgb(theme.text_muted),
                            ))
                            .child(self.i18n_replace(
                                "settings_view.privilege_credentials.credential_count",
                                &[("count", credential_count.to_string())],
                            )),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.privilege_credentials.description"),
                            ),
                    ),
            )
            .child(
                self.workspace_toolbar_action_button(
                    self.i18n.t("terminal.privilege_helper.manage"),
                    Some(
                        Self::render_lucide_icon(LucideIcon::Settings, 14.0, rgb(theme.text_muted))
                            .into_any_element(),
                    ),
                    ToolbarButtonOptions {
                        button: ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            ..ButtonOptions::default()
                        },
                        ..ToolbarButtonOptions::default()
                    },
                    cx.listener(move |this, _event, window, cx| {
                        // Local terminal settings intentionally delegate
                        // credential editing to the unified privilege surface.
                        this.open_privilege_credentials_settings(
                            Some(scope_id.clone()),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ),
            );

        self.settings_card(
            "settings_view.local_terminal.privilege_credentials",
            "settings_view.local_terminal.privilege_credentials_hint",
            vec![summary.into_any_element()],
        )
    }

    pub(in crate::workspace) fn settings_reconnect_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if section_index != 0 {
            return div().into_any_element();
        }

        let settings = self.settings_store.settings();
        let reconnect_enabled = settings.reconnect.enabled;
        // Keep reconnect controls in one virtual-list item so the shared card
        // surface cannot be split by list spacing or independent measurement.
        let strategy = div()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .opacity(if reconnect_enabled { 1.0 } else { 0.4 })
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_heading))
                    .child(self.i18n.t("settings_view.reconnect.strategy")),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
                    .flex()
                    .flex_row()
                    .gap(px(32.0))
                    .child(self.reconnect_select_field(
                        "settings_view.reconnect.max_attempts",
                        "settings_view.reconnect.max_attempts_hint",
                        SettingsSelect::ReconnectMaxAttempts,
                        reconnect_attempt_label(settings.reconnect.max_attempts),
                        reconnect_enabled,
                        cx,
                    ))
                    .child(self.reconnect_select_field(
                        "settings_view.reconnect.base_delay",
                        "settings_view.reconnect.base_delay_hint",
                        SettingsSelect::ReconnectBaseDelay,
                        reconnect_delay_label(settings.reconnect.base_delay_ms),
                        reconnect_enabled,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
                    .flex()
                    .flex_row()
                    .gap(px(32.0))
                    .child(self.reconnect_select_field(
                        "settings_view.reconnect.max_delay",
                        "settings_view.reconnect.max_delay_hint",
                        SettingsSelect::ReconnectMaxDelay,
                        reconnect_delay_label(settings.reconnect.max_delay_ms),
                        reconnect_enabled,
                        cx,
                    )),
            )
            .child(
                div()
                    .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
                    .pt(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.reconnect.formula_hint")),
            )
            .into_any_element();

        self.settings_card(
            "settings_view.reconnect.title",
            "settings_view.reconnect.description",
            vec![
                self.reconnect_enabled_row(reconnect_enabled, cx),
                self.card_separator(),
                strategy,
            ],
        )
    }

    pub(in crate::workspace) fn reconnect_enabled_row(
        &self,
        checked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .max_w(px(SETTINGS_RECONNECT_MAX_WIDTH))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .grid()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.reconnect.enabled")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.reconnect.enabled_hint")),
                    ),
            )
            .child(
                checkbox(&self.tokens, String::new(), checked)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| set_reconnect_enabled(settings, !checked),
                                cx,
                            );
                        }),
                    )
                    .into_any_element(),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn reconnect_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = self.settings_select_control(select_id, value, !enabled, None, cx);

        div()
            .w(px(SETTINGS_RECONNECT_FIELD_WIDTH))
            .min_w(px(0.0))
            .grid()
            .gap(px(8.0))
            .child(
                div()
                    .grid()
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
                            .w_full()
                            .min_w_0()
                            .whitespace_normal()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(SETTINGS_RECONNECT_HINT_LINE_HEIGHT))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(control)
            .when(!enabled, |field| {
                field.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
            })
            .into_any_element()
    }
}
