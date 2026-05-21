impl WorkspaceApp {
    fn render_session_manager_toolbar(
        &self,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected_count = self.session_manager.selected_ids.len();
        let viewport_width = f32::from(window.viewport_size().width);
        let show_primary_labels = viewport_width >= MANAGER_RESPONSIVE_SM;
        let show_transfer_labels = viewport_width >= MANAGER_RESPONSIVE_MD;
        div()
            .min_h(px(48.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(theme_bg(theme.bg, has_background))
            .child(
                div()
                    .flex_1()
                    .min_w(px(160.0))
                    .max_w(px(MANAGER_TOOLBAR_SEARCH_WIDTH))
                    .child(self.render_session_text_input(
                        SessionManagerInput::Search,
                        &self.session_manager.search_query,
                        self.i18n.t("sessionManager.toolbar.search_placeholder"),
                        cx,
                    )),
            )
            .child(
                self.render_toolbar_button(
                    LucideIcon::Plus,
                    self.i18n.t("sessionManager.toolbar.new_connection"),
                    ButtonVariant::Default,
                    has_background,
                    show_primary_labels,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        this.open_new_connection_form(window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                self.render_toolbar_button(
                    LucideIcon::Network,
                    self.i18n.t("sessionManager.toolbar.auto_route"),
                    ButtonVariant::Outline,
                    has_background,
                    show_primary_labels,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        this.open_auto_route_modal(window, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .when(selected_count > 0, |batch| {
                        batch
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .px_1()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(selected_count_label(&self.i18n, selected_count)),
                            )
                            .child(
                                self.render_session_manager_button(
                                    LucideIcon::FolderInput,
                                    self.i18n.t("sessionManager.batch.move_to_group"),
                                    ButtonVariant::Outline,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.session_manager.show_batch_move =
                                            !this.session_manager.show_batch_move;
                                        cx.notify();
                                        cx.stop_propagation();
                                    }),
                                ),
                            )
                            .child(
                                self.render_session_manager_button(
                                    LucideIcon::Trash2,
                                    self.i18n.t("sessionManager.batch.delete"),
                                    ButtonVariant::Outline,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.delete_selected_connections(cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            )
                    }),
            )
            .when(
                selected_count > 0 && self.session_manager.show_batch_move,
                |toolbar| toolbar.child(self.render_batch_move_popover(cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(22.0))
                    .child(self.render_toolbar_link_icon(
                        LucideIcon::Download,
                        "sessionManager.toolbar.import",
                        SessionTransferAction::ImportOxide,
                        show_transfer_labels,
                        cx,
                    ))
                    .child(self.render_toolbar_link_icon(
                        LucideIcon::Upload,
                        "sessionManager.toolbar.export",
                        SessionTransferAction::ExportOxide,
                        show_transfer_labels,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_session_manager_folder_tree(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let all_count = self.connection_store.connections().len();
        let ungrouped_count = self
            .connection_store
            .connections()
            .iter()
            .filter(|conn| conn.group.is_none())
            .count();
        let (root_groups, child_groups) = self.session_group_tree();
        let mut groups = div()
            .id("session-manager-folder-tree-scroll")
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .selectable_overflow_y_scroll(
                &self.selectable_text_scroll_handle("session-manager-folder-tree-scroll"),
            )
            .px_1()
            .py_1();

        for group in root_groups {
            groups = groups.child(self.render_group_tree_node(
                group,
                0,
                &child_groups,
                has_background,
                cx,
            ));
        }

        if ungrouped_count > 0 {
            groups = groups.child(self.render_group_tree_item(
                Some(UNGROUPED_FILTER.to_string()),
                LucideIcon::Folder,
                self.i18n.t("sessionManager.folder_tree.ungrouped"),
                Some(ungrouped_count),
                0,
                has_background,
                cx,
            ));
        }

        div()
            .id("session-manager-folder-tree")
            .w(px(MANAGER_FOLDER_TREE_WIDTH))
            .min_w(px(140.0))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(
                div()
                    .flex_none()
                    .pt_2()
                    .px_1()
                    .child(self.render_group_tree_item(
                        None,
                        LucideIcon::Inbox,
                        self.i18n.t("sessionManager.folder_tree.all_connections"),
                        Some(all_count),
                        0,
                        has_background,
                        cx,
                    ))
                    .child(self.render_new_group_tree_item(has_background, cx)),
            )
            .child(groups)
            .child(
                div()
                    .flex_none()
                    .border_t_1()
                    .border_color(theme_border(theme.border, has_background))
                    .px_1()
                    .py(px(6.0))
                    .child(self.render_group_tree_item(
                        Some(RECENT_FILTER.to_string()),
                        LucideIcon::Clock,
                        self.i18n.t("sessionManager.folder_tree.recent"),
                        None,
                        0,
                        has_background,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_group_tree_item(
        &self,
        group: Option<String>,
        icon: LucideIcon,
        label: String,
        count: Option<usize>,
        depth: usize,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.session_manager.selected_group == group;
        let selection_group_id =
            crate::workspace::selectable_text::selectable_text_id("session-manager-tree-row", (
                group.as_deref(),
                label.as_str(),
            ));
        div()
            .min_h(px(32.0))
            .pl(px(12.0 + depth as f32 * 16.0))
            .pr_2()
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .bg(if active {
                theme_active_bg(theme.bg_active, has_background)
            } else {
                rgba(0x00000000)
            })
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text)
            })
            .hover(move |item| {
                if active {
                    item
                } else {
                    item.bg(theme_hover_bg(theme.bg_hover, has_background))
                }
            })
            .child(Self::render_lucide_icon(
                icon,
                16.0,
                match icon {
                    LucideIcon::Inbox => rgb(0x60a5fa),
                    LucideIcon::Folder | LucideIcon::FolderOpen => rgb(0xeab308),
                    _ if active => rgb(theme.text),
                    _ => rgb(theme.text_muted),
                },
            ))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(if active {
                        gpui::FontWeight::MEDIUM
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .child(self.render_row_safe_selectable_display_text_in_group(
                        selection_group_id,
                        "session-manager-tree-cell",
                        "label",
                        0,
                        label,
                        theme.text,
                        None,
                        cx,
                    )),
            )
            .when_some(count, |item, count| {
                item.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(self.render_row_safe_selectable_display_text_in_group(
                            selection_group_id,
                            "session-manager-tree-cell",
                            "count",
                            1,
                            count.to_string(),
                            theme.text_muted,
                            None,
                            cx,
                        )),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.session_manager.selected_group = group.clone();
                    this.clear_session_selection_for_invisible_rows();
                    this.session_manager.show_batch_move = false;
                    this.session_manager.focused_input = None;
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_group_tree_node(
        &self,
        group: String,
        depth: usize,
        child_groups: &HashMap<String, Vec<String>>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let children = child_groups.get(&group).cloned().unwrap_or_default();
        let has_children = !children.is_empty();
        let expanded = self.session_manager.expanded_groups.contains(&group);
        let active = self.session_manager.selected_group.as_deref() == Some(group.as_str());
        let label = group
            .rsplit('/')
            .next()
            .unwrap_or(group.as_str())
            .to_string();
        let selected_group = group.clone();
        let selection_group_id =
            crate::workspace::selectable_text::selectable_text_id("session-manager-tree-node", (
                group.as_str(),
                label.as_str(),
            ));
        let mut node = div().child(
            div()
                .min_h(px(28.0))
                .pl(px((depth.min(5) as f32 * 16.0) + 8.0))
                .pr_2()
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(self.tokens.radii.md))
                .cursor_pointer()
                .bg(if active {
                    theme_active_bg(theme.bg_active, has_background)
                } else {
                    rgba(0x00000000)
                })
                .hover(move |item| {
                    if active {
                        item
                    } else {
                        item.bg(theme_hover_bg(theme.bg_hover, has_background))
                    }
                })
                .child(if has_children {
                    div()
                        .size(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.md))
                        .hover(move |button| {
                            button.bg(theme_hover_bg(self.tokens.ui.bg_hover, has_background))
                        })
                        .child(Self::render_lucide_icon(
                            if expanded {
                                LucideIcon::ChevronDown
                            } else {
                                LucideIcon::ChevronRight
                            },
                            14.0,
                            rgb(theme.text_muted),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener({
                                let group = group.clone();
                                move |this, _event, _window, cx| {
                                    this.toggle_session_group_expanded(&group);
                                    cx.notify();
                                    cx.stop_propagation();
                                }
                            }),
                        )
                        .into_any_element()
                } else {
                    div().w(px(18.0)).flex_none().into_any_element()
                })
                .child(Self::render_lucide_icon(
                    if expanded && has_children {
                        LucideIcon::FolderOpen
                    } else {
                        LucideIcon::Folder
                    },
                    16.0,
                    rgb(0xeab308),
                ))
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(if active {
                            gpui::FontWeight::MEDIUM
                        } else {
                        gpui::FontWeight::NORMAL
                    })
                    .child(self.render_row_safe_selectable_display_text_in_group(
                        selection_group_id,
                        "session-manager-tree-cell",
                        "label",
                        0,
                        label,
                        theme.text,
                        None,
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_row_safe_selectable_display_text_in_group(
                        selection_group_id,
                        "session-manager-tree-cell",
                        "count",
                        1,
                        self.connection_count_for_group(&group).to_string(),
                        theme.text_muted,
                        None,
                        cx,
                    )),
            )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.session_manager.selected_group = Some(selected_group.clone());
                        this.clear_session_selection_for_invisible_rows();
                        this.session_manager.show_batch_move = false;
                        this.session_manager.focused_input = None;
                        cx.notify();
                    }),
                ),
        );

        if expanded && has_children {
            for child in children {
                node = node.child(self.render_group_tree_node(
                    child,
                    depth + 1,
                    child_groups,
                    has_background,
                    cx,
                ));
            }
        }

        node.into_any_element()
    }

    fn render_new_group_tree_item(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(32.0))
            .w_full()
            .px_3()
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(theme_border(theme.border, has_background))
            .text_color(rgb(theme.text_muted))
            .cursor_pointer()
            .hover(move |item| {
                item.bg(theme_hover_bg(self.tokens.ui.bg_hover, has_background))
                    .text_color(rgb(self.tokens.ui.text))
            })
            .child(Self::render_lucide_icon(
                LucideIcon::Plus,
                16.0,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "session-manager-tree-action",
                        "new-group",
                        self.i18n.t("sessionManager.folder_tree.new_group"),
                        theme.text_muted,
                        cx,
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.session_manager.show_new_group = true;
                    this.session_manager.focused_input = Some(SessionManagerInput::NewGroup);
                    // The text field is the initial browser focus target; the
                    // footer owner is established only after Tab enters it.
                    this.session_manager.focused_basic_dialog_footer_action = None;
                    this.session_manager.new_group_name.clear();
                    this.needs_active_pane_focus = false;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

}
