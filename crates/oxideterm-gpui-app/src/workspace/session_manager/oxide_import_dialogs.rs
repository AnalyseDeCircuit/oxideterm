impl WorkspaceApp {
    fn render_oxide_import_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(dialog) = self.session_manager.oxide_import_dialog.as_ref() else {
            return div().into_any_element();
        };
        let preview = dialog.preview.clone();
        let has_result = dialog.result.is_some();
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    // Tauri OxideImportModal wires Dialog onOpenChange through
                    // handleClose, which clears the dialog state on backdrop click.
                    this.session_manager.oxide_import_dialog = None;
                    this.session_manager.focused_input = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .w(px(OXIDE_MODAL_WIDTH))
                    .max_h(relative(OXIDE_MODAL_MAX_HEIGHT_RATIO))
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px(px(OXIDE_MODAL_HEADER_PX))
                            .py(px(OXIDE_MODAL_HEADER_PY))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "oxide-import-dialog",
                                        "title",
                                        "从 .oxide 文件导入配置",
                                        theme.text_heading,
                                        cx,
                                    )),
                            )
                            .child(self.render_oxide_close_button(true, cx)),
                    )
                    .child(
                        div()
                            .id("oxide-import-dialog-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .selectable_overflow_y_scroll(
                                &self.selectable_text_scroll_handle("oxide-import-dialog-scroll"),
                            )
                            .p(px(OXIDE_MODAL_BODY_P))
                            .flex()
                            .flex_col()
                            .gap(px(OXIDE_MODAL_SECTION_GAP))
                            .when(dialog.file_data.is_none(), |body| {
                                body.child(
                                    div()
                                        .py(px(32.0))
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .gap(px(24.0))
                                        .child(
                                            // Tauri OxideImportModal uses the
                                            // default accent Button for first
                                            // file selection; keep the shape on
                                            // the shared button primitive.
                                            self.workspace_toolbar_action_button(
                                                "选择 .oxide 文件".to_string(),
                                                None,
                                                ToolbarButtonOptions {
                                                    button: ButtonOptions {
                                                        variant: ButtonVariant::Default,
                                                        size: ButtonSize::Default,
                                                        radius: ButtonRadius::Md,
                                                        disabled: dialog.busy,
                                                    },
                                                    ..ToolbarButtonOptions::default()
                                                },
                                                cx.listener(|this, _event, _window, cx| {
                                                    this.select_oxide_import_file(cx);
                                                    cx.stop_propagation();
                                                }),
                                            ),
                                        )
                                        .child(self.render_oxide_tone_notice(
                                            OXIDE_BLUE_500,
                                            "导入说明".to_string(),
                                            vec![
                                                "选择 OxideTerm 导出的 .oxide 文件".to_string(),
                                                "输入导出时设置的加密密码".to_string(),
                                                "解密并预览后，可以选择要导入的连接、应用设置分组、插件偏好和端口转发".to_string(),
                                                "文件中包含的密钥口令会安全存入系统钥匙串；已保存的服务器密码不会出现在文件中".to_string(),
                                            ],
                                            cx,
                                        )),
                                )
                            })
                            .when_some(dialog.progress_stage.clone().filter(|_| !has_result), |body, progress| {
                                body.child(self.render_oxide_progress(progress, None, cx))
                            })
                            .when_some(dialog.metadata.clone().filter(|_| !has_result), |body, metadata| {
                                body.child(self.render_oxide_import_file_info(metadata, cx))
                            })
                            .when(dialog.file_data.is_some() && !has_result, |body| {
                                body.child(self.render_oxide_labeled_input(
                                    "解密密码".to_string(),
                                    self.render_session_password_input(
                                        SessionManagerInput::OxideImportPassword,
                                        &dialog.password,
                                        "输入导出时设置的密码".to_string(),
                                        cx,
                                    ),
                                    cx,
                                ))
                                .child(self.render_oxide_conflict_strategy(cx))
                            })
                            .when(dialog.file_data.is_some() && dialog.preview.is_none() && !has_result, |body| {
                                body.child(self.render_oxide_import_warning(cx))
                            })
                            .when_some(preview.clone().filter(|_| !has_result), |body, preview| {
                                body.child(self.render_oxide_import_preview(preview, cx))
                            })
                            .when_some(dialog.result.clone(), |body, result| {
                                body.child(self.render_oxide_import_result_summary(
                                    result,
                                    preview.clone(),
                                    cx,
                                ))
                            })
                            .when_some(dialog.error.clone().filter(|_| !has_result), |body, error| {
                                body.child(self.render_oxide_error_banner(error, cx))
                            })
                            .when(dialog.file_data.is_some() && !has_result, |body| {
                                body.child(self.render_oxide_import_footer(dialog, cx))
                            })
                            .when(dialog.file_data.is_some() && has_result, |body| {
                                body.child(self.render_oxide_import_result_footer(cx))
                            }),
                    )
            )
            .into_any_element()
    }


    fn render_oxide_import_file_info(
        &self,
        metadata: OxideMetadata,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = vec![
            (
                "导出时间:".to_string(),
                metadata
                    .exported_at
                    .with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
            ),
            ("导出者:".to_string(), metadata.exported_by),
            (
                "包含:".to_string(),
                format!("{} 个连接", metadata.num_connections),
            ),
        ];
        if let Some(description) = metadata.description.filter(|value| !value.trim().is_empty()) {
            rows.insert(2, ("描述:".to_string(), description));
        }
        if metadata.has_app_settings.unwrap_or(false) {
            rows.push((
                "应用设置:".to_string(),
                "应用设置: 预览后可按分组选择导入".to_string(),
            ));
        }
        if metadata.has_quick_commands.unwrap_or(false) {
            rows.push((
                "快捷命令:".to_string(),
                format!("{} 条命令", metadata.quick_commands_count.unwrap_or(0)),
            ));
        }
        if let Some(count) = metadata.plugin_settings_count.filter(|count| *count > 0) {
            rows.push(("插件偏好设置:".to_string(), format!("{count} 项")));
        }
        if let Some(count) = metadata.portable_secret_count.filter(|count| *count > 0) {
            rows.push(("便携秘密项:".to_string(), format!("{count} 项")));
        }

        let mut children = vec![
            div()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(self.tokens.ui.text))
                .child(self.render_selectable_text_scoped(
                    "oxide-import-file-info-heading",
                    (),
                    "文件信息",
                    self.tokens.ui.text,
                    cx,
                ))
                .into_any_element(),
        ];
        children.extend(rows.into_iter().enumerate().map(|(index, (label, value))| {
            div()
                .flex()
                .items_baseline()
                .gap(px(4.0))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(self.tokens.ui.text))
                .child(
                    div()
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.render_selectable_text_scoped(
                            "oxide-import-file-info-label",
                            index,
                            label,
                            self.tokens.ui.text_muted,
                            cx,
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(self.render_selectable_text_scoped(
                            "oxide-import-file-info-value",
                            index,
                            value,
                            self.tokens.ui.text,
                            cx,
                        )),
                )
                .into_any_element()
        }));
        children.push(
            div()
                .mt(px(4.0))
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .line_height(px(16.0))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.render_selectable_text_scoped(
                    "oxide-import-file-info-note",
                    (),
                    "预览后可按连接、应用设置分组、插件偏好和端口转发进行部分导入",
                    self.tokens.ui.text_muted,
                    cx,
                ))
                .into_any_element(),
        );
        if !metadata.connection_names.is_empty() {
            let mut list = div()
                .id("oxide-import-connections-preview")
                .mt(px(4.0))
                .max_h(px(128.0))
                .selectable_overflow_y_scrollbar(
                    &self.selectable_text_scroll_handle("oxide-import-connections-preview"),
                )
                .flex()
                .flex_col()
                .gap(px(4.0));
            for (index, name) in metadata.connection_names.into_iter().enumerate() {
                list = list.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.render_selectable_text_scoped(
                            "oxide-import-file-info-connection-name",
                            index,
                            format!("• {name}"),
                            self.tokens.ui.text_muted,
                            cx,
                        )),
                );
            }
            children.push(
                div()
                    .mt(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.render_selectable_text_scoped(
                        "oxide-import-file-info-connections-heading",
                        (),
                        "连接列表:",
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            );
            children.push(list.into_any_element());
        }

        self.render_oxide_padded_card(16.0, None, children, cx)
    }

    fn render_oxide_conflict_strategy(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .session_manager
            .oxide_import_dialog
            .as_ref()
            .map(|dialog| dialog.conflict_strategy)
            .unwrap_or(ImportConflictStrategy::Rename);
        let strategies = [
            (ImportConflictStrategy::Rename, "冲突时重命名"),
            (ImportConflictStrategy::Skip, "跳过冲突项"),
            (ImportConflictStrategy::Replace, "替换现有项"),
            (ImportConflictStrategy::Merge, "合并到现有项"),
        ];
        let mut row = div().grid().grid_cols(2).gap(px(8.0));
        for (strategy, label) in strategies {
            let selected = current == strategy;
            let strategy_key = match strategy {
                ImportConflictStrategy::Rename => "rename",
                ImportConflictStrategy::Skip => "skip",
                ImportConflictStrategy::Replace => "replace",
                ImportConflictStrategy::Merge => "merge",
            };
            row = row.child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(if selected {
                        self.tokens.ui.accent
                    } else {
                        self.tokens.ui.border
                    }))
                    .bg(if selected {
                        rgba((self.tokens.ui.accent << 8) | OXIDE_TONE_BG_ALPHA)
                    } else {
                        rgb(self.tokens.ui.bg)
                    })
                    .px_3()
                    .py_2()
                    .cursor_pointer()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(if selected {
                                self.tokens.ui.text
                            } else {
                                self.tokens.ui.text_muted
                            }))
                            // Source tabs are selectable rows, so the visible label follows Tauri select-none behavior.
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "oxide-import-source-tab",
                                strategy_key,
                                label,
                                if selected {
                                    self.tokens.ui.text
                                } else {
                                    self.tokens.ui.text_muted
                                },
                                cx,
                            )),
                    )
                    .hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(dialog) =
                                this.session_manager.oxide_import_dialog.as_mut()
                            {
                                dialog.conflict_strategy = strategy;
                                dialog.preview = None;
                            }
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "oxide-import-conflict",
                        "title",
                        "冲突处理策略",
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .child(row)
            .into_any_element()
    }


    fn render_oxide_import_warning(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .px_3()
            .py_2()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((OXIDE_YELLOW_500 << 8) | OXIDE_TONE_BORDER_ALPHA))
            .bg(rgba((OXIDE_YELLOW_500 << 8) | OXIDE_TONE_BG_ALPHA))
            .text_color(rgb(OXIDE_YELLOW_500))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "oxide-import-warning",
                        "title",
                        "⚠️ 注意",
                        OXIDE_YELLOW_500,
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(16.0))
                    .opacity(0.9)
                    .child(self.render_display_text_with_role_and_alpha(
                        SelectableTextRole::PlainDocument,
                        "oxide-import-warning",
                        "conflict",
                        "导入会一次处理所有已选连接。名称冲突的处理方式取决于你在下方选择的冲突策略。",
                        OXIDE_YELLOW_500,
                        0.9,
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(16.0))
                    .opacity(0.9)
                    .child(self.render_display_text_with_role_and_alpha(
                        SelectableTextRole::PlainDocument,
                        "oxide-import-warning",
                        "passwords",
                        ".oxide 文件从不包含已保存的服务器密码。使用密码认证的连接导入后，后续可能需要你重新输入密码。",
                        OXIDE_YELLOW_500,
                        0.9,
                        cx,
                    )),
            )
            .into_any_element()
    }


}
