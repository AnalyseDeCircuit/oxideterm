// Hallmark · pre-emit critique: P4 H5 E5 S5 R5 V5
// Hallmark · macrostructure: Asymmetric Operations Bento · genre: modern-minimal · tone: technical and restrained
use super::*;

const RUNTIME_CONTENT_PADDING: f32 = 20.0;
const RUNTIME_PRIMARY_PANEL_MIN_WIDTH: f32 = 440.0;
const RUNTIME_SECONDARY_PANEL_MIN_WIDTH: f32 = 260.0;
const RUNTIME_PRIMARY_PANEL_WIDTH_RATIO: f32 = 0.64;
const RUNTIME_SECONDARY_PANEL_WIDTH_RATIO: f32 = 0.32;
const RUNTIME_ATTENTION_ROW_LIMIT: usize = 5;
const RUNTIME_TAB_BAR_WIDTH: f32 = 320.0; // Two equal header tabs keep localized runtime labels readable.

fn connection_runtime_section_index(section: ConnectionRuntimeSection) -> usize {
    match section {
        ConnectionRuntimeSection::Overview => 0,
        ConnectionRuntimeSection::Topology => 1,
    }
}

fn connection_runtime_state_needs_attention(state: &ConnectionPoolEntryState) -> bool {
    matches!(
        state,
        ConnectionPoolEntryState::Reconnecting
            | ConnectionPoolEntryState::LinkDown
            | ConnectionPoolEntryState::Error(_)
    )
}

fn connection_runtime_attention_rank(state: &ConnectionPoolEntryState) -> usize {
    match state {
        ConnectionPoolEntryState::LinkDown | ConnectionPoolEntryState::Error(_) => 0,
        ConnectionPoolEntryState::Reconnecting => 1,
        _ => 2,
    }
}

fn connection_runtime_pool_usage(total: usize, capacity: usize) -> f32 {
    if capacity == 0 {
        0.0
    } else {
        (total as f32 / capacity as f32).clamp(0.0, 1.0)
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_connection_runtime_surface(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("runtime");
        let active_section = self.host_tools.read(cx).active_runtime_section;
        let content = match active_section {
            ConnectionRuntimeSection::Overview => self.render_connection_runtime_overview(cx),
            ConnectionRuntimeSection::Topology => self.render_connection_runtime_topology(cx),
        };
        let content = oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            SharedString::from(format!("runtime-page-{active_section:?}")),
            div()
                .w_full()
                .min_w_0()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(content),
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            // Tauri makes page roots transparent when TabBgWrapper is active.
            .bg(connection_monitor_surface_bg(theme.bg, has_background))
            .text_color(rgb(theme.text))
            .child(self.render_connection_runtime_header(has_background, cx))
            .child(content)
            .into_any_element()
    }

    fn render_connection_runtime_header(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_page_gap))
            .px(px(self.tokens.metrics.settings_content_padding))
            .pt(px(self.tokens.metrics.settings_content_padding))
            .pb(px(self.tokens.metrics.settings_page_gap))
            // Preserve Tauri's background-image contract: the shared image layer
            // sits behind the tab while ordinary header content stays readable.
            .bg(connection_monitor_surface_bg(theme.bg, has_background))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .min_w(px(280.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_2xl))
                                    // Match the shared manager-page title hierarchy.
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "connection-runtime-header",
                                        "title",
                                        self.i18n.t("sidebar.panels.runtime"),
                                        theme.text_heading,
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_base))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "connection-runtime-header",
                                        "description",
                                        self.i18n.t("sidebar.panels.runtime_description"),
                                        theme.text_muted,
                                        cx,
                                    )),
                            ),
                    )
                    .child(self.render_connection_runtime_section_tabs(has_background, cx)),
            )
            .child(div().w_full().h(px(1.0)).bg(rgb(theme.border)))
            .into_any_element()
    }

    fn render_connection_runtime_section_tabs(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = vec![
            self.render_connection_runtime_section_tab(
                ConnectionRuntimeSection::Overview,
                "sidebar.panels.runtime_overview",
                LucideIcon::LayoutList,
                cx,
            ),
            self.render_connection_runtime_section_tab(
                ConnectionRuntimeSection::Topology,
                "sidebar.panels.connection_matrix",
                LucideIcon::Network,
                cx,
            ),
        ];
        let host_tools = self.host_tools.read(cx);
        let active_index = connection_runtime_section_index(host_tools.active_runtime_section);
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            selection_motion::CONNECTION_RUNTIME_SWITCHER_ID,
            oxideterm_gpui_ui::SegmentedControlOptions::new(
                active_index,
                connection_runtime_section_index(host_tools.previous_runtime_section),
                2,
            )
            .user_transition_active(self.segmented_control_user_transition_active(
                selection_motion::CONNECTION_RUNTIME_SWITCHER_ID,
                active_index,
            ))
            .has_background_image(has_background)
            .compact(RUNTIME_TAB_BAR_WIDTH),
            items,
        )
        .into_any_element()
    }

    fn render_connection_runtime_section_tab(
        &self,
        section: ConnectionRuntimeSection,
        label_key: &'static str,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.host_tools.read(cx).active_runtime_section == section;
        let content = div()
            .w_full()
            .py(px(2.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .child(Self::render_lucide_icon(
                icon,
                16.0,
                if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(self.i18n.t(label_key));
        oxideterm_gpui_ui::segmented_control_item_content(
            &self.tokens,
            active,
            content.into_any_element(),
        )
        .font_weight(gpui::FontWeight::MEDIUM)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                if this.host_tools.read(cx).active_runtime_section != section {
                    this.set_connection_runtime_section(section, cx);
                    this.begin_user_segmented_control_transition(
                        selection_motion::CONNECTION_RUNTIME_SWITCHER_ID,
                        connection_runtime_section_index(section),
                        cx,
                    );
                }
                this.sync_host_tools_lifecycle(true, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_connection_runtime_overview(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(stats) = self.host_tools.read(cx).pool_stats_snapshot() else {
            return div()
                .id("connection-runtime-overview")
                .flex_1()
                .min_h_0()
                .child(monitor_center_state(
                    self,
                    LucideIcon::RefreshCw,
                    theme.text_muted,
                    self.i18n.t("connections.monitor.loading"),
                    cx,
                ))
                .into_any_element();
        };

        let mut attention_connections = self
            .host_tools
            .read(cx)
            .pool_summaries_snapshot()
            .into_iter()
            .filter(|summary| connection_runtime_state_needs_attention(&summary.state))
            .collect::<Vec<_>>();
        attention_connections.sort_by_key(|summary| {
            (
                connection_runtime_attention_rank(&summary.state),
                summary.host.clone(),
                summary.port,
            )
        });

        div()
            .id("connection-runtime-overview")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .p(px(RUNTIME_CONTENT_PADDING))
                    .flex()
                    .flex_col()
                    .gap_3()
                    // Zero connections is a valid monitor sample, not a
                    // separate onboarding state. Keep the dashboard stable so
                    // its capacity and health remain visible at all times.
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_wrap()
                            .items_stretch()
                            .gap_3()
                            .child(self.render_connection_runtime_pool_panel(&stats, cx))
                            .child(self.render_connection_runtime_consumer_panel(&stats, cx)),
                    )
                    .child(self.render_connection_runtime_attention_panel(&attention_connections))
                    .child(self.render_connection_runtime_overview_summary(&stats, cx)),
            )
            .into_any_element()
    }

    fn connection_runtime_panel(&self, has_background: bool) -> gpui::Div {
        // Runtime cards use the same background-aware surface contract as
        // Plugin Manager cards, including theme radius, opacity, and shadow.
        oxideterm_gpui_ui::semantic_surface(
            &self.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::Inspector)
                .padding(oxideterm_gpui_ui::SurfacePadding::None)
                .has_background_image(has_background),
        )
    }

    fn render_connection_runtime_panel_heading(
        &self,
        icon: LucideIcon,
        label: String,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(theme.text))
            .child(Self::render_lucide_icon(icon, 15.0, rgb(theme.text_muted)))
            .child(label)
    }

    fn render_connection_runtime_pool_panel(
        &self,
        stats: &ConnectionPoolMonitorStats,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("runtime");
        let pool_capacity = stats.pool_capacity;
        let pool_usage = connection_runtime_pool_usage(stats.total_connections, pool_capacity);
        let timeout_minutes = stats.idle_timeout_secs.saturating_add(59) / 60;
        let timeout_label = if stats.idle_timeout_secs == 0 {
            self.i18n.t("connections.monitor.idle_timeout_never")
        } else {
            self.i18n
                .t("connections.monitor.idle_timeout")
                .replace("{{min}}", &timeout_minutes.to_string())
        };

        self.connection_runtime_panel(has_background)
            .w(relative(RUNTIME_PRIMARY_PANEL_WIDTH_RATIO))
            .min_w(px(RUNTIME_PRIMARY_PANEL_MIN_WIDTH))
            .flex_grow_1()
            .p_4()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .child(self.render_connection_runtime_panel_heading(
                        LucideIcon::Gauge,
                        self.i18n.t("connections.monitor.overview_pool_usage"),
                    ))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(timeout_label),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(
                        oxideterm_gpui_ui::monospace_datum(
                            &self.tokens,
                            stats.total_connections.to_string(),
                            Some(settings_mono_font_family(self.settings_store.settings())),
                            oxideterm_gpui_ui::MonospaceDatumOptions::new(
                                oxideterm_gpui_ui::MonospaceDatumTone::Primary,
                            )
                            .text_size(30.0)
                            .strong(),
                        )
                        .text_color(rgb(theme.text_heading)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                self.i18n
                                    .t("connections.monitor.capacity")
                                    .replace("{{capacity}}", &pool_capacity.to_string()),
                            ),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .h(px(6.0))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(rgb(theme.bg_hover))
                    .child(
                        div()
                            .h_full()
                            .w(relative(pool_usage))
                            .rounded_full()
                            .bg(rgb(theme.accent)),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .pt_3()
                    .border_t_1()
                    .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(self.render_connection_runtime_metric(
                        self.i18n.t("connections.monitor.active"),
                        stats.active_connections,
                        LucideIcon::Activity,
                        if stats.active_connections > 0 {
                            MONITOR_EMERALD_DARK
                        } else {
                            theme.text_muted
                        },
                    ))
                    .child(self.render_connection_runtime_metric(
                        self.i18n.t("connections.monitor.idle"),
                        stats.idle_connections,
                        LucideIcon::Link2,
                        if stats.idle_connections > 0 {
                            MONITOR_BLUE
                        } else {
                            theme.text_muted
                        },
                    ))
                    .child(self.render_connection_runtime_metric(
                        self.i18n.t("connections.monitor.reconnecting"),
                        stats.reconnecting_connections,
                        LucideIcon::RefreshCw,
                        if stats.reconnecting_connections > 0 {
                            MONITOR_AMBER
                        } else {
                            theme.text_muted
                        },
                    ))
                    .child(self.render_connection_runtime_metric(
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
            .into_any_element()
    }

    fn render_connection_runtime_metric(
        &self,
        label: String,
        value: usize,
        icon: LucideIcon,
        color: u32,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        div()
            .min_w(px(104.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(icon, 12.0, rgb(color)))
                    .child(label),
            )
            .child(
                oxideterm_gpui_ui::monospace_datum(
                    &self.tokens,
                    value.to_string(),
                    Some(settings_mono_font_family(self.settings_store.settings())),
                    oxideterm_gpui_ui::MonospaceDatumOptions::new(
                        oxideterm_gpui_ui::MonospaceDatumTone::Primary,
                    )
                    .text_size(20.0)
                    .strong(),
                )
                .text_color(rgb(color)),
            )
    }

    fn render_connection_runtime_consumer_panel(
        &self,
        stats: &ConnectionPoolMonitorStats,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_background = self.background_surface_active("runtime");
        self.connection_runtime_panel(has_background)
            .w(relative(RUNTIME_SECONDARY_PANEL_WIDTH_RATIO))
            .min_w(px(RUNTIME_SECONDARY_PANEL_MIN_WIDTH))
            .flex_grow_1()
            .p_4()
            .flex()
            .flex_col()
            .child(self.render_connection_runtime_panel_heading(
                LucideIcon::AppWindow,
                self.i18n.t("connections.monitor.overview_consumers"),
            ))
            .child(self.render_connection_runtime_consumer_row(
                self.i18n.t("connections.monitor.terminals"),
                stats.total_terminals,
                LucideIcon::Terminal,
                MONITOR_EMERALD_DARK,
                false,
            ))
            .child(self.render_connection_runtime_consumer_row(
                self.i18n.t("connections.monitor.sftp"),
                stats.total_sftp_sessions,
                LucideIcon::FolderSync,
                MONITOR_BLUE,
                true,
            ))
            .child(self.render_connection_runtime_consumer_row(
                self.i18n.t("connections.monitor.forwards"),
                stats.total_forwards,
                LucideIcon::ArrowLeftRight,
                MONITOR_BLUE,
                true,
            ))
            .into_any_element()
    }

    fn render_connection_runtime_consumer_row(
        &self,
        label: String,
        value: usize,
        icon: LucideIcon,
        color: u32,
        bordered: bool,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        div()
            .mt_2()
            .pt_2()
            .when(bordered, |row| {
                row.border_t_1()
                    .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            })
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(icon, 14.0, rgb(color)))
                    .child(label),
            )
            .child(
                oxideterm_gpui_ui::monospace_datum(
                    &self.tokens,
                    value.to_string(),
                    Some(settings_mono_font_family(self.settings_store.settings())),
                    oxideterm_gpui_ui::MonospaceDatumOptions::new(
                        oxideterm_gpui_ui::MonospaceDatumTone::Primary,
                    )
                    .text_size(17.0)
                    .strong(),
                )
                .flex_none(),
            )
    }

    fn render_connection_runtime_attention_panel(
        &self,
        attention_connections: &[ConnectionPoolEntrySummary],
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let count = attention_connections.len();
        let has_background = self.background_surface_active("runtime");
        let mut panel = self
            .connection_runtime_panel(has_background)
            .w_full()
            .p_4()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(self.render_connection_runtime_panel_heading(
                        LucideIcon::ShieldAlert,
                        self.i18n.t("connections.monitor.overview_attention"),
                    ))
                    .child(
                        oxideterm_gpui_ui::monospace_datum(
                            &self.tokens,
                            count.to_string(),
                            Some(settings_mono_font_family(self.settings_store.settings())),
                            oxideterm_gpui_ui::MonospaceDatumOptions::new(if count == 0 {
                                oxideterm_gpui_ui::MonospaceDatumTone::Muted
                            } else {
                                oxideterm_gpui_ui::MonospaceDatumTone::Error
                            })
                            .text_size(13.0)
                            .strong(),
                        )
                        .flex_none(),
                    ),
            );

        if attention_connections.is_empty() {
            return panel
                .child(
                    div()
                        .mt_3()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.text_muted))
                        .child(Self::render_lucide_icon(
                            LucideIcon::CheckCircle,
                            15.0,
                            rgb(MONITOR_EMERALD_DARK),
                        ))
                        .child(self.i18n.t("connections.monitor.overview_healthy")),
                )
                .into_any_element();
        }

        for summary in attention_connections
            .iter()
            .take(RUNTIME_ATTENTION_ROW_LIMIT)
        {
            panel = panel.child(self.render_connection_runtime_attention_row(summary));
        }
        if count > RUNTIME_ATTENTION_ROW_LIMIT {
            panel = panel.child(
                div()
                    .pt_2()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(format!("+{}", count - RUNTIME_ATTENTION_ROW_LIMIT)),
            );
        }
        panel.into_any_element()
    }

    fn render_connection_runtime_attention_row(
        &self,
        summary: &ConnectionPoolEntrySummary,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (icon, color, status_label) = match &summary.state {
            ConnectionPoolEntryState::Reconnecting => (
                LucideIcon::RefreshCw,
                MONITOR_AMBER,
                self.i18n.t("connections.monitor.reconnecting"),
            ),
            ConnectionPoolEntryState::LinkDown => (
                LucideIcon::AlertTriangle,
                MONITOR_RED,
                self.i18n.t("connections.monitor.link_down"),
            ),
            ConnectionPoolEntryState::Error(_) => (
                LucideIcon::AlertCircle,
                MONITOR_RED,
                self.i18n.t("connections.monitor.overview_error"),
            ),
            _ => return div().into_any_element(),
        };
        let identity = format!("{}@{}:{}", summary.username, summary.host, summary.port);
        let usage = format!(
            "{}  ·  {}  ·  {}",
            self.i18n
                .t("connections.panel.terminals")
                .replace("{{count}}", &summary.terminal_count.to_string()),
            self.i18n.t("connections.panel.sftp").replace(
                "{{count}}",
                &usize::from(summary.has_sftp_session).to_string()
            ),
            self.i18n
                .t("connections.panel.forwards")
                .replace("{{count}}", &summary.forward_count.to_string()),
        );

        div()
            .mt_2()
            .pt_2()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .flex()
            .items_center()
            .gap_3()
            .child(Self::render_lucide_icon(icon, 15.0, rgb(color)))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        oxideterm_gpui_ui::monospace_datum(
                            &self.tokens,
                            identity,
                            Some(settings_mono_font_family(self.settings_store.settings())),
                            oxideterm_gpui_ui::MonospaceDatumOptions::new(
                                oxideterm_gpui_ui::MonospaceDatumTone::Primary,
                            )
                            .text_size(12.0)
                            .strong(),
                        )
                        .w_full(),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(usage),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(color))
                    .child(status_label),
            )
            .into_any_element()
    }

    fn render_connection_runtime_overview_summary(
        &self,
        stats: &ConnectionPoolMonitorStats,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
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
                    .child(
                        self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "connection-runtime-overview",
                            "live",
                            self.i18n
                                .t("connections.monitor.overview_refresh_interval")
                                .replace(
                                    "{{seconds}}",
                                    &MONITOR_POOL_REFRESH_INTERVAL.as_secs().to_string(),
                                ),
                            theme.text_muted,
                            cx,
                        ),
                    ),
            )
            .into_any_element()
    }
}
