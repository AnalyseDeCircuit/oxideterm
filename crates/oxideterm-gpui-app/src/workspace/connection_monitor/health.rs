impl WorkspaceApp {
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
                cx,
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
                cx,
            ));
        }
        if !is_rtt_only && metrics.disk_used.is_some() && metrics.disk_total.is_some() {
            panel = panel.child(self.render_metric_card(
                self.i18n.t("profiler.panel.disk"),
                format!(
                    "{} / {}",
                    format_bytes(metrics.disk_used.unwrap_or_default()),
                    format_bytes(metrics.disk_total.unwrap_or_default())
                ),
                LucideIcon::HardDrive,
                threshold_color(metrics.disk_percent),
                metrics.disk_percent.map(|value| value as f32),
                history.iter().map(|metric| metric.disk_percent).collect(),
                cx,
            ));
        }
        if !is_rtt_only
            && (metrics.net_rx_bytes_per_sec.is_some() || metrics.net_tx_bytes_per_sec.is_some())
        {
            panel = panel.child(self.render_network_metric_card(metrics, cx));
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
                            self.tokens.ui.text,
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
                            cx,
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
                    .child(self.render_selectable_display_text(
                        "monitor-metric-source-label",
                        "profiler.panel.source",
                        self.i18n.t("profiler.panel.source"),
                        self.tokens.ui.text_muted,
                        cx,
                    ))
                    .child(
                        div()
                            .font_family("monospace")
                            .child(self.render_selectable_text_scoped(
                                "monitor-metric-source",
                                (),
                                metrics_source_label(metrics.source),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
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
                            .child(self.render_selectable_display_text(
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
                            .child(self.render_selectable_text_scoped(
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

    fn render_network_metric_card(
        &self,
        metrics: &ResourceMetrics,
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
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
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
                            .child(self.render_selectable_text_scoped(
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
                            .child(self.render_selectable_text_scoped(
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

    fn render_compact_metric_box(
        &self,
        icon: LucideIcon,
        label: String,
        value: String,
        value_color: u32,
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
                    .child(self.render_selectable_display_text(
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
                    .child(self.render_selectable_text_scoped(
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
