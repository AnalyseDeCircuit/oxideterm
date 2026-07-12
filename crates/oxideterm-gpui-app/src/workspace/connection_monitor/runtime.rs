use super::*;

const RUNTIME_CONTENT_PADDING: f32 = 24.0;
const RUNTIME_BACKGROUND_PANEL_ALPHA: u32 = 0x66;
const RUNTIME_BACKGROUND_BORDER_ALPHA: u32 = 0xbf;

impl WorkspaceApp {
    pub(in crate::workspace) fn render_connection_runtime_surface(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("runtime");
        let active_section = self.active_connection_runtime_section;
        let content = match active_section {
            ConnectionRuntimeSection::Overview => self.render_connection_runtime_overview(cx),
            ConnectionRuntimeSection::Health => self.render_connection_runtime_health(cx),
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
                                    .font_weight(gpui::FontWeight::MEDIUM)
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
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(self.render_connection_runtime_section_tab(
                ConnectionRuntimeSection::Overview,
                "sidebar.panels.runtime_overview",
                LucideIcon::LayoutList,
                has_background,
                cx,
            ))
            .child(self.render_connection_runtime_section_tab(
                ConnectionRuntimeSection::Health,
                "sidebar.panels.system_health",
                LucideIcon::Activity,
                has_background,
                cx,
            ))
            .child(self.render_connection_runtime_section_tab(
                ConnectionRuntimeSection::Topology,
                "sidebar.panels.connection_matrix",
                LucideIcon::Network,
                has_background,
                cx,
            ))
            .into_any_element()
    }

    fn render_connection_runtime_section_tab(
        &self,
        section: ConnectionRuntimeSection,
        label_key: &'static str,
        icon: LucideIcon,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_connection_runtime_section == section;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if active {
                oxideterm_gpui_ui::surface::color_for_background(
                    theme.border,
                    has_background,
                    RUNTIME_BACKGROUND_BORDER_ALPHA,
                )
            } else {
                connection_monitor_surface_bg(theme.bg, has_background)
            })
            .bg(if active {
                oxideterm_gpui_ui::surface::color_for_background(
                    theme.bg_panel,
                    has_background,
                    RUNTIME_BACKGROUND_PANEL_ALPHA,
                )
            } else {
                connection_monitor_surface_bg(theme.bg, has_background)
            })
            .px(px(16.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .whitespace_nowrap()
            .cursor_pointer()
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .child(Self::render_lucide_icon(
                icon,
                16.0,
                if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(self.i18n.t(label_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.active_connection_runtime_section = section;
                    this.refresh_connection_monitor_pool_stats();
                    this.sync_connection_monitor_selection(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_connection_runtime_overview(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(stats) = self.connection_monitor.pool_stats.clone() else {
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
                    .gap_4()
                    // Overview keeps aggregate counters only; the detailed
                    // pool, health, and topology widgets own their tabs.
                    .child(self.render_connection_runtime_overview_pool_stats(&stats, cx))
                    .child(self.render_connection_runtime_overview_consumer_stats(&stats, cx))
                    .child(self.render_connection_runtime_overview_summary(&stats, cx)),
            )
            .into_any_element()
    }

    fn render_connection_runtime_overview_pool_stats(
        &self,
        stats: &ConnectionPoolMonitorStats,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .grid()
            .grid_cols(4)
            .gap_3()
            .child(self.render_pool_stat_card(
                self.i18n.t("connections.monitor.active"),
                stats.active_connections,
                LucideIcon::Activity,
                if stats.active_connections > 0 {
                    MONITOR_EMERALD_DARK
                } else {
                    theme.text_muted
                },
                cx,
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
                cx,
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
                cx,
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
                cx,
            ))
            .into_any_element()
    }

    fn render_connection_runtime_overview_consumer_stats(
        &self,
        stats: &ConnectionPoolMonitorStats,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .grid()
            .grid_cols(3)
            .gap_3()
            .child(self.render_pool_stat_card(
                self.i18n.t("connections.monitor.terminals"),
                stats.total_terminals,
                LucideIcon::Terminal,
                if stats.total_terminals > 0 {
                    MONITOR_EMERALD_DARK
                } else {
                    theme.text_muted
                },
                cx,
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
                cx,
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
                cx,
            ))
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
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "connection-runtime-overview",
                        "live",
                        self.i18n.t("connections.monitor.live"),
                        theme.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_connection_runtime_health(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("connection-runtime-health")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .p(px(RUNTIME_CONTENT_PADDING))
                    .child(self.render_system_health_panel(false, cx)),
            )
            .into_any_element()
    }
}
