use super::*;

pub(in crate::workspace) const CONNECTION_IDLE_TIMEOUT_CONTROL_WIDTH: f32 = 320.0; // Keep the standalone idle-timeout select from expanding into side panels.
pub(in crate::workspace) const CONNECTION_RESPONSIVE_FIELD_BASIS: f32 = 240.0; // Preferred field width before a settings row redistributes space.
pub(in crate::workspace) const CONNECTION_IMPORT_SOURCE_BASIS: f32 = 220.0; // Match the desktop source column while allowing narrow panes to wrap.
pub(in crate::workspace) const CONNECTION_IMPORT_PATH_BASIS: f32 = 420.0; // Give localized path actions room before they move below the source picker.
pub(in crate::workspace) const CONNECTION_IMPORT_PREVIEW_ACTIONS_BASIS: f32 = 420.0; // Keep preview controls together until the toolbar wraps.
pub(in crate::workspace) const CONNECTION_IMPORT_AUTH_WIDTH: f32 = 120.0; // Match the compact trailing authentication column from Tauri.
pub(in crate::workspace) const SSH_KEY_HEADER_TEXT_BASIS: f32 = 320.0; // Let long localized descriptions wrap before key actions.
pub(in crate::workspace) const SSH_CONFIG_IMPORT_DIALOG_WIDTH: f32 = 720.0;
pub(in crate::workspace) const SSH_CONFIG_IMPORT_DIALOG_HEIGHT: f32 = 560.0;

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_connections_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        match section_index {
            0 => self.settings_ssh_section(0, cx),
            1 => self.settings_card(
                "settings_view.connections.title",
                "settings_view.connections.description",
                vec![self.connection_defaults_section(settings, cx)],
            ),
            2 => self.connection_section(
                "settings_view.connections.idle_timeout.title",
                "settings_view.connections.idle_timeout.description",
                vec![self.connection_idle_timeout_control(settings, cx)],
            ),
            3 => self.settings_reconnect_section(0, cx),
            4 => self.ssh_config_import_section(cx),
            5 => self.connection_importers_section(cx),
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn settings_ssh_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if section_index != 0 {
            return div().into_any_element();
        }
        let keys = list_available_ssh_keys();
        let managed_keys = self.connection_store.managed_ssh_keys();
        let mut local_list = div().w_full().min_w_0().flex().flex_col();
        if keys.is_empty() {
            local_list = local_list.child(self.ssh_keys_empty_state());
        } else {
            let key_count = keys.len();
            for (index, key) in keys.into_iter().enumerate() {
                local_list = local_list.child(self.ssh_key_row(key));
                if index + 1 < key_count {
                    local_list = local_list.child(self.card_separator());
                }
            }
        }
        let managed_key_count = managed_keys.len();
        let mut managed_list = div().w_full().min_w_0().flex().flex_col();
        for (index, key) in managed_keys.into_iter().enumerate() {
            managed_list = managed_list.child(self.managed_ssh_key_row(key, cx));
            if index + 1 < managed_key_count {
                managed_list = managed_list.child(self.card_separator());
            }
        }
        let content = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(32.0))
            .child(self.ssh_key_section_header(
                "settings_view.ssh_keys.local_section",
                "settings_view.ssh_keys.local_description",
                None,
            ))
            .child(local_list)
            .child(self.ssh_key_section_header(
                "settings_view.ssh_keys.managed_section",
                "settings_view.ssh_keys.managed_description",
                Some(self.managed_ssh_key_toolbar(cx)),
            ))
            .when_some(
                self.settings_managed_key_status.clone(),
                |section, status| section.child(self.connection_status_row(status)),
            )
            .child(if self.connection_store.managed_ssh_keys().is_empty() {
                self.managed_ssh_keys_empty_state()
            } else {
                managed_list.into_any_element()
            });

        self.settings_card(
            "settings_view.ssh_keys.title",
            "settings_view.ssh_keys.description",
            vec![content.into_any_element()],
        )
    }

    pub(in crate::workspace) fn connection_defaults_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
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

    pub(in crate::workspace) fn connection_labeled_input(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .min_w_0()
            .max_w_full()
            .flex_1()
            .flex_basis(px(CONNECTION_RESPONSIVE_FIELD_BASIS))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.settings_text_input_control_fill(input, value, placeholder, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_idle_timeout_control(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select_id = SettingsSelect::ConnectionIdleTimeout;
        let value =
            connection_idle_timeout_label(settings.connection_pool.idle_timeout_secs, &self.i18n);
        let control = self.settings_select_control(
            select_id,
            value,
            false,
            Some(CONNECTION_IDLE_TIMEOUT_CONTROL_WIDTH),
            cx,
        );

        self.setting_row(
            "settings_view.connections.idle_timeout.label",
            "settings_view.connections.idle_timeout.hint",
            control,
            cx,
        )
    }

    pub(in crate::workspace) fn ssh_config_import_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.connection_section(
            "settings_view.connections.ssh_config.title",
            "settings_view.connections.ssh_config.description",
            vec![
                div()
                    .flex()
                    .justify_start()
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.connections.ssh_config.open"),
                        Some(Self::render_lucide_icon(
                            LucideIcon::FolderInput,
                            16.0,
                            rgb(self.tokens.ui.text),
                        )),
                        self.connection_import_secondary_button_options(false),
                        cx.listener(|this, _event, _window, cx| {
                            this.open_settings_ssh_config_import_dialog(cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .into_any_element(),
            ],
        )
    }

    pub(in crate::workspace) fn render_settings_ssh_config_import_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.settings_page.ssh_config_import_dialog_open {
            return None;
        }

        let ssh_hosts = self.settings_ssh_config_hosts();
        let importable_count = ssh_hosts
            .iter()
            .filter(|host| !host.already_imported)
            .count();
        let selected_count = self.settings_page.settings_selected_ssh_hosts.len();
        let all_selected = importable_count > 0 && selected_count == importable_count;
        let backdrop = dismissible_dialog_backdrop().on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.close_settings_ssh_config_import_dialog(cx);
                cx.stop_propagation();
            }),
        );

        let mut list = div()
            .id("settings-ssh-config-dialog-scroll")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h(px(0.0))
            .selectable_overflow_y_scroll(
                &self.selectable_text_scroll_handle("settings-ssh-config-dialog-scroll"),
            )
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
            .p(px(8.0));
        if ssh_hosts.is_empty() {
            list = list.child(self.ssh_config_empty_state());
        } else {
            for host in ssh_hosts {
                list = list.child(self.ssh_config_host_row(host, cx));
            }
        }

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .px(px(24.0))
            .py(px(18.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .when(importable_count > 0, |body| {
                body.child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .child(self.ssh_config_toggle_all_button(
                            all_selected,
                            importable_count,
                            cx,
                        ))
                        .when(selected_count > 0, |toolbar| {
                            toolbar.child(self.ssh_config_batch_import_button(selected_count, cx))
                        }),
                )
            })
            .when_some(
                self.settings_page.settings_connection_status.clone(),
                |body, status| body.child(self.connection_status_row(status)),
            )
            .child(list);

        let form = dialog_content(&self.tokens)
            .w(px(SSH_CONFIG_IMPORT_DIALOG_WIDTH))
            .max_w(relative(0.92))
            .h(px(SSH_CONFIG_IMPORT_DIALOG_HEIGHT))
            .max_h(relative(0.88))
            .flex()
            .flex_col()
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(
                        &self.tokens,
                        self.i18n.t("settings_view.connections.ssh_config.title"),
                    ))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n
                            .t("settings_view.connections.ssh_config.description"),
                    )),
            )
            .child(body)
            .child(
                dialog_footer(&self.tokens).child(self.standard_footer_action_button(
                    self.i18n.t("settings_view.connections.ssh_config.close"),
                    ButtonVariant::Secondary,
                    ConfirmDialogAction::Cancel,
                    false,
                    |this, _event, _window, cx| {
                        this.close_settings_ssh_config_import_dialog(cx);
                    },
                    cx,
                )),
            );

        Some(settings_dialog_transition(
            &self.tokens,
            "ssh-config-import-dialog-form",
            backdrop,
            form,
            self.ssh_config_import_dialog_presence,
        ))
    }

    pub(in crate::workspace) fn settings_ssh_config_hosts(&self) -> Vec<SshConfigHost> {
        let existing_names = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.name.clone())
            .collect::<HashSet<_>>();
        list_ssh_config_hosts(&existing_names).unwrap_or_default()
    }

    pub(in crate::workspace) fn open_settings_ssh_config_import_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.ssh_config_import_dialog_presence.reopen();
        self.settings_page.open_ssh_config_import_dialog();
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    pub(in crate::workspace) fn close_settings_ssh_config_import_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(generation) = self.ssh_config_import_dialog_presence.begin_exit() else {
            return;
        };
        self.clear_standard_confirm_focus();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if delay.is_zero() {
            self.settings_page.close_ssh_config_import_dialog();
            self.ssh_config_import_dialog_presence.reopen();
            cx.notify();
            return;
        }
        cx.spawn(async move |weak, cx| {
            gpui::Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this
                    .ssh_config_import_dialog_presence
                    .finish_exit(generation)
                {
                    this.settings_page.close_ssh_config_import_dialog();
                    this.ssh_config_import_dialog_presence.reopen();
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn connection_section(
        &self,
        title_key: &str,
        description_key: &str,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        let mut card_rows = vec![
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t(description_key))
                .into_any_element(),
        ];
        card_rows.extend(rows);

        self.settings_card(title_key, description_key, card_rows)
    }

    pub(in crate::workspace) fn ssh_config_toggle_all_button(
        &self,
        all_selected: bool,
        importable_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = importable_count == 0;
        let label = if all_selected {
            self.i18n
                .t("settings_view.connections.ssh_config.deselect_all")
        } else {
            self.i18n
                .t("settings_view.connections.ssh_config.select_all")
        };
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.accent_secondary))
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

    pub(in crate::workspace) fn ssh_config_batch_import_button(
        &self,
        selected_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self
            .i18n
            .t("settings_view.connections.ssh_config.import_selected")
            .replace("{{count}}", &selected_count.to_string());
        // This is the same compact outline action chrome as other migrated
        // settings toolbars; keep it on the workspace action wrapper so click
        // dispatch shares the disabled/loading guard with other Buttons.
        self.workspace_toolbar_action_button(
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
            cx.listener(|this, _event, _window, cx| {
                this.import_selected_settings_ssh_hosts(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ssh_config_host_row(
        &self,
        host: SshConfigHost,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let alias = host.alias.clone();
        let checked = self
            .settings_page
            .settings_selected_ssh_hosts
            .contains(&alias);
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

    pub(in crate::workspace) fn ssh_config_import_button(
        &self,
        alias: String,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Host-row import uses the shared outline action path but preserves
        // Tauri's pill shape with a post-primitive radius override.
        self.workspace_toolbar_action_button(
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
            cx.listener(move |this, _event, _window, cx| {
                this.import_settings_ssh_host(alias.clone(), cx);
                cx.stop_propagation();
            }),
        )
        .rounded_full()
        .into_any_element()
    }

    pub(in crate::workspace) fn ssh_config_checkbox(&self, checked: bool) -> AnyElement {
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

    pub(in crate::workspace) fn ssh_config_imported_badge(&self) -> AnyElement {
        div()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x20))
            .text_size(px(10.0))
            .text_color(rgb(self.tokens.ui.accent_secondary))
            .child(
                self.i18n
                    .t("settings_view.connections.ssh_config.already_imported"),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ssh_config_empty_state(&self) -> AnyElement {
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

    pub(in crate::workspace) fn connection_importers_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = vec![self.connection_import_input_row(cx)];

        if let Some(preview) = self.settings_connection_import_preview.clone() {
            rows.push(self.connection_import_preview_toolbar(&preview, cx));
            rows.push(self.connection_import_preview_list(preview, cx));
        }
        if let Some(status) = self.settings_page.settings_connection_status.clone() {
            rows.push(self.connection_status_row(status));
        }

        self.connection_section(
            "settings_view.connections.importers.title",
            "settings_view.connections.importers.description",
            rows,
        )
    }

    pub(in crate::workspace) fn connection_import_input_row(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_start()
            .gap(px(16.0))
            .child(
                div()
                    .min_w_0()
                    .max_w_full()
                    .flex_1()
                    .flex_basis(px(CONNECTION_IMPORT_SOURCE_BASIS))
                    .child(self.connection_import_source_picker(cx)),
            )
            .child(
                div()
                    .min_w_0()
                    .max_w_full()
                    .flex_1()
                    .flex_basis(px(CONNECTION_IMPORT_PATH_BASIS))
                    .grid()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.connections.importers.paths")),
                    )
                    .child(self.connection_import_path_toolbar(cx))
                    .child(self.connection_import_path_summary()),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_source_picker(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label =
            connection_import_source_label(self.settings_connection_import_source, &self.i18n);
        div()
            .w_full()
            .min_w_0()
            .grid()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.connections.importers.source")),
            )
            .child(self.settings_select_control(
                SettingsSelect::ConnectionImportSource,
                selected_label,
                false,
                None,
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_path_toolbar(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_paths = !self.settings_connection_import_paths.is_empty();
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(8.0))
            .child(self.connection_import_pick_files_button(cx))
            .when(
                self.settings_connection_import_source != ConnectionImportSource::Termius,
                |row| row.child(self.connection_import_pick_directory_button(cx)),
            )
            .child(self.connection_import_preview_button(has_paths, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_pick_files_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n
                .t("settings_view.connections.importers.choose_files"),
            Some(Self::render_lucide_icon(
                LucideIcon::FolderInput,
                16.0,
                rgb(self.tokens.ui.text),
            )),
            self.connection_import_secondary_button_options(false),
            cx.listener(|this, _event, _window, cx| {
                this.pick_connection_import_paths(false, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_pick_directory_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n
                .t("settings_view.connections.importers.choose_directory"),
            Some(Self::render_lucide_icon(
                LucideIcon::FolderOpen,
                16.0,
                rgb(self.tokens.ui.text),
            )),
            self.connection_import_secondary_button_options(false),
            cx.listener(|this, _event, _window, cx| {
                this.pick_connection_import_paths(true, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_preview_button(
        &self,
        has_paths: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t("settings_view.connections.importers.preview"),
            Some(Self::render_lucide_icon(
                LucideIcon::RefreshCw,
                16.0,
                rgb(if has_paths {
                    self.tokens.ui.bg
                } else {
                    self.tokens.ui.text_muted
                }),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Default,
                    size: ButtonSize::Default,
                    radius: ButtonRadius::Md,
                    disabled: !has_paths,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.preview_settings_connection_import(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_secondary_button_options(
        &self,
        disabled: bool,
    ) -> ToolbarButtonOptions {
        ToolbarButtonOptions {
            button: ButtonOptions {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Default,
                radius: ButtonRadius::Md,
                disabled,
            },
            background: Some(self.settings_panel_background(self.tokens.ui.bg_panel)),
            border: Some(rgb(self.tokens.ui.border)),
            text_color: Some(rgb(self.tokens.ui.text)),
            hover_background: Some(rgb(self.tokens.ui.bg_hover)),
            height: Some(36.0),
            padding_x: Some(12.0),
            font_size: Some(self.tokens.metrics.ui_text_sm),
            ..ToolbarButtonOptions::default()
        }
    }

    pub(in crate::workspace) fn connection_import_path_summary(&self) -> AnyElement {
        let summary = if self.settings_connection_import_paths.is_empty() {
            self.i18n.t("settings_view.connections.importers.no_paths")
        } else {
            self.settings_connection_import_paths.join(" · ")
        };
        div()
            .w_full()
            .min_w_0()
            .truncate()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(summary)
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_preview_toolbar(
        &self,
        preview: &ConnectionImportPreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let importable = preview
            .drafts
            .iter()
            .filter(|draft| draft.importable)
            .count();
        let all_selected = importable > 0
            && preview
                .drafts
                .iter()
                .filter(|draft| draft.importable)
                .all(|draft| {
                    self.settings_selected_connection_import_drafts
                        .contains(&draft.id)
                });
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .child(self.connection_import_toggle_all_button(all_selected, importable, cx))
            .child(
                div()
                    .min_w_0()
                    .max_w_full()
                    .flex_1()
                    .flex_basis(px(CONNECTION_IMPORT_PREVIEW_ACTIONS_BASIS))
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.connection_import_duplicate_strategy_picker(cx))
                    .child(
                        self.settings_text_input_control(
                            SettingsInput::ConnectionImportTargetGroup,
                            self.settings_connection_import_target_group.clone(),
                            self.i18n
                                .t("settings_view.connections.importers.target_group"),
                            192.0,
                            cx,
                        )
                        .into_any_element(),
                    )
                    .child(self.connection_import_apply_button(cx)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_toggle_all_button(
        &self,
        all_selected: bool,
        importable_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = importable_count == 0;
        let label = if all_selected {
            self.i18n
                .t("settings_view.connections.importers.deselect_all")
        } else {
            self.i18n
                .t("settings_view.connections.importers.select_all")
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
                        this.toggle_all_settings_connection_import_drafts(all_selected, cx);
                        cx.stop_propagation();
                    }),
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_duplicate_strategy_picker(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = connection_import_duplicate_strategy_label(
            self.settings_connection_import_duplicate_strategy,
            &self.i18n,
        );
        // Tauri renders duplicate strategy as a compact SelectTrigger (w-36 h-8)
        // in the import preview toolbar, not as adjacent action buttons.
        self.settings_select_control_with_trigger_style(
            SettingsSelect::ConnectionImportDuplicateStrategy,
            selected_label,
            false,
            Some(144.0),
            |trigger| {
                trigger
                    .h(px(32.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
            },
            cx,
        )
    }

    pub(in crate::workspace) fn connection_import_apply_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_count = self.settings_selected_connection_import_drafts.len();
        let label = self
            .i18n
            .t("settings_view.connections.importers.import_selected")
            .replace("{{count}}", &selected_count.to_string());
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                LucideIcon::Upload,
                16.0,
                rgb(if selected_count == 0 {
                    self.tokens.ui.text_muted
                } else {
                    self.tokens.ui.bg
                }),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Default,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: selected_count == 0,
                },
                height: Some(32.0),
                padding_x: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.apply_settings_connection_import(cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_preview_list(
        &self,
        preview: ConnectionImportPreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if preview.drafts.is_empty() {
            return div()
                .w_full()
                .h(px(288.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(self.tokens.ui.border))
                .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.connections.importers.no_drafts"))
                .into_any_element();
        }

        let mut list = div()
            .id("settings-connection-import-scroll")
            .w_full()
            .h(px(288.0))
            .selectable_overflow_y_scroll(
                &self.selectable_text_scroll_handle("settings-connection-import-scroll"),
            )
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_panel));
        for draft in preview.drafts {
            list = list.child(self.connection_import_preview_row(draft, cx));
        }
        list.into_any_element()
    }

    pub(in crate::workspace) fn connection_import_preview_row(
        &self,
        draft: oxideterm_connections::ImportedConnectionDraft,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let checked = self
            .settings_selected_connection_import_drafts
            .contains(&draft.id);
        let disabled = !draft.importable;
        let detail = format!("{}@{}:{}", draft.username, draft.host, draft.port);
        let origin_detail = [
            draft.group.clone(),
            Some(connection_import_source_label(draft.source, &self.i18n)),
            Some(draft.source_path.clone()),
        ]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
        let warnings = draft
            .warnings
            .iter()
            .chain(draft.unsupported_fields.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        let draft_id = draft.id.clone();
        div()
            .w_full()
            .min_w_0()
            .flex()
            .items_start()
            .gap(px(8.0))
            .border_b_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x99))
            .p(px(12.0))
            .opacity(if disabled { 0.5 } else { 1.0 })
            .child(
                div()
                    .w(px(28.0))
                    .flex_none()
                    .child(self.ssh_config_checkbox(checked)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(draft.name),
                            )
                            .when(draft.duplicate, |row| {
                                row.child(self.connection_import_duplicate_badge())
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(detail),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(origin_detail),
                    )
                    .when(!warnings.is_empty(), |column| {
                        column.child(
                            div()
                                .truncate()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.warning))
                                .child(warnings),
                        )
                    }),
            )
            .child(
                div()
                    .w(px(CONNECTION_IMPORT_AUTH_WIDTH))
                    .flex_none()
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(imported_auth_label(draft.auth_type, &self.i18n)),
            )
            .when(!disabled, |row| {
                row.cursor_pointer().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.toggle_settings_connection_import_draft(draft_id.clone(), cx);
                        cx.stop_propagation();
                    }),
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_import_duplicate_badge(&self) -> AnyElement {
        div()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((self.tokens.ui.accent << 8) | 0x20))
            .text_size(px(10.0))
            .text_color(rgb(self.tokens.ui.accent))
            .child(self.i18n.t("settings_view.connections.importers.duplicate"))
            .into_any_element()
    }

    pub(in crate::workspace) fn ssh_key_row(
        &self,
        key: oxideterm_connections::SshKeyInfo,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(4.0))
            .py(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .flex_none()
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
                            .flex_1()
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
                row.child(div().flex_none().child(self.text_badge(
                    self.i18n.t("settings_view.ssh_keys.encrypted"),
                    theme.warning,
                )))
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn ssh_key_section_header(
        &self,
        title_key: &str,
        description_key: &str,
        actions: Option<AnyElement>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex_basis(px(SSH_KEY_HEADER_TEXT_BASIS))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(description_key)),
                    ),
            )
            .when_some(actions, |header, actions| header.child(actions))
            .into_any_element()
    }

    pub(in crate::workspace) fn managed_ssh_key_toolbar(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .max_w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .child(self.managed_key_action_button(
                LucideIcon::FileLock,
                "settings_view.ssh_keys.import_file",
                ButtonVariant::Outline,
                cx,
                |this, _event, _window, cx| {
                    this.open_managed_key_import_file_dialog(cx);
                    cx.stop_propagation();
                },
            ))
            .child(self.managed_key_action_button(
                LucideIcon::ShieldCheck,
                "settings_view.ssh_keys.paste_key",
                ButtonVariant::Outline,
                cx,
                |this, _event, _window, cx| {
                    this.open_managed_key_paste_dialog(cx);
                    cx.stop_propagation();
                },
            ))
            .child(self.managed_key_action_button(
                LucideIcon::RefreshCw,
                "settings_view.ssh_keys.refresh",
                ButtonVariant::Ghost,
                cx,
                |this, _event, _window, cx| {
                    this.settings_managed_key_status = None;
                    cx.stop_propagation();
                    cx.notify();
                },
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn managed_key_action_button(
        &self,
        icon: LucideIcon,
        label_key: &'static str,
        variant: ButtonVariant,
        cx: &mut Context<Self>,
        handler: impl Fn(
            &mut WorkspaceApp,
            &gpui::MouseDownEvent,
            &mut Window,
            &mut Context<WorkspaceApp>,
        ) + 'static,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            Some(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
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
            cx.listener(handler),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn managed_ssh_key_row(
        &self,
        key: ManagedSshKeyInfo,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let usage = self
            .connection_store
            .managed_ssh_key_usage(&key.id)
            .map(|usage| usage.count)
            .unwrap_or(0);
        let detail = format!(
            "{} · {} · {}",
            self.managed_key_origin_label(&key.origin),
            if key.requires_passphrase {
                self.i18n.t("settings_view.ssh_keys.passphrase_required")
            } else {
                self.i18n
                    .t("settings_view.ssh_keys.passphrase_not_required")
            },
            self.i18n
                .t("settings_view.ssh_keys.used_by")
                .replace("{{count}}", &usage.to_string())
        );
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(4.0))
            .py(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(rgba((theme.accent << 8) | 0x1a))
                            .child(Self::render_lucide_icon(
                                LucideIcon::ShieldCheck,
                                18.0,
                                rgb(theme.accent),
                            )),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(key.name.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    ))
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(key.fingerprint.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(detail),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.workspace_icon_action_button(
                        LucideIcon::Pencil,
                        14.0,
                        rgb(theme.text),
                        IconButtonOptions::opaque_toolbar(30.0, ButtonRadius::Md),
                        {
                            let key_id = key.id.clone();
                            let key_name = key.name.clone();
                            move |this, _event, _window, cx| {
                                this.open_managed_key_rename_dialog(
                                    key_id.clone(),
                                    key_name.clone(),
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        },
                        cx,
                    ))
                    .child(self.workspace_icon_action_button(
                        LucideIcon::Trash2,
                        14.0,
                        rgb(theme.error),
                        IconButtonOptions {
                            hover_background: Some(rgba((theme.error << 8) | 0x14)),
                            ..IconButtonOptions::opaque_toolbar(30.0, ButtonRadius::Md)
                        },
                        {
                            let key = key;
                            move |this, _event, _window, cx| {
                                this.open_managed_key_delete_dialog(key.clone(), cx);
                                cx.stop_propagation();
                            }
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn managed_ssh_keys_empty_state(&self) -> AnyElement {
        div()
            .w_full()
            .min_w_0()
            .py(px(24.0))
            .text_align(gpui::TextAlign::Center)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.ssh_keys.no_managed_keys"))
            .into_any_element()
    }

    pub(in crate::workspace) fn managed_key_origin_label(
        &self,
        origin: &ManagedSshKeyOrigin,
    ) -> String {
        match origin {
            ManagedSshKeyOrigin::ImportedFile => {
                self.i18n.t("settings_view.ssh_keys.origin_imported_file")
            }
            ManagedSshKeyOrigin::PastedText => {
                self.i18n.t("settings_view.ssh_keys.origin_pasted_text")
            }
            ManagedSshKeyOrigin::OxideImport => {
                self.i18n.t("settings_view.ssh_keys.origin_oxide_import")
            }
        }
    }

    pub(in crate::workspace) fn render_settings_managed_key_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match self.settings_managed_key_dialog.clone()? {
            SettingsManagedKeyDialog::ImportFile => {
                Some(self.render_settings_managed_key_import_file_dialog(cx))
            }
            SettingsManagedKeyDialog::Paste => {
                Some(self.render_settings_managed_key_paste_dialog(cx))
            }
            SettingsManagedKeyDialog::Rename { key_id } => {
                Some(self.render_settings_managed_key_rename_dialog(key_id, cx))
            }
            SettingsManagedKeyDialog::Delete { key, usage } => {
                Some(self.render_settings_managed_key_delete_dialog(key, usage, cx))
            }
        }
    }

    pub(in crate::workspace) fn render_settings_managed_key_import_file_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_import = !self.settings_managed_key_file_path.trim().is_empty();
        self.settings_managed_key_dialog_frame(
            "modals.managed_key.import_file.title",
            "modals.managed_key.import_file.description",
            vec![
                self.settings_managed_key_input_field(
                    "modals.managed_key.import_file.path",
                    SettingsInput::ManagedKeyFilePath,
                    self.settings_managed_key_file_path.clone(),
                    "~/.ssh/id_ed25519".to_string(),
                    420.0,
                    cx,
                ),
                div()
                    .flex()
                    .justify_start()
                    .child(self.managed_key_dialog_button(
                        self.i18n.t("modals.managed_key.import_file.browse_title"),
                        ButtonVariant::Outline,
                        false,
                        |this, _event, _window, cx| {
                            this.pick_managed_key_import_file(cx);
                        },
                        cx,
                    ))
                    .into_any_element(),
                self.settings_managed_key_input_field(
                    "modals.managed_key.display_name",
                    SettingsInput::ManagedKeyFileName,
                    self.settings_managed_key_file_name.clone(),
                    "Managed SSH Key".to_string(),
                    420.0,
                    cx,
                ),
                self.settings_managed_key_secret_input_field(
                    "modals.managed_key.passphrase",
                    SettingsInput::ManagedKeyFilePassphrase,
                    self.settings_managed_key_file_passphrase.clone(),
                    self.i18n.t("modals.managed_key.passphrase_placeholder"),
                    420.0,
                    cx,
                ),
                self.settings_managed_key_hint("modals.managed_key.custody_hint"),
            ],
            self.i18n.t("modals.managed_key.import"),
            can_import,
            |this, _event, _window, cx| {
                this.import_managed_key_from_file(cx);
            },
            cx,
        )
    }

    pub(in crate::workspace) fn render_settings_managed_key_paste_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_import = !self
            .settings_managed_key_paste_private_key
            .trim()
            .is_empty();
        self.settings_managed_key_dialog_frame(
            "modals.managed_key.paste.title",
            "modals.managed_key.paste.description",
            vec![
                self.settings_managed_key_input_field(
                    "modals.managed_key.display_name",
                    SettingsInput::ManagedKeyPasteName,
                    self.settings_managed_key_paste_name.clone(),
                    "Managed SSH Key".to_string(),
                    420.0,
                    cx,
                ),
                self.settings_managed_key_private_key_textarea(cx),
                self.settings_managed_key_secret_input_field(
                    "modals.managed_key.passphrase",
                    SettingsInput::ManagedKeyPastePassphrase,
                    self.settings_managed_key_paste_passphrase.clone(),
                    self.i18n.t("modals.managed_key.passphrase_placeholder"),
                    420.0,
                    cx,
                ),
                self.settings_managed_key_hint("modals.managed_key.custody_hint"),
            ],
            self.i18n.t("modals.managed_key.import"),
            can_import,
            |this, _event, _window, cx| {
                this.import_managed_key_from_paste(cx);
            },
            cx,
        )
    }

    pub(in crate::workspace) fn render_settings_managed_key_rename_dialog(
        &self,
        key_id: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_save = !self.settings_managed_key_rename_name.trim().is_empty();
        self.settings_managed_key_dialog_frame(
            "settings_view.ssh_keys.rename_title",
            "settings_view.ssh_keys.managed_description",
            vec![self.settings_managed_key_input_field(
                "settings_view.ssh_keys.rename_name",
                SettingsInput::ManagedKeyRenameName,
                self.settings_managed_key_rename_name.clone(),
                "Managed SSH Key".to_string(),
                420.0,
                cx,
            )],
            self.i18n.t("settings_view.ssh_keys.rename"),
            can_save,
            move |this, _event, _window, cx| {
                this.rename_managed_key(key_id.clone(), cx);
            },
            cx,
        )
    }

    pub(in crate::workspace) fn render_settings_managed_key_delete_dialog(
        &self,
        key: ManagedSshKeyInfo,
        usage: ManagedSshKeyUsage,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_delete = usage.count == 0;
        let mut description = if can_delete {
            self.i18n
                .t("settings_view.ssh_keys.delete_unused_description")
                .replace("{{name}}", &key.name)
        } else {
            self.i18n
                .t("settings_view.ssh_keys.delete_blocked_description")
                .replace("{{count}}", &usage.count.to_string())
        };
        if !usage.items.is_empty() {
            let used_by = usage
                .items
                .iter()
                .map(|item| format!("{} ({})", item.connection_name, item.location))
                .collect::<Vec<_>>()
                .join(", ");
            description.push_str("\n");
            description.push_str(&used_by);
        }
        self.settings_managed_key_dialog_frame(
            "settings_view.ssh_keys.delete_title",
            "",
            vec![
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(description)
                    .into_any_element(),
            ],
            self.i18n.t("settings_view.ssh_keys.delete"),
            can_delete,
            move |this, _event, _window, cx| {
                this.delete_managed_key(key.id.clone(), cx);
            },
            cx,
        )
    }

    pub(in crate::workspace) fn settings_managed_key_dialog_frame(
        &self,
        title_key: &str,
        description_key: &str,
        rows: Vec<AnyElement>,
        confirm_label: String,
        can_confirm: bool,
        confirm: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let backdrop = dismissible_dialog_backdrop().on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.close_managed_key_dialog(cx);
                cx.stop_propagation();
            }),
        );
        let form = dialog_content(&self.tokens)
            .w(px(520.0))
            .max_w(relative(0.92))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(&self.tokens, self.i18n.t(title_key)))
                    .when(!description_key.is_empty(), |header| {
                        header.child(dialog_description(
                            &self.tokens,
                            self.i18n.t(description_key),
                        ))
                    }),
            )
            .child(
                div()
                    .px(px(24.0))
                    .py(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .children(rows),
            )
            .child(
                dialog_footer(&self.tokens)
                    .child(self.standard_footer_action_button(
                        self.i18n.t("common.actions.cancel"),
                        ButtonVariant::Outline,
                        ConfirmDialogAction::Cancel,
                        false,
                        |this, _event, _window, cx| {
                            this.close_managed_key_dialog(cx);
                        },
                        cx,
                    ))
                    .child(self.standard_footer_action_button(
                        confirm_label,
                        ButtonVariant::Default,
                        ConfirmDialogAction::Confirm,
                        !can_confirm,
                        confirm,
                        cx,
                    )),
            );
        settings_dialog_transition(
            &self.tokens,
            "managed-key-dialog-form",
            backdrop,
            form,
            self.managed_key_dialog_presence,
        )
    }

    pub(in crate::workspace) fn settings_managed_key_input_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.settings_text_input_control(input, value, placeholder, width, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_managed_key_secret_input_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.settings_secret_text_input_control(input, value, placeholder, width, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_managed_key_private_key_textarea(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = SettingsInput::ManagedKeyPastePrivateKey;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.settings_managed_key_paste_private_key.clone()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut textarea = div()
            .w_full()
            .min_h(px(160.0))
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
            textarea = self.render_settings_multiline_textarea_lines(
                textarea,
                target,
                "-----BEGIN OPENSSH PRIVATE KEY-----",
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
        let control =
            text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
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
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("modals.managed_key.paste.private_key")),
            )
            .child(control)
            .into_any_element()
    }

    pub(in crate::workspace) fn settings_managed_key_hint(&self, label_key: &str) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t(label_key))
            .into_any_element()
    }

    pub(in crate::workspace) fn managed_key_dialog_button(
        &self,
        label: String,
        variant: ButtonVariant,
        disabled: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(listener),
        )
    }

    pub(in crate::workspace) fn open_managed_key_import_file_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.clear_managed_key_dialog_drafts();
        self.settings_managed_key_status = None;
        self.managed_key_dialog_presence.reopen();
        self.settings_managed_key_dialog = Some(SettingsManagedKeyDialog::ImportFile);
        cx.notify();
    }

    pub(in crate::workspace) fn open_managed_key_paste_dialog(&mut self, cx: &mut Context<Self>) {
        self.clear_managed_key_dialog_drafts();
        self.settings_managed_key_status = None;
        self.managed_key_dialog_presence.reopen();
        self.settings_managed_key_dialog = Some(SettingsManagedKeyDialog::Paste);
        cx.notify();
    }

    pub(in crate::workspace) fn open_managed_key_rename_dialog(
        &mut self,
        key_id: String,
        key_name: String,
        cx: &mut Context<Self>,
    ) {
        self.clear_managed_key_dialog_drafts();
        self.settings_managed_key_rename_name = key_name;
        self.managed_key_dialog_presence.reopen();
        self.settings_managed_key_dialog = Some(SettingsManagedKeyDialog::Rename { key_id });
        cx.notify();
    }

    pub(in crate::workspace) fn open_managed_key_delete_dialog(
        &mut self,
        key: ManagedSshKeyInfo,
        cx: &mut Context<Self>,
    ) {
        match self.connection_store.managed_ssh_key_usage(&key.id) {
            Ok(usage) => {
                self.managed_key_dialog_presence.reopen();
                self.settings_managed_key_dialog =
                    Some(SettingsManagedKeyDialog::Delete { key, usage });
            }
            Err(error) => self.set_managed_key_action_error(error),
        }
        cx.notify();
    }

    pub(in crate::workspace) fn close_managed_key_dialog(&mut self, cx: &mut Context<Self>) {
        self.clear_standard_confirm_focus();
        let Some(generation) = self.managed_key_dialog_presence.begin_exit() else {
            return;
        };
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if delay.is_zero() {
            self.settings_managed_key_dialog = None;
            self.clear_managed_key_dialog_drafts();
            self.managed_key_dialog_presence.reopen();
            cx.notify();
            return;
        }
        cx.spawn(async move |weak, cx| {
            gpui::Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.managed_key_dialog_presence.finish_exit(generation) {
                    this.settings_managed_key_dialog = None;
                    this.clear_managed_key_dialog_drafts();
                    this.managed_key_dialog_presence.reopen();
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn clear_managed_key_dialog_drafts(&mut self) {
        self.settings_managed_key_file_path.clear();
        self.settings_managed_key_file_name.clear();
        zeroize::Zeroize::zeroize(&mut self.settings_managed_key_file_passphrase);
        self.settings_managed_key_paste_name.clear();
        zeroize::Zeroize::zeroize(&mut self.settings_managed_key_paste_private_key);
        zeroize::Zeroize::zeroize(&mut self.settings_managed_key_paste_passphrase);
        self.settings_managed_key_rename_name.clear();
        if matches!(
            self.focused_settings_input,
            Some(SettingsInput::ManagedKeyFilePath)
                | Some(SettingsInput::ManagedKeyFileName)
                | Some(SettingsInput::ManagedKeyFilePassphrase)
                | Some(SettingsInput::ManagedKeyPasteName)
                | Some(SettingsInput::ManagedKeyPastePrivateKey)
                | Some(SettingsInput::ManagedKeyPastePassphrase)
                | Some(SettingsInput::ManagedKeyRenameName)
        ) {
            if self
                .focused_settings_input
                .is_some_and(|input| input.is_secret())
            {
                zeroize::Zeroize::zeroize(&mut self.settings_input_draft);
            } else {
                self.settings_input_draft.clear();
            }
            self.focused_settings_input = None;
        }
    }

    pub(in crate::workspace) fn pick_managed_key_import_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(gpui::SharedString::from(
                self.i18n.t("modals.managed_key.import_file.browse_title"),
            )),
        });
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Managed SSH Key")
                .to_string();
            let _ = weak.update(cx, |this, cx| {
                this.settings_managed_key_file_path = path.display().to_string();
                if this.settings_managed_key_file_name.trim().is_empty() {
                    this.settings_managed_key_file_name = file_name;
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn import_managed_key_from_file(&mut self, cx: &mut Context<Self>) {
        let path = self.current_settings_managed_key_file_path();
        let name = self.optional_trimmed_string(&self.settings_managed_key_file_name);
        let passphrase =
            self.optional_managed_key_secret(&self.settings_managed_key_file_passphrase);
        match self
            .connection_store
            .create_managed_ssh_key_from_file(path.trim(), name, passphrase)
        {
            Ok(info) => {
                self.settings_managed_key_status = Some(
                    self.i18n
                        .t("settings_view.ssh_keys.import_success")
                        .replace("{{name}}", &info.name),
                );
                self.settings_managed_key_dialog = None;
                self.clear_managed_key_dialog_drafts();
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => self.set_managed_key_action_error(error),
        }
        cx.notify();
    }

    pub(in crate::workspace) fn import_managed_key_from_paste(&mut self, cx: &mut Context<Self>) {
        let private_key = self.current_settings_managed_key_private_key();
        let name = self.optional_trimmed_string(&self.settings_managed_key_paste_name);
        let passphrase =
            self.optional_managed_key_secret(&self.settings_managed_key_paste_passphrase);
        // Transfer the pasted private key into SecretString before clearing UI drafts.
        let private_key_secret = SecretString::from(private_key);
        match self.connection_store.create_managed_ssh_key_from_text(
            private_key_secret,
            name,
            passphrase,
        ) {
            Ok(info) => {
                self.settings_managed_key_status = Some(
                    self.i18n
                        .t("settings_view.ssh_keys.import_success")
                        .replace("{{name}}", &info.name),
                );
                self.settings_managed_key_dialog = None;
                self.clear_managed_key_dialog_drafts();
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => self.set_managed_key_action_error(error),
        }
        cx.notify();
    }

    pub(in crate::workspace) fn rename_managed_key(
        &mut self,
        key_id: String,
        cx: &mut Context<Self>,
    ) {
        let name = self.current_settings_managed_key_rename_name();
        match self
            .connection_store
            .rename_managed_ssh_key(&key_id, name.trim().to_string())
        {
            Ok(info) => {
                self.settings_managed_key_status = Some(
                    self.i18n
                        .t("settings_view.ssh_keys.rename_success")
                        .replace("{{name}}", &info.name),
                );
                self.settings_managed_key_dialog = None;
                self.clear_managed_key_dialog_drafts();
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => self.set_managed_key_action_error(error),
        }
        cx.notify();
    }

    pub(in crate::workspace) fn delete_managed_key(
        &mut self,
        key_id: String,
        cx: &mut Context<Self>,
    ) {
        match self.connection_store.delete_managed_ssh_key(&key_id, false) {
            Ok(result) => {
                self.settings_managed_key_status = Some(
                    self.i18n
                        .t("settings_view.ssh_keys.delete_success")
                        .replace("{{count}}", &result.deleted.to_string()),
                );
                self.settings_managed_key_dialog = None;
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Err(error) => self.set_managed_key_action_error(error),
        }
        cx.notify();
    }

    pub(in crate::workspace) fn set_managed_key_action_error(
        &mut self,
        error: impl std::fmt::Display,
    ) {
        self.settings_managed_key_status = Some(
            self.i18n
                .t("settings_view.ssh_keys.action_failed")
                .replace("{{error}}", &error.to_string()),
        );
    }

    pub(in crate::workspace) fn current_settings_managed_key_file_path(&self) -> String {
        if self.focused_settings_input == Some(SettingsInput::ManagedKeyFilePath) {
            self.settings_input_draft.clone()
        } else {
            self.settings_managed_key_file_path.clone()
        }
    }

    pub(in crate::workspace) fn current_settings_managed_key_private_key(&self) -> String {
        if self.focused_settings_input == Some(SettingsInput::ManagedKeyPastePrivateKey) {
            self.settings_input_draft.clone()
        } else {
            self.settings_managed_key_paste_private_key.clone()
        }
    }

    pub(in crate::workspace) fn current_settings_managed_key_rename_name(&self) -> String {
        if self.focused_settings_input == Some(SettingsInput::ManagedKeyRenameName) {
            self.settings_input_draft.clone()
        } else {
            self.settings_managed_key_rename_name.clone()
        }
    }

    pub(in crate::workspace) fn optional_trimmed_string(&self, value: &str) -> Option<String> {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }

    pub(in crate::workspace) fn optional_managed_key_secret(
        &self,
        value: &str,
    ) -> Option<SecretString> {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| SecretString::from(trimmed.to_string()))
    }

    pub(in crate::workspace) fn ssh_keys_empty_state(&self) -> AnyElement {
        div()
            .w_full()
            .py(px(24.0))
            .text_align(gpui::TextAlign::Center)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n.t("settings_view.ssh_keys.no_keys"))
            .into_any_element()
    }

    pub(in crate::workspace) fn connection_status_row(&self, status: String) -> AnyElement {
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

    pub(in crate::workspace) fn toggle_settings_ssh_config_host(
        &mut self,
        alias: String,
        cx: &mut Context<Self>,
    ) {
        self.settings_page.toggle_ssh_host_selection(alias);
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_all_settings_ssh_config_hosts(
        &mut self,
        all_selected: bool,
        cx: &mut Context<Self>,
    ) {
        if all_selected {
            self.settings_page.clear_ssh_host_selection();
        } else {
            let existing_names = self
                .connection_store
                .connections()
                .iter()
                .map(|conn| conn.name.clone())
                .collect::<HashSet<_>>();
            if let Ok(hosts) = list_ssh_config_hosts(&existing_names) {
                self.settings_page.set_selected_ssh_hosts(
                    hosts
                        .into_iter()
                        .filter(|host| !host.already_imported)
                        .map(|host| host.alias)
                        .collect(),
                );
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn import_settings_ssh_host(
        &mut self,
        alias: String,
        cx: &mut Context<Self>,
    ) {
        match oxideterm_connections::import_ssh_config_alias(&mut self.connection_store, &alias) {
            Ok(true) => {
                self.settings_page.remove_selected_ssh_host(&alias);
                self.settings_page.set_connection_status(Some(
                    self.i18n
                        .t("settings_view.errors.import_success")
                        .replace("{{name}}", &alias),
                ));
                self.queue_cloud_sync_dirty_refresh(cx);
            }
            Ok(false) => {
                self.settings_page.set_connection_status(Some(
                    self.i18n
                        .t("settings_view.connections.ssh_config.batch_import_skipped")
                        .replace("{{count}}", "1"),
                ));
            }
            Err(error) => {
                self.settings_page.set_connection_status(Some(
                    self.i18n
                        .t("settings_view.errors.import_failed")
                        .replace("{{error}}", &error.to_string()),
                ));
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn import_selected_settings_ssh_hosts(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let aliases = self
            .settings_page
            .settings_selected_ssh_hosts
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut imported = 0usize;
        let mut skipped = 0usize;
        let mut errors = Vec::new();

        for alias in aliases {
            match oxideterm_connections::import_ssh_config_alias(&mut self.connection_store, &alias)
            {
                Ok(true) => {
                    imported += 1;
                    self.settings_page.remove_selected_ssh_host(&alias);
                }
                Ok(false) => {
                    skipped += 1;
                    self.settings_page.remove_selected_ssh_host(&alias);
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
        self.settings_page
            .set_connection_status((!parts.is_empty()).then(|| parts.join("; ")));
        if imported > 0 {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn set_connection_import_source(
        &mut self,
        source: ConnectionImportSource,
        cx: &mut Context<Self>,
    ) {
        if self.settings_connection_import_source == source {
            return;
        }
        self.settings_connection_import_source = source;
        self.clear_connection_import_preview();
        self.settings_connection_import_paths.clear();
        self.settings_page.set_connection_status(None);
        cx.notify();
    }

    pub(in crate::workspace) fn pick_connection_import_paths(
        &mut self,
        directories: bool,
        cx: &mut Context<Self>,
    ) {
        let multiple = !directories
            && self.settings_connection_import_source != ConnectionImportSource::Termius;
        let prompt_key = if directories {
            "settings_view.connections.importers.choose_directory"
        } else {
            "settings_view.connections.importers.choose_files"
        };
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !directories,
            directories,
            multiple,
            prompt: Some(SharedString::from(self.i18n.t(prompt_key))),
        });
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let selected = paths
                .into_iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            if selected.is_empty() {
                return;
            }
            let _ = weak.update(cx, |this, cx| {
                this.settings_connection_import_paths = selected;
                this.clear_connection_import_preview();
                this.settings_page.set_connection_status(None);
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn preview_settings_connection_import(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.settings_connection_import_paths.is_empty() {
            return;
        }
        let existing_names = self
            .connection_store
            .connections()
            .iter()
            .map(|conn| conn.name.clone())
            .collect::<HashSet<_>>();
        match preview_connection_import(
            self.settings_connection_import_source,
            &self.settings_connection_import_paths,
            &existing_names,
        ) {
            Ok(preview) => {
                self.settings_selected_connection_import_drafts = preview
                    .drafts
                    .iter()
                    .filter(|draft| draft.importable && !draft.duplicate)
                    .map(|draft| draft.id.clone())
                    .collect();
                self.settings_connection_import_preview = Some(preview);
                self.settings_page.set_connection_status(None);
            }
            Err(error) => {
                self.settings_page.set_connection_status(Some(
                    self.i18n
                        .t("settings_view.connections.importers.preview_failed")
                        .replace("{{error}}", &error.to_string()),
                ));
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_settings_connection_import_draft(
        &mut self,
        draft_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self
            .settings_selected_connection_import_drafts
            .insert(draft_id.clone())
        {
            self.settings_selected_connection_import_drafts
                .remove(&draft_id);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn toggle_all_settings_connection_import_drafts(
        &mut self,
        all_selected: bool,
        cx: &mut Context<Self>,
    ) {
        if all_selected {
            self.settings_selected_connection_import_drafts.clear();
        } else if let Some(preview) = self.settings_connection_import_preview.as_ref() {
            self.settings_selected_connection_import_drafts = preview
                .drafts
                .iter()
                .filter(|draft| draft.importable)
                .map(|draft| draft.id.clone())
                .collect();
        }
        cx.notify();
    }

    pub(in crate::workspace) fn apply_settings_connection_import(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.settings_selected_connection_import_drafts.is_empty()
            || self.settings_connection_import_paths.is_empty()
        {
            return;
        }
        let request = ConnectionImportApplyRequest {
            source: self.settings_connection_import_source,
            paths: self.settings_connection_import_paths.clone(),
            selected_draft_ids: self
                .settings_selected_connection_import_drafts
                .iter()
                .cloned()
                .collect(),
            duplicate_strategy: self.settings_connection_import_duplicate_strategy,
            target_group: non_empty_trimmed(&self.settings_connection_import_target_group),
        };
        match apply_connection_import(&mut self.connection_store, request) {
            Ok(result) => {
                let mut parts = Vec::new();
                if result.imported > 0 {
                    parts.push(
                        self.i18n
                            .t("settings_view.connections.importers.imported_count")
                            .replace("{{count}}", &result.imported.to_string()),
                    );
                }
                if result.skipped > 0 {
                    parts.push(
                        self.i18n
                            .t("settings_view.connections.importers.skipped_count")
                            .replace("{{count}}", &result.skipped.to_string()),
                    );
                }
                if result.renamed > 0 {
                    parts.push(
                        self.i18n
                            .t("settings_view.connections.importers.renamed_count")
                            .replace("{{count}}", &result.renamed.to_string()),
                    );
                }
                if !result.errors.is_empty() {
                    parts.push(
                        self.i18n
                            .t("settings_view.connections.importers.error_count")
                            .replace("{{count}}", &result.errors.len().to_string()),
                    );
                }
                self.settings_page
                    .set_connection_status(Some(if parts.is_empty() {
                        self.i18n
                            .t("settings_view.connections.importers.no_changes")
                    } else {
                        parts.join(" · ")
                    }));
                if result.imported > 0 {
                    self.queue_cloud_sync_dirty_refresh(cx);
                }
                self.preview_settings_connection_import(cx);
            }
            Err(error) => {
                self.settings_page.set_connection_status(Some(
                    self.i18n
                        .t("settings_view.connections.importers.apply_failed")
                        .replace("{{error}}", &error.to_string()),
                ));
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn clear_connection_import_preview(&mut self) {
        self.settings_connection_import_preview = None;
        self.settings_selected_connection_import_drafts.clear();
    }
}

pub(in crate::workspace) fn connection_idle_timeout_options(i18n: &I18n) -> Vec<(i64, String)> {
    vec![
        (300, i18n.t("settings_view.connections.idle_timeout.5min")),
        (900, i18n.t("settings_view.connections.idle_timeout.15min")),
        (1800, i18n.t("settings_view.connections.idle_timeout.30min")),
        (3600, i18n.t("settings_view.connections.idle_timeout.1hr")),
        (0, i18n.t("settings_view.connections.idle_timeout.never")),
    ]
}

pub(in crate::workspace) fn connection_idle_timeout_label(seconds: i64, i18n: &I18n) -> String {
    connection_idle_timeout_options(i18n)
        .into_iter()
        .find_map(|(value, label)| (value == seconds).then_some(label))
        .unwrap_or_else(|| seconds.to_string())
}

pub(in crate::workspace) fn connection_import_source_options() -> &'static [ConnectionImportSource]
{
    &[
        ConnectionImportSource::SecureCrt,
        ConnectionImportSource::Xshell,
        ConnectionImportSource::Termius,
        ConnectionImportSource::MobaXterm,
        ConnectionImportSource::WindTerm,
    ]
}

pub(in crate::workspace) fn connection_import_source_label(
    source: ConnectionImportSource,
    i18n: &I18n,
) -> String {
    match source {
        ConnectionImportSource::SecureCrt => {
            i18n.t("settings_view.connections.importers.sources.securecrt")
        }
        ConnectionImportSource::Xshell => {
            i18n.t("settings_view.connections.importers.sources.xshell")
        }
        ConnectionImportSource::Termius => {
            i18n.t("settings_view.connections.importers.sources.termius")
        }
        ConnectionImportSource::MobaXterm => {
            i18n.t("settings_view.connections.importers.sources.mobaxterm")
        }
        ConnectionImportSource::WindTerm => {
            i18n.t("settings_view.connections.importers.sources.windterm")
        }
    }
}

pub(in crate::workspace) fn connection_import_duplicate_strategy_label(
    strategy: ConnectionImportDuplicateStrategy,
    i18n: &I18n,
) -> String {
    match strategy {
        ConnectionImportDuplicateStrategy::Skip => {
            i18n.t("settings_view.connections.importers.duplicate_skip")
        }
        ConnectionImportDuplicateStrategy::Rename => {
            i18n.t("settings_view.connections.importers.duplicate_rename")
        }
    }
}

pub(in crate::workspace) fn imported_auth_label(
    auth_type: ImportedConnectionAuthType,
    _i18n: &I18n,
) -> String {
    match auth_type {
        ImportedConnectionAuthType::Password => "password",
        ImportedConnectionAuthType::Key => "key",
        ImportedConnectionAuthType::Certificate => "certificate",
        ImportedConnectionAuthType::Agent => "agent",
    }
    .to_string()
}

pub(in crate::workspace) fn non_empty_trimmed(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
