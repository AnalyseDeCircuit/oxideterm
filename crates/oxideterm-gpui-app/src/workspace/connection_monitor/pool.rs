#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ConnectionMonitorSection {
    Pool,
    Health,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ConnectionPoolBodySection {
    Error,
    Loading,
    Empty,
    Connection(usize),
}

impl WorkspaceApp {
    pub(super) fn render_connection_monitor_surface(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.sync_connection_monitor_section_list_state();
        let state = self.connection_monitor_section_list_state.clone();
        let workspace = cx.entity();
        let spec = self.connection_monitor_section_list_spec();
        div()
            .id("connection-monitor-scroll")
            .size_full()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(tauri_virtual_list(state, spec, move |index, _window, cx| {
                workspace.update(cx, |this, cx| {
                    this.render_connection_monitor_section_item(index, cx)
                })
            }))
            .into_any_element()
    }

    fn sync_connection_monitor_section_list_state(&mut self) {
        let spec = self.connection_monitor_section_list_spec();
        let signatures = [
            self.connection_monitor_section_signature(ConnectionMonitorSection::Pool),
            self.connection_monitor_section_signature(ConnectionMonitorSection::Health),
        ];
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor_section_list_state,
            &mut self.connection_monitor_section_list_cache.borrow_mut(),
            "connection-monitor",
            &signatures,
            spec,
        );
    }

    fn connection_monitor_section_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CONNECTION_MONITOR_SECTION_LIST_ESTIMATED_HEIGHT),
            CONNECTION_MONITOR_SECTION_LIST_OVERSCAN,
        )
    }

    fn connection_monitor_section_signature(&self, section: ConnectionMonitorSection) -> u64 {
        let mut hasher = DefaultHasher::new();
        // Pool/health cards change height when loading, errors, or profiler
        // selection state changes; include those browser-section states so
        // GPUI remeasures the variable-height List rows.
        section.hash(&mut hasher);
        self.connection_monitor.pool_error.is_some().hash(&mut hasher);
        self.connection_monitor.pool_stats.is_some().hash(&mut hasher);
        self.connection_monitor.pool_summaries.len().hash(&mut hasher);
        if matches!(section, ConnectionMonitorSection::Health) {
            self.connection_monitor.selected_connection_id.hash(&mut hasher);
            self.connection_monitor
                .disabled_profiler_connections
                .len()
                .hash(&mut hasher);
        }
        hasher.finish()
    }

    fn render_connection_monitor_section_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let section = match index {
            0 => ConnectionMonitorSection::Pool,
            1 => ConnectionMonitorSection::Health,
            _ => return div().into_any_element(),
        };
        div()
            .max_w(px(MONITOR_CONTENT_MAX_WIDTH))
            .mx_auto()
            .px(px(MONITOR_PAGE_PADDING))
            .pb(px(MONITOR_SECTION_GAP))
            .when(index == 0, |item| item.pt(px(MONITOR_PAGE_PADDING)))
            .when(
                index + 1 == CONNECTION_MONITOR_SECTION_LIST_ITEM_COUNT,
                |item| item.pb(px(MONITOR_PAGE_PADDING)),
            )
            .child(self.render_connection_monitor_section(section, cx))
            .into_any_element()
    }

    fn render_connection_monitor_section(
        &self,
        section: ConnectionMonitorSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
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
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "connection-monitor-page-title",
                            "pool",
                            self.i18n.t("layout.connection_monitor.title"),
                            theme.text,
                            cx,
                        )),
                )
                .child(self.render_connection_pool_monitor(cx))
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
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "connection-monitor-page-title",
                            "health",
                            self.i18n.t("sidebar.panels.system_health"),
                            theme.text,
                            cx,
                        )),
                )
                .child(self.render_system_health_panel(cx))
                .into_any_element(),
        }
    }

    pub(super) fn render_connection_pool_surface(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let stats = self.connection_monitor.pool_stats.as_ref();
        let idle_timeout_secs = stats.map_or(0, |stats| stats.idle_timeout_secs);
        self.sync_connection_pool_body_list_state();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(CONNECTION_POOL_HEADER_X))
                    .py(px(CONNECTION_POOL_HEADER_Y))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_card))
                    // Tauri gives bg-theme-bg-card surfaces the shared card
                    // elevation even outside the Settings view.
                    .shadow(oxideterm_gpui_ui::tauri_card_shadow(theme.bg_card))
                    .child(
                        div()
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "connection-pool-header",
                                        "title",
                                        self.i18n.t("connections.panel.title"),
                                        theme.text_heading,
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(14.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "connection-pool-header",
                                        "description",
                                        self.i18n.t("connections.panel.description"),
                                        theme.text_muted,
                                        cx,
                                    )),
                            ),
                    )
                    .child(self.render_connection_pool_refresh_button(cx)),
            )
            .child(
                div()
                    .id("connection-pool-scroll")
                    .flex_1()
                    .child(self.render_connection_pool_body_list(idle_timeout_secs, cx)),
            )
            .child(self.render_connection_pool_keep_alive_legend(idle_timeout_secs, cx))
            .into_any_element()
    }

    fn sync_connection_pool_body_list_state(&mut self) {
        let spec = self.connection_pool_body_list_spec();
        let signatures = self.connection_pool_body_signatures();
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_pool_body_list_state,
            &mut self.connection_pool_body_list_cache.borrow_mut(),
            "connection-pool-body",
            &signatures,
            spec,
        );
    }

    fn connection_pool_body_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CONNECTION_POOL_BODY_LIST_ESTIMATED_HEIGHT),
            CONNECTION_POOL_BODY_LIST_OVERSCAN,
        )
    }

    fn connection_pool_body_sections(&self) -> Vec<ConnectionPoolBodySection> {
        if self.connection_monitor.pool_error.is_some() {
            return vec![ConnectionPoolBodySection::Error];
        }
        if self.connection_monitor.pool_stats.is_none() {
            return vec![ConnectionPoolBodySection::Loading];
        }
        let count = self
            .connection_monitor
            .pool_summaries
            .iter()
            .filter(|summary| summary.is_displayed_in_pool())
            .count();
        if count == 0 {
            vec![ConnectionPoolBodySection::Empty]
        } else {
            (0..count).map(ConnectionPoolBodySection::Connection).collect()
        }
    }

    fn connection_pool_body_signatures(&self) -> Vec<u64> {
        self.connection_pool_body_sections()
            .into_iter()
            .map(|section| self.connection_pool_body_signature(section))
            .collect()
    }

    fn connection_pool_body_signature(&self, section: ConnectionPoolBodySection) -> u64 {
        let mut hasher = DefaultHasher::new();
        // Connection cards are variable height because idle hints and metric
        // counts can appear/disappear. Hash the visible card state so ListState
        // remeasures only the affected card instead of rebuilding a full grid.
        section.hash(&mut hasher);
        match section {
            ConnectionPoolBodySection::Error => {
                self.connection_monitor.pool_error.hash(&mut hasher);
            }
            ConnectionPoolBodySection::Loading | ConnectionPoolBodySection::Empty => {}
            ConnectionPoolBodySection::Connection(index) => {
                if let Some(summary) = self.visible_connection_pool_summary(index) {
                    summary.id.hash(&mut hasher);
                    format!("{:?}", summary.state).hash(&mut hasher);
                    summary.terminal_count.hash(&mut hasher);
                    summary.forward_count.hash(&mut hasher);
                    summary.has_sftp_session.hash(&mut hasher);
                    summary.keep_alive.hash(&mut hasher);
                }
            }
        }
        hasher.finish()
    }

    fn visible_connection_pool_summary(&self, index: usize) -> Option<ConnectionPoolEntrySummary> {
        self.connection_monitor
            .pool_summaries
            .iter()
            .filter(|summary| summary.is_displayed_in_pool())
            .nth(index)
            .cloned()
    }

    fn render_connection_pool_body_list(
        &self,
        idle_timeout_secs: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.connection_pool_body_list_state.clone();
        let workspace = cx.entity();
        let spec = self.connection_pool_body_list_spec();
        tauri_virtual_list(state, spec, move |index, _window, cx| {
            workspace.update(cx, |this, cx| {
                this.render_connection_pool_body_item(index, idle_timeout_secs, cx)
            })
        })
        .into_any_element()
    }

    fn render_connection_pool_body_item(
        &self,
        index: usize,
        idle_timeout_secs: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(section) = self.connection_pool_body_sections().get(index).copied() else {
            return div().into_any_element();
        };
        let child = match section {
            ConnectionPoolBodySection::Error => monitor_center_state(
                self,
                LucideIcon::AlertTriangle,
                MONITOR_RED,
                self.connection_monitor.pool_error.clone().unwrap_or_default(),
                cx,
            ),
            ConnectionPoolBodySection::Loading => monitor_center_state(
                self,
                LucideIcon::RefreshCw,
                theme.text_muted,
                self.i18n.t("connections.monitor.loading"),
                cx,
            ),
            ConnectionPoolBodySection::Empty => self.render_connection_pool_empty_state(cx),
            ConnectionPoolBodySection::Connection(connection_index) => self
                .visible_connection_pool_summary(connection_index)
                .map(|connection| {
                    self.render_connection_pool_card(connection, idle_timeout_secs, cx)
                })
                .unwrap_or_else(|| div().into_any_element()),
        };
        div()
            .max_w(px(896.0))
            .px(px(CONNECTION_POOL_BODY_PADDING))
            .pb(px(CONNECTION_POOL_CARD_GAP))
            .when(index == 0, |item| item.pt(px(CONNECTION_POOL_BODY_PADDING)))
            .when(index + 1 == self.connection_pool_body_sections().len(), |item| {
                item.pb(px(CONNECTION_POOL_BODY_PADDING))
            })
            .child(child)
            .into_any_element()
    }

    fn render_connection_pool_refresh_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        // Pool header refresh is toolbar-shaped and follows Tauri's outline
        // Button. Use the workspace wrapper so activation guards stay shared
        // with other migrated action buttons.
        self.workspace_toolbar_action_button(
            self.i18n.t("connections.panel.refresh"),
            Some(Self::render_lucide_icon(
                LucideIcon::RefreshCw,
                CONNECTION_POOL_ICON_SIZE_MD,
                rgb(theme.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                background: Some(rgb(theme.bg)),
                border: Some(rgb(theme.border)),
                text_color: Some(rgb(theme.text)),
                hover_background: Some(rgb(theme.bg_hover)),
                hover_border: Some(rgb(theme.border_strong)),
                height: Some(CONNECTION_POOL_BUTTON_SIZE),
                padding_x: Some(12.0),
                font_size: Some(14.0),
                icon_gap: Some(8.0),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.refresh_connection_monitor_pool_stats();
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_connection_pool_empty_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .py(px(CONNECTION_POOL_EMPTY_Y))
            .flex()
            .flex_col()
            .items_center()
            .text_align(gpui::TextAlign::Center)
            .text_color(rgb(theme.text_muted))
            .child(Self::render_lucide_icon(
                LucideIcon::Server,
                64.0,
                rgba((theme.text_muted << 8) | CONNECTION_POOL_EMPTY_ICON_ALPHA),
            ))
            .child(
                div()
                    .mt_4()
                    .text_size(px(18.0))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "connection-pool-empty",
                        "title",
                        self.i18n.t("connections.panel.no_connections"),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .mt_2()
                    .text_size(px(14.0))
                    .opacity(CONNECTION_POOL_EMPTY_HINT_OPACITY)
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "connection-pool-empty",
                        "hint",
                        self.i18n.t("connections.panel.no_connections_hint"),
                        theme.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_connection_pool_card(
        &self,
        connection: ConnectionPoolEntrySummary,
        idle_timeout_secs: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let state = connection_pool_state_view(&connection.state, &self.i18n, &self.tokens);
        let is_idle = matches!(connection.state, ConnectionPoolEntryState::Idle);
        let is_active = matches!(connection.state, ConnectionPoolEntryState::Active);
        let global_never_timeout = idle_timeout_secs == 0;
        let idle_timeout_min = (idle_timeout_secs as f64 / 60.0).round() as u64;
        let tooltip = connection_pool_keep_alive_tooltip(
            &self.i18n,
            connection.keep_alive,
            global_never_timeout,
            idle_timeout_min,
        );
        let connection_id = connection.id.clone();
        let next_keep_alive = !connection.keep_alive;

        div()
            .border_1()
            .border_color(if is_idle {
                rgba((MONITOR_AMBER << 8) | CONNECTION_POOL_IDLE_BORDER_ALPHA_30)
            } else {
                rgb(theme.border)
            })
            .rounded(px(self.tokens.radii.lg))
            .p(px(CONNECTION_POOL_CARD_PADDING))
            .bg(rgb(theme.bg_panel))
            .flex()
            .flex_col()
            .gap_3()
            .hover(|style| style.border_color(rgb(theme.border_strong)))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Self::render_lucide_icon(
                                LucideIcon::Server,
                                CONNECTION_POOL_ICON_SIZE_LG,
                                rgb(if is_active {
                                    CONNECTION_POOL_GREEN_400
                                } else if is_idle {
                                    CONNECTION_POOL_AMBER_400
                                } else {
                                    theme.text_muted
                                }),
                            ))
                            .child(
                                div()
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(theme.text))
                                            .child(self.render_display_text_with_role(
                                                SelectableTextRole::PlainDocument,
                                                "connection-pool-card-endpoint",
                                                connection.id.as_str(),
                                                format!(
                                                    "{}@{}:{}",
                                                    connection.username,
                                                    connection.host,
                                                    connection.port
                                                ),
                                                theme.text,
                                                cx,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(rgb(state.color))
                                            .child(state.label),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .w(px(CONNECTION_POOL_BUTTON_SIZE))
                            .h(px(CONNECTION_POOL_BUTTON_SIZE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.md))
                            .cursor(if global_never_timeout {
                                CursorStyle::Arrow
                            } else {
                                CursorStyle::PointingHand
                            })
                            .opacity(if global_never_timeout { 0.5 } else { 1.0 })
                            .hover(|style| style.bg(rgb(theme.bg_hover)))
                            .on_mouse_move(cx.listener({
                                let tooltip = tooltip.clone();
                                let tooltip_id = format!("pool-keepalive-{}", connection_id);
                                move |this, event: &MouseMoveEvent, _window, cx| {
                                    this.queue_workspace_tooltip(
                                        tooltip_id.clone(),
                                        tooltip.clone(),
                                        f32::from(event.position.x) + 12.0,
                                        f32::from(event.position.y) + 16.0,
                                        cx,
                                    );
                                }
                            }))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener({
                                    let connection_id = connection_id.clone();
                                    let tooltip_id = format!("pool-keepalive-{}", connection_id);
                                    move |this, _event, _window, cx| {
                                        this.clear_workspace_tooltip(&tooltip_id, cx);
                                        if !global_never_timeout {
                                            this.set_connection_pool_keep_alive(
                                                &connection_id,
                                                next_keep_alive,
                                                cx,
                                            );
                                        }
                                        cx.stop_propagation();
                                    }
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                if global_never_timeout || connection.keep_alive {
                                    LucideIcon::Shield
                                } else {
                                    LucideIcon::ShieldOff
                                },
                                CONNECTION_POOL_ICON_SIZE_MD,
                                rgb(if global_never_timeout || connection.keep_alive {
                                    CONNECTION_POOL_GREEN_400
                                } else {
                                    theme.text_muted
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap_2()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.text_muted))
                    .child(
                        self.render_connection_pool_metric(
                            LucideIcon::Terminal,
                            self.i18n
                                .t("connections.panel.terminals")
                                .replace("{{count}}", &connection.terminal_count.to_string()),
                            cx,
                        ),
                    )
                    .child(self.render_connection_pool_metric(
                        LucideIcon::FolderOpen,
                        self.i18n.t("connections.panel.sftp").replace(
                            "{{count}}",
                            if connection.has_sftp_session {
                                "1"
                            } else {
                                "0"
                            },
                        ),
                        cx,
                    ))
                    .child(
                        self.render_connection_pool_metric(
                            LucideIcon::GitFork,
                            self.i18n
                                .t("connections.panel.forwards")
                                .replace("{{count}}", &connection.forward_count.to_string()),
                            cx,
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_connection_pool_metric(
                        LucideIcon::Clock,
                        self.i18n.t("connections.panel.created").replace(
                            "{{time}}",
                            &self.format_connection_pool_time(connection.created_at),
                        ),
                        cx,
                    ))
                    .when(is_idle, |row| {
                        let keep_alive_label = if global_never_timeout || connection.keep_alive {
                            self.i18n.t("connections.panel.keep_alive_enabled")
                        } else {
                            self.i18n
                                .t("connections.panel.disconnect_in")
                                .replace("{{min}}", &idle_timeout_min.to_string())
                        };
                        row.child(
                            div().text_color(rgb(CONNECTION_POOL_AMBER_400)).child(
                                self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "connection-pool-idle-hint",
                                    connection.id.as_str(),
                                    self.i18n
                                        .t("connections.panel.idle_hint")
                                        .replace("{{keepAlive}}", &keep_alive_label),
                                    CONNECTION_POOL_AMBER_400,
                                    cx,
                                ),
                            ),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_connection_pool_metric(
        &self,
        icon: LucideIcon,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label_key = label.clone();
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(Self::render_lucide_icon(
                icon,
                CONNECTION_POOL_ICON_SIZE_SM,
                rgb(theme.text_muted),
            ))
            .child(self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "connection-pool-metric",
                (label_key, icon as u8),
                label,
                theme.text_muted,
                cx,
            ))
            .into_any_element()
    }

    fn render_connection_pool_keep_alive_legend(
        &self,
        idle_timeout_secs: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let disabled_label = if idle_timeout_secs == 0 {
            self.i18n.t("connections.keep_alive.global_never_tooltip")
        } else {
            self.i18n
                .t("connections.keep_alive.legend_disabled")
                .replace(
                    "{{min}}",
                    &((idle_timeout_secs as f64 / 60.0).round() as u64).to_string(),
                )
        };
        div()
            .px(px(CONNECTION_POOL_HEADER_X))
            .py(px(CONNECTION_POOL_HEADER_Y))
            .border_t_1()
            .border_color(rgb(theme.border))
            .bg(rgba((theme.bg_panel << 8) | CONNECTION_POOL_PANEL_ALPHA_30))
            .flex()
            .items_center()
            .gap(px(24.0))
            .text_size(px(14.0))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Self::render_lucide_icon(
                        LucideIcon::Shield,
                        CONNECTION_POOL_ICON_SIZE_MD,
                        rgb(CONNECTION_POOL_GREEN_400),
                    ))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "connection-pool-keep-alive-legend",
                        "enabled",
                        self.i18n.t("connections.keep_alive.legend_enabled"),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Self::render_lucide_icon(
                        LucideIcon::ShieldOff,
                        CONNECTION_POOL_ICON_SIZE_MD,
                        rgb(theme.text_muted),
                    ))
                    .child(disabled_label),
            )
            .into_any_element()
    }

}
