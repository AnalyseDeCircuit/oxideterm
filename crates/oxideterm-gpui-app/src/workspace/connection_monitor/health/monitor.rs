//! Owns host-monitor presentation inside Host Tools.

use super::*;

use gpui::{Div, StatefulInteractiveElement};
use oxideterm_connection_monitor::{
    CompactMonitorOptions, CompactMonitorRow, MonitorSectionKind, MonitorSparklineKind,
    MonitorUsageKind, ProfilerState, ResourceTrendSample, compact_monitor_row_signature,
    compact_monitor_rows_with_options, format_boot_time, format_compact_system_parts,
    format_compact_uptime, has_cpu_trend_series, has_network_trend_series,
    metrics_source_label_key, rtt_level,
};

#[path = "monitor/charts.rs"]
mod charts;
use charts::{
    render_compact_usage_row, render_cpu_core_pair_row, render_sparkline, usage_value_color,
};

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
    connection_id: String,
    cpu_level: MonitorValueLevel,
    history: Arc<[ResourceTrendSample]>,
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

    pub(in crate::workspace::connection_monitor) fn render_host_monitor_panel(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let render = self.monitor_render_context(cx);
        let monitor_enabled = self.settings_store.settings().host_tools.monitor_enabled;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_monitor_panel(&render, monitor_enabled, cx)
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
            .text_size(px(render.tokens.metrics.ui_text_xs))
            .cursor_pointer()
            .hover(|button| button.bg(rgb(render.tokens.ui.bg_hover)))
            .child(Self::render_monitor_text_with_role(
                render,
                SelectableTextRole::NonSelectable,
                "host-monitor-profiler",
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

    pub(in crate::workspace::connection_monitor) fn render_host_monitor_panel(
        &self,
        render: &MonitorRenderContext,
        monitor_enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connections = self.monitor_connections();
        let Some(active) = self
            .selected_connection_id_owned()
            .and_then(|id| {
                connections
                    .iter()
                    .find(|connection| connection.connection_id == id)
            })
            .or_else(|| connections.first())
        else {
            return host_tools_center_state(
                LucideIcon::WifiOff,
                render.tokens.ui.text_muted,
                render.i18n.t("profiler.panel.no_connection"),
                &render.selectable_text,
                cx,
            );
        };
        let selected_id = active.connection_id.as_str();
        let current = self.profiler_registry().current(selected_id);
        let (metrics, profiler_state) =
            current.as_ref().map_or((None, None), |(metrics, state)| {
                (metrics.as_ref().filter(|_| monitor_enabled), Some(*state))
            });
        let failure = self.profiler_registry().failure(selected_id);
        let failure_key = failure
            .map(|failure| failure.label_key())
            .unwrap_or("profiler.panel.sampling_failed");
        let is_running = matches!(profiler_state, Some(ProfilerState::Running));
        let can_retry_sampling = metrics.is_some_and(|metrics| {
            failure.is_some()
                || matches!(profiler_state, Some(ProfilerState::Degraded))
                || metrics.source == MetricsSource::Unsupported
        });
        let theme = render.tokens.ui;
        let header = div()
            .flex_none()
            .w_full()
            .min_w_0()
            .px_3()
            .pt_3()
            .pb_2()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .child(self.render_connection_switcher(
                &connections,
                selected_id,
                is_running,
                &render.tokens,
                render.mono_font_family.clone(),
                &render.selectable_text,
                cx,
            ))
            .when_some(metrics, |header, metrics| {
                self.render_monitor_header_summary(header, selected_id, metrics, render, cx)
            })
            .when(can_retry_sampling, |header| {
                header.child(
                    div()
                        .px_1()
                        .text_size(px(render.tokens.metrics.ui_text_2xs))
                        .child(Self::render_monitor_text_with_role(
                            render,
                            SelectableTextRole::PlainDocument,
                            "host-monitor-profiler",
                            "sampling-failed",
                            render.i18n.t(failure_key),
                            theme.text_muted,
                            cx,
                        )),
                )
            });
        let body = if !monitor_enabled {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .p_4()
                .child(host_tools_center_state(
                    LucideIcon::Power,
                    theme.text_muted,
                    render.i18n.t("profiler.panel.disabled"),
                    &render.selectable_text,
                    cx,
                ))
                .child(self.render_monitor_enable_control(render, cx))
                .into_any_element()
        } else if let Some(metrics) = metrics {
            self.render_compact_monitor_metrics(
                metrics,
                can_retry_sampling,
                selected_id,
                render,
                cx,
            )
        } else if is_running {
            host_tools_center_state(
                LucideIcon::Activity,
                theme.text_muted,
                render.i18n.t("profiler.panel.sampling"),
                &render.selectable_text,
                cx,
            )
        } else {
            let detecting = self.remote_env_detection_running(selected_id);
            let system_known = self
                .connection_os_type(selected_id)
                .is_some_and(|os| !os.eq_ignore_ascii_case("unknown"));
            div()
                .flex()
                .flex_col()
                .items_center()
                .px_3()
                .child(host_tools_center_state(
                    LucideIcon::Activity,
                    theme.text_muted,
                    render.i18n.t(if detecting {
                        "profiler.panel.detecting_system"
                    } else if system_known {
                        failure_key
                    } else {
                        "profiler.panel.system_unavailable"
                    }),
                    &render.selectable_text,
                    cx,
                ))
                .when(!detecting, |panel| {
                    panel.child(self.render_retry_sampling_button(
                        selected_id.to_owned(),
                        &render.tokens,
                        &render.i18n,
                        cx,
                    ))
                })
                .into_any_element()
        };

        div()
            .id("host-monitor-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(body)
            .into_any_element()
    }

    fn render_monitor_header_summary(
        &self,
        header: Div,
        selected_id: &str,
        metrics: &ResourceMetrics,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> Div {
        let tokens = render.tokens;
        let theme = tokens.ui;
        let info = metrics.system_info.as_ref();
        let left = if let Some(uptime) = info.and_then(|info| info.uptime_seconds) {
            let uptime_row = div()
                .min_w_0()
                .flex_1()
                .flex()
                .items_center()
                .gap_1()
                .text_color(rgb(theme.text_muted))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(render.i18n.t("profiler.panel.uptime")),
                )
                .child(
                    div()
                        .flex_none()
                        .font_family(render.mono_font_family.clone())
                        .child(format_compact_uptime(uptime)),
                );
            if let Some(boot_time) = info
                .and_then(|info| info.boot_time_ms)
                .and_then(format_boot_time)
            {
                let boot_label = render.i18n.t("profiler.panel.boot_time");
                uptime_row
                    .id(host_tools_row_id(
                        "host-monitor-uptime",
                        selected_id,
                        "uptime",
                    ))
                    .tooltip(move |_window, cx| {
                        oxideterm_gpui_ui::tooltip::tooltip_view(
                            tokens,
                            format!("{boot_label}: {boot_time}"),
                            None,
                            cx,
                        )
                    })
                    .into_any_element()
            } else {
                uptime_row.into_any_element()
            }
        } else {
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_color(rgb(theme.text_muted))
                .when(metrics.source != MetricsSource::Full, |row| {
                    row.child(monitor_source_label(&render.i18n, metrics.source))
                })
                .into_any_element()
        };
        let has_left = info.and_then(|info| info.uptime_seconds).is_some()
            || metrics.source != MetricsSource::Full;
        let (os_label, arch_label) = info
            .map(format_compact_system_parts)
            .unwrap_or((None, None));
        header
            .when(os_label.is_some() || arch_label.is_some(), |header| {
                header.child(
                    div()
                        .px_1()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .when_some(os_label, |row, os| {
                            row.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(tokens.metrics.ui_text_xs))
                                    .child(Self::render_monitor_text_with_role(
                                        render,
                                        SelectableTextRole::PlainDocument,
                                        "host-monitor-system",
                                        selected_id,
                                        os,
                                        theme.text_secondary,
                                        cx,
                                    )),
                            )
                        })
                        .when_some(arch_label, |row, arch| {
                            row.child(
                                div()
                                    .flex_none()
                                    .font_family(render.mono_font_family.clone())
                                    .text_size(px(tokens.metrics.ui_text_2xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(arch),
                            )
                        }),
                )
            })
            .when(has_left || metrics.ssh_rtt_ms.is_some(), |header| {
                header.child(
                    div()
                        .px_1()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .text_size(px(tokens.metrics.ui_text_2xs))
                        .child(left)
                        .when_some(metrics.ssh_rtt_ms, |row, ms| {
                            row.child(
                                div()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_color(rgb(theme.text_muted))
                                            .child(render.i18n.t("profiler.panel.rtt")),
                                    )
                                    .child(
                                        div()
                                            .font_family(render.mono_font_family.clone())
                                            .text_color(usage_value_color(
                                                rtt_level(Some(ms)),
                                                &theme,
                                            ))
                                            .child(format!("{ms} ms")),
                                    ),
                            )
                        }),
                )
            })
    }

    fn render_retry_sampling_button(
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
            .text_size(px(tokens.metrics.ui_text_xs))
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

    fn render_compact_monitor_metrics(
        &self,
        metrics: &ResourceMetrics,
        can_retry_sampling: bool,
        connection_id: &str,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let history: Arc<[ResourceTrendSample]> =
            self.profiler_registry().trend_history(connection_id).into();
        let options = CompactMonitorOptions {
            cpu_cores_expanded: self.monitor_cpu_cores_expanded(connection_id),
            show_cpu_sparkline: has_cpu_trend_series(&history),
            show_network_sparkline: has_network_trend_series(&history),
        };
        let rows = compact_monitor_rows_with_options(
            metrics,
            options,
            can_retry_sampling.then(|| connection_id.to_owned()),
        );
        let layout = compact_monitor_layout_for_width(render.sidebar_width);
        let signatures = rows
            .iter()
            .map(compact_monitor_row_signature)
            .collect::<Vec<_>>();
        self.sync_compact_monitor_list_signatures(
            &compact_monitor_list_identity(connection_id, layout),
            &signatures,
        );
        if rows.is_empty() {
            return div()
                .flex_1()
                .min_h_0()
                .p_3()
                .flex()
                .items_center()
                .justify_center()
                .text_align(gpui::TextAlign::Center)
                .text_size(px(render.tokens.metrics.ui_text_xs))
                .text_color(rgb(render.tokens.ui.text_muted))
                .child(monitor_source_label(&render.i18n, metrics.source))
                .into_any_element();
        }
        let row_render = CompactMonitorRenderContext {
            tokens: render.tokens,
            i18n: render.i18n.clone(),
            mono_font_family: render.mono_font_family.clone(),
            connection_id: connection_id.to_owned(),
            cpu_level: percent_level(metrics.cpu_percent.filter(|value| value.is_finite())),
            history,
        };
        let state = self.compact_monitor_list_state();
        let spec = TauriVirtualListSpec::new(
            px(COMPACT_MONITOR_LIST_ESTIMATED_ROW_HEIGHT),
            COMPACT_MONITOR_LIST_OVERSCAN,
        );
        let host_tools = cx.entity();
        div()
            .id("host-tools-monitor-metrics-scroll")
            .w_full()
            .flex_1()
            .min_h_0()
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    host_tools.update(cx, |host_tools, cx| {
                        rows.get(index).map_or_else(
                            || div().into_any_element(),
                            |row| {
                                host_tools.render_compact_monitor_virtual_row(
                                    row,
                                    layout,
                                    &row_render,
                                    cx,
                                )
                            },
                        )
                    })
                },
            ))
            .into_any_element()
    }

    fn render_compact_monitor_virtual_row(
        &self,
        row: &CompactMonitorRow,
        layout: CompactMonitorLayout,
        render: &CompactMonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = render.tokens;
        let theme = tokens.ui;
        match row {
            CompactMonitorRow::Usage {
                kind,
                detail,
                percent,
                level,
            } => {
                let row =
                    render_compact_usage_row(kind, detail.as_deref(), *percent, *level, render);
                if matches!(
                    kind,
                    MonitorUsageKind::Cpu {
                        expandable: true,
                        ..
                    }
                ) {
                    let connection_id = render.connection_id.clone();
                    row.id(host_tools_row_id(
                        "host-monitor-cpu-row",
                        &connection_id,
                        "cpu",
                    ))
                    .cursor_pointer()
                    .hover(move |row| row.bg(rgb(theme.bg_hover)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |host_tools, _event, _window, cx| {
                            host_tools.toggle_monitor_cpu_cores(connection_id.clone(), cx);
                            cx.stop_propagation();
                        }),
                    )
                    .into_any_element()
                } else {
                    row.into_any_element()
                }
            }
            CompactMonitorRow::CpuCorePair { left, right } => {
                render_cpu_core_pair_row(left, right.as_ref(), render).into_any_element()
            }
            CompactMonitorRow::LoadAverage { value } => {
                compact_row(COMPACT_MONITOR_METRIC_ROW_HEIGHT)
                    .justify_between()
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(theme.text_muted))
                            .child(render.i18n.t("profiler.panel.load_avg")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_family(render.mono_font_family.clone())
                            .text_color(rgb(theme.text_secondary))
                            .child(value.clone()),
                    )
                    .into_any_element()
            }
            CompactMonitorRow::Sparkline { kind } => {
                render_sparkline(render, *kind == MonitorSparklineKind::Cpu).into_any_element()
            }
            CompactMonitorRow::SectionHeader { kind, disk_io } => {
                compact_row(COMPACT_MONITOR_SECTION_ROW_HEIGHT)
                    .justify_between()
                    .bg(rgba((theme.bg_sunken << 8) | 0x45))
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(compact_section_label(
                        monitor_section_label_key(*kind),
                        render,
                    ))
                    .when_some(disk_io.as_ref(), |row, meta| {
                        row.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_family(render.mono_font_family.clone())
                                .text_size(px(tokens.metrics.ui_text_2xs))
                                .text_color(rgb(if meta.active {
                                    theme.text_secondary
                                } else {
                                    theme.text_muted
                                }))
                                .child(format!("R {} · W {}", meta.read, meta.write)),
                        )
                    })
                    .into_any_element()
            }
            CompactMonitorRow::NetworkSummary {
                rx,
                tx,
                rx_active,
                tx_active,
            } => {
                let rates =
                    render_bandwidth(rx.as_deref(), tx.as_deref(), *rx_active, *tx_active, render)
                        .text_size(px(tokens.metrics.ui_text_xs));
                render_bandwidth_row(
                    compact_section_label("profiler.panel.network", render),
                    rates,
                    COMPACT_MONITOR_NETWORK_SUMMARY_ROW_HEIGHT,
                    layout,
                )
                .bg(rgba((theme.bg_sunken << 8) | 0x45))
                .border_t_1()
                .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
                .into_any_element()
            }
            CompactMonitorRow::Interface {
                name,
                rx,
                tx,
                rx_active,
                tx_active,
            } => {
                let label = div()
                    .min_w_0()
                    .truncate()
                    .font_family(render.mono_font_family.clone())
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_secondary))
                    .child(name.clone());
                let rates = render_bandwidth(Some(rx), Some(tx), *rx_active, *tx_active, render)
                    .text_size(px(tokens.metrics.ui_text_2xs));
                render_bandwidth_row(label, rates, COMPACT_MONITOR_INTERFACE_ROW_HEIGHT, layout)
                    .into_any_element()
            }
            CompactMonitorRow::Process {
                pid,
                command,
                memory_percent,
                level,
            } => compact_row(COMPACT_MONITOR_PROCESS_ROW_HEIGHT)
                .text_size(px(tokens.metrics.ui_text_xs))
                .justify_between()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.text))
                        .child(command.clone()),
                )
                .child(
                    div()
                        .w(px(62.0))
                        .flex_none()
                        .truncate()
                        .text_align(gpui::TextAlign::Right)
                        .font_family(render.mono_font_family.clone())
                        .text_size(px(tokens.metrics.ui_text_2xs))
                        .text_color(rgb(theme.text_muted))
                        .child(format!("#{pid}")),
                )
                .child(
                    div()
                        .w(px(40.0))
                        .flex_none()
                        .text_align(gpui::TextAlign::Right)
                        .font_family(render.mono_font_family.clone())
                        .text_color(usage_value_color(*level, &theme))
                        .child(memory_percent.clone()),
                )
                .into_any_element(),
            CompactMonitorRow::Retry { connection_id } => {
                compact_row(COMPACT_MONITOR_RETRY_ROW_HEIGHT)
                    .child(self.render_retry_sampling_button(
                        connection_id.clone(),
                        &render.tokens,
                        &render.i18n,
                        cx,
                    ))
                    .into_any_element()
            }
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
}

fn compact_row(height: f32) -> Div {
    div()
        .w_full()
        .min_w_0()
        .h(px(height))
        .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
        .flex()
        .items_center()
        .gap_2()
}

fn compact_section_label(label: &str, render: &CompactMonitorRenderContext) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_size(px(render.tokens.metrics.ui_text_2xs))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(render.tokens.ui.text_secondary))
        .child(render.i18n.t(label))
}

fn render_bandwidth(
    rx: Option<&str>,
    tx: Option<&str>,
    rx_active: bool,
    tx_active: bool,
    render: &CompactMonitorRenderContext,
) -> Div {
    let theme = render.tokens.ui;
    let mut values = div()
        .min_w_0()
        .flex()
        .items_center()
        .gap_2()
        .font_family(render.mono_font_family.clone())
        .text_color(rgb(theme.text_muted));
    for (arrow, value, active) in [('↓', rx, rx_active), ('↑', tx, tx_active)] {
        if let Some(value) = value {
            values = values.child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                    .child(format!("{arrow} {value}")),
            );
        }
    }
    values.when(rx.is_none() && tx.is_none(), |values| values.child("—"))
}

fn render_bandwidth_row(label: Div, rates: Div, height: f32, layout: CompactMonitorLayout) -> Div {
    match layout {
        CompactMonitorLayout::Inline => compact_row(height)
            .justify_between()
            .child(label.flex_1())
            .child(rates),
        CompactMonitorLayout::Stacked => compact_row(COMPACT_MONITOR_STACKED_ROW_HEIGHT)
            .flex_col()
            .items_start()
            .justify_center()
            .gap_1()
            .child(label.w_full())
            .child(rates.w_full()),
    }
}

fn monitor_source_label(i18n: &I18n, source: MetricsSource) -> String {
    format!(
        "{}: {}",
        i18n.t("profiler.panel.source"),
        i18n.t(metrics_source_label_key(source))
    )
}

fn monitor_section_label_key(kind: MonitorSectionKind) -> &'static str {
    match kind {
        MonitorSectionKind::CpuCores => "profiler.detail.cpu_cores",
        MonitorSectionKind::Mounts => "profiler.panel.mounts",
        MonitorSectionKind::Gpus => "profiler.panel.gpus",
        MonitorSectionKind::Interfaces => "profiler.panel.interfaces",
        MonitorSectionKind::TopProcesses => "profiler.panel.top_processes",
    }
}

fn compact_monitor_layout_for_width(sidebar_width: f32) -> CompactMonitorLayout {
    if sidebar_width <= COMPACT_MONITOR_STACKED_LAYOUT_MAX_WIDTH {
        CompactMonitorLayout::Stacked
    } else {
        CompactMonitorLayout::Inline
    }
}

fn compact_monitor_list_identity(connection_id: &str, layout: CompactMonitorLayout) -> String {
    let mode = match layout {
        CompactMonitorLayout::Inline => "inline",
        CompactMonitorLayout::Stacked => "stacked",
    };
    format!("host-tools-monitor:{connection_id}:{mode}")
}
