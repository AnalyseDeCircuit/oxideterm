impl WorkspaceApp {
    fn render_active_sessions_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut node_views = self
            .ssh_nodes
            .iter()
            .map(|(node_id, node)| ActiveSessionNode {
                id: node_id.0.clone(),
                title: node.title.clone(),
                port: node.config.port,
                terminal_ids: node.terminal_ids.clone(),
                readiness: active_session_readiness(&node.readiness),
            })
            .collect::<Vec<_>>();
        sort_active_session_nodes(&mut node_views);

        if node_views.is_empty() {
            return self.render_empty_sessions_sidebar_content();
        }
        let node_count = node_views.len();

        div()
            .id("active-sessions-sidebar-scroll")
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .overflow_y_scroll()
            .px_1()
            .children(
                node_views
                    .into_iter()
                    .enumerate()
                    .filter_map(|(index, node_view)| {
                        let node_id = NodeId::new(node_view.id.clone());
                        let node = self.ssh_nodes.get(&node_id)?.clone();
                        let is_last = index + 1 == node_count;
                        Some(self.render_active_session_node(node_id, node, node_view, is_last, cx))
                    }),
            )
            .into_any_element()
    }

    fn render_active_session_node(
        &self,
        node_id: NodeId,
        node: WorkspaceSshNode,
        node_view: ActiveSessionNode,
        is_last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.expanded_ssh_nodes.contains(&node_id);
        let selected = self.active_ssh_node_id.as_ref() == Some(&node_id);
        let status = self.session_node_status(node_view.status());
        let terminal_ids = node_view.terminal_ids.clone();
        let mut children = Vec::new();

        if expanded {
            if matches!(
                node_view.status(),
                ActiveSessionStatus::Active
                    | ActiveSessionStatus::Connected
                    | ActiveSessionStatus::Connecting
            ) {
                children.push(self.render_session_action_item(
                    1,
                    false,
                    LucideIcon::Plus,
                    self.i18n.t("sessions.tree.actions.new_terminal"),
                    SessionActionVariant::Primary,
                    cx.listener({
                        let node_id = node_id.clone();
                        let config = node.config.clone();
                        let title = node.title.clone();
                        let saved_connection_id = node.saved_connection_id.clone();
                        move |this, _event, window, cx| {
                            let _ = this.create_ssh_terminal_tab_for_node(
                                config.clone(),
                                title.clone(),
                                saved_connection_id.clone(),
                                Some(node_id.clone()),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }
                    }),
                ));
                children.push(self.render_session_action_item(
                    1,
                    false,
                    LucideIcon::FolderInput,
                    self.i18n.t("sessions.tree.actions.sftp"),
                    SessionActionVariant::Primary,
                    cx.listener({
                        let node_id = node_id.clone();
                        move |this, _event, window, cx| {
                            this.open_sftp_tab(node_id.clone(), window, cx);
                            cx.stop_propagation();
                        }
                    }),
                ));
                children.push(self.render_session_action_item(
                    1,
                    false,
                    LucideIcon::ArrowLeftRight,
                    self.i18n.t("sessions.tree.actions.port_forwarding"),
                    SessionActionVariant::Primary,
                    cx.listener({
                        let node_id = node_id.clone();
                        move |this, _event, window, cx| {
                            this.open_forwards_tab(node_id.clone(), window, cx);
                            cx.stop_propagation();
                        }
                    }),
                ));
                for (index, session_id) in terminal_ids.iter().copied().enumerate() {
                    children.push(self.render_session_terminal_item(
                        1,
                        false,
                        session_id,
                        index + 1,
                        cx,
                    ));
                }
                children.push(self.render_session_action_item(
                    1,
                    is_last,
                    LucideIcon::WifiOff,
                    self.i18n.t("sessions.tree.actions.disconnect"),
                    SessionActionVariant::Danger,
                    cx.listener({
                        let node_id = node_id.clone();
                        move |this, _event, window, cx| {
                            this.disconnect_ssh_node(&node_id, window, cx);
                            cx.stop_propagation();
                        }
                    }),
                ));
            } else if matches!(
                node_view.status(),
                ActiveSessionStatus::Error | ActiveSessionStatus::Idle
            ) {
                children.push(self.render_session_action_item(
                    1,
                    is_last,
                    LucideIcon::Play,
                    self.i18n.t("sessions.tree.actions.reconnect"),
                    SessionActionVariant::Primary,
                    cx.listener({
                        let node_id = node_id.clone();
                        let config = node.config.clone();
                        let title = node.title.clone();
                        let saved_connection_id = node.saved_connection_id.clone();
                        move |this, _event, window, cx| {
                            let _ = this.create_ssh_terminal_tab_for_node(
                                config.clone(),
                                title.clone(),
                                saved_connection_id.clone(),
                                Some(node_id.clone()),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }
                    }),
                ));
            }
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                self.render_session_node_header(node_id, node_view, expanded, selected, status, cx),
            )
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
                    .child(node.title),
            )
            .when(node.port != 22, |row| {
                row.child(
                    div()
                        .ml_2()
                        .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                        .text_color(muted_text)
                        .child(port_text),
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
                        .child(terminal_count.to_string()),
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
                        .child(text),
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
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (text_color, hover_bg) = match variant {
            SessionActionVariant::Primary => (theme.accent, theme.bg_hover),
            SessionActionVariant::Danger => (0xef4444, mix_rgb(theme.bg_hover, 0xef4444, 0.10)),
        };

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
                .child(div().truncate().child(label))
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
