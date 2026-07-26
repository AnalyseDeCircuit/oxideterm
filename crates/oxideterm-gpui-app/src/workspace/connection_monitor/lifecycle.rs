use super::*;

use oxideterm_connection_monitor::ResourceSampler;

fn is_host_tools_tab_kind(tab_kind: &TabKind) -> bool {
    matches!(
        tab_kind,
        TabKind::ConnectionPool | TabKind::ConnectionMonitor | TabKind::Topology | TabKind::Runtime
    )
}

fn host_tools_surface_visible(
    main_tab_visible: bool,
    detached_tab_visible: bool,
    sidebar_visible: bool,
) -> bool {
    main_tab_visible || detached_tab_visible || sidebar_visible
}

impl WorkspaceApp {
    pub(in crate::workspace) fn host_tools_surface_visible(&self) -> bool {
        let main_tab_visible = self.tabs.iter().any(|tab| {
            is_host_tools_tab_kind(&tab.kind)
                && self.main_window_tabs.active_tab_id == Some(tab.id)
                && !self.detached_tabs.contains(&tab.id)
        });
        let detached_tab_visible = self.tabs.iter().any(|tab| {
            is_host_tools_tab_kind(&tab.kind) && self.detached_tab_windows.contains_key(&tab.id)
        });
        let sidebar_visible = self.context_sidebar_visible()
            && self.active_context_sidebar_panel == ContextSidebarPanel::HostTools;

        host_tools_surface_visible(main_tab_visible, detached_tab_visible, sidebar_visible)
    }

    pub(in crate::workspace) fn set_connection_runtime_section(
        &mut self,
        section: ConnectionRuntimeSection,
    ) {
        if self.active_connection_runtime_section != section {
            self.previous_connection_runtime_section = self.active_connection_runtime_section;
            self.active_connection_runtime_section = section;
        }
    }

    pub(in crate::workspace) fn open_connection_runtime_tab(
        &mut self,
        section: ConnectionRuntimeSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_connection_runtime_section(section);
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

    pub(in crate::workspace) fn open_connection_monitor_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_connection_runtime_tab(ConnectionRuntimeSection::Health, window, cx);
    }

    pub(in crate::workspace) fn open_connection_pool_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_connection_runtime_tab(ConnectionRuntimeSection::Overview, window, cx);
    }

    pub(in crate::workspace) fn open_topology_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_connection_runtime_tab(ConnectionRuntimeSection::Topology, window, cx);
    }

    pub(in crate::workspace) fn maybe_refresh_connection_monitor(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.sync_host_gpu_sampling(cx);
        if !self.host_tools_surface_visible() {
            // Profilers own only sampling shells. Shared SSH nodes and user-triggered
            // Host Tools operations continue independently while every surface is hidden.
            self.connection_monitor.profiler_registry.stop_all();
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

    pub(in crate::workspace) fn refresh_connection_monitor_pool_stats(&mut self) {
        self.connection_monitor.pool_stats = Some(self.ssh_registry.monitor_stats());
        self.connection_monitor.pool_summaries = self.ssh_registry.list_connection_summaries();
        self.connection_monitor.topology_snapshot =
            Some(self.ssh_registry.connection_topology_snapshot());
        self.connection_monitor.pool_error = None;
        self.connection_monitor.last_pool_refresh = Some(Instant::now());
    }

    pub(in crate::workspace) fn sync_connection_monitor_selection(
        &mut self,
        cx: &mut Context<Self>,
    ) {
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
        if !self.host_tools_surface_visible() || self.resource_sampling_config().is_empty() {
            self.connection_monitor.profiler_registry.stop_all();
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

    pub(super) fn start_connection_monitor_profiler(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            return;
        };
        let Some(os_type) = handle.remote_env().map(|env| env.os_type) else {
            // Lifecycle polling retries this start after environment detection;
            // choosing Linux here would run incorrect probes on other hosts.
            return;
        };
        let sampler: Arc<dyn ResourceSampler> = Arc::new(handle);
        self.connection_monitor
            .profiler_registry
            .start_with_sampler_on_config(
                connection_id,
                sampler,
                os_type,
                self.resource_sampling_config(),
                Some(self.connection_monitor.profiler_update_tx.clone()),
                self.forwarding_runtime.handle().clone(),
            );
        cx.notify();
    }

    pub(in crate::workspace) fn apply_host_tool_monitoring_settings(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let config = self.resource_sampling_config();
        if config.is_empty() || !self.host_tools_surface_visible() {
            // The registry owns persistent shells, so stop them at the settings boundary.
            self.connection_monitor.profiler_registry.stop_all();
        } else {
            for connection_id in self.connection_monitor.profiler_registry.connection_ids() {
                self.start_connection_monitor_profiler(connection_id, cx);
            }
            self.sync_connection_monitor_selection(cx);
        }
        self.sync_host_gpu_sampling(cx);
    }

    fn resource_sampling_config(&self) -> oxideterm_connection_monitor::ResourceSamplingConfig {
        let host_tools = &self.settings_store.settings().host_tools;
        oxideterm_connection_monitor::ResourceSamplingConfig {
            system: host_tools.monitor_enabled,
            // The detailed GPU page owns its own task; this probe only feeds Monitor summaries.
            gpu: host_tools.monitor_enabled && host_tools.gpu_enabled,
            processes: host_tools.processes_enabled,
            docker: host_tools.docker_enabled,
        }
    }

    pub(super) fn monitor_connections(&self) -> Vec<MonitorConnectionOption> {
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

#[cfg(test)]
mod tests {
    use super::host_tools_surface_visible;

    #[test]
    fn host_tools_visibility_covers_main_detached_and_sidebar_surfaces() {
        assert!(host_tools_surface_visible(true, false, false));
        assert!(host_tools_surface_visible(false, true, false));
        assert!(host_tools_surface_visible(false, false, true));
        assert!(host_tools_surface_visible(true, true, true));
        assert!(!host_tools_surface_visible(false, false, false));
    }
}
