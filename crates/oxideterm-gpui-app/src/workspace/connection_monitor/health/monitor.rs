//! Owns system-health monitoring presentation inside Host Tools.

use super::*;

use oxideterm_connection_monitor::ProfilerState;
use oxideterm_gpui_ui::progress::progress;

pub(in crate::workspace::connection_monitor) struct MonitorRenderContext {
    pub(in crate::workspace::connection_monitor) tokens: ThemeTokens,
    pub(in crate::workspace::connection_monitor) i18n: I18n,
    pub(in crate::workspace::connection_monitor) mono_font_family: SharedString,
    pub(in crate::workspace::connection_monitor) selectable_text: SelectableTextRenderState,
    pub(in crate::workspace::connection_monitor) sidebar_width: f32,
}

#[derive(Clone)]
struct CompactMonitorRenderContext {
    tokens: ThemeTokens,
    i18n: I18n,
    mono_font_family: SharedString,
}

impl WorkspaceApp {
    pub(in crate::workspace::connection_monitor) fn monitor_render_context(
        &self,
        cx: &mut Context<Self>,
    ) -> MonitorRenderContext {
        // This snapshot is frame-scoped. Cloning I18n shares its catalog Arc
        // and does not duplicate the locale tables.
        MonitorRenderContext {
            tokens: self.tokens,
            i18n: self.i18n.clone(),
            mono_font_family: settings_mono_font_family(self.settings_store.settings()),
            selectable_text: self.selectable_text_render_state(cx),
            sidebar_width: self.ai_entity.read(cx).chat_ui().sidebar_width,
        }
    }

    pub(in crate::workspace::connection_monitor) fn render_system_health_panel(
        &self,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let render = self.monitor_render_context(cx);
        let monitor_enabled = self.settings_store.settings().host_tools.monitor_enabled;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_system_health_panel(compact, &render, monitor_enabled, cx)
        })
    }
}

impl HostToolsEntity {
    fn render_monitor_enable_control(
        &self,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px_3()
            .py_1()
            .rounded(px(render.tokens.radii.md))
            .border_1()
            .border_color(rgba((render.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
            .text_size(px(12.0))
            .cursor_pointer()
            .hover(|button| button.bg(rgb(render.tokens.ui.bg_hover)))
            .child(Self::render_monitor_text_with_role(
                render,
                SelectableTextRole::NonSelectable,
                "system-health-profiler",
                "enable",
                render.i18n.t("profiler.panel.enable"),
                render.tokens.ui.text_muted,
                cx,
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_host_tools, _event, window, cx| {
                    window.dispatch_action(
                        Box::new(HostToolsWindowRequest::new(
                            HostToolsWindowIntent::SetMonitoringEnabled {
                                tool: ContextSidebarTool::Monitor,
                                enabled: true,
                            },
                        )),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_monitor_toggle_control(
        &self,
        render: &MonitorRenderContext,
        monitor_enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_none()
            .p_1()
            .rounded(px(render.tokens.radii.md))
            .cursor_pointer()
            .text_color(if monitor_enabled {
                rgb(MONITOR_EMERALD)
            } else {
                rgb(render.tokens.ui.text_muted)
            })
            .hover(move |button| {
                if monitor_enabled {
                    button
                        .text_color(rgb(MONITOR_RED))
                        .bg(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA))
                } else {
                    button
                        .text_color(rgb(MONITOR_EMERALD))
                        .bg(rgba((MONITOR_EMERALD_DARK << 8) | MONITOR_TINT_ALPHA))
                }
            })
            .child(WorkspaceApp::render_lucide_icon(
                LucideIcon::Power,
                14.0,
                if monitor_enabled {
                    rgb(MONITOR_EMERALD)
                } else {
                    rgb(render.tokens.ui.text_muted)
                },
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_host_tools, _event, window, cx| {
                    window.dispatch_action(
                        Box::new(HostToolsWindowRequest::new(
                            HostToolsWindowIntent::SetMonitoringEnabled {
                                tool: ContextSidebarTool::Monitor,
                                enabled: !monitor_enabled,
                            },
                        )),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace::connection_monitor) fn render_system_health_panel(
        &self,
        compact: bool,
        render: &MonitorRenderContext,
        monitor_enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enable_control = self.render_monitor_enable_control(render, cx);
        let toggle_control = self.render_monitor_toggle_control(render, monitor_enabled, cx);
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
                .text_color(rgb(render.tokens.ui.text_muted))
                .child(
                    div()
                        .mb_2()
                        .opacity(0.3)
                        .child(WorkspaceApp::render_lucide_icon(
                            LucideIcon::WifiOff,
                            32.0,
                            rgb(render.tokens.ui.text_muted),
                        )),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .child(Self::render_monitor_text_with_role(
                            render,
                            SelectableTextRole::PlainDocument,
                            "system-health-empty",
                            "no-connection",
                            render.i18n.t("profiler.panel.no_connection"),
                            render.tokens.ui.text_muted,
                            cx,
                        )),
                )
                .into_any_element();
        }

        let selected_connection_id = self.selected_connection_id_owned();
        let selected_id = selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let active_connection = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .unwrap_or(&connections[0]);
        let snapshot = (!compact)
            .then(|| {
                self.profiler_registry()
                    .snapshot(&active_connection.connection_id)
            })
            .flatten();
        let current = compact
            .then(|| {
                self.profiler_registry()
                    .current(&active_connection.connection_id)
            })
            .flatten();
        let disabled = !monitor_enabled;
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
            .child(self.render_monitor_panel_header(
                &connections,
                selected_id,
                is_running,
                !compact,
                toggle_control,
                render,
                cx,
            ));

        if disabled || (!is_running && metrics.is_none()) {
            return panel
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .py_8()
                        .text_color(rgb(render.tokens.ui.text_muted))
                        .child(
                            div()
                                .mb_3()
                                .opacity(0.2)
                                .child(WorkspaceApp::render_lucide_icon(
                                    LucideIcon::Power,
                                    32.0,
                                    rgb(render.tokens.ui.text_muted),
                                )),
                        )
                        .child(div().mb_3().text_size(px(14.0)).child(
                            Self::render_monitor_text_with_role(
                                render,
                                SelectableTextRole::PlainDocument,
                                "system-health-profiler",
                                "disabled",
                                render.i18n.t("profiler.panel.disabled"),
                                render.tokens.ui.text_muted,
                                cx,
                            ),
                        ))
                        // Settings persistence stays in the transient workspace-owned control.
                        .child(enable_control),
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
                        .text_color(rgb(render.tokens.ui.text_muted))
                        .child(
                            div()
                                .mb_2()
                                .opacity(0.5)
                                .child(WorkspaceApp::render_lucide_icon(
                                    LucideIcon::Activity,
                                    20.0,
                                    rgb(render.tokens.ui.text_muted),
                                )),
                        )
                        .child(div().text_size(px(12.0)).child(
                            Self::render_monitor_text_with_role(
                                render,
                                SelectableTextRole::PlainDocument,
                                "system-health-profiler",
                                "sampling",
                                render.i18n.t("profiler.panel.sampling"),
                                render.tokens.ui.text_muted,
                                cx,
                            ),
                        )),
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
                        .text_color(rgb(render.tokens.ui.text_muted))
                        .child(div().opacity(0.6).text_size(px(12.0)).child(
                            Self::render_monitor_text_with_role(
                                render,
                                SelectableTextRole::PlainDocument,
                                "system-health-profiler",
                                "no-data",
                                render.i18n.t("profiler.panel.no_data"),
                                render.tokens.ui.text_muted,
                                cx,
                            ),
                        )),
                )
                .into_any_element();
        };

        let is_rtt_only = resource_metrics_is_rtt_only(metrics);
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
                            can_retry_sampling,
                            active_connection.connection_id.clone(),
                            render,
                            cx,
                        )),
                )
                .into_any_element();
        }

        let mut metric_body = div().flex().flex_col().gap_2();
        if metrics.system_info.is_some() {
            metric_body = metric_body
                .child(self.render_system_information_card(metrics, !compact, render, cx));
        }
        if !is_rtt_only && let Some(cpu) = metrics.cpu_percent {
            metric_body = metric_body.child(self.render_metric_card(
                render.i18n.t("profiler.panel.cpu"),
                format!("{cpu:.1}%"),
                LucideIcon::Cpu,
                threshold_color(Some(cpu)),
                Some(cpu as f32),
                Self::metric_history(show_history, &history, |metric| metric.cpu_percent),
                !compact,
                render,
                cx,
            ));
        }
        if !is_rtt_only && metrics.memory_used.is_some() && metrics.memory_total.is_some() {
            metric_body = metric_body.child(self.render_metric_card(
                render.i18n.t("profiler.panel.memory"),
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
                render,
                cx,
            ));
        }
        if !is_rtt_only && metrics.swap_used.is_some() && metrics.swap_total.is_some() {
            metric_body = metric_body.child(self.render_metric_card(
                render.i18n.t("profiler.panel.swap"),
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
                render,
                cx,
            ));
        }
        if !is_rtt_only && metrics.disk_used.is_some() && metrics.disk_total.is_some() {
            metric_body = metric_body.child(self.render_metric_card(
                render.i18n.t("profiler.panel.disk"),
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
                render,
                cx,
            ));
        }
        if !is_rtt_only && let Some(gpu_utilization) = gpu_utilization_percent(metrics) {
            metric_body = metric_body.child(self.render_metric_card(
                render.i18n.t("profiler.panel.gpu"),
                format!("{gpu_utilization:.1}%"),
                LucideIcon::Cpu,
                threshold_color(Some(gpu_utilization)),
                Some(gpu_utilization as f32),
                Self::metric_history(show_history, &history, gpu_utilization_percent),
                !compact,
                render,
                cx,
            ));
        }
        if !is_rtt_only && let Some(gpu_memory) = gpu_memory_summary(metrics) {
            metric_body = metric_body.child(self.render_metric_card(
                render.i18n.t("profiler.panel.gpu_memory"),
                format!(
                    "{} / {}",
                    format_bytes(gpu_memory.used),
                    format_bytes(gpu_memory.total)
                ),
                LucideIcon::MemoryStick,
                threshold_color(gpu_memory.percent),
                gpu_memory.percent.map(|value| value as f32),
                Self::metric_history(show_history, &history, gpu_memory_percent),
                !compact,
                render,
                cx,
            ));
        }
        if !is_rtt_only
            && (metrics.net_rx_bytes_per_sec.is_some() || metrics.net_tx_bytes_per_sec.is_some())
        {
            metric_body =
                metric_body.child(self.render_network_metric_card(metrics, !compact, render, cx));
        }
        if !is_rtt_only && !metrics.gpus.is_empty() {
            metric_body =
                metric_body.child(self.render_gpu_list_card(metrics, !compact, render, cx));
        }
        if !is_rtt_only && !metrics.disks.is_empty() {
            metric_body =
                metric_body.child(self.render_disk_list_card(metrics, !compact, render, cx));
        }
        if !is_rtt_only && !metrics.net_interfaces.is_empty() {
            metric_body =
                metric_body.child(self.render_interface_list_card(metrics, !compact, render, cx));
        }
        if !is_rtt_only && !metrics.top_processes.is_empty() {
            metric_body =
                metric_body.child(self.render_top_process_list_card(metrics, !compact, render, cx));
        }

        let metric_body =
            metric_body
                .child(
                    div()
                        .grid()
                        .grid_cols(2)
                        .gap_2()
                        .when(!is_rtt_only && metrics.load_avg_1.is_some(), |row| {
                            row.child(self.render_compact_metric_box(
                                LucideIcon::Gauge,
                                render.i18n.t("profiler.panel.load_avg"),
                                format!(
                                    "{:.2} / {:.2} / {:.2}",
                                    metrics.load_avg_1.unwrap_or_default(),
                                    metrics.load_avg_5.unwrap_or_default(),
                                    metrics.load_avg_15.unwrap_or_default()
                                ),
                                render.tokens.ui.text,
                                !compact,
                                render,
                                cx,
                            ))
                        })
                        .child(
                            self.render_compact_metric_box(
                                LucideIcon::Activity,
                                render.i18n.t("profiler.panel.rtt"),
                                metrics
                                    .ssh_rtt_ms
                                    .map(|rtt| format!("{rtt} ms"))
                                    .unwrap_or_else(|| "—".to_string()),
                                rtt_color(metrics.ssh_rtt_ms),
                                !compact,
                                render,
                                cx,
                            ),
                        ),
                )
                .when(can_retry_sampling, |panel| {
                    panel.child(self.render_retry_sampling_button(
                        active_connection.connection_id.clone(),
                        &render.tokens,
                        &render.i18n,
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
                            (render.tokens.ui.text_muted << 8) | MONITOR_SOURCE_ALPHA,
                        ))
                        .child(div().flex_none().whitespace_nowrap().child(
                            self.render_monitor_text(
                                !compact,
                                "monitor-metric-source-label",
                                "profiler.panel.source",
                                render.i18n.t("profiler.panel.source"),
                                render.tokens.ui.text_muted,
                                render,
                                cx,
                            ),
                        ))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .font_family(render.mono_font_family.clone())
                                .child(self.render_monitor_text(
                                    !compact,
                                    "monitor-metric-source",
                                    (),
                                    render.i18n.t(metrics_source_label_key(metrics.source)),
                                    render.tokens.ui.text_muted,
                                    render,
                                    cx,
                                )),
                        ),
                );

        panel.child(metric_body).into_any_element()
    }

    fn render_monitor_panel_header(
        &self,
        connections: &[MonitorConnectionOption],
        selected_id: &str,
        is_running: bool,
        show_toggle: bool,
        toggle_control: AnyElement,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = render.tokens.ui;
        div()
            .min_h(px(HOST_TOOLS_CONNECTION_ROW_HEIGHT))
            .w_full()
            .min_w_0()
            .flex()
            .items_start()
            .gap_2()
            .px_1()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.render_connection_switcher(
                        connections,
                        selected_id,
                        is_running,
                        &render.tokens,
                        render.mono_font_family.clone(),
                        &render.selectable_text,
                        cx,
                    )),
            )
            .when(show_toggle, |header| {
                // The control persists settings in WorkspaceApp, but Entity owns
                // its placement and the rest of the monitor header.
                header.child(toggle_control)
            })
            .child(
                div()
                    .flex_none()
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

    pub(super) fn render_retry_sampling_button(
        &self,
        connection_id: String,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px_3()
            .py_1()
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
            .text_size(px(12.0))
            .text_color(rgb(tokens.ui.text_muted))
            .cursor_pointer()
            .hover(|button| button.bg(rgb(tokens.ui.bg_hover)))
            // Button labels stay outside selectable document ownership.
            .child(i18n.t("profiler.panel.retry"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.request_profiler_refresh(connection_id.clone(), cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_compact_system_health_metrics(
        &self,
        metrics: &ResourceMetrics,
        can_retry_sampling: bool,
        connection_id: String,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = Arc::new(compact_monitor_rows(
            metrics,
            can_retry_sampling.then_some(connection_id),
        ));
        self.sync_compact_monitor_list_state(&rows, render.sidebar_width);
        let state = self.compact_monitor_list_state();
        let spec = self.compact_monitor_list_spec();
        let layout = compact_monitor_layout_for_width(render.sidebar_width);
        let host_tools = cx.entity();
        let row_render = CompactMonitorRenderContext {
            tokens: render.tokens,
            i18n: render.i18n.clone(),
            mono_font_family: render.mono_font_family.clone(),
        };

        div()
            .size_full()
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    let rows = rows.clone();
                    let row_render = row_render.clone();
                    host_tools.update(cx, |host_tools, cx| {
                        host_tools.render_compact_monitor_virtual_row(
                            rows.get(index).cloned(),
                            layout,
                            &row_render,
                            cx,
                        )
                    })
                },
            ))
            .into_any_element()
    }

    pub(super) fn sync_compact_monitor_list_state(
        &self,
        rows: &[CompactMonitorRow],
        sidebar_width: f32,
    ) {
        let signatures = rows
            .iter()
            .map(compact_monitor_row_signature)
            .collect::<Vec<_>>();
        let layout = compact_monitor_layout_for_width(sidebar_width);
        self.sync_compact_monitor_list_signatures(
            compact_monitor_list_identity(layout),
            &signatures,
        );
    }

    pub(super) fn compact_monitor_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(COMPACT_MONITOR_LIST_ESTIMATED_ROW_HEIGHT),
            COMPACT_MONITOR_LIST_OVERSCAN,
        )
    }

    fn render_compact_monitor_virtual_row(
        &self,
        row: Option<CompactMonitorRow>,
        layout: CompactMonitorLayout,
        render: &CompactMonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = row else {
            return div().into_any_element();
        };
        match row {
            CompactMonitorRow::Metric { kind, value, level } => {
                let value = if kind == MonitorMetricKind::Source {
                    render.i18n.t(&value)
                } else {
                    value
                };
                self.render_compact_monitor_metric_row(
                    monitor_metric_icon(kind),
                    self.compact_monitor_metric_label(kind, render),
                    value,
                    monitor_value_level_color(level, render.tokens.ui.text_muted),
                    render,
                )
            }
            CompactMonitorRow::Network { rx, tx } => {
                self.render_compact_monitor_network_row(rx, tx, layout, render)
            }
            CompactMonitorRow::Section { kind } => self.render_compact_monitor_section_row(
                monitor_section_icon(kind),
                render.i18n.t(monitor_section_label_key(kind)),
                render,
            ),
            CompactMonitorRow::Detail { name, value, level } => self
                .render_compact_monitor_detail_row(
                    name,
                    value,
                    monitor_value_level_color(level, render.tokens.ui.text_muted),
                    render,
                ),
            CompactMonitorRow::Interface { name, rx, tx } => {
                self.render_compact_monitor_interface_row(name, rx, tx, layout, render)
            }
            CompactMonitorRow::Retry { connection_id } => div()
                .w_full()
                .h(px(COMPACT_MONITOR_RETRY_ROW_HEIGHT))
                .flex()
                .items_center()
                .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
                .child(self.render_retry_sampling_button(
                    connection_id,
                    &render.tokens,
                    &render.i18n,
                    cx,
                ))
                .into_any_element(),
        }
    }

    fn render_compact_monitor_metric_row(
        &self,
        icon: LucideIcon,
        label: String,
        value: String,
        value_color: u32,
        render: &CompactMonitorRenderContext,
    ) -> AnyElement {
        let theme = render.tokens.ui;
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
                    .child(WorkspaceApp::render_lucide_icon(
                        icon,
                        13.0,
                        rgb(theme.text_muted),
                    ))
                    .child(div().min_w_0().truncate().child(label)),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(relative(COMPACT_MONITOR_VALUE_MAX_WIDTH_RATIO))
                    .truncate()
                    .font_family(render.mono_font_family.clone())
                    .text_align(gpui::TextAlign::Right)
                    .text_color(rgb(value_color))
                    .child(value),
            )
            .into_any_element()
    }

    fn compact_monitor_metric_label(
        &self,
        kind: MonitorMetricKind,
        render: &CompactMonitorRenderContext,
    ) -> String {
        match kind {
            MonitorMetricKind::Source => render.i18n.t("profiler.panel.source"),
            _ => render.i18n.t(monitor_metric_label_key(kind)),
        }
    }

    fn monitor_level_color(&self, level: MonitorValueLevel, render: &MonitorRenderContext) -> u32 {
        monitor_value_level_color(level, render.tokens.ui.text_muted)
    }

    fn render_compact_monitor_network_row(
        &self,
        rx: String,
        tx: String,
        layout: CompactMonitorLayout,
        render: &CompactMonitorRenderContext,
    ) -> AnyElement {
        let theme = render.tokens.ui;
        if layout == CompactMonitorLayout::Stacked {
            return div()
                .w_full()
                .h(px(COMPACT_MONITOR_STACKED_ROW_HEIGHT))
                .min_w_0()
                .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
                .flex()
                .flex_col()
                .justify_center()
                .gap_1()
                .text_size(px(12.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_color(rgb(theme.text_muted))
                        .child(WorkspaceApp::render_lucide_icon(
                            LucideIcon::Wifi,
                            13.0,
                            rgb(theme.text_muted),
                        ))
                        .child(render.i18n.t("profiler.panel.network")),
                )
                .child(
                    div()
                        .min_w_0()
                        .pl(px(COMPACT_MONITOR_DETAIL_INDENT))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .font_family(render.mono_font_family.clone())
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_color(rgb(MONITOR_EMERALD))
                                .child(format!("↓ {rx}")),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_align(gpui::TextAlign::Right)
                                .text_color(rgb(MONITOR_AMBER))
                                .child(format!("↑ {tx}")),
                        ),
                )
                .into_any_element();
        }

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
                    .child(WorkspaceApp::render_lucide_icon(
                        LucideIcon::Wifi,
                        13.0,
                        rgb(theme.text_muted),
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(render.i18n.t("profiler.panel.network")),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(relative(COMPACT_MONITOR_VALUE_MAX_WIDTH_RATIO))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .font_family(render.mono_font_family.clone())
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

    fn render_compact_monitor_section_row(
        &self,
        icon: LucideIcon,
        label: String,
        render: &CompactMonitorRenderContext,
    ) -> AnyElement {
        let theme = render.tokens.ui;
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
            .child(WorkspaceApp::render_lucide_icon(
                icon,
                13.0,
                rgb(theme.text_muted),
            ))
            .child(div().min_w_0().truncate().child(label))
            .into_any_element()
    }

    fn render_compact_monitor_detail_row(
        &self,
        name: String,
        value: String,
        value_color: u32,
        render: &CompactMonitorRenderContext,
    ) -> AnyElement {
        let theme = render.tokens.ui;
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
            .font_family(render.mono_font_family.clone())
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

    fn render_compact_monitor_interface_row(
        &self,
        name: String,
        rx: String,
        tx: String,
        layout: CompactMonitorLayout,
        render: &CompactMonitorRenderContext,
    ) -> AnyElement {
        let theme = render.tokens.ui;
        if layout == CompactMonitorLayout::Stacked {
            return div()
                .w_full()
                .h(px(COMPACT_MONITOR_STACKED_ROW_HEIGHT))
                .min_w_0()
                .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
                .pl(px(
                    COMPACT_MONITOR_ROW_SIDE_PADDING + COMPACT_MONITOR_DETAIL_INDENT
                ))
                .flex()
                .flex_col()
                .justify_center()
                .gap_1()
                .font_family(render.mono_font_family.clone())
                .text_size(px(11.0))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.text))
                        .child(name),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .text_color(rgb(theme.text_muted))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .child(format!("rx {rx}")),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_align(gpui::TextAlign::Right)
                                .child(format!("tx {tx}")),
                        ),
                )
                .into_any_element();
        }

        self.render_compact_monitor_detail_row(
            name,
            format!("rx {rx} / tx {tx}"),
            theme.text_muted,
            render,
        )
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
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = render.tokens.ui;
        div()
            .rounded(px(render.tokens.radii.md))
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
                            .child(WorkspaceApp::render_lucide_icon(
                                icon,
                                14.0,
                                rgb(theme.text_muted),
                            ))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-metric-label",
                                &label,
                                label.clone(),
                                theme.text_muted,
                                render,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .font_family(render.mono_font_family.clone())
                            .text_size(px(12.0))
                            .text_color(rgb(color))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-metric-value",
                                &label,
                                value,
                                color,
                                render,
                                cx,
                            )),
                    ),
            )
            .child(progress(&render.tokens, progress_value, false).h(px(6.0)))
            .when(
                history.iter().filter_map(|value| *value).count() >= 2,
                |card| card.child(render_sparkline(history, color)),
            )
            .into_any_element()
    }

    pub(super) fn metric_history(
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
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        if selectable {
            render
                .selectable_text
                .render_display_text_with_role_in_group(
                    SelectableTextRole::PlainDocument,
                    selectable_document_group_id(),
                    scope,
                    key,
                    0,
                    text,
                    color,
                    cx,
                )
        } else {
            Self::render_monitor_text_with_role(
                render,
                SelectableTextRole::NonSelectable,
                scope,
                key,
                text,
                color,
                cx,
            )
        }
    }

    fn render_monitor_text_with_role(
        render: &MonitorRenderContext,
        role: SelectableTextRole,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut App,
    ) -> AnyElement {
        render
            .selectable_text
            .render_display_text_with_role_in_group(
                role,
                selectable_document_group_id(),
                scope,
                key,
                0,
                text,
                color,
                cx,
            )
    }

    fn render_network_metric_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = render.tokens.ui;
        let rx_rate = format_rate(metrics.net_rx_bytes_per_sec.unwrap_or_default());
        let tx_rate = format_rate(metrics.net_tx_bytes_per_sec.unwrap_or_default());
        div()
            .rounded(px(render.tokens.radii.md))
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
                    .child(WorkspaceApp::render_lucide_icon(
                        LucideIcon::Wifi,
                        14.0,
                        rgb(theme.text_muted),
                    ))
                    .child(self.render_monitor_text(
                        selectable,
                        "system-health-section-label",
                        "network",
                        render.i18n.t("profiler.panel.network"),
                        theme.text_muted,
                        render,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .font_family(render.mono_font_family.clone())
                    .text_size(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(WorkspaceApp::render_lucide_icon(
                                LucideIcon::ArrowDown,
                                12.0,
                                rgb(MONITOR_EMERALD),
                            ))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-network-rx",
                                (),
                                rx_rate,
                                render.tokens.ui.text,
                                render,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(WorkspaceApp::render_lucide_icon(
                                LucideIcon::ArrowUp,
                                12.0,
                                rgb(MONITOR_AMBER),
                            ))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-network-tx",
                                (),
                                tx_rate,
                                render.tokens.ui.text,
                                render,
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
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_monitor_list_card(
            LucideIcon::HardDrive,
            render.i18n.t("profiler.panel.mounts"),
            disk_list_rows(metrics, 4),
            selectable,
            render,
            cx,
        )
    }

    fn render_system_information_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(system_info) = metrics.system_info.as_ref() else {
            return div().into_any_element();
        };
        let mut rows = Vec::new();
        let mut push_row = |label_key: &str, value: Option<String>| {
            if let Some(value) = value {
                rows.push(MonitorListRow {
                    name: render.i18n.t(label_key),
                    value,
                    level: MonitorValueLevel::Normal,
                });
            }
        };
        push_row("profiler.panel.system", system_info.system_name.clone());
        push_row(
            "profiler.panel.system_version",
            system_info.system_version.clone(),
        );
        push_row(
            "profiler.panel.architecture",
            system_info.architecture.clone(),
        );
        push_row(
            "profiler.panel.boot_time",
            system_info.boot_time_ms.and_then(format_boot_time),
        );
        push_row(
            "profiler.panel.uptime",
            system_info.uptime_seconds.map(format_uptime),
        );

        self.render_monitor_list_card(
            LucideIcon::Monitor,
            render.i18n.t("profiler.panel.system_information"),
            rows,
            selectable,
            render,
            cx,
        )
    }

    fn render_interface_list_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_monitor_list_card(
            LucideIcon::Wifi,
            render.i18n.t("profiler.panel.interfaces"),
            interface_list_rows(metrics, 4),
            selectable,
            render,
            cx,
        )
    }

    fn render_gpu_list_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_monitor_list_card(
            LucideIcon::Cpu,
            render.i18n.t("profiler.panel.gpus"),
            gpu_list_rows(metrics, 4),
            selectable,
            render,
            cx,
        )
    }

    fn render_top_process_list_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_monitor_list_card(
            LucideIcon::Activity,
            render.i18n.t("profiler.panel.top_processes"),
            top_process_list_rows(metrics, 5),
            selectable,
            render,
            cx,
        )
    }

    fn render_monitor_list_card(
        &self,
        icon: LucideIcon,
        label: String,
        rows: Vec<MonitorListRow>,
        selectable: bool,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = render.tokens.ui;
        let mut card = div()
            .rounded(px(render.tokens.radii.md))
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
                    .child(WorkspaceApp::render_lucide_icon(
                        icon,
                        14.0,
                        rgb(theme.text_muted),
                    ))
                    .child(div().min_w(px(0.0)).truncate().whitespace_nowrap().child(
                        self.render_monitor_text(
                            selectable,
                            "monitor-list-label",
                            &label,
                            label.clone(),
                            theme.text_muted,
                            render,
                            cx,
                        ),
                    )),
            );
        for (index, row) in rows.into_iter().enumerate() {
            let value_color = self.monitor_level_color(row.level, render);
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
                            .font_family(render.mono_font_family.clone())
                            .text_color(rgb(theme.text))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-list-name",
                                (&label, index),
                                row.name,
                                theme.text,
                                render,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(180.0))
                            .truncate()
                            .whitespace_nowrap()
                            .font_family(render.mono_font_family.clone())
                            .text_color(rgb(value_color))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-list-value",
                                (&label, index),
                                row.value,
                                value_color,
                                render,
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
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = render.tokens.ui;
        div()
            .rounded(px(render.tokens.radii.md))
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
                    .child(WorkspaceApp::render_lucide_icon(
                        icon,
                        14.0,
                        rgb(theme.text_muted),
                    ))
                    .child(self.render_monitor_text(
                        selectable,
                        "monitor-compact-metric-label",
                        &label,
                        label.clone(),
                        theme.text_muted,
                        render,
                        cx,
                    )),
            )
            .child(
                div()
                    .font_family(render.mono_font_family.clone())
                    .text_size(px(12.0))
                    .text_color(rgb(value_color))
                    .child(self.render_monitor_text(
                        selectable,
                        "monitor-compact-metric-value",
                        &label,
                        value,
                        value_color,
                        render,
                        cx,
                    )),
            )
            .into_any_element()
    }
}

fn monitor_metric_icon(kind: MonitorMetricKind) -> LucideIcon {
    match kind {
        MonitorMetricKind::System => LucideIcon::Monitor,
        MonitorMetricKind::SystemVersion => LucideIcon::Info,
        MonitorMetricKind::Architecture => LucideIcon::Cpu,
        MonitorMetricKind::BootTime | MonitorMetricKind::Uptime => LucideIcon::Clock,
        MonitorMetricKind::Cpu | MonitorMetricKind::Gpu => LucideIcon::Cpu,
        MonitorMetricKind::Memory | MonitorMetricKind::Swap | MonitorMetricKind::GpuMemory => {
            LucideIcon::MemoryStick
        }
        MonitorMetricKind::Disk => LucideIcon::HardDrive,
        MonitorMetricKind::LoadAverage => LucideIcon::Gauge,
        MonitorMetricKind::Rtt => LucideIcon::Activity,
        MonitorMetricKind::Source => LucideIcon::Info,
    }
}

fn monitor_metric_label_key(kind: MonitorMetricKind) -> &'static str {
    match kind {
        MonitorMetricKind::System => "profiler.panel.system",
        MonitorMetricKind::SystemVersion => "profiler.panel.system_version",
        MonitorMetricKind::Architecture => "profiler.panel.architecture",
        MonitorMetricKind::BootTime => "profiler.panel.boot_time",
        MonitorMetricKind::Uptime => "profiler.panel.uptime",
        MonitorMetricKind::Cpu => "profiler.panel.cpu",
        MonitorMetricKind::Memory => "profiler.panel.memory",
        MonitorMetricKind::Swap => "profiler.panel.swap",
        MonitorMetricKind::Disk => "profiler.panel.disk",
        MonitorMetricKind::Gpu => "profiler.panel.gpu",
        MonitorMetricKind::GpuMemory => "profiler.panel.gpu_memory",
        MonitorMetricKind::LoadAverage => "profiler.panel.load_avg",
        MonitorMetricKind::Rtt => "profiler.panel.rtt",
        MonitorMetricKind::Source => "profiler.panel.source",
    }
}

fn monitor_section_icon(kind: MonitorSectionKind) -> LucideIcon {
    match kind {
        MonitorSectionKind::Mounts => LucideIcon::HardDrive,
        MonitorSectionKind::Gpus => LucideIcon::Cpu,
        MonitorSectionKind::Interfaces => LucideIcon::Wifi,
        MonitorSectionKind::TopProcesses => LucideIcon::Activity,
    }
}

fn monitor_section_label_key(kind: MonitorSectionKind) -> &'static str {
    match kind {
        MonitorSectionKind::Mounts => "profiler.panel.mounts",
        MonitorSectionKind::Gpus => "profiler.panel.gpus",
        MonitorSectionKind::Interfaces => "profiler.panel.interfaces",
        MonitorSectionKind::TopProcesses => "profiler.panel.top_processes",
    }
}

fn compact_monitor_layout_for_width(sidebar_width: f32) -> CompactMonitorLayout {
    // Stack bandwidth values before the narrow sidebar can force labels and
    // rates to paint over each other.
    if sidebar_width <= COMPACT_MONITOR_STACKED_LAYOUT_MAX_WIDTH {
        CompactMonitorLayout::Stacked
    } else {
        CompactMonitorLayout::Inline
    }
}

fn compact_monitor_list_identity(layout: CompactMonitorLayout) -> &'static str {
    // Variable-height list measurements cannot be reused after rows switch
    // between inline and stacked geometry.
    match layout {
        CompactMonitorLayout::Inline => "host-tools-monitor-compact-inline",
        CompactMonitorLayout::Stacked => "host-tools-monitor-compact-stacked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_monitor_stacks_at_the_narrow_width_boundary() {
        assert_eq!(
            compact_monitor_layout_for_width(COMPACT_MONITOR_STACKED_LAYOUT_MAX_WIDTH),
            CompactMonitorLayout::Stacked
        );
        assert_eq!(
            compact_monitor_layout_for_width(COMPACT_MONITOR_STACKED_LAYOUT_MAX_WIDTH + 1.0),
            CompactMonitorLayout::Inline
        );
    }

    #[test]
    fn compact_monitor_uses_twelve_pixel_side_padding_at_every_width() {
        assert_eq!(COMPACT_MONITOR_ROW_SIDE_PADDING, 12.0);
    }

    #[test]
    fn compact_monitor_layouts_use_distinct_list_identities() {
        assert_ne!(
            compact_monitor_list_identity(CompactMonitorLayout::Inline),
            compact_monitor_list_identity(CompactMonitorLayout::Stacked)
        );
    }
}
