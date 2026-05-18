impl WorkspaceApp {
    fn render_oxide_import_result_summary(
        &self,
        result: OxideImportResultView,
        preview: Option<ImportPreview>,
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
                    .child(format!("✓ 导入成功: {} 个连接", result.imported)),
            );
        if result.skipped > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "跳过: {}",
                result.skipped
            )));
        }
        if result.merged > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "已合并: {}",
                result.merged
            )));
        }
        if result.imported_app_settings {
            card = card.child(self.render_oxide_import_result_line(
                "已恢复全局 OxideTerm 设置。".to_string(),
            ));
        }
        if skipped_app_settings {
            card = card.child(self.render_oxide_import_result_line("已跳过应用设置".to_string()));
        }
        if result.imported_quick_commands > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "已导入 {} 项快捷命令。",
                result.imported_quick_commands
            )));
        }
        if result.skipped_quick_commands {
            card = card.child(self.render_oxide_import_result_line(
                "已跳过快捷命令".to_string(),
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
                        .child(format!("• {error}")),
                );
            }
            card = card.child(quick_errors);
        }
        if result.imported_plugin_settings > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "已恢复 {} 项插件偏好设置。",
                result.imported_plugin_settings
            )));
        }
        if skipped_plugin_settings {
            card = card.child(self.render_oxide_import_result_line(
                "已跳过插件偏好设置".to_string(),
            ));
        }
        if result.imported_portable_secrets > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "已恢复 {} 项便携秘密项。",
                result.imported_portable_secrets
            )));
        }
        if skipped_portable_secrets {
            card = card.child(self.render_oxide_import_result_line(
                "已跳过便携秘密项".to_string(),
            ));
        }
        if skipped_forwards {
            card = card.child(self.render_oxide_import_result_line(
                "已跳过端口转发".to_string(),
            ));
        }
        if result.replaced > 0 {
            card = card.child(self.render_oxide_import_result_line(format!(
                "已替换: {}",
                result.replaced
            )));
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
                        .child(format!("⚠️ 因冲突被重命名: {}", result.renamed)),
                );
            for (original, renamed_name) in &result.renames {
                renamed = renamed.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .line_height(px(16.0))
                        .opacity(0.9)
                        .child(format!("• \"{original}\" → \"{renamed_name}\"")),
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
                        .child("错误:"),
                );
            for error in &result.errors {
                error_block = error_block.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .line_height(px(16.0))
                        .opacity(0.9)
                        .child(format!("• {error}")),
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
                        .child("窗口将在 2 秒后自动关闭..."),
                )
            })
            .into_any_element()
    }

    fn render_oxide_import_result_line(&self, text: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(20.0))
            .child(text)
            .into_any_element()
    }

    fn render_oxide_import_result_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .pt(px(8.0))
            .child(
                button_with(
                    &self.tokens,
                    "关闭".to_string(),
                    ButtonOptions {
                        variant: ButtonVariant::Default,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.oxide_import_dialog = None;
                        this.session_manager.focused_input = None;
                        cx.notify();
                        cx.stop_propagation();
                    }),
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
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .pt(px(8.0))
                .child(
                    button_with(
                        &self.tokens,
                        "返回".to_string(),
                        ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: dialog.busy,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut()
                            {
                                dialog.preview = None;
                                dialog.result_summary = None;
                            }
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ),
                )
                .child(
                    button_with(
                        &self.tokens,
                        if dialog.busy {
                            "导入中...".to_string()
                        } else {
                            "确认导入".to_string()
                        },
                        ButtonOptions {
                            variant: ButtonVariant::Default,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: dialog.busy || !oxide_import_has_selected_content(dialog),
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.apply_oxide_import_dialog(cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(8.0))
                .pt(px(8.0))
                .child(
                    button_with(
                        &self.tokens,
                        "重新选择文件".to_string(),
                        ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: dialog.busy,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.select_oxide_import_file(cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
                .child(
                    button_with(
                        &self.tokens,
                        "取消".to_string(),
                        ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: dialog.busy,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.session_manager.oxide_import_dialog = None;
                            this.session_manager.focused_input = None;
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ),
                )
                .child(
                    button_with(
                        &self.tokens,
                        if dialog.busy {
                            "加载中...".to_string()
                        } else {
                            "预览".to_string()
                        },
                        ButtonOptions {
                            variant: ButtonVariant::Default,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: dialog.busy || dialog.password.is_empty(),
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.preview_oxide_import_dialog(cx);
                            cx.stop_propagation();
                        }),
                    ),
                )
                .into_any_element()
        }
    }

}
