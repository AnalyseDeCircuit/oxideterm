impl WorkspaceApp {
    fn settings_local_section(&self, section_index: usize, cx: &mut Context<Self>) -> AnyElement {
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
            2 => {
                let mut oh_my_posh_rows = vec![self.checkbox_row(
                    "settings_view.local_terminal.oh_my_posh_enable",
                    "settings_view.local_terminal.oh_my_posh_enable_hint",
                    settings.local_terminal.oh_my_posh_enabled,
                    set_oh_my_posh,
                    cx,
                )];
                if settings.local_terminal.oh_my_posh_enabled {
                    oh_my_posh_rows.push(
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
                                        self.i18n
                                            .t("settings_view.local_terminal.oh_my_posh_note")
                                    )),
                            )
                            .into_any_element(),
                    );
                    oh_my_posh_rows.push(self.card_separator());
                    oh_my_posh_rows.push(
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
                self.settings_card(
                "settings_view.local_terminal.oh_my_posh",
                "settings_view.local_terminal.oh_my_posh_note",
                oh_my_posh_rows,
                )
            }
            3 => {
                let shortcut_default = if cfg!(target_os = "macos") {
                    "⌘T"
                } else {
                    "Ctrl+T"
                };
                let shortcut_launcher = if cfg!(target_os = "macos") {
                    "⌘⇧T"
                } else {
                    "Ctrl+Shift+T"
                };
                self.settings_card(
                "settings_view.local_terminal.shortcuts",
                "settings_view.local_terminal.custom_env_hint",
                vec![
                    self.local_shortcut_row(
                        "settings_view.local_terminal.new_default_shell",
                        shortcut_default,
                    ),
                    self.card_separator(),
                    self.local_shortcut_row(
                        "settings_view.local_terminal.new_shell_launcher",
                        shortcut_launcher,
                    ),
                ],
                )
            }
            4 => {
                let effective_shells = self.effective_local_shells_for_settings(settings);
                let shell_list = if effective_shells.is_empty() {
                    vec![
                        div()
                            .text_align(gpui::TextAlign::Center)
                            .py(px(32.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.local_terminal.loading_shells"))
                            .into_any_element(),
                    ]
                } else {
                    effective_shells
                        .iter()
                        .map(|shell| {
                            self.available_shell_row(
                                shell,
                                settings.local_terminal.default_shell_id.as_deref(),
                            )
                        })
                        .collect()
                };
                self.settings_card(
                "settings_view.local_terminal.available_shells",
                "settings_view.local_terminal.select_shell",
                shell_list,
                )
            }
            _ => div().into_any_element(),
        }
    }

    fn settings_reconnect_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let reconnect_enabled = settings.reconnect.enabled;
        match section_index {
            0 => self.reconnect_enabled_row(reconnect_enabled, cx),
            1 => separator(&self.tokens, SeparatorOrientation::Horizontal).into_any_element(),
            2 => div()
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
                        .p(px(16.0))
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                        .bg(self.settings_panel_background(self.tokens.ui.bg_card))
                        .shadow(oxideterm_gpui_ui::tauri_card_shadow(
                            self.tokens.ui.bg_card,
                        ))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.reconnect.formula_hint")),
                )
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }

    fn reconnect_enabled_row(&self, checked: bool, cx: &mut Context<Self>) -> AnyElement {
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

    fn reconnect_select_field(
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
