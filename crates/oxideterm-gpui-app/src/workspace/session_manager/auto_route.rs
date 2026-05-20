use std::{
    cmp::Ordering,
    collections::BinaryHeap,
};

use gpui_component::scroll::ScrollableElement;

const AUTO_ROUTE_MODAL_WIDTH: f32 = 512.0; // Tauri max-w-lg
const AUTO_ROUTE_MODAL_MAX_HEIGHT_RATIO: f32 = 0.80; // Tauri max-h-[80vh]
const AUTO_ROUTE_NODE_LIST_MAX_HEIGHT: f32 = 240.0; // Tauri max-h-60
const AUTO_ROUTE_EMPTY_INFO_BLUE: u32 = 0x3b82f6; // Tauri blue-500
const AUTO_ROUTE_INFO_BG_ALPHA: u32 = 0x1a; // Tauri blue-500/10
const AUTO_ROUTE_INFO_BORDER_ALPHA: u32 = 0x33; // Tauri blue-500/20
const AUTO_ROUTE_SELECTED_BG_ALPHA: u32 = 0x1a; // Tauri accent/10
const AUTO_ROUTE_ROW_BORDER_ALPHA: u32 = 0x80; // Tauri border-b /50
const AUTO_ROUTE_ROUTE_VERSION: &str = "2.0";

#[derive(Clone, Debug, Default)]
pub(super) struct AutoRouteModalState {
    pub(super) open: bool,
    pub(super) loading: bool,
    pub(super) connecting: bool,
    nodes: Vec<TopologyNodeInfo>,
    selected_node_id: Option<String>,
    pub(super) display_name: String,
    error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TopologyNodeConfig {
    id: String,
    host: String,
    port: u16,
    username: String,
    auth_type: TopologyAuthType,
    key_path: Option<String>,
    display_name: Option<String>,
    is_local: bool,
    tags: Vec<String>,
    saved_connection_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TopologyAuthType {
    Password,
    Key,
    Certificate,
    Agent,
}

impl TopologyAuthType {
    fn label_key(self) -> &'static str {
        match self {
            Self::Password => "sessionManager.auto_route.auth.password",
            Self::Key => "sessionManager.auto_route.auth.key",
            Self::Certificate => "sessionManager.auto_route.auth.certificate",
            Self::Agent => "sessionManager.auto_route.auth.agent",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Key => "key",
            Self::Certificate => "certificate",
            Self::Agent => "agent",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct TopologyEdge {
    from: String,
    to: String,
    cost: i32,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct RouteResult {
    path: Vec<String>,
    total_cost: i32,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct NetworkTopology {
    version: &'static str,
    nodes: HashMap<String, TopologyNodeConfig>,
    edges: Vec<TopologyEdge>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct TopologyNodeInfo {
    id: String,
    host: String,
    port: u16,
    username: String,
    display_name: Option<String>,
    auth_type: TopologyAuthType,
    is_local: bool,
    neighbors: Vec<String>,
    tags: Vec<String>,
    saved_connection_id: Option<String>,
}

#[derive(Eq, PartialEq)]
struct DijkstraState {
    cost: i32,
    node: String,
}

impl Ord for DijkstraState {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap; Tauri reverses the comparison to run Dijkstra as a min-heap.
        other.cost.cmp(&self.cost)
    }
}

impl PartialOrd for DijkstraState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl WorkspaceApp {
    pub(super) fn open_auto_route_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prepare_modal_interaction_boundary();
        self.auto_route_modal = AutoRouteModalState {
            open: true,
            loading: true,
            ..AutoRouteModalState::default()
        };
        self.load_auto_route_topology();
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(super) fn close_auto_route_modal(&mut self, cx: &mut Context<Self>) {
        self.auto_route_modal = AutoRouteModalState::default();
        if self.session_manager.focused_input == Some(SessionManagerInput::AutoRouteDisplayName) {
            self.session_manager.focused_input = None;
        }
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn handle_auto_route_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_auto_route_modal(cx);
                true
            }
            "enter" if self.session_manager.focused_input.is_none() => {
                self.connect_auto_route(window, cx);
                true
            }
            _ if self.session_manager.focused_input == Some(SessionManagerInput::AutoRouteDisplayName) => {
                self.handle_session_manager_key(event, window, cx)
            }
            _ => false,
        }
    }

    fn load_auto_route_topology(&mut self) {
        // Tauri builds this graph from the saved connection store every time the dialog opens.
        // Keep it local and synchronous in GPUI too: no terminal pane or live SSH connection is involved.
        let topology = NetworkTopology::build_from_connections(self.connection_store.connections());
        let mut nodes = topology.get_all_nodes();
        nodes.sort_by(|left, right| {
            left.display_title()
                .cmp(&right.display_title())
                .then_with(|| left.host.cmp(&right.host))
        });
        self.auto_route_modal.loading = false;
        self.auto_route_modal.nodes = nodes;
        self.auto_route_modal.error = None;
        self.auto_route_modal.selected_node_id = None;
        self.auto_route_modal.display_name.clear();
    }

    fn select_auto_route_node(&mut self, node_id: String, cx: &mut Context<Self>) {
        let display_name = self
            .auto_route_modal
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .map(TopologyNodeInfo::display_title)
            .unwrap_or_default();
        self.auto_route_modal.selected_node_id = Some(node_id);
        self.auto_route_modal.display_name = display_name;
        self.auto_route_modal.error = None;
        cx.notify();
    }

    fn selected_auto_route_node(&self) -> Option<&TopologyNodeInfo> {
        let selected_id = self.auto_route_modal.selected_node_id.as_ref()?;
        self.auto_route_modal
            .nodes
            .iter()
            .find(|node| &node.id == selected_id)
    }

    fn connect_auto_route(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.auto_route_modal.connecting {
            return;
        }
        let Some(target_id) = self.auto_route_modal.selected_node_id.clone() else {
            return;
        };
        let topology = NetworkTopology::build_from_connections(self.connection_store.connections());
        let route = match topology.compute_route(&target_id) {
            Ok(route) => route,
            Err(error) => {
                self.auto_route_modal.error = Some(error);
                cx.notify();
                return;
            }
        };
        let Some(target_node) = topology.get_node(&target_id).cloned() else {
            self.auto_route_modal.error =
                Some(self.i18n.t("sessionManager.auto_route.errors.target_not_found"));
            cx.notify();
            return;
        };
        let hops = match route
            .path
            .iter()
            .map(|node_id| topology.get_node(node_id).ok_or_else(|| {
                self.i18n
                    .t("sessionManager.auto_route.errors.invalid_route_node")
                    .replace("{{id}}", node_id)
            }))
            .map(|node| node.and_then(|node| self.topology_node_to_ssh_config(node)))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(hops) => hops,
            Err(error) => {
                self.auto_route_modal.error = Some(error);
                cx.notify();
                return;
            }
        };
        let target_config = match self.topology_node_to_ssh_config(&target_node) {
            Ok(config) => config,
            Err(error) => {
                self.auto_route_modal.error = Some(error);
                cx.notify();
                return;
            }
        };
        let route_id = uuid::Uuid::new_v4().to_string();
        let target_host = target_node.host.clone();
        let expansion = match self
            .node_router
            .expand_auto_route(&target_host, &route_id, hops, target_config)
        {
            Ok(expansion) => expansion,
            Err(error) => {
                self.auto_route_modal.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let display_name = if self.auto_route_modal.display_name.trim().is_empty() {
            target_node.display_title()
        } else {
            self.auto_route_modal.display_name.trim().to_string()
        };
        self.register_auto_route_tree_nodes(&topology, &expansion, display_name);
        self.expanded_ssh_nodes
            .extend(expansion.path_node_ids.iter().cloned());
        self.active_ssh_node_id = Some(expansion.target_node_id.clone());
        self.auto_route_modal.connecting = true;
        self.session_manager.status = Some(
            self.i18n
                .t("sessionManager.auto_route.status.connecting")
                .replace("{{name}}", &target_node.display_title()),
        );
        self.ensure_node_connection_started_without_ancestors(&expansion.target_node_id);
        self.close_auto_route_modal(cx);
        self.persist_session_tree_snapshot();
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn topology_node_to_ssh_config(&self, node: &TopologyNodeConfig) -> Result<SshConfig, String> {
        let auth = match node.auth_type {
            TopologyAuthType::Password => {
                return Err(
                    self.i18n
                        .t("sessionManager.auto_route.errors.password_auth"),
                );
            }
            TopologyAuthType::Key => {
                let key_path = node.key_path.clone().ok_or_else(|| {
                    self.i18n
                        .t("sessionManager.auto_route.errors.key_path_required")
                })?;
                AuthMethod::Key {
                    key_path,
                    passphrase: None,
                }
            }
            TopologyAuthType::Agent => AuthMethod::Agent,
            TopologyAuthType::Certificate => {
                return Err(self
                    .i18n
                    .t("sessionManager.auto_route.errors.unsupported_auth")
                    .replace("{{type}}", node.auth_type.as_str()));
            }
        };
        Ok(SshConfig {
            host: node.host.clone(),
            port: node.port,
            username: node.username.clone(),
            auth,
            proxy_chain: None,
            agent_forwarding: false,
            strict_host_key_checking: true,
            ..SshConfig::default()
        })
    }

    fn register_auto_route_tree_nodes(
        &mut self,
        topology: &NetworkTopology,
        expansion: &NodeTreeExpansion,
        target_title: String,
    ) {
        for (index, node_id) in expansion.path_node_ids.iter().enumerate() {
            let Some(snapshot) = self.node_runtime_store.snapshot(node_id) else {
                continue;
            };
            let topology_id = if node_id == &expansion.target_node_id {
                expansion
                    .path_node_ids
                    .last()
                    .and_then(|_| self.auto_route_modal.selected_node_id.as_deref())
            } else {
                expansion.path_node_ids.get(index).and_then(|_| {
                    topology
                        .nodes
                        .values()
                        .find(|node| {
                            node.host == snapshot.config.host
                                && node.port == snapshot.config.port
                                && node.username == snapshot.config.username
                        })
                        .map(|node| node.id.as_str())
                })
            };
            let topology_node = topology_id.and_then(|id| topology.get_node(id));
            let title = if node_id == &expansion.target_node_id {
                target_title.clone()
            } else {
                topology_node
                    .map(TopologyNodeConfig::display_title)
                    .unwrap_or_else(|| format!("{}@{}", snapshot.config.username, snapshot.config.host))
            };
            self.ssh_nodes.insert(
                node_id.clone(),
                WorkspaceSshNode {
                    saved_connection_id: topology_node
                        .and_then(|node| node.saved_connection_id.clone()),
                    config: snapshot.config,
                    title,
                    terminal_ids: Vec::new(),
                    readiness: NodeReadiness::Disconnected,
                },
            );
        }
    }

    pub(super) fn render_auto_route_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let modal_max_height =
            f32::from(window.viewport_size().height) * AUTO_ROUTE_MODAL_MAX_HEIGHT_RATIO;
        modal_overlay(
            &self.tokens,
            modal_container(&self.tokens)
                .w(px(AUTO_ROUTE_MODAL_WIDTH))
                .max_h(px(modal_max_height))
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(self.render_auto_route_header())
                .child(
                    modal_body(&self.tokens)
                        .id("auto-route-modal-body-scroll")
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_y_scroll()
                        .child(self.render_auto_route_body(cx)),
                )
                .child(self.render_auto_route_footer(cx)),
        )
    }

    fn render_auto_route_header(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_col()
            .flex_none()
            .justify_center()
            .px(px(self.tokens.metrics.modal_header_padding_x))
            .py(px(self.tokens.metrics.modal_header_padding_y))
            .bg(rgb(theme.bg_panel))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Self::render_lucide_icon(
                        LucideIcon::Network,
                        20.0,
                        rgb(theme.text_heading),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_heading))
                            .child(self.i18n.t("sessionManager.auto_route.title")),
                    ),
            )
            .child(
                div()
                    .mt(px(self.tokens.spacing.one + self.tokens.spacing.one / 2.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("sessionManager.auto_route.description")),
            )
            .into_any_element()
    }

    fn render_auto_route_body(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.auto_route_modal.loading {
            return div()
                .py_8()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(Self::render_lucide_icon(
                    LucideIcon::RefreshCw,
                    24.0,
                    rgb(self.tokens.ui.text_muted),
                ))
                .child(
                    div()
                        .ml_2()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .child(self.i18n.t("sessionManager.auto_route.loading")),
                )
                .into_any_element();
        }

        let mut body = div().flex().flex_col().gap_4();
        if let Some(error) = self.auto_route_modal.error.as_ref() {
            body = body.child(self.render_auto_route_error(error));
        }
        if self.auto_route_modal.nodes.is_empty() {
            return body
                .child(self.render_auto_route_empty_state())
                .into_any_element();
        }
        body.child(self.render_auto_route_node_picker(cx))
            .when_some(self.selected_auto_route_node(), |body, node| {
                body.child(self.render_auto_route_selected_details(node, cx))
            })
            .into_any_element()
    }

    fn render_auto_route_empty_state(&self) -> AnyElement {
        div()
            .py_4()
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_3()
                    .p_4()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgba(
                        (AUTO_ROUTE_EMPTY_INFO_BLUE << 8) | AUTO_ROUTE_INFO_BORDER_ALPHA,
                    ))
                    .bg(rgba(
                        (AUTO_ROUTE_EMPTY_INFO_BLUE << 8) | AUTO_ROUTE_INFO_BG_ALPHA,
                    ))
                    .child(
                        div().mt(px(2.0)).child(Self::render_lucide_icon(
                            LucideIcon::AlertCircle,
                            20.0,
                            rgb(AUTO_ROUTE_EMPTY_INFO_BLUE),
                        )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text_heading))
                                    .child(
                                        self.i18n
                                            .t("sessionManager.auto_route.empty.title"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(
                                        self.i18n
                                            .t("sessionManager.auto_route.empty.description"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_auto_route_error(&self, error: &str) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(0xef444433))
            .bg(rgba(0xef44441a))
            .p_3()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(0xf87171))
            .child(error.to_string())
            .into_any_element()
    }

    fn render_auto_route_node_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.i18n.t("sessionManager.auto_route.select_target")),
            )
            .child(
                div()
                    .max_h(px(AUTO_ROUTE_NODE_LIST_MAX_HEIGHT))
                    .overflow_y_scrollbar()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .children(
                        self.auto_route_modal
                            .nodes
                            .iter()
                            .map(|node| self.render_auto_route_node_row(node.clone(), cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_auto_route_node_row(
        &self,
        node: TopologyNodeInfo,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected = self.auto_route_modal.selected_node_id.as_ref() == Some(&node.id);
        let has_background = self
            .terminal_background_preferences("session_manager")
            .is_some();
        div()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py(px(10.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | AUTO_ROUTE_ROW_BORDER_ALPHA))
            .cursor_pointer()
            .bg(if selected {
                rgba((theme.accent << 8) | AUTO_ROUTE_SELECTED_BG_ALPHA)
            } else {
                rgba(0x00000000)
            })
            .hover(move |row| row.bg(theme_hover_bg(theme.bg_hover, has_background)))
            .child(auto_route_radio(&self.tokens, selected))
            .child(Self::render_lucide_icon(
                LucideIcon::Server,
                16.0,
                rgb(theme.text_muted),
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
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_heading))
                            .child(node.display_title()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{}@{}:{}", node.username, node.host, node.port)),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.select_auto_route_node(node.id.clone(), cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_auto_route_selected_details(
        &self,
        node: &TopologyNodeInfo,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_heading))
                            .child(self.render_selectable_text_scoped(
                                "auto-route-details-label",
                                "display_name",
                                self.i18n.t("sessionManager.auto_route.display_name"),
                                theme.text_heading,
                                cx,
                            )),
                    )
                    .child(self.render_session_text_input(
                        SessionManagerInput::AutoRouteDisplayName,
                        &self.auto_route_modal.display_name,
                        node.display_title(),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_heading))
                            .child(self.render_selectable_text_scoped(
                                "auto-route-details-label",
                                "connection_info",
                                self.i18n.t("sessionManager.auto_route.connection_info"),
                                theme.text_heading,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .rounded(px(self.tokens.radii.sm))
                            .bg(rgba((theme.bg << 8) | 0x80))
                            .px_2()
                            .py_1()
                            .font_family("monospace")
                            .child(self.render_selectable_text_scoped(
                                "auto-route-connection-info",
                                &node.id,
                                format!("{}@{}:{}", node.username, node.host, node.port),
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div().child(self.render_selectable_text_scoped(
                            "auto-route-auth-info",
                            &node.id,
                            format!(
                                "{}: {}",
                                self.i18n.t("sessionManager.auto_route.auth.label"),
                                self.i18n.t(node.auth_type.label_key())
                            ),
                            theme.text_muted,
                            cx,
                        )),
                    )
                    .when(node.auth_type == TopologyAuthType::Password, |details| {
                        details.child(
                            div()
                                .text_color(rgb(0xf59e0b))
                                .child(self.render_selectable_text_scoped(
                                    "auto-route-password-warning",
                                    &node.id,
                                    self.i18n.t("sessionManager.auto_route.auth.password_warning"),
                                    0xf59e0b,
                                    cx,
                                )),
                        )
                    })
                    .when(!node.neighbors.is_empty(), |details| {
                        details.child(
                            div().flex().flex_wrap().gap_1().children(node.neighbors.iter().map(
                                |neighbor| {
                                    div()
                                        .rounded(px(self.tokens.radii.sm))
                                        .bg(rgba((theme.accent << 8) | 0x1a))
                                        .px_2()
                                        .py(px(2.0))
                                        .text_color(rgb(theme.accent))
                                        .child(neighbor.clone())
                                },
                            )),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_auto_route_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected_is_password = self
            .selected_auto_route_node()
            .is_some_and(|node| node.auth_type == TopologyAuthType::Password);
        let connect_disabled = self.auto_route_modal.selected_node_id.is_none()
            || self.auto_route_modal.connecting
            || selected_is_password;
        modal_footer(&self.tokens)
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("sessionManager.auto_route.cancel"),
                    ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Default,
                        radius: ButtonRadius::Md,
                        disabled: self.auto_route_modal.connecting,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        if !this.auto_route_modal.connecting {
                            this.close_auto_route_modal(cx);
                        }
                        cx.stop_propagation();
                    }),
                ),
            )
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("sessionManager.auto_route.connect"),
                    ButtonOptions {
                        variant: ButtonVariant::Default,
                        size: ButtonSize::Default,
                        radius: ButtonRadius::Md,
                        disabled: connect_disabled,
                    },
                )
                .when(self.auto_route_modal.connecting, |button| {
                    button.child(Self::render_lucide_icon(
                        LucideIcon::RefreshCw,
                        16.0,
                        rgb(self.tokens.ui.bg),
                    ))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        if !connect_disabled {
                            this.connect_auto_route(window, cx);
                        }
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }
}

impl NetworkTopology {
    fn build_from_connections(connections: &[SavedConnection]) -> Self {
        let mut nodes = HashMap::new();
        let mut edges_set = HashSet::new();

        for conn in connections {
            let node_id = conn.id.clone();
            nodes.insert(
                node_id.clone(),
                TopologyNodeConfig {
                    id: node_id.clone(),
                    host: conn.host.clone(),
                    port: conn.port,
                    username: conn.username.clone(),
                    auth_type: topology_auth_type(&conn.auth),
                    key_path: topology_key_path(&conn.auth),
                    display_name: Some(conn.name.clone()),
                    is_local: false,
                    tags: conn.tags.clone(),
                    saved_connection_id: Some(conn.id.clone()),
                },
            );

            if conn.proxy_chain.is_empty() {
                edges_set.insert(TopologyEdge {
                    from: "local".to_string(),
                    to: node_id,
                    cost: 1,
                });
            } else {
                let mut previous = "local".to_string();
                for hop in &conn.proxy_chain {
                    let hop_id = Self::find_or_create_hop_node(&mut nodes, hop, connections);
                    edges_set.insert(TopologyEdge {
                        from: previous,
                        to: hop_id.clone(),
                        cost: 1,
                    });
                    previous = hop_id;
                }
                edges_set.insert(TopologyEdge {
                    from: previous,
                    to: node_id,
                    cost: 1,
                });
            }
        }

        Self {
            version: AUTO_ROUTE_ROUTE_VERSION,
            nodes,
            edges: edges_set.into_iter().collect(),
        }
    }

    fn find_or_create_hop_node(
        nodes: &mut HashMap<String, TopologyNodeConfig>,
        hop: &SavedProxyHop,
        connections: &[SavedConnection],
    ) -> String {
        for conn in connections {
            if conn.host == hop.host && conn.port == hop.port && conn.username == hop.username {
                return conn.id.clone();
            }
        }

        let hop_key = format!("{}:{}@{}", hop.username, hop.host, hop.port);
        if nodes.contains_key(&hop_key) {
            return hop_key;
        }

        nodes.insert(
            hop_key.clone(),
            TopologyNodeConfig {
                id: hop_key.clone(),
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth_type: topology_auth_type(&hop.auth),
                key_path: topology_key_path(&hop.auth),
                display_name: Some(format!("{}@{}", hop.username, hop.host)),
                is_local: false,
                tags: vec!["auto-generated".to_string()],
                saved_connection_id: None,
            },
        );
        hop_key
    }

    fn compute_route(&self, target_id: &str) -> Result<RouteResult, String> {
        if !self.nodes.contains_key(target_id) {
            return Err(format!("Target node '{}' not found in topology", target_id));
        }
        for edge in &self.edges {
            if edge.cost <= 0 {
                return Err(format!(
                    "Invalid edge cost from '{}' to '{}': {}",
                    edge.from, edge.to, edge.cost
                ));
            }
            if edge.from != "local" && !self.nodes.contains_key(&edge.from) {
                return Err(format!("Invalid edge source '{}'", edge.from));
            }
            if !self.nodes.contains_key(&edge.to) {
                return Err(format!("Invalid edge target '{}'", edge.to));
            }
        }

        let mut adj: HashMap<String, Vec<(String, i32)>> = HashMap::new();
        adj.insert("local".to_string(), Vec::new());
        for node_id in self.nodes.keys() {
            adj.insert(node_id.clone(), Vec::new());
        }
        for edge in &self.edges {
            adj.entry(edge.from.clone())
                .or_default()
                .push((edge.to.clone(), edge.cost));
        }

        let mut dist = HashMap::new();
        let mut prev = HashMap::new();
        let mut heap = BinaryHeap::new();
        dist.insert("local".to_string(), 0);
        heap.push(DijkstraState {
            cost: 0,
            node: "local".to_string(),
        });

        while let Some(DijkstraState { cost, node }) = heap.pop() {
            if node == target_id {
                break;
            }
            if cost > *dist.get(&node).unwrap_or(&i32::MAX) {
                continue;
            }
            if let Some(neighbors) = adj.get(&node) {
                for (next, edge_cost) in neighbors {
                    let next_cost = cost.saturating_add(*edge_cost);
                    if next_cost < *dist.get(next).unwrap_or(&i32::MAX) {
                        dist.insert(next.clone(), next_cost);
                        prev.insert(next.clone(), node.clone());
                        heap.push(DijkstraState {
                            cost: next_cost,
                            node: next.clone(),
                        });
                    }
                }
            }
        }

        if !prev.contains_key(target_id) {
            return Err(format!("No route found to '{}'", target_id));
        }

        let mut path = Vec::new();
        let mut current = target_id.to_string();
        while let Some(parent) = prev.get(&current) {
            if parent == "local" {
                break;
            }
            path.push(parent.clone());
            current = parent.clone();
        }
        path.reverse();
        Ok(RouteResult {
            path,
            total_cost: *dist.get(target_id).unwrap_or(&0),
        })
    }

    fn get_all_nodes(&self) -> Vec<TopologyNodeInfo> {
        let mut neighbors_map: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &self.edges {
            neighbors_map
                .entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
        }

        self.nodes
            .values()
            .filter(|node| !node.is_local)
            .map(|node| TopologyNodeInfo {
                id: node.id.clone(),
                host: node.host.clone(),
                port: node.port,
                username: node.username.clone(),
                display_name: node.display_name.clone(),
                auth_type: node.auth_type,
                is_local: node.is_local,
                neighbors: neighbors_map.get(&node.id).cloned().unwrap_or_default(),
                tags: node.tags.clone(),
                saved_connection_id: node.saved_connection_id.clone(),
            })
            .collect()
    }

    fn get_node(&self, node_id: &str) -> Option<&TopologyNodeConfig> {
        self.nodes.get(node_id)
    }
}

impl TopologyNodeConfig {
    fn display_title(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| format!("{}@{}", self.username, self.host))
    }
}

impl TopologyNodeInfo {
    fn display_title(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| format!("{}@{}", self.username, self.host))
    }
}

fn topology_auth_type(auth: &SavedAuth) -> TopologyAuthType {
    match auth {
        SavedAuth::Password { .. } => TopologyAuthType::Password,
        SavedAuth::Key { .. } => TopologyAuthType::Key,
        SavedAuth::Certificate { .. } => TopologyAuthType::Certificate,
        SavedAuth::Agent => TopologyAuthType::Agent,
    }
}

fn topology_key_path(auth: &SavedAuth) -> Option<String> {
    match auth {
        SavedAuth::Key { key_path, .. } | SavedAuth::Certificate { key_path, .. } => {
            Some(key_path.clone())
        }
        _ => None,
    }
}

fn auto_route_radio(tokens: &ThemeTokens, selected: bool) -> AnyElement {
    div()
        .size(px(16.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(999.0))
        .border_1()
        .border_color(if selected {
            rgb(tokens.ui.accent)
        } else {
            rgb(tokens.ui.border)
        })
        .when(selected, |radio| {
            radio.child(
                div()
                    .size(px(8.0))
                    .rounded(px(999.0))
                    .bg(rgb(tokens.ui.accent)),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod auto_route_tests {
    use super::*;
    use oxideterm_connections::ConnectionOptions;

    #[test]
    fn topology_routes_proxy_chain_like_tauri() {
        let jump = saved_connection("jump", "jump.internal", Vec::new());
        let mut target = saved_connection("target", "db.internal", Vec::new());
        target.proxy_chain.push(SavedProxyHop {
            host: "jump.internal".to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SavedAuth::Agent,
            agent_forwarding: false,
        });

        let topology = NetworkTopology::build_from_connections(&[jump, target]);
        let route = topology.compute_route("target").expect("route");

        assert_eq!(route.path, vec!["jump".to_string()]);
        assert_eq!(route.total_cost, 2);
    }

    #[test]
    fn topology_keeps_tauri_temp_hop_id_format() {
        let mut target = saved_connection("target", "db.internal", Vec::new());
        target.proxy_chain.push(SavedProxyHop {
            host: "jump.internal".to_string(),
            port: 2222,
            username: "alice".to_string(),
            auth: SavedAuth::Agent,
            agent_forwarding: false,
        });

        let topology = NetworkTopology::build_from_connections(&[target]);

        assert!(topology.get_node("alice:jump.internal@2222").is_some());
    }

    fn saved_connection(id: &str, host: &str, proxy_chain: Vec<SavedProxyHop>) -> SavedConnection {
        SavedConnection {
            id: id.to_string(),
            version: oxideterm_connections::CONFIG_VERSION,
            name: id.to_string(),
            group: None,
            host: host.to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SavedAuth::Agent,
            proxy_chain,
            options: ConnectionOptions::default(),
            created_at: Utc::now(),
            last_used_at: None,
            updated_at: None,
            color: None,
            tags: Vec::new(),
        }
    }
}
