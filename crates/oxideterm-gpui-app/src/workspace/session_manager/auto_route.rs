use oxideterm_topology::{
    AutoRouteNetworkTopology as NetworkTopology, AutoRouteNodeConfig as TopologyNodeConfig,
    AutoRouteNodeInfo as TopologyNodeInfo, AutoRouteTopologyAuthType as TopologyAuthType,
};

const AUTO_ROUTE_MODAL_WIDTH: f32 = 512.0; // Tauri max-w-lg
const AUTO_ROUTE_MODAL_MAX_HEIGHT_RATIO: f32 = 0.80; // Tauri max-h-[80vh]
const AUTO_ROUTE_NODE_LIST_MAX_HEIGHT: f32 = 240.0; // Tauri max-h-60
const AUTO_ROUTE_EMPTY_INFO_BLUE: u32 = 0x3b82f6; // Tauri blue-500
const AUTO_ROUTE_INFO_BG_ALPHA: u32 = 0x1a; // Tauri blue-500/10
const AUTO_ROUTE_INFO_BORDER_ALPHA: u32 = 0x33; // Tauri blue-500/20
const AUTO_ROUTE_SELECTED_BG_ALPHA: u32 = 0x1a; // Tauri accent/10
const AUTO_ROUTE_ROW_BORDER_ALPHA: u32 = 0x80; // Tauri border-b /50

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
        self.session_manager.focused_basic_dialog_footer_action = None;
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn handle_auto_route_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match browser_behavior::modal_footer_input_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
            self.auto_route_modal.selected_node_id.is_some(),
            self.session_manager.focused_input == Some(SessionManagerInput::AutoRouteDisplayName),
            self.session_manager.focused_basic_dialog_footer_action,
            SessionManagerBasicDialogFooterAction::Cancel,
            Some(SessionManagerBasicDialogFooterAction::Primary),
        ) {
            Some(browser_behavior::ModalFooterInputKeyAction::Cancel) => {
                self.close_auto_route_modal(cx);
                true
            }
            Some(browser_behavior::ModalFooterInputKeyAction::FocusInput) => {
                self.session_manager.focused_input = Some(SessionManagerInput::AutoRouteDisplayName);
                self.session_manager.focused_basic_dialog_footer_action = None;
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(action)) => {
                self.session_manager.focused_input = None;
                self.session_manager.focused_basic_dialog_footer_action = Some(action);
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterInputKeyAction::Activate(action)) => {
                self.activate_auto_route_footer(action, window, cx);
                true
            }
            None if self.session_manager.focused_input
                == Some(SessionManagerInput::AutoRouteDisplayName) =>
            {
                self.handle_session_manager_key(event, window, cx)
            }
            None => false,
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
        self.session_manager.focused_basic_dialog_footer_action = None;
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
        self.session_manager.focused_basic_dialog_footer_action = None;
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

    fn activate_auto_route_footer(
        &mut self,
        action: SessionManagerBasicDialogFooterAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            SessionManagerBasicDialogFooterAction::Cancel => {
                if !self.auto_route_modal.connecting {
                    self.close_auto_route_modal(cx);
                }
            }
            SessionManagerBasicDialogFooterAction::Primary => {
                let selected_is_password = self
                    .selected_auto_route_node()
                    .is_some_and(|node| node.auth_type == TopologyAuthType::Password);
                let disabled = self.auto_route_modal.selected_node_id.is_none()
                    || self.auto_route_modal.connecting
                    || selected_is_password;
                if !disabled {
                    self.session_manager.focused_basic_dialog_footer_action = None;
                    self.connect_auto_route(window, cx);
                }
            }
        }
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
            TopologyAuthType::ManagedKey
            | TopologyAuthType::Certificate
            | TopologyAuthType::KeyboardInteractive => {
                // Managed keys need the encrypted key resolver; until that slice lands,
                // topology drill-down must not guess a filesystem key path or prompt flow.
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
                        .find_node_for_runtime_config(
                            &snapshot.config.host,
                            snapshot.config.port,
                            &snapshot.config.username,
                        )
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
                        .selectable_overflow_y_scroll(
                            &self.selectable_text_scroll_handle("auto-route-modal-body-scroll"),
                        )
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
                    .id("auto-route-node-picker-scroll")
                    .max_h(px(AUTO_ROUTE_NODE_LIST_MAX_HEIGHT))
                    .selectable_overflow_y_scrollbar(
                        &self.selectable_text_scroll_handle("auto-route-node-picker-scroll"),
                    )
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
        let selection_group_id =
            crate::workspace::selectable_text::selectable_text_id("auto-route-target-row", &node.id);
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
                            .child(self.render_row_safe_selectable_display_text_in_group(
                                selection_group_id,
                                "auto-route-target-cell",
                                ("title", node.id.as_str()),
                                0,
                                node.display_title(),
                                theme.text_heading,
                                None,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_row_safe_selectable_display_text_in_group(
                                selection_group_id,
                                "auto-route-target-cell",
                                ("detail", node.id.as_str()),
                                1,
                                format!("{}@{}:{}", node.username, node.host, node.port),
                                theme.text_muted,
                                None,
                                cx,
                            )),
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
        let connect_icon = self.auto_route_modal.connecting.then(|| {
            Self::render_lucide_icon(LucideIcon::RefreshCw, 16.0, rgb(self.tokens.ui.bg))
                .into_any_element()
        });
        modal_footer(&self.tokens)
            .child(
                self.session_manager_dialog_footer_action(
                    self.i18n.t("sessionManager.auto_route.cancel"),
                    ButtonVariant::Ghost,
                    SessionManagerBasicDialogFooterAction::Cancel,
                    self.auto_route_modal.connecting,
                    ButtonSize::Default,
                    None,
                    |this, _event, _window, cx| {
                        this.close_auto_route_modal(cx);
                        cx.stop_propagation();
                    },
                    cx,
                ),
            )
            .child(
                self.session_manager_dialog_footer_action(
                    self.i18n.t("sessionManager.auto_route.connect"),
                    ButtonVariant::Default,
                    SessionManagerBasicDialogFooterAction::Primary,
                    connect_disabled,
                    ButtonSize::Default,
                    connect_icon,
                    |this, _event, window, cx| {
                        this.connect_auto_route(window, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ),
            )
            .into_any_element()
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
