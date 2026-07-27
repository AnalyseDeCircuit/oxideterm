use super::*;

fn is_host_tools_tab_kind(tab_kind: &TabKind) -> bool {
    matches!(
        tab_kind,
        TabKind::ConnectionPool | TabKind::ConnectionMonitor | TabKind::Topology | TabKind::Runtime
    )
}

fn host_tools_visibility(
    main_tab_visible: bool,
    detached_tab_visible: bool,
    sidebar_visible: bool,
) -> HostToolsVisibility {
    HostToolsVisibility::from_mounts(main_tab_visible, sidebar_visible, detached_tab_visible)
}

impl HostToolsEntity {
    pub(in crate::workspace) fn set_messages(&mut self, messages: HostToolsMessages) {
        self.messages = Some(messages);
    }

    pub(in crate::workspace) fn update_lifecycle(
        &mut self,
        visibility: HostToolsVisibility,
        monitoring: oxideterm_settings::HostToolsSettings,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: tokio::runtime::Handle,
        force_pool_refresh: bool,
        cx: &mut Context<Self>,
    ) {
        self.visibility = visibility;
        self.monitoring = monitoring;
        self.sampling_config = sampling_config;
        self.lifecycle_runtime = Some(runtime.clone());
        let gpu_visible = self.monitoring.gpu_enabled
            && visibility.sidebar_is_visible()
            && self.active_tool() == ContextSidebarTool::Gpu;
        let selected_connection_id = self.selected_connection_id_owned();
        self.sync_gpu_sampling(gpu_visible, selected_connection_id, runtime.clone(), cx);

        let stale = self.pool_refresh_is_stale(MONITOR_POOL_REFRESH_INTERVAL);
        if force_pool_refresh || stale {
            self.refresh_pool_snapshot(cx);
        }
        if !visibility.is_visible() {
            // Hidden Host Tools stop page samplers only. The registry keeps
            // shared nodes, SFTP sessions, and forwarding tasks alive.
            self.stop_profiler_sampling();
            self.pause_service_refreshes();
            return;
        }

        let connections = self.monitor_connections();
        let selected_missing = self.selected_connection_id().is_none_or(|selected| {
            !connections
                .iter()
                .any(|connection| connection.connection_id == selected)
        });
        if force_pool_refresh || stale || selected_missing {
            self.sync_live_connections(connections, sampling_config, runtime, cx);
        }
    }

    pub(super) fn sync_live_connections(
        &mut self,
        connections: Vec<MonitorConnectionOption>,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let live_connection_ids = connections
            .iter()
            .map(|connection| connection.connection_id.as_str())
            .collect::<HashSet<_>>();
        for connection_id in self.profiler_connection_ids() {
            if !live_connection_ids.contains(connection_id.as_str()) {
                self.remove_profiler_connection(&connection_id);
            }
        }
        if connections.is_empty() {
            if let Some(connection_id) = self.take_selected_connection(cx) {
                self.remove_profiler_connection(&connection_id);
            }
            return;
        }

        let live_connection_ids = connections
            .into_iter()
            .map(|connection| connection.connection_id)
            .collect::<Vec<_>>();
        let Some(connection_id) = self.ensure_selected_connection(&live_connection_ids, cx) else {
            return;
        };
        if !self.visibility.is_visible() || sampling_config.is_empty() {
            self.stop_profiler_sampling();
            return;
        }
        if self.profiler_connection_missing(&connection_id) {
            self.start_profiler(connection_id, sampling_config, runtime, cx);
        }
    }

    pub(in crate::workspace) fn apply_monitoring_settings(
        &mut self,
        visibility: HostToolsVisibility,
        monitoring: oxideterm_settings::HostToolsSettings,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        self.visibility = visibility;
        self.monitoring = monitoring;
        self.sampling_config = sampling_config;
        self.lifecycle_runtime = Some(runtime.clone());
        if sampling_config.is_empty() || !visibility.is_visible() {
            self.stop_profiler_sampling();
        } else {
            for connection_id in self.profiler_connection_ids() {
                self.start_profiler(connection_id, sampling_config.clone(), runtime.clone(), cx);
            }
            self.sync_live_connections(
                self.monitor_connections(),
                sampling_config,
                runtime.clone(),
                cx,
            );
        }

        let gpu_visible = self.monitoring.gpu_enabled
            && visibility.sidebar_is_visible()
            && self.active_tool() == ContextSidebarTool::Gpu;
        let selected_connection_id = self.selected_connection_id_owned();
        self.sync_gpu_sampling(gpu_visible, selected_connection_id, runtime, cx);
    }
}

impl WorkspaceApp {
    pub(in crate::workspace::connection_monitor) fn host_tools_visibility(
        &self,
    ) -> HostToolsVisibility {
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

        host_tools_visibility(main_tab_visible, detached_tab_visible, sidebar_visible)
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
            let tab_id = self.alloc_tab_id(cx);
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
        self.sync_host_tools_lifecycle(true, cx);
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
        self.sync_host_tools_lifecycle(false, cx);
    }

    pub(in crate::workspace) fn sync_host_tools_lifecycle(
        &mut self,
        force_pool_refresh: bool,
        cx: &mut Context<Self>,
    ) {
        let visibility = self.host_tools_visibility();
        let monitoring = self.settings_store.settings().host_tools.clone();
        let sampling_config = self.resource_sampling_config();
        let runtime = self.forwarding_runtime.handle().clone();
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.update_lifecycle(
                visibility,
                monitoring,
                sampling_config,
                runtime,
                force_pool_refresh,
                cx,
            );
        });
    }

    pub(in crate::workspace) fn apply_host_tool_monitoring_settings(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let visibility = self.host_tools_visibility();
        let monitoring = self.settings_store.settings().host_tools.clone();
        let sampling_config = self.resource_sampling_config();
        let runtime = self.forwarding_runtime.handle().clone();
        let messages = HostToolsMessages::from_i18n(&self.i18n);
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.set_messages(messages);
            host_tools.apply_monitoring_settings(
                visibility,
                monitoring,
                sampling_config,
                runtime,
                cx,
            );
        });
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
    use super::{HostToolsVisibility, host_tools_visibility};

    #[test]
    fn host_tools_visibility_covers_main_detached_and_sidebar_surfaces() {
        assert_eq!(
            host_tools_visibility(true, false, false),
            HostToolsVisibility::VisibleMainTab
        );
        assert_eq!(
            host_tools_visibility(false, true, false),
            HostToolsVisibility::VisibleDetachedWindow
        );
        assert_eq!(
            host_tools_visibility(false, false, true),
            HostToolsVisibility::VisibleSidebar
        );
        assert!(matches!(
            host_tools_visibility(true, true, true),
            HostToolsVisibility::VisibleMultiple {
                main_tab: true,
                sidebar: true,
                detached_window: true,
            }
        ));
        assert_eq!(
            host_tools_visibility(false, false, false),
            HostToolsVisibility::Hidden
        );
        assert!(!HostToolsVisibility::Dropped.is_visible());
    }
}
