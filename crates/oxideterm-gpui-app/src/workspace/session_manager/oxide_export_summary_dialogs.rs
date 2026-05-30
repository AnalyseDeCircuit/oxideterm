fn oxide_export_summary_line_signature(line: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Warning lines are visible verbatim in the compact preflight body.
    line.hash(&mut hasher);
    hasher.finish()
}

impl WorkspaceApp {
    fn render_oxide_export_preflight(
        &self,
        preflight: Option<ExportPreflightResult>,
        show_card: bool,
        embed_keys: bool,
        include_passwords: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut section = div().flex().flex_col().gap(px(8.0)).child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(theme.text))
                .child(Self::render_lucide_icon(
                    LucideIcon::Shield,
                    16.0,
                    rgb(theme.text),
                ))
                .child(self.render_selectable_text_scoped(
                    "oxide-export-preflight-heading",
                    (),
                    "导出概览",
                    theme.text,
                    cx,
                )),
        );
        let Some(preflight) = preflight.filter(|_| show_card) else {
            return section.into_any_element();
        };
        let mut card_children = vec![
            div()
                .grid()
                .grid_cols(3)
                .gap(px(8.0))
                .children([
                    (
                        LucideIcon::Lock,
                        format!("{} 个使用密码", preflight.connections_with_passwords),
                    ),
                    (
                        LucideIcon::Key,
                        format!("{} 个使用密钥", preflight.connections_with_keys),
                    ),
                    (
                        LucideIcon::FileLock,
                        format!("{} 个使用代理", preflight.connections_with_agent),
                    ),
                ].into_iter().map(|(icon, label)| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(Self::render_lucide_icon(icon, 12.0, rgb(theme.text_muted)))
                        .child(self.render_selectable_text_scoped(
                            "oxide-export-preflight-stat",
                            label.clone(),
                            label,
                            theme.text_muted,
                            cx,
                        ))
                }))
                .into_any_element(),
        ];
        if preflight.portable_secret_count > 0 {
            let label = format!("将打包 {} 项便携秘密项。", preflight.portable_secret_count);
            card_children.push(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "oxide-export-preflight-portable-secret",
                        (),
                        label,
                        theme.text_muted,
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        card_children.push(
            div()
                .grid()
                .grid_cols(2)
                .gap(px(8.0))
                .children(
                    [
                        self.i18n
                            .t("modals.export.summary_key_passphrases")
                            .replace(
                                "{{count}}",
                                &preflight.key_passphrase_count.to_string(),
                            ),
                        self.i18n.t("modals.export.summary_managed_keys").replace(
                            "{{count}}",
                            &preflight.managed_key_count.to_string(),
                        ),
                        self.i18n
                            .t("modals.export.summary_managed_key_passphrases")
                            .replace(
                                "{{count}}",
                                &preflight.managed_key_passphrase_count.to_string(),
                            ),
                    ]
                    .into_iter()
                    .map(|label| {
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_text_scoped(
                                "oxide-export-preflight-credential-stat",
                                label.clone(),
                                label,
                                theme.text_muted,
                                cx,
                            ))
                    }),
                )
                .into_any_element(),
        );
        if !preflight.can_export {
            card_children.push(self.render_oxide_compact_warning(
                OXIDE_RED_500,
                self.i18n
                    .t("modals.export.warning_managed_keys_required")
                    .replace(
                        "{{count}}",
                        &preflight.blocked_managed_key_connections.len().to_string(),
                    ),
                Vec::new(),
                cx,
            ));
        }
        if preflight.connections_with_passwords > 0 {
            let password_warning = if include_passwords {
                self.i18n
                    .t("modals.export.warning_passwords_included")
                    .replace(
                        "{{count}}",
                        &preflight.connections_with_passwords.to_string(),
                    )
            } else {
                self.i18n
                    .t("modals.export.warning_passwords_excluded")
                    .replace(
                        "{{count}}",
                        &preflight.connections_with_passwords.to_string(),
                    )
            };
            card_children.push(self.render_oxide_compact_warning(
                OXIDE_YELLOW_500,
                password_warning,
                Vec::new(),
                cx,
            ));
        }
        if embed_keys && !preflight.missing_keys.is_empty() {
            card_children.push(self.render_oxide_compact_warning(
                OXIDE_YELLOW_500,
                format!("{} 个密钥文件未找到：", preflight.missing_keys.len()),
                preflight
                    .missing_keys
                    .iter()
                    .map(|(name, path)| format!("{name}: {path}"))
                    .collect(),
                cx,
            ));
        }
        if preflight.total_key_bytes > 0 {
            let label = format!("密钥数据总计：{}", oxide_format_bytes(preflight.total_key_bytes));
            card_children.push(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "oxide-export-preflight-key-bytes",
                        (),
                        label,
                        theme.text_muted,
                        cx,
                    ))
                    .into_any_element(),
            );
        }

        section = section.child(self.render_oxide_card(None, card_children, cx));
        section.into_any_element()
    }

    fn render_oxide_export_content_summary(
        &self,
        dialog: &OxideExportDialogState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut items = Vec::new();
        let connection_count = oxide_export_connection_count(dialog);
        if connection_count > 0 {
            items.push(
                self.i18n
                    .t("modals.export.content_summary_connections")
                    .replace("{{count}}", &connection_count.to_string()),
            );
        }
        if dialog.include_forwards && !dialog.selected_forward_ids.is_empty() {
            items.push(
                self.i18n
                    .t("modals.export.content_summary_forwards")
                    .replace("{{count}}", &dialog.selected_forward_ids.len().to_string()),
            );
        }
        if dialog.include_app_settings && !dialog.selected_app_settings_sections.is_empty() {
            let labels = OXIDE_APP_SETTINGS_SECTIONS
                .iter()
                .filter(|section| dialog.selected_app_settings_sections.contains(**section))
                .map(|section| oxide_settings_section_label(section, &self.i18n))
                .collect::<Vec<_>>()
                .join(", ");
            items.push(format!(
                "{}: {labels}",
                self.i18n.t("modals.export.content_summary_app_settings")
            ));
        }
        let selected_plugin_setting_count = oxide_export_selected_plugin_setting_count(dialog);
        if dialog.include_plugin_settings && selected_plugin_setting_count > 0 {
            items.push(
                self.i18n
                    .t("modals.export.content_summary_plugin_settings")
                    .replace("{{plugins}}", &dialog.selected_plugin_ids.len().to_string())
                    .replace("{{count}}", &selected_plugin_setting_count.to_string()),
            );
        }
        if dialog.include_portable_secrets {
            let count = dialog
                .preflight
                .as_ref()
                .map(|preflight| preflight.portable_secret_count)
                    .unwrap_or(0);
            items.push(
                self.i18n
                    .t("modals.export.content_summary_portable_secrets")
                    .replace("{{count}}", &count.to_string()),
            );
        }
        if dialog.embed_keys {
            items.push(self.i18n.t("modals.export.content_summary_embed_keys"));
        }
        if dialog.include_passwords {
            items.push(self.i18n.t("modals.export.content_summary_passwords"));
        }
        if dialog.include_key_passphrases {
            items.push(self.i18n.t("modals.export.content_summary_key_passphrases"));
        }
        if dialog.include_managed_keys {
            if let Some(count) = dialog
                .preflight
                .as_ref()
                .map(|preflight| preflight.managed_key_count)
                .filter(|count| *count > 0)
            {
                items.push(
                    self.i18n
                        .t("modals.export.content_summary_managed_keys")
                        .replace("{{count}}", &count.to_string()),
                );
            }
        }
        if dialog.include_managed_key_passphrases {
            if let Some(count) = dialog
                .preflight
                .as_ref()
                .map(|preflight| preflight.managed_key_passphrase_count)
                .filter(|count| *count > 0)
            {
                items.push(
                    self.i18n
                        .t("modals.export.content_summary_managed_key_passphrases")
                        .replace("{{count}}", &count.to_string()),
                );
            }
        }
        if let Some(preflight) = dialog.preflight.as_ref().filter(|preflight| !preflight.can_export)
        {
            items.push(
                self.i18n
                    .t("modals.export.warning_managed_keys_required")
                    .replace(
                        "{{count}}",
                        &preflight.blocked_managed_key_connections.len().to_string(),
                    ),
            );
        }
        let content = if items.is_empty() {
            vec![
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "oxide-export-content-summary-empty",
                        (),
                        self.i18n.t("modals.export.app_settings_no_sections"),
                        self.tokens.ui.text_muted,
                        cx,
                    ))
                    .into_any_element(),
            ]
        } else {
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| {
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.render_selectable_text_scoped(
                            "oxide-export-content-summary-item",
                            index,
                            format!("• {item}"),
                            self.tokens.ui.text_muted,
                            cx,
                        ))
                        .into_any_element()
                })
                .collect()
        };
        self.render_oxide_card(
            Some((LucideIcon::Shield, self.i18n.t("modals.export.content_summary_title"))),
            content,
            cx,
        )
    }

    fn render_oxide_security_notice(
        &self,
        dialog: &OxideExportDialogState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let yes_label = self.i18n.t("common.yes");
        let no_label = self.i18n.t("common.no");
        let app_settings_label = if dialog.include_app_settings {
            yes_label.as_str()
        } else {
            no_label.as_str()
        };
        let plugin_settings_label =
            if dialog.include_plugin_settings && oxide_export_selected_plugin_setting_count(dialog) > 0
            {
                yes_label.as_str()
            } else {
                no_label.as_str()
            };
        let portable_secrets_label = if dialog.include_portable_secrets {
            yes_label.as_str()
        } else {
            no_label.as_str()
        };
        self.render_oxide_tone_notice(
            OXIDE_BLUE_500,
            self.i18n.t("modals.export.security_notice"),
            vec![
                self.i18n.t("modals.export.security_encryption"),
                self.i18n.t("modals.export.security_kdf"),
                self.i18n.t("modals.export.security_contains"),
                self.i18n
                    .t("modals.export.security_settings")
                    .replace("{{app}}", app_settings_label)
                    .replace("{{plugin}}", plugin_settings_label),
                self.i18n
                    .t("modals.export.security_portable_secrets")
                    .replace("{{portable}}", portable_secrets_label),
                if dialog.include_passwords {
                    self.i18n.t("modals.export.security_passwords_included")
                } else {
                    self.i18n.t("modals.export.security_passwords_excluded")
                },
                self.i18n.t("modals.export.security_no_session"),
                self.i18n.t("modals.export.security_keep_safe"),
            ],
            cx,
        )
    }


    fn render_oxide_export_password_input(
        &self,
        dialog: &OxideExportDialogState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "oxide-export-password",
                        "label",
                        "加密密码 *",
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .child(self.render_session_password_input(
                SessionManagerInput::OxideExportPassword,
                &dialog.password,
                "至少 6 位，推荐 12 位以上并混合大小写字母、数字和符号".to_string(),
                cx,
            ))
            .when(!dialog.password.is_empty(), |input| {
                input.child(
                    div()
                        .mt(px(4.0))
                        .child(self.render_oxide_password_strength(&dialog.password, cx)),
                )
            })
            .into_any_element()
    }

    fn render_oxide_compact_warning(
        &self,
        color: u32,
        title: String,
        lines: Vec<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let line_list = if lines.is_empty() {
            None
        } else {
            self.sync_oxide_export_summary_line_list_state(&lines);
            let state = self.oxide_export_summary_line_list_state.clone();
            let spec = self.oxide_export_summary_line_list_spec();
            let workspace = cx.entity();
            let line_color = color;
            let item_count = lines.len();
            let virtual_lines = lines;
            Some(
                div()
                    .id("oxide-export-summary-lines")
                    .h(px(
                        (item_count as f32 * OXIDE_EXPORT_SUMMARY_LINE_LIST_ESTIMATED_HEIGHT)
                            .min(64.0),
                    ))
                    .selectable_overflow_y_scrollbar(
                        &self.selectable_text_scroll_handle("oxide-export-summary-lines"),
                    )
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            let Some(line) = virtual_lines.get(index).cloned() else {
                                return div().into_any_element();
                            };
                            workspace.update(cx, |this, cx| {
                                this.render_oxide_export_summary_line_item(
                                    index, line, line_color, cx,
                                )
                            })
                        },
                    ))
                    .into_any_element(),
            )
        };
        div()
            .px(px(8.0))
            .py(px(6.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((color << 8) | OXIDE_TONE_BORDER_ALPHA))
            .bg(rgba((color << 8) | OXIDE_TONE_BG_ALPHA))
            .text_color(rgb(color))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(Self::render_lucide_icon(
                        LucideIcon::AlertTriangle,
                        12.0,
                        rgb(color),
                    ))
                    .child(self.render_selectable_text_scoped(
                        "oxide-export-compact-warning-title",
                        title.clone(),
                        title,
                        color,
                        cx,
                    )),
            )
            .when_some(line_list, |notice, line_list| notice.child(line_list))
            .into_any_element()
    }

    fn sync_oxide_export_summary_line_list_state(&self, lines: &[String]) {
        let signatures = lines
            .iter()
            .map(|line| oxide_export_summary_line_signature(line))
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.oxide_export_summary_line_list_state,
            &mut self.oxide_export_summary_line_list_cache.borrow_mut(),
            "oxide-export-summary-lines",
            &signatures,
            self.oxide_export_summary_line_list_spec(),
        );
    }

    fn oxide_export_summary_line_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(OXIDE_EXPORT_SUMMARY_LINE_LIST_ESTIMATED_HEIGHT),
            OXIDE_EXPORT_SUMMARY_LINE_LIST_OVERSCAN,
        )
    }

    fn render_oxide_export_summary_line_item(
        &self,
        index: usize,
        line: String,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .opacity(0.8)
            .line_height(px(16.0))
            .child(self.render_selectable_text_scoped(
                "oxide-export-compact-warning-line",
                index,
                format!("• {line}"),
                color,
                cx,
            ))
            .into_any_element()
    }


    fn render_oxide_export_footer(
        &self,
        dialog: &OxideExportDialogState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let primary_label = dialog
            .progress_stage
            .as_ref()
            .filter(|_| dialog.busy)
            .map(|progress| oxide_export_progress_label(&progress.stage, dialog.embed_keys, &self.i18n))
            .unwrap_or_else(|| self.i18n.t("modals.export.export"));
        self.render_oxide_footer(
            dialog.busy,
            !oxide_export_has_selected_content(dialog),
            String::new(),
            primary_label,
            dialog.focused_footer_action,
            |_this, _event, _window, cx| {
                cx.stop_propagation();
            },
            |this, _event, _window, cx| {
                this.export_oxide_dialog(cx);
                cx.stop_propagation();
            },
            |this, _event, _window, cx| {
                this.session_manager.oxide_export_dialog = None;
                this.session_manager.focused_input = None;
                cx.notify();
                cx.stop_propagation();
            },
            cx,
        )
    }
}
