use super::*;

struct OxideExportPreflightRenderSnapshot {
    connections_with_passwords: usize,
    connections_with_keys: usize,
    connections_with_agent: usize,
    portable_secret_count: usize,
    key_passphrase_count: usize,
    managed_key_count: usize,
    managed_key_passphrase_count: usize,
    can_export: bool,
    blocked_managed_key_connection_count: usize,
    missing_key_lines: Vec<String>,
    total_key_bytes: u64,
}

#[derive(Clone)]
struct OxideExportSummaryLineRenderer {
    // Warning lines are immutable for this frame and shared across visible callbacks.
    session_manager: Entity<SessionManagerState>,
    tokens: ThemeTokens,
    color: u32,
    lines: Arc<[String]>,
}

impl OxideExportSummaryLineRenderer {
    fn render(&self, index: usize, cx: &App) -> AnyElement {
        if self.session_manager.read(cx).oxide_export_dialog.is_none() {
            return div().into_any_element();
        }
        self.lines
            .get(index)
            .map(|line| {
                div()
                    .opacity(0.8)
                    .line_height(px(16.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.color))
                    .child(format!("• {line}"))
                    .into_any_element()
            })
            .unwrap_or_else(|| div().into_any_element())
    }
}

pub(super) fn oxide_export_summary_line_signature(line: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Warning lines are visible verbatim in the compact preflight body.
    line.hash(&mut hasher);
    hasher.finish()
}

impl WorkspaceApp {
    pub(super) fn render_oxide_export_preflight_stat(
        &self,
        icon: LucideIcon,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .min_w(px(160.0))
            .flex_1()
            .px(px(8.0))
            .py(px(6.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            .bg(self.render_oxide_subcard_bg(false))
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(Self::render_lucide_icon(
                icon,
                12.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "oxide-export-preflight-stat",
                        label.clone(),
                        label,
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_oxide_export_preflight_stats(
        &self,
        stats: Vec<(LucideIcon, String)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_wrap()
            .gap(px(8.0))
            .children(
                stats
                    .into_iter()
                    .map(|(icon, label)| self.render_oxide_export_preflight_stat(icon, label, cx)),
            )
            .into_any_element()
    }

    pub(super) fn render_oxide_export_preflight(
        &self,
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
                .child(self.render_display_text_with_role(
                    SelectableTextRole::NonSelectable,
                    "oxide-export-preflight-heading",
                    (),
                    self.i18n.t("export.summary_title"),
                    theme.text,
                    cx,
                )),
        );
        let preflight = self
            .session_manager
            .read(cx)
            .oxide_export_dialog
            .as_ref()
            .and_then(|dialog| dialog.preflight.as_ref())
            .filter(|_| show_card)
            .map(|preflight| OxideExportPreflightRenderSnapshot {
                connections_with_passwords: preflight.connections_with_passwords,
                connections_with_keys: preflight.connections_with_keys,
                connections_with_agent: preflight.connections_with_agent,
                portable_secret_count: preflight.portable_secret_count,
                key_passphrase_count: preflight.key_passphrase_count,
                managed_key_count: preflight.managed_key_count,
                managed_key_passphrase_count: preflight.managed_key_passphrase_count,
                can_export: preflight.can_export,
                blocked_managed_key_connection_count: preflight
                    .blocked_managed_key_connections
                    .len(),
                missing_key_lines: preflight
                    .missing_keys
                    .iter()
                    .map(|(name, path)| format!("{name}: {path}"))
                    .collect(),
                total_key_bytes: preflight.total_key_bytes,
            });
        let Some(preflight) = preflight else {
            return section.into_any_element();
        };
        let mut card_children = vec![self.render_oxide_export_preflight_stats(
            vec![
                (
                    LucideIcon::Lock,
                    self.i18n
                        .t("export.summary_passwords")
                        .replace("{{count}}", &preflight.connections_with_passwords.to_string()),
                ),
                (
                    LucideIcon::Key,
                    self.i18n
                        .t("export.summary_keys")
                        .replace("{{count}}", &preflight.connections_with_keys.to_string()),
                ),
                (
                    LucideIcon::FileLock,
                    self.i18n
                        .t("export.summary_agent")
                        .replace("{{count}}", &preflight.connections_with_agent.to_string()),
                ),
            ],
            cx,
        )];
        if preflight.portable_secret_count > 0 {
            let label = self
                .i18n
                .t("export.summary_portable_secrets")
                .replace("{{count}}", &preflight.portable_secret_count.to_string());
            card_children.push(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "oxide-export-preflight-portable-secret",
                        (),
                        label,
                        theme.text_muted,
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        card_children.push(self.render_oxide_export_preflight_stats(
            vec![
                    (
                        LucideIcon::Key,
                        self.i18n
                            .t("export.summary_key_passphrases")
                            .replace(
                                "{{count}}",
                                &preflight.key_passphrase_count.to_string(),
                            ),
                    ),
                    (
                        LucideIcon::Key,
                        self.i18n.t("export.summary_managed_keys").replace(
                            "{{count}}",
                            &preflight.managed_key_count.to_string(),
                        ),
                    ),
                    (
                        LucideIcon::FileLock,
                        self.i18n
                            .t("export.summary_managed_key_passphrases")
                            .replace(
                                "{{count}}",
                                &preflight.managed_key_passphrase_count.to_string(),
                            ),
                    ),
                ],
            cx,
        ));
        if !preflight.can_export {
            card_children.push(self.render_oxide_compact_warning(
                OXIDE_RED_500,
                self.i18n.t("export.warning_managed_keys_required").replace(
                    "{{count}}",
                    &preflight.blocked_managed_key_connection_count.to_string(),
                ),
                Vec::new(),
                cx,
            ));
        }
        if preflight.connections_with_passwords > 0 {
            let password_warning = if include_passwords {
                self.i18n.t("export.warning_passwords_included").replace(
                    "{{count}}",
                    &preflight.connections_with_passwords.to_string(),
                )
            } else {
                self.i18n.t("export.warning_passwords_excluded").replace(
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
        if embed_keys && !preflight.missing_key_lines.is_empty() {
            card_children.push(
                self.render_oxide_compact_warning(
                    OXIDE_YELLOW_500,
                    self.i18n
                        .t("export.warning_missing_keys")
                        .replace("{{count}}", &preflight.missing_key_lines.len().to_string()),
                    preflight.missing_key_lines,
                    cx,
                ),
            );
        }
        if preflight.total_key_bytes > 0 {
            let label = self
                .i18n
                .t("export.key_size")
                .replace("{{size}}", &oxide_format_bytes(preflight.total_key_bytes));
            card_children.push(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
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

    pub(super) fn render_oxide_export_content_summary(&self, cx: &mut Context<Self>) -> AnyElement {
        let items = {
            let manager = self.session_manager.read(cx);
            let Some(dialog) = manager.oxide_export_dialog.as_ref() else {
                return div().into_any_element();
            };
            let mut items = Vec::new();
            let connection_count = oxide_export_connection_count(dialog);
            if connection_count > 0 {
                items.push(
                    self.i18n
                        .t("export.content_summary_connections")
                        .replace("{{count}}", &connection_count.to_string()),
                );
            }
            if dialog.include_forwards && !dialog.selected_forward_ids.is_empty() {
                items.push(
                    self.i18n
                        .t("export.content_summary_forwards")
                        .replace("{{count}}", &dialog.selected_forward_ids.len().to_string()),
                );
            }
            if dialog.include_serial_profiles {
                items.push(
                    self.i18n
                        .t("export.content_summary_serial_profiles")
                        .replace(
                            "{{count}}",
                            &self.connection_store.serial_profiles().len().to_string(),
                        ),
                );
            }
            if dialog.include_telnet_profiles {
                items.push(
                    self.i18n
                        .t("export.content_summary_telnet_profiles")
                        .replace(
                            "{{count}}",
                            &self.connection_store.telnet_profiles().len().to_string(),
                        ),
                );
            }
            if dialog.include_mosh_profiles {
                items.push(self.i18n.t("export.content_summary_mosh_profiles").replace(
                    "{{count}}",
                    &self.connection_store.mosh_profiles().len().to_string(),
                ));
            }
            if dialog.include_remote_desktop_profiles {
                items.push(
                    self.i18n
                        .t("export.content_summary_remote_desktop_profiles")
                        .replace(
                            "{{count}}",
                            &self
                                .connection_store
                                .remote_desktop_profiles()
                                .len()
                                .to_string(),
                        ),
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
                    self.i18n.t("export.content_summary_app_settings")
                ));
            }
            let selected_plugin_setting_count = oxide_export_selected_plugin_setting_count(dialog);
            if dialog.include_plugin_settings && selected_plugin_setting_count > 0 {
                items.push(
                    self.i18n
                        .t("export.content_summary_plugin_settings")
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
                        .t("export.content_summary_portable_secrets")
                        .replace("{{count}}", &count.to_string()),
                );
            }
            if dialog.embed_keys {
                items.push(self.i18n.t("export.content_summary_embed_keys"));
            }
            if dialog.include_passwords {
                items.push(self.i18n.t("export.content_summary_passwords"));
            }
            if dialog.include_key_passphrases {
                items.push(self.i18n.t("export.content_summary_key_passphrases"));
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
                            .t("export.content_summary_managed_keys")
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
                            .t("export.content_summary_managed_key_passphrases")
                            .replace("{{count}}", &count.to_string()),
                    );
                }
            }
            if let Some(preflight) = dialog
                .preflight
                .as_ref()
                .filter(|preflight| !preflight.can_export)
            {
                items.push(self.i18n.t("export.warning_managed_keys_required").replace(
                    "{{count}}",
                    &preflight.blocked_managed_key_connections.len().to_string(),
                ));
            }
            items
        };
        let content = if items.is_empty() {
            vec![
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "oxide-export-content-summary-empty",
                        (),
                        self.i18n.t("export.app_settings_no_sections"),
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
            Some((
                LucideIcon::Shield,
                self.i18n.t("export.content_summary_title"),
            )),
            content,
            cx,
        )
    }

    pub(super) fn render_oxide_security_notice(&self, cx: &mut Context<Self>) -> AnyElement {
        let (
            include_app_settings,
            include_plugin_settings,
            selected_plugin_setting_count,
            include_portable_secrets,
            include_passwords,
        ) = {
            let manager = self.session_manager.read(cx);
            let Some(dialog) = manager.oxide_export_dialog.as_ref() else {
                return div().into_any_element();
            };
            (
                dialog.include_app_settings,
                dialog.include_plugin_settings,
                oxide_export_selected_plugin_setting_count(dialog),
                dialog.include_portable_secrets,
                dialog.include_passwords,
            )
        };
        let yes_label = self.i18n.t("common.yes");
        let no_label = self.i18n.t("common.no");
        let app_settings_label = if include_app_settings {
            yes_label.as_str()
        } else {
            no_label.as_str()
        };
        let plugin_settings_label = if include_plugin_settings && selected_plugin_setting_count > 0
        {
            yes_label.as_str()
        } else {
            no_label.as_str()
        };
        let portable_secrets_label = if include_portable_secrets {
            yes_label.as_str()
        } else {
            no_label.as_str()
        };
        self.render_oxide_tone_notice(
            OXIDE_BLUE_500,
            self.i18n.t("export.security_notice"),
            vec![
                self.i18n.t("export.security_encryption"),
                self.i18n.t("export.security_kdf"),
                self.i18n.t("export.security_contains"),
                self.i18n
                    .t("export.security_settings")
                    .replace("{{app}}", app_settings_label)
                    .replace("{{plugin}}", plugin_settings_label),
                self.i18n
                    .t("export.security_portable_secrets")
                    .replace("{{portable}}", portable_secrets_label),
                if include_passwords {
                    self.i18n.t("export.security_passwords_included")
                } else {
                    self.i18n.t("export.security_passwords_excluded")
                },
                self.i18n.t("export.security_no_session"),
                self.i18n.t("export.security_keep_safe"),
            ],
            cx,
        )
    }

    pub(super) fn render_oxide_export_password_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let password_strength = self
            .session_manager
            .read(cx)
            .oxide_export_dialog
            .as_ref()
            .and_then(|dialog| {
                (!dialog.password.is_empty()).then(|| oxide_password_strength(&dialog.password))
            });
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
                "至少 6 位，推荐 12 位以上并混合大小写字母、数字和符号".to_string(),
                cx,
            ))
            .when_some(password_strength, |input, strength| {
                input.child(
                    div()
                        .mt(px(4.0))
                        .child(self.render_oxide_password_strength(strength, cx)),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_oxide_compact_warning(
        &self,
        color: u32,
        title: String,
        lines: Vec<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let line_list = if lines.is_empty() {
            None
        } else {
            self.sync_oxide_export_summary_line_list_state(&lines, cx);
            let state = self
                .session_manager
                .read(cx)
                .oxide_export_summary_line_list_state
                .clone();
            let spec = self.oxide_export_summary_line_list_spec();
            let item_count = lines.len();
            let renderer = OxideExportSummaryLineRenderer {
                session_manager: self.session_manager.clone(),
                tokens: self.tokens,
                color,
                lines: lines.into(),
            };
            Some(
                div()
                    .id("oxide-export-summary-lines")
                    .h(px((item_count as f32
                        * OXIDE_EXPORT_SUMMARY_LINE_LIST_ESTIMATED_HEIGHT)
                        .min(64.0)))
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| renderer.render(index, cx),
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

    pub(super) fn sync_oxide_export_summary_line_list_state(&self, lines: &[String], cx: &App) {
        let signatures = lines
            .iter()
            .map(|line| oxide_export_summary_line_signature(line))
            .collect::<Vec<_>>();
        let manager = self.session_manager.read(cx);
        sync_tauri_variable_list_state_by_signatures(
            &manager.oxide_export_summary_line_list_state,
            &mut manager.oxide_export_summary_line_list_cache.borrow_mut(),
            "oxide-export-summary-lines",
            &signatures,
            self.oxide_export_summary_line_list_spec(),
        );
    }

    pub(super) fn oxide_export_summary_line_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(OXIDE_EXPORT_SUMMARY_LINE_LIST_ESTIMATED_HEIGHT),
            OXIDE_EXPORT_SUMMARY_LINE_LIST_OVERSCAN,
        )
    }

    pub(super) fn render_oxide_export_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some((busy, no_selected_content, focused_footer_action, embed_keys, progress_stage)) =
            ({
                self.session_manager
                    .read(cx)
                    .oxide_export_dialog
                    .as_ref()
                    .map(|dialog| {
                        (
                            dialog.busy,
                            !oxide_export_has_selected_content(dialog),
                            dialog.focused_footer_action,
                            dialog.embed_keys,
                            dialog.progress_stage.clone(),
                        )
                    })
            })
        else {
            return div().into_any_element();
        };
        let primary_label = progress_stage
            .as_ref()
            .filter(|_| busy)
            .map(|progress| oxide_export_progress_label(&progress.stage, embed_keys, &self.i18n))
            .unwrap_or_else(|| self.i18n.t("export.export"));
        self.render_oxide_footer(
            busy,
            no_selected_content,
            String::new(),
            primary_label,
            focused_footer_action,
            |_this, _event, _window, cx| {
                cx.stop_propagation();
            },
            |this, _event, _window, cx| {
                this.export_oxide_dialog(cx);
                cx.stop_propagation();
            },
            |this, _event, _window, cx| {
                this.session_manager.update(cx, |manager, cx| {
                    manager.oxide_export_dialog = None;
                    manager.focused_input = None;
                    cx.notify();
                });
                cx.stop_propagation();
            },
            cx,
        )
    }
}
