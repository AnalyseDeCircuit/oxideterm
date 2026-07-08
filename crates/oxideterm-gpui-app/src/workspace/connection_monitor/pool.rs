use gpui::StatefulInteractiveElement;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ConnectionMonitorSection {
    Pool,
    Health,
}

impl WorkspaceApp {
    pub(super) fn render_connection_monitor_surface(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self
            .terminal_background_preferences("connection_monitor")
            .is_some();
        self.sync_connection_monitor_section_list_state();
        let state = self.connection_monitor_section_list_state.clone();
        let workspace = cx.entity();
        let spec = self.connection_monitor_section_list_spec();
        div()
            .id("connection-monitor-scroll")
            .size_full()
            .bg(connection_monitor_surface_bg(theme.bg, has_background))
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
            // The legacy monitor tab should follow the runtime pages: page
            // padding is local, while cards can use the full workspace width.
            .w_full()
            .min_w(px(0.0))
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
                .child(self.render_system_health_panel(false, cx))
                .into_any_element(),
        }
    }
}
