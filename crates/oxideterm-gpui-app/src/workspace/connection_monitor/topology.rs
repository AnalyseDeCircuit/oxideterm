use super::*;

use oxideterm_gpui_host_tools::{topology_canvas, topology_node};
use oxideterm_gpui_ui::context_menu::{ContextMenuActionableStyle, context_menu_event_boundary};
use oxideterm_gpui_ui::modal::rounded_shell_child_radius;
use oxideterm_topology::{ConnectionTopologyLayout, TopologyLayoutNode};

impl WorkspaceApp {
    pub(in crate::workspace) fn render_topology_surface(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("topology");
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(connection_monitor_surface_bg(theme.bg, has_background))
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
                            // Page headings are navigation chrome and must not
                            // enter the shared read-only selection lifecycle.
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
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
                                SelectableTextRole::NonSelectable,
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
                    .child(self.render_connection_topology(has_background, cx)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_connection_runtime_topology(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("runtime");
        div()
            .id("connection-runtime-topology")
            .flex_1()
            .min_h_0()
            .relative()
            .overflow_hidden()
            .bg(connection_monitor_surface_bg(theme.bg, has_background))
            .text_color(rgb(theme.text))
            // Runtime already labels this section as Connection Matrix, so the
            // embedded graph starts directly at the canvas.
            .child(self.render_connection_topology(has_background, cx))
            .into_any_element()
    }

    fn render_connection_topology(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(snapshot) = self.host_tools.read(cx).topology_snapshot() else {
            return monitor_center_state(
                self,
                LucideIcon::RefreshCw,
                theme.text_muted,
                self.i18n.t("connections.monitor.loading"),
                cx,
            );
        };
        let layout = ConnectionTopologyLayout::from_snapshot(&snapshot);
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
                .child(div().mt_2().text_size(px(14.0)).opacity(0.7).child(
                    self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "topology-empty",
                        "hint",
                        self.i18n.t("topology.page.connect_hint"),
                        theme.text_muted,
                        cx,
                    ),
                ))
                .into_any_element();
        }

        let edges = layout.edges.clone();
        let (transform, topology_dragging, topology_menu) = {
            let host_tools = self.host_tools.read(cx);
            (
                host_tools.topology_transform(),
                host_tools.topology_dragging(),
                host_tools.topology_menu(),
            )
        };
        let mut graph = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(connection_monitor_surface_bg(theme.bg, has_background))
            .rounded(px(self.tokens.radii.lg))
            .cursor(if topology_dragging {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _window, cx| {
                let host_tools = this.host_tools.clone();
                host_tools.update(cx, |host_tools, cx| {
                    host_tools.zoom_topology_graph(event, cx);
                    host_tools.dismiss_topology_menu(cx);
                });
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    let host_tools = this.host_tools.clone();
                    host_tools.update(cx, |host_tools, cx| {
                        host_tools.dismiss_topology_menu(cx);
                        host_tools.begin_topology_drag(event, cx);
                    });
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                let host_tools = this.host_tools.clone();
                if host_tools.update(cx, |host_tools, cx| {
                    host_tools.pan_topology_graph(event, cx)
                }) {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    let host_tools = this.host_tools.clone();
                    if host_tools.update(cx, |host_tools, cx| host_tools.end_topology_drag(cx)) {
                        cx.stop_propagation();
                    }
                }),
            )
            .child(topology_canvas(theme, has_background, transform, edges))
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
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
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
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
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

        if let Some(menu) = topology_menu {
            // Topology node actions are a context menu, not a graph child popover:
            // keep outside pointer and Esc dismissal on the same workspace menu
            // owner as FileManager/SFTP/session menus.
            graph = graph.child(self.workspace_context_menu_backdrop(
                self.render_topology_node_action_menu(menu, cx),
                cx,
            ));
        }

        div()
            .size_full()
            .overflow_hidden()
            .bg(connection_monitor_surface_bg(theme.bg, has_background))
            .child(graph)
            .into_any_element()
    }

    fn render_topology_graph_node(
        &self,
        node: TopologyLayoutNode,
        transform: TopologyTransform,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        topology_node(
            &self.tokens,
            self.background_surface_active("topology"),
            &node,
            transform,
            settings_mono_font_family(self.settings_store.settings()),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener({
                move |this, event: &MouseDownEvent, window, cx| {
                    if event.click_count >= 2 {
                        let node_id = this.node_router.node_id_for_connection(&node.connection_id);
                        this.host_tools.update(cx, |host_tools, cx| {
                            host_tools.open_topology_node_menu(node_id, &node, window, cx);
                        });
                    }
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.end_topology_drag(cx);
                    });
                    cx.stop_propagation();
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
                move |this, _event, _window, cx| {
                    if let Some(node_id) = node_id.clone() {
                        this.active_ssh_node_id = Some(node_id);
                        this.close_session_manager_sidebar_search(cx);
                        this.rails.section = SidebarSection::Sessions;
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
                        move |this, event: &MouseDownEvent, window, cx| {
                            if let Some(node_id) = node_id.clone()
                                && let Some(title) =
                                    this.ssh_nodes.get(&node_id).map(|node| node.title.clone())
                            {
                                let _ = this.queue_ssh_terminal_tab_for_existing_node(
                                    node_id,
                                    None,
                                    title,
                                    event.modifiers.control,
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
                        move |this, _event, window, _cx| {
                            if let Some(node_id) = node_id.clone() {
                                this.open_sftp_tab(node_id, window, _cx);
                            }
                        }
                    },
                    cx,
                ));
        }

        let menu_body = context_menu_event_boundary(
            div()
                .absolute()
                .left(px(menu.x))
                .top(px(menu.y))
                .min_w(px(TOPOLOGY_MENU_WIDTH))
                .overflow_hidden()
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgba((theme.bg_elevated << 8) | 0xf2))
                .shadow_lg(),
        )
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
                        .max_w(px(280.0))
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
                        .font_family(settings_mono_font_family(self.settings_store.settings()))
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
        .into_any_element();

        div().child(menu_body).into_any_element()
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
            |_this| {
                // The menu helper does not receive a GPUI context. The action
                // listener below closes the Entity-owned menu before routing.
            },
            move |this, event, window, cx| {
                this.host_tools.update(cx, |host_tools, cx| {
                    host_tools.dismiss_topology_menu(cx);
                });
                listener(this, event, window, cx);
            },
            cx,
        )
        .into_any_element()
    }
}
