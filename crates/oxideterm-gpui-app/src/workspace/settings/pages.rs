const SFTP_SETTINGS_CARD_PADDING: f32 = 20.0; // Tauri p-5
const SFTP_SETTINGS_SELECT_WIDTH: f32 = 180.0; // Tauri w-[180px]
const IDE_SETTINGS_CARD_PADDING: f32 = 20.0; // Tauri p-5.
const IDE_SETTINGS_CARD_GAP: f32 = 16.0; // Tauri space-y-4.
const IDE_SETTINGS_TOGGLE_CARD_GAP: f32 = 16.0; // Tauri flex gap between copy and control.
const IDE_SETTINGS_INPUT_WIDTH: f32 = 80.0; // Tauri w-20.
const IDE_SETTINGS_AGENT_SELECT_WIDTH: f32 = 160.0; // Tauri w-40.
const IDE_SETTINGS_AGENT_DOT_SIZE: f32 = 4.0; // Tauri w-1 h-1.
const IDE_SETTINGS_AGENT_DOT_TOP_MARGIN: f32 = 6.0; // Tauri mt-1.5.
const IDE_SETTINGS_AGENT_PRIVACY_BORDER_ALPHA: u8 = 0x33; // Tauri blue-500/20.
const IDE_SETTINGS_AGENT_PRIVACY_BG_ALPHA: u8 = 0x0d; // Tauri blue-500/5.
const IDE_SETTINGS_AGENT_SUBTLE_BORDER_ALPHA: u8 = 0x80; // Tauri border/50.
const IDE_SETTINGS_EMERALD_400: u32 = 0x34d399;
const IDE_SETTINGS_BLUE_400: u32 = 0x60a5fa;
const IDE_SETTINGS_BLUE_500: u32 = 0x3b82f6;

impl WorkspaceApp {
    fn settings_local(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
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
            ),
        );

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
                                self.i18n.t("settings_view.local_terminal.oh_my_posh_note")
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
                ),
            );
        }

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

        vec![
            self.settings_card(
                "settings_view.local_terminal.shell",
                "settings_view.local_terminal.default_shell_hint",
                shell_rows,
            ),
            self.settings_card(
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
            self.settings_card(
                "settings_view.local_terminal.oh_my_posh",
                "settings_view.local_terminal.oh_my_posh_note",
                oh_my_posh_rows,
            ),
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
            ),
            self.settings_card(
                "settings_view.local_terminal.available_shells",
                "settings_view.local_terminal.select_shell",
                shell_list,
            ),
        ]
    }

    fn settings_connections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        let existing_names = self
            .connection_store
            .connections()
            .iter()
            .map(|conn| conn.name.clone())
            .collect::<HashSet<_>>();
        let ssh_hosts = list_ssh_config_hosts(&existing_names).unwrap_or_default();

        vec![
            self.connection_defaults_section(settings, cx),
            self.connection_groups_section(cx),
            self.connection_section(
                "settings_view.connections.idle_timeout.title",
                "settings_view.connections.idle_timeout.description",
                vec![self.connection_idle_timeout_control(settings, cx)],
            ),
            self.ssh_config_import_section(ssh_hosts, cx),
        ]
    }

    fn settings_ssh(&self) -> Vec<AnyElement> {
        let keys = list_available_ssh_keys();
        if keys.is_empty() {
            vec![
                div()
                    .max_w(px(768.0))
                    .child(self.ssh_keys_empty_state())
                    .into_any_element(),
            ]
        } else {
            let mut list = div().max_w(px(768.0)).flex().flex_col().gap(px(12.0));
            for key in keys {
                list = list.child(self.ssh_key_row(key));
            }
            vec![list.into_any_element()]
        }
    }

    fn connection_defaults_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .max_w(px(672.0))
            .grid()
            .grid_cols(2)
            .gap(px(32.0))
            .child(self.connection_labeled_input(
                "settings_view.connections.default_username",
                SettingsInput::ConnectionDefaultUsername,
                settings.connection_defaults.username.clone(),
                settings.connection_defaults.username.clone(),
                cx,
            ))
            .child(self.connection_labeled_input(
                "settings_view.connections.default_port",
                SettingsInput::ConnectionDefaultPort,
                settings.connection_defaults.port.to_string(),
                "22".to_string(),
                cx,
            ))
            .into_any_element()
    }

    fn connection_labeled_input(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .grid()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.settings_text_input_control(
                input,
                value,
                placeholder,
                self.tokens.metrics.settings_select_width,
                cx,
            ))
            .into_any_element()
    }

    fn connection_idle_timeout_control(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select_id = SettingsSelect::ConnectionIdleTimeout;
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let value =
            connection_idle_timeout_label(settings.connection_pool.idle_timeout_secs, &self.i18n);
        let trigger = select_trigger(&self.tokens, value, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        div()
            .max_w(px(320.0))
            .grid()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.connections.idle_timeout.label")),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .child(select_anchor_probe(anchor_id, trigger, move |anchor, _window, cx| {
                        let _ = workspace.update(cx, |this, cx| {
                            this.update_select_anchor(anchor, cx);
                        });
                    })),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.connections.idle_timeout.hint")),
            )
            .into_any_element()
    }

    fn connection_groups_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = vec![
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .max_w(px(448.0))
                .child(
                    self.settings_text_input_control(
                        SettingsInput::ConnectionNewGroup,
                        self.settings_connection_new_group.clone(),
                        self.i18n
                            .t("settings_view.connections.groups.new_placeholder"),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    )
                    .into_any_element(),
                )
                .child(self.connection_add_group_button(cx))
                .into_any_element(),
        ];

        for group in self.connection_store.groups() {
            rows.push(self.connection_group_row(group.clone(), cx));
        }
        if let Some(status) = self.settings_connection_status.clone() {
            rows.push(self.connection_status_row(status));
        }

        self.connection_section(
            "settings_view.connections.groups.title",
            "settings_view.connections.groups.description",
            rows,
        )
    }

    fn connection_add_group_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let disabled = self.connection_new_group_text().trim().is_empty();
        button_with(
            &self.tokens,
            self.i18n.t("settings_view.connections.groups.add"),
            ButtonOptions {
                variant: ButtonVariant::Secondary,
                size: ButtonSize::Default,
                radius: ButtonRadius::Md,
                disabled,
            },
        )
        .when(!disabled, |button| {
            button.on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.create_settings_connection_group(cx);
                    cx.stop_propagation();
                }),
            )
        })
        .into_any_element()
    }

    fn connection_new_group_text(&self) -> &str {
        if self.focused_settings_input == Some(SettingsInput::ConnectionNewGroup) {
            &self.settings_input_draft
        } else {
            &self.settings_connection_new_group
        }
    }

    fn connection_group_row(&self, group: String, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(self.settings_panel_background(theme.bg_panel))
            .p(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text))
                    .child(group.clone()),
            )
            .child(
                div()
                    .size(px(30.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.md))
                    .text_color(rgb(theme.error))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba((self.tokens.ui.error << 8) | 0x14)))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Trash2,
                        15.0,
                        rgb(theme.error),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.delete_settings_connection_group(group.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn ssh_config_import_section(
        &self,
        ssh_hosts: Vec<SshConfigHost>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let importable_count = ssh_hosts
            .iter()
            .filter(|host| !host.already_imported)
            .count();
        let selected_count = self.settings_selected_ssh_hosts.len();
        let all_selected = importable_count > 0 && selected_count == importable_count;

        let mut rows = Vec::new();
        if !ssh_hosts.is_empty() {
            rows.push(
                div()
                    .w_full()
                    .max_w(px(672.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .mb(px(-6.0))
                    .child(self.ssh_config_toggle_all_button(
                        all_selected,
                        importable_count,
                        cx,
                    ))
                    .when(selected_count > 0, |toolbar| {
                        toolbar.child(self.ssh_config_batch_import_button(selected_count, cx))
                    })
                    .into_any_element(),
            );
        }

        if ssh_hosts.is_empty() {
            rows.push(self.ssh_config_empty_state());
        } else {
            let mut list = div()
                .id("settings-ssh-config-scroll")
                .w_full()
                .max_w(px(672.0))
                .h(px(256.0))
                .overflow_y_scroll()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(self.tokens.ui.border))
                .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
                .p(px(8.0));
            for host in ssh_hosts {
                list = list.child(self.ssh_config_host_row(host, cx));
            }
            rows.push(list.into_any_element());
        }

        self.connection_section(
            "settings_view.connections.ssh_config.title",
            "settings_view.connections.ssh_config.description",
            rows,
        )
    }

    fn connection_section(
        &self,
        title_key: &str,
        description_key: &str,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        div()
            .pt(px(32.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(description_key)),
                    ),
            )
            .child(separator(&self.tokens, SeparatorOrientation::Horizontal))
            .children(rows)
            .into_any_element()
    }

    fn ssh_config_toggle_all_button(
        &self,
        all_selected: bool,
        importable_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = importable_count == 0;
        let label = if all_selected {
            self.i18n.t("settings_view.connections.ssh_config.deselect_all")
        } else {
            self.i18n.t("settings_view.connections.ssh_config.select_all")
        };
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.accent))
            .opacity(if disabled { 0.45 } else { 1.0 })
            .cursor_pointer()
            .hover(|style| style.text_color(rgb(self.tokens.ui.accent_hover)))
            .child(label)
            .when(!disabled, |button| {
                button.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.toggle_all_settings_ssh_config_hosts(all_selected, cx);
                        cx.stop_propagation();
                    }),
                )
            })
            .into_any_element()
    }

    fn ssh_config_batch_import_button(
        &self,
        selected_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self
            .i18n
            .t("settings_view.connections.ssh_config.import_selected")
            .replace("{{count}}", &selected_count.to_string());
        div()
            .h(px(28.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
            .px(px(10.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                LucideIcon::FolderInput,
                14.0,
                rgb(self.tokens.ui.text),
            ))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.import_selected_settings_ssh_hosts(cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn ssh_config_host_row(&self, host: SshConfigHost, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let alias = host.alias.clone();
        let checked = self.settings_selected_ssh_hosts.contains(&alias);
        let disabled = host.already_imported;
        let detail = format!(
            "{}@{}:{}",
            host.user.as_deref().unwrap_or_default(),
            host.hostname.as_deref().unwrap_or(alias.as_str()),
            host.port.unwrap_or(22)
        );

        div()
            .w_full()
            .mb(px(4.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(0x00000000))
            .bg(rgba(0x00000000))
            .p(px(12.0))
            .opacity(if disabled { 0.5 } else { 1.0 })
            .hover(|style| {
                if disabled {
                    style
                } else {
                    style
                        .bg(rgb(self.tokens.ui.bg_hover))
                        .border_color(rgb(self.tokens.ui.border))
                }
            })
            .child(
                div()
                    .cursor_pointer()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.ssh_config_checkbox(checked))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(theme.text))
                                            .child(host.alias.clone()),
                                    )
                                    .when(host.already_imported, |row| {
                                        row.child(self.ssh_config_imported_badge())
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(detail),
                            ),
                    )
                    .when(!disabled, |left| {
                        let alias = alias.clone();
                        left.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.toggle_settings_ssh_config_host(alias.clone(), cx);
                                cx.stop_propagation();
                            }),
                        )
                    }),
            )
            .child(self.ssh_config_import_button(host.alias, disabled, cx))
            .into_any_element()
    }

    fn ssh_config_import_button(
        &self,
        alias: String,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h(px(34.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .rounded_full()
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgba(0x00000000))
            .px(px(14.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .opacity(if disabled { 0.5 } else { 1.0 })
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                LucideIcon::FolderInput,
                16.0,
                rgb(self.tokens.ui.text),
            ))
            .child(self.i18n.t("settings_view.connections.ssh_config.import"))
            .when(!disabled, |button| {
                button.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.import_settings_ssh_host(alias.clone(), cx);
                        cx.stop_propagation();
                    }),
                )
            })
            .into_any_element()
    }

    fn ssh_config_checkbox(&self, checked: bool) -> AnyElement {
        div()
            .size(px(20.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(if checked {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .bg(if checked {
                rgb(self.tokens.ui.accent)
            } else {
                rgba(0x00000000)
            })
            .text_size(px(12.0))
            .text_color(rgb(self.tokens.ui.accent_text))
            .child(if checked { "✓" } else { "" })
            .into_any_element()
    }

    fn ssh_config_imported_badge(&self) -> AnyElement {
        div()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x20))
            .text_size(px(10.0))
            .text_color(rgb(self.tokens.ui.accent))
            .child(
                self.i18n
                    .t("settings_view.connections.ssh_config.already_imported"),
            )
            .into_any_element()
    }

    fn ssh_config_empty_state(&self) -> AnyElement {
        div()
            .w_full()
            .max_w(px(672.0))
            .h(px(256.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
            .p(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .text_align(gpui::TextAlign::Center)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.connections.ssh_config.no_hosts"))
            .into_any_element()
    }

    fn ssh_key_row(&self, key: oxideterm_connections::SshKeyInfo) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .bg(self.settings_panel_background(theme.bg_panel))
            .p(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(rgba((theme.accent << 8) | 0x1a))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Key,
                                18.0,
                                rgb(theme.accent),
                            )),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(key.name),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!("{} · {}", key.key_type, key.path)),
                            ),
                    ),
            )
            .when(key.has_passphrase, |row| {
                row.child(self.text_badge(
                    self.i18n.t("settings_view.ssh_keys.encrypted"),
                    theme.warning,
                ))
            })
            .into_any_element()
    }

    fn ssh_keys_empty_state(&self) -> AnyElement {
        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
            .py(px(48.0))
            .text_align(gpui::TextAlign::Center)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.ssh_keys.no_keys"))
            .into_any_element()
    }

    fn connection_status_row(&self, status: String) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.info << 8) | 0x33))
            .bg(rgba((self.tokens.ui.info << 8) | 0x1a))
            .px(px(12.0))
            .py(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.info))
            .child(status)
            .into_any_element()
    }

    fn create_settings_connection_group(&mut self, cx: &mut Context<Self>) -> bool {
        let group = if self.focused_settings_input == Some(SettingsInput::ConnectionNewGroup) {
            self.settings_input_draft.trim().to_string()
        } else {
            self.settings_connection_new_group.trim().to_string()
        };
        if group.is_empty() {
            return false;
        }
        let created = match self.connection_store.create_group(group.clone()) {
            Ok(()) => {
                self.settings_connection_new_group.clear();
                self.settings_input_draft.clear();
                self.settings_connection_status = None;
                true
            }
            Err(error) => {
                self.settings_connection_status = Some(
                    self.i18n
                        .t("settings_view.errors.create_group_failed")
                        .replace("{{error}}", &error.to_string()),
                );
                false
            }
        };
        cx.notify();
        created
    }

    fn delete_settings_connection_group(&mut self, group: String, cx: &mut Context<Self>) {
        match self.connection_store.delete_group(&group) {
            Ok(()) => self.settings_connection_status = None,
            Err(error) => {
                self.settings_connection_status = Some(
                    self.i18n
                        .t("settings_view.errors.delete_group_failed")
                        .replace("{{error}}", &error.to_string()),
                );
            }
        }
        cx.notify();
    }

    fn toggle_settings_ssh_config_host(&mut self, alias: String, cx: &mut Context<Self>) {
        if !self.settings_selected_ssh_hosts.insert(alias.clone()) {
            self.settings_selected_ssh_hosts.remove(&alias);
        }
        cx.notify();
    }

    fn toggle_all_settings_ssh_config_hosts(
        &mut self,
        all_selected: bool,
        cx: &mut Context<Self>,
    ) {
        if all_selected {
            self.settings_selected_ssh_hosts.clear();
        } else {
            let existing_names = self
                .connection_store
                .connections()
                .iter()
                .map(|conn| conn.name.clone())
                .collect::<HashSet<_>>();
            if let Ok(hosts) = list_ssh_config_hosts(&existing_names) {
                self.settings_selected_ssh_hosts = hosts
                    .into_iter()
                    .filter(|host| !host.already_imported)
                    .map(|host| host.alias)
                    .collect();
            }
        }
        cx.notify();
    }

    fn import_settings_ssh_host(&mut self, alias: String, cx: &mut Context<Self>) {
        match import_ssh_config_alias(&mut self.connection_store, &alias) {
            Ok(true) => {
                self.settings_selected_ssh_hosts.remove(&alias);
                self.settings_connection_status = Some(
                    self.i18n
                        .t("settings_view.errors.import_success")
                        .replace("{{name}}", &alias),
                );
            }
            Ok(false) => {
                self.settings_connection_status = Some(
                    self.i18n
                        .t("settings_view.connections.ssh_config.batch_import_skipped")
                        .replace("{{count}}", "1"),
                );
            }
            Err(error) => {
                self.settings_connection_status = Some(
                    self.i18n
                        .t("settings_view.errors.import_failed")
                        .replace("{{error}}", &error.to_string()),
                );
            }
        }
        cx.notify();
    }

    fn import_selected_settings_ssh_hosts(&mut self, cx: &mut Context<Self>) {
        let aliases = self
            .settings_selected_ssh_hosts
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut imported = 0usize;
        let mut skipped = 0usize;
        let mut errors = Vec::new();

        for alias in aliases {
            match import_ssh_config_alias(&mut self.connection_store, &alias) {
                Ok(true) => {
                    imported += 1;
                    self.settings_selected_ssh_hosts.remove(&alias);
                }
                Ok(false) => {
                    skipped += 1;
                    self.settings_selected_ssh_hosts.remove(&alias);
                }
                Err(error) => errors.push(format!("{alias}: {error}")),
            }
        }

        let mut parts = Vec::new();
        if imported > 0 {
            parts.push(
                self.i18n
                    .t("settings_view.connections.ssh_config.batch_import_success")
                    .replace("{{count}}", &imported.to_string()),
            );
        }
        if skipped > 0 {
            parts.push(
                self.i18n
                    .t("settings_view.connections.ssh_config.batch_import_skipped")
                    .replace("{{count}}", &skipped.to_string()),
            );
        }
        if !errors.is_empty() {
            parts.push(errors.join(", "));
        }
        self.settings_connection_status = (!parts.is_empty()).then(|| parts.join("; "));
        cx.notify();
    }

    fn settings_reconnect(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.bool_row(
                "settings_view.reconnect.enabled",
                "settings_view.reconnect.enabled_hint",
                settings.reconnect.enabled,
                set_reconnect_enabled,
                cx,
            ),
            separator(&self.tokens, SeparatorOrientation::Horizontal).into_any_element(),
            div()
                .flex()
                .flex_col()
                .gap(px(24.0))
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
                        .max_w(px(672.0))
                        .grid()
                        .grid_cols(2)
                        .gap(px(32.0))
                        .child(self.number_row(
                            "settings_view.reconnect.max_attempts",
                            "settings_view.reconnect.max_attempts_hint",
                            settings.reconnect.max_attempts,
                            1,
                            1,
                            20,
                            set_reconnect_max_attempts,
                            cx,
                        ))
                        .child(self.number_row(
                            "settings_view.reconnect.base_delay",
                            "settings_view.reconnect.base_delay_hint",
                            settings.reconnect.base_delay_ms,
                            500,
                            500,
                            10000,
                            set_reconnect_base_delay,
                            cx,
                        )),
                )
                .child(
                    div()
                        .w_full()
                        .max_w(px(672.0))
                        .grid()
                        .grid_cols(2)
                        .gap(px(32.0))
                        .child(self.number_row(
                            "settings_view.reconnect.max_delay",
                            "settings_view.reconnect.max_delay_hint",
                            settings.reconnect.max_delay_ms,
                            5000,
                            5000,
                            60000,
                            set_reconnect_max_delay,
                            cx,
                        )),
                )
                .child(
                    div()
                        .max_w(px(672.0))
                        .p(px(16.0))
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                        .bg(rgb(self.tokens.ui.bg_card))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.reconnect.formula_hint")),
                )
                .into_any_element(),
        ]
    }

    fn settings_sftp(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        let mut speed_rows = vec![self.sftp_settings_row(
            "settings_view.sftp.bandwidth",
            Some("settings_view.sftp.bandwidth_hint"),
            checkbox(&self.tokens, String::new(), settings.sftp.speed_limit_enabled)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(
                            |settings| {
                                settings.sftp.speed_limit_enabled =
                                    !settings.sftp.speed_limit_enabled
                            },
                            cx,
                        );
                    }),
                )
                .into_any_element(),
        )];

        if settings.sftp.speed_limit_enabled {
            speed_rows.push(
                div()
                    .pt(px(8.0))
                    .child(self.sftp_settings_row(
                        "settings_view.sftp.speed_limit",
                        None,
                        self.settings_text_input_control(
                            SettingsInput::SftpSpeedLimitKbps,
                            settings.sftp.speed_limit_kbps.to_string(),
                            "0 = unlimited".to_string(),
                            SFTP_SETTINGS_SELECT_WIDTH,
                            cx,
                        ),
                    ))
                    .into_any_element(),
            );
        }

        vec![
            self.sftp_settings_card(
                vec![
                    self.sftp_settings_row(
                        "settings_view.sftp.concurrent",
                        Some("settings_view.sftp.concurrent_hint"),
                        self.sftp_select_control(
                            SettingsSelect::SftpConcurrent,
                            sftp_transfer_count_label(
                                &self.i18n,
                                settings.sftp.max_concurrent_transfers,
                            ),
                            cx,
                        ),
                    ),
                    self.card_separator(),
                    self.sftp_settings_row(
                        "settings_view.sftp.directory_parallelism",
                        Some("settings_view.sftp.directory_parallelism_hint"),
                        self.sftp_select_control(
                            SettingsSelect::SftpDirectoryParallelism,
                            sftp_transfer_count_label(&self.i18n, settings.sftp.directory_parallelism),
                            cx,
                        ),
                    ),
                ],
                20.0,
            ),
            self.sftp_settings_card(speed_rows, 16.0),
            self.sftp_settings_card(
                vec![
                    div()
                        .mb(px(8.0))
                        .child(self.sftp_settings_row(
                            "settings_view.sftp.conflict",
                            Some("settings_view.sftp.conflict_hint"),
                            self.sftp_select_control(
                                SettingsSelect::SftpConflict,
                                conflict_label(settings.sftp.conflict_action, &self.i18n),
                                cx,
                            ),
                        ))
                        .into_any_element(),
                ],
                0.0,
            ),
        ]
    }

    fn sftp_settings_card(&self, rows: Vec<AnyElement>, gap: f32) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .p(px(SFTP_SETTINGS_CARD_PADDING))
            .flex()
            .flex_col()
            .gap(px(gap))
            .children(rows)
            .into_any_element()
    }

    fn sftp_settings_row(
        &self,
        label_key: &str,
        hint_key: Option<&str>,
        control: AnyElement,
    ) -> AnyElement {
        let mut label = div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            );
        if let Some(hint_key) = hint_key {
            label = label.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(hint_key)),
            );
        }

        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(label)
            .child(control)
            .into_any_element()
    }

    fn sftp_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, value, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        div()
            .relative()
            .w(px(SFTP_SETTINGS_SELECT_WIDTH))
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn settings_ide(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.ide_toggle_card(
                "settings_view.ide.auto_save",
                "settings_view.ide.auto_save_hint",
                settings.ide.auto_save,
                set_ide_auto_save,
                cx,
            ),
            self.ide_toggle_card(
                "settings_view.ide.word_wrap",
                "settings_view.ide.word_wrap_hint",
                settings.ide.word_wrap,
                set_ide_word_wrap,
                cx,
            ),
            self.ide_typography_card(settings, cx),
            self.ide_agent_card(settings, cx),
            self.ide_agent_privacy_card(),
        ]
    }

    fn ide_card(&self) -> Div {
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .p(px(IDE_SETTINGS_CARD_PADDING))
    }

    fn ide_toggle_card(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ide_card()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(IDE_SETTINGS_TOGGLE_CARD_GAP))
            .child(self.ide_label_block(label_key, hint_key))
            .child(
                checkbox(&self.tokens, String::new(), checked)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(|settings| setter(settings, !checked), cx);
                        }),
                    )
                    .into_any_element(),
            )
            .into_any_element()
    }

    fn ide_typography_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ide_card()
            .flex()
            .flex_col()
            .gap(px(IDE_SETTINGS_CARD_GAP))
            .child(self.ide_card_title(
                self.i18n.t("settings_view.ide.editor_typography"),
            ))
            .child(self.ide_card_description(
                self.i18n.t("settings_view.ide.editor_typography_hint"),
            ))
            .child(self.ide_setting_row(
                "settings_view.ide.font_size",
                "settings_view.ide.font_size_hint",
                self.ide_number_input_with_suffix(
                    SettingsInput::IdeFontSize,
                    settings
                        .ide
                        .font_size
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    settings.terminal.font_size.to_string(),
                    Some("px"),
                    cx,
                ),
            ))
            .child(self.card_separator())
            .child(self.ide_setting_row(
                "settings_view.ide.line_height",
                "settings_view.ide.line_height_hint",
                self.ide_number_input_with_suffix(
                    SettingsInput::IdeLineHeight,
                    settings
                        .ide
                        .line_height
                        .map(compact_decimal)
                        .unwrap_or_default(),
                    compact_decimal(settings.terminal.line_height),
                    None,
                    cx,
                ),
            ))
            .into_any_element()
    }

    fn ide_agent_card(&self, settings: &PersistedSettings, cx: &mut Context<Self>) -> AnyElement {
        self.ide_card()
            .flex()
            .flex_col()
            .gap(px(IDE_SETTINGS_CARD_GAP))
            .child(self.ide_card_title(self.i18n.t("settings_view.ide.agent_title")))
            .child(self.ide_card_description(
                self.i18n.t("settings_view.ide.agent_description"),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .children([
                        self.ide_feature_row("settings_view.ide.agent_feature_atomic"),
                        self.ide_feature_row("settings_view.ide.agent_feature_watch"),
                        self.ide_feature_row("settings_view.ide.agent_feature_hash"),
                        self.ide_feature_row("settings_view.ide.agent_feature_search"),
                    ]),
            )
            .child(
                div()
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(rgba(
                        (self.tokens.ui.border << 8) | IDE_SETTINGS_AGENT_SUBTLE_BORDER_ALPHA as u32,
                    ))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t("settings_view.ide.agent_supported")),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child("x86_64, aarch64 (Linux)"),
                            ),
                    )
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(self.tokens.radii.sm))
                            .border_1()
                            .border_color(rgba(
                                (self.tokens.ui.border << 8)
                                    | IDE_SETTINGS_AGENT_SUBTLE_BORDER_ALPHA as u32,
                            ))
                            .bg(rgb(self.tokens.ui.bg_panel))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child("~1 MB"),
                    ),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .italic()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ide.agent_auto_hint")),
            )
            .child(self.card_separator())
            .child(self.ide_setting_row(
                "settings_view.ide.agent_mode_label",
                "settings_view.ide.agent_mode_hint",
                self.ide_select_control(
                    SettingsSelect::IdeAgentMode,
                    ide_agent_label(settings.ide.agent_mode, &self.i18n),
                    IDE_SETTINGS_AGENT_SELECT_WIDTH,
                    cx,
                ),
            ))
            .into_any_element()
    }

    fn ide_agent_privacy_card(&self) -> AnyElement {
        self.ide_card()
            .border_color(rgba(
                (IDE_SETTINGS_BLUE_500 << 8) | IDE_SETTINGS_AGENT_PRIVACY_BORDER_ALPHA as u32,
            ))
            .bg(rgba(
                (IDE_SETTINGS_BLUE_500 << 8) | IDE_SETTINGS_AGENT_PRIVACY_BG_ALPHA as u32,
            ))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Shield,
                        16.0,
                        rgb(IDE_SETTINGS_BLUE_400),
                    ))
                    .child(self.i18n.t("settings_view.ide.agent_transparency_title")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .children([
                        self.ide_privacy_row(
                            "settings_view.ide.agent_path_label",
                            "settings_view.ide.agent_path_detail",
                        ),
                        self.ide_privacy_row(
                            "settings_view.ide.agent_lifecycle_label",
                            "settings_view.ide.agent_lifecycle_detail",
                        ),
                        self.ide_privacy_row(
                            "settings_view.ide.agent_privacy_label",
                            "settings_view.ide.agent_privacy_detail",
                        ),
                    ]),
            )
            .into_any_element()
    }

    fn ide_card_title(&self, title: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(title.to_uppercase())
            .into_any_element()
    }

    fn ide_card_description(&self, description: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(description)
            .into_any_element()
    }

    fn ide_label_block(&self, label_key: &str, hint_key: &str) -> AnyElement {
        div()
            .min_w(px(0.0))
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
            )
            .into_any_element()
    }

    fn ide_setting_row(&self, label_key: &str, hint_key: &str, control: AnyElement) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(self.ide_label_block(label_key, hint_key))
            .child(control)
            .into_any_element()
    }

    fn ide_number_input_with_suffix(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        suffix: Option<&'static str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut control = div().flex().items_center().gap(px(4.0)).child(
            self.settings_text_input_control(
                input,
                value,
                placeholder,
                IDE_SETTINGS_INPUT_WIDTH,
                cx,
            ),
        );
        if let Some(suffix) = suffix {
            control = control.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(suffix),
            );
        }
        control.into_any_element()
    }

    fn ide_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, value, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        div()
            .relative()
            .w(px(width))
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn ide_feature_row(&self, label_key: &str) -> AnyElement {
        self.ide_dot_row(
            IDE_SETTINGS_EMERALD_400,
            div().child(self.i18n.t(label_key)).into_any_element(),
        )
    }

    fn ide_privacy_row(&self, label_key: &str, detail_key: &str) -> AnyElement {
        self.ide_dot_row(
            IDE_SETTINGS_BLUE_400,
            div()
                .flex()
                .flex_wrap()
                .gap(px(4.0))
                .child(
                    div()
                        .text_color(rgb(self.tokens.ui.text))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(self.i18n.t(label_key)),
                )
                .child(div().child(self.i18n.t(detail_key)))
                .into_any_element(),
        )
    }

    fn ide_dot_row(&self, color: u32, content: AnyElement) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                div()
                    .mt(px(IDE_SETTINGS_AGENT_DOT_TOP_MARGIN))
                    .size(px(IDE_SETTINGS_AGENT_DOT_SIZE))
                    .rounded_full()
                    .bg(rgb(color))
                    .flex_none(),
            )
            .child(content)
            .into_any_element()
    }

    fn settings_ai(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![self.settings_card(
            "settings_view.ai.title",
            "settings_view.ai.description",
            vec![
                self.bool_row(
                    "settings_view.ai.enable",
                    "settings_view.ai.enable_hint",
                    settings.ai.enabled,
                    set_ai_enabled,
                    cx,
                ),
                self.bool_row(
                    "settings_view.ai.privacy_notice",
                    "settings_view.ai.privacy_text",
                    settings.ai.enabled_confirmed,
                    set_ai_enabled_confirmed,
                    cx,
                ),
                self.value_row(
                    "settings_view.ai.base_url",
                    "settings_view.ai.provider_settings_summary",
                    settings.ai.base_url.clone(),
                ),
                self.value_row(
                    "settings_view.ai.model",
                    "settings_view.ai.provider_settings_summary",
                    settings.ai.model.clone(),
                ),
                self.count_row(
                    "settings_view.ai.provider_settings",
                    "settings_view.ai.provider_settings_summary",
                    settings.ai.providers.len(),
                ),
                self.value_row(
                    "settings_view.ai.default_model",
                    "settings_view.ai.provider_settings_summary",
                    settings
                        .ai
                        .active_model
                        .clone()
                        .unwrap_or_else(|| settings.ai.model.clone()),
                ),
                self.number_row(
                    "settings_view.ai.max_context",
                    "settings_view.ai.max_context_hint",
                    settings.ai.context_max_chars,
                    2000,
                    2000,
                    32000,
                    set_ai_context_max_chars,
                    cx,
                ),
                self.number_row(
                    "settings_view.ai.buffer_history",
                    "settings_view.ai.buffer_history_hint",
                    settings.ai.context_visible_lines,
                    20,
                    20,
                    1000,
                        set_ai_context_lines,
                        cx,
                    ),
                self.bool_row(
                    "settings_view.ai.context_source_ide",
                    "settings_view.ai.context_source_ide_hint",
                    settings.ai.context_sources.ide,
                    set_ai_context_source_ide,
                    cx,
                ),
                self.bool_row(
                    "settings_view.ai.context_source_sftp",
                    "settings_view.ai.context_source_sftp_hint",
                    settings.ai.context_sources.sftp,
                    set_ai_context_source_sftp,
                    cx,
                ),
                self.cycle_row(
                    "settings_view.ai.reasoning_title",
                    "settings_view.ai.reasoning_hint",
                    ai_thinking_label(settings.ai.thinking_style),
                    cycle_ai_thinking,
                    cx,
                ),
                self.cycle_row(
                    "settings_view.ai.reasoning_title",
                    "settings_view.ai.reasoning_hint",
                    ai_reasoning_label(settings.ai.reasoning_effort),
                        cycle_ai_reasoning,
                        cx,
                    ),
                self.bool_row(
                    "settings_view.ai.memory_enabled",
                    "settings_view.ai.memory_enabled_hint",
                    settings.ai.memory.enabled,
                    set_ai_memory_enabled,
                    cx,
                ),
                self.value_row(
                    "settings_view.ai.custom_system_prompt",
                    "settings_view.ai.system_prompt_hint",
                    if settings.ai.custom_system_prompt.trim().is_empty() {
                        self.i18n.t("settings_view.ai.system_prompt_placeholder")
                    } else {
                        settings.ai.custom_system_prompt.clone()
                    },
                ),
                self.value_row(
                    "settings_view.ai.memory_title",
                    "settings_view.ai.memory_hint",
                    if settings.ai.memory.content.trim().is_empty() {
                        self.i18n.t("settings_view.ai.memory_placeholder")
                    } else {
                        settings.ai.memory.content.clone()
                    },
                ),
                self.bool_row(
                    "settings_view.ai.tool_use_enabled",
                    "settings_view.ai.tool_use_enabled_hint",
                    settings.ai.tool_use.enabled,
                    set_ai_tool_use_enabled,
                    cx,
                ),
                self.number_row(
                    "settings_view.ai.tool_use_max_rounds",
                    "settings_view.ai.tool_use_max_rounds_hint",
                    settings.ai.tool_use.max_rounds.unwrap_or(10),
                    1,
                    1,
                    30,
                    set_ai_tool_use_max_rounds,
                    cx,
                ),
                self.count_row(
                    "settings_view.ai.tool_use_policy_summary",
                    "settings_view.ai.tool_use_approve_hint",
                    settings.ai.tool_use.auto_approve_tools.len(),
                ),
                self.count_row(
                    "settings_view.mcp.title",
                    "settings_view.mcp.description",
                    settings.ai.mcp_servers.len(),
                ),
                self.value_row(
                    "settings_view.ai.embedding_title",
                    "settings_view.ai.embedding_description",
                    if settings.ai.embedding_config.is_some() {
                        self.i18n.t("settings_view.knowledge.semantic_search_using")
                    } else {
                        self.i18n
                            .t("settings_view.knowledge.semantic_search_not_configured")
                    },
                ),
                self.count_row(
                    "settings_view.ai.execution_profiles",
                    "settings_view.ai.execution_profiles_hint",
                    settings
                        .ai
                        .execution_profiles
                        .get("profiles")
                        .and_then(|profiles| profiles.as_array())
                        .map(Vec::len)
                        .unwrap_or(0),
                ),
            ],
        )]
    }

    fn settings_knowledge(&self) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![self.settings_card(
            "settings_view.knowledge.title",
            "settings_view.knowledge.description",
            vec![
                self.value_row(
                    "settings_view.knowledge.semantic_search",
                    "settings_view.knowledge.semantic_search_description",
                    if settings.ai.embedding_config.is_some() {
                        self.i18n.t("settings_view.knowledge.semantic_search_using")
                    } else {
                        self.i18n
                            .t("settings_view.knowledge.semantic_search_not_configured")
                    },
                ),
                self.value_row(
                    "settings_view.knowledge.keyword_search_ready",
                    "settings_view.knowledge.description",
                    self.i18n.t("common.enabled"),
                ),
                self.value_row(
                    "settings_view.knowledge.collections",
                    "settings_view.knowledge.create_description",
                    self.i18n.t("settings_view.knowledge.no_collections"),
                ),
                self.value_row(
                    "settings_view.knowledge.import_files",
                    "settings_view.knowledge.file_filter_documents",
                    self.i18n.t("common.disabled"),
                ),
                self.value_row(
                    "settings_view.knowledge.generate_embeddings",
                    "settings_view.knowledge.semantic_search_description",
                    self.i18n.t("common.disabled"),
                ),
                self.value_row(
                    "settings_view.knowledge.configure_embeddings",
                    "settings_view.ai.embedding_description",
                    if settings.ai.embedding_config.is_some() {
                        self.i18n.t("settings_view.knowledge.semantic_search_using")
                    } else {
                        self.i18n
                            .t("settings_view.knowledge.semantic_search_not_configured")
                    },
                ),
            ],
        )]
    }

    fn settings_keybindings(&self) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![self.settings_card(
            "settings_view.keybindings.title",
            "settings_view.keybindings.description",
            vec![
                self.value_row(
                    "settings_view.keybindings.modified",
                    "settings_view.keybindings.intl_keyboard_note",
                    settings.keybindings.overrides.len().to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.import",
                    "settings_view.keybindings.import_invalid",
                    self.i18n.t("settings_view.keybindings.default_value"),
                ),
                self.value_row(
                    "settings_view.keybindings.export",
                    "settings_view.keybindings.export_error",
                    self.i18n.t("common.disabled"),
                ),
                self.value_row(
                    "settings_view.keybindings.reset_all",
                    "settings_view.keybindings.reset_all_confirm",
                    self.i18n.t("settings_view.keybindings.default_value"),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.app.newTerminal",
                    "settings_view.keybindings.scope_global",
                    "Cmd+T".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.app.closeTab",
                    "settings_view.keybindings.scope_global",
                    "Cmd+W".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.app.settings",
                    "settings_view.keybindings.scope_global",
                    "Cmd+,".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.split.horizontal",
                    "settings_view.keybindings.scope_split",
                    "Cmd+Shift+E".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.split.vertical",
                    "settings_view.keybindings.scope_split",
                    "Cmd+Shift+D".to_string(),
                ),
            ],
        )]
    }

    fn settings_help(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.settings_card(
                "settings_view.help.version_info",
                "settings_view.help.description",
                vec![
                    self.value_row(
                        "settings_view.help.app_name",
                        "settings_view.help.version_info",
                        "OxideTerm Native".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.version",
                        "settings_view.help.version_info",
                        env!("CARGO_PKG_VERSION").to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.license",
                        "settings_view.help.resources",
                        "GPL-3.0-only".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.portable_mode",
                        "settings_view.help.portable_mode_hint",
                        self.i18n.t("settings_view.help.updates_manual_only"),
                    ),
                    self.cycle_row(
                        "settings_view.help.update_channel",
                        "settings_view.help.update_channel_hint",
                        update_channel_label(settings.general.update_channel, &self.i18n),
                        cycle_update_channel,
                        cx,
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.help.shortcuts",
                "settings_view.help.resources",
                vec![
                    self.value_row(
                        "settings_view.help.shortcut_new_tab",
                        "settings_view.help.category_app",
                        "Cmd+T".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_close_tab",
                        "settings_view.help.category_app",
                        "Cmd+W".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_find",
                        "settings_view.help.category_terminal",
                        "Cmd+F".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_split_h",
                        "settings_view.help.category_split",
                        "Cmd+Shift+E".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_split_v",
                        "settings_view.help.category_split",
                        "Cmd+Shift+D".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_settings",
                        "settings_view.help.category_app",
                        "Cmd+,".to_string(),
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.help.diagnostics",
                "settings_view.help.open_logs_hint",
                vec![
                    self.value_row(
                        "settings_view.help.open_logs",
                        "settings_view.help.open_logs_hint",
                        self.i18n.t("common.disabled"),
                    ),
                    self.value_row(
                        "settings_view.help.memory_diagnostics_title",
                        "settings_view.help.memory_diagnostics_hint",
                        self.i18n.t("common.disabled"),
                    ),
                    self.value_row(
                        "settings_view.help.check_update",
                        "settings_view.help.updates_manual_only_hint",
                        self.i18n.t("settings_view.help.updates_manual_only"),
                    ),
                ],
            ),
        ]
    }

    fn cycle_row(
        &self,
        label_key: &str,
        hint_key: &str,
        value: String,
        cycle: fn(&mut PersistedSettings),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = button(&self.tokens, value, oxideterm_gpui_ui::ButtonTone::Secondary)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(cycle, cx);
                }),
            )
            .into_any_element();
        self.setting_row(label_key, hint_key, control)
    }

    fn language_label(&self, language: Language) -> String {
        match language {
            Language::De => "Deutsch",
            Language::En => "English",
            Language::EsEs => "Español (España)",
            Language::FrFr => "Français (France)",
            Language::It => "Italiano",
            Language::Ko => "한국어",
            Language::PtBr => "Português (Brasil)",
            Language::Vi => "Tiếng Việt",
            Language::Ja => "日本語",
            Language::ZhCn => "简体中文",
            Language::ZhTw => "繁體中文",
        }
        .to_string()
    }
}

fn connection_idle_timeout_options(i18n: &I18n) -> Vec<(i64, String)> {
    vec![
        (300, i18n.t("settings_view.connections.idle_timeout.5min")),
        (900, i18n.t("settings_view.connections.idle_timeout.15min")),
        (
            1800,
            i18n.t("settings_view.connections.idle_timeout.30min"),
        ),
        (3600, i18n.t("settings_view.connections.idle_timeout.1hr")),
        (0, i18n.t("settings_view.connections.idle_timeout.never")),
    ]
}

fn connection_idle_timeout_label(seconds: i64, i18n: &I18n) -> String {
    connection_idle_timeout_options(i18n)
        .into_iter()
        .find_map(|(value, label)| (value == seconds).then_some(label))
        .unwrap_or_else(|| seconds.to_string())
}

fn import_ssh_config_alias(
    store: &mut oxideterm_connections::ConnectionStore,
    alias: &str,
) -> anyhow::Result<bool> {
    if store.connections().iter().any(|conn| conn.name == alias) {
        return Ok(false);
    }
    let Some(host) = resolve_ssh_config_alias(alias)? else {
        return Ok(false);
    };
    let connection = saved_connection_from_ssh_host(host)?;
    store.import_ssh_connection(connection)?;
    Ok(true)
}
