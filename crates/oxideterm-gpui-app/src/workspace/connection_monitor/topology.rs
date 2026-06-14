use oxideterm_gpui_ui::modal::rounded_shell_child_radius;

impl WorkspaceApp {
    pub(super) fn render_topology_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .p(px(24.0))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .child(
                        div()
                            .mb_2()
                            .text_size(px(24.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(theme.text_heading))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "topology-page-header",
                                "title",
                                self.i18n.t("topology.page.title"),
                                theme.text_heading,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "topology-page-header",
                                "description",
                                self.i18n.t("topology.page.description"),
                                theme.text_muted,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .relative()
                    .overflow_hidden()
                    .child(self.render_connection_topology(cx)),
            )
            .into_any_element()
    }

    pub(super) fn render_connection_runtime_topology(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .id("connection-runtime-topology")
            .flex_1()
            .min_h_0()
            .relative()
            .overflow_hidden()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            // Runtime already labels this section as Connection Matrix, so the
            // embedded graph starts directly at the canvas.
            .child(self.render_connection_topology(cx))
            .into_any_element()
    }

    fn format_connection_pool_time(&self, time: SystemTime) -> String {
        let elapsed = SystemTime::now()
            .duration_since(time)
            .unwrap_or(Duration::from_secs(0));
        let diff_mins = elapsed.as_secs() / 60;
        if diff_mins < 1 {
            return self.i18n.t("connections.time.just_now");
        }
        if diff_mins < 60 {
            return self
                .i18n
                .t("connections.time.mins_ago")
                .replace("{{count}}", &diff_mins.to_string());
        }
        let diff_hours = diff_mins / 60;
        if diff_hours < 24 {
            return self
                .i18n
                .t("connections.time.hrs_ago")
                .replace("{{count}}", &diff_hours.to_string());
        }

        let date: chrono::DateTime<chrono::Local> = time.into();
        date.format("%Y-%m-%d").to_string()
    }

    fn render_connection_pool_monitor(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        if let Some(error) = &self.connection_monitor.pool_error {
            return monitor_center_state(
                self,
                LucideIcon::AlertTriangle,
                MONITOR_RED,
                error.clone(),
                cx,
            );
        }
        let Some(stats) = self.connection_monitor.pool_stats.as_ref() else {
            return monitor_center_state(
                self,
                LucideIcon::RefreshCw,
                theme.text_muted,
                self.i18n.t("connections.monitor.loading"),
                cx,
            );
        };

        let idle_timeout_label = if stats.idle_timeout_secs == 0 {
            self.i18n.t("connections.monitor.idle_timeout_never")
        } else {
            self.i18n
                .t("connections.monitor.idle_timeout")
                .replace("{{min}}", &(stats.idle_timeout_secs / 60).to_string())
        };
        let capacity = if stats.pool_capacity == 0 {
            "∞".to_string()
        } else {
            stats.pool_capacity.to_string()
        };
        let capacity_label = self
            .i18n
            .t("connections.monitor.capacity")
            .replace("{{capacity}}", &capacity);

        div()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "topology-monitor-header",
                                "title",
                                self.i18n.t("connections.monitor.title"),
                                theme.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(12.0))
                            .text_color(rgb(theme.text_muted))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Clock,
                                14.0,
                                rgb(theme.text_muted),
                            ))
                            .child(idle_timeout_label)
                            .child("•")
                            .child(capacity_label),
                    ),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(4)
                    .gap_2()
                    .child(self.render_pool_stat_card(
                        self.i18n.t("connections.monitor.active"),
                        stats.active_connections,
                        LucideIcon::Activity,
                        if stats.active_connections > 0 {
                            MONITOR_EMERALD_DARK
                        } else {
                            theme.text_muted
                        },
                        cx,
                    ))
                    .child(self.render_pool_stat_card(
                        self.i18n.t("connections.monitor.idle"),
                        stats.idle_connections,
                        LucideIcon::Link2,
                        if stats.idle_connections > 0 {
                            MONITOR_BLUE
                        } else {
                            theme.text_muted
                        },
                        cx,
                    ))
                    .child(self.render_pool_stat_card(
                        self.i18n.t("connections.monitor.reconnecting"),
                        stats.reconnecting_connections,
                        LucideIcon::RefreshCw,
                        if stats.reconnecting_connections > 0 {
                            MONITOR_AMBER
                        } else {
                            theme.text_muted
                        },
                        cx,
                    ))
                    .child(self.render_pool_stat_card(
                        self.i18n.t("connections.monitor.link_down"),
                        stats.link_down_connections,
                        LucideIcon::AlertTriangle,
                        if stats.link_down_connections > 0 {
                            MONITOR_RED
                        } else {
                            theme.text_muted
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap_2()
                    .child(self.render_pool_stat_card(
                        self.i18n.t("connections.monitor.terminals"),
                        stats.total_terminals,
                        LucideIcon::Terminal,
                        if stats.total_terminals > 0 {
                            MONITOR_EMERALD_DARK
                        } else {
                            theme.text_muted
                        },
                        cx,
                    ))
                    .child(self.render_pool_stat_card(
                        self.i18n.t("connections.monitor.sftp"),
                        stats.total_sftp_sessions,
                        LucideIcon::FolderSync,
                        if stats.total_sftp_sessions > 0 {
                            MONITOR_BLUE
                        } else {
                            theme.text_muted
                        },
                        cx,
                    ))
                    .child(self.render_pool_stat_card(
                        self.i18n.t("connections.monitor.forwards"),
                        stats.total_forwards,
                        LucideIcon::ArrowLeftRight,
                        if stats.total_forwards > 0 {
                            MONITOR_BLUE
                        } else {
                            theme.text_muted
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt_3()
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
                    .text_size(px(12.0))
                    .text_color(rgb(theme.text_muted))
                    .child(
                        self.i18n
                            .t("connections.monitor.summary")
                            .replace("{{total}}", &stats.total_connections.to_string())
                            .replace("{{refs}}", &stats.total_ref_count.to_string()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(Self::render_lucide_icon(
                                LucideIcon::RefreshCw,
                                12.0,
                                rgb(theme.text_muted),
                            ))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "topology-monitor-header",
                                "live",
                                self.i18n.t("connections.monitor.live"),
                                theme.text_muted,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_pool_stat_card(
        &self,
        label: String,
        value: usize,
        icon: LucideIcon,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let background = if color == theme.text_muted {
            rgba((theme.bg_hover << 8) | 0x4d)
        } else {
            rgba((color << 8) | MONITOR_TINT_ALPHA)
        };
        div()
            .rounded(px(self.tokens.radii.lg))
            .bg(background)
            .p_3()
            .shadow_sm()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Self::render_lucide_icon(icon, 16.0, rgb(color)))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_display_text(
                                "connection-pool-stat-label",
                                &label,
                                label.clone(),
                                theme.text_muted,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .mt_1()
                    .flex()
                    .items_baseline()
                    .gap_1()
                    .text_size(px(24.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(color))
                    .child(self.render_selectable_display_text(
                        "connection-pool-stat-value",
                        &label,
                        value.to_string(),
                        color,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_connection_topology(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(snapshot) = self.connection_monitor.topology_snapshot.as_ref() else {
            return monitor_center_state(
                self,
                LucideIcon::RefreshCw,
                theme.text_muted,
                self.i18n.t("connections.monitor.loading"),
                cx,
            );
        };
        let layout = ConnectionTopologyLayout::from_snapshot(snapshot);
        if layout.nodes.is_empty() {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .text_color(rgb(theme.text_muted))
                .child(
                    div()
                        .text_size(px(18.0))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "topology-empty",
                            "title",
                            self.i18n.t("topology.page.no_connections"),
                            theme.text_muted,
                            cx,
                        )),
                )
                .child(
                    div()
                        .mt_2()
                        .text_size(px(14.0))
                        .opacity(0.7)
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "topology-empty",
                            "hint",
                            self.i18n.t("topology.page.connect_hint"),
                            theme.text_muted,
                            cx,
                        )),
                )
                .into_any_element();
        }

        let edges = layout.edges.clone();
        let transform = self.connection_monitor.topology_transform;
        let mut graph = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(rgb(theme.bg))
            .rounded(px(self.tokens.radii.lg))
            .cursor(if self.connection_monitor.topology_drag.is_some() {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, cx| {
                let zoom_changed = this.zoom_topology_graph(event);
                let menu_changed = this.connection_monitor.dismiss_topology_menu();
                cx.stop_propagation();
                if zoom_changed || menu_changed {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    this.connection_monitor.dismiss_topology_menu();
                    this.connection_monitor.topology_drag = Some(TopologyDragState {
                        last_x: f32::from(event.position.x),
                        last_y: f32::from(event.position.y),
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.pan_topology_graph(event) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.connection_monitor.topology_drag.take().is_some() {
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        window.paint_quad(fill(bounds.clone(), rgb(theme.bg)));
                        let mut y = 0.0;
                        while y <= f32::from(bounds.size.height) {
                            let mut x = 0.0;
                            while x <= f32::from(bounds.size.width) {
                                let dot_bounds = gpui::Bounds::new(
                                    point(bounds.origin.x + px(x), bounds.origin.y + px(y)),
                                    gpui::size(px(1.0), px(1.0)),
                                );
                                window.paint_quad(fill(
                                    dot_bounds,
                                    rgba((theme.text_muted << 8) | TOPOLOGY_BG_GRID_ALPHA),
                                ));
                                x += TOPOLOGY_BG_GRID_STEP;
                            }
                            y += TOPOLOGY_BG_GRID_STEP;
                        }

                        for edge in &edges {
                            let start = point(
                                bounds.origin.x
                                    + px(topology_transform_x(edge.source_x, transform)),
                                bounds.origin.y
                                    + px(topology_transform_y(
                                        edge.source_y + TOPOLOGY_NODE_HEIGHT / 2.0,
                                        transform,
                                    )),
                            );
                            let end = point(
                                bounds.origin.x
                                    + px(topology_transform_x(edge.target_x, transform)),
                                bounds.origin.y
                                    + px(topology_transform_y(
                                        edge.target_y - TOPOLOGY_NODE_HEIGHT / 2.0,
                                        transform,
                                    )),
                            );
                            let delta_y = edge.target_y - edge.source_y;
                            let control_a = point(
                                bounds.origin.x
                                    + px(topology_transform_x(edge.source_x, transform)),
                                bounds.origin.y
                                    + px(topology_transform_y(
                                        edge.source_y + delta_y * 0.4,
                                        transform,
                                    )),
                            );
                            let control_b = point(
                                bounds.origin.x
                                    + px(topology_transform_x(edge.target_x, transform)),
                                bounds.origin.y
                                    + px(topology_transform_y(
                                        edge.target_y - delta_y * 0.4,
                                        transform,
                                    )),
                            );

                            if edge.active {
                                let mut glow = PathBuilder::stroke(px(6.0 * transform.k));
                                glow.move_to(start);
                                glow.cubic_bezier_to(end, control_a, control_b);
                                if let Ok(path) = glow.build() {
                                    window.paint_path(
                                        path,
                                        rgba(
                                            (topology_view_status_color(edge.source_status) << 8)
                                                | TOPOLOGY_LINE_GLOW_ALPHA,
                                        ),
                                    );
                                }
                            }

                            let mut line =
                                PathBuilder::stroke(px(
                                    if edge.active { 2.5 } else { 1.5 } * transform.k
                                ));
                            line.move_to(start);
                            line.cubic_bezier_to(end, control_a, control_b);
                            if let Ok(path) = line.build() {
                                window.paint_path(
                                    path,
                                    rgba(
                                        (topology_view_status_color(edge.source_status) << 8)
                                            | if edge.active {
                                                0xff
                                            } else {
                                                TOPOLOGY_LINE_INACTIVE_ALPHA
                                            },
                                    ),
                                );
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .top(px(16.0))
                    .right(px(16.0))
                    .px_2()
                    .py(px(4.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgba((theme.bg_panel << 8) | 0xcc))
                    .text_size(px(12.0))
                    .font_family("monospace")
                    .text_color(rgb(theme.text_muted))
                    .shadow_sm()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "topology-zoom-chip",
                        "scale",
                        format!("{}%", (transform.k * 100.0).round() as i32),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(16.0))
                    .left(px(16.0))
                    .text_size(px(10.0))
                    .font_family("monospace")
                    .text_color(rgba(
                        (theme.text_muted << 8) | TOPOLOGY_INSTRUCTION_ALPHA_60,
                    ))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "topology-instructions",
                        "controls",
                        self.i18n.t("topology.controls.instructions"),
                        theme.text_muted,
                        cx,
                    )),
            );

        for node in layout.nodes {
            graph = graph.child(self.render_topology_graph_node(node, transform, cx));
        }

        if let Some(menu) = self.connection_monitor.topology_menu.clone() {
            // Topology node actions are a context menu, not a graph child popover:
            // keep outside pointer and Esc dismissal on the same workspace menu
            // owner as FileManager/SFTP/session menus.
            graph = graph.child(
                self.workspace_context_menu_backdrop(
                    self.render_topology_node_action_menu(menu, cx),
                    cx,
                ),
            );
        }

        div()
            .size_full()
            .overflow_hidden()
            .bg(rgb(theme.bg))
            .child(graph)
            .into_any_element()
    }

    fn render_topology_graph_node(
        &self,
        node: TopologyLayoutNode,
        transform: TopologyTransform,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let status_color = topology_view_status_color(node.view_status);
        let is_down = node.view_status.is_down();
        let is_connecting = node.view_status.is_connecting();
        let scale = transform.k;
        let left = topology_transform_x(node.x, transform) - (TOPOLOGY_NODE_WIDTH * scale / 2.0);
        let top = topology_transform_y(node.y, transform) - (TOPOLOGY_NODE_HEIGHT * scale / 2.0);
        let connected_shadow = if node.view_status.is_connected() {
            vec![gpui::BoxShadow {
                color: rgba((status_color << 8) | 0x30).into(),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(15.0),
                spread_radius: px(0.0),
            }]
        } else {
            Vec::new()
        };

        // Mirrors TopologyViewEnhanced NodeCard: fixed 140x50 glass panel with centered
        // status dot, semibold 11px name, and 9px mono host line.
        div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(TOPOLOGY_NODE_WIDTH * scale))
            .h(px(TOPOLOGY_NODE_HEIGHT * scale))
            .rounded(px(self.tokens.radii.lg * scale))
            .border_1()
            .border_color(if is_down {
                rgba((TOPOLOGY_FAILED << 8) | 0x66)
            } else {
                rgba((theme.border << 8) | TOPOLOGY_PANEL_BORDER_ALPHA_50)
            })
            .bg(rgba((theme.bg_panel << 8) | TOPOLOGY_PANEL_BG_ALPHA_20))
            .shadow(connected_shadow)
            .cursor_pointer()
            .hover(|style| {
                style
                    .border_color(rgba((theme.accent << 8) | TOPOLOGY_PANEL_BORDER_ALPHA_50))
                    .shadow(vec![gpui::BoxShadow {
                        color: rgba((theme.accent << 8) | 0x26).into(),
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(20.0),
                        spread_radius: px(0.0),
                    }])
            })
            .child(
                div()
                    .size_full()
                    .relative()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0 * scale))
                            .mb(px(2.0 * scale))
                            .child(
                                div()
                                    .w(px(8.0 * scale))
                                    .h(px(8.0 * scale))
                                    .rounded_full()
                                    .bg(rgb(status_color))
                                    .when(is_down || is_connecting, |dot| {
                                        dot.shadow(vec![gpui::BoxShadow {
                                            color: rgba((status_color << 8) | 0x66).into(),
                                            offset: point(px(0.0), px(0.0)),
                                            blur_radius: px(8.0),
                                            spread_radius: px(0.0),
                                        }])
                                    }),
                            )
                            .child(
                                div()
                                    .max_w(px(100.0 * scale))
                                    .truncate()
                                    .text_size(px(11.0 * scale))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text))
                                    .child(node.name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .max_w(px(120.0 * scale))
                            .truncate()
                            .font_family("monospace")
                            .text_size(px(9.0 * scale))
                            .text_color(rgba(
                                (theme.text_muted << 8) | TOPOLOGY_MUTED_TEXT_ALPHA_70,
                            ))
                            .child(node.host.clone()),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let node = node.clone();
                    move |this, event: &MouseDownEvent, window, cx| {
                        if event.click_count >= 2 {
                            this.open_topology_node_menu(&node, window);
                        }
                        this.connection_monitor.topology_drag = None;
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .into_any_element()
    }

    fn render_topology_node_action_menu(
        &self,
        menu: TopologyNodeMenuState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let is_connected = menu.view_status.is_connected();
        let node_id = menu.node_id.clone();
        let menu_key = menu
            .node_id
            .as_ref()
            .map(|node_id| node_id.0.as_str())
            .unwrap_or("unknown")
            .to_string();

        let mut actions = div().py(px(4.0)).child(self.render_topology_menu_action(
            LucideIcon::ExternalLink,
            theme.accent,
            self.i18n.t("topology.menu.navigate_session"),
            false,
            false,
            {
                let node_id = node_id.clone();
                move |this, _event, _window, _cx| {
                    if let Some(node_id) = node_id.clone() {
                        this.active_ssh_node_id = Some(node_id);
                        this.active_sidebar_section = SidebarSection::Sessions;
                    }
                }
            },
            cx,
        ));

        if is_connected {
            actions = actions
                .child(self.render_topology_menu_action(
                    LucideIcon::Terminal,
                    MONITOR_EMERALD_DARK,
                    self.i18n.t("topology.menu.new_terminal"),
                    false,
                    false,
                    {
                        let node_id = node_id.clone();
                        move |this, _event, window, cx| {
                            if let Some(node_id) = node_id.clone()
                                && let Some(node) = this.ssh_nodes.get(&node_id).cloned()
                            {
                                let _ = this.queue_ssh_terminal_tab_for_node(
                                    node_id,
                                    node.config,
                                    node.title,
                                    node.saved_connection_id,
                                    window,
                                    cx,
                                );
                            }
                        }
                    },
                    cx,
                ))
                .child(self.render_topology_menu_action(
                    LucideIcon::FolderOpen,
                    0xeab308,
                    self.i18n.t("topology.menu.open_sftp"),
                    false,
                    false,
                    {
                        let node_id = node_id.clone();
                        move |this, _event, window, _cx| {
                            if let Some(node_id) = node_id.clone() {
                                this.open_sftp_tab(node_id, window, _cx);
                            }
                        }
                    },
                    cx,
                ));
        }

        context_menu_event_boundary(div()
            .absolute()
            .left(px(menu.x))
            .top(px(menu.y))
            .min_w(px(TOPOLOGY_MENU_WIDTH))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgba((theme.bg_elevated << 8) | 0xf2))
            .shadow_lg())
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | TOPOLOGY_PANEL_BORDER_ALPHA_50))
                    // Match Tauri menu clipping: the header paints at the top
                    // edge but must still follow the rounded shell.
                    .rounded_t(px(rounded_shell_child_radius(self.tokens.radii.lg)))
                    .bg(rgba((theme.bg << 8) | 0x80))
                    .child(
                        div()
                            .max_w(px(TOPOLOGY_MENU_WIDTH - 24.0))
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "topology-menu-title",
                                (menu_key.as_str(), "name"),
                                menu.name,
                                theme.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "topology-menu-host",
                                (menu_key.as_str(), "host"),
                                menu.host,
                                theme.text_muted,
                                cx,
                            )),
                    ),
            )
            .child(actions)
            .child(
                div()
                    .px_3()
                    .py(px(6.0))
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | TOPOLOGY_PANEL_BORDER_ALPHA_50))
                    // Footer paint is flush with the popover bottom; keep it
                    // inside the same rounded menu boundary as the browser UI.
                    .rounded_b(px(rounded_shell_child_radius(self.tokens.radii.lg)))
                    .bg(rgba((theme.bg << 8) | 0x4d))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(10.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "topology-menu-close-hint",
                        "label",
                        self.i18n.t("topology.menu.close_hint"),
                        theme.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_topology_menu_action(
        &self,
        icon: LucideIcon,
        icon_color: u32,
        label: String,
        disabled: bool,
        loading: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label_key = label.clone();
        let item = div()
            .w_full()
            .px_3()
            .py_2()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(14.0))
            .text_color(rgb(theme.text_muted))
            .child(Self::render_lucide_icon(icon, 16.0, rgb(icon_color)))
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "topology-menu-action-label",
                label_key,
                label,
                theme.text_muted,
                cx,
            ));
        // Topology node actions are menu items; route invocation, close, and
        // disabled/loading behavior through the workspace shared menu action.
        // The shared helper applies cx.listener once; nested listener closures
        // would re-enter WorkspaceApp while GPUI is already updating it.
        self.workspace_context_menu_styled_action(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: Some(rgba((theme.accent << 8) | 0x1a)),
                hover_text_color: Some(rgb(theme.text)),
            },
            |this| {
                this.connection_monitor.dismiss_topology_menu();
            },
            listener,
            cx,
        )
        .into_any_element()
    }

    fn zoom_topology_graph(&mut self, event: &ScrollWheelEvent) -> bool {
        let delta = event.delta.pixel_delta(px(16.0));
        let vertical = f32::from(delta.y);
        if vertical == 0.0 {
            return false;
        }

        let old = self.connection_monitor.topology_transform;
        let wheel_factor = (1.0 - vertical * 0.001).clamp(0.85, 1.15);
        let next_k = (old.k * wheel_factor).clamp(TOPOLOGY_ZOOM_MIN, TOPOLOGY_ZOOM_MAX);
        if (next_k - old.k).abs() < f32::EPSILON {
            return false;
        }

        let cursor_x = f32::from(event.position.x);
        let cursor_y = f32::from(event.position.y);
        let graph_x = (cursor_x - old.x) / old.k;
        let graph_y = (cursor_y - old.y) / old.k;
        self.connection_monitor.topology_transform = TopologyTransform {
            x: cursor_x - graph_x * next_k,
            y: cursor_y - graph_y * next_k,
            k: next_k,
        };
        true
    }

    fn pan_topology_graph(&mut self, event: &MouseMoveEvent) -> bool {
        let Some(drag) = self.connection_monitor.topology_drag else {
            return false;
        };
        if !event.dragging() {
            return false;
        }

        let x = f32::from(event.position.x);
        let y = f32::from(event.position.y);
        let dx = x - drag.last_x;
        let dy = y - drag.last_y;
        if dx == 0.0 && dy == 0.0 {
            return false;
        }
        self.connection_monitor.topology_transform.x += dx;
        self.connection_monitor.topology_transform.y += dy;
        self.connection_monitor.topology_drag = Some(TopologyDragState {
            last_x: x,
            last_y: y,
        });
        true
    }

    fn open_topology_node_menu(&mut self, node: &TopologyLayoutNode, window: &Window) {
        let transform = self.connection_monitor.topology_transform;
        let node_id = self.node_router.node_id_for_connection(&node.connection_id);
        let window_bounds = window.inner_window_bounds().get_bounds();
        let max_x = (f32::from(window_bounds.size.width) - TOPOLOGY_MENU_WIDTH).max(0.0);
        let max_y = (f32::from(window_bounds.size.height) - TOPOLOGY_MENU_MAX_HEIGHT).max(0.0);
        let x = (topology_transform_x(node.x, transform)
            + TOPOLOGY_NODE_WIDTH * transform.k / 2.0
            + 8.0)
            .min(max_x)
            .max(0.0);
        let y = (topology_transform_y(node.y, transform)
            - TOPOLOGY_NODE_HEIGHT * transform.k / 2.0)
            .min(max_y)
            .max(0.0);

        self.connection_monitor.topology_menu = Some(TopologyNodeMenuState {
            node_id,
            name: node.name.clone(),
            host: node.host.clone(),
            view_status: node.view_status,
            x,
            y,
        });
    }

}
