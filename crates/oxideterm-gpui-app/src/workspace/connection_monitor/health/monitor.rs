//! Owns host-monitor presentation inside Host Tools.

use super::*;

use oxideterm_connection_monitor::ProfilerState;

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
            .text_size(px(12.0))
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
        let enable_control = self.render_monitor_enable_control(render, cx);
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
                            "host-monitor-empty",
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
        let current = self
            .profiler_registry()
            .current(&active_connection.connection_id);
        let disabled = !monitor_enabled;
        let profiler_state = current.as_ref().map(|(_, state)| *state);
        let is_running = matches!(profiler_state, Some(ProfilerState::Running));
        let metrics = current.as_ref().and_then(|(metrics, _)| metrics.as_ref());
        let panel = div()
            .relative()
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .min_h_0()
            .child(self.render_monitor_panel_header(
                &connections,
                selected_id,
                is_running,
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
                                "host-monitor-profiler",
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
                                "host-monitor-profiler",
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
                                "host-monitor-profiler",
                                "no-data",
                                render.i18n.t("profiler.panel.no_data"),
                                render.tokens.ui.text_muted,
                                cx,
                            ),
                        )),
                )
                .into_any_element();
        };

        let can_retry_sampling = !disabled
            && (matches!(profiler_state, Some(ProfilerState::Degraded))
                || matches!(metrics.source, MetricsSource::Unsupported));
        panel
            .child(
                div()
                    .id("host-tools-monitor-metrics-scroll")
                    .flex_1()
                    .min_h_0()
                    .child(self.render_compact_monitor_metrics(
                        metrics,
                        can_retry_sampling,
                        active_connection.connection_id.clone(),
                        render,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_monitor_panel_header(
        &self,
        connections: &[MonitorConnectionOption],
        selected_id: &str,
        is_running: bool,
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

    fn render_compact_monitor_metrics(
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
    fn compact_monitor_layouts_use_distinct_list_identities() {
        assert_ne!(
            compact_monitor_list_identity(CompactMonitorLayout::Inline),
            compact_monitor_list_identity(CompactMonitorLayout::Stacked)
        );
    }
}
