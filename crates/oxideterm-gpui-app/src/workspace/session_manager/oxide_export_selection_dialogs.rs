impl WorkspaceApp {
    fn render_oxide_connection_selection(
        &self,
        connections: &[SavedConnection],
        selected_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let total = connections.len();
        let all_selected = total > 0 && selected_count == total;
        let new_connection_count = self
            .session_manager
            .oxide_export_dialog
            .as_ref()
            .and_then(|dialog| dialog.last_export_timestamp)
            .map(|timestamp| {
                connections
                    .iter()
                    .filter(|connection| connection.created_at.timestamp_millis() > timestamp)
                    .count()
            })
            .unwrap_or(0);
        let mut list = div()
            .id("oxide-export-connections-selection")
            .max_h(px(OXIDE_MODAL_LIST_MAX_H))
            .selectable_overflow_y_scrollbar(
                &self.selectable_text_scroll_handle("oxide-export-connections-selection"),
            )
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg))
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(4.0));
        if connections.is_empty() {
            list = list.child(
                div()
                    .py(px(16.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "oxide-export-connections",
                        "empty",
                        "无保存的连接",
                        theme.text_muted,
                        cx,
                    )),
            );
        } else {
            for connection in connections {
                let id = connection.id.clone();
                let row_key = id.clone();
                let checked = self.session_manager.oxide_export_dialog.as_ref().is_some_and(
                    |dialog| dialog.selected_ids.contains(&connection.id),
                );
                let meta = format!(
                    "{}@{}:{}{}",
                    connection.username,
                    connection.host,
                    connection.port,
                    connection
                        .group
                        .as_ref()
                        .map(|group| format!(" [{group}]"))
                        .unwrap_or_default()
                );
                let is_new_since_last_export = self
                    .session_manager
                    .oxide_export_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.last_export_timestamp)
                    .is_some_and(|timestamp| connection.created_at.timestamp_millis() > timestamp);
                list = list.child(
                    div()
                        .p(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(self.tokens.radii.sm))
                        .hover(move |row| row.bg(rgb(theme.bg_hover)))
                        .cursor_pointer()
                        .child(self.render_oxide_checkbox(
                            String::new(),
                            checked,
                            cx.listener(move |this, _event, _window, cx| {
                                if let Some(dialog) =
                                    this.session_manager.oxide_export_dialog.as_mut()
                                {
                                    if dialog.selected_ids.contains(&id) {
                                        dialog.selected_ids.remove(&id);
                                    } else {
                                        dialog.selected_ids.insert(id.clone());
                                    }
                                }
                                this.refresh_oxide_export_preflight();
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .truncate()
                                        .text_size(px(self.tokens.metrics.ui_text_sm))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(rgb(theme.text))
                                        .child(self.render_display_text_with_role(
                                            SelectableTextRole::NonSelectable,
                                            "oxide-export-connection-name",
                                            row_key.as_str(),
                                            connection.name.clone(),
                                            theme.text,
                                            cx,
                                        ))
                                        .when(is_new_since_last_export, |row| {
                                            row.child(
                                                div()
                                                    .px(px(6.0))
                                                    .py(px(2.0))
                                                    .rounded_full()
                                                    .bg(rgba(
                                                        (OXIDE_GREEN_500 << 8)
                                                            | OXIDE_NEW_BADGE_BG_ALPHA,
                                                    ))
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(2.0))
                                                    .text_size(px(10.0))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(rgb(OXIDE_GREEN_500))
                                                    .child(Self::render_lucide_icon(
                                                        LucideIcon::Sparkles,
                                                        10.0,
                                                        rgb(OXIDE_GREEN_500),
                                                    ))
                                                    .child(self.render_display_text_with_role(
                                                        SelectableTextRole::PlainDocument,
                                                        "oxide-export-new-badge",
                                                        row_key.as_str(),
                                                        "新",
                                                        OXIDE_GREEN_500,
                                                        cx,
                                                    )),
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(theme.text_muted))
                                        .child(self.render_display_text_with_role(
                                            SelectableTextRole::NonSelectable,
                                            "oxide-export-connection-meta",
                                            row_key.as_str(),
                                            meta,
                                            theme.text_muted,
                                            cx,
                                        )),
                                ),
                        ),
                );
            }
        }

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "oxide-export-selection",
                                "connection-count",
                                format!("选择要导出的连接 ({selected_count}/{total})"),
                                theme.text,
                                cx,
                            )),
                    )
                    .child(
                        // Tauri OxideExportModal renders select-all as an
                        // outline h-7 text-xs Button. Route through the shared
                        // toolbar primitive so disabled/focus behavior matches
                        // the rest of the dialog actions.
                        self.workspace_toolbar_action_button(
                            if all_selected {
                                "取消全选".to_string()
                            } else {
                                "全选".to_string()
                            },
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: total == 0,
                                },
                                height: Some(OXIDE_SELECT_ALL_BUTTON_HEIGHT),
                                font_size: Some(self.tokens.metrics.ui_text_xs),
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(move |this, _event, _window, cx| {
                                let all_ids = this
                                    .connection_store
                                    .connections()
                                    .iter()
                                    .map(|connection| connection.id.clone())
                                    .collect::<HashSet<_>>();
                                if let Some(dialog) =
                                    this.session_manager.oxide_export_dialog.as_mut()
                                {
                                    if dialog.selected_ids.len() == all_ids.len() {
                                        dialog.selected_ids.clear();
                                    } else {
                                        dialog.selected_ids = all_ids;
                                    }
                                }
                                this.refresh_oxide_export_preflight();
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            )
            .when(new_connection_count > 0, |section| {
                section.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(OXIDE_GREEN_500))
                        .child(Self::render_lucide_icon(
                            LucideIcon::Sparkles,
                            12.0,
                            rgb(OXIDE_GREEN_500),
                        ))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "oxide-export-selection",
                            "new-connections",
                            format!("上次导出后新增 {new_connection_count} 个连接"),
                            OXIDE_GREEN_500,
                            cx,
                        )),
                )
            })
            .child(list)
            .into_any_element()
    }

    fn render_oxide_export_options(
        &self,
        dialog: &OxideExportDialogState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(OXIDE_MODAL_SECTION_GAP))
            .child(self.render_oxide_forward_card(dialog, cx))
            .child(self.render_oxide_option_row(
                "包含全局设置".to_string(),
                "导出终端外观、操作习惯和其他 OxideTerm 应用设置。".to_string(),
                dialog.include_app_settings,
                cx.listener(|this, _event, _window, cx| {
                    if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut() {
                        dialog.include_app_settings = !dialog.include_app_settings;
                    }
                    this.refresh_oxide_export_preflight();
                    cx.notify();
                    cx.stop_propagation();
                }),
                cx,
            ))
            .when(dialog.include_app_settings, |options| {
                let mut children = vec![
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(self.tokens.ui.text))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "oxide-export-app-settings",
                                    "title",
                                    "应用设置分组",
                                    self.tokens.ui.text,
                                    cx,
                                )),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "oxide-export-app-settings",
                                    "description",
                                    "选择要包含到 .oxide 文件中的应用设置分组。",
                                    self.tokens.ui.text_muted,
                                    cx,
                                )),
                        )
                        .into_any_element(),
                    self.render_oxide_settings_section_grid(
                        &dialog.selected_app_settings_sections,
                        false,
                        cx,
                    ),
                ];
                if dialog.selected_app_settings_sections.is_empty() {
                    children.push(self.render_oxide_section_empty_warning(
                        "尚未选择任何应用设置分组".to_string(),
                        cx,
                    ));
                }
                options.child(self.render_oxide_card(None, children, cx))
            })
            .when(
                dialog.include_app_settings
                    && dialog
                        .selected_app_settings_sections
                        .contains("localTerminal"),
                |options| {
                    options.child(self.render_oxide_card(
                        None,
                        vec![self.render_oxide_option_row(
                            "包含本地终端环境变量".to_string(),
                            "可能包含机器相关或敏感值。".to_string(),
                            dialog.include_local_terminal_env_vars,
                            cx.listener(|this, _event, _window, cx| {
                                if let Some(dialog) =
                                    this.session_manager.oxide_export_dialog.as_mut()
                                {
                                    dialog.include_local_terminal_env_vars =
                                        !dialog.include_local_terminal_env_vars;
                                }
                                cx.notify();
                                cx.stop_propagation();
                            }),
                            cx,
                        )],
                        cx,
                    ))
                },
            )
            .child(self.render_oxide_option_row(
                "包含快捷命令".to_string(),
                "快捷命令可能包含主机名、路径或命令中的敏感信息。".to_string(),
                dialog.include_quick_commands,
                cx.listener(|this, _event, _window, cx| {
                    if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut() {
                        dialog.include_quick_commands = !dialog.include_quick_commands;
                    }
                    this.refresh_oxide_export_preflight();
                    cx.notify();
                    cx.stop_propagation();
                }),
                cx,
            ))
            .child(self.render_oxide_option_row(
                "包含插件偏好设置".to_string(),
                "导出存放在 OxideTerm 本地存储中的声明式插件 settings。".to_string(),
                dialog.include_plugin_settings,
                cx.listener(|this, _event, _window, cx| {
                    if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut() {
                        dialog.include_plugin_settings = !dialog.include_plugin_settings;
                    }
                    this.refresh_oxide_export_preflight();
                    cx.notify();
                    cx.stop_propagation();
                }),
                cx,
            ))
            .child(self.render_oxide_export_plugin_settings(dialog, cx))
            .child(self.render_oxide_option_row(
                "包含便携秘密项".to_string(),
                "导出可在导入时恢复的便携安全秘密项，例如 AI 提供商密钥。".to_string(),
                dialog.include_portable_secrets,
                cx.listener(|this, _event, _window, cx| {
                    if let Some(dialog) = this.session_manager.oxide_export_dialog.as_mut() {
                        dialog.include_portable_secrets = !dialog.include_portable_secrets;
                    }
                    this.refresh_oxide_export_preflight();
                    cx.notify();
                    cx.stop_propagation();
                }),
                cx,
            ))
            .into_any_element()
    }


    fn render_oxide_forward_card(
        &self,
        dialog: &OxideExportDialogState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut children = vec![div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .line_height(px(16.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "oxide-export-forwards",
                "description",
                "所选的已保存端口转发会连同其所属的连接配置一起导出。",
                self.tokens.ui.text_muted,
                cx,
            ))
            .into_any_element()];
        if dialog.available_forwards.is_empty() {
            children.push(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "oxide-export-forwards",
                        "empty",
                        "没有已保存的端口转发",
                        self.tokens.ui.text_muted,
                        cx,
                    ))
                    .into_any_element(),
            );
        } else {
            children.push(self.render_oxide_forward_selection(dialog, cx));
        }
        self.render_oxide_card(
            Some((
                LucideIcon::Shield,
                format!("已保存的端口转发（{}）", dialog.available_forwards.len()),
            )),
            children,
            cx,
        )
    }

    fn render_oxide_forward_selection(
        &self,
        dialog: &OxideExportDialogState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut groups: HashMap<String, Vec<PersistedForward>> = HashMap::new();
        let names = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| (connection.id.clone(), connection.name.clone()))
            .collect::<HashMap<_, _>>();
        for forward in &dialog.available_forwards {
            let owner = forward
                .owner_connection_id
                .as_ref()
                .and_then(|id| names.get(id).cloned().or_else(|| Some(id.clone())))
                .unwrap_or_else(|| "-".to_string());
            groups.entry(owner).or_default().push(forward.clone());
        }
        let mut entries = groups.into_iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));

        let mut list = div()
            .id("oxide-export-forwards-selection")
            .max_h(px(OXIDE_MODAL_FORWARDS_MAX_H))
            .selectable_overflow_y_scrollbar(
                &self.selectable_text_scroll_handle("oxide-export-forwards-selection"),
            )
            .flex()
            .flex_col()
            .gap(px(12.0));
        for (owner, forwards) in entries {
            let mut group = div().flex().flex_col().gap(px(4.0)).child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(owner),
            );
            for forward in forwards {
                let forward_id = forward.id.clone();
                let checked = dialog.selected_forward_ids.contains(&forward.id);
                group = group.child(
                    div()
                        .px_1()
                        .py(px(4.0))
                        .rounded(px(self.tokens.radii.sm))
                        .flex()
                        .items_start()
                        .gap(px(8.0))
                        .hover(|row| row.bg(rgb(self.tokens.ui.bg_hover)))
                        .cursor_pointer()
                        .child(self.render_oxide_checkbox(
                            String::new(),
                            checked,
                            cx.listener(move |this, _event, _window, cx| {
                                if let Some(dialog) =
                                    this.session_manager.oxide_export_dialog.as_mut()
                                {
                                    if dialog.selected_forward_ids.contains(&forward_id) {
                                        dialog.selected_forward_ids.remove(&forward_id);
                                    } else {
                                        dialog.selected_forward_ids.insert(forward_id.clone());
                                    }
                                }
                                this.refresh_oxide_export_preflight();
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        ))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .child(
                                    div()
                                        .text_color(rgb(self.tokens.ui.text))
                                        .child(oxide_forward_description_or_summary(&forward)),
                                )
                                .child(
                                    div()
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(oxide_forward_summary(&forward)),
                                ),
                        ),
                );
            }
            list = list.child(group);
        }
        list.into_any_element()
    }

    fn render_oxide_export_plugin_settings(
        &self,
        dialog: &OxideExportDialogState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut entries = dialog
            .plugin_groups
            .iter()
            .map(|(plugin_id, count)| (plugin_id.clone(), *count))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        if entries.is_empty() {
            return self.render_oxide_card(
                None,
                vec![div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "oxide-export-plugin-settings",
                        "empty",
                        "没有可导出的插件偏好设置",
                        self.tokens.ui.text_muted,
                        cx,
                    ))
                    .into_any_element()],
                cx,
            );
        }

        let mut children = Vec::new();
        for (plugin_id, count) in entries {
            let selected = dialog.selected_plugin_ids.contains(&plugin_id);
            let enabled = dialog.include_plugin_settings;
            let row_plugin_id = plugin_id.clone();
            children.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .opacity(if enabled { 1.0 } else { 0.6 })
                    .cursor_pointer()
                    .child(self.render_oxide_checkbox(
                        String::new(),
                        selected,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(dialog) =
                                this.session_manager.oxide_export_dialog.as_mut()
                            {
                                if dialog.selected_plugin_ids.contains(&row_plugin_id) {
                                    dialog.selected_plugin_ids.remove(&row_plugin_id);
                                } else {
                                    dialog.selected_plugin_ids.insert(row_plugin_id.clone());
                                }
                            }
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "oxide-export-plugin-settings",
                                plugin_id.as_str(),
                                format!("{}（{} 项设置）", plugin_id, count),
                                self.tokens.ui.text,
                                cx,
                            )),
                    )
                    .into_any_element(),
            );
        }
        self.render_oxide_card(None, children, cx)
    }

}
