#[derive(Clone)]
struct ActiveSessionSidebarRow {
    node_id: NodeId,
    node: WorkspaceSshNode,
    node_view: ActiveSessionNode,
    depth: usize,
    is_last: bool,
}

impl WorkspaceApp {
    fn render_active_sessions_sidebar_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.active_session_sidebar_rows();
        if rows.is_empty() {
            return self.render_empty_sessions_sidebar_content(cx);
        }

        self.sync_active_session_sidebar_list_state(&rows);
        let state = self.active_session_sidebar_list_state.clone();
        let spec = self.active_session_sidebar_list_spec();
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

    fn active_session_sidebar_rows(&self) -> Vec<ActiveSessionSidebarRow> {
        let mut tree_nodes = self.node_router.flatten_tree();
        let tree_node_ids = tree_nodes
            .iter()
            .map(|node| NodeId::new(node.id.clone()))
            .collect::<std::collections::HashSet<_>>();
        let mut orphan_node_views = self
            .ssh_nodes
            .iter()
            .filter(|(node_id, _)| !tree_node_ids.contains(*node_id))
            .map(|(node_id, node)| ActiveSessionNode {
                id: node_id.0.clone(),
                title: node.title.clone(),
                port: node.config.port,
                terminal_ids: node.terminal_ids.clone(),
                readiness: active_session_readiness(&node.readiness),
            })
            .collect::<Vec<_>>();
        sort_active_session_nodes(&mut orphan_node_views);

        let mut rows = tree_nodes
            .drain(..)
            .filter_map(|flat_node| {
                let node_id = NodeId::new(flat_node.id.clone());
                let node = self.ssh_nodes.get(&node_id)?.clone();
                let node_view = ActiveSessionNode {
                    id: flat_node.id,
                    title: node.title.clone(),
                    port: flat_node.port,
                    terminal_ids: node.terminal_ids.clone(),
                    readiness: active_session_readiness(&node.readiness),
                };
                Some(ActiveSessionSidebarRow {
                    node_id,
                    node,
                    node_view,
                    depth: flat_node.depth as usize,
                    is_last: flat_node.is_last_child,
                })
            })
            .collect::<Vec<_>>();
        let orphan_count = orphan_node_views.len();
        rows.extend(
            orphan_node_views
                .into_iter()
                .enumerate()
                .filter_map(|(index, node_view)| {
                    let node_id = NodeId::new(node_view.id.clone());
                    let node = self.ssh_nodes.get(&node_id)?.clone();
                    Some(ActiveSessionSidebarRow {
                        node_id,
                        node,
                        node_view,
                        depth: 0,
                        is_last: index + 1 == orphan_count,
                    })
                }),
        );
        rows
    }

    fn sync_active_session_sidebar_list_state(&mut self, rows: &[ActiveSessionSidebarRow]) {
        let signatures = rows
            .iter()
            .map(|row| self.active_session_sidebar_row_signature(row))
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.active_session_sidebar_list_state,
            &mut self.active_session_sidebar_list_cache.borrow_mut(),
            "active-sessions-sidebar",
            &signatures,
            self.active_session_sidebar_list_spec(),
        );
    }

    fn active_session_sidebar_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(ACTIVE_SESSION_SIDEBAR_LIST_ESTIMATED_HEIGHT),
            ACTIVE_SESSION_SIDEBAR_LIST_OVERSCAN,
        )
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

    fn active_session_sidebar_row_signature(&self, row: &ActiveSessionSidebarRow) -> u64 {
        let mut hasher = DefaultHasher::new();
        // This virtual row owns the node header plus expanded action/terminal
        // children. Hash all state that can change its visible height or labels.
        row.node_id.hash(&mut hasher);
        row.node.title.hash(&mut hasher);
        row.node.config.port.hash(&mut hasher);
        row.node.terminal_ids.hash(&mut hasher);
        row.node_view.title.hash(&mut hasher);
        row.node_view.terminal_ids.hash(&mut hasher);
        format!("{:?}", row.node_view.status()).hash(&mut hasher);
        row.depth.hash(&mut hasher);
        row.is_last.hash(&mut hasher);
        self.expanded_ssh_nodes.contains(&row.node_id).hash(&mut hasher);
        self.has_active_reconnect_job(&row.node_id).hash(&mut hasher);
        (self.active_ssh_node_id.as_ref() == Some(&row.node_id)).hash(&mut hasher);
        hasher.finish()
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
                    move |this, _event, window, cx| {
                        this.disconnect_ssh_node(&node_id, window, cx);
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
                    is_last,
                    LucideIcon::ArrowDownRight,
                    self.i18n.t("sessions.tree.actions.drill_in"),
                    SessionActionVariant::Primary,
                    listener,
                    cx,
                ));
            } else if matches!(
                node_view.status(),
                ActiveSessionStatus::Error | ActiveSessionStatus::Idle
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
                    is_last,
                    LucideIcon::Play,
                    self.i18n.t("sessions.tree.actions.reconnect"),
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
