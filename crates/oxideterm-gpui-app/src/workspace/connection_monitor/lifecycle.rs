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

impl WorkspaceApp {
    pub(in crate::workspace) fn host_tools_visibility(&self, cx: &App) -> HostToolsVisibility {
        let tab_host = self.tab_host.read(cx);
        let main_tab_visible = self.tabs(cx).iter().any(|tab| {
            is_host_tools_tab_kind(&tab.kind)
                && tab_host.surface_is_visible(tab.id)
                && !tab_host.is_detached(tab.id)
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
