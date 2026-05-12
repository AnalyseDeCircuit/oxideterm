use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{MouseButton, PathBuilder, canvas, point};
use gpui_component::scroll::ScrollableElement;
use oxideterm_connection_monitor::{ProfilerState, ResourceSampler};
use oxideterm_gpui_ui::progress::progress;
use oxideterm_gpui_ui::select::{select_option, select_trigger};

use super::*;

const MONITOR_POOL_REFRESH_INTERVAL: Duration = Duration::from_millis(2000);
const MONITOR_SPARKLINE_POINTS: usize = 12;
const MONITOR_CONTENT_MAX_WIDTH: f32 = 1024.0;
const MONITOR_PAGE_PADDING: f32 = 32.0;
const MONITOR_SECTION_GAP: f32 = 32.0;
const MONITOR_CARD_RADIUS: f32 = 6.0;
const MONITOR_POOL_CARD_RADIUS: f32 = 8.0;
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

pub(super) struct ConnectionMonitorState {
    pub(super) pool_stats: Option<ConnectionPoolMonitorStats>,
    pub(super) pool_error: Option<String>,
    pub(super) last_pool_refresh: Option<Instant>,
    pub(super) selected_connection_id: Option<String>,
    pub(super) selector_open: bool,
    pub(super) disabled_profiler_connections: HashSet<String>,
    pub(super) profiler_registry: ProfilerRegistry,
    pub(super) profiler_update_tx: tokio::sync::mpsc::UnboundedSender<ProfilerUpdate>,
    pub(super) profiler_update_rx: tokio::sync::mpsc::UnboundedReceiver<ProfilerUpdate>,
}

impl ConnectionMonitorState {
    pub(super) fn new(
        profiler_update_tx: tokio::sync::mpsc::UnboundedSender<ProfilerUpdate>,
        profiler_update_rx: tokio::sync::mpsc::UnboundedReceiver<ProfilerUpdate>,
    ) -> Self {
        Self {
            pool_stats: None,
            pool_error: None,
            last_pool_refresh: None,
            selected_connection_id: None,
            selector_open: false,
            disabled_profiler_connections: HashSet::new(),
            profiler_registry: ProfilerRegistry::new(),
            profiler_update_tx,
            profiler_update_rx,
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
        self.refresh_connection_monitor_pool_stats();
        self.sync_connection_monitor_selection(cx);
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
        if !self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::ConnectionMonitor)
        {
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
            .rounded(px(MONITOR_POOL_CARD_RADIUS))
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
                                .rounded(px(MONITOR_CARD_RADIUS))
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
            .rounded(px(MONITOR_CARD_RADIUS))
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
                    .rounded(px(MONITOR_CARD_RADIUS))
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
            .rounded(px(MONITOR_CARD_RADIUS))
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
            .rounded(px(MONITOR_CARD_RADIUS))
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
            .rounded(px(MONITOR_CARD_RADIUS))
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
