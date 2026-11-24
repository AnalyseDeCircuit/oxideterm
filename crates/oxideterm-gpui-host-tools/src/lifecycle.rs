use super::*;

impl Drop for HostToolsEntity {
    fn drop(&mut self) {
        // A discovery probe can hold its SSH route for its full 40s timeout.
        // Releasing it here keeps Entity teardown from leaving that work behind.
        self.cancel_container_discovery();
    }
}

impl HostToolsEntity {
    /// Container discovery is host-scoped state. Retire the previous host's
    /// engines as soon as the selection moves, so neither a panel render nor a
    /// command builder can resolve an instance that belongs to another host,
    /// and cancel the superseded probe instead of letting it hold an SSH route
    /// for its full timeout. The task handle dies with the Entity too.
    fn retire_foreign_container_state(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_connection_id().map(str::to_owned);
        let foreign = |owner: Option<&str>| match (owner, selected.as_deref()) {
            (Some(owner), Some(selected)) => owner != selected,
            _ => false,
        };
        let discovery_foreign = foreign(self.host_container.discovery_connection_id.as_deref());
        let snapshot_foreign = foreign(self.host_container.snapshot_connection_id.as_deref());
        if !discovery_foreign && !snapshot_foreign {
            return;
        }
        if discovery_foreign {
            // The probe belongs to another host: aborting it releases the SSH
            // route immediately, and the reset marker lets the current host
            // start from scratch.
            self.cancel_container_discovery();
            self.host_container.discovery_connection_id = None;
            self.host_container.discovery = None;
            self.host_container.selected_instance_id = None;
            self.host_container.selected_row = None;
            // Inventory tabs describe the retired engine; Containers is the
            // only tab every engine supports.
            self.host_container.tab = HostContainerTab::Containers;
            // A confirm or capture dialog was composed for the retired host.
            self.dismiss_container_confirm(cx);
            self.dismiss_container_text_dialog(cx);
            self.dismiss_container_input_dialog(cx);
        }
        if discovery_foreign || snapshot_foreign {
            self.host_container.snapshot = None;
            self.host_container.snapshot_connection_id = None;
            self.host_container.stats = None;
            self.host_container.registries = None;
        }
        cx.notify();
    }

    pub(crate) fn schedule_lifecycle_refresh(&mut self, cx: &mut Context<Self>) {
        // Pool/topology freshness belongs to Host Tools and stops with the Entity.
        let executor = cx.background_executor().clone();
        self.lifecycle_refresh_task = Some(cx.spawn(async move |host_tools, cx| {
            loop {
                executor.timer(MONITOR_POOL_REFRESH_INTERVAL).await;
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

    pub(crate) fn refresh_lifecycle_tick(&mut self, cx: &mut Context<Self>) {
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

    pub fn set_messages(&mut self, messages: HostToolsMessages) {
        self.messages = Some(messages);
    }

    pub fn update_lifecycle(
        &mut self,
        visibility: HostToolsVisibility,
        monitoring: oxideterm_settings::HostToolsSettings,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: HostToolsRuntime,
        force_pool_refresh: bool,
        cx: &mut Context<Self>,
    ) {
        self.visibility = visibility;
        self.monitoring = monitoring;
        self.sampling_config = sampling_config;
        self.lifecycle_runtime = Some(runtime.clone());
        self.retire_foreign_container_state(cx);
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
            if selected_missing
                && self.active_tool() == ContextSidebarTool::Tmux
                && self.host_tmux.mux.provider.is_some()
            {
                self.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
            }
        }
    }

    pub(crate) fn sync_live_connections(
        &mut self,
        connections: Vec<MonitorConnectionOption>,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: HostToolsRuntime,
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
        // The selection can move here when the previous host disappears, which
        // happens after the lifecycle refresh already retired container state.
        // Retire again before the probe flips the discovery marker to this
        // connection, otherwise the previous host's engines would be shown as
        // this host's until the new result lands.
        self.retire_foreign_container_state(cx);
        if !self.visibility.sidebar_is_visible() || sampling_config.is_empty() {
            self.stop_profiler_sampling();
            return;
        }
        if self.profiler_connection_missing(&connection_id) {
            self.start_profiler(connection_id, sampling_config, runtime, cx);
        }
        if self.active_tool() == ContextSidebarTool::Container {
            // Detection is one-shot per connection; the panel refreshes it on
            // demand and after every mutation.
            self.request_container_discovery_if_needed(cx);
        }
    }

    pub fn apply_monitoring_settings(
        &mut self,
        visibility: HostToolsVisibility,
        monitoring: oxideterm_settings::HostToolsSettings,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: HostToolsRuntime,
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
                self.start_profiler(connection_id, sampling_config, runtime.clone(), cx);
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
