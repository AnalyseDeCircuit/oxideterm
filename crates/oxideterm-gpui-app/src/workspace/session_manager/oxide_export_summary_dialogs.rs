impl WorkspaceApp {
    fn render_oxide_export_preflight(
        &self,
        preflight: Option<ExportPreflightResult>,
        show_card: bool,
        embed_keys: bool,
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
                .child("导出概览"),
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
                        .child(label)
                }))
                .into_any_element(),
        ];
        if preflight.portable_secret_count > 0 {
            card_children.push(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(format!("将打包 {} 项便携秘密项。", preflight.portable_secret_count))
                    .into_any_element(),
            );
        }
        if preflight.connections_with_passwords > 0 {
            card_children.push(self.render_oxide_compact_warning(
                OXIDE_YELLOW_500,
                format!(
                    "{} 个密码认证连接会导出配置，但不会包含已保存的服务器密码。",
                    preflight.connections_with_passwords
                ),
                Vec::new(),
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
            ));
        }
        if preflight.total_key_bytes > 0 {
            card_children.push(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(format!(
                        "密钥数据总计：{}",
                        oxide_format_bytes(preflight.total_key_bytes)
                    ))
                    .into_any_element(),
            );
        }

        section = section.child(self.render_oxide_card(None, card_children));
        section.into_any_element()
    }

    fn render_oxide_export_content_summary(&self, dialog: &OxideExportDialogState) -> AnyElement {
        let mut items = Vec::new();
        let connection_count = oxide_export_connection_count(dialog);
        if connection_count > 0 {
            items.push(format!("{connection_count} 个连接"));
        }
        if dialog.include_forwards && !dialog.selected_forward_ids.is_empty() {
            items.push(format!("{} 个已保存的转发", dialog.selected_forward_ids.len()));
        }
        if dialog.include_app_settings && !dialog.selected_app_settings_sections.is_empty() {
            let labels = OXIDE_APP_SETTINGS_SECTIONS
                .iter()
                .filter(|section| dialog.selected_app_settings_sections.contains(**section))
                .map(|section| oxide_settings_section_label(section))
                .collect::<Vec<_>>()
                .join(", ");
            items.push(format!("应用设置: {labels}"));
        }
        let selected_plugin_setting_count = oxide_export_selected_plugin_setting_count(dialog);
        if dialog.include_plugin_settings && selected_plugin_setting_count > 0 {
            items.push(format!(
                "{} 个插件，{} 项设置",
                dialog.selected_plugin_ids.len(),
                selected_plugin_setting_count
            ));
        }
        if dialog.include_portable_secrets {
            items.push(format!(
                "便携秘密项：{} 项",
                dialog
                    .preflight
                    .as_ref()
                    .map(|preflight| preflight.portable_secret_count)
                    .unwrap_or(0)
            ));
        }
        if dialog.embed_keys {
            items.push("SSH 私钥将被嵌入".to_string());
        }
        let content = if items.is_empty() {
            vec![
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child("尚未选择导出内容")
                    .into_any_element(),
            ]
        } else {
            items
                .into_iter()
                .map(|item| {
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(format!("• {item}"))
                        .into_any_element()
                })
                .collect()
        };
        self.render_oxide_card(Some((LucideIcon::Shield, "所选内容".to_string())), content)
    }

    fn render_oxide_security_notice(&self, dialog: &OxideExportDialogState) -> AnyElement {
        self.render_oxide_tone_notice(
            OXIDE_BLUE_500,
            "🔒 安全提示".to_string(),
            vec![
                "文件使用 ChaCha20-Poly1305 加密，军事级安全".to_string(),
                "密码使用 Argon2id 派生 (256MB, 4 轮)".to_string(),
                "文件包含所选连接、转发规则和密钥口令".to_string(),
                format!(
                    "包含全局设置：{}；包含插件偏好：{}",
                    if dialog.include_app_settings { "是" } else { "否" },
                    if dialog.include_plugin_settings
                        && oxide_export_selected_plugin_setting_count(dialog) > 0
                    {
                        "是"
                    } else {
                        "否"
                    }
                ),
                format!(
                    "包含便携秘密项：{}",
                    if dialog.include_portable_secrets { "是" } else { "否" }
                ),
                "保存的服务器密码不会写入 .oxide 文件".to_string(),
                "会话数据不会包含——仅连接配置".to_string(),
                "请妥善保管加密密码，丢失无法恢复".to_string(),
            ],
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
                    .child("加密密码 *"),
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
                        .child(self.render_oxide_password_strength(&dialog.password)),
                )
            })
            .into_any_element()
    }

    fn render_oxide_compact_warning(
        &self,
        color: u32,
        title: String,
        lines: Vec<String>,
    ) -> AnyElement {
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
                    .child(title),
            )
            .when(!lines.is_empty(), |notice| {
                notice.child(
                    div()
                        .max_h(px(64.0))
                        .overflow_y_scrollbar()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .children(lines.into_iter().map(|line| {
                            div()
                                .opacity(0.8)
                                .line_height(px(16.0))
                                .child(format!("• {line}"))
                        })),
                )
            })
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
            .map(|progress| oxide_export_progress_label(&progress.stage, dialog.embed_keys))
            .unwrap_or_else(|| "导出".to_string());
        self.render_oxide_footer(
            dialog.busy,
            !oxide_export_has_selected_content(dialog),
            String::new(),
            primary_label,
            cx.listener(|_this, _event, _window, cx| {
                cx.stop_propagation();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.export_oxide_dialog(cx);
                cx.stop_propagation();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.session_manager.oxide_export_dialog = None;
                this.session_manager.focused_input = None;
                cx.notify();
                cx.stop_propagation();
            }),
        )
    }
}
