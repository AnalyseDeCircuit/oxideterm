impl WorkspaceApp {
    pub(super) fn open_connection_runtime_tab(
        &mut self,
        section: ConnectionRuntimeSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_connection_runtime_section = section;
        let tab_id = if let Some(tab) = self.tabs.iter().find(|tab| tab.kind == TabKind::Runtime) {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Runtime,
                title: self.i18n.t("sidebar.panels.runtime"),
                title_source: TabTitleSource::I18nKey("sidebar.panels.runtime"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.set_active_tab(tab_id, window, cx);
        self.refresh_connection_monitor_pool_stats();
        self.sync_connection_monitor_selection(cx);
    }

    pub(super) fn open_connection_monitor_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_connection_runtime_tab(ConnectionRuntimeSection::Health, window, cx);
    }

    pub(super) fn open_connection_pool_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_connection_runtime_tab(ConnectionRuntimeSection::Pool, window, cx);
    }

    pub(super) fn open_topology_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_connection_runtime_tab(ConnectionRuntimeSection::Topology, window, cx);
    }

    pub(super) fn poll_connection_monitor_updates(
        &mut self,
        request_repaint: bool,
        cx: &mut Context<Self>,
    ) {
        let mut received_update = false;
        while self
            .connection_monitor
            .profiler_update_rx
            .try_recv()
            .is_ok()
        {
            received_update = true;
        }
        if received_update && request_repaint {
            // Background polling should wake the UI, but render-time draining
            // must not schedule a second frame after the current one.
            cx.notify();
        }
    }

    pub(super) fn maybe_refresh_connection_monitor(&mut self, cx: &mut Context<Self>) {
        let monitor_surface_visible = self.active_tab().is_some_and(|tab| {
            matches!(
                tab.kind,
                TabKind::ConnectionPool | TabKind::ConnectionMonitor | TabKind::Topology
                    | TabKind::Runtime
            )
        }) || (self.context_sidebar_visible()
            && self.active_context_sidebar_panel == ContextSidebarPanel::HostTools
            && matches!(
                self.active_context_sidebar_tool,
                ContextSidebarTool::Monitor
                    | ContextSidebarTool::Processes
                    | ContextSidebarTool::Services
                    | ContextSidebarTool::Logs
                    | ContextSidebarTool::Tmux
                    | ContextSidebarTool::Docker
            ));
        if !monitor_surface_visible {
            return;
        }

        let stale = self
            .connection_monitor
            .last_pool_refresh
            .is_none_or(|last| last.elapsed() >= MONITOR_POOL_REFRESH_INTERVAL);
        if stale {
            self.refresh_connection_monitor_pool_stats();
        }
        let selected_missing = self
            .connection_monitor
            .selected_connection_id
            .as_ref()
            .is_none_or(|selected| {
                !self
                    .connection_monitor
                    .pool_summaries
                    .iter()
                    .any(|summary| summary.id == *selected)
            });
        if stale || selected_missing {
            // Selection sync scans the registry and may start profilers. Keep it
            // tied to pool refreshes instead of every terminal-driven repaint.
            self.sync_connection_monitor_selection(cx);
        }
    }

    pub(super) fn refresh_connection_monitor_pool_stats(&mut self) {
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

    pub(super) fn sync_connection_monitor_selection(&mut self, cx: &mut Context<Self>) {
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
        let os_type = handle
            .remote_env()
            .map(|env| env.os_type)
            .unwrap_or_else(|| "Linux".to_string());
        let sampler: Arc<dyn ResourceSampler> = Arc::new(handle);
        self.connection_monitor
            .profiler_registry
            .start_with_sampler_on(
                connection_id,
                sampler,
                os_type,
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

    fn monitor_connections(&self) -> Vec<MonitorConnectionOption> {
        if !self.connection_monitor.pool_summaries.is_empty() {
            return self
                .connection_monitor
                .pool_summaries
                .iter()
                .filter(|summary| summary.is_displayed_in_pool())
                .map(MonitorConnectionOption::from_pool_summary)
                .collect();
        }

        let mut connections = self
            .ssh_registry
            .list()
            .into_iter()
            .map(MonitorConnectionOption::from_connection_info)
            .collect::<Vec<_>>();
        connections.sort_by(|left, right| {
            monitor_connection_label(left).cmp(&monitor_connection_label(right))
        });
        connections
    }

}
