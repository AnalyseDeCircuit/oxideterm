use super::*;

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
        cx: &mut Context<Self>,
    ) {
        self.host_tools.update(cx, |host_tools, _cx| {
            host_tools.set_runtime_section(section);
        });
    }

    pub(in crate::workspace) fn open_connection_runtime_tab(
        &mut self,
        section: ConnectionRuntimeSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_connection_runtime_section(section, cx);
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
        self.refresh_connection_monitor_pool_stats(cx);
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
            self.host_tools.read(cx).stop_profiler_sampling();
            return;
        }

        let stale = self
            .host_tools
            .read(cx)
            .pool_refresh_is_stale(MONITOR_POOL_REFRESH_INTERVAL);
        if stale {
            self.refresh_connection_monitor_pool_stats(cx);
        }
        let selected_connection_id = self.host_tools.read(cx).selected_connection_id_owned();
        let connections = self.monitor_connections(cx);
        let selected_missing = selected_connection_id.as_ref().is_none_or(|selected| {
            !connections
                .iter()
                .any(|connection| connection.connection_id == *selected)
        });
        if stale || selected_missing {
            // Selection sync scans the registry and may start profilers. Keep it
            // tied to pool refreshes instead of every terminal-driven repaint.
            self.sync_connection_monitor_selection(cx);
        }
    }

    pub(in crate::workspace) fn refresh_connection_monitor_pool_stats(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.host_tools
            .update(cx, |host_tools, cx| host_tools.refresh_pool_snapshot(cx));
    }

    pub(in crate::workspace) fn sync_connection_monitor_selection(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let connections = self.monitor_connections(cx);
        let live_connection_ids = connections
            .iter()
            .map(|connection| connection.connection_id.as_str())
            .collect::<HashSet<_>>();
        for connection_id in self.host_tools.read(cx).profiler_connection_ids() {
            if !live_connection_ids.contains(connection_id.as_str()) {
                self.host_tools
                    .read(cx)
                    .remove_profiler_connection(&connection_id);
            }
        }
        if connections.is_empty() {
            self.host_tools.update(cx, |host_tools, cx| {
                if let Some(connection_id) = host_tools.take_selected_connection(cx) {
                    host_tools.remove_profiler_connection(&connection_id);
                }
            });
            return;
        }

        let live_connection_ids = connections
            .iter()
            .map(|connection| connection.connection_id.clone())
            .collect::<Vec<_>>();
        let Some(connection_id) = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.ensure_selected_connection(&live_connection_ids, cx)
        }) else {
            return;
        };
        if !self.host_tools_surface_visible() || self.resource_sampling_config().is_empty() {
            self.host_tools.read(cx).stop_profiler_sampling();
            return;
        }
        if self
            .host_tools
            .read(cx)
            .profiler_connection_missing(&connection_id)
        {
            self.start_connection_monitor_profiler(connection_id, cx);
        }
    }

    pub(super) fn start_connection_monitor_profiler(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        let sampling_config = self.resource_sampling_config();
        let runtime = self.forwarding_runtime.handle().clone();
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.start_profiler(connection_id, sampling_config, runtime, cx);
        });
    }

    pub(in crate::workspace) fn apply_host_tool_monitoring_settings(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let config = self.resource_sampling_config();
        if config.is_empty() || !self.host_tools_surface_visible() {
            // The registry owns persistent shells, so stop them at the settings boundary.
            self.host_tools.read(cx).stop_profiler_sampling();
        } else {
            for connection_id in self.host_tools.read(cx).profiler_connection_ids() {
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

    pub(super) fn monitor_connections(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<MonitorConnectionOption> {
        self.host_tools.read(cx).monitor_connections()
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
