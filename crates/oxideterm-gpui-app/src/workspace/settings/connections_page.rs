impl WorkspaceApp {
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
        let value =
            connection_idle_timeout_label(settings.connection_pool.idle_timeout_secs, &self.i18n);
        let control = self.settings_select_control(select_id, value, false, None, cx);

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
            .child(control)
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
        // Tauri ConnectionsTab renders Add Group as the default shadcn Button
        // with a leading Plus icon. Keep the disabled action guard local, but
        // route the chrome through the shared toolbar primitive.
        toolbar_button(
            &self.tokens,
            self.i18n.t("settings_view.connections.groups.add"),
            Some(Self::render_lucide_icon(
                LucideIcon::Plus,
                16.0,
                rgb(if disabled {
                    self.tokens.ui.text_muted
                } else {
                    self.tokens.ui.bg
                }),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Default,
                    size: ButtonSize::Default,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                ..ToolbarButtonOptions::default()
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
                .selectable_overflow_y_scroll(
                    &self.selectable_text_scroll_handle("settings-ssh-config-scroll"),
                )
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
        // This is the same compact outline action chrome as other migrated
        // settings toolbars; keep it on the shared button primitive so hover,
        // disabled, and future focus-visible behavior stay centralized.
        toolbar_button(
            &self.tokens,
            label,
            Some(Self::render_lucide_icon(
                LucideIcon::FolderInput,
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                background: Some(self.settings_panel_background(self.tokens.ui.bg_panel)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                height: Some(28.0),
                padding_x: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                ..ToolbarButtonOptions::default()
            },
        )
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
        // Host-row import uses the shared outline action path but preserves
        // Tauri's pill shape with a post-primitive radius override.
        toolbar_button(
            &self.tokens,
            self.i18n.t("settings_view.connections.ssh_config.import"),
            Some(Self::render_lucide_icon(
                LucideIcon::FolderInput,
                16.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                background: Some(rgba(0x00000000)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                height: Some(34.0),
                padding_x: Some(14.0),
                font_size: Some(self.tokens.metrics.ui_text_sm),
                ..ToolbarButtonOptions::default()
            },
        )
            .rounded_full()
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
        if created {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        cx.notify();
        created
    }

    fn delete_settings_connection_group(&mut self, group: String, cx: &mut Context<Self>) {
        match self.connection_store.delete_group(&group) {
            Ok(()) => {
                self.settings_connection_status = None;
                self.queue_cloud_sync_dirty_refresh(cx);
            }
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
                self.queue_cloud_sync_dirty_refresh(cx);
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
        if imported > 0 {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        cx.notify();
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
