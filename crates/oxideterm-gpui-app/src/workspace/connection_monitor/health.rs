const SYSTEM_HEALTH_SELECTOR_TRIGGER_HEIGHT: f32 = 38.0;
const SYSTEM_HEALTH_SELECTOR_OPTION_HEIGHT: f32 = 36.0;
const SYSTEM_HEALTH_SELECTOR_MENU_PADDING_Y: f32 = 8.0;
const SYSTEM_HEALTH_SELECTOR_VISIBLE_OPTIONS: usize = 4;
const SYSTEM_HEALTH_SELECTOR_GAP: f32 = 8.0;
const HOST_TOOLS_TAB_STRIP_HEIGHT: f32 = 44.0;

impl WorkspaceApp {
    pub(super) fn render_host_tools_context_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .id("host-tools-context-panel")
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_hidden()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(self.render_host_tools_context_tabs(cx))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    // Only the secondary tab strip may own horizontal scroll.
                    // Keep tool bodies clipped to the companion-sidebar width.
                    .overflow_hidden()
                    .child(match self.active_context_sidebar_tool {
                        ContextSidebarTool::Monitor => self.render_host_tools_monitor_panel(cx),
                        ContextSidebarTool::Processes => self.render_host_tool_placeholder(
                            "sidebar.panels.processes",
                            LucideIcon::ListChecks,
                            cx,
                        ),
                        ContextSidebarTool::Services => self.render_host_tool_placeholder(
                            "sidebar.panels.services",
                            LucideIcon::Wrench,
                            cx,
                        ),
                        ContextSidebarTool::Logs => self.render_host_tool_placeholder(
                            "sidebar.panels.logs",
                            LucideIcon::FileText,
                            cx,
                        ),
                        ContextSidebarTool::Tmux => self.render_host_tool_placeholder(
                            "sidebar.panels.tmux_management",
                            LucideIcon::Terminal,
                            cx,
                        ),
                        ContextSidebarTool::Docker => self.render_host_tool_placeholder(
                            "sidebar.panels.docker_management",
                            LucideIcon::Layers,
                            cx,
                        ),
                    }),
            )
            .into_any_element()
    }

    fn render_host_tools_monitor_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("system-health-context-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .occlude()
            .child(
                div()
                    .size_full()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .px_3()
                    .py_3()
                    // Host Tools owns the secondary navigation; monitoring
                    // keeps the existing health panel behavior inside it.
                    .child(self.render_system_health_panel(true, cx)),
            )
            .into_any_element()
    }

    fn render_host_tools_context_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut tabs = div()
            .id("host-tools-tab-scroll-viewport")
            .flex_none()
            .w_full()
            .h(px(HOST_TOOLS_TAB_STRIP_HEIGHT))
            .min_w_0()
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_3()
            .py_2()
            // Match the main tabbar scroll model: the strip clips its own
            // children and maps wheel movement to horizontal offset, while the
            // Host Tools body remains width-clipped below it.
            .occlude()
            .overflow_hidden()
            .track_scroll(&self.host_tools_tab_scroll_handle)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                this.handle_host_tools_tab_scroll(event, window, cx);
            }))
            .border_b_1()
            .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA));

        tabs = tabs
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Monitor,
                LucideIcon::Activity,
                "sidebar.panels.host_monitor",
                true,
                cx,
            ))
            // These entries reserve the host-tools IA before their backends land.
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Processes,
                LucideIcon::ListChecks,
                "sidebar.panels.processes",
                false,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Services,
                LucideIcon::Wrench,
                "sidebar.panels.services",
                false,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Logs,
                LucideIcon::FileText,
                "sidebar.panels.logs",
                false,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Tmux,
                LucideIcon::Terminal,
                "sidebar.panels.tmux",
                false,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Docker,
                LucideIcon::Layers,
                "sidebar.panels.docker",
                false,
                cx,
            ));

        tabs.into_any_element()
    }

    fn handle_host_tools_tab_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let max_scroll = f32::from(self.host_tools_tab_scroll_handle.max_offset().width);
        if max_scroll <= 1.0 {
            if self.host_tools_tab_scroll_handle.offset().x != px(0.0) {
                self.host_tools_tab_scroll_handle
                    .set_offset(Point::new(px(0.0), px(0.0)));
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }

        let delta = event.delta.pixel_delta(px(HOST_TOOLS_TAB_STRIP_HEIGHT));
        let delta_x = f32::from(delta.x);
        let delta_y = f32::from(delta.y);
        let scroll_delta = if delta_y != 0.0 { delta_y } else { delta_x };
        if scroll_delta == 0.0 {
            return;
        }

        let current_scroll_x =
            f32::from(-self.host_tools_tab_scroll_handle.offset().x).clamp(0.0, max_scroll);
        let next_scroll_x = (current_scroll_x - scroll_delta).clamp(0.0, max_scroll);
        if (next_scroll_x - current_scroll_x).abs() < 0.01 {
            cx.stop_propagation();
            return;
        }

        self.host_tools_tab_scroll_handle
            .set_offset(Point::new(px(-next_scroll_x), px(0.0)));
        cx.notify();
        cx.stop_propagation();
    }

    fn render_host_tools_context_tab(
        &self,
        tool: ContextSidebarTool,
        icon: LucideIcon,
        label_key: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_context_sidebar_tool == tool;
        div()
            .h(px(28.0))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .rounded(px(self.tokens.radii.md))
            .cursor(if enabled {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .opacity(if enabled { 1.0 } else { 0.45 })
            .bg(if active {
                rgb(theme.bg_hover)
            } else {
                rgba(0x00000000)
            })
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |tab| {
                if enabled {
                    tab.bg(rgb(theme.bg_hover))
                } else {
                    tab
                }
            })
            .child(Self::render_lucide_icon(
                icon,
                13.0,
                if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(
                div()
                    .text_size(px(12.0))
                    .whitespace_nowrap()
                    .truncate()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "host-tools-tab",
                        label_key,
                        self.i18n.t(label_key),
                        if active { theme.text } else { theme.text_muted },
                        cx,
                    )),
            )
            .when(enabled, |tab| {
                tab.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if this.active_context_sidebar_tool != tool {
                            this.active_context_sidebar_tool = tool;
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                )
            })
            .into_any_element()
    }

    fn render_host_tool_placeholder(
        &self,
        label_key: &'static str,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        monitor_center_state(
            self,
            icon,
            self.tokens.ui.text_muted,
            self.i18n.t(label_key),
            cx,
        )
    }

    fn render_system_health_panel(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
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
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "system-health-empty",
                            "no-connection",
                            self.i18n.t("profiler.panel.no_connection"),
                            self.tokens.ui.text_muted,
                            cx,
                        )),
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
        let snapshot = (!compact).then(|| {
            self.connection_monitor
                .profiler_registry
                .snapshot(&active_connection.connection_id)
        }).flatten();
        let current = compact.then(|| {
            self.connection_monitor
                .profiler_registry
                .current(&active_connection.connection_id)
        }).flatten();
        let disabled = self
            .connection_monitor
            .disabled_profiler_connections
            .contains(&active_connection.connection_id);
        let profiler_state = if compact {
            current.as_ref().map(|(_, state)| *state)
        } else {
            snapshot.as_ref().map(|snapshot| snapshot.state)
        };
        let is_running = matches!(profiler_state, Some(ProfilerState::Running));
        let metrics = if compact {
            current.as_ref().and_then(|(metrics, _)| metrics.as_ref())
        } else {
            snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.metrics.as_ref())
        };
        let show_history = !compact;
        let history = if show_history {
            snapshot
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
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let panel = div()
            .relative()
            .flex()
            .flex_col()
            .gap_2()
            .when(compact, |panel| panel.flex_1().min_h_0())
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
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "system-health-profiler",
                                    "disabled",
                                    self.i18n.t("profiler.panel.disabled"),
                                    self.tokens.ui.text_muted,
                                    cx,
                                )),
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
                                // Profiler enable is a button label; keep it outside selection ownership.
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::NonSelectable,
                                    "system-health-profiler",
                                    "enable",
                                    self.i18n.t("profiler.panel.enable"),
                                    self.tokens.ui.text_muted,
                                    cx,
                                ))
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
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "system-health-profiler",
                                    "sampling",
                                    self.i18n.t("profiler.panel.sampling"),
                                    self.tokens.ui.text_muted,
                                    cx,
                                )),
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
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "system-health-profiler",
                                    "no-data",
                                    self.i18n.t("profiler.panel.no_data"),
                                    self.tokens.ui.text_muted,
                                    cx,
                                )),
                        ),
                )
                .into_any_element();
        };

        let is_rtt_only = matches!(
            metrics.source,
            MetricsSource::RttOnly | MetricsSource::Failed | MetricsSource::Unsupported
        );
        let can_retry_sampling = !disabled
            && (matches!(profiler_state, Some(ProfilerState::Degraded))
                || matches!(metrics.source, MetricsSource::Unsupported));
        if compact {
            return panel
                .child(
                    div()
                        .id("host-tools-monitor-metrics-scroll")
                        .flex_1()
                        .min_h_0()
                        .child(self.render_compact_system_health_metrics(
                            metrics,
                            is_rtt_only,
                            can_retry_sampling,
                            active_connection.connection_id.clone(),
                            cx,
                        )),
                )
                .into_any_element();
        }

        let mut metric_body = div().flex().flex_col().gap_2();
        if !is_rtt_only && let Some(cpu) = metrics.cpu_percent {
            metric_body = metric_body.child(self.render_metric_card(
                self.i18n.t("profiler.panel.cpu"),
                format!("{cpu:.1}%"),
                LucideIcon::Cpu,
                threshold_color(Some(cpu)),
                Some(cpu as f32),
                Self::metric_history(show_history, &history, |metric| metric.cpu_percent),
                !compact,
                cx,
            ));
        }
        if !is_rtt_only && metrics.memory_used.is_some() && metrics.memory_total.is_some() {
            metric_body = metric_body.child(self.render_metric_card(
                self.i18n.t("profiler.panel.memory"),
                format!(
                    "{} / {}",
                    format_bytes(metrics.memory_used.unwrap_or_default()),
                    format_bytes(metrics.memory_total.unwrap_or_default())
                ),
                LucideIcon::MemoryStick,
                threshold_color(metrics.memory_percent),
                metrics.memory_percent.map(|value| value as f32),
                Self::metric_history(show_history, &history, |metric| metric.memory_percent),
                !compact,
                cx,
            ));
        }
        if !is_rtt_only && metrics.swap_used.is_some() && metrics.swap_total.is_some() {
            metric_body = metric_body.child(self.render_metric_card(
                self.i18n.t("profiler.panel.swap"),
                format!(
                    "{} / {}",
                    format_bytes(metrics.swap_used.unwrap_or_default()),
                    format_bytes(metrics.swap_total.unwrap_or_default())
                ),
                LucideIcon::MemoryStick,
                threshold_color(metrics.swap_percent),
                metrics.swap_percent.map(|value| value as f32),
                Self::metric_history(show_history, &history, |metric| metric.swap_percent),
                !compact,
                cx,
            ));
        }
        if !is_rtt_only && metrics.disk_used.is_some() && metrics.disk_total.is_some() {
            metric_body = metric_body.child(self.render_metric_card(
                self.i18n.t("profiler.panel.disk"),
                format!(
                    "{} / {}",
                    format_bytes(metrics.disk_used.unwrap_or_default()),
                    format_bytes(metrics.disk_total.unwrap_or_default())
                ),
                LucideIcon::HardDrive,
                threshold_color(metrics.disk_percent),
                metrics.disk_percent.map(|value| value as f32),
                Self::metric_history(show_history, &history, |metric| metric.disk_percent),
                !compact,
                cx,
            ));
        }
        if !is_rtt_only
            && (metrics.net_rx_bytes_per_sec.is_some() || metrics.net_tx_bytes_per_sec.is_some())
        {
            metric_body = metric_body.child(self.render_network_metric_card(metrics, !compact, cx));
        }
        if !is_rtt_only && !metrics.disks.is_empty() {
            metric_body = metric_body.child(self.render_disk_list_card(metrics, !compact, cx));
        }
        if !is_rtt_only && !metrics.net_interfaces.is_empty() {
            metric_body = metric_body.child(self.render_interface_list_card(metrics, !compact, cx));
        }
        if !is_rtt_only && !metrics.top_processes.is_empty() {
            metric_body =
                metric_body.child(self.render_top_process_list_card(metrics, !compact, cx));
        }

        let metric_body = metric_body
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
                            self.tokens.ui.text,
                            !compact,
                            cx,
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
                            rtt_color(metrics.ssh_rtt_ms),
                            !compact,
                            cx,
                        ),
                    ),
            )
            .when(can_retry_sampling, |panel| {
                panel.child(self.render_retry_sampling_button(
                    active_connection.connection_id.clone(),
                    cx,
                ))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_1()
                    .pt_1()
                    .text_size(px(10.0))
                    .text_color(rgba(
                        (self.tokens.ui.text_muted << 8) | MONITOR_SOURCE_ALPHA,
                    ))
                    .child(
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .child(self.render_monitor_text(
                                !compact,
                                "monitor-metric-source-label",
                                "profiler.panel.source",
                                self.i18n.t("profiler.panel.source"),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .truncate()
                            .font_family("monospace")
                            .child(self.render_monitor_text(
                                !compact,
                                "monitor-metric-source",
                                (),
                                self.i18n.t(metrics_source_label_key(metrics.source)),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    ),
            );

        panel.child(metric_body).into_any_element()
    }

    fn render_connection_selector(
        &self,
        connections: &[MonitorConnectionOption],
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .map(monitor_connection_label)
            .unwrap_or_default();
        let trigger = select_trigger_with_focus_visible(
            &self.tokens,
            selected_label,
            false,
            false,
            // The monitor selector is pointer-opened today, but it should use
            // the same modality gate as other native Select triggers.
            browser_behavior::browser_focus_visible(
                self.connection_monitor.selector_focus_origin.is_some(),
                self.connection_monitor.selector_focus_origin,
            ),
        )
        .font_family("monospace");
        let selected_index = monitor_connection_selected_index(connections, selected_id);
        // The popup is painted inside this narrow panel, so reserve enough
        // layout space while it is open instead of letting health cards overlap it.
        let selector_bottom_margin = if self.connection_monitor.selector_open {
            let visible_options = connections
                .len()
                .max(1)
                .min(SYSTEM_HEALTH_SELECTOR_VISIBLE_OPTIONS) as f32;
            SYSTEM_HEALTH_SELECTOR_TRIGGER_HEIGHT
                + SYSTEM_HEALTH_SELECTOR_MENU_PADDING_Y
                + (visible_options * SYSTEM_HEALTH_SELECTOR_OPTION_HEIGHT)
                + SYSTEM_HEALTH_SELECTOR_GAP
        } else {
            16.0
        };
        let mut wrapper = div().relative().mb(px(selector_bottom_margin)).child(trigger.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Pointer);
                if this.connection_monitor.selector_open {
                    this.connection_monitor.selector_open = false;
                    this.connection_monitor.selector_highlighted_index = None;
                } else {
                    let connections = this.monitor_connections();
                    let selected_id = this
                        .connection_monitor
                        .selected_connection_id
                        .as_deref()
                        .unwrap_or_else(|| {
                            connections
                                .first()
                                .map(|connection| connection.connection_id.as_str())
                                .unwrap_or_default()
                        });
                    this.connection_monitor.selector_highlighted_index =
                        Some(monitor_connection_selected_index(&connections, selected_id));
                    this.connection_monitor.selector_open = true;
                }
                cx.stop_propagation();
                cx.notify();
            }),
        ));
        if self.connection_monitor.selector_open {
            let highlighted = self
                .connection_monitor
                .selector_highlighted_index
                .unwrap_or(selected_index);
            let mut popup = select_event_boundary(
                div()
                    .absolute()
                    .top(px(SYSTEM_HEALTH_SELECTOR_TRIGGER_HEIGHT))
                    .left_0()
                    .right_0()
                    .overflow_hidden()
                    .max_h(px(
                        SYSTEM_HEALTH_SELECTOR_MENU_PADDING_Y
                            + (SYSTEM_HEALTH_SELECTOR_VISIBLE_OPTIONS as f32
                                * SYSTEM_HEALTH_SELECTOR_OPTION_HEIGHT),
                    ))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .p_1()
                    .shadow_lg(),
            );
            for (index, connection) in connections.iter().enumerate() {
                let connection_id = connection.connection_id.clone();
                let selected = connection.connection_id == selected_id;
                let highlighted = highlighted == index;
                popup = popup.child(
                    select_option_action(
                        select_option_highlighted(
                            &self.tokens,
                            monitor_connection_label(connection),
                            selected,
                            highlighted,
                        )
                            .font_family("monospace")
                            .on_mouse_move(cx.listener(move |this, _event, _window, cx| {
                                if this.connection_monitor.selector_highlighted_index
                                    != Some(index)
                                {
                                    this.connection_monitor.selector_highlighted_index =
                                        Some(index);
                                    cx.notify();
                                }
                            }))
                            .child(div().mr_2().child(Self::render_lucide_icon(
                                LucideIcon::Server,
                                14.0,
                                rgb(self.tokens.ui.text_muted),
                            ))),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.connection_monitor.selected_connection_id =
                                Some(connection_id.clone());
                            this.connection_monitor.selector_open = false;
                            this.connection_monitor.selector_highlighted_index = None;
                            this.connection_monitor.selector_focus_origin = None;
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

    pub(super) fn handle_connection_monitor_select_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return false;
        }
        let selected_id = self
            .connection_monitor
            .selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let selected_index = monitor_connection_selected_index(&connections, selected_id);
        let current = self
            .connection_monitor
            .selector_highlighted_index
            .unwrap_or(selected_index);

        if self.connection_monitor.selector_open {
            return self.handle_open_connection_monitor_select_key(event, &connections, current, cx);
        }

        match event.keystroke.key.as_str() {
            "tab" => {
                // Tauri/Radix exposes the select trigger as a keyboard tab stop.
                // Native has no DOM focus chain, so the monitor page owns that
                // first trigger focus explicitly.
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "enter" | "space" | " " | "arrowdown" | "down"
                if self.connection_monitor.selector_focus_origin.is_some() =>
            {
                self.connection_monitor.selector_open = true;
                self.connection_monitor.selector_highlighted_index = Some(selected_index);
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "escape" if self.connection_monitor.selector_focus_origin.is_some() => {
                self.connection_monitor.selector_focus_origin = None;
                self.connection_monitor.selector_highlighted_index = None;
                cx.notify();
                true
            }
            _ => false,
        }
    }

    fn handle_open_connection_monitor_select_key(
        &mut self,
        event: &KeyDownEvent,
        connections: &[MonitorConnectionOption],
        current: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        match event.keystroke.key.as_str() {
            "escape" => {
                self.connection_monitor.selector_open = false;
                self.connection_monitor.selector_highlighted_index = None;
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "tab" => {
                self.connection_monitor.selector_open = false;
                self.connection_monitor.selector_highlighted_index = None;
                self.connection_monitor.selector_focus_origin = None;
                cx.notify();
                true
            }
            "arrowdown" | "down" => {
                self.connection_monitor.selector_highlighted_index =
                    Some(browser_behavior::browser_select_next_index(
                        current,
                        connections.len(),
                        browser_behavior::BrowserSelectKeyDirection::Next,
                    ));
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "arrowup" | "up" => {
                self.connection_monitor.selector_highlighted_index =
                    Some(browser_behavior::browser_select_next_index(
                        current,
                        connections.len(),
                        browser_behavior::BrowserSelectKeyDirection::Previous,
                    ));
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "home" => {
                self.connection_monitor.selector_highlighted_index = Some(0);
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "end" => {
                self.connection_monitor.selector_highlighted_index =
                    Some(connections.len().saturating_sub(1));
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "enter" | "space" | " " => {
                if let Some(connection) = connections.get(current.min(connections.len() - 1)) {
                    self.connection_monitor.selected_connection_id =
                        Some(connection.connection_id.clone());
                    self.connection_monitor.selector_open = false;
                    self.connection_monitor.selector_highlighted_index = None;
                    self.connection_monitor.selector_focus_origin =
                        Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                    self.sync_connection_monitor_selection(cx);
                }
                true
            }
            _ => false,
        }
    }

    fn render_monitor_panel_header(
        &self,
        connection: &MonitorConnectionOption,
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
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "system-health-connection-endpoint",
                                connection.connection_id.as_str(),
                                format!("{}@{}", connection.username, connection.host),
                                theme.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(12.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "system-health-connection-port",
                                connection.connection_id.as_str(),
                                format!(":{}", connection.port),
                                theme.text_muted,
                                cx,
                            )),
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
                                let profiler_state = this
                                    .connection_monitor
                                    .profiler_registry
                                    .state(&connection_id);
                                if this
                                    .connection_monitor
                                    .disabled_profiler_connections
                                    .contains(&connection_id)
                                    || !matches!(profiler_state, Some(ProfilerState::Running))
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

    fn render_retry_sampling_button(
        &self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px_3()
            .py_1()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
            .text_size(px(12.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor_pointer()
            .hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "system-health-profiler",
                "retry",
                self.i18n.t("profiler.panel.retry"),
                self.tokens.ui.text_muted,
                cx,
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.start_connection_monitor_profiler(connection_id.clone(), cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_compact_system_health_metrics(
        &self,
        metrics: &ResourceMetrics,
        is_rtt_only: bool,
        can_retry_sampling: bool,
        connection_id: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = Arc::new(self.compact_monitor_rows(
            metrics,
            is_rtt_only,
            can_retry_sampling.then_some(connection_id),
        ));
        self.sync_compact_monitor_list_state(&rows);
        let state = self.connection_monitor.compact_monitor_list_state.clone();
        let spec = self.compact_monitor_list_spec();
        let workspace = cx.entity();

        div()
            .size_full()
            .child(tauri_virtual_list(state, spec, move |index, _window, cx| {
                let rows = rows.clone();
                workspace.update(cx, |this, cx| {
                    this.render_compact_monitor_virtual_row(rows.get(index).cloned(), cx)
                })
            }))
            .into_any_element()
    }

    fn compact_monitor_rows(
        &self,
        metrics: &ResourceMetrics,
        is_rtt_only: bool,
        retry_connection_id: Option<String>,
    ) -> Vec<CompactMonitorRow> {
        let theme = self.tokens.ui;
        let mut rows = Vec::new();

        if !is_rtt_only {
            if let Some(cpu) = metrics.cpu_percent {
                rows.push(CompactMonitorRow::Metric {
                    icon: LucideIcon::Cpu,
                    label: self.i18n.t("profiler.panel.cpu"),
                    value: format!("{cpu:.1}%"),
                    value_color: threshold_color(Some(cpu)),
                });
            }
            if metrics.memory_used.is_some() && metrics.memory_total.is_some() {
                rows.push(CompactMonitorRow::Metric {
                    icon: LucideIcon::MemoryStick,
                    label: self.i18n.t("profiler.panel.memory"),
                    value: format!(
                        "{} / {}",
                        format_bytes(metrics.memory_used.unwrap_or_default()),
                        format_bytes(metrics.memory_total.unwrap_or_default())
                    ),
                    value_color: threshold_color(metrics.memory_percent),
                });
            }
            if metrics.swap_used.is_some() && metrics.swap_total.is_some() {
                rows.push(CompactMonitorRow::Metric {
                    icon: LucideIcon::MemoryStick,
                    label: self.i18n.t("profiler.panel.swap"),
                    value: format!(
                        "{} / {}",
                        format_bytes(metrics.swap_used.unwrap_or_default()),
                        format_bytes(metrics.swap_total.unwrap_or_default())
                    ),
                    value_color: threshold_color(metrics.swap_percent),
                });
            }
            if metrics.disk_used.is_some() && metrics.disk_total.is_some() {
                rows.push(CompactMonitorRow::Metric {
                    icon: LucideIcon::HardDrive,
                    label: self.i18n.t("profiler.panel.disk"),
                    value: format!(
                        "{} / {}",
                        format_bytes(metrics.disk_used.unwrap_or_default()),
                        format_bytes(metrics.disk_total.unwrap_or_default())
                    ),
                    value_color: threshold_color(metrics.disk_percent),
                });
            }
            if let Some(load) = metrics.load_avg_1 {
                rows.push(CompactMonitorRow::Metric {
                    icon: LucideIcon::Gauge,
                    label: self.i18n.t("profiler.panel.load_avg"),
                    value: format!(
                        "{load:.2} / {:.2} / {:.2}",
                        metrics.load_avg_5.unwrap_or_default(),
                        metrics.load_avg_15.unwrap_or_default()
                    ),
                    value_color: theme.text,
                });
            }
            if metrics.net_rx_bytes_per_sec.is_some() || metrics.net_tx_bytes_per_sec.is_some() {
                rows.push(CompactMonitorRow::Network {
                    rx: format_rate(metrics.net_rx_bytes_per_sec.unwrap_or_default()),
                    tx: format_rate(metrics.net_tx_bytes_per_sec.unwrap_or_default()),
                });
            }
            self.push_compact_disk_rows(metrics, &mut rows);
            self.push_compact_interface_rows(metrics, &mut rows);
            self.push_compact_process_rows(metrics, &mut rows);
        }

        rows.push(CompactMonitorRow::Metric {
            icon: LucideIcon::Activity,
            label: self.i18n.t("profiler.panel.rtt"),
            value: metrics
                .ssh_rtt_ms
                .map(|rtt| format!("{rtt} ms"))
                .unwrap_or_else(|| "—".to_string()),
            value_color: rtt_color(metrics.ssh_rtt_ms),
        });
        rows.push(CompactMonitorRow::Metric {
            icon: LucideIcon::Info,
            label: self.i18n.t("profiler.panel.source"),
            value: self.i18n.t(metrics_source_label_key(metrics.source)),
            value_color: theme.text_muted,
        });
        if let Some(connection_id) = retry_connection_id {
            rows.push(CompactMonitorRow::Retry { connection_id });
        }
        rows
    }

    fn push_compact_disk_rows(&self, metrics: &ResourceMetrics, rows: &mut Vec<CompactMonitorRow>) {
        if metrics.disks.is_empty() {
            return;
        }
        let theme = self.tokens.ui;
        rows.push(CompactMonitorRow::Section {
            icon: LucideIcon::HardDrive,
            label: self.i18n.t("profiler.panel.mounts"),
        });
        for disk in metrics.disks.iter().take(8) {
            rows.push(CompactMonitorRow::Detail {
                name: disk.mount_point.clone(),
                value: disk
                    .percent
                    .map(|percent| format!("{percent:.0}%"))
                    .unwrap_or_else(|| "—".to_string()),
                value_color: theme.text_muted,
            });
        }
    }

    fn push_compact_interface_rows(
        &self,
        metrics: &ResourceMetrics,
        rows: &mut Vec<CompactMonitorRow>,
    ) {
        if metrics.net_interfaces.is_empty() {
            return;
        }
        let theme = self.tokens.ui;
        rows.push(CompactMonitorRow::Section {
            icon: LucideIcon::Wifi,
            label: self.i18n.t("profiler.panel.interfaces"),
        });
        for iface in metrics.net_interfaces.iter().take(8) {
            let rx = iface
                .rx_bytes_per_sec
                .map(format_rate)
                .unwrap_or_else(|| "—".to_string());
            let tx = iface
                .tx_bytes_per_sec
                .map(format_rate)
                .unwrap_or_else(|| "—".to_string());
            rows.push(CompactMonitorRow::Detail {
                name: iface.name.clone(),
                value: format!("rx {rx} / tx {tx}"),
                value_color: theme.text_muted,
            });
        }
    }

    fn push_compact_process_rows(
        &self,
        metrics: &ResourceMetrics,
        rows: &mut Vec<CompactMonitorRow>,
    ) {
        if metrics.top_processes.is_empty() {
            return;
        }
        rows.push(CompactMonitorRow::Section {
            icon: LucideIcon::Activity,
            label: self.i18n.t("profiler.panel.top_processes"),
        });
        for process in metrics.top_processes.iter().take(8) {
            rows.push(CompactMonitorRow::Detail {
                name: format!("{} {}", process.pid, process.command),
                value: format!("{:.1}%", process.memory_percent),
                value_color: threshold_color(Some(process.memory_percent)),
            });
        }
    }

    fn sync_compact_monitor_list_state(&self, rows: &[CompactMonitorRow]) {
        let signatures = rows
            .iter()
            .map(compact_monitor_row_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.compact_monitor_list_state,
            &mut self
                .connection_monitor
                .compact_monitor_list_cache
                .borrow_mut(),
            "host-tools-monitor-compact",
            &signatures,
            self.compact_monitor_list_spec(),
        );
    }

    fn compact_monitor_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(COMPACT_MONITOR_LIST_ESTIMATED_ROW_HEIGHT),
            COMPACT_MONITOR_LIST_OVERSCAN,
        )
    }

    fn render_compact_monitor_virtual_row(
        &self,
        row: Option<CompactMonitorRow>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = row else {
            return div().into_any_element();
        };
        match row {
            CompactMonitorRow::Metric {
                icon,
                label,
                value,
                value_color,
            } => self.render_compact_monitor_metric_row(icon, label, value, value_color),
            CompactMonitorRow::Network { rx, tx } => {
                self.render_compact_monitor_network_row(rx, tx)
            }
            CompactMonitorRow::Section { icon, label } => {
                self.render_compact_monitor_section_row(icon, label)
            }
            CompactMonitorRow::Detail {
                name,
                value,
                value_color,
            } => self.render_compact_monitor_detail_row(name, value, value_color),
            CompactMonitorRow::Retry { connection_id } => div()
                .w_full()
                .h(px(COMPACT_MONITOR_RETRY_ROW_HEIGHT))
                .flex()
                .items_center()
                .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
                .child(self.render_retry_sampling_button(connection_id, cx))
                .into_any_element(),
        }
    }

    fn render_compact_monitor_metric_row(
        &self,
        icon: LucideIcon,
        label: String,
        value: String,
        value_color: u32,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Compact metric rows stay flat so labels keep room in the narrow
        // companion panel while the GPUI List owns the hot scroll surface.
        div()
            .w_full()
            .h(px(COMPACT_MONITOR_METRIC_ROW_HEIGHT))
            .min_w_0()
            .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .text_size(px(12.0))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(icon, 13.0, rgb(theme.text_muted)))
                    .child(div().min_w_0().truncate().child(label)),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(relative(COMPACT_MONITOR_VALUE_MAX_WIDTH_RATIO))
                    .truncate()
                    .font_family("monospace")
                    .text_align(gpui::TextAlign::Right)
                    .text_color(rgb(value_color))
                    .child(value),
            )
            .into_any_element()
    }

    fn render_compact_monitor_network_row(&self, rx: String, tx: String) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .h(px(COMPACT_MONITOR_METRIC_ROW_HEIGHT))
            .min_w_0()
            .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .text_size(px(12.0))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Wifi,
                        13.0,
                        rgb(theme.text_muted),
                    ))
                    .child(div().min_w_0().truncate().child(self.i18n.t("profiler.panel.network"))),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(relative(COMPACT_MONITOR_VALUE_MAX_WIDTH_RATIO))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .font_family("monospace")
                    .child(
                        div()
                            .flex_none()
                            .truncate()
                            .text_color(rgb(MONITOR_EMERALD))
                            .child(format!("↓ {rx}")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .truncate()
                            .text_color(rgb(MONITOR_AMBER))
                            .child(format!("↑ {tx}")),
                    ),
            )
            .into_any_element()
    }

    fn render_compact_monitor_section_row(&self, icon: LucideIcon, label: String) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .h(px(COMPACT_MONITOR_SECTION_ROW_HEIGHT))
            .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
            .flex()
            .items_center()
            .gap(px(6.0))
            .min_w_0()
            .text_size(px(12.0))
            .text_color(rgb(theme.text_muted))
            .child(Self::render_lucide_icon(icon, 13.0, rgb(theme.text_muted)))
            .child(div().min_w_0().truncate().child(label))
            .into_any_element()
    }

    fn render_compact_monitor_detail_row(
        &self,
        name: String,
        value: String,
        value_color: u32,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Detail rows are plain measured list items, not selectable dashboard
        // widgets, so scroll stays owned by the GPUI List surface.
        div()
            .w_full()
            .h(px(COMPACT_MONITOR_DETAIL_ROW_HEIGHT))
            .flex()
            .items_center()
            .min_w_0()
            .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
            .text_size(px(11.0))
            .font_family("monospace")
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .pl(px(COMPACT_MONITOR_DETAIL_INDENT))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_color(rgb(theme.text))
                            .child(name),
                    )
                    .child(
                        div()
                            .flex_none()
                            .max_w(relative(COMPACT_MONITOR_DETAIL_VALUE_MAX_WIDTH_RATIO))
                            .truncate()
                            .text_align(gpui::TextAlign::Right)
                            .text_color(rgb(value_color))
                            .child(value),
                    ),
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
        selectable: bool,
        cx: &mut Context<Self>,
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
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-metric-label",
                                &label,
                                label.clone(),
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(12.0))
                            .text_color(rgb(color))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-metric-value",
                                &label,
                                value,
                                color,
                                cx,
                            )),
                    ),
            )
            .child(progress(&self.tokens, progress_value, false).h(px(6.0)))
            .when(
                history.iter().filter_map(|value| *value).count() >= 2,
                |card| card.child(render_sparkline(history, color)),
            )
            .into_any_element()
    }

    fn metric_history(
        show_history: bool,
        history: &[ResourceMetrics],
        value: impl Fn(&ResourceMetrics) -> Option<f64>,
    ) -> Vec<Option<f64>> {
        // Compact sidebars avoid sparkline canvas work; full pages keep history.
        if show_history {
            history.iter().map(value).collect()
        } else {
            Vec::new()
        }
    }

    fn render_monitor_text(
        &self,
        selectable: bool,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        if selectable {
            self.render_selectable_text_scoped(scope, key, text, color, cx)
        } else {
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                scope,
                key,
                text,
                color,
                cx,
            )
        }
    }

    fn render_network_metric_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let rx_rate = format_rate(metrics.net_rx_bytes_per_sec.unwrap_or_default());
        let tx_rate = format_rate(metrics.net_tx_bytes_per_sec.unwrap_or_default());
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
                    .child(self.render_monitor_text(
                        selectable,
                        "system-health-section-label",
                        "network",
                        self.i18n.t("profiler.panel.network"),
                        theme.text_muted,
                        cx,
                    )),
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
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-network-rx",
                                (),
                                rx_rate,
                                self.tokens.ui.text,
                                cx,
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
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-network-tx",
                                (),
                                tx_rate,
                                self.tokens.ui.text,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_disk_list_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_monitor_list_card(
            LucideIcon::HardDrive,
            self.i18n.t("profiler.panel.mounts"),
            metrics
                .disks
                .iter()
                .take(4)
                .map(|disk| {
                    let value = disk
                        .percent
                        .map(|percent| format!("{percent:.0}%"))
                        .unwrap_or_else(|| "—".to_string());
                    (disk.mount_point.clone(), value)
                })
                .collect(),
            selectable,
            cx,
        )
    }

    fn render_interface_list_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_monitor_list_card(
            LucideIcon::Wifi,
            self.i18n.t("profiler.panel.interfaces"),
            metrics
                .net_interfaces
                .iter()
                .take(4)
                .map(|iface| {
                    let rx = iface
                        .rx_bytes_per_sec
                        .map(format_rate)
                        .unwrap_or_else(|| "—".to_string());
                    let tx = iface
                        .tx_bytes_per_sec
                        .map(format_rate)
                        .unwrap_or_else(|| "—".to_string());
                    (iface.name.clone(), format!("rx {rx} / tx {tx}"))
                })
                .collect(),
            selectable,
            cx,
        )
    }

    fn render_top_process_list_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_monitor_list_card(
            LucideIcon::Activity,
            self.i18n.t("profiler.panel.top_processes"),
            metrics
                .top_processes
                .iter()
                .take(5)
                .map(|process| {
                    (
                        format!("{} {}", process.pid, process.command),
                        format!("{:.1}%", process.memory_percent),
                    )
                })
                .collect(),
            selectable,
            cx,
        )
    }

    fn render_monitor_list_card(
        &self,
        icon: LucideIcon,
        label: String,
        rows: Vec<(String, String)>,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut card = div()
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
                    .gap_1()
                    .min_w(px(0.0))
                    .text_size(px(12.0))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(icon, 14.0, rgb(theme.text_muted)))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .truncate()
                            .whitespace_nowrap()
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-list-label",
                                &label,
                                label.clone(),
                                theme.text_muted,
                                cx,
                            )),
                    ),
            );
        for (index, (name, value)) in rows.into_iter().enumerate() {
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .min_w(px(0.0))
                    .text_size(px(11.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .whitespace_nowrap()
                            .font_family("monospace")
                            .text_color(rgb(theme.text))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-list-name",
                                (&label, index),
                                name,
                                theme.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(180.0))
                            .truncate()
                            .whitespace_nowrap()
                            .font_family("monospace")
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-list-value",
                                (&label, index),
                                value,
                                theme.text_muted,
                                cx,
                            )),
                    ),
            );
        }
        card.into_any_element()
    }

    fn render_compact_metric_box(
        &self,
        icon: LucideIcon,
        label: String,
        value: String,
        value_color: u32,
        selectable: bool,
        cx: &mut Context<Self>,
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
                    .child(self.render_monitor_text(
                        selectable,
                        "monitor-compact-metric-label",
                        &label,
                        label.clone(),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .font_family("monospace")
                    .text_size(px(12.0))
                    .text_color(rgb(value_color))
                    .child(self.render_monitor_text(
                        selectable,
                        "monitor-compact-metric-value",
                        &label,
                        value,
                        value_color,
                        cx,
                    )),
            )
            .into_any_element()
    }
}

fn compact_monitor_row_signature(row: &CompactMonitorRow) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        CompactMonitorRow::Metric {
            label,
            value,
            value_color,
            ..
        } => {
            "metric".hash(&mut hasher);
            label.hash(&mut hasher);
            value.hash(&mut hasher);
            value_color.hash(&mut hasher);
        }
        CompactMonitorRow::Network { rx, tx } => {
            "network".hash(&mut hasher);
            rx.hash(&mut hasher);
            tx.hash(&mut hasher);
        }
        CompactMonitorRow::Section { label, .. } => {
            "section".hash(&mut hasher);
            label.hash(&mut hasher);
        }
        CompactMonitorRow::Detail {
            name,
            value,
            value_color,
        } => {
            "detail".hash(&mut hasher);
            name.hash(&mut hasher);
            value.hash(&mut hasher);
            value_color.hash(&mut hasher);
        }
        CompactMonitorRow::Retry { connection_id } => {
            "retry".hash(&mut hasher);
            connection_id.hash(&mut hasher);
        }
    }
    hasher.finish()
}
