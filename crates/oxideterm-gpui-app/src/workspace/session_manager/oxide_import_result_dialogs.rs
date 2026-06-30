impl WorkspaceApp {
    fn render_oxide_import_result_summary(
        &self,
        result: OxideImportResultView,
        preview: Option<ImportPreview>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_error = !result.errors.is_empty();
        let tone = if has_error {
            OXIDE_YELLOW_500
        } else {
            OXIDE_GREEN_500
        };
        let skipped_app_settings = preview.as_ref().is_some_and(|preview| {
            preview.has_app_settings && result.skipped_app_settings
        });
        let skipped_plugin_settings = preview.as_ref().is_some_and(|preview| {
            preview.plugin_settings_count > 0 && result.skipped_plugin_settings
        });
        let skipped_serial_profiles = preview.as_ref().is_some_and(|preview| {
            preview.serial_profiles_count > 0 && result.skipped_serial_profiles > 0
        });
        let skipped_raw_tcp_profiles = preview.as_ref().is_some_and(|preview| {
            preview.raw_tcp_profiles_count > 0 && result.skipped_raw_tcp_profiles > 0
        });
        let skipped_raw_udp_profiles = preview.as_ref().is_some_and(|preview| {
            preview.raw_udp_profiles_count > 0 && result.skipped_raw_udp_profiles > 0
        });
        let skipped_portable_secrets = preview.as_ref().is_some_and(|preview| {
            preview.portable_secret_count > 0 && result.skipped_portable_secrets > 0
        });
        let skipped_forwards = preview.as_ref().is_some_and(|preview| {
            preview.total_forwards > 0 && result.skipped_forwards > 0
        });
        let mut card = div()
            .p_4()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((tone << 8) | OXIDE_TONE_BORDER_ALPHA))
            .bg(rgba((tone << 8) | OXIDE_TONE_BG_ALPHA))
            .text_color(rgb(tone))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(self.render_selectable_text_scoped(
                        "oxide-import-result-title",
                        result.imported,
                        format!("✓ 导入成功: {} 个连接", result.imported),
                        tone,
                        cx,
                    )),
            );
        if result.skipped > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "跳过: {}",
                result.skipped
            ), tone, cx));
        }
        if result.merged > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "已合并: {}",
                result.merged
            ), tone, cx));
        }
        if result.imported_app_settings {
            card = card.child(self.render_oxide_import_result_line(
                "已恢复全局 OxideTerm 设置。".to_string(),
                tone,
                cx,
            ));
        }
        if skipped_app_settings {
            card = card.child(self.render_oxide_import_result_line(
                "已跳过应用设置".to_string(),
                tone,
                cx,
            ));
        }
        if result.imported_quick_commands > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "已导入 {} 项快捷命令。",
                result.imported_quick_commands
            ), tone, cx));
        }
        if result.skipped_quick_commands {
            card = card.child(self.render_oxide_import_result_line(
                "已跳过快捷命令".to_string(),
                tone,
                cx,
            ));
        }
        if result.imported_serial_profiles > 0 {
            card = card.child(self.render_oxide_import_result_line(
                self.i18n
                    .t("modals.import.imported_serial_profiles")
                    .replace(
                        "{{count}}",
                        &result.imported_serial_profiles.to_string(),
                    ),
                tone,
                cx,
            ));
        }
        if skipped_serial_profiles {
            card = card.child(self.render_oxide_import_result_line(
                self.i18n.t("modals.import.skipped_serial_profiles"),
                tone,
                cx,
            ));
        }
        if result.imported_raw_tcp_profiles > 0 {
            card = card.child(self.render_oxide_import_result_line(
                self.i18n
                    .t("modals.import.imported_raw_tcp_profiles")
                    .replace(
                        "{{count}}",
                        &result.imported_raw_tcp_profiles.to_string(),
                    ),
                tone,
                cx,
            ));
        }
        if skipped_raw_tcp_profiles {
            card = card.child(self.render_oxide_import_result_line(
                self.i18n.t("modals.import.skipped_raw_tcp_profiles"),
                tone,
                cx,
            ));
        }
        if result.imported_raw_udp_profiles > 0 {
            card = card.child(self.render_oxide_import_result_line(
                self.i18n
                    .t("modals.import.imported_raw_udp_profiles")
                    .replace(
                        "{{count}}",
                        &result.imported_raw_udp_profiles.to_string(),
                    ),
                tone,
                cx,
            ));
        }
        if skipped_raw_udp_profiles {
            card = card.child(self.render_oxide_import_result_line(
                self.i18n.t("modals.import.skipped_raw_udp_profiles"),
                tone,
                cx,
            ));
        }
        if !result.quick_commands_errors.is_empty() {
            let mut quick_errors = div()
                .mt(px(4.0))
                .flex()
                .flex_col()
                .gap(px(4.0));
            for error in &result.quick_commands_errors {
                quick_errors = quick_errors.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .line_height(px(16.0))
                        .opacity(0.9)
                        .child(self.render_selectable_text_scoped(
                            "oxide-import-quick-error",
                            error,
                            format!("• {error}"),
                            tone,
                            cx,
                        )),
                );
            }
            card = card.child(quick_errors);
        }
        if result.imported_plugin_settings > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "已恢复 {} 项插件偏好设置。",
                result.imported_plugin_settings
            ), tone, cx));
        }
        if skipped_plugin_settings {
            card = card.child(self.render_oxide_import_result_line(
                "已跳过插件偏好设置".to_string(),
                tone,
                cx,
            ));
        }
        if result.imported_portable_secrets > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "已恢复 {} 项便携秘密项。",
                result.imported_portable_secrets
            ), tone, cx));
        }
        if skipped_portable_secrets {
            card = card.child(self.render_oxide_import_result_line(
                "已跳过便携秘密项".to_string(),
                tone,
                cx,
            ));
        }
        if skipped_forwards {
            card = card.child(self.render_oxide_import_result_line(
                "已跳过端口转发".to_string(),
                tone,
                cx,
            ));
        }
        if result.replaced > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "已替换: {}",
                result.replaced
            ), tone, cx));
        }
        if result.renamed > 0 {
            let mut renamed = div()
                .mt(px(8.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(OXIDE_YELLOW_500))
                        .child(self.render_selectable_text_scoped(
                            "oxide-import-renamed-title",
                            result.renamed,
                            format!("⚠️ 因冲突被重命名: {}", result.renamed),
                            OXIDE_YELLOW_500,
                            cx,
                        )),
                );
            for (original, renamed_name) in &result.renames {
                let line = format!("• \"{original}\" → \"{renamed_name}\"");
                renamed = renamed.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .line_height(px(16.0))
                        .opacity(0.9)
                        .child(self.render_selectable_text_scoped(
                            "oxide-import-rename-line",
                            (original, renamed_name),
                            line,
                            OXIDE_YELLOW_500,
                            cx,
                        )),
                );
            }
            card = card.child(renamed);
        }
        if !result.errors.is_empty() {
            let mut error_block = div()
                .mt(px(8.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(self.render_selectable_text_scoped(
                            "oxide-import-error-title",
                            (),
                            "错误:",
                            tone,
                            cx,
                        )),
                );
            for error in &result.errors {
                error_block = error_block.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .line_height(px(16.0))
                        .opacity(0.9)
                        .child(self.render_selectable_text_scoped(
                            "oxide-import-error-line",
                            error,
                            format!("• {error}"),
                            tone,
                            cx,
                        )),
                );
            }
            card = card.child(error_block);
        }
        div()
            .py_4()
            .flex()
            .flex_col()
            .child(card)
            .when(!has_error, |body| {
                body.child(
                    div()
                        .mt(px(16.0))
                        .text_align(gpui::TextAlign::Center)
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.render_selectable_text_scoped(
                            "oxide-import-autoclose",
                            (),
                            "窗口将在 2 秒后自动关闭...",
                            self.tokens.ui.text_muted,
                            cx,
                        )),
                )
            })
            .into_any_element()
    }

    fn render_oxide_import_result_line(
        &self,
        text: String,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(20.0))
            .child(self.render_selectable_text_scoped(
                "oxide-import-result-line",
                (),
                text,
                color,
                cx,
            ))
            .into_any_element()
    }

    fn render_oxide_import_result_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let focused_action = self
            .session_manager
            .oxide_import_dialog
            .as_ref()
            .and_then(|dialog| dialog.focused_footer_action);
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .pt(px(8.0))
            .child(
                self.render_oxide_footer_click_action(
                    "关闭".to_string(),
                    ButtonVariant::Default,
                    OxideDialogFooterAction::Primary,
                    focused_action,
                    false,
                    None,
                    |this, _event, _window, cx| {
                        this.session_manager.oxide_import_dialog = None;
                        this.session_manager.focused_input = None;
                        cx.notify();
                        cx.stop_propagation();
                    },
                    cx,
                ),
            )
            .into_any_element()
    }

    fn render_oxide_import_footer(
        &self,
        dialog: &OxideImportDialogState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if dialog.preview.is_some() {
            let primary_disabled = dialog.busy || !oxide_import_has_selected_content(dialog);
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .pt(px(8.0))
                .child(
                    self.render_oxide_footer_click_action(
                        "返回".to_string(),
                        ButtonVariant::Outline,
                        OxideDialogFooterAction::Secondary,
                        dialog.focused_footer_action,
                        dialog.busy,
                        None,
                        |this, _event, _window, cx| {
                            if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut()
                            {
                                dialog.preview = None;
                                dialog.result_summary = None;
                            }
                            cx.notify();
                            cx.stop_propagation();
                        },
                        cx,
                    ),
                )
                .child(
                    self.render_oxide_footer_click_action(
                        if dialog.busy {
                            "导入中...".to_string()
                        } else {
                            "确认导入".to_string()
                        },
                        ButtonVariant::Default,
                        OxideDialogFooterAction::Primary,
                        dialog.focused_footer_action,
                        primary_disabled,
                        None,
                        |this, _event, _window, cx| {
                            this.apply_oxide_import_dialog(cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ),
                )
                .into_any_element()
        } else {
            let primary_disabled = dialog.busy || dialog.password.is_empty();
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .pt(px(8.0))
                .child(
                    self.render_oxide_footer_click_action(
                        "重新选择文件".to_string(),
                        ButtonVariant::Outline,
                        OxideDialogFooterAction::Secondary,
                        dialog.focused_footer_action,
                        dialog.busy,
                        None,
                        |this, _event, _window, cx| {
                            this.select_oxide_import_file(cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ),
                )
                .child(
                    self.render_oxide_footer_click_action(
                        "取消".to_string(),
                        ButtonVariant::Outline,
                        OxideDialogFooterAction::Cancel,
                        dialog.focused_footer_action,
                        dialog.busy,
                        None,
                        |this, _event, _window, cx| {
                            this.session_manager.oxide_import_dialog = None;
                            this.session_manager.focused_input = None;
                            cx.notify();
                            cx.stop_propagation();
                        },
                        cx,
                    ),
                )
                .child(
                    self.render_oxide_footer_click_action(
                        if dialog.busy {
                            "加载中...".to_string()
                        } else {
                            "预览".to_string()
                        },
                        ButtonVariant::Default,
                        OxideDialogFooterAction::Primary,
                        dialog.focused_footer_action,
                        primary_disabled,
                        None,
                        |this, _event, _window, cx| {
                            this.preview_oxide_import_dialog(cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ),
                )
                .into_any_element()
        }
    }

}
