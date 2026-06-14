#[derive(Clone)]
struct ActiveSessionSidebarRow {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    node: WorkspaceSshNode,
    node_view: ActiveSessionNode,
    depth: usize,
    is_last: bool,
    has_children: bool,
}

impl WorkspaceApp {
    fn render_active_sessions_sidebar_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.active_session_sidebar_view_mode == ActiveSessionSidebarViewMode::Focus {
            return self.render_active_sessions_focus_sidebar_content(cx);
        }

        let rows = self.active_session_sidebar_rows();
        if rows.is_empty() {
            return self.render_empty_sessions_sidebar_content(cx);
        }

        self.sync_active_session_sidebar_list_state(&rows, ActiveSessionSidebarViewMode::Tree);
        let state = self.active_session_sidebar_list_state.clone();
        let spec = self.active_session_sidebar_list_spec(ActiveSessionSidebarViewMode::Tree);
        let workspace = cx.entity();
        div()
            .id("active-sessions-sidebar-scroll")
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.render_active_session_sidebar_list_item(index, cx)
                    })
                },
            ))
            .into_any_element()
    }

    fn render_active_sessions_focus_sidebar_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.active_session_sidebar_rows();
        let focused_node_id = self.effective_active_session_focus_node_id(&rows);
        self.active_session_sidebar_focused_node_id = focused_node_id.clone();
        let visible_rows = self.active_session_focus_rows(&rows, focused_node_id.as_ref());
        self.sync_active_session_sidebar_list_state(
            &visible_rows,
            ActiveSessionSidebarViewMode::Focus,
        );

        let state = self.active_session_sidebar_list_state.clone();
        let spec = self.active_session_sidebar_list_spec(ActiveSessionSidebarViewMode::Focus);
        let workspace = cx.entity();
        div()
            .id("active-sessions-focus-sidebar")
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .child(self.render_active_session_focus_breadcrumb(&rows, focused_node_id.as_ref(), cx))
            .child(self.render_active_session_focus_location_header(
                &rows,
                focused_node_id.as_ref(),
                visible_rows.len(),
                cx,
            ))
            .child(
                div()
                    .id("active-sessions-focus-list")
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .py_2()
                    .child(if visible_rows.is_empty() {
                        self.render_active_session_focus_empty(focused_node_id.as_ref(), cx)
                    } else {
                        tauri_virtual_list(
                            state,
                            spec,
                            move |index, _window, cx| {
                                workspace.update(cx, |this, cx| {
                                    this.render_active_session_focus_list_item(index, cx)
                                })
                            },
                        )
                        .into_any_element()
                    }),
            )
            .into_any_element()
    }

    fn active_session_sidebar_rows(&self) -> Vec<ActiveSessionSidebarRow> {
        let mut tree_nodes = self.node_router.flatten_tree();
        let flat_node_child_counts = tree_nodes
            .iter()
            .filter_map(|node| node.parent_id.as_ref())
            .fold(HashMap::<String, usize>::new(), |mut counts, parent_id| {
                *counts.entry(parent_id.clone()).or_default() += 1;
                counts
            });

        let rows = tree_nodes
            .drain(..)
            .filter_map(|flat_node| {
                let flat_node_id = flat_node.id.clone();
                let node_id = NodeId::new(flat_node_id.clone());
                let node = self.ssh_nodes.get(&node_id)?.clone();
                let node_view = ActiveSessionNode {
                    id: flat_node_id.clone(),
                    title: node.title.clone(),
                    port: flat_node.port,
                    terminal_ids: node.terminal_ids.clone(),
                    readiness: active_session_readiness(&node.readiness),
                };
                Some(ActiveSessionSidebarRow {
                    node_id,
                    parent_id: flat_node.parent_id.map(NodeId::new),
                    node,
                    node_view,
                    depth: flat_node.depth as usize,
                    is_last: flat_node.is_last_child,
                    has_children: flat_node_child_counts
                        .get(&flat_node_id)
                        .is_some_and(|count| *count > 0),
                })
            })
            .collect::<Vec<_>>();
        rows
    }

    fn sync_active_session_sidebar_list_state(
        &mut self,
        rows: &[ActiveSessionSidebarRow],
        view_mode: ActiveSessionSidebarViewMode,
    ) {
        let signatures = rows
            .iter()
            .map(|row| self.active_session_sidebar_row_signature(row, view_mode))
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.active_session_sidebar_list_state,
            &mut self.active_session_sidebar_list_cache.borrow_mut(),
            "active-sessions-sidebar",
            &signatures,
            self.active_session_sidebar_list_spec(view_mode),
        );
    }

    fn active_session_sidebar_list_spec(
        &self,
        view_mode: ActiveSessionSidebarViewMode,
    ) -> TauriVirtualListSpec {
        let estimated_height = match view_mode {
            ActiveSessionSidebarViewMode::Tree => ACTIVE_SESSION_SIDEBAR_LIST_ESTIMATED_HEIGHT,
            ActiveSessionSidebarViewMode::Focus => ACTIVE_SESSION_FOCUS_LIST_ESTIMATED_HEIGHT,
        };
        TauriVirtualListSpec::new(px(estimated_height), ACTIVE_SESSION_SIDEBAR_LIST_OVERSCAN)
    }

    fn render_active_session_sidebar_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = self.active_session_sidebar_rows().into_iter().nth(index) else {
            return div().into_any_element();
        };
        div()
            .px_1()
            .child(self.render_active_session_node(
                row.node_id,
                row.node,
                row.node_view,
                row.depth,
                row.is_last,
                cx,
            ))
            .into_any_element()
    }

    fn active_session_sidebar_row_signature(
        &self,
        row: &ActiveSessionSidebarRow,
        view_mode: ActiveSessionSidebarViewMode,
    ) -> u64 {
        let mut hasher = DefaultHasher::new();
        // This virtual row owns the node header plus expanded action/terminal
        // children. Hash all state that can change its visible height or labels.
        view_mode.hash(&mut hasher);
        row.node_id.hash(&mut hasher);
        row.parent_id.hash(&mut hasher);
        row.node.title.hash(&mut hasher);
        row.node.config.port.hash(&mut hasher);
        row.node.terminal_ids.hash(&mut hasher);
        row.node_view.title.hash(&mut hasher);
        row.node_view.terminal_ids.hash(&mut hasher);
        format!("{:?}", row.node_view.status()).hash(&mut hasher);
        row.depth.hash(&mut hasher);
        row.is_last.hash(&mut hasher);
        row.has_children.hash(&mut hasher);
        self.expanded_ssh_nodes.contains(&row.node_id).hash(&mut hasher);
        self.has_active_reconnect_job(&row.node_id).hash(&mut hasher);
        (self.active_ssh_node_id.as_ref() == Some(&row.node_id)).hash(&mut hasher);
        hasher.finish()
    }

    fn effective_active_session_focus_node_id(
        &self,
        rows: &[ActiveSessionSidebarRow],
    ) -> Option<NodeId> {
        let focused_node_id = self.active_session_sidebar_focused_node_id.as_ref()?;
        rows.iter()
            .any(|row| row.node_id == *focused_node_id)
            .then(|| focused_node_id.clone())
    }

    fn active_session_focus_rows(
        &self,
        rows: &[ActiveSessionSidebarRow],
        focused_node_id: Option<&NodeId>,
    ) -> Vec<ActiveSessionSidebarRow> {
        rows.iter()
            .filter(|row| match focused_node_id {
                Some(focused_node_id) => row.parent_id.as_ref() == Some(focused_node_id),
                None => row.parent_id.is_none(),
            })
            .cloned()
            .collect()
    }

    fn active_session_breadcrumb_rows(
        &self,
        rows: &[ActiveSessionSidebarRow],
        focused_node_id: Option<&NodeId>,
    ) -> Vec<ActiveSessionSidebarRow> {
        let row_by_id = rows
            .iter()
            .map(|row| (row.node_id.clone(), row.clone()))
            .collect::<HashMap<_, _>>();
        let mut path = Vec::new();
        let mut current_id = focused_node_id.cloned();
        while let Some(node_id) = current_id {
            let Some(row) = row_by_id.get(&node_id) else {
                break;
            };
            path.push(row.clone());
            current_id = row.parent_id.clone();
        }
        path.reverse();
        path
    }

    pub(super) fn toggle_active_session_sidebar_view(&mut self, cx: &mut Context<Self>) {
        self.active_session_sidebar_view_mode = match self.active_session_sidebar_view_mode {
            ActiveSessionSidebarViewMode::Tree => ActiveSessionSidebarViewMode::Focus,
            ActiveSessionSidebarViewMode::Focus => ActiveSessionSidebarViewMode::Tree,
        };
        // Tauri stores the focus node separately from expansion. Keep native's
        // selected node visible when entering focus mode, but fall back to root
        // if the selected node is stale or has disappeared.
        if self.active_session_sidebar_view_mode == ActiveSessionSidebarViewMode::Focus {
            let rows = self.active_session_sidebar_rows();
            let selected = self
                .active_ssh_node_id
                .as_ref()
                .filter(|node_id| rows.iter().any(|row| row.node_id == **node_id))
                .cloned();
            self.active_session_sidebar_focused_node_id = selected;
        }
        cx.notify();
    }

    fn render_active_session_focus_breadcrumb(
        &self,
        rows: &[ActiveSessionSidebarRow],
        focused_node_id: Option<&NodeId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let path_rows = self.active_session_breadcrumb_rows(rows, focused_node_id);
        let root_active = focused_node_id.is_none();
        let root_color = if root_active {
            theme.accent
        } else {
            theme.text_muted
        };

        let mut breadcrumb = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_card))
            .overflow_hidden();

        breadcrumb = breadcrumb.child(
            div()
                .h(px(22.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .rounded(px(self.tokens.radii.md))
                .px(px(6.0))
                .text_size(px(12.0))
                .font_weight(if root_active {
                    gpui::FontWeight::MEDIUM
                } else {
                    gpui::FontWeight::NORMAL
                })
                .text_color(rgb(root_color))
                .cursor_pointer()
                .hover(move |button| button.bg(rgb(theme.bg_hover)))
                .child(Self::render_lucide_icon(LucideIcon::Home, 14.0, rgb(root_color)))
                .when(root_active, |button| {
                    button.child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "session-focus-breadcrumb-root",
                        "sessions.breadcrumb.all_servers",
                        self.i18n.t("sessions.breadcrumb.all_servers"),
                        root_color,
                        cx,
                    ))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.active_session_sidebar_focused_node_id = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
        );

        let path_len = path_rows.len();
        for (index, row) in path_rows.into_iter().enumerate() {
            let is_last = index + 1 == path_len;
            let text_color = if is_last {
                theme.accent
            } else {
                theme.text_muted
            };
            let node_id = row.node_id.clone();
            let title = row.node_view.title.clone();
            let key = format!("session-focus-breadcrumb-{}", node_id.0);
            breadcrumb = breadcrumb
                .child(Self::render_lucide_icon(
                    LucideIcon::ChevronRight,
                    12.0,
                    rgb(theme.text_muted),
                ))
                .child(
                    div()
                        .max_w(px(120.0))
                        .h(px(22.0))
                        .flex()
                        .items_center()
                        .rounded(px(self.tokens.radii.md))
                        .px(px(6.0))
                        .truncate()
                        .text_size(px(12.0))
                        .font_weight(if is_last {
                            gpui::FontWeight::MEDIUM
                        } else {
                            gpui::FontWeight::NORMAL
                        })
                        .text_color(rgb(text_color))
                        .cursor_pointer()
                        .hover(move |button| button.bg(rgb(theme.bg_hover)))
                        .child(self.render_row_safe_selectable_display_text_in_group(
                            crate::workspace::selectable_text::selectable_text_id(
                                "session-focus-breadcrumb",
                                &node_id,
                            ),
                            &key,
                            "label",
                            0,
                            title,
                            text_color,
                            None,
                            cx,
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.active_session_sidebar_focused_node_id = Some(node_id.clone());
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                );
        }

        breadcrumb.into_any_element()
    }

    fn render_active_session_focus_location_header(
        &self,
        rows: &[ActiveSessionSidebarRow],
        focused_node_id: Option<&NodeId>,
        visible_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let focused_row = focused_node_id
            .and_then(|node_id| rows.iter().find(|row| row.node_id == *node_id));
        let title = focused_row
            .map(|row| row.node_view.title.clone())
            .unwrap_or_else(|| self.i18n.t("sessions.focused_list.all_servers"));
        let title = title.to_uppercase();
        let count_label_key = if visible_count == 1 {
            "sessions.focused_list.child"
        } else {
            "sessions.focused_list.children"
        };
        let count_text = if focused_node_id.is_some() {
            format!("({} {})", visible_count, self.i18n.t(count_label_key))
        } else {
            format!("({})", visible_count)
        }
        .to_uppercase();

        // Tauri FocusedNodeList renders this compact location strip below the
        // breadcrumb (`🏠 All Servers (n)` or `📍 node (n children)`), separate
        // from the sidebar section title above the scroll area.
        div()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | SESSION_FOCUS_DIVIDER_ALPHA))
            .text_size(px(SESSION_TREE_META_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .child(if focused_node_id.is_some() { "📍" } else { "🏠" })
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "session-focus-location-title",
                        if focused_node_id.is_some() {
                            "session-focus-location-node"
                        } else {
                            "sessions.focused_list.all_servers"
                        },
                        title,
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(rgba((theme.text_muted << 8) | 0x80))
                    .child(count_text),
            )
            .into_any_element()
    }

    fn render_active_session_focus_empty(
        &self,
        focused_node_id: Option<&NodeId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let title_key = if focused_node_id.is_some() {
            "sessions.focused_list.no_child_nodes"
        } else {
            "sessions.focused_list.no_servers"
        };
        let subtitle_key = if focused_node_id.is_some() {
            "sessions.focused_list.add_by_drilling"
        } else {
            "sessions.focused_list.click_to_add"
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(32.0))
            .px_4()
            .text_center()
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .mb_2()
                    .child(Self::render_lucide_icon(
                        LucideIcon::Server,
                        SESSION_FOCUS_EMPTY_ICON_SIZE,
                        rgba((theme.text_muted << 8) | SESSION_FOCUS_EMPTY_ICON_ALPHA),
                    )),
            )
            .child(
                div()
                    .text_size(px(SESSION_FOCUS_EMPTY_TITLE_TEXT_SIZE))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "session-focus-empty-title",
                        title_key,
                        self.i18n.t(title_key),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(SESSION_FOCUS_EMPTY_SUBTITLE_TEXT_SIZE))
                    .text_color(rgba(
                        (theme.text_muted << 8)
                            | (SESSION_FOCUS_EMPTY_SUBTITLE_ALPHA * 255.0).round() as u32,
                    ))
                    .child(self.render_display_text_with_role_and_alpha(
                        SelectableTextRole::PlainDocument,
                        "session-focus-empty-subtitle",
                        subtitle_key,
                        self.i18n.t(subtitle_key),
                        theme.text_muted,
                        SESSION_FOCUS_EMPTY_SUBTITLE_ALPHA,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_active_session_focus_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.active_session_sidebar_rows();
        let focused_node_id = self.effective_active_session_focus_node_id(&rows);
        let Some(row) = self
            .active_session_focus_rows(&rows, focused_node_id.as_ref())
            .into_iter()
            .nth(index)
        else {
            return div().into_any_element();
        };
        self.render_active_session_focus_node(row, cx)
    }

    fn render_active_session_focus_node(
        &self,
        row: ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected = self.active_ssh_node_id.as_ref() == Some(&row.node_id);
        let status = self.session_node_status(row.node_view.status());
        let connected = matches!(
            row.node_view.status(),
            ActiveSessionStatus::Active | ActiveSessionStatus::Connected
        );
        let connecting = matches!(row.node_view.status(), ActiveSessionStatus::Connecting);
        let subtitle = format!(
            "{}@{}:{}",
            row.node.config.username, row.node.config.host, row.node.config.port
        );
        let terminal_count = row.node_view.terminal_ids.len();
        let has_children = row.has_children;
        let action_label = self.i18n.t("sessions.actions.connect");
        let selection_group_id = crate::workspace::selectable_text::selectable_text_id(
            "session-focus-card",
            &row.node_id,
        );
        let border_color = if selected {
            rgba((theme.accent << 8) | SESSION_FOCUS_CARD_SELECTED_BORDER_ALPHA)
        } else {
            rgba((theme.border << 8) | SESSION_FOCUS_CARD_BORDER_ALPHA)
        };
        let background = if selected {
            rgba((theme.accent << 8) | SESSION_FOCUS_CARD_SELECTED_BG_ALPHA)
        } else {
            rgba(theme.bg_card << 8)
        };

        let node_id = row.node_id.clone();
        let mut card = div()
            .mx_2()
            .mb_2()
            .p_3()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(border_color)
            .bg(background)
            .cursor_pointer()
            .hover(move |card| card.bg(rgb(theme.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.active_ssh_node_id = Some(node_id.clone());
                    if event.click_count >= 2 && has_children {
                        this.active_session_sidebar_focused_node_id = Some(node_id.clone());
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.render_session_status_dot(status))
                    .child(Self::render_lucide_icon(
                        status.icon,
                        SESSION_TREE_ICON_SIZE,
                        rgb(status.text_color),
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(SESSION_TREE_TEXT_SIZE))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(status.text_color))
                                    .child(self.render_row_safe_selectable_display_text_in_group(
                                        selection_group_id,
                                        "session-focus-card-cell",
                                        "title",
                                        0,
                                        row.node_view.title.clone(),
                                        status.text_color,
                                        None,
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.render_row_safe_selectable_display_text_in_group(
                                        selection_group_id,
                                        "session-focus-card-cell",
                                        "subtitle",
                                        1,
                                        subtitle,
                                        theme.text_muted,
                                        None,
                                        cx,
                                    )),
                            ),
                    )
                    .when(terminal_count > 0, |row_el| {
                        row_el.child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(4.0))
                                .rounded(px(self.tokens.radii.md))
                                .px(px(6.0))
                                .py(px(2.0))
                                .bg(rgba(
                                    (SESSION_FOCUS_EMERALD << 8)
                                        | SESSION_FOCUS_TERMINAL_BADGE_BG_ALPHA,
                                ))
                                .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                                .text_color(rgb(SESSION_FOCUS_EMERALD))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Terminal,
                                    12.0,
                                    rgb(SESSION_FOCUS_EMERALD),
                                ))
                                .child(terminal_count.to_string()),
                        )
                    })
                    .when(has_children, |row_el| {
                        row_el.child(Self::render_lucide_icon(
                            LucideIcon::ChevronRight,
                            16.0,
                            rgb(theme.text_muted),
                        ))
                    })
                    .when(!connected && !connecting, |row_el| {
                        let node_id = row.node_id.clone();
                        let config = row.node.config.clone();
                        let title = row.node.title.clone();
                        let saved_connection_id = row.node.saved_connection_id.clone();
                        row_el.child(
                            div()
                                .rounded(px(self.tokens.radii.md))
                                .px(px(8.0))
                                .py(px(4.0))
                                .text_size(px(11.0))
                                .text_color(rgb(SESSION_FOCUS_EMERALD))
                                .bg(rgba(
                                    (SESSION_FOCUS_EMERALD << 8)
                                        | SESSION_FOCUS_TERMINAL_BADGE_BG_ALPHA,
                                ))
                                .hover(|button| {
                                    button.bg(rgba(
                                        (SESSION_FOCUS_EMERALD << 8)
                                            | SESSION_FOCUS_TERMINAL_BADGE_HOVER_ALPHA,
                                    ))
                                })
                                .child(action_label)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, window, cx| {
                                        let _ = this.queue_ssh_terminal_tab_for_node(
                                            node_id.clone(),
                                            config.clone(),
                                            title.clone(),
                                            saved_connection_id.clone(),
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }),
                                ),
                        )
                    }),
            );

        if selected && terminal_count > 0 {
            card = card.child(
                div()
                    .mt_1()
                    .pt_2()
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | SESSION_FOCUS_DIVIDER_ALPHA))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(row.node_view.terminal_ids.iter().enumerate().map(
                        |(index, session_id)| {
                            self.render_active_session_focus_terminal(
                                *session_id,
                                index + 1,
                                cx,
                            )
                        },
                    )),
            );
        }

        if selected && connected {
            card = card.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .children(self.render_active_session_focus_actions(&row, cx)),
            );
        }

        card.into_any_element()
    }

    fn render_active_session_focus_terminal(
        &self,
        session_id: TerminalSessionId,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_terminal_session_id() == Some(session_id);
        let text_color = if active { theme.accent } else { theme.text_muted };
        let text = self
            .i18n
            .t("sessions.focused_list.terminal")
            .replace("{{number}}", &index.to_string());

        div()
            .h(px(24.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .rounded(px(self.tokens.radii.md))
            .px_2()
            .bg(if active {
                rgba((theme.accent << 8) | SESSION_FOCUS_TERMINAL_ACTIVE_BG_ALPHA)
            } else {
                rgba(theme.bg << 8)
            })
            .text_color(rgb(text_color))
            .hover(move |row| row.bg(rgb(theme.bg_hover)))
            .child(Self::render_lucide_icon(
                LucideIcon::Terminal,
                12.0,
                rgb(text_color),
            ))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                    .child(self.render_row_safe_selectable_display_text_in_group(
                        crate::workspace::selectable_text::selectable_text_id(
                            "session-focus-terminal",
                            session_id,
                        ),
                        "session-focus-terminal-cell",
                        "label",
                        0,
                        text,
                        text_color,
                        None,
                        cx,
                    )),
            )
            .child(
                div()
                    .size(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.md))
                    .child(Self::render_lucide_icon(LucideIcon::X, 12.0, rgb(text_color)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.close_terminal_session(session_id, window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.focus_terminal_session(session_id, window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn open_saved_next_hop_dialog(
        &mut self,
        parent_node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.node_runtime_store.snapshot(&parent_node_id).is_some_and(|snapshot| {
            matches!(snapshot.state.readiness, NodeReadiness::Ready)
        }) {
            self.push_command_palette_toast(
                self.i18n.t("sessions.saved_next_hop.parent_not_ready"),
                None,
                TerminalNoticeVariant::Warning,
            );
            cx.notify();
            return;
        }
        self.prepare_modal_interaction_boundary();
        self.saved_next_hop_dialog = Some(SavedNextHopDialog {
            parent_node_id,
            error: None,
        });
        window.refresh();
        cx.notify();
    }

    fn close_saved_next_hop_dialog(&mut self, cx: &mut Context<Self>) {
        self.saved_next_hop_dialog = None;
        cx.notify();
    }

    fn connect_saved_next_hop(
        &mut self,
        saved_connection_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(parent_node_id) = self
            .saved_next_hop_dialog
            .as_ref()
            .map(|dialog| dialog.parent_node_id.clone())
        else {
            return;
        };
        let Some(connection) = self.connection_store.get(&saved_connection_id).cloned() else {
            if let Some(dialog) = self.saved_next_hop_dialog.as_mut() {
                dialog.error = Some(self.i18n.t("sessions.saved_next_hop.not_found"));
            }
            cx.notify();
            return;
        };
        let title = connection.name.clone();
        let Some(mut config) = ssh_config_from_saved_connection(
            &self.connection_store,
            self.settings_store.settings(),
            &connection,
        ) else {
            if let Some(dialog) = self.saved_next_hop_dialog.as_mut() {
                dialog.error = Some(self.i18n.t("sessions.saved_next_hop.missing_credentials"));
            }
            cx.notify();
            return;
        };
        if let Err(error) = self.prepare_saved_tree_config_for_connect(&mut config) {
            if let Some(dialog) = self.saved_next_hop_dialog.as_mut() {
                dialog.error = Some(error);
            }
            cx.notify();
            return;
        }

        let expansion = match self.expand_saved_connection_tree_under_parent(
            parent_node_id.clone(),
            &saved_connection_id,
            config,
            title.clone(),
        ) {
            Ok(expansion) => expansion,
            Err(error) => {
                if let Some(dialog) = self.saved_next_hop_dialog.as_mut() {
                    dialog.error = Some(error.to_string());
                }
                cx.notify();
                return;
            }
        };
        let Some(target_config) = self
            .node_runtime_store
            .snapshot(&expansion.target_node_id)
            .map(|snapshot| snapshot.config)
        else {
            if let Some(dialog) = self.saved_next_hop_dialog.as_mut() {
                dialog.error = Some(self.i18n.t("sessions.saved_next_hop.materialize_failed"));
            }
            cx.notify();
            return;
        };
        self.expanded_ssh_nodes.insert(parent_node_id);
        for node_id in &expansion.path_node_ids {
            self.expanded_ssh_nodes.insert(node_id.clone());
        }
        self.active_ssh_node_id = Some(expansion.target_node_id.clone());
        self.saved_next_hop_dialog = None;
        let post_connect_command = target_config.post_connect_command.clone();
        if let Err(error) = self.queue_ssh_terminal_tab_for_node_with_mark_used(
            expansion.target_node_id,
            post_connect_command,
            target_config,
            title,
            Some(saved_connection_id.clone()),
            Some(saved_connection_id),
            None,
            window,
            cx,
        ) {
            self.push_command_palette_toast(
                self.i18n.t("sessions.saved_next_hop.materialize_failed"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
        }
    }

    pub(in crate::workspace) fn render_saved_next_hop_dialog(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(dialog) = self.saved_next_hop_dialog.as_ref() else {
            return div().into_any_element();
        };
        let parent_title = self
            .ssh_nodes
            .get(&dialog.parent_node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| dialog.parent_node_id.0.clone());
        let description = self
            .i18n
            .t("sessions.saved_next_hop.description")
            .replace("{{host}}", &parent_title);
        let connections = self.connection_store.connection_infos();

        oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_saved_next_hop_dialog(cx);
                    cx.stop_propagation();
                }),
            )
            .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                div()
                    .w(px(460.0))
                    .max_h(px(560.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .p(px(16.0))
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(self.i18n.t("sessions.saved_next_hop.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(theme.text_muted))
                                    .child(description),
                            ),
                    )
                    .when_some(dialog.error.as_ref(), |panel, error| {
                        panel.child(
                            div()
                                .rounded(px(self.tokens.radii.md))
                                .border_1()
                                .border_color(rgb(theme.error))
                                .bg(rgba((theme.error << 8) | 0x14))
                                .px_3()
                                .py_2()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(theme.error))
                                .child(error.clone()),
                        )
                    })
                    .child(
                        div()
                            .id("saved-next-hop-dialog-scroll")
                            .max_h(px(360.0))
                            .min_h(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .selectable_overflow_y_scroll(
                                &self.selectable_text_scroll_handle(
                                    "saved-next-hop-dialog-scroll",
                                ),
                            )
                            .children(if connections.is_empty() {
                                vec![
                                    div()
                                        .rounded(px(self.tokens.radii.md))
                                        .border_1()
                                        .border_color(rgb(theme.border))
                                        .px_3()
                                        .py_3()
                                        .text_size(px(self.tokens.metrics.ui_text_sm))
                                        .text_color(rgb(theme.text_muted))
                                        .child(self.i18n.t("sessions.saved_next_hop.empty"))
                                        .into_any_element(),
                                ]
                            } else {
                                connections
                                    .into_iter()
                                    .map(|conn| {
                                        let id = conn.id.clone();
                                        let detail =
                                            format!("{}@{}:{}", conn.username, conn.host, conn.port);
                                        let hop_count = conn.proxy_chain.len();
                                        div()
                                            .w_full()
                                            .flex()
                                            .items_center()
                                            .gap(px(10.0))
                                            .rounded(px(self.tokens.radii.md))
                                            .border_1()
                                            .border_color(rgb(theme.border))
                                            .bg(rgb(theme.bg_sunken))
                                            .px_3()
                                            .py_2()
                                            .cursor_pointer()
                                            .hover(|row| row.bg(rgb(self.tokens.ui.bg_hover)))
                                            .child(Self::render_lucide_icon(
                                                LucideIcon::Server,
                                                16.0,
                                                rgb(theme.accent),
                                            ))
                                            .child(
                                                div()
                                                    .min_w(px(0.0))
                                                    .flex_1()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(2.0))
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .text_size(px(
                                                                self.tokens.metrics.ui_text_sm,
                                                            ))
                                                            .font_weight(
                                                                gpui::FontWeight::MEDIUM,
                                                            )
                                                            .text_color(rgb(theme.text))
                                                            .child(conn.name),
                                                    )
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .text_size(px(
                                                                self.tokens.metrics.ui_text_xs,
                                                            ))
                                                            .text_color(rgb(theme.text_muted))
                                                            .child(detail),
                                                    ),
                                            )
                                            .when(hop_count > 0, |row| {
                                                row.child(
                                                    div()
                                                        .flex_shrink_0()
                                                        .rounded_full()
                                                        .border_1()
                                                        .border_color(rgb(theme.border))
                                                        .px_2()
                                                        .py(px(2.0))
                                                        .text_size(px(10.0))
                                                        .text_color(rgb(theme.text_muted))
                                                        .child(
                                                            self.i18n
                                                                .t(
                                                                    "sessions.saved_next_hop.proxy_chain_badge",
                                                                )
                                                                .replace(
                                                                    "{{count}}",
                                                                    &hop_count.to_string(),
                                                                ),
                                                        ),
                                                )
                                            })
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |this, _event, window, cx| {
                                                    this.connect_saved_next_hop(
                                                        id.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                }),
                                            )
                                            .into_any_element()
                                    })
                                    .collect::<Vec<_>>()
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .child(
                                div()
                                    .h(px(32.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(self.tokens.radii.md))
                                    .border_1()
                                    .border_color(rgb(theme.border))
                                    .px_3()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(theme.text))
                                    .cursor_pointer()
                                    .hover(|button| button.bg(rgb(theme.bg_hover)))
                                    .child(self.i18n.t("sessions.saved_next_hop.cancel"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.close_saved_next_hop_dialog(cx);
                                            cx.stop_propagation();
                                        }),
                                    ),
                            ),
                    ),
            ))
            .into_any_element()
    }

    fn render_active_session_focus_actions(
        &self,
        row: &ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let node_id = row.node_id.clone();
        let config = row.node.config.clone();
        let title = row.node.title.clone();
        let saved_connection_id = row.node.saved_connection_id.clone();
        vec![
            self.render_active_session_focus_action_chip(
                LucideIcon::Plus,
                self.i18n.t("sessions.tree.actions.new_terminal"),
                cx.listener(move |this, _event, window, cx| {
                    let _ = this.queue_ssh_terminal_tab_for_node(
                        node_id.clone(),
                        config.clone(),
                        title.clone(),
                        saved_connection_id.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
                cx,
            ),
            {
                let node_id = row.node_id.clone();
                self.render_active_session_focus_action_chip(
                    LucideIcon::FolderOpen,
                    self.i18n.t("sessions.tree.actions.sftp"),
                    cx.listener(move |this, _event, window, cx| {
                        this.open_sftp_tab(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }),
                    cx,
                )
            },
            {
                let node_id = row.node_id.clone();
                self.render_active_session_focus_action_chip(
                    LucideIcon::ArrowLeftRight,
                    self.i18n.t("sessions.tree.actions.port_forwarding"),
                    cx.listener(move |this, _event, window, cx| {
                        this.open_forwards_tab(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }),
                    cx,
                )
            },
            {
                let node_id = row.node_id.clone();
                self.render_active_session_focus_action_chip(
                    LucideIcon::Server,
                    self.i18n.t("sessions.tree.actions.saved_next_hop"),
                    cx.listener(move |this, _event, window, cx| {
                        this.open_saved_next_hop_dialog(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }),
                    cx,
                )
            },
        ]
    }

    fn render_active_session_focus_action_chip(
        &self,
        icon: LucideIcon,
        label: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(24.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(7.0))
            .text_size(px(11.0))
            .text_color(rgb(theme.accent))
            .bg(rgba((theme.accent << 8) | SESSION_FOCUS_ACTION_BG_ALPHA))
            .hover(move |chip| {
                chip.bg(rgba(
                    (theme.accent << 8) | SESSION_FOCUS_ACTION_HOVER_ALPHA,
                ))
            })
            .child(Self::render_lucide_icon(icon, 12.0, rgb(theme.accent)))
            .child(label)
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_active_session_node(
        &self,
        node_id: NodeId,
        node: WorkspaceSshNode,
        node_view: ActiveSessionNode,
        node_depth: usize,
        is_last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.expanded_ssh_nodes.contains(&node_id);
        let selected = self.active_ssh_node_id.as_ref() == Some(&node_id);
        let status = self.session_node_status(node_view.status());
        let terminal_ids = node_view.terminal_ids.clone();
        let mut children = Vec::new();

        if expanded {
            if self.has_active_reconnect_job(&node_id) {
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, _window, cx| {
                        this.cancel_reconnect_for_node(&node_id, cx);
                        cx.stop_propagation();
                    }
                });
                children.push(self.render_session_action_item(
                    node_depth + 1,
                    is_last,
                    LucideIcon::X,
                    self.i18n.t("sessions.tree.actions.cancel_reconnect"),
                    SessionActionVariant::Danger,
                    listener,
                    cx,
                ));
            } else if matches!(
                node_view.status(),
                ActiveSessionStatus::Active | ActiveSessionStatus::Connected
            ) {
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    let config = node.config.clone();
                    let title = node.title.clone();
                    let saved_connection_id = node.saved_connection_id.clone();
                    move |this, _event, window, cx| {
                        let _ = this.queue_ssh_terminal_tab_for_node(
                            node_id.clone(),
                            config.clone(),
                            title.clone(),
                            saved_connection_id.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                });
                children.push(self.render_session_action_item(
                    node_depth + 1,
                    false,
                    LucideIcon::Plus,
                    self.i18n.t("sessions.tree.actions.new_terminal"),
                    SessionActionVariant::Primary,
                    listener,
                    cx,
                ));
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, window, cx| {
                        this.open_sftp_tab(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }
                });
                children.push(self.render_session_action_item(
                    node_depth + 1,
                    false,
                    LucideIcon::FolderOpen,
                    self.i18n.t("sessions.tree.actions.sftp"),
                    SessionActionVariant::Primary,
                    listener,
                    cx,
                ));
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, _window, cx| {
                        // Mirrors Tauri's node-first IDE route: opening IDE creates
                        // an IDE owner surface and remote folder chooser for the
                        // node, not a terminal pane or implicit "/" project.
                        this.open_ide_folder_picker_tab(node_id.clone(), cx);
                        cx.stop_propagation();
                    }
                });
                children.push(self.render_session_action_item(
                    node_depth + 1,
                    false,
                    LucideIcon::Code2,
                    "IDE".to_string(),
                    SessionActionVariant::Primary,
                    listener,
                    cx,
                ));
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, window, cx| {
                        this.open_forwards_tab(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }
                });
                children.push(self.render_session_action_item(
                    node_depth + 1,
                    false,
                    LucideIcon::ArrowLeftRight,
                    self.i18n.t("sessions.tree.actions.port_forwarding"),
                    SessionActionVariant::Primary,
                    listener,
                    cx,
                ));
                if node.saved_connection_id.is_none() {
                    let listener = cx.listener({
                        let node_id = node_id.clone();
                        move |this, _event, window, cx| {
                            this.open_save_runtime_node_form(node_id.clone(), window, cx);
                            cx.stop_propagation();
                        }
                    });
                    children.push(self.render_session_action_item(
                        node_depth + 1,
                        false,
                        LucideIcon::Save,
                        self.i18n.t("sessions.tree.actions.save_as_connection"),
                        SessionActionVariant::Primary,
                        listener,
                        cx,
                    ));
                }
                for (index, session_id) in terminal_ids.iter().copied().enumerate() {
                    children.push(self.render_session_terminal_item(
                        node_depth + 1,
                        false,
                        session_id,
                        index + 1,
                        cx,
                    ));
                }
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, _window, cx| {
                        this.request_disconnect_ssh_node(&node_id, cx);
                        cx.stop_propagation();
                    }
                });
                children.push(self.render_session_action_item(
                    node_depth + 1,
                    false,
                    LucideIcon::WifiOff,
                    self.i18n.t("sessions.tree.actions.disconnect"),
                    SessionActionVariant::Danger,
                    listener,
                    cx,
                ));
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, window, cx| {
                        this.open_drill_down_form(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }
                });
                children.push(self.render_session_action_item(
                    node_depth + 1,
                    false,
                    LucideIcon::ArrowDownRight,
                    self.i18n.t("sessions.tree.actions.drill_in"),
                    SessionActionVariant::Primary,
                    listener,
                    cx,
                ));
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, window, cx| {
                        this.open_saved_next_hop_dialog(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }
                });
                children.push(self.render_session_action_item(
                    node_depth + 1,
                    is_last,
                    LucideIcon::Server,
                    self.i18n.t("sessions.tree.actions.saved_next_hop"),
                    SessionActionVariant::Primary,
                    listener,
                    cx,
                ));
            } else if matches!(node_view.status(), ActiveSessionStatus::Error) {
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    let config = node.config.clone();
                    let title = node.title.clone();
                    let saved_connection_id = node.saved_connection_id.clone();
                    move |this, _event, window, cx| {
                        let _ = this.queue_ssh_terminal_tab_for_node(
                            node_id.clone(),
                            config.clone(),
                            title.clone(),
                            saved_connection_id.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                });
                children.push(self.render_session_action_item(
                    node_depth + 1,
                    is_last,
                    LucideIcon::RefreshCw,
                    self.i18n.t("sessions.actions.reconnect"),
                    SessionActionVariant::Primary,
                    listener,
                    cx,
                ));
            } else if matches!(node_view.status(), ActiveSessionStatus::Idle) {
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    let config = node.config.clone();
                    let title = node.title.clone();
                    let saved_connection_id = node.saved_connection_id.clone();
                    move |this, _event, window, cx| {
                        let _ = this.queue_ssh_terminal_tab_for_node(
                            node_id.clone(),
                            config.clone(),
                            title.clone(),
                            saved_connection_id.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                });
                children.push(self.render_session_action_item(
                    node_depth + 1,
                    is_last,
                    LucideIcon::Power,
                    self.i18n.t("sessions.actions.connect"),
                    SessionActionVariant::Primary,
                    listener,
                    cx,
                ));
            }
        }

        let header = self.render_session_node_header(
            node_id,
            node_view,
            expanded,
            selected,
            status,
            cx,
        );
        let header = if node_depth == 0 {
            header
        } else {
            self.render_session_tree_child(node_depth, is_last && children.is_empty(), header)
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .children(children)
            .into_any_element()
    }

    fn render_session_node_header(
        &self,
        node_id: NodeId,
        node: ActiveSessionNode,
        expanded: bool,
        selected: bool,
        status: SessionStatusStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected_bg = rgba((theme.accent << 8) | 0x1a);
        let selected_border = rgba((theme.accent << 8) | 0x4d);
        let muted_text = rgb(theme.text_muted);
        let row_text = rgb(status.text_color);
        let port_text = format!(":{}", node.port);
        let terminal_count = node.terminal_ids.len();
        let selection_group_id =
            crate::workspace::selectable_text::selectable_text_id("session-sidebar-node", &node_id);

        div()
            .relative()
            .h(px(SESSION_TREE_NODE_HEIGHT))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .rounded(px(self.tokens.radii.md))
            .px_2()
            .cursor_pointer()
            .bg(if selected {
                selected_bg
            } else {
                rgba(theme.bg << 8)
            })
            .border_1()
            .border_color(if selected {
                selected_border
            } else {
                rgba(theme.bg << 8)
            })
            .hover(move |row| row.bg(rgb(theme.bg_hover)))
            .opacity(status.opacity)
            .child(Self::render_lucide_icon(
                if expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                12.0,
                muted_text,
            ))
            .child(div().ml_1().mr_2().child(Self::render_lucide_icon(
                status.icon,
                SESSION_TREE_ICON_SIZE,
                row_text,
            )))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(SESSION_TREE_TEXT_SIZE))
                    .font_weight(if selected {
                        gpui::FontWeight::MEDIUM
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .text_color(row_text)
                    .child(self.render_row_safe_selectable_display_text_in_group(
                        selection_group_id,
                        "session-sidebar-node-cell",
                        "title",
                        0,
                        node.title,
                        status.text_color,
                        None,
                        cx,
                    )),
            )
            .when(node.port != 22, |row| {
                row.child(
                    div()
                        .ml_2()
                        .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                        .text_color(muted_text)
                        .child(self.render_row_safe_selectable_display_text_in_group(
                            selection_group_id,
                            "session-sidebar-node-cell",
                            "port",
                            1,
                            port_text,
                            theme.text_muted,
                            None,
                            cx,
                        )),
                )
            })
            .when(terminal_count > 0, |row| {
                row.child(
                    div()
                        .ml_2()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(2.0))
                        .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                        .text_color(muted_text)
                        .child(Self::render_lucide_icon(
                            LucideIcon::Terminal,
                            12.0,
                            muted_text,
                        ))
                        .child(self.render_row_safe_selectable_display_text_in_group(
                            selection_group_id,
                            "session-sidebar-node-cell",
                            "terminal-count",
                            2,
                            terminal_count.to_string(),
                            theme.text_muted,
                            None,
                            cx,
                        )),
                )
            })
            .child(self.render_session_status_dot(status))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.active_ssh_node_id = Some(node_id.clone());
                    if !this.expanded_ssh_nodes.insert(node_id.clone()) {
                        this.expanded_ssh_nodes.remove(&node_id);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_session_status_dot(&self, status: SessionStatusStyle) -> AnyElement {
        div()
            .ml_2()
            .size(px(if status.ring { 12.0 } else { 8.0 }))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(if status.ring {
                rgba((status.dot_color << 8) | 0x33)
            } else {
                rgba(status.dot_color << 8)
            })
            .child(div().size(px(8.0)).rounded_full().bg(rgb(status.dot_color)))
            .into_any_element()
    }

    fn render_session_terminal_item(
        &self,
        depth: usize,
        line_stops_here: bool,
        session_id: TerminalSessionId,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_terminal_session_id() == Some(session_id);
        let text = self
            .i18n
            .t("sessions.focused_list.terminal")
            .replace("{{number}}", &index.to_string());
        let row_bg = if active {
            rgba((theme.accent << 8) | 0x1a)
        } else {
            rgba(theme.bg << 8)
        };
        let text_color = if active {
            rgb(theme.accent)
        } else {
            rgb(theme.text_muted)
        };

        self.render_session_tree_child(
            depth,
            line_stops_here,
            div()
                .relative()
                .h(px(SESSION_TREE_ITEM_HEIGHT))
                .w_full()
                .ml_1()
                .flex()
                .flex_row()
                .items_center()
                .rounded(px(self.tokens.radii.md))
                .px_2()
                .cursor_pointer()
                .bg(row_bg)
                .hover(move |row| row.bg(rgb(theme.bg_hover)))
                .when(active, |row| {
                    row.child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(5.0))
                            .bottom(px(5.0))
                            .w(px(2.0))
                            .rounded_full()
                            .bg(rgb(theme.accent)),
                    )
                    .pl(px(6.0))
                })
                .child(Self::render_lucide_icon(
                    LucideIcon::Terminal,
                    SESSION_TREE_CHILD_ICON_SIZE,
                    text_color,
                ))
                .child(
                    div()
                        .ml_2()
                        .min_w(px(0.0))
                        .flex_1()
                        .truncate()
                        .text_size(px(SESSION_TREE_TEXT_SIZE))
                        .font_weight(if active {
                            gpui::FontWeight::MEDIUM
                        } else {
                            gpui::FontWeight::NORMAL
                        })
                        .text_color(text_color)
                        .child(self.render_row_safe_selectable_display_text_in_group(
                            crate::workspace::selectable_text::selectable_text_id(
                                "session-sidebar-terminal",
                                session_id,
                            ),
                            "session-sidebar-terminal-cell",
                            "label",
                            0,
                            text,
                            if active {
                                theme.accent
                            } else {
                                theme.text_muted
                            },
                            None,
                            cx,
                        )),
                )
                .child(
                    div()
                        .size(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.md))
                        .opacity(0.0)
                        .hover(|button| button.opacity(1.0))
                        .child(Self::render_lucide_icon(LucideIcon::X, 12.0, text_color))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, window, cx| {
                                this.close_terminal_session(session_id, window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        this.focus_terminal_session(session_id, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
        )
    }

    fn render_session_action_item(
        &self,
        depth: usize,
        line_stops_here: bool,
        icon: LucideIcon,
        label: String,
        variant: SessionActionVariant,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (text_color, hover_bg) = match variant {
            SessionActionVariant::Primary => (theme.accent, theme.bg_hover),
            SessionActionVariant::Danger => (0xef4444, mix_rgb(theme.bg_hover, 0xef4444, 0.10)),
        };
        let selection_group_id = crate::workspace::selectable_text::selectable_text_id(
            "session-sidebar-action",
            (depth, line_stops_here, label.as_str()),
        );

        self.render_session_tree_child(
            depth,
            line_stops_here,
            div()
                .h(px(SESSION_TREE_ITEM_HEIGHT))
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .rounded(px(self.tokens.radii.md))
                .px_2()
                .text_size(px(SESSION_TREE_TEXT_SIZE))
                .text_color(rgb(text_color))
                .cursor_pointer()
                .hover(move |row| row.bg(rgb(hover_bg)))
                .child(Self::render_lucide_icon(
                    icon,
                    SESSION_TREE_CHILD_ICON_SIZE,
                    rgb(text_color),
                ))
                .child(div().truncate().child(
                    self.render_row_safe_selectable_display_text_in_group(
                        selection_group_id,
                        "session-sidebar-action-cell",
                        "label",
                        0,
                        label,
                        text_color,
                        None,
                        cx,
                    ),
                ))
                .on_mouse_down(MouseButton::Left, listener)
                .into_any_element(),
        )
    }

    fn render_session_tree_child(
        &self,
        depth: usize,
        line_stops_here: bool,
        child: AnyElement,
    ) -> AnyElement {
        tree_child(
            &self.tokens,
            TreeBranchMetrics::tauri_session_tree(),
            depth,
            line_stops_here,
            child,
        )
    }

    fn session_node_status(&self, status: ActiveSessionStatus) -> SessionStatusStyle {
        match status {
            ActiveSessionStatus::Connecting => SessionStatusStyle {
                icon: LucideIcon::LoaderCircle,
                text_color: 0x3b82f6,
                dot_color: 0x3b82f6,
                opacity: 1.0,
                ring: false,
            },
            ActiveSessionStatus::Active => SessionStatusStyle {
                icon: LucideIcon::Server,
                text_color: 0x059669,
                dot_color: 0x10b981,
                opacity: 1.0,
                ring: true,
            },
            ActiveSessionStatus::Connected => SessionStatusStyle {
                icon: LucideIcon::Server,
                text_color: 0x10b981,
                dot_color: 0x10b981,
                opacity: 1.0,
                ring: true,
            },
            ActiveSessionStatus::Error => SessionStatusStyle {
                icon: LucideIcon::WifiOff,
                text_color: 0xef4444,
                dot_color: 0xef4444,
                opacity: 1.0,
                ring: false,
            },
            ActiveSessionStatus::Idle => SessionStatusStyle {
                icon: LucideIcon::Server,
                text_color: self.tokens.ui.text_muted,
                dot_color: self.tokens.ui.text_muted,
                opacity: 0.7,
                ring: false,
            },
        }
    }
}
