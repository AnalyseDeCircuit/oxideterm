impl WorkspaceApp {
    fn settings_portable_section(
        &mut self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_portable_settings_snapshot(cx);
        match section_index {
            0 => div()
                .flex()
                .flex_col()
                .gap(px(self.tokens.metrics.settings_page_gap))
                .child(self.portable_runtime_card(cx))
                .child(self.portable_migration_card(cx))
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }

    fn portable_runtime_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let portable_status = self.portable_status_snapshot.as_ref();
        let is_portable = portable_status.is_some_and(|status| status.is_portable);
        let hint_key = if is_portable {
            "settings_view.general.portable_runtime_hint"
        } else {
            "settings_view.general.portable_runtime_disabled_hint"
        };
        let mut rows = vec![
            self.portable_runtime_summary_row(portable_status, hint_key),
            self.card_separator(),
        ];

        if let Some(status) = portable_status.filter(|status| status.is_portable) {
            rows.push(self.portable_path_group(status, cx));
            rows.push(self.card_separator());
            rows.push(self.portable_security_group(status, cx));
        } else {
            rows.push(self.portable_disabled_notice());
        }

        self.plain_settings_card(
            std::iter::once(self.card_title("settings_view.general.portable_runtime"))
                .chain(rows)
                .collect(),
        )
    }

    fn portable_runtime_summary_row(
        &self,
        portable_status: Option<&oxideterm_portable_runtime::PortableStatusSnapshot>,
        hint_key: &str,
    ) -> AnyElement {
        let (badge_label, badge_color) = portable_status
            .map(|status| {
                (
                    format!("{:?}", status.status),
                    portable_status_badge_color(status.status, &self.tokens),
                )
            })
            .unwrap_or_else(|| {
                (
                    self.i18n.t("settings_view.general.portable_activation_disabled"),
                    self.tokens.ui.text_muted,
                )
            });

        div()
            .w_full()
            .flex()
            .flex_row()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.general.portable_runtime")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(self.text_badge(badge_label, badge_color))
            .into_any_element()
    }

    fn portable_path_group(
        &self,
        status: &oxideterm_portable_runtime::PortableStatusSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(PORTABLE_SETTINGS_PATH_CARD_GAP))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg))
            .p(px(12.0))
            .child(self.portable_value_box(
                "settings_view.general.portable_root_dir",
                status.portable_root_dir.clone(),
                true,
                cx,
            ))
            .child(self.portable_value_box(
                "settings_view.general.portable_activation",
                portable_activation_label(&self.i18n, status.activation),
                false,
                cx,
            ))
            .child(self.portable_value_box(
                "settings_view.general.portable_config_path",
                status.config_path.clone(),
                true,
                cx,
            ))
            .child(self.portable_value_box(
                "settings_view.general.data_directory",
                status.data_dir.clone(),
                true,
                cx,
            ))
            .child(self.portable_value_box(
                "settings_view.general.portable_instance_lock_path",
                status.instance_lock_path.clone().unwrap_or_else(|| {
                    self.i18n
                        .t("settings_view.general.portable_instance_lock_unavailable")
                }),
                true,
                cx,
            ))
            .into_any_element()
    }

    fn portable_value_box(
        &self,
        label_key: &str,
        value: String,
        mono: bool,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut value_row = div()
            .mt(px(4.0))
            .w_full()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgb(self.tokens.ui.bg))
            .px(px(10.0))
            .py(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .child(value);
        if mono {
            value_row =
                value_row.font_family(settings_mono_font_family(self.settings_store.settings()));
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .min_w(px(0.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .child(value_row)
            .into_any_element()
    }

    fn portable_security_group(
        &self,
        status: &oxideterm_portable_runtime::PortableStatusSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let can_change_password = status.is_unlocked;
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(PORTABLE_SETTINGS_BUTTON_GAP))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg))
            .p(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.general.portable_biometric")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.general.portable_biometric_unsupported"),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(PORTABLE_SETTINGS_BUTTON_GAP))
                    .child(self.portable_action_button(
                        self.i18n.t("settings_view.general.portable_change_password"),
                        LucideIcon::Key,
                        can_change_password,
                        false,
                        |this, _event, _window, cx| {
                            this.open_portable_password_change_dialog(cx);
                        },
                        cx,
                    )),
            )
            .when_some(self.portable_settings_action_error.clone(), |group, error| {
                group.child(
                    div()
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba((self.tokens.ui.error << 8) | 0x4d))
                        .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                        .px(px(10.0))
                        .py(px(8.0))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.error))
                        .child(error),
                )
            })
            .into_any_element()
    }

    fn portable_disabled_notice(&self) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg))
            .px(px(12.0))
            .py(px(12.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                self.i18n
                    .t("settings_view.general.portable_runtime_disabled_hint"),
            )
            .into_any_element()
    }

    fn portable_migration_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let portable_status = self.portable_status_snapshot.as_ref();
        let is_portable = portable_status.is_some_and(|status| status.is_portable);
        let current_data_dir = self
            .settings_store
            .path()
            .parent()
            .unwrap_or_else(|| self.settings_store.path())
            .display()
            .to_string();
        let portable_data_dir = portable_status
            .map(|status| status.data_dir.clone())
            .unwrap_or_else(|| current_data_dir.clone());
        let secret_count = self.portable_exportable_secret_count.unwrap_or(0);

        self.plain_settings_card(vec![
            self.card_title("settings_view.general.portable_migration"),
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(self.tokens.ui.text))
                        .child(self.i18n.t("settings_view.general.portable_migration")),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(if is_portable {
                            self.i18n
                                .t("settings_view.general.portable_migration_portable_hint")
                        } else {
                            self.i18n
                                .t("settings_view.general.portable_migration_installed_hint")
                        }),
                )
                .into_any_element(),
            div()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(self.tokens.ui.border))
                .bg(self.settings_panel_background(self.tokens.ui.bg))
                .p(px(12.0))
                .flex()
                .flex_col()
                .gap(px(PORTABLE_SETTINGS_PATH_CARD_GAP))
                .child(self.portable_value_box(
                    "settings_view.general.portable_migration_current_dir",
                    current_data_dir,
                    true,
                    cx,
                ))
                .child(self.portable_value_box(
                    "settings_view.general.portable_migration_target_dir",
                    portable_data_dir,
                    true,
                    cx,
                ))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n_with(
                            "settings_view.general.portable_migration_secret_summary",
                            &[("count", secret_count.to_string())],
                        )),
                )
                .into_any_element(),
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(px(PORTABLE_SETTINGS_BUTTON_GAP))
                .child(self.portable_action_button(
                    self.i18n.t("settings_view.general.portable_migration_export"),
                    LucideIcon::Upload,
                    true,
                    false,
                    |this, _event, _window, cx| {
                        this.open_oxide_export_portable_migration_dialog(cx);
                    },
                    cx,
                ))
                .child(self.portable_action_button(
                    self.i18n.t("settings_view.general.portable_migration_import"),
                    LucideIcon::Download,
                    true,
                    false,
                    |this, _event, _window, cx| {
                        this.open_oxide_import_portable_migration_dialog(cx);
                    },
                    cx,
                ))
                .into_any_element(),
        ])
    }

    fn portable_action_button(
        &self,
        label: String,
        icon: LucideIcon,
        enabled: bool,
        loading: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(icon, 14.0, rgb(self.tokens.ui.text)).into_any_element()),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: !enabled,
                },
                icon_position: ToolbarButtonIconPosition::Leading,
                loading,
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, event, window, cx| {
                listener(this, event, window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }
}
