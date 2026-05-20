impl WorkspaceApp {
    fn render_oxide_import_preview(
        &self,
        preview: ImportPreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected_names = self
            .session_manager
            .oxide_import_dialog
            .as_ref()
            .map(|dialog| dialog.selected_names.clone())
            .unwrap_or_default();
        let selectable_names = import_preview_selectable_names(&preview);
        let total_selectable = selectable_names.len();
        let all_selected = total_selectable > 0 && selected_names.len() == total_selectable;

        let mut children = vec![
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(Self::render_lucide_icon(
                            LucideIcon::CheckCircle,
                            20.0,
                            rgb(OXIDE_GREEN_500),
                        ))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(theme.text))
                                .child(self.render_selectable_text_scoped(
                                    "oxide-import-preview-heading",
                                    (),
                                    "导入预览",
                                    theme.text,
                                    cx,
                                )),
                        ),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.accent))
                        .cursor_pointer()
                        .child(if all_selected { "取消全选" } else { "全选" })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                if let Some(dialog) =
                                    this.session_manager.oxide_import_dialog.as_mut()
                                {
                                    if dialog.selected_names.len() == selectable_names.len() {
                                        dialog.selected_names.clear();
                                    } else {
                                        dialog.selected_names = selectable_names.clone();
                                    }
                                }
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        ),
                )
                .into_any_element(),
            div()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(theme.text))
                .child(self.render_selectable_text_scoped(
                    "oxide-import-preview-summary",
                    (),
                    format!(
                        "将导入 {} 个连接 — 已选 {} 个",
                        preview.total_connections,
                        selected_names.len()
                    ),
                    theme.text,
                    cx,
                ))
                .into_any_element(),
        ];

        children.extend(self.render_oxide_import_connection_groups(&preview, cx));
        if preview.has_app_settings {
            children.push(self.render_oxide_import_app_settings(&preview, cx));
        }
        if preview.has_quick_commands {
            children.push(self.render_oxide_import_quick_commands(&preview, cx));
        }
        if preview.plugin_settings_count > 0 {
            children.push(self.render_oxide_import_plugins(&preview, cx));
        }
        if preview.portable_secret_count > 0 {
            children.push(self.render_oxide_import_portable_secrets(&preview, cx));
        }
        if preview.total_forwards > 0 {
            children.push(self.render_oxide_import_forwards(&preview, cx));
        }
        if preview.has_embedded_keys {
            children.push(self.render_oxide_tone_notice(
                OXIDE_BLUE_500,
                "包含嵌入私钥".to_string(),
                vec!["私钥将被提取到 ~/.ssh/imported/ 目录".to_string()],
                cx,
            ));
        }

        self.render_oxide_padded_card(16.0, None, children)
    }

    fn render_oxide_import_connection_groups(
        &self,
        preview: &ImportPreview,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut groups = Vec::new();
        if !preview.unchanged.is_empty() {
            groups.push(self.render_oxide_import_name_group(
                format!("✓ {} 个连接将原样导入:", preview.unchanged.len()),
                OXIDE_GREEN_500,
                None,
                preview
                    .unchanged
                    .iter()
                    .map(|name| (name.clone(), name.clone()))
                    .collect(),
                cx,
            ));
        }
        if !preview.will_rename.is_empty() {
            groups.push(self.render_oxide_import_name_group(
                format!("{} 个连接因名称冲突将被重命名:", preview.will_rename.len()),
                OXIDE_YELLOW_500,
                Some(LucideIcon::AlertTriangle),
                preview
                    .will_rename
                    .iter()
                    .map(|(original, renamed)| (original.clone(), format!("\"{original}\" → \"{renamed}\"")))
                    .collect(),
                cx,
            ));
        }
        if !preview.will_merge.is_empty() {
            groups.push(self.render_oxide_import_name_group(
                format!("{} 个连接将合并到现有连接:", preview.will_merge.len()),
                OXIDE_BLUE_500,
                Some(LucideIcon::CheckCircle),
                preview
                    .will_merge
                    .iter()
                    .map(|name| (name.clone(), name.clone()))
                    .collect(),
                cx,
            ));
        }
        if !preview.will_replace.is_empty() {
            groups.push(self.render_oxide_import_name_group(
                format!("{} 个连接将替换现有连接:", preview.will_replace.len()),
                OXIDE_ORANGE_500,
                Some(LucideIcon::AlertTriangle),
                preview
                    .will_replace
                    .iter()
                    .map(|name| (name.clone(), name.clone()))
                    .collect(),
                cx,
            ));
        }
        if !preview.will_skip.is_empty() {
            groups.push(self.render_oxide_import_name_group(
                format!("{} 个连接将因冲突被跳过:", preview.will_skip.len()),
                OXIDE_SLATE_400,
                Some(LucideIcon::AlertTriangle),
                preview
                    .will_skip
                    .iter()
                    .map(|name| (name.clone(), name.clone()))
                    .collect(),
                cx,
            ));
        }
        groups
    }

    fn render_oxide_import_name_group(
        &self,
        title: String,
        color: u32,
        icon: Option<LucideIcon>,
        items: Vec<(String, String)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .max_h(px(96.0))
            .overflow_y_scrollbar();
        for (name, label) in items {
            let checked = self
                .session_manager
                .oxide_import_dialog
                .as_ref()
                .is_some_and(|dialog| dialog.selected_names.contains(&name));
            list = list.child(self.render_oxide_import_check_line(name, label, checked, cx));
        }

        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when_some(icon, |header, icon| {
                        header.child(Self::render_lucide_icon(icon, 16.0, rgb(color)))
                    })
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(color))
                            .child(title),
                    ),
            )
            .child(list)
            .into_any_element()
    }

    fn render_oxide_import_check_line(
        &self,
        name: String,
        label: String,
        checked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor_pointer()
            .hover(|row| row.text_color(rgb(self.tokens.ui.text)))
            .child(Self::render_lucide_icon(
                if checked {
                    LucideIcon::CheckSquare
                } else {
                    LucideIcon::Square
                },
                14.0,
                if checked {
                    rgb(self.tokens.ui.accent)
                } else {
                    rgb(self.tokens.ui.text_muted)
                },
            ))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut() {
                        if dialog.selected_names.contains(&name) {
                            dialog.selected_names.remove(&name);
                        } else {
                            dialog.selected_names.insert(name.clone());
                        }
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_oxide_import_app_settings(
        &self,
        preview: &ImportPreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let import_app_settings = self
            .session_manager
            .oxide_import_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.import_app_settings);
        let all_section_ids = preview
            .app_settings_sections
            .iter()
            .map(|section| section.id.clone())
            .collect::<HashSet<_>>();
        let mut children = vec![self.render_oxide_option_row(
            "应用设置".to_string(),
            "导入应用设置".to_string(),
            import_app_settings,
            cx.listener(move |this, _event, _window, cx| {
                if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut() {
                    if dialog.import_app_settings {
                        dialog.import_app_settings = false;
                        dialog.selected_app_settings_sections.clear();
                    } else {
                        dialog.import_app_settings = true;
                        dialog.selected_app_settings_sections = all_section_ids.clone();
                    }
                }
                cx.notify();
                cx.stop_propagation();
            }),
        )];

        if !preview.app_settings_sections.is_empty() {
            let mut sections = div()
                .mt(px(4.0))
                .pt(px(12.0))
                .border_t_1()
                .border_color(rgb(self.tokens.ui.border))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(self.tokens.ui.text))
                        .child(format!("设置分组（{}）", preview.app_settings_sections.len())),
                );
            for section in &preview.app_settings_sections {
                sections = sections.child(self.render_oxide_import_app_settings_section(section, cx));
            }
            children.push(sections.into_any_element());
        }
        self.render_oxide_import_preview_subcard(children)
    }

    fn render_oxide_import_app_settings_section(
        &self,
        section: &oxideterm_connections::oxide_file::AppSettingsSectionPreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let section_id = section.id.clone();
        let selected = self
            .session_manager
            .oxide_import_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.selected_app_settings_sections.contains(&section_id));
        let expanded = self
            .session_manager
            .oxide_import_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.expanded_app_settings_sections.contains(&section_id));
        let key_summary = if section.id == "legacy" {
            "该文件导出于分组设置支持之前，只能整包导入应用设置。".to_string()
        } else {
            format!(
                "顶层键：{}",
                section
                    .field_keys
                    .iter()
                    .map(|key| oxide_settings_field_label(key))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let mut card = div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.render_oxide_subcard_bg(false))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
                    .cursor_pointer()
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                if selected {
                                    LucideIcon::CheckSquare
                                } else {
                                    LucideIcon::Square
                                },
                                16.0,
                                if selected {
                                    rgb(self.tokens.ui.accent)
                                } else {
                                    rgb(self.tokens.ui.text_muted)
                                },
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(rgb(self.tokens.ui.text))
                                            .child(oxide_settings_section_label(&section.id)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .line_height(px(16.0))
                                            .text_color(rgb(self.tokens.ui.text_muted))
                                            .child(key_summary),
                                    )
                                    .when(section.contains_env_vars, |body| {
                                        body.child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .text_color(rgb(OXIDE_YELLOW_500))
                                                .child("包含本地终端环境变量名"),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(format!("{} 项", section.field_keys.len())),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(dialog) =
                                this.session_manager.oxide_import_dialog.as_mut()
                            {
                                if dialog.selected_app_settings_sections.contains(&section_id) {
                                    dialog.selected_app_settings_sections.remove(&section_id);
                                } else {
                                    dialog
                                        .selected_app_settings_sections
                                        .insert(section_id.clone());
                                }
                                dialog.import_app_settings =
                                    !dialog.selected_app_settings_sections.is_empty();
                            }
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ),
            );

        if section.id != "legacy" && !section.field_values.is_empty() {
            let toggle_id = section.id.clone();
            card = card.child(
                div()
                    .border_t_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .pt(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.accent))
                            .cursor_pointer()
                            .child(if expanded { "隐藏更改" } else { "查看更改" })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    if let Some(dialog) =
                                        this.session_manager.oxide_import_dialog.as_mut()
                                    {
                                        if dialog.expanded_app_settings_sections.contains(&toggle_id)
                                        {
                                            dialog.expanded_app_settings_sections.remove(&toggle_id);
                                        } else {
                                            dialog
                                                .expanded_app_settings_sections
                                                .insert(toggle_id.clone());
                                        }
                                    }
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                    .when(expanded, |values| {
                        let mut values = values.child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(self.tokens.ui.text))
                                .child(self.render_selectable_text_scoped(
                                    "oxide-import-app-settings-changes-heading",
                                    &section.id,
                                    "顶层设置变更",
                                    self.tokens.ui.text,
                                    cx,
                                )),
                        );
                        for key in &section.field_keys {
                            if let Some(value) = section.field_values.get(key) {
                                let line =
                                    format!("{}: {}", oxide_settings_field_label(key), value);
                                values = values.child(
                                    div()
                                        .rounded(px(self.tokens.radii.sm))
                                        .bg(self.render_oxide_subcard_bg(true))
                                        .px_2()
                                        .py(px(6.0))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(self.render_selectable_text_scoped(
                                            "oxide-import-app-settings-change",
                                            key,
                                            line,
                                            self.tokens.ui.text_muted,
                                            cx,
                                        )),
                                );
                            }
                        }
                        values
                    }),
            );
        }
        card.into_any_element()
    }

    fn render_oxide_import_quick_commands(
        &self,
        preview: &ImportPreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let checked = self
            .session_manager
            .oxide_import_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.import_quick_commands);
        self.render_oxide_import_preview_subcard(vec![self.render_oxide_option_row(
                format!("快捷命令（{} 条命令）", preview.quick_commands_count),
                format!(
                    "导入 {} 个快捷命令组。已有冲突会按当前冲突策略处理；替换只替换冲突项。",
                    preview.quick_command_categories_count
                ),
                checked,
                cx.listener(|this, _event, _window, cx| {
                    if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut() {
                        dialog.import_quick_commands = !dialog.import_quick_commands;
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )])
    }

    fn render_oxide_import_plugins(
        &self,
        preview: &ImportPreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let import_plugin_settings = self
            .session_manager
            .oxide_import_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.import_plugin_settings);
        let mut children = vec![self.render_oxide_option_row(
            format!("插件偏好设置（{} 个插件）", preview.plugin_settings_by_plugin.len()),
            "导入插件偏好设置".to_string(),
            import_plugin_settings,
            cx.listener(|this, _event, _window, cx| {
                if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut() {
                    dialog.import_plugin_settings = !dialog.import_plugin_settings;
                }
                cx.notify();
                cx.stop_propagation();
            }),
        )];
        let mut entries = preview
            .plugin_settings_by_plugin
            .iter()
            .map(|(plugin_id, count)| (plugin_id.clone(), *count))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        if !entries.is_empty() {
            let mut list = div().flex().flex_col().gap(px(4.0));
            for (plugin_id, count) in entries {
                let checked = self
                    .session_manager
                    .oxide_import_dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.selected_plugin_ids.contains(&plugin_id));
                list = list.child(self.render_oxide_import_plugin_row(plugin_id, count, checked, cx));
            }
            children.push(list.into_any_element());
        } else {
            children.push(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "oxide-import-plugin-settings-summary",
                        (),
                        format!("此文件还会恢复 {} 项插件偏好设置。", preview.plugin_settings_count),
                        self.tokens.ui.text_muted,
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        self.render_oxide_import_preview_subcard(children)
    }

    fn render_oxide_import_plugin_row(
        &self,
        plugin_id: String,
        count: usize,
        checked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(self.tokens.radii.md))
            .px_2()
            .py(px(6.0))
            .hover(|row| row.bg(rgb(self.tokens.ui.bg)))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(Self::render_lucide_icon(
                        if checked {
                            LucideIcon::CheckSquare
                        } else {
                            LucideIcon::Square
                        },
                        14.0,
                        if checked {
                            rgb(self.tokens.ui.accent)
                        } else {
                            rgb(self.tokens.ui.text_muted)
                        },
                    ))
                    .child(plugin_id.clone()),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(format!("{count} 项设置")),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut() {
                        if dialog.selected_plugin_ids.contains(&plugin_id) {
                            dialog.selected_plugin_ids.remove(&plugin_id);
                        } else {
                            dialog.selected_plugin_ids.insert(plugin_id.clone());
                            dialog.import_plugin_settings = true;
                        }
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_oxide_import_portable_secrets(
        &self,
        preview: &ImportPreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let checked = self
            .session_manager
            .oxide_import_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.import_portable_secrets);
        self.render_oxide_import_preview_subcard(
            vec![
                self.render_oxide_option_row(
                    format!("便携秘密项（{} 项）", preview.portable_secret_count),
                    "导入便携秘密项".to_string(),
                    checked,
                    cx.listener(|this, _event, _window, cx| {
                        if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut() {
                            dialog.import_portable_secrets = !dialog.import_portable_secrets;
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }),
                ),
                self.render_oxide_tone_notice(
                    OXIDE_BLUE_500,
                    format!("此文件还包含 {} 项便携秘密项，例如 AI 提供商密钥。", preview.portable_secret_count),
                    Vec::new(),
                    cx,
                ),
            ],
        )
    }

    fn render_oxide_import_forwards(
        &self,
        preview: &ImportPreview,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let checked = self
            .session_manager
            .oxide_import_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.import_forwards);
        let mut children = vec![self.render_oxide_option_row(
            format!("已保存的端口转发（{}）", preview.total_forwards),
            "导入已保存的端口转发".to_string(),
            checked,
            cx.listener(|this, _event, _window, cx| {
                if let Some(dialog) = this.session_manager.oxide_import_dialog.as_mut() {
                    dialog.import_forwards = !dialog.import_forwards;
                }
                cx.notify();
                cx.stop_propagation();
            }),
        )];
        if !preview.forward_details.is_empty() {
            let mut list = div()
                .max_h(px(112.0))
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .gap(px(4.0));
            for (index, detail) in preview.forward_details.iter().enumerate() {
                list = list.child(
                    div()
                        .rounded(px(self.tokens.radii.md))
                        .bg(self.render_oxide_subcard_bg(false))
                        .px_2()
                        .py(px(6.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.render_selectable_text_scoped(
                            "oxide-import-forward-detail",
                            index,
                            format!("{} · {}", detail.owner_connection_name, detail.description),
                            self.tokens.ui.text_muted,
                            cx,
                        )),
                );
            }
            children.push(list.into_any_element());
        }
        self.render_oxide_import_preview_subcard(children)
    }

    fn render_oxide_import_preview_subcard(&self, children: Vec<AnyElement>) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.render_oxide_subcard_bg(true))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .children(children)
            .into_any_element()
    }


}
