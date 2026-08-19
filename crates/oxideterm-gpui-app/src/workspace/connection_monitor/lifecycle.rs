use super::*;

fn is_host_tools_tab_kind(tab_kind: &TabKind) -> bool {
    matches!(
        tab_kind,
        TabKind::ConnectionPool | TabKind::Topology | TabKind::Runtime
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
    pub(super) fn schedule_lifecycle_refresh(&mut self, cx: &mut Context<Self>) {
        // Pool/topology freshness belongs to Host Tools and stops with the Entity.
        self.lifecycle_refresh_task = Some(cx.spawn(async move |host_tools, cx| {
            loop {
                Timer::after(MONITOR_POOL_REFRESH_INTERVAL).await;
                let should_continue = host_tools
                    .update(cx, |host_tools, cx| {
                        host_tools.refresh_lifecycle_tick(cx);
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        }));
    }

    pub(super) fn refresh_lifecycle_tick(&mut self, cx: &mut Context<Self>) {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        if !self.visibility.is_visible() {
            // Hidden pages keep shared nodes and long-running work, but do not
            // sample or repaint page-only snapshots.
            return;
        }
        self.update_lifecycle(
            self.visibility,
            self.monitoring.clone(),
            self.sampling_config,
            runtime,
            false,
            cx,
        );
    }

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
        if !visibility.sidebar_is_visible() {
            // Runtime tabs only need pool and topology snapshots. Resource
            // samplers belong to the mounted Host Tools sidebar.
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
        let profiler_missing = self
            .selected_connection_id()
            .is_some_and(|connection_id| self.profiler_connection_missing(connection_id));
        if force_pool_refresh || stale || selected_missing || profiler_missing {
            // Re-showing Host Tools must restart the page profiler even when
            // the selected connection and cached pool snapshot stayed stable.
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
        if !self.visibility.sidebar_is_visible() || sampling_config.is_empty() {
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
        if sampling_config.is_empty() || !visibility.sidebar_is_visible() {
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
    pub(in crate::workspace) fn host_tools_visibility(&self, cx: &App) -> HostToolsVisibility {
        let tab_host = self.tab_host.read(cx);
        let main_tab_visible = self.tabs(cx).iter().any(|tab| {
            is_host_tools_tab_kind(&tab.kind)
                && self.active_tab_id(cx) == Some(tab.id)
                && !tab_host.is_outside_main_window(tab.id)
        });
        let detached_tab_visible = self
            .tabs(cx)
            .iter()
            .any(|tab| is_host_tools_tab_kind(&tab.kind) && tab_host.is_detached(tab.id));
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
        let tab_id = if let Some(tab) = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.kind == TabKind::Runtime)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::Runtime,
                    title: self.i18n.t("sidebar.panels.runtime"),
                    title_source: TabTitleSource::I18nKey("sidebar.panels.runtime"),
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            tab_id
        };
        self.set_active_tab(tab_id, window, cx);
        self.sync_host_tools_lifecycle(true, cx);
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

    pub(in crate::workspace) fn sync_host_tools_lifecycle(
        &mut self,
        force_pool_refresh: bool,
        cx: &mut App,
    ) {
        let visibility = self.host_tools_visibility(cx);
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
        let visibility = self.host_tools_visibility(cx);
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
