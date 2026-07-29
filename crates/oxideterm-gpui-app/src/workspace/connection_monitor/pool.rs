use super::*;

use super::health::MonitorRenderContext;
use crate::workspace::selectable_text::selectable_document_group_id;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ConnectionMonitorSection {
    Pool,
    Health,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn render_connection_monitor_surface(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_background = self.background_surface_active("connection_monitor");
        let render = Arc::new(self.monitor_render_context(cx));
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_connection_monitor_surface(render, has_background, cx)
        })
    }
}

impl HostToolsEntity {
    fn render_connection_monitor_surface(
        &mut self,
        render: Arc<MonitorRenderContext>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_connection_monitor_section_list_state();
        let state = self.section_list_state.clone();
        let host_tools = cx.entity();
        let spec = Self::connection_monitor_section_list_spec();
        let theme = render.tokens.ui;
        div()
            .id("connection-monitor-scroll")
            .size_full()
            .bg(connection_monitor_surface_bg(theme.bg, has_background))
            .text_color(rgb(theme.text))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    host_tools.update(cx, |host_tools, cx| {
                        host_tools.render_connection_monitor_section_item(index, &render, cx)
                    })
                },
            ))
            .into_any_element()
    }

    fn sync_connection_monitor_section_list_state(&self) {
        let spec = Self::connection_monitor_section_list_spec();
        let signatures = [
            self.connection_monitor_section_signature(ConnectionMonitorSection::Pool),
            self.connection_monitor_section_signature(ConnectionMonitorSection::Health),
        ];
        sync_tauri_variable_list_state_by_signatures(
            &self.section_list_state,
            &mut self.section_list_cache.borrow_mut(),
            "connection-monitor",
            &signatures,
            spec,
        );
    }

    fn connection_monitor_section_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CONNECTION_MONITOR_SECTION_LIST_ESTIMATED_HEIGHT),
            CONNECTION_MONITOR_SECTION_LIST_OVERSCAN,
        )
    }

    fn connection_monitor_section_signature(&self, section: ConnectionMonitorSection) -> u64 {
        let mut hasher = DefaultHasher::new();
        // Loading and profiler state affect row height, so they must invalidate
        // the variable-list measurement without consulting the workspace.
        section.hash(&mut hasher);
        self.pool_error().is_some().hash(&mut hasher);
        self.pool_stats_snapshot().is_some().hash(&mut hasher);
        self.pool_summary_count().hash(&mut hasher);
        if matches!(section, ConnectionMonitorSection::Health) {
            self.selected_connection_id().hash(&mut hasher);
            self.monitoring.monitor_enabled.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn render_connection_monitor_section_item(
        &self,
        index: usize,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let section = match index {
            0 => ConnectionMonitorSection::Pool,
            1 => ConnectionMonitorSection::Health,
            _ => return div().into_any_element(),
        };
        div()
            // Page padding stays local while the virtual list owns scrolling.
            .w_full()
            .min_w(px(0.0))
            .px(px(MONITOR_PAGE_PADDING))
            .pb(px(MONITOR_SECTION_GAP))
            .when(index == 0, |item| item.pt(px(MONITOR_PAGE_PADDING)))
            .when(
                index + 1 == CONNECTION_MONITOR_SECTION_LIST_ITEM_COUNT,
                |item| item.pb(px(MONITOR_PAGE_PADDING)),
            )
            .child(self.render_connection_monitor_section(section, render, cx))
            .into_any_element()
    }

    fn render_connection_monitor_section(
        &self,
        section: ConnectionMonitorSection,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = render.tokens.ui;
        match section {
            ConnectionMonitorSection::Pool => div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .mb_6()
                        .text_size(px(24.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(theme.text))
                        .child(Self::render_pool_text(
                            render,
                            "connection-monitor-page-title",
                            "pool",
                            render.i18n.t("layout.connection_monitor.title"),
                            theme.text,
                            cx,
                        )),
                )
                .child(self.render_connection_pool_monitor(render, cx))
                .into_any_element(),
            ConnectionMonitorSection::Health => div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .mb_4()
                        .text_size(px(20.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(theme.text))
                        .child(Self::render_pool_text(
                            render,
                            "connection-monitor-page-title",
                            "health",
                            render.i18n.t("sidebar.panels.system_health"),
                            theme.text,
                            cx,
                        )),
                )
                .child(self.render_system_health_panel(
                    false,
                    render,
                    self.monitoring.monitor_enabled,
                    cx,
                ))
                .into_any_element(),
        }
    }

    fn render_connection_pool_monitor(
        &self,
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = render.tokens.ui;
        if let Some(error) = self.pool_error() {
            return host_tools_center_state(
                LucideIcon::AlertTriangle,
                MONITOR_RED,
                error.to_string(),
                &render.selectable_text,
                cx,
            );
        }
        let Some(stats) = self.pool_stats_snapshot() else {
            return host_tools_center_state(
                LucideIcon::RefreshCw,
                theme.text_muted,
                render.i18n.t("connections.monitor.loading"),
                &render.selectable_text,
                cx,
            );
        };

        let idle_timeout_label = if stats.idle_timeout_secs == 0 {
            render.i18n.t("connections.monitor.idle_timeout_never")
        } else {
            render
                .i18n
                .t("connections.monitor.idle_timeout")
                .replace("{{min}}", &(stats.idle_timeout_secs / 60).to_string())
        };
        let capacity = if stats.pool_capacity == 0 {
            "∞".to_string()
        } else {
            stats.pool_capacity.to_string()
        };
        let capacity_label = render
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
                            .child(Self::render_pool_text(
                                render,
                                "topology-monitor-header",
                                "title",
                                render.i18n.t("connections.monitor.title"),
                                theme.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(12.0))
                            .text_color(rgb(theme.text_muted))
                            .child(WorkspaceApp::render_lucide_icon(
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
                        render.i18n.t("connections.monitor.active"),
                        stats.active_connections,
                        LucideIcon::Activity,
                        if stats.active_connections > 0 {
                            MONITOR_EMERALD_DARK
                        } else {
                            theme.text_muted
                        },
                        render,
                        cx,
                    ))
                    .child(self.render_pool_stat_card(
                        render.i18n.t("connections.monitor.idle"),
                        stats.idle_connections,
                        LucideIcon::Link2,
                        if stats.idle_connections > 0 {
                            MONITOR_BLUE
                        } else {
                            theme.text_muted
                        },
                        render,
                        cx,
                    ))
                    .child(self.render_pool_stat_card(
                        render.i18n.t("connections.monitor.reconnecting"),
                        stats.reconnecting_connections,
                        LucideIcon::RefreshCw,
                        if stats.reconnecting_connections > 0 {
                            MONITOR_AMBER
                        } else {
                            theme.text_muted
                        },
                        render,
                        cx,
                    ))
                    .child(self.render_pool_stat_card(
                        render.i18n.t("connections.monitor.link_down"),
                        stats.link_down_connections,
                        LucideIcon::AlertTriangle,
                        if stats.link_down_connections > 0 {
                            MONITOR_RED
                        } else {
                            theme.text_muted
                        },
                        render,
                        cx,
                    )),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap_2()
                    .child(self.render_pool_stat_card(
                        render.i18n.t("connections.monitor.terminals"),
                        stats.total_terminals,
                        LucideIcon::Terminal,
                        if stats.total_terminals > 0 {
                            MONITOR_EMERALD_DARK
                        } else {
                            theme.text_muted
                        },
                        render,
                        cx,
                    ))
                    .child(self.render_pool_stat_card(
                        render.i18n.t("connections.monitor.sftp"),
                        stats.total_sftp_sessions,
                        LucideIcon::FolderSync,
                        if stats.total_sftp_sessions > 0 {
                            MONITOR_BLUE
                        } else {
                            theme.text_muted
                        },
                        render,
                        cx,
                    ))
                    .child(self.render_pool_stat_card(
                        render.i18n.t("connections.monitor.forwards"),
                        stats.total_forwards,
                        LucideIcon::ArrowLeftRight,
                        if stats.total_forwards > 0 {
                            MONITOR_BLUE
                        } else {
                            theme.text_muted
                        },
                        render,
                        cx,
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
                        render
                            .i18n
                            .t("connections.monitor.summary")
                            .replace("{{total}}", &stats.total_connections.to_string())
                            .replace("{{refs}}", &stats.total_ref_count.to_string()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(WorkspaceApp::render_lucide_icon(
                                LucideIcon::RefreshCw,
                                12.0,
                                rgb(theme.text_muted),
                            ))
                            .child(Self::render_pool_text(
                                render,
                                "topology-monitor-header",
                                "live",
                                render.i18n.t("connections.monitor.live"),
                                theme.text_muted,
                                cx,
                            )),
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
        render: &MonitorRenderContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = render.tokens.ui;
        let background = if color == theme.text_muted {
            rgba((theme.bg_hover << 8) | 0x4d)
        } else {
            rgba((color << 8) | MONITOR_TINT_ALPHA)
        };
        oxideterm_gpui_ui::semantic_surface(
            &render.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::InsetGroup)
                .padding(oxideterm_gpui_ui::SurfacePadding::None),
        )
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
                .child(WorkspaceApp::render_lucide_icon(icon, 16.0, rgb(color)))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(theme.text_muted))
                        .child(Self::render_pool_text(
                            render,
                            "connection-pool-stat-label",
                            &label,
                            label.clone(),
                            theme.text_muted,
                            cx,
                        )),
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
                .child(Self::render_pool_text(
                    render,
                    "connection-pool-stat-value",
                    &label,
                    value.to_string(),
                    color,
                    cx,
                )),
        )
        .into_any_element()
    }

    fn render_pool_text(
        render: &MonitorRenderContext,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut App,
    ) -> AnyElement {
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
    }
}
