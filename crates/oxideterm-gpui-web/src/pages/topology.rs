use super::*;
use crate::demo::Drive;
use oxideterm_gpui_host_tools::{topology_canvas, topology_node};
use oxideterm_gpui_ui::context_menu::*;
use oxideterm_topology::{
    ConnectionTopologyLayout, ConnectionTopologyNode, ConnectionTopologySnapshot,
    ConnectionTopologyStatus as Status,
};
use oxideterm_workspace::TabId;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum GraphTarget {
    Node(String),
    Files(TabId),
}

pub(super) fn address(node: &ConnectionTopologyNode) -> String {
    if node.port == 0 {
        return node.host.clone();
    }
    if node.username.is_empty() {
        return format!("{}:{}", node.host, node.port);
    }
    oxideterm_ssh_launch::format_user_host_port_target(&node.username, &node.host, node.port)
}

impl Workspace {
    pub(super) fn workspace_topology(
        &self,
        cx: &App,
    ) -> (ConnectionTopologySnapshot, HashMap<String, GraphTarget>) {
        let mut nodes = BTreeMap::new();
        let mut targets = HashMap::new();
        let mut consume =
            |drive: Drive,
             target: GraphTarget,
             consumer: fn(&mut oxideterm_topology::ConnectionTopologyConsumerSummary),
             status: Status| {
                let (id, host, port, username) = match drive {
                    Drive::Node(id) if id == "local" => return,
                    Drive::Node(id) => {
                        let row = self.catalog_data.items.iter().find(|r| r.id() == id);
                        (
                            format!("node:{id}"),
                            row.map_or_else(|| id.clone(), |r| r.host.clone()),
                            row.map_or(0, |r| r.port as u16),
                            row.map_or_else(String::new, |r| r.username.clone()),
                        )
                    }
                    Drive::Endpoint {
                        host,
                        port,
                        username,
                    } => (
                        format!("endpoint:{}:{username}{host}:{port}", username.len()),
                        host,
                        port,
                        username,
                    ),
                };
                let node = nodes
                    .entry(id.clone())
                    .or_insert_with(|| ConnectionTopologyNode {
                        connection_id: id.clone(),
                        parent_connection_id: None,
                        host,
                        port,
                        username,
                        status,
                        depth: 0,
                        ref_count: 0,
                        consumers: Default::default(),
                    });
                consumer(&mut node.consumers);
                node.ref_count = node.consumers.total() as u64;
                if status == Status::Active
                    || (status == Status::Connecting && node.status != Status::Active)
                {
                    node.status = status;
                }
                targets
                    .entry(id)
                    .and_modify(|existing| {
                        if let (GraphTarget::Files(a), GraphTarget::Files(b)) = (existing, &target)
                            && b.0 < a.0
                        {
                            *a = *b;
                        }
                    })
                    .or_insert(target);
            };
        for s in self.sessions.values().filter(|s| !s.is_ble()) {
            let status = if s.connected(self.ready) {
                Status::Active
            } else if s.writable(self.ready) {
                Status::Connecting
            } else {
                Status::LinkDown
            };
            consume(
                Drive::Node(s.node.clone()),
                GraphTarget::Node(s.node.clone()),
                |c| c.terminals += 1,
                status,
            );
        }
        if let Some(files) = &self.files {
            for (tab, p) in &files.sftp_pages {
                let target = &p.read(cx).target;
                let [(left, _), (right, _)] = target.panes();
                for drive in std::iter::once(right.clone()).chain((left != right).then_some(left)) {
                    let action = match &drive {
                        Drive::Node(id) => GraphTarget::Node(id.clone()),
                        Drive::Endpoint { .. } => GraphTarget::Files(*tab),
                    };
                    consume(drive, action, |c| c.sftp += 1, Status::Active);
                }
            }
            for node in self.projects.project_nodes() {
                consume(
                    Drive::Node(node.into()),
                    GraphTarget::Node(node.into()),
                    |c| c.ide += 1,
                    Status::Active,
                );
            }
        }
        (
            ConnectionTopologySnapshot::new(nodes.into_values().collect(), vec![]),
            targets,
        )
    }

    pub(super) fn node_status(&self, status: Status) -> (&'static str, StatusTone, u32) {
        match status {
            Status::Active => (
                "connections.monitor.active",
                StatusTone::Success,
                self.tokens.ui.success,
            ),
            Status::Connecting => (
                "connections.state.connecting",
                StatusTone::Warning,
                self.tokens.ui.warning,
            ),
            _ => (
                "connections.monitor.link_down",
                StatusTone::Error,
                self.tokens.ui.error,
            ),
        }
    }

    pub(crate) fn topology_page(&self, cx: &Context<Self>) -> AnyElement {
        let (snapshot, targets) = self.workspace_topology(cx);
        let theme = self.tokens.ui;
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .p(px(24.))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .child(page_header(
                        &self.tokens,
                        self.i18n.t("topology.page.title"),
                        Some(self.i18n.t("topology.demo_workspace")),
                        None,
                    )),
            )
            .child(self.topology_graph(&snapshot, &targets, cx))
            .into_any_element()
    }

    pub(super) fn topology_graph(
        &self,
        snapshot: &ConnectionTopologySnapshot,
        targets: &HashMap<String, GraphTarget>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let t = &self.tokens;
        let layout = ConnectionTopologyLayout::from_snapshot(snapshot);
        if layout.nodes.is_empty() {
            return div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(rgb(t.ui.text_muted))
                .child(self.i18n.t("topology.page.no_connections"))
                .child(self.i18n.t("topology.page.connect_hint"))
                .into_any_element();
        }
        let Some(monitor) = &self.tools.monitor else {
            return div().into_any_element();
        };
        let controls = monitor.read(cx).graph_controls();
        let transform = controls.read(cx).topology_transform();
        let dragging = controls.read(cx).topology_dragging();
        let mut graph = div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .cursor(if dragging {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .on_scroll(cx.listener({
                let controls = controls.clone();
                move |_, e: &ScrollEvent, _, cx| {
                    if controls.update(cx, |c, cx| c.zoom_topology_graph(e, cx)) {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let controls = controls.clone();
                    move |_, e, _, cx| {
                        controls.update(cx, |c, cx| c.begin_topology_drag(e, cx));
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .on_mouse_move(cx.listener({
                let controls = controls.clone();
                move |_, e, _, cx| {
                    if controls.update(cx, |c, cx| c.pan_topology_graph(e, cx)) {
                        cx.notify();
                        cx.stop_propagation();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |_, _, _, cx| {
                    if controls.update(cx, |c, cx| c.end_topology_drag(cx)) {
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .child(topology_canvas(t.ui, false, transform, layout.edges))
            .child(
                div()
                    .absolute()
                    .top(px(16.))
                    .right(px(16.))
                    .px_2()
                    .py_1()
                    .rounded(px(t.radii.md))
                    .border_1()
                    .border_color(rgb(t.ui.border))
                    .text_color(rgb(t.ui.text_muted))
                    .child(format!("{:.0}%", transform.k * 100.)),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(16.))
                    .left(px(16.))
                    .text_size(px(10.))
                    .text_color(rgb(t.ui.text_muted))
                    .child(self.i18n.t("topology.controls.instructions")),
            );
        for mut node in layout.nodes {
            let Some(target) = targets.get(&node.connection_id).cloned() else {
                continue;
            };
            if let Some(stripped) = node.name.strip_prefix('@') {
                node.name = stripped.to_owned();
            }
            graph = graph.child(
                topology_node(
                    t,
                    false,
                    &node,
                    transform,
                    t.metrics.markdown_code_font_family.into(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |h, e: &MouseDownEvent, _, cx| {
                        if e.click_count == 2 {
                            h.pages.graph_menu = Some((target.clone(), e.position));
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        graph.into_any_element()
    }

    pub(crate) fn topology_menu(&self, w: &Window, cx: &Context<Self>) -> Option<AnyElement> {
        let (target, pos) = self.pages.graph_menu.clone()?;
        let t = &self.tokens;
        let mut items = vec![];
        match &target {
            GraphTarget::Node(id) => {
                if self.sessions.values().any(|s| s.node == *id) {
                    items.push("topology.menu.navigate_session");
                }
                if self.ready {
                    items.push("topology.menu.new_terminal");
                }
                items.push("topology.menu.open_sftp");
            }
            GraphTarget::Files(tab) if self.layout.tab_by_id(*tab).is_some() => {
                items.push("topology.menu.open_sftp")
            }
            _ => {}
        }
        let placement = clamp_context_menu_position(
            pos.x.into(),
            pos.y.into(),
            w.viewport_size().width.into(),
            w.viewport_size().height.into(),
            200.,
            items.len() as f32 * context_menu_item_height_estimate(t) + 8.,
            4.,
        );
        let mut menu = context_menu_content(t)
            .absolute()
            .left(px(placement.x))
            .top(px(placement.y))
            .w(px(200.));
        for key in items {
            let target = target.clone();
            menu = menu.child(context_menu_action(
                context_menu_item(
                    t,
                    self.i18n.t(key),
                    ContextMenuItemKind::Plain,
                    false,
                    false,
                ),
                false,
                false,
                cx.listener(move |h, _, w, cx| {
                    h.pages.graph_menu = None;
                    match &target {
                        GraphTarget::Node(id) if key == "topology.menu.new_terminal" => {
                            h.new_terminal(id.clone(), None, w, cx)
                        }
                        GraphTarget::Node(id) if key == "topology.menu.navigate_session" => {
                            h.reveal_session_node(id, w, cx)
                        }
                        GraphTarget::Node(id) => h.open_node_sftp(id, cx),
                        GraphTarget::Files(tab) if h.layout.tab_by_id(*tab).is_some() => {
                            h.activate_page(*tab, cx)
                        }
                        _ => {}
                    }
                    h.changed(cx);
                    cx.stop_propagation();
                }),
            ));
        }
        let dismiss = cx.listener(|h, _: &MouseDownEvent, _, cx| {
            h.pages.graph_menu = None;
            cx.notify();
            cx.stop_propagation();
        });
        Some(
            context_menu_backdrop()
                .on_mouse_down(MouseButton::Left, dismiss)
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|h, _, _, cx| {
                        h.pages.graph_menu = None;
                        cx.notify();
                        cx.stop_propagation();
                    }),
                )
                .child(context_menu_event_boundary(menu))
                .into_any_element(),
        )
    }
}
