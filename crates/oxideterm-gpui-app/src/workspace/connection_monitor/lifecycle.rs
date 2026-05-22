impl WorkspaceApp {
    pub(super) fn open_connection_monitor_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.kind == TabKind::ConnectionMonitor)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::ConnectionMonitor,
                title: self.i18n.t("sidebar.panels.connection_monitor"),
                title_source: TabTitleSource::I18nKey("sidebar.panels.connection_monitor"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.set_active_tab(tab_id, window, cx);
        self.active_sidebar_section = SidebarSection::Activity;
        self.refresh_connection_monitor_pool_stats();
        self.sync_connection_monitor_selection(cx);
    }

    pub(super) fn open_connection_pool_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.kind == TabKind::ConnectionPool)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::ConnectionPool,
                title: self.i18n.t("sidebar.panels.connection_pool"),
                title_source: TabTitleSource::I18nKey("sidebar.panels.connection_pool"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.active_sidebar_section = SidebarSection::Terminal;
        self.set_active_tab(tab_id, window, cx);
        self.refresh_connection_monitor_pool_stats();
    }

    pub(super) fn open_topology_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self.tabs.iter().find(|tab| tab.kind == TabKind::Topology) {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Topology,
                title: self.i18n.t("sidebar.panels.connection_matrix"),
                title_source: TabTitleSource::I18nKey("sidebar.panels.connection_matrix"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.active_sidebar_section = SidebarSection::Network;
        self.set_active_tab(tab_id, window, cx);
        self.refresh_connection_monitor_pool_stats();
    }

    pub(super) fn poll_connection_monitor_updates(&mut self, cx: &mut Context<Self>) {
        while self
            .connection_monitor
            .profiler_update_rx
            .try_recv()
            .is_ok()
        {
            cx.notify();
        }
    }

    pub(super) fn maybe_refresh_connection_monitor(&mut self, cx: &mut Context<Self>) {
        if !self.active_tab().is_some_and(|tab| {
            matches!(
                tab.kind,
                TabKind::ConnectionPool | TabKind::ConnectionMonitor | TabKind::Topology
            )
        }) {
            return;
        }

        let stale = self
            .connection_monitor
            .last_pool_refresh
            .is_none_or(|last| last.elapsed() >= MONITOR_POOL_REFRESH_INTERVAL);
        if stale {
            self.refresh_connection_monitor_pool_stats();
        }
        self.sync_connection_monitor_selection(cx);
    }

    fn refresh_connection_monitor_pool_stats(&mut self) {
        self.connection_monitor.pool_stats = Some(self.ssh_registry.monitor_stats());
        self.connection_monitor.pool_summaries = self.ssh_registry.list_connection_summaries();
        self.connection_monitor.topology_snapshot =
            Some(self.ssh_registry.connection_topology_snapshot());
        self.connection_monitor.pool_error = None;
        self.connection_monitor.last_pool_refresh = Some(Instant::now());
    }

    fn set_connection_pool_keep_alive(
        &mut self,
        connection_id: &str,
        keep_alive: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .ssh_registry
            .set_keep_alive(connection_id, keep_alive)
            .is_none()
        {
            return;
        }
        self.refresh_connection_monitor_pool_stats();
        cx.notify();
    }

    fn sync_connection_monitor_selection(&mut self, cx: &mut Context<Self>) {
        let connections = self.monitor_connections();
        let live_connection_ids = connections
            .iter()
            .map(|connection| connection.connection_id.as_str())
            .collect::<HashSet<_>>();
        for connection_id in self.connection_monitor.profiler_registry.connection_ids() {
            if !live_connection_ids.contains(connection_id.as_str()) {
                self.connection_monitor
                    .profiler_registry
                    .remove(&connection_id);
                self.connection_monitor
                    .disabled_profiler_connections
                    .remove(&connection_id);
            }
        }
        if connections.is_empty() {
            if let Some(connection_id) = self.connection_monitor.selected_connection_id.take() {
                self.connection_monitor
                    .profiler_registry
                    .remove(&connection_id);
            }
            self.connection_monitor.selector_open = false;
            self.connection_monitor.selector_highlighted_index = None;
            self.connection_monitor.selector_focus_origin = None;
            return;
        }

        let selected_missing = self
            .connection_monitor
            .selected_connection_id
            .as_ref()
            .is_none_or(|selected| {
                !connections
                    .iter()
                    .any(|connection| connection.connection_id == *selected)
            });
        if selected_missing {
            self.connection_monitor.selected_connection_id =
                Some(connections[0].connection_id.clone());
        }

        let Some(connection_id) = self.connection_monitor.selected_connection_id.clone() else {
            return;
        };
        if self
            .connection_monitor
            .disabled_profiler_connections
            .contains(&connection_id)
        {
            return;
        }
        if self
            .connection_monitor
            .profiler_registry
            .state(&connection_id)
            .is_none()
        {
            self.start_connection_monitor_profiler(connection_id, cx);
        }
    }

    fn start_connection_monitor_profiler(&mut self, connection_id: String, cx: &mut Context<Self>) {
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            return;
        };
        self.connection_monitor
            .disabled_profiler_connections
            .remove(&connection_id);
        let sampler: Arc<dyn ResourceSampler> = Arc::new(handle);
        self.connection_monitor
            .profiler_registry
            .start_with_sampler_on(
                connection_id,
                sampler,
                "Linux",
                Some(self.connection_monitor.profiler_update_tx.clone()),
                self.forwarding_runtime.handle().clone(),
            );
        cx.notify();
    }

    fn stop_connection_monitor_profiler(&mut self, connection_id: String, cx: &mut Context<Self>) {
        self.connection_monitor
            .profiler_registry
            .stop(&connection_id);
        self.connection_monitor
            .disabled_profiler_connections
            .insert(connection_id);
        cx.notify();
    }

    fn monitor_connections(&self) -> Vec<oxideterm_ssh::ConnectionInfo> {
        let mut connections = self.ssh_registry.list();
        connections.sort_by(|left, right| {
            monitor_connection_label(left).cmp(&monitor_connection_label(right))
        });
        connections
    }

}
