use super::*;
use gpui::{Div, StatefulInteractiveElement, point};
use oxideterm_remote_desktop::RemoteDesktopSessionStatus;
use oxideterm_settings::SessionSortOrder;

fn active_connection_count(rows: &[ActiveSessionSidebarRow]) -> usize {
    // Count connection owners, including SSH nodes with only SFTP/forwarding consumers.
    rows.iter()
        .filter(|row| {
            matches!(
                row.node_view.status(),
                ActiveSessionStatus::Active | ActiveSessionStatus::Connected
            )
        })
        .count()
}

fn sidebar_terminal_post_connect_command(
    saved_connection: Option<&oxideterm_connections::SavedConnection>,
    node_router: &NodeRouter,
    node_id: &NodeId,
) -> Option<String> {
    if let Some(connection) = saved_connection {
        // An explicitly cleared saved command must not fall back to an older runtime value.
        return connection.post_connect_command().map(ToOwned::to_owned);
    }
    // Temporary nodes have no saved profile. Read their zeroizing config only
    // for the explicit open action and move the command into the terminal request.
    node_router
        .node_runtime_snapshot(node_id)
        .and_then(|mut snapshot| snapshot.config.post_connect_command.take())
}

impl standalone_connections::StandaloneConnectionKind {
    fn icon(self) -> LucideIcon {
        match self {
            standalone_connections::StandaloneConnectionKind::Mosh => LucideIcon::Wifi,
            standalone_connections::StandaloneConnectionKind::Telnet => LucideIcon::Terminal,
            standalone_connections::StandaloneConnectionKind::Serial => LucideIcon::Cable,
            standalone_connections::StandaloneConnectionKind::Rdp
            | standalone_connections::StandaloneConnectionKind::Vnc => LucideIcon::Monitor,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct StandaloneActiveSession {
    connection_id: String,
    kind: standalone_connections::StandaloneConnectionKind,
    target: Option<standalone_connections::StandaloneConnectionSurface>,
}

#[derive(Clone)]
pub(in crate::workspace) struct ActiveSessionSidebarRow {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    saved_connection_id: Option<String>,
    title: String,
    host: String,
    username: String,
    port: u16,
    node_view: ActiveSessionNode,
    depth: usize,
    is_last: bool,
    has_children: bool,
    standalone_session: Option<StandaloneActiveSession>,
}

fn terminal_lifecycle_readiness(lifecycle: &TerminalLifecycle) -> ActiveSessionReadiness {
    match lifecycle {
        TerminalLifecycle::Running => ActiveSessionReadiness::Ready,
        TerminalLifecycle::Exited(_) => ActiveSessionReadiness::Error,
        TerminalLifecycle::Closed => ActiveSessionReadiness::Disconnected,
    }
}

fn remote_desktop_readiness(status: RemoteDesktopSessionStatus) -> ActiveSessionReadiness {
    match status {
        RemoteDesktopSessionStatus::Connected => ActiveSessionReadiness::Ready,
        RemoteDesktopSessionStatus::Connecting | RemoteDesktopSessionStatus::Reconnecting => {
            ActiveSessionReadiness::Connecting
        }
        RemoteDesktopSessionStatus::Failed => ActiveSessionReadiness::Error,
        RemoteDesktopSessionStatus::Idle | RemoteDesktopSessionStatus::Disconnected => {
            ActiveSessionReadiness::Disconnected
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionNodeRowAction {
    Connect,
    Reconnect,
    Disconnect,
    CancelReconnect,
    Remove,
}

fn session_node_row_actions(
    status: ActiveSessionStatus,
    reconnecting: bool,
) -> [Option<SessionNodeRowAction>; 2] {
    use SessionNodeRowAction::*;
    if reconnecting {
        return [Some(CancelReconnect), None];
    }
    match status {
        ActiveSessionStatus::Active | ActiveSessionStatus::Connected => [Some(Disconnect), None],
        ActiveSessionStatus::Error => [Some(Reconnect), Some(Remove)],
        ActiveSessionStatus::Idle => [Some(Connect), Some(Remove)],
        ActiveSessionStatus::Connecting => [None, None],
    }
}

fn standalone_session_actions(status: ActiveSessionStatus) -> [Option<SessionNodeRowAction>; 2] {
    use SessionNodeRowAction::*;
    match status {
        ActiveSessionStatus::Active
        | ActiveSessionStatus::Connected
        | ActiveSessionStatus::Connecting => [Some(Disconnect), None],
        ActiveSessionStatus::Error | ActiveSessionStatus::Idle => [Some(Reconnect), Some(Remove)],
    }
}

const SESSION_SORT_OPTIONS: [(SessionSortOrder, &str); 4] = [
    (SessionSortOrder::Default, "sidebar.sort.default"),
    (
        SessionSortOrder::NameAscending,
        "sidebar.sort.name_ascending",
    ),
    (
        SessionSortOrder::NameDescending,
        "sidebar.sort.name_descending",
    ),
    (
        SessionSortOrder::ConnectedFirst,
        "sidebar.sort.connected_first",
    ),
];

fn sort_active_session_rows(
    rows: Vec<ActiveSessionSidebarRow>,
    order: SessionSortOrder,
    manual_order: &[String],
) -> Vec<ActiveSessionSidebarRow> {
    if order == SessionSortOrder::Default && manual_order.is_empty() {
        return rows;
    }
    let known: HashSet<_> = rows.iter().map(|row| row.node_id.clone()).collect();
    let names: Vec<_> = rows.iter().map(|row| row.title.to_lowercase()).collect();
    let mut siblings: HashMap<Option<NodeId>, Vec<usize>> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        let parent = row
            .parent_id
            .as_ref()
            .filter(|id| known.contains(id))
            .cloned();
        siblings.entry(parent).or_default().push(index);
    }
    let readiness = |row: &ActiveSessionSidebarRow| match row.node_view.readiness {
        ActiveSessionReadiness::Ready => 0,
        ActiveSessionReadiness::Connecting => 1,
        ActiveSessionReadiness::Error => 2,
        ActiveSessionReadiness::Disconnected => 3,
    };
    let ranks: HashMap<_, _> = manual_order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    for group in siblings.values_mut() {
        group.sort_by(|a, b| match order {
            SessionSortOrder::NameAscending => names[*a].cmp(&names[*b]),
            SessionSortOrder::NameDescending => names[*b].cmp(&names[*a]),
            SessionSortOrder::ConnectedFirst => readiness(&rows[*a])
                .cmp(&readiness(&rows[*b]))
                .then_with(|| names[*a].cmp(&names[*b])),
            SessionSortOrder::Default => ranks
                .get(rows[*a].node_id.0.as_str())
                .copied()
                .unwrap_or(usize::MAX)
                .cmp(
                    &ranks
                        .get(rows[*b].node_id.0.as_str())
                        .copied()
                        .unwrap_or(usize::MAX),
                ),
        });
    }
    let roots = siblings.remove(&None).unwrap_or_default();
    let mut pending: Vec<_> = roots
        .iter()
        .enumerate()
        .rev()
        .map(|(position, index)| (*index, 0, position + 1 == roots.len()))
        .collect();
    let mut output = Vec::with_capacity(rows.len());
    let mut rows: Vec<_> = rows.into_iter().map(Some).collect();
    // Walk sorted sibling groups in preorder so descendants always follow their parent.
    while let Some((index, depth, is_last)) = pending.pop() {
        let mut row = rows[index].take().unwrap();
        row.depth = depth;
        row.is_last = is_last;
        if let Some(children) = siblings.remove(&Some(row.node_id.clone())) {
            pending.extend(
                children
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(position, index)| (*index, depth + 1, position + 1 == children.len())),
            );
        }
        output.push(row);
    }
    output
}

fn filter_active_session_rows(
    mut rows: Vec<ActiveSessionSidebarRow>,
    query: &str,
) -> Vec<ActiveSessionSidebarRow> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return rows;
    }
    let parents: HashMap<_, _> = rows
        .iter()
        .map(|row| (row.node_id.clone(), row.parent_id.clone()))
        .collect();
    let mut retained = HashSet::new();
    for row in &rows {
        let protocol = row
            .standalone_session
            .as_ref()
            .map(|session| format!("{:?}", session.kind))
            .unwrap_or_else(|| "ssh".into());
        let text = format!(
            "{} {} {} {} {}",
            row.title, row.host, row.username, row.port, protocol
        )
        .to_lowercase();
        if query.split_whitespace().all(|term| text.contains(term)) {
            let mut next = Some(row.node_id.clone());
            while let Some(id) = next {
                if !retained.insert(id.clone()) {
                    break;
                }
                next = parents.get(&id).cloned().flatten();
            }
        }
    }
    rows.retain(|row| retained.contains(&row.node_id));
    let last_children: HashMap<_, _> = rows
        .iter()
        .map(|row| (row.parent_id.clone(), row.node_id.clone()))
        .collect();
    for row in &mut rows {
        row.is_last = last_children.get(&row.parent_id) == Some(&row.node_id);
        row.has_children = last_children.contains_key(&Some(row.node_id.clone()));
    }
    rows
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_session_search_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_tooltip_icon_button(
            LucideIcon::Search,
            self.tokens.metrics.sidebar_action_icon_size,
            rgb(self.tokens.ui.text),
            IconButtonOptions {
                has_background: self.session_search_open,
                background: self
                    .session_search_open
                    .then_some(rgb(self.tokens.ui.bg_hover)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(
                    self.tokens.metrics.sidebar_action_size,
                    ButtonRadius::Md,
                )
            },
            self.i18n.t("sidebar.search.title"),
            "session-search",
            false,
            cx.listener(|this, _, window, cx| {
                this.prepare_modal_interaction_boundary(cx);
                this.session_search_open = !this.session_search_open;
                this.begin_disclosure_motion("session-search".into(), this.session_search_open, cx);
                this.clear_ime_selection();
                if this.session_search_open {
                    this.selected_ime_target = Some(ime::WorkspaceImeTarget::ActiveSessionSearch);
                    window.focus(&this.focus_handle, cx);
                    this.show_active_input_caret(cx);
                } else {
                    this.session_search_query.clear();
                }
                cx.stop_propagation();
                cx.notify();
            }),
            cx.entity(),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn render_session_search_input(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sidebar_search_row(self.workspace_sidebar_background(self.tokens.ui.bg))
            .child(self.render_overlay_query_input(
                ime::WorkspaceImeTarget::ActiveSessionSearch,
                self.session_search_query.clone(),
                self.i18n.t("sidebar.search.placeholder"),
                self.tokens.metrics.sidebar_title_font_size,
                20.0,
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_session_sort_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let button = self.workspace_tooltip_icon_button(
            LucideIcon::ArrowDownAZ,
            self.tokens.metrics.sidebar_action_icon_size,
            rgb(self.tokens.ui.text),
            IconButtonOptions {
                has_background: self.session_sort_menu_open,
                background: self
                    .session_sort_menu_open
                    .then_some(rgb(self.tokens.ui.bg_hover)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(
                    self.tokens.metrics.sidebar_action_size,
                    ButtonRadius::Md,
                )
            },
            self.i18n.t("sidebar.sort.title"),
            "session-sort",
            false,
            cx.listener(|this, _, window, cx| {
                let open = !this.session_sort_menu_open;
                this.prepare_modal_interaction_boundary(cx);
                this.session_sort_menu_open = open;
                window.focus(&this.focus_handle, cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.entity(),
        );
        let workspace = cx.entity();
        div()
            .ml_1()
            .child(oxideterm_gpui_ui::select::select_anchor_probe(
                SelectAnchorId::ActiveSessionSort,
                button,
                move |anchor, window, cx| {
                    window.defer(cx, move |_, cx| {
                        workspace.update(cx, |this, cx| this.update_select_anchor(anchor, cx));
                    })
                },
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_session_sort_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.session_sort_menu_open
            || self.sidebar_collapsed
            || self.effective_sidebar_panel_section() != SidebarSection::Sessions
        {
            return None;
        }
        let anchor = self
            .select_anchors
            .get(&SelectAnchorId::ActiveSessionSort)?;
        let selected = self.settings_store.settings().sidebar_ui.session_sort_order;
        let mut popup = oxideterm_gpui_ui::select::select_overlay_popup(&self.tokens, 220.0);
        for (order, label) in SESSION_SORT_OPTIONS {
            popup = popup.child(oxideterm_gpui_ui::select::select_option_action(
                oxideterm_gpui_ui::select::select_option(
                    &self.tokens,
                    self.i18n.t(label),
                    order == selected,
                ),
                false,
                false,
                cx.listener(move |this, _, _, cx| {
                    this.settings_store
                        .settings_mut()
                        .sidebar_ui
                        .session_sort_order = order;
                    this.session_sort_menu_open = false;
                    this.persist_sidebar_settings(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }
        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.session_sort_menu_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, _, cx| {
                        this.session_sort_menu_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .anchor(Corner::TopRight)
                            .position(anchor.bounds.bottom_right())
                            .offset(point(px(0.0), px(4.0)))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(popup),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_SELECT_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    /// Keeps clickable session labels from competing with their control's pointer interaction.
    fn render_session_control_label(
        &self,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_display_text_with_role(
            SelectableTextRole::NonSelectable,
            scope,
            key,
            text,
            color,
            cx,
        )
    }

    fn queue_ssh_terminal_tab_for_sidebar_node(
        &mut self,
        node_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .ok_or_else(|| anyhow::anyhow!("SSH node {} not found", node_id.0))?;
        let saved_connection_id = self
            .ssh_nodes
            .get(&node_id)
            .and_then(|node| node.saved_connection_id.clone());
        let post_connect_command = sidebar_terminal_post_connect_command(
            saved_connection_id
                .as_deref()
                .and_then(|id| self.connection_store.get(id)),
            &self.node_router,
            &node_id,
        );
        if self.node_is_ready_for_terminal(&node_id) {
            return self.queue_ssh_terminal_tab_for_existing_node(
                node_id,
                post_connect_command,
                title,
                window,
                cx,
            );
        }

        let config = self
            .node_router
            .node_runtime_snapshot(&node_id)
            .map(|snapshot| snapshot.config)
            .ok_or_else(|| anyhow::anyhow!("SSH node {} has no runtime config", node_id.0))?;
        // Keep secret-bearing config out of virtual rows and retained listeners.
        // A disconnected node copies it only at the explicit connect action.
        self.queue_ssh_terminal_tab_for_node_with_mark_used(
            node_id,
            post_connect_command,
            config,
            title,
            saved_connection_id,
            None,
            None,
            window,
            cx,
        )
    }

    pub(in crate::workspace) fn render_active_sessions_sidebar_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.active_session_sidebar_view_mode == ActiveSessionSidebarViewMode::Focus
            && self.session_search_query.trim().is_empty()
        {
            return self.render_active_sessions_focus_sidebar_content(cx);
        }

        let rows = self.active_session_sidebar_rows(cx);
        if rows.is_empty() {
            if !self.session_search_query.trim().is_empty() {
                return div()
                    .p_4()
                    .text_size(px(self.tokens.metrics.sidebar_title_font_size))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("sidebar.search.empty"))
                    .into_any_element();
            }
            return self.render_empty_sessions_sidebar_content(cx);
        }

        self.sync_active_session_sidebar_list_state(&rows, ActiveSessionSidebarViewMode::Tree, cx);
        let state = self.active_session_sidebar_list_state.clone();
        let spec = self.active_session_sidebar_list_spec(ActiveSessionSidebarViewMode::Tree);
        let workspace = cx.entity();
        div()
            .id("active-sessions-sidebar-scroll")
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
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

    pub(in crate::workspace) fn render_active_sessions_focus_sidebar_content(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.active_session_sidebar_rows(cx);
        let focused_node_id = self.effective_active_session_focus_node_id(&rows);
        self.active_session_sidebar_focused_node_id = focused_node_id.clone();
        let visible_rows = self.active_session_focus_rows(&rows, focused_node_id.as_ref());
        self.sync_active_session_sidebar_list_state(
            &visible_rows,
            ActiveSessionSidebarViewMode::Focus,
            cx,
        );

        let state = self.active_session_sidebar_list_state.clone();
        let spec = self.active_session_sidebar_list_spec(ActiveSessionSidebarViewMode::Focus);
        let workspace = cx.entity();
        div()
            .id("active-sessions-focus-sidebar")
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .pt(px(PRIMARY_SIDEBAR_CONTENT_TOP_INSET))
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
                        tauri_virtual_list(state, spec, move |index, _window, cx| {
                            workspace.update(cx, |this, cx| {
                                this.render_active_session_focus_list_item(index, cx)
                            })
                        })
                        .into_any_element()
                    }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn active_session_sidebar_rows(
        &self,
        cx: &App,
    ) -> Vec<ActiveSessionSidebarRow> {
        filter_active_session_rows(
            sort_active_session_rows(
                self.unfiltered_active_session_sidebar_rows(cx),
                self.settings_store.settings().sidebar_ui.session_sort_order,
                &self
                    .settings_store
                    .settings()
                    .sidebar_ui
                    .session_manual_order,
            ),
            &self.session_search_query,
        )
    }

    fn unfiltered_active_session_sidebar_rows(&self, cx: &App) -> Vec<ActiveSessionSidebarRow> {
        let mut tree_nodes = self.node_router.flatten_tree();
        let flat_node_child_counts = tree_nodes
            .iter()
            .filter_map(|node| node.parent_id.as_ref())
            .fold(HashMap::<String, usize>::new(), |mut counts, parent_id| {
                *counts.entry(parent_id.clone()).or_default() += 1;
                counts
            });

        let mut rows = tree_nodes
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
                    saved_connection_id: node.saved_connection_id.clone(),
                    title: node.title.clone(),
                    host: node.endpoint.host.clone(),
                    username: node.endpoint.username.clone(),
                    port: node.endpoint.port,
                    node_view,
                    depth: flat_node.depth as usize,
                    is_last: flat_node.is_last_child,
                    has_children: flat_node_child_counts
                        .get(&flat_node_id)
                        .is_some_and(|count| *count > 0),
                    standalone_session: None,
                })
            })
            .collect::<Vec<_>>();

        // Standalone records remain visible after their current surface is closed.
        rows.extend(
            self.standalone_connections
                .records()
                .iter()
                .map(|record| self.standalone_active_session_sidebar_row(record, cx)),
        );
        rows
    }

    pub(in crate::workspace) fn render_active_sessions_footer(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Search and tree expansion affect presentation, never the workspace total.
        let rows = self.unfiltered_active_session_sidebar_rows(cx);
        let node_connections: HashSet<_> = rows
            .iter()
            .filter_map(|row| self.node_router.connection_id_for_node(&row.node_id))
            .collect();
        let standalone_sftp_count = self
            .standalone_sftp_sessions
            .values()
            .filter(|runtime| {
                matches!(
                    runtime.handle.state(),
                    oxideterm_ssh::ConnectionState::Active | oxideterm_ssh::ConnectionState::Idle
                ) && !node_connections.contains(&runtime.connection_id)
            })
            .map(|runtime| &runtime.connection_id)
            .collect::<HashSet<_>>()
            .len();
        // FTP/FTPS runtimes are registered after connect and own any transfer
        // sockets. Count the runtime once, not its tabs or temporary sockets.
        let ftp_count = self
            .ftp_sessions
            .values()
            .filter(|runtime| !runtime.cancel.is_cancelled())
            .count();
        let count = active_connection_count(&rows) + standalone_sftp_count + ftp_count;
        let label = self
            .i18n
            .t("sidebar.active_session_count")
            .replace("{{count}}", &count.to_string());
        div()
            .w_full()
            .min_w_0()
            .flex_none()
            .h(px(
                crate::workspace::terminal_command_bar::TERMINAL_SENDER_COMPACT_HEIGHT,
            ))
            .px(px(self.tokens.spacing.three))
            .py_0()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .border_t_1()
            .border_color(self.workspace_chrome_divider())
            .bg(self.workspace_sidebar_background(theme.bg))
            .child(
                div()
                    .flex_none()
                    .size(px(6.0))
                    .rounded_full()
                    .bg(rgb(if count > 0 {
                        theme.success
                    } else {
                        theme.text_muted
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.text_muted))
                    .child(label),
            )
            .when(
                self.settings_store.settings().sidebar_ui.show_app_lock_icon,
                |footer| footer.child(self.render_app_lock_button(24.0, cx)),
            )
            .into_any_element()
    }

    fn standalone_active_session_sidebar_row(
        &self,
        record: &standalone_connections::StandaloneConnectionRecord,
        cx: &App,
    ) -> ActiveSessionSidebarRow {
        let (readiness, terminal_ids) = match record.surface {
            Some(standalone_connections::StandaloneConnectionSurface::RemoteDesktop(tab_id)) => {
                let Some(session) = self.remote_desktop.read(cx).session(tab_id) else {
                    return self.disconnected_standalone_sidebar_row(record);
                };
                let session = session.read(cx);
                (
                    remote_desktop_readiness(session.active_session_status()),
                    Vec::new(),
                )
            }
            Some(standalone_connections::StandaloneConnectionSurface::Terminal(session_id)) => {
                let Some(shared_session) = self
                    .tab_host
                    .read(cx)
                    .terminal_location(session_id)
                    .and_then(|location| {
                        self.tab_host
                            .read(cx)
                            .panes()
                            .get(&location.pane_id)
                            .map(|pane| pane.read(cx).shared_session())
                    })
                else {
                    return self.disconnected_standalone_sidebar_row(record);
                };
                let terminal = shared_session.lock();
                let readiness =
                    if record.kind == standalone_connections::StandaloneConnectionKind::Mosh {
                        terminal
                            .mosh_connection_status()
                            .map(|status| match status {
                                oxideterm_terminal::MoshConnectionStatus::Connecting => {
                                    ActiveSessionReadiness::Connecting
                                }
                                oxideterm_terminal::MoshConnectionStatus::Connected => {
                                    ActiveSessionReadiness::Ready
                                }
                                oxideterm_terminal::MoshConnectionStatus::Interrupted => {
                                    ActiveSessionReadiness::Error
                                }
                            })
                            .unwrap_or(ActiveSessionReadiness::Connecting)
                    } else {
                        terminal_lifecycle_readiness(&terminal.lifecycle())
                    };
                (readiness, vec![session_id])
            }
            None => (record.readiness.clone(), Vec::new()),
        };

        let row_id = format!("standalone-connection-{}", record.id);
        ActiveSessionSidebarRow {
            node_id: NodeId::new(row_id.clone()),
            parent_id: None,
            saved_connection_id: None,
            title: record.title.clone(),
            host: String::new(),
            username: String::new(),
            port: 0,
            node_view: ActiveSessionNode {
                id: row_id,
                title: record.title.clone(),
                port: 0,
                terminal_ids,
                readiness,
            },
            depth: 0,
            is_last: true,
            has_children: false,
            standalone_session: Some(StandaloneActiveSession {
                connection_id: record.id.clone(),
                kind: record.kind,
                target: record.surface,
            }),
        }
    }

    fn disconnected_standalone_sidebar_row(
        &self,
        record: &standalone_connections::StandaloneConnectionRecord,
    ) -> ActiveSessionSidebarRow {
        let row_id = format!("standalone-connection-{}", record.id);
        ActiveSessionSidebarRow {
            node_id: NodeId::new(row_id.clone()),
            parent_id: None,
            saved_connection_id: None,
            title: record.title.clone(),
            host: String::new(),
            username: String::new(),
            port: 0,
            node_view: ActiveSessionNode {
                id: row_id,
                title: record.title.clone(),
                port: 0,
                terminal_ids: Vec::new(),
                readiness: record.readiness.clone(),
            },
            depth: 0,
            is_last: true,
            has_children: false,
            standalone_session: Some(StandaloneActiveSession {
                connection_id: record.id.clone(),
                kind: record.kind,
                target: None,
            }),
        }
    }

    pub(in crate::workspace) fn sync_active_session_sidebar_list_state(
        &mut self,
        rows: &[ActiveSessionSidebarRow],
        view_mode: ActiveSessionSidebarViewMode,
        cx: &App,
    ) {
        let signatures = rows
            .iter()
            .map(|row| self.active_session_sidebar_row_signature(row, view_mode, cx))
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.active_session_sidebar_list_state,
            &mut self.active_session_sidebar_list_cache.borrow_mut(),
            "active-sessions-sidebar",
            &signatures,
            self.active_session_sidebar_list_spec(view_mode),
        );
    }

    pub(in crate::workspace) fn active_session_sidebar_list_spec(
        &self,
        view_mode: ActiveSessionSidebarViewMode,
    ) -> TauriVirtualListSpec {
        let estimated_height = match view_mode {
            ActiveSessionSidebarViewMode::Tree => ACTIVE_SESSION_SIDEBAR_LIST_ESTIMATED_HEIGHT,
            ActiveSessionSidebarViewMode::Focus => ACTIVE_SESSION_FOCUS_LIST_ESTIMATED_HEIGHT,
        };
        TauriVirtualListSpec::new(px(estimated_height), ACTIVE_SESSION_SIDEBAR_LIST_OVERSCAN)
    }

    pub(in crate::workspace) fn render_active_session_sidebar_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = self.active_session_sidebar_rows(cx).into_iter().nth(index) else {
            return div().into_any_element();
        };
        div()
            .px_1()
            .child(self.render_active_session_node(row, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn active_session_sidebar_row_signature(
        &self,
        row: &ActiveSessionSidebarRow,
        view_mode: ActiveSessionSidebarViewMode,
        cx: &App,
    ) -> u64 {
        let mut hasher = DefaultHasher::new();
        // This virtual row owns the node header plus expanded action/terminal
        // children. Hash all state that can change its visible height or labels.
        view_mode.hash(&mut hasher);
        row.node_id.hash(&mut hasher);
        row.parent_id.hash(&mut hasher);
        row.title.hash(&mut hasher);
        row.port.hash(&mut hasher);
        row.node_view.title.hash(&mut hasher);
        row.node_view.terminal_ids.hash(&mut hasher);
        format!("{:?}", row.node_view.status()).hash(&mut hasher);
        row.depth.hash(&mut hasher);
        row.is_last.hash(&mut hasher);
        row.has_children.hash(&mut hasher);
        row.standalone_session.hash(&mut hasher);
        row.standalone_session
            .as_ref()
            .is_some_and(|session| {
                self.expanded_standalone_connections
                    .contains(&session.connection_id)
            })
            .hash(&mut hasher);
        self.disclosure_motions
            .signature(&format!("session:{}:", row.node_id.0))
            .hash(&mut hasher);
        self.expanded_ssh_nodes
            .contains(&row.node_id)
            .hash(&mut hasher);
        self.has_active_reconnect_job(&row.node_id, cx)
            .hash(&mut hasher);
        (self.active_ssh_node_id.as_ref() == Some(&row.node_id)).hash(&mut hasher);
        hasher.finish()
    }

    pub(in crate::workspace) fn effective_active_session_focus_node_id(
        &self,
        rows: &[ActiveSessionSidebarRow],
    ) -> Option<NodeId> {
        let focused_node_id = self.active_session_sidebar_focused_node_id.as_ref()?;
        rows.iter()
            .any(|row| row.node_id == *focused_node_id)
            .then(|| focused_node_id.clone())
    }

    pub(in crate::workspace) fn active_session_focus_rows(
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

    pub(in crate::workspace) fn active_session_breadcrumb_rows(
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

    pub(in crate::workspace) fn toggle_active_session_sidebar_view(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.active_session_sidebar_view_mode = match self.active_session_sidebar_view_mode {
            ActiveSessionSidebarViewMode::Tree => ActiveSessionSidebarViewMode::Focus,
            ActiveSessionSidebarViewMode::Focus => ActiveSessionSidebarViewMode::Tree,
        };
        // Tauri stores the focus node separately from expansion. Keep native's
        // selected node visible when entering focus mode, but fall back to root
        // if the selected node is stale or has disappeared.
        if self.active_session_sidebar_view_mode == ActiveSessionSidebarViewMode::Focus {
            let rows = self.active_session_sidebar_rows(cx);
            let selected = self
                .active_ssh_node_id
                .as_ref()
                .filter(|node_id| rows.iter().any(|row| row.node_id == **node_id))
                .cloned();
            self.active_session_sidebar_focused_node_id = selected;
        }
        cx.notify();
    }

    pub(in crate::workspace) fn render_active_session_focus_breadcrumb(
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
            // The sidebar body already owns the background tint.
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
                .child(Self::render_lucide_icon(
                    LucideIcon::Home,
                    14.0,
                    rgb(root_color),
                ))
                .when(root_active, |button| {
                    button.child(self.render_session_control_label(
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
                        .child(self.render_session_control_label(
                            "session-focus-breadcrumb",
                            &node_id,
                            title,
                            text_color,
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

    pub(in crate::workspace) fn render_active_session_focus_location_header(
        &self,
        rows: &[ActiveSessionSidebarRow],
        focused_node_id: Option<&NodeId>,
        visible_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let focused_row =
            focused_node_id.and_then(|node_id| rows.iter().find(|row| row.node_id == *node_id));
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
            .child(if focused_node_id.is_some() {
                "📍"
            } else {
                "🏠"
            })
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

    pub(in crate::workspace) fn render_active_session_focus_empty(
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
            .child(div().mb_2().child(Self::render_lucide_icon(
                LucideIcon::Server,
                SESSION_FOCUS_EMPTY_ICON_SIZE,
                rgba((theme.text_muted << 8) | SESSION_FOCUS_EMPTY_ICON_ALPHA),
            )))
            .child(
                div()
                    .text_size(px(SESSION_FOCUS_EMPTY_TITLE_TEXT_SIZE))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
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
                        SelectableTextRole::NonSelectable,
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

    pub(in crate::workspace) fn render_active_session_focus_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.active_session_sidebar_rows(cx);
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

    pub(in crate::workspace) fn render_active_session_focus_node(
        &self,
        row: ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if row.standalone_session.is_some() {
            return self.render_standalone_session_sidebar_row(row, cx);
        }
        let theme = self.tokens.ui;
        let selected = self.active_ssh_node_id.as_ref() == Some(&row.node_id);
        let status = self.session_node_status(row.node_view.status());
        let connected = matches!(
            row.node_view.status(),
            ActiveSessionStatus::Active | ActiveSessionStatus::Connected
        );
        let connecting = matches!(row.node_view.status(), ActiveSessionStatus::Connecting);
        let subtitle = format!("{}@{}:{}", row.username, row.host, row.port);
        let terminal_count = row.node_view.terminal_ids.len();
        let has_children = row.has_children;
        let action_label = self.i18n.t("sessions.actions.connect");
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
                    .child(if matches!(status.icon, LucideIcon::LoaderCircle) {
                        self.render_loading_icon(
                            (
                                gpui::SharedString::from(format!(
                                    "session-focus-connecting-{:?}",
                                    row.node_id
                                )),
                                0usize,
                            ),
                            SESSION_TREE_ICON_SIZE,
                            rgb(status.text_color),
                        )
                    } else {
                        Self::render_lucide_icon(
                            status.icon,
                            SESSION_TREE_ICON_SIZE,
                            rgb(status.text_color),
                        )
                    })
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
                                    .child(self.render_session_control_label(
                                        "session-focus-card-cell",
                                        "title",
                                        row.node_view.title.clone(),
                                        status.text_color,
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.render_session_control_label(
                                        "session-focus-card-cell",
                                        "subtitle",
                                        subtitle,
                                        theme.text_muted,
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
                                        let _ = this.queue_ssh_terminal_tab_for_sidebar_node(
                                            node_id.clone(),
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
                            self.render_active_session_focus_terminal(*session_id, index + 1, cx)
                        },
                    )),
            );
        }

        if selected && connected {
            card = card.child(
                div()
                    .flex()
                    .flex_row()
                    // Sidebar width is user-controlled, so actions must form
                    // additional rows instead of extending beyond the card.
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.0))
                    .children(self.render_active_session_focus_actions(&row, cx)),
            );
        }

        if selected {
            card = card.children(self.render_session_node_lifecycle_items(
                &row.node_id,
                row.node_view.status(),
                0,
                true,
                cx,
            ));
        }

        card.into_any_element()
    }

    pub(in crate::workspace) fn render_active_session_focus_terminal(
        &self,
        session_id: TerminalSessionId,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_terminal_session_id(cx) == Some(session_id);
        let text_color = if active {
            theme.accent
        } else {
            theme.text_muted
        };
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
                    .child(self.render_session_control_label(
                        "session-focus-terminal-cell",
                        "label",
                        text,
                        text_color,
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
                    .child(Self::render_lucide_icon(
                        LucideIcon::X,
                        12.0,
                        rgb(text_color),
                    ))
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

    pub(in crate::workspace) fn render_active_session_focus_actions(
        &self,
        row: &ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let node_id = row.node_id.clone();
        vec![
            self.render_active_session_focus_action_chip(
                LucideIcon::Plus,
                self.i18n.t("sessions.tree.actions.new_terminal"),
                SessionActionVariant::Primary,
                cx.listener(move |this, _event, window, cx| {
                    let _ =
                        this.queue_ssh_terminal_tab_for_sidebar_node(node_id.clone(), window, cx);
                    cx.stop_propagation();
                }),
                cx,
            ),
            {
                let node_id = row.node_id.clone();
                self.render_active_session_focus_action_chip(
                    LucideIcon::FolderOpen,
                    self.i18n.t("sessions.tree.actions.sftp"),
                    SessionActionVariant::Primary,
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
                    SessionActionVariant::Primary,
                    cx.listener(move |this, _event, window, cx| {
                        this.open_forwards_tab(node_id.clone(), window, cx);
                        cx.stop_propagation();
                    }),
                    cx,
                )
            },
        ]
    }

    pub(in crate::workspace) fn render_active_session_focus_action_chip(
        &self,
        icon: LucideIcon,
        label: String,
        variant: SessionActionVariant,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (text_color, background, hover_background) = match variant {
            SessionActionVariant::Primary => (
                theme.accent,
                rgba((theme.accent << 8) | SESSION_FOCUS_ACTION_BG_ALPHA),
                rgba((theme.accent << 8) | SESSION_FOCUS_ACTION_HOVER_ALPHA),
            ),
            SessionActionVariant::Danger => (
                theme.error,
                rgba((theme.error << 8) | SESSION_FOCUS_ACTION_BG_ALPHA),
                rgba((theme.error << 8) | SESSION_FOCUS_ACTION_HOVER_ALPHA),
            ),
        };
        div()
            .h(px(24.0))
            .max_w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(7.0))
            .text_size(px(11.0))
            .text_color(rgb(text_color))
            .bg(background)
            .hover(move |chip| chip.bg(hover_background))
            .child(Self::render_lucide_icon(icon, 12.0, rgb(text_color)))
            .child(
                // Long localized labels stay inside the chip at the narrowest
                // supported sidebar widths.
                div().min_w(px(0.0)).truncate().child(label),
            )
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    pub(in crate::workspace) fn render_active_session_node(
        &self,
        row: ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if row.standalone_session.is_some() {
            return self.render_standalone_session_sidebar_row(row, cx);
        }
        let node_id = row.node_id;
        let node_view = row.node_view;
        let node_depth = row.depth;
        let is_last = row.is_last;
        let expanded = self.expanded_ssh_nodes.contains(&node_id);
        let motion_key = format!("session:{}:children", node_id.0);
        let selected = self.active_ssh_node_id.as_ref() == Some(&node_id);
        let status = self.session_node_status(node_view.status());
        let terminal_ids = node_view.terminal_ids.clone();
        let mut children = Vec::new();

        if self.disclosure_motions.retained(&motion_key, expanded)
            && !self.has_active_reconnect_job(&node_id, cx)
        {
            if matches!(
                node_view.status(),
                ActiveSessionStatus::Active | ActiveSessionStatus::Connected
            ) {
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    move |this, _event, window, cx| {
                        let _ = this.queue_ssh_terminal_tab_for_sidebar_node(
                            node_id.clone(),
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
                if self.can_save_runtime_node_as_connection(
                    &node_id,
                    row.saved_connection_id.as_deref(),
                ) {
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
            } else if matches!(node_view.status(), ActiveSessionStatus::Error) {
                let listener = cx.listener({
                    let node_id = node_id.clone();
                    let saved_connection_id = row.saved_connection_id;
                    move |this, _event, window, cx| {
                        if let Some(saved_connection_id) = saved_connection_id.as_deref() {
                            this.open_saved_connection_reconnect_editor(
                                node_id.clone(),
                                saved_connection_id,
                                window,
                                cx,
                            );
                        } else {
                            this.open_runtime_node_reconnect_editor(node_id.clone(), window, cx);
                        }
                        cx.stop_propagation();
                    }
                });
                children.push(self.render_session_action_item(
                    node_depth + 1,
                    false,
                    LucideIcon::Pencil,
                    self.i18n.t("sessions.tree.actions.edit_and_reconnect"),
                    SessionActionVariant::Primary,
                    listener,
                    cx,
                ));
            }
        }

        if self.disclosure_motions.retained(&motion_key, expanded) {
            children.extend(self.render_session_node_lifecycle_items(
                &node_id,
                node_view.status(),
                node_depth + 1,
                is_last,
                cx,
            ));
        }

        let header =
            self.render_session_node_header(node_id, node_view, expanded, selected, status, cx);
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
            .when(!children.is_empty(), |node| {
                node.child(self.disclosure_motions.render(
                    &motion_key,
                    &self.tokens,
                    div().w_full().flex().flex_col().children(children),
                    None,
                ))
            })
            .into_any_element()
    }

    fn session_sidebar_row(&self, selected: bool, height: f32) -> gpui::Div {
        let theme = self.tokens.ui;
        div()
            .relative()
            .w_full()
            .min_w_0()
            .h(px(height))
            .px_2()
            .flex()
            .items_center()
            .rounded_none()
            .cursor_pointer()
            .bg(if selected {
                rgba((theme.accent << 8) | SESSION_FOCUS_TERMINAL_ACTIVE_BG_ALPHA)
            } else {
                rgba(0x00000000)
            })
            .hover(move |row| row.bg(rgb(theme.bg_hover)))
            .when(selected, |row| {
                row.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(4.0))
                        .bottom(px(4.0))
                        .w(px(2.0))
                        .bg(rgb(theme.accent)),
                )
            })
    }

    fn render_session_node_lifecycle_items(
        &self,
        node_id: &NodeId,
        status: ActiveSessionStatus,
        depth: usize,
        is_last: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let actions = session_node_row_actions(status, self.has_active_reconnect_job(node_id, cx));
        let last = actions.iter().rposition(Option::is_some);
        actions
            .into_iter()
            .enumerate()
            .filter_map(|(index, action)| {
                let action = action?;
                let (icon, key, danger) = match action {
                    SessionNodeRowAction::Connect => {
                        (LucideIcon::Power, "sessions.actions.connect", false)
                    }
                    SessionNodeRowAction::Reconnect => {
                        (LucideIcon::RefreshCw, "sessions.actions.reconnect", false)
                    }
                    SessionNodeRowAction::Disconnect => (
                        LucideIcon::WifiOff,
                        "sessions.tree.actions.disconnect",
                        true,
                    ),
                    SessionNodeRowAction::CancelReconnect => (
                        LucideIcon::X,
                        "sessions.tree.actions.cancel_reconnect",
                        false,
                    ),
                    SessionNodeRowAction::Remove => (
                        LucideIcon::Trash2,
                        "sessions.tree.actions.remove_session",
                        true,
                    ),
                };
                let id = node_id.clone();
                Some(self.render_session_action_item(
                    depth,
                    is_last && last == Some(index),
                    icon,
                    self.i18n.t(key),
                    if danger {
                        SessionActionVariant::Danger
                    } else {
                        SessionActionVariant::Primary
                    },
                    cx.listener(move |this, _, window, cx| {
                        match action {
                            SessionNodeRowAction::Remove => {
                                this.remove_inactive_session_tree_node(&id, window, cx)
                            }
                            SessionNodeRowAction::CancelReconnect => {
                                this.cancel_reconnect_for_node(&id, cx)
                            }
                            SessionNodeRowAction::Disconnect => {
                                this.request_disconnect_ssh_node(&id, window, cx)
                            }
                            SessionNodeRowAction::Connect | SessionNodeRowAction::Reconnect => {
                                let _ = this.queue_ssh_terminal_tab_for_sidebar_node(
                                    id.clone(),
                                    window,
                                    cx,
                                );
                            }
                        }
                        cx.stop_propagation();
                    }),
                    cx,
                ))
            })
            .collect()
    }

    fn render_standalone_session_sidebar_row(
        &self,
        row: ActiveSessionSidebarRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = row.standalone_session else {
            return div().into_any_element();
        };
        let serial_details = self
            .standalone_connections
            .record(&session.connection_id)
            .and_then(|record| match &record.launch {
                standalone_connections::StandaloneConnectionLaunch::Serial { config, .. }
                | standalone_connections::StandaloneConnectionLaunch::SavedSerial {
                    config, ..
                } => Some(format!("{} · {}", config.port_path, config.baud_rate)),
                _ => None,
            });
        let theme = self.tokens.ui;
        let active = match session.target {
            Some(standalone_connections::StandaloneConnectionSurface::Terminal(session_id)) => {
                self.active_terminal_session_id(cx) == Some(session_id)
            }
            Some(standalone_connections::StandaloneConnectionSurface::RemoteDesktop(tab_id)) => {
                self.active_tab_id(cx) == Some(tab_id)
            }
            None => false,
        };
        let status = self.session_node_status(row.node_view.status());
        let expanded = self
            .expanded_standalone_connections
            .contains(&session.connection_id);
        let motion_key = format!("session:{}:children", row.node_id.0);
        let expand_id = session.connection_id.clone();
        let expand_motion_key = motion_key.clone();
        let mut children = Vec::new();
        if self.disclosure_motions.retained(&motion_key, expanded) {
            match session.target {
                Some(standalone_connections::StandaloneConnectionSurface::Terminal(id)) => {
                    children.push(self.render_session_terminal_item(1, false, id, 1, cx));
                }
                Some(standalone_connections::StandaloneConnectionSurface::RemoteDesktop(id)) => {
                    children.push(self.render_session_action_item(
                        1,
                        false,
                        session.kind.icon(),
                        self.i18n.t("sessions.tree.actions.open_session"),
                        SessionActionVariant::Primary,
                        cx.listener(move |this, _, window, cx| {
                            this.set_active_tab(id, window, cx);
                            cx.stop_propagation();
                        }),
                        cx,
                    ));
                }
                None => {}
            }
            let actions = standalone_session_actions(row.node_view.status());
            let last = actions.iter().rposition(Option::is_some);
            for (index, action) in actions.into_iter().enumerate() {
                let Some(action) = action else {
                    continue;
                };
                let connection_id = session.connection_id.clone();
                let (icon, key, variant) = match action {
                    SessionNodeRowAction::Disconnect => (
                        LucideIcon::WifiOff,
                        "sessions.tree.actions.disconnect",
                        SessionActionVariant::Danger,
                    ),
                    SessionNodeRowAction::Remove => (
                        LucideIcon::Trash2,
                        "sessions.tree.actions.remove_session",
                        SessionActionVariant::Danger,
                    ),
                    _ => (
                        LucideIcon::RefreshCw,
                        "sessions.actions.reconnect",
                        SessionActionVariant::Primary,
                    ),
                };
                children.push(self.render_session_action_item(
                    1,
                    last == Some(index),
                    icon,
                    self.i18n.t(key),
                    variant,
                    cx.listener(move |this, _, window, cx| {
                        match action {
                            SessionNodeRowAction::Disconnect => {
                                this.disconnect_standalone_connection(&connection_id, window, cx)
                            }
                            SessionNodeRowAction::Remove => {
                                this.remove_standalone_connection(&connection_id, window, cx)
                            }
                            _ => this.reconnect_standalone_connection(&connection_id, window, cx),
                        }
                        cx.stop_propagation();
                    }),
                    cx,
                ));
            }
        }
        let header = self
            .reorderable_session_row(
                self.session_sidebar_row(active, SESSION_TREE_NODE_HEIGHT),
                row.node_id.clone(),
                None,
                row.title.clone(),
                cx,
            )
            .gap(px(6.0))
            .child(self.render_animated_chevron(
                (
                    SharedString::from(format!("standalone-chevron-{}", session.connection_id)),
                    expanded as usize,
                ),
                expanded,
                12.0,
                rgb(theme.text_muted),
            ))
            .child(Self::render_lucide_icon(
                session.kind.icon(),
                SESSION_TREE_ICON_SIZE,
                rgb(theme.text_muted),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(SESSION_TREE_TEXT_SIZE))
                            .when(serial_details.is_some(), |title| {
                                title.line_height(px(SESSION_TREE_NODE_HEIGHT / 2.0))
                            })
                            .text_color(rgb(theme.text))
                            .child(row.title),
                    )
                    .when_some(serial_details, |label, details| {
                        label.child(
                            div()
                                .truncate()
                                .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                                .line_height(px(SESSION_TREE_NODE_HEIGHT / 2.0))
                                .text_color(rgb(theme.text_muted))
                                .child(details),
                        )
                    }),
            )
            .child(self.render_session_status_dot(status))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this
                    .expanded_standalone_connections
                    .insert(expand_id.clone())
                {
                    this.expanded_standalone_connections.remove(&expand_id);
                }
                this.begin_disclosure_motion(
                    expand_motion_key.clone(),
                    this.expanded_standalone_connections.contains(&expand_id),
                    cx,
                );
                cx.stop_propagation();
                cx.notify();
            }));
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .when(!children.is_empty(), |row| {
                row.child(self.disclosure_motions.render(
                    &motion_key,
                    &self.tokens,
                    div().w_full().flex().flex_col().children(children),
                    None,
                ))
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn can_save_runtime_node_as_connection(
        &self,
        node_id: &NodeId,
        saved_connection_id: Option<&str>,
    ) -> bool {
        if saved_connection_id.is_some() {
            return false;
        }
        let Some(snapshot) = self.node_router.node_metadata(node_id) else {
            return true;
        };
        // ManualPreset/Restored are already saved-connection materializations,
        // while legacy AutoRoute nodes came from derived topology. Only live
        // drill-down nodes and genuinely unsaved direct nodes expose this action.
        matches!(
            snapshot.origin,
            NodeOrigin::DrillDown { .. } | NodeOrigin::Direct
        )
    }

    pub(in crate::workspace) fn render_session_node_header(
        &self,
        node_id: NodeId,
        node: ActiveSessionNode,
        expanded: bool,
        selected: bool,
        status: SessionStatusStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let muted_text = rgb(theme.text_muted);
        let row_text = rgb(theme.text);
        let port_text = format!(":{}", node.port);
        let terminal_count = node.terminal_ids.len();
        self.reorderable_session_row(
            self.session_sidebar_row(selected, SESSION_TREE_NODE_HEIGHT),
            node_id.clone(),
            self.node_router
                .node_metadata(&node_id)
                .and_then(|node| node.parent_id),
            node.title.clone(),
            cx,
        )
        .child(self.render_animated_chevron(
            (
                gpui::SharedString::from(format!("session-node-chevron-{}", node_id.0)),
                expanded as usize,
            ),
            expanded,
            12.0,
            muted_text,
        ))
        .child(
            div()
                .ml_1()
                .mr(px(6.0))
                .child(if matches!(status.icon, LucideIcon::LoaderCircle) {
                    self.render_loading_icon(
                        (
                            gpui::SharedString::from(format!("session-connecting-{node_id:?}")),
                            0usize,
                        ),
                        SESSION_TREE_ICON_SIZE,
                        row_text,
                    )
                } else {
                    Self::render_lucide_icon(LucideIcon::Server, SESSION_TREE_ICON_SIZE, muted_text)
                }),
        )
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
                .child(self.render_session_control_label(
                    "session-sidebar-node-cell",
                    "title",
                    node.title,
                    theme.text,
                    cx,
                )),
        )
        .when(node.port != 22, |row| {
            row.child(
                div()
                    .ml_2()
                    .text_size(px(SESSION_TREE_META_TEXT_SIZE))
                    .text_color(muted_text)
                    .child(self.render_session_control_label(
                        "session-sidebar-node-cell",
                        "port",
                        port_text,
                        theme.text_muted,
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
                    .child(self.render_session_control_label(
                        "session-sidebar-node-cell",
                        "terminal-count",
                        terminal_count.to_string(),
                        theme.text_muted,
                        cx,
                    )),
            )
        })
        .child(self.render_session_status_dot(status))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.active_ssh_node_id = Some(node_id.clone());
            if !this.expanded_ssh_nodes.insert(node_id.clone()) {
                this.expanded_ssh_nodes.remove(&node_id);
            }
            this.begin_disclosure_motion(
                format!("session:{}:children", node_id.0),
                this.expanded_ssh_nodes.contains(&node_id),
                cx,
            );
            cx.stop_propagation();
            cx.notify();
        }))
        .into_any_element()
    }

    pub(in crate::workspace) fn render_session_status_dot(
        &self,
        status: SessionStatusStyle,
    ) -> AnyElement {
        div()
            .ml(px(6.0))
            .size(px(12.0))
            .flex_none()
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

    pub(in crate::workspace) fn render_session_terminal_item(
        &self,
        depth: usize,
        line_stops_here: bool,
        session_id: TerminalSessionId,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_terminal_session_id(cx) == Some(session_id);
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
                .rounded_none()
                .px_2()
                .cursor_pointer()
                .bg(row_bg)
                .hover(move |row| row.bg(rgb(theme.bg_hover)))
                .when(active, |row| {
                    row.child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(4.0))
                            .bottom(px(4.0))
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
                        .ml(px(6.0))
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
                        .child(self.render_session_control_label(
                            "session-sidebar-terminal-cell",
                            "label",
                            text,
                            if active {
                                theme.accent
                            } else {
                                theme.text_muted
                            },
                            cx,
                        )),
                )
                .child(
                    div()
                        .size(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_none()
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

    pub(in crate::workspace) fn render_session_action_item(
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
            SessionActionVariant::Danger => {
                (theme.error, mix_rgb(theme.bg_hover, theme.error, 0.10))
            }
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
                .gap(px(6.0))
                .rounded_none()
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
                .child(div().truncate().child(self.render_session_control_label(
                    "session-sidebar-action-cell",
                    "label",
                    label,
                    text_color,
                    cx,
                )))
                .on_mouse_down(MouseButton::Left, listener)
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_session_tree_child(
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

    pub(in crate::workspace) fn session_node_status(
        &self,
        status: ActiveSessionStatus,
    ) -> SessionStatusStyle {
        match status {
            ActiveSessionStatus::Connecting => SessionStatusStyle {
                icon: LucideIcon::LoaderCircle,
                text_color: self.tokens.ui.info,
                dot_color: self.tokens.ui.info,
                ring: false,
            },
            ActiveSessionStatus::Active => SessionStatusStyle {
                icon: LucideIcon::Server,
                text_color: self.tokens.ui.success,
                dot_color: self.tokens.ui.success,
                ring: true,
            },
            ActiveSessionStatus::Connected => SessionStatusStyle {
                icon: LucideIcon::Server,
                text_color: self.tokens.ui.success,
                dot_color: self.tokens.ui.success,
                ring: true,
            },
            ActiveSessionStatus::Error => SessionStatusStyle {
                icon: LucideIcon::WifiOff,
                text_color: self.tokens.ui.error,
                dot_color: self.tokens.ui.error,
                ring: false,
            },
            ActiveSessionStatus::Idle => SessionStatusStyle {
                icon: LucideIcon::Server,
                text_color: self.tokens.ui.text_muted,
                dot_color: self.tokens.ui.text_muted,
                ring: false,
            },
        }
    }
}

#[cfg(test)]
mod terminal_open_tests {
    use super::*;

    #[test]
    fn node_row_actions_keep_disconnect_and_removal_separate() {
        use SessionNodeRowAction::*;
        for (status, expected) in [
            (ActiveSessionStatus::Active, [Some(Disconnect), None]),
            (ActiveSessionStatus::Connected, [Some(Disconnect), None]),
            (ActiveSessionStatus::Connecting, [None, None]),
            (ActiveSessionStatus::Error, [Some(Reconnect), Some(Remove)]),
            (ActiveSessionStatus::Idle, [Some(Connect), Some(Remove)]),
        ] {
            assert_eq!(session_node_row_actions(status, false), expected);
            assert_eq!(
                session_node_row_actions(status, true),
                [Some(CancelReconnect), None]
            );
        }
    }

    #[test]
    fn standalone_lifecycle_actions_do_not_remove_live_or_connecting_sessions() {
        use SessionNodeRowAction::*;
        for (status, expected) in [
            (ActiveSessionStatus::Active, [Some(Disconnect), None]),
            (ActiveSessionStatus::Connected, [Some(Disconnect), None]),
            (ActiveSessionStatus::Connecting, [Some(Disconnect), None]),
            (ActiveSessionStatus::Error, [Some(Reconnect), Some(Remove)]),
            (ActiveSessionStatus::Idle, [Some(Reconnect), Some(Remove)]),
        ] {
            assert_eq!(standalone_session_actions(status), expected);
        }
    }

    #[test]
    fn sidebar_terminal_uses_current_saved_command_or_temporary_node_command() {
        let node_id = NodeId::new("temporary");
        let router = NodeRouter::new(SshConnectionRegistry::new(ConnectionPoolConfig::default()));
        router.upsert_node(
            node_id.clone(),
            SshConfig {
                post_connect_command: Some("cd /srv/original".into()),
                ..Default::default()
            },
        );
        let mut saved: oxideterm_connections::SavedConnection =
            serde_json::from_value(serde_json::json!({
                "id": "saved", "name": "Saved connection", "host": "example.com",
                "port": 22, "username": "ops", "auth": { "type": "password" },
                "created_at": "2026-01-01T00:00:00Z"
            }))
            .unwrap();
        for command in [Some("cd /srv/updated"), None] {
            saved.post_connect_command = command.map(str::to_owned);
            assert_eq!(
                sidebar_terminal_post_connect_command(Some(&saved), &router, &node_id).as_deref(),
                command,
            );
        }
        assert_eq!(
            sidebar_terminal_post_connect_command(None, &router, &node_id).as_deref(),
            Some("cd /srv/original"),
        );
    }
}

#[cfg(test)]
mod sorting_tests {
    use super::*;

    fn row(id: &str, title: &str, parent: Option<&str>, ready: bool) -> ActiveSessionSidebarRow {
        ActiveSessionSidebarRow {
            node_id: NodeId::new(id),
            parent_id: parent.map(NodeId::new),
            saved_connection_id: None,
            title: title.into(),
            host: String::new(),
            username: String::new(),
            port: 22,
            node_view: ActiveSessionNode {
                id: id.into(),
                title: title.into(),
                port: 22,
                terminal_ids: Vec::new(),
                readiness: if ready {
                    ActiveSessionReadiness::Ready
                } else {
                    ActiveSessionReadiness::Disconnected
                },
            },
            depth: usize::from(parent.is_some()),
            is_last: false,
            has_children: false,
            standalone_session: None,
        }
    }

    #[test]
    fn manual_reorder_moves_siblings_with_their_subtrees() {
        let ids = |rows: &[ActiveSessionSidebarRow]| {
            rows.iter()
                .map(|row| row.node_id.0.as_str())
                .collect::<Vec<_>>()
                .join(",")
        };
        let rows = vec![
            row("ssh", "SSH", None, true),
            row("child", "Child", Some("ssh"), true),
            row("mosh", "Mosh", None, true),
            row("last", "Last", None, false),
        ];
        let order =
            reordered_session_ids(&rows, &NodeId::new("ssh"), &NodeId::new("last")).unwrap();
        let sorted = sort_active_session_rows(rows.clone(), SessionSortOrder::Default, &order);
        assert_eq!(ids(&sorted), "mosh,last,ssh,child");
        assert_eq!(sorted[3].parent_id, Some(NodeId::new("ssh")));
        let order =
            reordered_session_ids(&sorted, &NodeId::new("ssh"), &NodeId::new("mosh")).unwrap();
        assert_eq!(
            ids(&sort_active_session_rows(
                sorted,
                SessionSortOrder::Default,
                &order
            )),
            "ssh,child,mosh,last"
        );
        assert_eq!(
            reordered_session_ids(&rows, &NodeId::new("child"), &NodeId::new("mosh")),
            None
        );
        assert_eq!(
            reordered_session_ids(&rows, &NodeId::new("missing"), &NodeId::new("mosh")),
            None
        );
        let order = vec!["mosh".to_string(), "ssh".to_string()];
        assert_eq!(
            ids(&sort_active_session_rows(
                rows,
                SessionSortOrder::Default,
                &order
            )),
            "mosh,ssh,child,last"
        );
    }

    #[test]
    fn active_connection_total_counts_owners_across_protocols() {
        let mut ssh = row("ssh", "SSH", None, true);
        ssh.node_view.terminal_ids = vec![TerminalSessionId(1), TerminalSessionId(2)];
        let sftp_only = row("sftp", "SFTP only", None, true);
        let jump_child = row("child", "Jump child", Some("ssh"), true);
        let mut rows = vec![ssh, sftp_only, jump_child];
        assert_eq!(active_connection_count(&rows), 3);
        for (index, kind) in [
            standalone_connections::StandaloneConnectionKind::Mosh,
            standalone_connections::StandaloneConnectionKind::Telnet,
            standalone_connections::StandaloneConnectionKind::Serial,
            standalone_connections::StandaloneConnectionKind::Rdp,
            standalone_connections::StandaloneConnectionKind::Vnc,
        ]
        .into_iter()
        .enumerate()
        {
            let id = format!("standalone-{index}");
            let mut standalone = row(&id, "Standalone", None, true);
            standalone.standalone_session = Some(StandaloneActiveSession {
                connection_id: id,
                kind,
                target: None,
            });
            rows.push(standalone);
            assert_eq!(active_connection_count(&rows), 4 + index, "{kind:?}");
        }
        for readiness in [
            ActiveSessionReadiness::Disconnected,
            ActiveSessionReadiness::Error,
            ActiveSessionReadiness::Connecting,
        ] {
            let mut inactive = row("inactive", "Inactive", None, false);
            inactive.node_view.readiness = readiness;
            rows.push(inactive);
        }
        assert_eq!(active_connection_count(&rows), 8);
        rows[0].node_view.readiness = ActiveSessionReadiness::Disconnected;
        assert_eq!(active_connection_count(&rows), 7);
    }

    #[test]
    fn session_search_retains_ancestors_and_respects_sort_order() {
        let mut a = row("a", "Alpha", Some("parent"), true);
        a.host = "prod.example".into();
        a.username = "ops".into();
        let mut z = row("z", "Zulu", Some("parent"), false);
        z.host = "prod.example".into();
        z.username = "ops".into();
        let rows = vec![
            row("parent", "Gateway", None, true),
            z,
            a,
            row("other", "Unrelated", None, true),
        ];
        for (order, expected) in [
            (SessionSortOrder::NameAscending, vec!["parent", "a", "z"]),
            (SessionSortOrder::NameDescending, vec!["parent", "z", "a"]),
        ] {
            let sorted = sort_active_session_rows(rows.clone(), order, &[]);
            let filtered = filter_active_session_rows(sorted, " PROD ops ");
            assert_eq!(
                filtered
                    .iter()
                    .map(|row| row.node_id.0.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(filtered[0].has_children);
            assert!(!filtered[1].is_last);
            assert!(filtered[2].is_last);
        }
        assert_eq!(
            filter_active_session_rows(rows.clone(), " ")
                .iter()
                .map(|row| row.node_id.0.as_str())
                .collect::<Vec<_>>(),
            vec!["parent", "z", "a", "other"]
        );
        assert!(filter_active_session_rows(rows, "missing").is_empty());
    }

    #[test]
    fn session_sort_preserves_subtrees_and_updates_branch_ends() {
        let rows = vec![
            row("parent", "Zulu", None, true),
            row("z", "zeta", Some("parent"), false),
            row("a", "Alpha", Some("parent"), true),
            row("other", "Beta", None, false),
        ];
        let ids = |rows: &[ActiveSessionSidebarRow]| {
            rows.iter()
                .map(|row| row.node_id.0.as_str())
                .collect::<Vec<_>>()
                .join(",")
        };
        assert_eq!(
            ids(&sort_active_session_rows(
                rows.clone(),
                SessionSortOrder::Default,
                &[]
            )),
            "parent,z,a,other"
        );
        let sorted = sort_active_session_rows(rows.clone(), SessionSortOrder::NameAscending, &[]);
        assert_eq!(ids(&sorted), "other,parent,a,z");
        assert_eq!(
            sorted
                .iter()
                .map(|row| (row.depth, row.is_last))
                .collect::<Vec<_>>(),
            [(0, false), (0, true), (1, false), (1, true)]
        );
        assert_eq!(
            ids(&sort_active_session_rows(
                rows.clone(),
                SessionSortOrder::NameDescending,
                &[]
            )),
            "parent,z,a,other"
        );
        assert_eq!(
            ids(&sort_active_session_rows(
                rows,
                SessionSortOrder::ConnectedFirst,
                &[]
            )),
            "parent,a,z,other"
        );
    }
}

#[derive(Clone)]
struct ActiveSessionDrag {
    id: NodeId,
    parent: Option<NodeId>,
    title: String,
    tokens: ThemeTokens,
}

impl Render for ActiveSessionDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .text_color(rgb(self.tokens.ui.text))
            .text_size(px(12.0))
            .child(self.title.clone())
    }
}

fn reordered_session_ids(
    rows: &[ActiveSessionSidebarRow],
    source: &NodeId,
    target: &NodeId,
) -> Option<Vec<String>> {
    if source == target {
        return None;
    }
    let source_row = rows.iter().find(|row| &row.node_id == source)?;
    let target_row = rows.iter().find(|row| &row.node_id == target)?;
    if source_row.parent_id != target_row.parent_id {
        return None;
    }
    let mut siblings: Vec<_> = rows
        .iter()
        .filter(|row| row.parent_id == source_row.parent_id)
        .map(|row| row.node_id.0.clone())
        .collect();
    let from = siblings.iter().position(|id| id == &source.0)?;
    let to = siblings.iter().position(|id| id == &target.0)?;
    let moved = siblings.remove(from);
    siblings.insert(to, moved);
    let mut reordered = siblings.into_iter();
    Some(
        rows.iter()
            .map(|row| {
                if row.parent_id == source_row.parent_id {
                    reordered.next().unwrap()
                } else {
                    row.node_id.0.clone()
                }
            })
            .collect(),
    )
}

impl WorkspaceApp {
    fn reorderable_session_row(
        &self,
        row: Div,
        id: NodeId,
        parent: Option<NodeId>,
        title: String,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let drag = ActiveSessionDrag {
            id: id.clone(),
            parent: parent.clone(),
            title,
            tokens: self.tokens,
        };
        let drop_id = id.clone();
        let accent = self.tokens.ui.accent;
        row.id(SharedString::from(format!("session-reorder-{}", id.0)))
            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
            .can_drop(move |value, _, _| {
                value
                    .downcast_ref::<ActiveSessionDrag>()
                    .is_some_and(|drag| drag.parent == parent && drag.id != id)
            })
            .drag_over::<ActiveSessionDrag>(move |style, _, _, _| {
                style.bg(rgba((accent << 8) | 0x26))
            })
            .on_drop(cx.listener(move |this, drag: &ActiveSessionDrag, _, cx| {
                let rows = sort_active_session_rows(
                    this.unfiltered_active_session_sidebar_rows(cx),
                    this.settings_store.settings().sidebar_ui.session_sort_order,
                    &this
                        .settings_store
                        .settings()
                        .sidebar_ui
                        .session_manual_order,
                );
                if let Some(order) = reordered_session_ids(&rows, &drag.id, &drop_id) {
                    let sidebar = &mut this.settings_store.settings_mut().sidebar_ui;
                    sidebar.session_manual_order = order;
                    sidebar.session_sort_order = SessionSortOrder::Default;
                    this.persist_sidebar_settings(cx);
                    cx.notify();
                }
                cx.stop_propagation();
            }))
    }
}
