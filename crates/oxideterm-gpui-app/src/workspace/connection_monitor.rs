use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{MouseButton, PathBuilder, canvas, fill, point, rgba};
use gpui_component::scroll::ScrollableElement;
use oxideterm_connection_monitor::{ProfilerState, ResourceSampler};
use oxideterm_gpui_ui::progress::progress;
use oxideterm_gpui_ui::select::{select_option, select_trigger};
use oxideterm_topology::{
    ConnectionTopologyLayout, ConnectionTopologySnapshot, TOPOLOGY_NODE_HEIGHT,
    TOPOLOGY_NODE_WIDTH, TopologyLayoutNode, TopologyViewStatus,
};

use super::*;

const MONITOR_POOL_REFRESH_INTERVAL: Duration = Duration::from_millis(2000);
const MONITOR_SPARKLINE_POINTS: usize = 12;
const MONITOR_CONTENT_MAX_WIDTH: f32 = 1024.0;
const MONITOR_PAGE_PADDING: f32 = 32.0;
const MONITOR_SECTION_GAP: f32 = 32.0;
const MONITOR_SPARKLINE_HEIGHT: f32 = 28.0;
const MONITOR_SPARKLINE_STROKE_WIDTH: f32 = 1.5;
const MONITOR_SPARKLINE_STROKE_ALPHA: u32 = 0x99;
const MONITOR_BORDER_ALPHA: u32 = 0x80;
const MONITOR_SOURCE_ALPHA: u32 = 0x80;
const MONITOR_TINT_ALPHA: u32 = 0x1a;
const MONITOR_EMERALD: u32 = 0x34d399;
const MONITOR_EMERALD_DARK: u32 = 0x10b981;
const MONITOR_AMBER: u32 = 0xf59e0b;
const MONITOR_RED: u32 = 0xef4444;
const MONITOR_BLUE: u32 = 0x3b82f6;
const TOPOLOGY_BG_GRID_STEP: f32 = 40.0;
const TOPOLOGY_BG_GRID_ALPHA: u32 = 0x1a;
const TOPOLOGY_PANEL_BG_ALPHA_20: u32 = 0x33;
const TOPOLOGY_PANEL_BORDER_ALPHA_50: u32 = 0x80;
const TOPOLOGY_MUTED_TEXT_ALPHA_70: u32 = 0xb3;
const TOPOLOGY_INSTRUCTION_ALPHA_60: u32 = 0x99;
const TOPOLOGY_LINE_INACTIVE_ALPHA: u32 = 0x66;
const TOPOLOGY_LINE_GLOW_ALPHA: u32 = 0x26;
const TOPOLOGY_CONNECTED: u32 = 0x22c55e;
const TOPOLOGY_CONNECTING: u32 = 0xeab308;
const TOPOLOGY_FAILED: u32 = 0xef4444;
const TOPOLOGY_DISCONNECTED: u32 = 0x71717a;
const TOPOLOGY_PENDING: u32 = 0xf59e0b;
const TOPOLOGY_ZOOM_INITIAL: f32 = 0.9;
const TOPOLOGY_ZOOM_MIN: f32 = 0.3;
const TOPOLOGY_ZOOM_MAX: f32 = 3.0;
const TOPOLOGY_PAN_INITIAL_X: f32 = 0.0;
const TOPOLOGY_PAN_INITIAL_Y: f32 = 50.0;
const TOPOLOGY_MENU_WIDTH: f32 = 180.0;
const TOPOLOGY_MENU_MAX_HEIGHT: f32 = 250.0;

#[derive(Clone, Copy)]
struct TopologyTransform {
    x: f32,
    y: f32,
    k: f32,
}

impl Default for TopologyTransform {
    fn default() -> Self {
        Self {
            x: TOPOLOGY_PAN_INITIAL_X,
            y: TOPOLOGY_PAN_INITIAL_Y,
            k: TOPOLOGY_ZOOM_INITIAL,
        }
    }
}

#[derive(Clone, Copy)]
struct TopologyDragState {
    last_x: f32,
    last_y: f32,
}

#[derive(Clone)]
struct TopologyNodeMenuState {
    node_id: Option<NodeId>,
    name: String,
    host: String,
    view_status: TopologyViewStatus,
    x: f32,
    y: f32,
}

pub(super) struct ConnectionMonitorState {
    pub(super) pool_stats: Option<ConnectionPoolMonitorStats>,
    pub(super) topology_snapshot: Option<ConnectionTopologySnapshot>,
    pub(super) pool_error: Option<String>,
    pub(super) last_pool_refresh: Option<Instant>,
    pub(super) selected_connection_id: Option<String>,
    pub(super) selector_open: bool,
    pub(super) disabled_profiler_connections: HashSet<String>,
    pub(super) profiler_registry: ProfilerRegistry,
    pub(super) profiler_update_tx: tokio::sync::mpsc::UnboundedSender<ProfilerUpdate>,
    pub(super) profiler_update_rx: tokio::sync::mpsc::UnboundedReceiver<ProfilerUpdate>,
    topology_transform: TopologyTransform,
    topology_drag: Option<TopologyDragState>,
    topology_menu: Option<TopologyNodeMenuState>,
}

impl ConnectionMonitorState {
    pub(super) fn new(
        profiler_update_tx: tokio::sync::mpsc::UnboundedSender<ProfilerUpdate>,
        profiler_update_rx: tokio::sync::mpsc::UnboundedReceiver<ProfilerUpdate>,
    ) -> Self {
        Self {
            pool_stats: None,
            topology_snapshot: None,
            pool_error: None,
            last_pool_refresh: None,
            selected_connection_id: None,
            selector_open: false,
            disabled_profiler_connections: HashSet::new(),
            profiler_registry: ProfilerRegistry::new(),
            profiler_update_tx,
            profiler_update_rx,
            topology_transform: TopologyTransform::default(),
            topology_drag: None,
            topology_menu: None,
        }
    }
}

impl WorkspaceApp {
    pub(super) fn open_connection_monitor_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.kind == TabKind::ConnectionMonitor)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::ConnectionMonitor,
                title: self.i18n.t("sidebar.panels.connection_monitor"),
                title_source: TabTitleSource::I18nKey("sidebar.panels.connection_monitor"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.set_active_tab(tab_id, window, cx);
        self.active_sidebar_section = SidebarSection::Activity;
        self.refresh_connection_monitor_pool_stats();
        self.sync_connection_monitor_selection(cx);
    }

    pub(super) fn open_connection_pool_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.kind == TabKind::ConnectionPool)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::ConnectionPool,
                title: self.i18n.t("sidebar.panels.connection_pool"),
                title_source: TabTitleSource::I18nKey("sidebar.panels.connection_pool"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.active_sidebar_section = SidebarSection::Terminal;
        self.set_active_tab(tab_id, window, cx);
        self.refresh_connection_monitor_pool_stats();
    }

    pub(super) fn open_topology_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self.tabs.iter().find(|tab| tab.kind == TabKind::Topology) {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Topology,
                title: self.i18n.t("sidebar.panels.connection_matrix"),
                title_source: TabTitleSource::I18nKey("sidebar.panels.connection_matrix"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.active_sidebar_section = SidebarSection::Network;
        self.set_active_tab(tab_id, window, cx);
        self.refresh_connection_monitor_pool_stats();
    }

    pub(super) fn poll_connection_monitor_updates(&mut self, cx: &mut Context<Self>) {
        while self
            .connection_monitor
            .profiler_update_rx
            .try_recv()
            .is_ok()
        {
            cx.notify();
        }
    }

    pub(super) fn maybe_refresh_connection_monitor(&mut self, cx: &mut Context<Self>) {
        if !self.active_tab().is_some_and(|tab| {
            matches!(
                tab.kind,
                TabKind::ConnectionPool | TabKind::ConnectionMonitor | TabKind::Topology
            )
        }) {
            return;
        }

        let stale = self
            .connection_monitor
            .last_pool_refresh
            .is_none_or(|last| last.elapsed() >= MONITOR_POOL_REFRESH_INTERVAL);
        if stale {
            self.refresh_connection_monitor_pool_stats();
        }
        self.sync_connection_monitor_selection(cx);
    }

    fn refresh_connection_monitor_pool_stats(&mut self) {
        self.connection_monitor.pool_stats = Some(self.ssh_registry.monitor_stats());
        self.connection_monitor.topology_snapshot =
            Some(self.ssh_registry.connection_topology_snapshot());
        self.connection_monitor.pool_error = None;
        self.connection_monitor.last_pool_refresh = Some(Instant::now());
    }

    fn sync_connection_monitor_selection(&mut self, cx: &mut Context<Self>) {
        let connections = self.monitor_connections();
        let live_connection_ids = connections
            .iter()
            .map(|connection| connection.connection_id.as_str())
            .collect::<HashSet<_>>();
        for connection_id in self.connection_monitor.profiler_registry.connection_ids() {
            if !live_connection_ids.contains(connection_id.as_str()) {
                self.connection_monitor
                    .profiler_registry
                    .remove(&connection_id);
                self.connection_monitor
                    .disabled_profiler_connections
                    .remove(&connection_id);
            }
        }
        if connections.is_empty() {
            if let Some(connection_id) = self.connection_monitor.selected_connection_id.take() {
                self.connection_monitor
                    .profiler_registry
                    .remove(&connection_id);
            }
            self.connection_monitor.selector_open = false;
            return;
        }

        let selected_missing = self
            .connection_monitor
            .selected_connection_id
            .as_ref()
            .is_none_or(|selected| {
                !connections
                    .iter()
                    .any(|connection| connection.connection_id == *selected)
            });
        if selected_missing {
            self.connection_monitor.selected_connection_id =
                Some(connections[0].connection_id.clone());
        }

        let Some(connection_id) = self.connection_monitor.selected_connection_id.clone() else {
            return;
        };
        if self
            .connection_monitor
            .disabled_profiler_connections
            .contains(&connection_id)
        {
            return;
        }
        if self
            .connection_monitor
            .profiler_registry
            .state(&connection_id)
            .is_none()
        {
            self.start_connection_monitor_profiler(connection_id, cx);
        }
    }

    fn start_connection_monitor_profiler(&mut self, connection_id: String, cx: &mut Context<Self>) {
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            return;
        };
        self.connection_monitor
            .disabled_profiler_connections
            .remove(&connection_id);
        let sampler: Arc<dyn ResourceSampler> = Arc::new(handle);
        self.connection_monitor
            .profiler_registry
            .start_with_sampler_on(
                connection_id,
                sampler,
                "Linux",
                Some(self.connection_monitor.profiler_update_tx.clone()),
                self.forwarding_runtime.handle().clone(),
            );
        cx.notify();
    }

    fn stop_connection_monitor_profiler(&mut self, connection_id: String, cx: &mut Context<Self>) {
        self.connection_monitor
            .profiler_registry
            .stop(&connection_id);
        self.connection_monitor
            .disabled_profiler_connections
            .insert(connection_id);
        cx.notify();
    }

    fn monitor_connections(&self) -> Vec<oxideterm_ssh::ConnectionInfo> {
        let mut connections = self.ssh_registry.list();
        connections.sort_by(|left, right| {
            monitor_connection_label(left).cmp(&monitor_connection_label(right))
        });
        connections
    }

    pub(super) fn render_connection_monitor_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size_full()
            .overflow_y_scrollbar()
            .p(px(MONITOR_PAGE_PADDING))
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .max_w(px(MONITOR_CONTENT_MAX_WIDTH))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .gap(px(MONITOR_SECTION_GAP))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .mb_6()
                                    .text_size(px(24.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("layout.connection_monitor.title")),
                            )
                            .child(self.render_connection_pool_monitor(cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .mb_4()
                                    .text_size(px(20.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("sidebar.panels.system_health")),
                            )
                            .child(self.render_system_health_panel(cx)),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_connection_pool_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size_full()
            .overflow_y_scrollbar()
            .p(px(MONITOR_PAGE_PADDING))
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .max_w(px(MONITOR_CONTENT_MAX_WIDTH))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .gap(px(MONITOR_SECTION_GAP))
                    .child(
                        div()
                            .mb_6()
                            .text_size(px(24.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("sidebar.panels.connection_pool")),
                    )
                    .child(self.render_connection_pool_monitor(cx)),
            )
            .into_any_element()
    }

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
                            .child(self.i18n.t("topology.page.title")),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("topology.page.description")),
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

    fn render_connection_pool_monitor(&self, _cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        if let Some(error) = &self.connection_monitor.pool_error {
            return monitor_center_state(
                &self.tokens,
                LucideIcon::AlertTriangle,
                MONITOR_RED,
                error.clone(),
            );
        }
        let Some(stats) = self.connection_monitor.pool_stats.as_ref() else {
            return monitor_center_state(
                &self.tokens,
                LucideIcon::RefreshCw,
                theme.text_muted,
                self.i18n.t("connections.monitor.loading"),
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
                            .child(self.i18n.t("connections.monitor.title")),
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
                            .child(self.i18n.t("connections.monitor.live")),
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
                            .child(label),
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
                    .child(value.to_string()),
            )
            .into_any_element()
    }

    fn render_connection_topology(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(snapshot) = self.connection_monitor.topology_snapshot.as_ref() else {
            return monitor_center_state(
                &self.tokens,
                LucideIcon::RefreshCw,
                theme.text_muted,
                self.i18n.t("connections.monitor.loading"),
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
                        .child(self.i18n.t("topology.page.no_connections")),
                )
                .child(
                    div()
                        .mt_2()
                        .text_size(px(14.0))
                        .opacity(0.7)
                        .child(self.i18n.t("topology.page.connect_hint")),
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
                this.zoom_topology_graph(event);
                this.connection_monitor.topology_menu = None;
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    this.connection_monitor.topology_menu = None;
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
                    .child(format!("{}%", (transform.k * 100.0).round() as i32)),
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
                    .child(self.i18n.t("topology.controls.instructions")),
            );

        for node in layout.nodes {
            graph = graph.child(self.render_topology_graph_node(node, transform, cx));
        }

        if let Some(menu) = self.connection_monitor.topology_menu.clone() {
            graph = graph.child(self.render_topology_node_action_menu(menu, cx));
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

        let mut actions = div().py(px(4.0)).child(self.render_topology_menu_action(
            LucideIcon::ExternalLink,
            theme.accent,
            self.i18n.t("topology.menu.navigate_session"),
            cx.listener({
                let node_id = node_id.clone();
                move |this, _event, _window, cx| {
                    if let Some(node_id) = node_id.clone() {
                        this.active_ssh_node_id = Some(node_id);
                        this.active_sidebar_section = SidebarSection::Sessions;
                    }
                    this.connection_monitor.topology_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        ));

        if is_connected {
            actions = actions
                .child(self.render_topology_menu_action(
                    LucideIcon::Terminal,
                    MONITOR_EMERALD_DARK,
                    self.i18n.t("topology.menu.new_terminal"),
                    cx.listener({
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
                            this.connection_monitor.topology_menu = None;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }),
                ))
                .child(self.render_topology_menu_action(
                    LucideIcon::FolderOpen,
                    0xeab308,
                    self.i18n.t("topology.menu.open_sftp"),
                    cx.listener({
                        let node_id = node_id.clone();
                        move |this, _event, window, cx| {
                            if let Some(node_id) = node_id.clone() {
                                this.open_sftp_tab(node_id, window, cx);
                            }
                            this.connection_monitor.topology_menu = None;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }),
                ));
        }

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
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation()
            })
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | TOPOLOGY_PANEL_BORDER_ALPHA_50))
                    .bg(rgba((theme.bg << 8) | 0x80))
                    .child(
                        div()
                            .max_w(px(TOPOLOGY_MENU_WIDTH - 24.0))
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text))
                            .child(menu.name),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text_muted))
                            .child(menu.host),
                    ),
            )
            .child(actions)
            .child(
                div()
                    .px_3()
                    .py(px(6.0))
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | TOPOLOGY_PANEL_BORDER_ALPHA_50))
                    .bg(rgba((theme.bg << 8) | 0x4d))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(10.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("topology.menu.close_hint")),
            )
            .into_any_element()
    }

    fn render_topology_menu_action(
        &self,
        icon: LucideIcon,
        icon_color: u32,
        label: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .px_3()
            .py_2()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(14.0))
            .text_color(rgb(theme.text_muted))
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(rgba((theme.accent << 8) | 0x1a))
                    .text_color(rgb(theme.text))
            })
            .on_mouse_down(MouseButton::Left, listener)
            .child(Self::render_lucide_icon(icon, 16.0, rgb(icon_color)))
            .child(label)
            .into_any_element()
    }

    fn zoom_topology_graph(&mut self, event: &ScrollWheelEvent) {
        let delta = event.delta.pixel_delta(px(16.0));
        let vertical = f32::from(delta.y);
        if vertical == 0.0 {
            return;
        }

        let old = self.connection_monitor.topology_transform;
        let wheel_factor = (1.0 - vertical * 0.001).clamp(0.85, 1.15);
        let next_k = (old.k * wheel_factor).clamp(TOPOLOGY_ZOOM_MIN, TOPOLOGY_ZOOM_MAX);
        if (next_k - old.k).abs() < f32::EPSILON {
            return;
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

    fn render_system_health_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .py_8()
                .px_4()
                .text_align(gpui::TextAlign::Center)
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(div().mb_2().opacity(0.3).child(Self::render_lucide_icon(
                    LucideIcon::WifiOff,
                    32.0,
                    rgb(self.tokens.ui.text_muted),
                )))
                .child(
                    div()
                        .text_size(px(14.0))
                        .child(self.i18n.t("profiler.panel.no_connection")),
                )
                .into_any_element();
        }

        let selected_id = self
            .connection_monitor
            .selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let active_connection = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .unwrap_or(&connections[0]);
        let snapshot = self
            .connection_monitor
            .profiler_registry
            .snapshot(&active_connection.connection_id);
        let disabled = self
            .connection_monitor
            .disabled_profiler_connections
            .contains(&active_connection.connection_id);
        let is_running = snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.state == ProfilerState::Running);
        let metrics = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.metrics.as_ref());
        let history = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .history
                    .iter()
                    .rev()
                    .take(MONITOR_SPARKLINE_POINTS)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut panel = div()
            .relative()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.render_connection_selector(&connections, selected_id, cx))
            .child(self.render_monitor_panel_header(active_connection, is_running, !disabled, cx));

        if disabled || (!is_running && metrics.is_none()) {
            return panel
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .py_8()
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(div().mb_3().opacity(0.2).child(Self::render_lucide_icon(
                            LucideIcon::Power,
                            32.0,
                            rgb(self.tokens.ui.text_muted),
                        )))
                        .child(
                            div()
                                .mb_3()
                                .text_size(px(14.0))
                                .child(self.i18n.t("profiler.panel.disabled")),
                        )
                        .child(
                            div()
                                .px_3()
                                .py_1()
                                .rounded(px(self.tokens.radii.md))
                                .border_1()
                                .border_color(rgba(
                                    (self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA,
                                ))
                                .text_size(px(12.0))
                                .cursor_pointer()
                                .hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
                                .child(self.i18n.t("profiler.panel.enable"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener({
                                        let connection_id = active_connection.connection_id.clone();
                                        move |this, _event, _window, cx| {
                                            this.start_connection_monitor_profiler(
                                                connection_id.clone(),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }
                                    }),
                                ),
                        ),
                )
                .into_any_element();
        }

        if metrics.is_none() && is_running {
            return panel
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .py_6()
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(div().mb_2().opacity(0.5).child(Self::render_lucide_icon(
                            LucideIcon::Activity,
                            20.0,
                            rgb(self.tokens.ui.text_muted),
                        )))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .child(self.i18n.t("profiler.panel.sampling")),
                        ),
                )
                .into_any_element();
        }

        let Some(metrics) = metrics else {
            return panel
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .py_6()
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(
                            div()
                                .opacity(0.6)
                                .text_size(px(12.0))
                                .child(self.i18n.t("profiler.panel.no_data")),
                        ),
                )
                .into_any_element();
        };

        let is_rtt_only = matches!(
            metrics.source,
            MetricsSource::RttOnly | MetricsSource::Failed
        );
        if !is_rtt_only && let Some(cpu) = metrics.cpu_percent {
            panel = panel.child(self.render_metric_card(
                self.i18n.t("profiler.panel.cpu"),
                format!("{cpu:.1}%"),
                LucideIcon::Cpu,
                threshold_color(Some(cpu)),
                Some(cpu as f32),
                history.iter().map(|metric| metric.cpu_percent).collect(),
            ));
        }
        if !is_rtt_only && metrics.memory_used.is_some() && metrics.memory_total.is_some() {
            panel = panel.child(self.render_metric_card(
                self.i18n.t("profiler.panel.memory"),
                format!(
                    "{} / {}",
                    format_bytes(metrics.memory_used.unwrap_or_default()),
                    format_bytes(metrics.memory_total.unwrap_or_default())
                ),
                LucideIcon::MemoryStick,
                threshold_color(metrics.memory_percent),
                metrics.memory_percent.map(|value| value as f32),
                history.iter().map(|metric| metric.memory_percent).collect(),
            ));
        }
        if !is_rtt_only
            && (metrics.net_rx_bytes_per_sec.is_some() || metrics.net_tx_bytes_per_sec.is_some())
        {
            panel = panel.child(self.render_network_metric_card(metrics));
        }

        panel
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap_2()
                    .when(!is_rtt_only && metrics.load_avg_1.is_some(), |row| {
                        row.child(self.render_compact_metric_box(
                            LucideIcon::Gauge,
                            self.i18n.t("profiler.panel.load_avg"),
                            format!(
                                "{:.2} / {:.2} / {:.2}",
                                metrics.load_avg_1.unwrap_or_default(),
                                metrics.load_avg_5.unwrap_or_default(),
                                metrics.load_avg_15.unwrap_or_default()
                            ),
                            rgb(self.tokens.ui.text),
                        ))
                    })
                    .child(
                        self.render_compact_metric_box(
                            LucideIcon::Activity,
                            self.i18n.t("profiler.panel.rtt"),
                            metrics
                                .ssh_rtt_ms
                                .map(|rtt| format!("{rtt} ms"))
                                .unwrap_or_else(|| "—".to_string()),
                            rgb(rtt_color(metrics.ssh_rtt_ms)),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_1()
                    .pt_1()
                    .text_size(px(10.0))
                    .text_color(rgba(
                        (self.tokens.ui.text_muted << 8) | MONITOR_SOURCE_ALPHA,
                    ))
                    .child(self.i18n.t("profiler.panel.source"))
                    .child(
                        div()
                            .font_family("monospace")
                            .child(metrics_source_label(metrics.source)),
                    ),
            )
            .into_any_element()
    }

    fn render_connection_selector(
        &self,
        connections: &[oxideterm_ssh::ConnectionInfo],
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .map(monitor_connection_label)
            .unwrap_or_default();
        let mut wrapper = div().relative().mb_4().child(
            select_trigger(&self.tokens, selected_label, false, false)
                .font_family("monospace")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.connection_monitor.selector_open =
                            !this.connection_monitor.selector_open;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
        );
        if self.connection_monitor.selector_open {
            let mut popup = div()
                .absolute()
                .top(px(38.0))
                .left_0()
                .right_0()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(self.tokens.ui.border))
                .bg(rgb(self.tokens.ui.bg_panel))
                .p_1()
                .shadow_lg();
            for connection in connections {
                let connection_id = connection.connection_id.clone();
                let selected = connection.connection_id == selected_id;
                popup = popup.child(
                    select_option(&self.tokens, monitor_connection_label(connection), selected)
                        .font_family("monospace")
                        .child(div().mr_2().child(Self::render_lucide_icon(
                            LucideIcon::Server,
                            14.0,
                            rgb(self.tokens.ui.text_muted),
                        )))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.connection_monitor.selected_connection_id =
                                    Some(connection_id.clone());
                                this.connection_monitor.selector_open = false;
                                this.sync_connection_monitor_selection(cx);
                                cx.stop_propagation();
                            }),
                        ),
                );
            }
            wrapper = wrapper.child(popup);
        }
        wrapper.into_any_element()
    }

    fn render_monitor_panel_header(
        &self,
        connection: &oxideterm_ssh::ConnectionInfo,
        is_running: bool,
        is_enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .p_3()
            .child(Self::render_lucide_icon(
                LucideIcon::Server,
                16.0,
                if is_running {
                    rgb(MONITOR_EMERALD)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(format!("{}@{}", connection.username, connection.host)),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(12.0))
                            .text_color(rgb(theme.text_muted))
                            .child(format!(":{}", connection.port)),
                    ),
            )
            .child(
                div()
                    .p_1()
                    .rounded(px(self.tokens.radii.md))
                    .cursor_pointer()
                    .text_color(if is_enabled {
                        rgb(MONITOR_EMERALD)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .hover(|button| {
                        if is_enabled {
                            button
                                .text_color(rgb(MONITOR_RED))
                                .bg(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA))
                        } else {
                            button
                                .text_color(rgb(MONITOR_EMERALD))
                                .bg(rgba((MONITOR_EMERALD_DARK << 8) | MONITOR_TINT_ALPHA))
                        }
                    })
                    .child(Self::render_lucide_icon(
                        LucideIcon::Power,
                        14.0,
                        if is_enabled {
                            rgb(MONITOR_EMERALD)
                        } else {
                            rgb(theme.text_muted)
                        },
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let connection_id = connection.connection_id.clone();
                            move |this, _event, _window, cx| {
                                if this
                                    .connection_monitor
                                    .disabled_profiler_connections
                                    .contains(&connection_id)
                                    || this
                                        .connection_monitor
                                        .profiler_registry
                                        .state(&connection_id)
                                        .is_none()
                                {
                                    this.start_connection_monitor_profiler(
                                        connection_id.clone(),
                                        cx,
                                    );
                                } else {
                                    this.stop_connection_monitor_profiler(
                                        connection_id.clone(),
                                        cx,
                                    );
                                }
                                cx.stop_propagation();
                            }
                        }),
                    ),
            )
            .child(
                div()
                    .w_2()
                    .h_2()
                    .rounded_full()
                    .bg(rgb(if is_running {
                        MONITOR_EMERALD_DARK
                    } else {
                        theme.text_muted
                    }))
                    .opacity(if is_running { 1.0 } else { 0.5 }),
            )
            .into_any_element()
    }

    fn render_metric_card(
        &self,
        label: String,
        value: String,
        icon: LucideIcon,
        color: u32,
        progress_value: Option<f32>,
        history: Vec<Option<f64>>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_size(px(12.0))
                            .text_color(rgb(theme.text_muted))
                            .child(Self::render_lucide_icon(icon, 14.0, rgb(theme.text_muted)))
                            .child(label),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(12.0))
                            .text_color(rgb(color))
                            .child(value),
                    ),
            )
            .child(progress(&self.tokens, progress_value, false).h(px(6.0)))
            .when(
                history.iter().filter_map(|value| *value).count() >= 2,
                |card| card.child(render_sparkline(history, color)),
            )
            .into_any_element()
    }

    fn render_network_metric_card(&self, metrics: &ResourceMetrics) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .p_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .mb_2()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Wifi,
                        14.0,
                        rgb(theme.text_muted),
                    ))
                    .child(self.i18n.t("profiler.panel.network")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .font_family("monospace")
                    .text_size(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(Self::render_lucide_icon(
                                LucideIcon::ArrowDown,
                                12.0,
                                rgb(MONITOR_EMERALD),
                            ))
                            .child(format_rate(
                                metrics.net_rx_bytes_per_sec.unwrap_or_default(),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(Self::render_lucide_icon(
                                LucideIcon::ArrowUp,
                                12.0,
                                rgb(MONITOR_AMBER),
                            ))
                            .child(format_rate(
                                metrics.net_tx_bytes_per_sec.unwrap_or_default(),
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_compact_metric_box(
        &self,
        icon: LucideIcon,
        label: String,
        value: String,
        value_color: Rgba,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .p_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .mb_1()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(icon, 14.0, rgb(theme.text_muted)))
                    .child(label),
            )
            .child(
                div()
                    .font_family("monospace")
                    .text_size(px(12.0))
                    .text_color(value_color)
                    .child(value),
            )
            .into_any_element()
    }
}

fn monitor_center_state(
    _tokens: &ThemeTokens,
    icon: LucideIcon,
    color: u32,
    label: String,
) -> AnyElement {
    div()
        .p_4()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .text_align(gpui::TextAlign::Center)
        .text_color(rgb(color))
        .child(
            div()
                .mb_2()
                .child(WorkspaceApp::render_lucide_icon(icon, 20.0, rgb(color))),
        )
        .child(div().text_size(px(14.0)).child(label))
        .into_any_element()
}

fn monitor_connection_label(connection: &oxideterm_ssh::ConnectionInfo) -> String {
    format!(
        "{}@{}:{}",
        connection.username, connection.host, connection.port
    )
}

fn topology_transform_x(x: f32, transform: TopologyTransform) -> f32 {
    transform.x + x * transform.k
}

fn topology_transform_y(y: f32, transform: TopologyTransform) -> f32 {
    transform.y + y * transform.k
}

fn topology_view_status_color(status: TopologyViewStatus) -> u32 {
    match status {
        TopologyViewStatus::Connected => TOPOLOGY_CONNECTED,
        TopologyViewStatus::Connecting => TOPOLOGY_CONNECTING,
        TopologyViewStatus::Failed => TOPOLOGY_FAILED,
        TopologyViewStatus::Disconnected => TOPOLOGY_DISCONNECTED,
        TopologyViewStatus::Pending => TOPOLOGY_PENDING,
    }
}

fn threshold_color(value: Option<f64>) -> u32 {
    match value {
        None => 0x94a3b8,
        Some(value) if value < 70.0 => MONITOR_EMERALD,
        Some(value) if value < 90.0 => MONITOR_AMBER,
        Some(_) => MONITOR_RED,
    }
}

fn rtt_color(value: Option<u64>) -> u32 {
    match value {
        None => 0x94a3b8,
        Some(value) if value < 100 => MONITOR_EMERALD,
        Some(value) if value < 300 => MONITOR_AMBER,
        Some(_) => MONITOR_RED,
    }
}

fn metrics_source_label(source: MetricsSource) -> &'static str {
    match source {
        MetricsSource::Full => "full",
        MetricsSource::Partial => "partial",
        MetricsSource::RttOnly => "rtt_only",
        MetricsSource::Failed => "failed",
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_rate(bytes: u64) -> String {
    format!("{}/s", format_bytes(bytes))
}

fn render_sparkline(values: Vec<Option<f64>>, color: u32) -> AnyElement {
    if values.iter().filter_map(|value| *value).count() < 2 {
        return div().into_any_element();
    }

    div()
        .h(px(MONITOR_SPARKLINE_HEIGHT))
        .w_full()
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let points = sparkline_polyline_points(
                        &values,
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                    );
                    if points.len() < 2 {
                        return;
                    }

                    let mut builder = PathBuilder::stroke(px(MONITOR_SPARKLINE_STROKE_WIDTH));
                    for (index, (x, y)) in points.into_iter().enumerate() {
                        let point = point(bounds.origin.x + px(x), bounds.origin.y + px(y));
                        if index == 0 {
                            builder.move_to(point);
                        } else {
                            builder.line_to(point);
                        }
                    }
                    if let Ok(path) = builder.build() {
                        window
                            .paint_path(path, rgba((color << 8) | MONITOR_SPARKLINE_STROKE_ALPHA));
                    }
                },
            )
            .size_full(),
        )
        .into_any_element()
}

fn sparkline_polyline_points(values: &[Option<f64>], width: f32, height: f32) -> Vec<(f32, f32)> {
    let valid = values.iter().filter_map(|value| *value).collect::<Vec<_>>();
    if valid.len() < 2 {
        return Vec::new();
    }

    let width = width.max(1.0);
    let height = height.max(1.0);
    let max = valid.iter().copied().fold(1.0_f64, f64::max);
    let step = width / (valid.len().saturating_sub(1) as f32);
    valid
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let x = index as f32 * step;
            let y = height - ((value / max) as f32 * height * 0.85) - height * 0.05;
            (x, y)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_points_match_tauri_polyline_mapping() {
        let points =
            sparkline_polyline_points(&[Some(10.0), None, Some(20.0), Some(5.0)], 100.0, 28.0);

        assert_eq!(points.len(), 3);
        assert_point_close(points[0], (0.0, 14.7));
        assert_point_close(points[1], (50.0, 2.8));
        assert_point_close(points[2], (100.0, 20.65));
    }

    fn assert_point_close(actual: (f32, f32), expected: (f32, f32)) {
        assert!((actual.0 - expected.0).abs() < 0.001);
        assert!((actual.1 - expected.1).abs() < 0.001);
    }
}
