use super::*;

use gpui::Task;
use oxideterm_connection_monitor::ResourceSampler;
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_topology::ConnectionTopologySnapshot;

/// Owns Host Tools sampling state independently from WorkspaceApp and SSH nodes.
pub(in crate::workspace) struct HostToolsEntity {
    // This handle is private to the Entity. Host Tools exposes snapshots and
    // sampler acquisition only; page code cannot disconnect shared nodes.
    ssh_registry: SshConnectionRegistry,
    pub(super) profiler_registry: ProfilerRegistry,
    pub(super) profiler_update_tx: tokio::sync::mpsc::UnboundedSender<ProfilerUpdate>,
    pub(super) sampler_delivery_wake: crate::workspace::delivery::ActiveDeliveryWake,
    pub(super) sampler_delivery_rx:
        std::sync::mpsc::Receiver<super::delivery::HostToolsSamplerDelivery>,
    // Reliable user-action results have their own wake and budget so sampler
    // traffic cannot delay a completed Host Tools operation.
    pub(super) reliable_delivery_wake: crate::workspace::delivery::ActiveDeliveryWake,
    pub(super) reliable_delivery_tx: crate::workspace::delivery::ActiveDeliverySender<
        super::delivery::HostToolsReliableDelivery,
    >,
    pub(super) reliable_delivery_rx:
        std::sync::mpsc::Receiver<super::delivery::HostToolsReliableDelivery>,
    // Search, list, expansion, and dialog state belongs to the shared Host
    // Tools surface rather than whichever workspace mount renders it.
    pub(in crate::workspace) ui: HostToolsUiState,
    pub(super) host_process_actions: HostProcessActionsState,
    pub(super) host_docker_operations: HostDockerOperationsState,
    pub(super) host_services: HostServicesState,
    pub(super) host_tmux: HostTmuxState,
    pub(super) host_gpu: HostGpuViewState,
    pub(super) host_logs: HostLogsState,
    pub(super) host_ports: HostPortsState,
    pub(super) host_filesystems: HostFilesystemsState,
    pub(super) host_packages: HostPackagesState,
    pub(super) host_schedules: HostSchedulesState,
    pub(in crate::workspace) active_runtime_section: ConnectionRuntimeSection,
    pub(in crate::workspace) previous_runtime_section: ConnectionRuntimeSection,
    selected_connection_id: Option<String>,
    selector_open: bool,
    selector_highlighted_index: Option<usize>,
    selector_focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
    tab_scroll_handle: ScrollHandle,
    // Secondary navigation and its pointer capture remain stable across every
    // Host Tools mount point.
    active_tool: ContextSidebarTool,
    previous_tool: ContextSidebarTool,
    tab_scrollbar_drag: Option<HostToolsTabScrollbarDragState>,
    // Visibility is an Entity lifecycle input. It controls only page-scoped
    // sampling and never owns or releases shared SSH transports.
    pub(super) visibility: HostToolsVisibility,
    // These inputs let the Entity refresh its own samplers after an action.
    // The runtime handle has no registry release or disconnect capability.
    pub(super) lifecycle_runtime: Option<tokio::runtime::Handle>,
    pub(super) sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
    // Host Tools receives one read-only settings snapshot. Persistent writes
    // remain owned by SettingsStore and are applied back through lifecycle.
    pub(super) monitoring: oxideterm_settings::HostToolsSettings,
    pub(super) messages: Option<HostToolsMessages>,
    pub(super) lifecycle_refresh_task: Option<Task<()>>,
    #[cfg(test)]
    test_resource_sampler: Option<Arc<dyn ResourceSampler>>,
    #[cfg(test)]
    test_snapshot_dispatches: Option<Vec<ContextSidebarTool>>,
    pool_stats: Option<ConnectionPoolMonitorStats>,
    pool_summaries: Vec<ConnectionPoolEntrySummary>,
    topology_snapshot: Option<ConnectionTopologySnapshot>,
    last_pool_refresh: Option<Instant>,
    // Topology interactions belong to the shared Host Tools surface, not to
    // the workspace window that happens to render the graph.
    pub(super) topology_transform: TopologyTransform,
    pub(super) topology_drag: Option<TopologyDragState>,
    pub(super) topology_menu: Option<TopologyNodeMenuState>,
    compact_monitor_list_state: ListState,
    compact_monitor_list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostToolsEntity {
    pub(in crate::workspace) fn execute_ai_action(
        &mut self,
        connection_id: String,
        resource: &str,
        action: &str,
        entity_id: &str,
        value: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let runtime = self
            .lifecycle_runtime
            .clone()
            .ok_or_else(|| "The Host Tools runtime is unavailable.".to_string())?;
        let notices = match resource {
            "process" => {
                let signal = value.unwrap_or("term").trim().to_ascii_lowercase();
                let action = match (action, signal.as_str()) {
                    ("signal", "term") => ProcessActionKind::Term,
                    ("signal", "kill") => ProcessActionKind::Kill,
                    ("signal", "stop") => ProcessActionKind::Stop,
                    ("signal", "cont") => ProcessActionKind::Cont,
                    _ => return Err("The requested process signal is unsupported.".to_string()),
                };
                self.start_process_action(
                    HostProcessActionRun {
                        connection_id,
                        pid: entity_id.to_string(),
                        action,
                    },
                    runtime,
                    cx,
                )
            }
            "docker" => {
                let action = match action {
                    "start" => DockerActionKind::Start,
                    "stop" => DockerActionKind::Stop,
                    "restart" => DockerActionKind::Restart,
                    _ => return Err("The requested Docker action is unsupported.".to_string()),
                };
                self.start_docker_action(
                    HostDockerActionRequest {
                        connection_id,
                        container_id: entity_id.to_string(),
                        container_name: entity_id.to_string(),
                        action,
                    },
                    runtime,
                    cx,
                )
            }
            "service" => {
                let action = match action {
                    "start" => ServiceActionKind::Start,
                    "stop" => ServiceActionKind::Stop,
                    "restart" => ServiceActionKind::Restart,
                    "enable" => ServiceActionKind::Enable,
                    "disable" => ServiceActionKind::Disable,
                    _ => return Err("The requested service action is unsupported.".to_string()),
                };
                self.start_service_action(
                    HostServiceActionRequest {
                        connection_id,
                        service_id: entity_id.to_string(),
                        description: entity_id.to_string(),
                        action,
                    },
                    runtime,
                    cx,
                )
            }
            "tmux" => {
                let request = HostTmuxActionRun {
                    connection_id: connection_id.clone(),
                    session_id: entity_id.to_string(),
                    session_name: entity_id.to_string(),
                    target_label: entity_id.to_string(),
                };
                match action {
                    "kill_session" | "kill_window" | "kill_pane" => {
                        let destructive_action = match action {
                            "kill_session" => HostTmuxDestructiveAction::KillSession {
                                target: entity_id.to_string(),
                            },
                            "kill_window" => HostTmuxDestructiveAction::KillWindow {
                                target: entity_id.to_string(),
                            },
                            _ => HostTmuxDestructiveAction::KillPane {
                                target: entity_id.to_string(),
                            },
                        };
                        self.start_tmux_action(
                            HostTmuxActionRequest {
                                connection_id,
                                session_id: entity_id.to_string(),
                                session_name: entity_id.to_string(),
                                target_label: entity_id.to_string(),
                                action: destructive_action,
                            },
                            runtime,
                            cx,
                        )
                    }
                    "rename_session" | "rename_window" | "send_pane_command" => {
                        let value = value
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| {
                                "The requested tmux action requires a value.".to_string()
                            })?;
                        let os_type = self.connection_os_type(&connection_id).ok_or_else(|| {
                            "The Host Tools connection is unavailable.".to_string()
                        })?;
                        let command = match action {
                            "rename_session" => {
                                build_tmux_rename_session_command(&os_type, entity_id, value)
                            }
                            "rename_window" => {
                                build_tmux_rename_window_command(&os_type, entity_id, value)
                            }
                            _ => build_tmux_send_pane_command(&os_type, entity_id, value),
                        }
                        .map_err(|_| "The requested tmux action is invalid.".to_string())?;
                        self.start_tmux_action_command(command, request, runtime, cx)
                    }
                    _ => return Err("The requested tmux action is unsupported.".to_string()),
                }
            }
            "schedule" => {
                let task = self
                    .host_schedules
                    .snapshot
                    .as_ref()
                    .filter(|_| {
                        self.host_schedules.snapshot_connection_id.as_deref()
                            == Some(connection_id.as_str())
                    })
                    .and_then(|snapshot| snapshot.entries.iter().find(|task| task.id == entity_id))
                    .cloned()
                    .ok_or_else(|| {
                        "Refresh scheduled tasks before controlling this task.".to_string()
                    })?;
                let scheduled_action = match action {
                    "run" => ScheduledTaskActionKind::RunNow {
                        id: task.id.clone(),
                        unit: task.unit.clone(),
                    },
                    "enable" => ScheduledTaskActionKind::Enable {
                        id: task.id.clone(),
                        source: task.source.clone(),
                    },
                    "disable" => ScheduledTaskActionKind::Disable {
                        id: task.id.clone(),
                        source: task.source.clone(),
                    },
                    _ => {
                        return Err(
                            "The requested scheduled task action is unsupported.".to_string()
                        );
                    }
                };
                self.start_schedule_action(
                    HostScheduleActionRequest {
                        connection_id,
                        task_id: task.id,
                        task_name: task.name,
                        unit: task.unit,
                        action: scheduled_action,
                    },
                    runtime,
                    cx,
                )
            }
            _ => return Err("The requested Host Tools resource is unsupported.".to_string()),
        };
        for notice in notices {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
        Ok(())
    }

    pub(in crate::workspace) fn ai_snapshot(
        &self,
        connection_id: &str,
        resource: &str,
    ) -> serde_json::Value {
        let latest_metrics = self.profiler_registry.latest(connection_id);
        match resource {
            "overview" => latest_metrics
                .map(|metrics| serde_json::to_value(metrics).unwrap_or_default())
                .unwrap_or(serde_json::Value::Null),
            "processes" => latest_metrics
                .map(|metrics| serde_json::to_value(metrics.top_processes).unwrap_or_default())
                .unwrap_or(serde_json::Value::Null),
            "docker" => latest_metrics
                .map(|metrics| serde_json::to_value(metrics.docker).unwrap_or_default())
                .unwrap_or(serde_json::Value::Null),
            "services" => self
                .host_services
                .snapshot
                .as_ref()
                .filter(|_| {
                    self.host_services.snapshot_connection_id.as_deref() == Some(connection_id)
                })
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "tmux" => self
                .host_tmux
                .snapshot
                .as_ref()
                .filter(|_| self.host_tmux.snapshot_connection_id.as_deref() == Some(connection_id))
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "ports" => self
                .host_ports
                .snapshot
                .as_ref()
                .filter(|_| {
                    self.host_ports.snapshot_connection_id.as_deref() == Some(connection_id)
                })
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "filesystems" => self
                .host_filesystems
                .snapshot
                .as_ref()
                .filter(|_| {
                    self.host_filesystems.snapshot_connection_id.as_deref() == Some(connection_id)
                })
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "packages" => self
                .host_packages
                .snapshot
                .as_ref()
                .filter(|_| {
                    self.host_packages.snapshot_connection_id.as_deref() == Some(connection_id)
                })
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "schedules" => self
                .host_schedules
                .snapshot
                .as_ref()
                .filter(|_| {
                    self.host_schedules.snapshot_connection_id.as_deref() == Some(connection_id)
                })
                .and_then(|snapshot| serde_json::to_value(snapshot).ok())
                .unwrap_or(serde_json::Value::Null),
            "logs" => self
                .log_snapshot_for(connection_id)
                .map(|snapshot| {
                    // AI receives bounded log metadata, never raw log messages.
                    let mut levels = std::collections::BTreeSet::new();
                    let mut sources = std::collections::BTreeSet::new();
                    let mut units = std::collections::BTreeSet::new();
                    for entry in &snapshot.entries {
                        if !entry.level.trim().is_empty() {
                            levels.insert(entry.level.clone());
                        }
                        if !entry.source.trim().is_empty() {
                            sources.insert(entry.source.clone());
                        }
                        if !entry.unit.trim().is_empty() {
                            units.insert(entry.unit.clone());
                        }
                    }
                    serde_json::json!({
                        "status": snapshot.status,
                        "entryCount": snapshot.entries.len(),
                        "levels": levels.into_iter().take(32).collect::<Vec<_>>(),
                        "sources": sources.into_iter().take(32).collect::<Vec<_>>(),
                        "units": units.into_iter().take(32).collect::<Vec<_>>(),
                    })
                })
                .unwrap_or(serde_json::Value::Null),
            _ => serde_json::Value::Null,
        }
    }

    pub(in crate::workspace) fn request_ai_snapshot_refresh(
        &mut self,
        connection_id: String,
        resource: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return false;
        };
        let Some(messages) = self.messages.clone() else {
            return false;
        };
        match resource {
            "overview" | "processes" | "docker" => {
                if !self.visibility.sidebar_is_visible() {
                    // Resource snapshots are scoped to the mounted Host Tools sidebar.
                    return false;
                }
                self.start_profiler(connection_id, self.sampling_config, runtime, cx);
            }
            "services" => {
                self.request_service_snapshot(
                    connection_id,
                    runtime,
                    messages.service_connection_missing,
                    messages.service_action_failed,
                    cx,
                );
            }
            "tmux" => {
                for notice in self.request_tmux_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    String::new(),
                    messages.tmux_unknown_error,
                    messages.tmux_unavailable,
                    runtime,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            "ports" => {
                for notice in self.request_port_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    true,
                    runtime,
                    messages.port_unknown_error,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            "filesystems" => {
                for notice in self.request_filesystem_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    true,
                    runtime,
                    messages.filesystem_unknown_error,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            "packages" => {
                for notice in self.request_package_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    true,
                    runtime,
                    messages.package_unknown_error,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            "schedules" => {
                for notice in self.request_schedule_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    true,
                    runtime,
                    messages.schedule_unknown_error,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            "logs" => {
                for notice in self.request_log_snapshot(
                    connection_id,
                    HostSnapshotFeedback::Silent,
                    self.monitoring.logs_enabled,
                    runtime,
                    messages.log_unknown_error,
                    cx,
                ) {
                    cx.emit(HostToolsEvent::ShowNotice(notice));
                }
            }
            _ => return false,
        }
        true
    }

    pub(in crate::workspace) fn window_modal_snapshot(
        &self,
    ) -> Option<HostToolsWindowModalSnapshot> {
        if self.host_schedules.logs_dialog.is_some() {
            Some(HostToolsWindowModalSnapshot::ScheduleLogs)
        } else if let Some(confirm) = self.host_schedules.pending_confirm.as_ref() {
            Some(HostToolsWindowModalSnapshot::ScheduleConfirm(
                confirm.presence.phase(),
            ))
        } else if self.ui.host_tmux_input_dialog.is_some() {
            Some(HostToolsWindowModalSnapshot::TmuxInput)
        } else if let Some(confirm) = self.host_tmux.pending_confirm.as_ref() {
            Some(HostToolsWindowModalSnapshot::TmuxConfirm(
                confirm.presence.phase(),
            ))
        } else if self.host_services.logs_dialog.is_some() {
            Some(HostToolsWindowModalSnapshot::ServiceLogs)
        } else if let Some(confirm) = self.host_services.pending_confirm.as_ref() {
            Some(HostToolsWindowModalSnapshot::ServiceConfirm(
                confirm.presence.phase(),
            ))
        } else if self.host_docker_operations.logs_dialog.is_some() {
            Some(HostToolsWindowModalSnapshot::DockerLogs)
        } else if let Some(confirm) = self.host_docker_operations.pending_confirm.as_ref() {
            Some(HostToolsWindowModalSnapshot::DockerConfirm(
                confirm.presence.phase(),
            ))
        } else {
            self.host_process_actions
                .pending_confirm
                .as_ref()
                .map(|confirm| {
                    HostToolsWindowModalSnapshot::ProcessConfirm(confirm.presence.phase())
                })
        }
    }

    pub(in crate::workspace) fn new(
        profiler_update_tx: tokio::sync::mpsc::UnboundedSender<ProfilerUpdate>,
        profiler_update_rx: tokio::sync::mpsc::UnboundedReceiver<ProfilerUpdate>,
        ssh_registry: SshConnectionRegistry,
        cx: &mut Context<Self>,
    ) -> Self {
        let sampler_delivery_wake = crate::workspace::delivery::ActiveDeliveryWake::default();
        let (sampler_delivery_tx, sampler_delivery_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel_with_wake(
                sampler_delivery_wake.clone(),
            );
        let (gpu_update_tx, gpu_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let reliable_delivery_wake = crate::workspace::delivery::ActiveDeliveryWake::default();
        let (reliable_delivery_tx, reliable_delivery_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel_with_wake(
                reliable_delivery_wake.clone(),
            );
        let mut entity = Self {
            ssh_registry,
            profiler_registry: ProfilerRegistry::new(),
            profiler_update_tx,
            sampler_delivery_wake,
            sampler_delivery_rx,
            reliable_delivery_wake,
            reliable_delivery_tx,
            reliable_delivery_rx,
            ui: HostToolsUiState::new(),
            host_process_actions: HostProcessActionsState::new(),
            host_docker_operations: HostDockerOperationsState::new(),
            host_services: HostServicesState::new(),
            host_tmux: HostTmuxState::new(),
            host_gpu: HostGpuViewState::new(gpu_update_tx),
            host_logs: HostLogsState::new(),
            host_ports: HostPortsState::new(),
            host_filesystems: HostFilesystemsState::new(),
            host_packages: HostPackagesState::new(),
            host_schedules: HostSchedulesState::new(),
            active_runtime_section: ConnectionRuntimeSection::Overview,
            previous_runtime_section: ConnectionRuntimeSection::Overview,
            selected_connection_id: None,
            selector_open: false,
            selector_highlighted_index: None,
            selector_focus_origin: None,
            tab_scroll_handle: ScrollHandle::new(),
            active_tool: ContextSidebarTool::Monitor,
            previous_tool: ContextSidebarTool::Monitor,
            tab_scrollbar_drag: None,
            visibility: HostToolsVisibility::Hidden,
            lifecycle_runtime: None,
            sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig::default(),
            monitoring: oxideterm_settings::HostToolsSettings::default(),
            messages: None,
            lifecycle_refresh_task: None,
            #[cfg(test)]
            test_resource_sampler: None,
            #[cfg(test)]
            test_snapshot_dispatches: None,
            pool_stats: None,
            pool_summaries: Vec::new(),
            topology_snapshot: None,
            last_pool_refresh: None,
            topology_transform: TopologyTransform::default(),
            topology_drag: None,
            topology_menu: None,
            compact_monitor_list_state: tauri_virtual_list_state(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(COMPACT_MONITOR_LIST_ESTIMATED_ROW_HEIGHT),
                    COMPACT_MONITOR_LIST_OVERSCAN,
                ),
            ),
            compact_monitor_list_cache: RefCell::new(VirtualListSignatureCache::default()),
        };
        entity.schedule_sampler_delivery(
            super::delivery::HostToolsDeliveryBridges {
                profiler_update_rx,
                gpu_update_rx,
                sampler_delivery_tx,
            },
            cx,
        );
        entity.schedule_reliable_delivery(cx);
        entity.schedule_lifecycle_refresh(cx);
        entity
    }

    pub(in crate::workspace) fn replace_text_input(
        &mut self,
        input: HostToolsTextInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
    ) -> bool {
        let Some(value) = self.ui.input_value_mut(input) else {
            return false;
        };
        replace_utf16(value, replacement_range, text);
        // Search edits invalidate expansion identities owned by the same
        // surface; command inputs preserve their surrounding action state.
        match input {
            HostToolsTextInput::ProcessSearch => self.ui.host_process_expanded_pid = None,
            HostToolsTextInput::DockerSearch => self.ui.host_docker_expanded_id = None,
            HostToolsTextInput::ServiceSearch => self.ui.host_service_expanded_id = None,
            HostToolsTextInput::LogSearch => self.host_logs.expanded_index = None,
            HostToolsTextInput::TmuxSearch => {
                self.ui.host_tmux_expanded_session_id = None;
                self.ui.host_tmux_expanded_window_id = None;
            }
            HostToolsTextInput::PortSearch => self.host_ports.expanded_index = None,
            HostToolsTextInput::ScheduleSearch => self.host_schedules.expanded_index = None,
            HostToolsTextInput::FilesystemSearch => self.host_filesystems.expanded_index = None,
            HostToolsTextInput::PackageSearch => self.host_packages.expanded_index = None,
            HostToolsTextInput::ProcessRenice | HostToolsTextInput::TmuxDialog => {}
        }
        true
    }

    pub(in crate::workspace) fn profiler_registry(&self) -> &ProfilerRegistry {
        &self.profiler_registry
    }

    pub(in crate::workspace) fn refresh_pool_snapshot(&mut self, cx: &mut Context<Self>) {
        self.pool_stats = Some(self.ssh_registry.monitor_stats());
        self.pool_summaries = self.ssh_registry.list_connection_summaries();
        self.topology_snapshot = Some(self.ssh_registry.connection_topology_snapshot());
        self.last_pool_refresh = Some(Instant::now());
        cx.notify();
    }

    pub(super) fn pool_refresh_is_stale(&self, interval: Duration) -> bool {
        self.last_pool_refresh
            .is_none_or(|last_refresh| last_refresh.elapsed() >= interval)
    }

    pub(super) fn pool_stats_snapshot(&self) -> Option<ConnectionPoolMonitorStats> {
        self.pool_stats.clone()
    }

    pub(super) fn pool_summaries_snapshot(&self) -> Vec<ConnectionPoolEntrySummary> {
        // Runtime views receive an immutable projection instead of borrowing
        // the registry-owned cache across GPUI rendering callbacks.
        self.pool_summaries.clone()
    }

    pub(super) fn topology_snapshot(&self) -> Option<ConnectionTopologySnapshot> {
        self.topology_snapshot.clone()
    }

    pub(super) fn monitor_connections(&self) -> Vec<MonitorConnectionOption> {
        if !self.pool_summaries.is_empty() {
            return self
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

    pub(super) fn connection_os_type(&self, connection_id: &str) -> Option<String> {
        self.ssh_registry.get(connection_id).map(|handle| {
            handle
                .remote_env()
                .map(|environment| environment.os_type)
                .unwrap_or_else(|| "Unknown".to_string())
        })
    }

    pub(super) fn spawn_log_snapshot_capture(
        &self,
        command: String,
        request: HostLogSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Only the generated log command crosses the task boundary. The
        // registry-owned handle keeps credentials and node lifetime private.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::LogSnapshot(
                HostLogSnapshotDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_port_snapshot_capture(
        &self,
        command: String,
        request: HostPortSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // The registry handle keeps authentication and node ownership inside
        // the SSH runtime while the generated diagnostic command runs.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::PortSnapshot(
                HostPortSnapshotDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_filesystem_snapshot_capture(
        &self,
        command: String,
        request: HostFilesystemSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Filesystem capture uses the registry-owned transport without moving
        // credentials or node lifecycle control into the page task.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(
                super::delivery::HostToolsReliableDelivery::FilesystemSnapshot(
                    HostFilesystemSnapshotDelivery { request, result },
                ),
            );
        });
        true
    }

    pub(super) fn spawn_package_snapshot_capture(
        &self,
        command: String,
        request: HostPackageSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Package inventory uses the registry-owned transport and cannot
        // release the shared node or inspect its authentication material.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::PackageSnapshot(
                HostPackageSnapshotDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_schedule_snapshot_capture(
        &self,
        command: String,
        request: HostScheduleSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Scheduled-task inventory runs on the registry-owned transport and
        // cannot acquire node release or authentication capabilities.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(
                super::delivery::HostToolsReliableDelivery::ScheduleSnapshot(
                    HostScheduleSnapshotDelivery { request, result },
                ),
            );
        });
        true
    }

    pub(super) fn spawn_schedule_logs_capture(
        &self,
        command: String,
        request: HostScheduleLogsRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Raw log output stays inside the Entity delivery and dialog state.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ScheduleLogs(
                HostScheduleLogsDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_schedule_action(
        &self,
        command: String,
        request: HostScheduleActionRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Action output is reduced to a success bit before it enters delivery.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map(|mut output| {
                    let succeeded = output.exit_code.unwrap_or(0) == 0;
                    // Captured action text may contain credentials and has no UI consumer.
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    succeeded
                })
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ScheduleAction(
                HostScheduleActionDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_process_action(
        &self,
        command: String,
        request: HostProcessActionRun,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Remote process output has no UI consumer. Reduce it in the worker
        // and clear both buffers before crossing the Entity delivery boundary.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map(|mut output| {
                    let succeeded = output.exit_code.unwrap_or(0) == 0;
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    succeeded
                })
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ProcessAction(
                HostProcessActionDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_docker_action(
        &self,
        command: String,
        request: HostDockerActionRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Docker action output is not product data. Clear it before delivery.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map(|mut output| {
                    let succeeded = output.exit_code.unwrap_or(0) == 0;
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    succeeded
                })
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::DockerAction(
                HostDockerActionDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_docker_logs_capture(
        &self,
        command: String,
        request: HostDockerLogsRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Raw logs remain inside the Entity delivery and dialog state.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::DockerLogs(
                HostDockerLogsDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_service_snapshot_capture(
        &self,
        command: String,
        request: HostServiceSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Inventory output stays inside the Entity and is parsed before render.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ServiceSnapshot(
                HostServiceSnapshotDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_service_action(
        &self,
        command: String,
        request: HostServiceActionRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Service action output has no UI consumer and is cleared in the worker.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map(|mut output| {
                    let succeeded = output.exit_code.unwrap_or(0) == 0;
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    succeeded
                })
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ServiceAction(
                HostServiceActionDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_service_logs_capture(
        &self,
        command: String,
        request: HostServiceLogsRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Raw service logs never cross the typed workspace event boundary.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ServiceLogs(
                HostServiceLogsDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_tmux_snapshot_capture(
        &self,
        command: String,
        request: HostTmuxSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Raw tmux inventory stays inside the Entity delivery boundary.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::TmuxSnapshot(
                HostTmuxSnapshotDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn spawn_tmux_action(
        &self,
        command: zeroize::Zeroizing<String>,
        request: HostTmuxActionRun,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // The generated command is moved once into the worker. Captured output
        // is cleared before only a success bit crosses delivery.
        runtime.spawn(async move {
            let result = handle
                .run_command_capture(command.as_str(), timeout, max_output_size)
                .await
                .map(|mut output| {
                    let succeeded = output.exit_code.unwrap_or(0) == 0;
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    succeeded
                })
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::TmuxAction(
                HostTmuxActionDelivery { request, result },
            ));
        });
        true
    }

    pub(super) fn compact_monitor_list_state(&self) -> ListState {
        self.compact_monitor_list_state.clone()
    }

    pub(super) fn sync_compact_monitor_list_signatures(&self, identity: &str, signatures: &[u64]) {
        sync_tauri_variable_list_state_by_signatures(
            &self.compact_monitor_list_state,
            &mut self.compact_monitor_list_cache.borrow_mut(),
            identity,
            signatures,
            TauriVirtualListSpec::new(
                px(COMPACT_MONITOR_LIST_ESTIMATED_ROW_HEIGHT),
                COMPACT_MONITOR_LIST_OVERSCAN,
            ),
        );
    }

    pub(super) fn request_profiler_refresh(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        if !self.visibility.sidebar_is_visible() || self.sampling_config.is_empty() {
            return;
        }
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        self.start_profiler(connection_id, self.sampling_config, runtime, cx);
    }

    pub(super) fn set_runtime_section(&mut self, section: ConnectionRuntimeSection) -> bool {
        if self.active_runtime_section == section {
            return false;
        }
        self.previous_runtime_section = self.active_runtime_section;
        self.active_runtime_section = section;
        true
    }

    pub(in crate::workspace) fn reset_runtime_section(&mut self) {
        self.active_runtime_section = ConnectionRuntimeSection::Overview;
        self.previous_runtime_section = ConnectionRuntimeSection::Overview;
    }

    pub(in crate::workspace) fn selected_connection_id(&self) -> Option<&str> {
        self.selected_connection_id.as_deref()
    }

    pub(super) fn tab_scroll_handle(&self) -> ScrollHandle {
        self.tab_scroll_handle.clone()
    }

    pub(in crate::workspace) fn active_tool(&self) -> ContextSidebarTool {
        self.active_tool
    }

    pub(super) fn previous_tool(&self) -> ContextSidebarTool {
        self.previous_tool
    }

    pub(in crate::workspace) fn select_tool(
        &mut self,
        tool: ContextSidebarTool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.active_tool == tool {
            return false;
        }
        self.previous_tool = self.active_tool;
        self.active_tool = tool;
        cx.notify();
        true
    }

    pub(super) fn select_sidebar_tool(
        &mut self,
        tool: ContextSidebarTool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.select_tool(tool, cx) {
            return false;
        }
        self.ui.retain_input_focus_for_tool(tool);
        if tool != ContextSidebarTool::Processes {
            self.dismiss_process_confirm(cx);
        }
        if tool != ContextSidebarTool::Docker {
            self.dismiss_docker_confirm(cx);
        }
        if tool != ContextSidebarTool::Services {
            self.dismiss_service_confirm(cx);
            self.pause_service_refreshes();
        }
        if tool != ContextSidebarTool::Tmux {
            self.dismiss_tmux_confirm(cx);
            self.dismiss_tmux_input_dialog(cx);
        }
        if tool != ContextSidebarTool::Schedules {
            self.dismiss_schedule_confirm(cx);
        }

        if let Some(runtime) = self.lifecycle_runtime.clone() {
            // Tool selection replaces only page samplers. Long-running SSH,
            // transfer, and forwarding ownership remains outside this Entity.
            self.update_lifecycle(
                self.visibility,
                self.monitoring.clone(),
                self.sampling_config,
                runtime,
                true,
                cx,
            );
            self.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
        }
        cx.emit(HostToolsEvent::ToolSelected(tool));
        true
    }

    pub(in crate::workspace) fn reset_active_tool(&mut self, cx: &mut Context<Self>) -> bool {
        self.select_tool(ContextSidebarTool::Monitor, cx)
    }

    pub(super) fn tab_scrollbar_drag_active(&self) -> bool {
        self.tab_scrollbar_drag.is_some()
    }

    pub(super) fn tab_scrollbar_grab_offset(&self) -> Option<f32> {
        self.tab_scrollbar_drag.map(|drag| drag.grab_offset_x)
    }

    pub(super) fn begin_tab_scrollbar_drag(&mut self, grab_offset_x: f32, cx: &mut Context<Self>) {
        self.tab_scrollbar_drag = Some(HostToolsTabScrollbarDragState { grab_offset_x });
        cx.notify();
    }

    pub(in crate::workspace) fn finish_tab_scrollbar_drag(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.tab_scrollbar_drag.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn selected_connection_id_owned(&self) -> Option<String> {
        self.selected_connection_id.clone()
    }

    pub(super) fn selector_open(&self) -> bool {
        self.selector_open
    }

    pub(super) fn selector_highlighted_index(&self) -> Option<usize> {
        self.selector_highlighted_index
    }

    pub(super) fn selector_focus_origin(&self) -> Option<browser_behavior::BrowserFocusOrigin> {
        self.selector_focus_origin
    }

    pub(in crate::workspace) fn close_selector(
        &mut self,
        clear_focus: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.selector_open
            || self.selector_highlighted_index.is_some()
            || (clear_focus && self.selector_focus_origin.is_some());
        self.selector_open = false;
        self.selector_highlighted_index = None;
        if clear_focus {
            self.selector_focus_origin = None;
        }
        if changed {
            cx.notify();
        }
        changed
    }

    pub(super) fn toggle_selector_from_pointer(
        &mut self,
        selected_index: usize,
        cx: &mut Context<Self>,
    ) {
        self.selector_focus_origin = Some(browser_behavior::BrowserFocusOrigin::Pointer);
        if self.selector_open {
            self.selector_open = false;
            self.selector_highlighted_index = None;
        } else {
            self.selector_open = true;
            self.selector_highlighted_index = Some(selected_index);
        }
        cx.notify();
    }

    pub(super) fn highlight_selector_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.selector_highlighted_index != Some(index) {
            self.selector_highlighted_index = Some(index);
            cx.notify();
        }
    }

    pub(super) fn focus_selector_trigger(&mut self, cx: &mut Context<Self>) {
        self.selector_focus_origin = Some(browser_behavior::BrowserFocusOrigin::Keyboard);
        cx.notify();
    }

    pub(super) fn open_selector_from_keyboard(
        &mut self,
        selected_index: usize,
        cx: &mut Context<Self>,
    ) {
        self.selector_open = true;
        self.selector_highlighted_index = Some(selected_index);
        self.selector_focus_origin = Some(browser_behavior::BrowserFocusOrigin::Keyboard);
        cx.notify();
    }

    pub(super) fn close_selector_to_keyboard_trigger(&mut self, cx: &mut Context<Self>) {
        self.selector_open = false;
        self.selector_highlighted_index = None;
        self.selector_focus_origin = Some(browser_behavior::BrowserFocusOrigin::Keyboard);
        cx.notify();
    }

    pub(super) fn highlight_selector_from_keyboard(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.selector_highlighted_index = Some(index);
        self.selector_focus_origin = Some(browser_behavior::BrowserFocusOrigin::Keyboard);
        cx.notify();
    }

    pub(super) fn select_connection(
        &mut self,
        connection_id: String,
        focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
        cx: &mut Context<Self>,
    ) {
        self.selected_connection_id = Some(connection_id);
        self.selector_open = false;
        self.selector_highlighted_index = None;
        self.selector_focus_origin = focus_origin;
        cx.notify();
    }

    pub(super) fn select_connection_for_active_tool(
        &mut self,
        connection_id: String,
        focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
        cx: &mut Context<Self>,
    ) {
        self.select_connection(connection_id, focus_origin, cx);
        self.dismiss_process_confirm(cx);
        self.dismiss_docker_confirm(cx);
        self.dismiss_service_confirm(cx);
        self.dismiss_tmux_input_dialog(cx);
        self.dismiss_tmux_confirm(cx);
        self.dismiss_schedule_confirm(cx);

        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        // Reuse the Entity lifecycle transition so a connection switch
        // replaces page samplers without releasing the registry-owned node.
        self.update_lifecycle(
            self.visibility,
            self.monitoring.clone(),
            self.sampling_config,
            runtime,
            true,
            cx,
        );
        self.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
    }

    pub(super) fn request_active_tool_snapshot(
        &mut self,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        if !self.visibility.sidebar_is_visible()
            || !self.active_tool.monitoring_enabled(&self.monitoring)
        {
            return;
        }
        let Some(connection_id) = self.selected_connection_id_owned() else {
            return;
        };
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        let Some(messages) = self.messages.clone() else {
            return;
        };
        #[cfg(test)]
        if let Some(dispatches) = self.test_snapshot_dispatches.as_mut()
            && !matches!(
                self.active_tool,
                ContextSidebarTool::Monitor
                    | ContextSidebarTool::Gpu
                    | ContextSidebarTool::Processes
                    | ContextSidebarTool::Docker
            )
        {
            // Tests record the command boundary without opening a real SSH channel.
            dispatches.push(self.active_tool);
            return;
        }
        let notices = match self.active_tool {
            ContextSidebarTool::Services => {
                self.request_service_snapshot(
                    connection_id,
                    runtime,
                    messages.service_connection_missing,
                    messages.service_action_failed,
                    cx,
                );
                Vec::new()
            }
            ContextSidebarTool::Logs => self.request_log_snapshot(
                connection_id,
                feedback,
                self.monitoring.logs_enabled,
                runtime,
                messages.log_unknown_error,
                cx,
            ),
            ContextSidebarTool::Tmux => self.request_tmux_snapshot(
                connection_id,
                feedback,
                self.ui.host_tmux_search_query.clone(),
                messages.tmux_unknown_error,
                messages.tmux_unavailable,
                runtime,
                cx,
            ),
            ContextSidebarTool::Ports => self.request_port_snapshot(
                connection_id,
                feedback,
                self.monitoring.ports_enabled,
                runtime,
                messages.port_unknown_error,
                cx,
            ),
            ContextSidebarTool::Schedules => self.request_schedule_snapshot(
                connection_id,
                feedback,
                self.monitoring.schedules_enabled,
                runtime,
                messages.schedule_unknown_error,
                cx,
            ),
            ContextSidebarTool::Filesystems => self.request_filesystem_snapshot(
                connection_id,
                feedback,
                self.monitoring.filesystems_enabled,
                runtime,
                messages.filesystem_unknown_error,
                cx,
            ),
            ContextSidebarTool::Packages => self.request_package_snapshot(
                connection_id,
                feedback,
                self.monitoring.packages_enabled,
                runtime,
                messages.package_unknown_error,
                cx,
            ),
            ContextSidebarTool::Monitor
            | ContextSidebarTool::Gpu
            | ContextSidebarTool::Processes
            | ContextSidebarTool::Docker => Vec::new(),
        };
        for notice in notices {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    pub(in crate::workspace) fn take_selected_connection(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let selected = self.selected_connection_id.take();
        self.close_selector(true, cx);
        selected
    }

    pub(in crate::workspace) fn ensure_selected_connection(
        &mut self,
        live_connection_ids: &[String],
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if live_connection_ids.is_empty() {
            self.take_selected_connection(cx);
            return None;
        }
        let selected_is_live = self
            .selected_connection_id
            .as_ref()
            .is_some_and(|selected| {
                live_connection_ids
                    .iter()
                    .any(|connection_id| connection_id == selected)
            });
        if !selected_is_live {
            self.selected_connection_id = live_connection_ids.first().cloned();
            cx.notify();
        }
        self.selected_connection_id.clone()
    }

    pub(super) fn stop_profiler_sampling(&self) {
        self.profiler_registry.stop_all();
    }

    pub(super) fn profiler_connection_ids(&self) -> Vec<String> {
        self.profiler_registry.connection_ids()
    }

    pub(super) fn remove_profiler_connection(&self, connection_id: &str) {
        self.profiler_registry.remove(connection_id);
    }

    pub(super) fn profiler_connection_missing(&self, connection_id: &str) -> bool {
        self.profiler_registry.state(connection_id).is_none()
    }

    pub(super) fn start_profiler(
        &self,
        connection_id: String,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            return;
        };
        let Some(os_type) = handle.remote_env().map(|environment| environment.os_type) else {
            // Environment detection owns OS selection; never guess a probe dialect.
            return;
        };
        let sampler = self.resource_sampler(handle);
        self.profiler_registry.start_with_sampler_on_config(
            connection_id,
            sampler,
            os_type,
            sampling_config,
            Some(self.profiler_update_tx.clone()),
            runtime,
        );
        cx.notify();
    }

    pub(super) fn sync_gpu_sampling(
        &mut self,
        enabled_and_visible: bool,
        selected_connection_id: Option<String>,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        if !enabled_and_visible {
            if let Some(task) = self.host_gpu.sampling_task.take() {
                task.stop();
            }
            return;
        }

        let Some(connection_id) = selected_connection_id else {
            return;
        };
        if self
            .host_gpu
            .sampling_task
            .as_ref()
            .is_some_and(|task| task.connection_id() == connection_id)
        {
            return;
        }
        if let Some(task) = self.host_gpu.sampling_task.take() {
            task.stop();
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            return;
        };
        let Some(os_type) = handle.remote_env().map(|environment| environment.os_type) else {
            return;
        };
        let sampler = self.resource_sampler(handle);
        self.host_gpu.snapshot_connection_id = Some(connection_id.clone());
        self.host_gpu.snapshot = None;
        self.host_gpu.expanded_uuid = None;
        // The Entity owns only the page sampler shell; the registry retains the shared node.
        self.host_gpu.sampling_task = Some(start_gpu_sampling_on(
            connection_id,
            sampler,
            os_type,
            self.host_gpu.update_tx.clone(),
            runtime,
        ));
        cx.notify();
    }

    fn resource_sampler(
        &self,
        handle: oxideterm_ssh::SshConnectionHandle,
    ) -> Arc<dyn ResourceSampler> {
        #[cfg(test)]
        if let Some(sampler) = self.test_resource_sampler.as_ref() {
            // Tests share one counting sampler across profiler and GPU tasks.
            return sampler.clone();
        }
        Arc::new(handle)
    }

    pub(super) fn gpu_snapshot_for(&self, connection_id: &str) -> Option<GpuSnapshot> {
        self.host_gpu
            .snapshot
            .as_ref()
            .filter(|_| self.host_gpu.snapshot_connection_id.as_deref() == Some(connection_id))
            .cloned()
    }

    pub(super) fn gpu_sampling_is_running(&self, connection_id: &str) -> bool {
        self.host_gpu
            .sampling_task
            .as_ref()
            .is_some_and(|task| task.connection_id() == connection_id && !task.is_finished())
    }

    pub(super) fn gpu_list_state(&self) -> ListState {
        self.host_gpu.list_state.clone()
    }

    pub(super) fn toggle_gpu_device(&mut self, device_uuid: String, cx: &mut Context<Self>) {
        if self.host_gpu.expanded_uuid.as_deref() == Some(device_uuid.as_str()) {
            self.host_gpu.expanded_uuid = None;
        } else {
            self.host_gpu.expanded_uuid = Some(device_uuid);
        }
        cx.notify();
    }

    pub(super) fn gpu_device_is_expanded(&self, device_uuid: &str) -> bool {
        self.host_gpu.expanded_uuid.as_deref() == Some(device_uuid)
    }

    pub(super) fn sync_gpu_list_state(
        &self,
        devices: &[GpuDevice],
        snapshot: Option<&GpuSnapshot>,
        selected_connection_id: &str,
    ) {
        let signatures = devices
            .iter()
            .map(|device| {
                let process_count = snapshot
                    .map(|snapshot| snapshot.processes_for(device).count())
                    .unwrap_or_default();
                gpu_device_row_signature(
                    device,
                    process_count,
                    self.gpu_device_is_expanded(&device.uuid),
                )
            })
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.host_gpu.list_state,
            &mut self.host_gpu.list_cache.borrow_mut(),
            &format!("host-gpu:{selected_connection_id}"),
            &signatures,
            TauriVirtualListSpec::new(px(HOST_GPU_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn request_gpu_refresh(&mut self, connection_id: String, cx: &mut Context<Self>) {
        let enabled_and_visible = self.monitoring.gpu_enabled
            && self.visibility.sidebar_is_visible()
            && self.active_tool() == ContextSidebarTool::Gpu;
        let selected_connection_id = self.selected_connection_id_owned();
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        self.restart_gpu_sampling(
            connection_id,
            enabled_and_visible,
            selected_connection_id,
            runtime,
            cx,
        );
    }

    pub(super) fn restart_gpu_sampling(
        &mut self,
        connection_id: String,
        enabled_and_visible: bool,
        selected_connection_id: Option<String>,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        if self
            .host_gpu
            .sampling_task
            .as_ref()
            .is_some_and(|task| task.connection_id() == connection_id)
            && let Some(task) = self.host_gpu.sampling_task.take()
        {
            task.stop();
        }
        self.host_gpu.snapshot = None;
        self.sync_gpu_sampling(enabled_and_visible, selected_connection_id, runtime, cx);
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_host_tools_window_modal_key(
        &mut self,
        modal: HostToolsWindowModalSnapshot,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        match modal {
            HostToolsWindowModalSnapshot::ProcessConfirm(_) => {
                self.handle_host_process_confirm_key(event, cx)
            }
            HostToolsWindowModalSnapshot::DockerConfirm(_) => {
                self.handle_host_docker_confirm_key(event, cx)
            }
            HostToolsWindowModalSnapshot::DockerLogs => {
                if self.handle_host_log_search_key(event, cx) {
                    return true;
                }
                if event.keystroke.key.as_str() == "escape" {
                    self.host_tools.update(cx, |host_tools, cx| {
                        host_tools.dismiss_docker_logs_dialog(cx);
                    });
                }
                true
            }
            HostToolsWindowModalSnapshot::ServiceConfirm(_) => {
                self.handle_host_service_confirm_key(event, cx)
            }
            HostToolsWindowModalSnapshot::ServiceLogs => {
                if self.handle_host_log_search_key(event, cx) {
                    return true;
                }
                if event.keystroke.key.as_str() == "escape" {
                    self.host_tools.update(cx, |host_tools, cx| {
                        host_tools.dismiss_service_logs_dialog(cx);
                    });
                }
                true
            }
            HostToolsWindowModalSnapshot::TmuxConfirm(_) => {
                self.handle_host_tmux_confirm_key(event, cx)
            }
            HostToolsWindowModalSnapshot::TmuxInput => {
                self.handle_host_tmux_input_dialog_key(event, cx)
            }
            HostToolsWindowModalSnapshot::ScheduleConfirm(_) => {
                self.handle_host_schedule_confirm_key(event, cx)
            }
            HostToolsWindowModalSnapshot::ScheduleLogs => {
                if self.handle_host_log_search_key(event, cx) {
                    return true;
                }
                if event.keystroke.key.as_str() == "escape" {
                    self.host_tools.update(cx, |host_tools, cx| {
                        host_tools.dismiss_schedule_logs_dialog(cx);
                    });
                }
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use oxideterm_connection_monitor::{
        GPU_END_MARKER, ProfilerState, ResourceSampleShell, ResourceSamplerFuture,
    };
    use oxideterm_ssh::{RemoteEnvInfo, SshCommandOutput};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct VisibilityCountingSampler {
        shell_open_count: Arc<AtomicUsize>,
        shell_close_count: Arc<AtomicUsize>,
    }

    impl ResourceSampler for VisibilityCountingSampler {
        fn open_shell<'a>(
            &'a self,
            _init_command: &'a str,
            _timeout: Duration,
        ) -> ResourceSamplerFuture<'a, Result<Box<dyn ResourceSampleShell>, String>> {
            let shell_open_count = self.shell_open_count.clone();
            let shell_close_count = self.shell_close_count.clone();
            Box::pin(async move {
                shell_open_count.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(VisibilityCountingShell { shell_close_count })
                    as Box<dyn ResourceSampleShell>)
            })
        }
    }

    struct VisibilityCountingShell {
        shell_close_count: Arc<AtomicUsize>,
    }

    impl ResourceSampleShell for VisibilityCountingShell {
        fn sample_until<'a>(
            &'a mut self,
            _command: &'a str,
            end_marker: &'a str,
            _timeout: Duration,
            _max_output_size: usize,
        ) -> ResourceSamplerFuture<'a, Result<String, String>> {
            Box::pin(async move {
                if end_marker == GPU_END_MARKER {
                    return Ok(concat!(
                        "===NVIDIA_STATUS===\navailable\n",
                        "===NVIDIA_GPUS===\n",
                        "0, GPU-a, 00000000:01:00.0, NVIDIA L40S, 555.42, P0, 10, 2, 512, 46068, 41, 50, 350, N/A\n",
                        "===NVIDIA_GPU_QUERY_EXIT===\n0\n",
                        "===NVIDIA_PROCESSES===\n",
                        "===NVIDIA_GPU_END==="
                    )
                    .to_string());
                }
                Ok("===END===\n".to_string())
            })
        }

        fn close<'a>(&'a mut self) -> ResourceSamplerFuture<'a, Result<(), String>> {
            Box::pin(async move {
                self.shell_close_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    fn wait_for_counter(counter: &AtomicUsize, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while counter.load(Ordering::SeqCst) < expected {
            assert!(
                Instant::now() < deadline,
                "counter did not reach {expected} before the deadline"
            );
            std::thread::yield_now();
        }
    }

    #[gpui::test]
    fn lifecycle_tick_samples_only_visible_host_tools(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
        let registry = SshConnectionRegistry::default();
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity =
            cx.new(|cx| HostToolsEntity::new(profiler_update_tx, profiler_update_rx, registry, cx));

        entity.update(cx, |entity, cx| {
            assert!(entity.lifecycle_refresh_task.is_some());
            entity.lifecycle_runtime = Some(runtime.handle().clone());
            entity.last_pool_refresh = None;
            entity.visibility = HostToolsVisibility::Hidden;
            entity.refresh_lifecycle_tick(cx);
            assert!(entity.last_pool_refresh.is_none());

            entity.visibility = HostToolsVisibility::VisibleMainTab;
            entity.refresh_lifecycle_tick(cx);
            assert!(entity.last_pool_refresh.is_some());
        });
    }

    #[gpui::test]
    fn visibility_stops_page_samplers_but_keeps_reliable_actions_and_node_owner(
        cx: &mut TestAppContext,
    ) {
        let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
        let registry = SshConnectionRegistry::default();
        let node_consumer = ConnectionConsumer::NodeRouter("node-visibility".to_string());
        let handle = registry.acquire(
            SshConfig {
                host: "host.example".to_string(),
                username: "alice".to_string(),
                auth: AuthMethod::Agent,
                ..SshConfig::default()
            },
            node_consumer.clone(),
        );
        assert!(handle.set_remote_env(RemoteEnvInfo {
            os_type: "Linux".to_string(),
            os_version: None,
            kernel: None,
            arch: None,
            shell: Some("/bin/sh".to_string()),
            home: None,
            zdotdir: None,
            xdg_config_home: None,
            detected_at: 1,
        }));
        let connection_id = handle.connection_id().to_string();
        let shell_open_count = Arc::new(AtomicUsize::new(0));
        let shell_close_count = Arc::new(AtomicUsize::new(0));
        let test_sampler: Arc<dyn ResourceSampler> = Arc::new(VisibilityCountingSampler {
            shell_open_count: shell_open_count.clone(),
            shell_close_count: shell_close_count.clone(),
        });
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity =
            cx.new(|cx| HostToolsEntity::new(profiler_update_tx, profiler_update_rx, registry, cx));
        let mut events = cx.events(&entity);
        let monitoring = oxideterm_settings::HostToolsSettings::default();
        let sampling_config = oxideterm_connection_monitor::ResourceSamplingConfig::default();

        entity.update(cx, |entity, cx| {
            entity.test_resource_sampler = Some(test_sampler);
            entity.test_snapshot_dispatches = Some(Vec::new());
            entity.messages = Some(HostToolsMessages {
                service_connection_missing: "Service connection missing".to_string(),
                service_action_failed: "Service action failed".to_string(),
                log_unknown_error: "Log capture failed".to_string(),
                port_unknown_error: "Port capture failed".to_string(),
                filesystem_unknown_error: "Filesystem capture failed".to_string(),
                package_unknown_error: "Package capture failed".to_string(),
                schedule_unknown_error: "Schedule capture failed".to_string(),
                tmux_unknown_error: "tmux capture failed".to_string(),
                tmux_unavailable: "tmux unavailable".to_string(),
            });
            entity.select_connection(connection_id.clone(), None, cx);
            entity.active_tool = ContextSidebarTool::Gpu;
            entity.update_lifecycle(
                HostToolsVisibility::VisibleSidebar,
                monitoring.clone(),
                sampling_config,
                runtime.handle().clone(),
                true,
                cx,
            );
        });

        // Visible GPU owns one profiler shell and one page-scoped GPU shell.
        wait_for_counter(&shell_open_count, 2);
        entity.read_with(cx, |entity, _cx| {
            assert_eq!(
                entity.profiler_registry.state(&connection_id),
                Some(ProfilerState::Running)
            );
            assert!(entity.gpu_sampling_is_running(&connection_id));
        });

        let action_request = HostServiceActionRequest {
            connection_id: connection_id.clone(),
            service_id: "ssh.service".to_string(),
            description: "SSH".to_string(),
            action: ServiceActionKind::Restart,
        };
        let reliable_sender = entity.update(cx, |entity, cx| {
            entity.host_services.action_running = Some(action_request.clone());
            entity.update_lifecycle(
                HostToolsVisibility::Hidden,
                monitoring.clone(),
                sampling_config,
                runtime.handle().clone(),
                false,
                cx,
            );
            entity.active_tool = ContextSidebarTool::Services;
            entity.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
            entity.reliable_delivery_tx.clone()
        });

        wait_for_counter(&shell_close_count, 2);
        let hidden_shell_open_count = shell_open_count.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(
            shell_open_count.load(Ordering::SeqCst),
            hidden_shell_open_count
        );
        entity.read_with(cx, |entity, _cx| {
            assert_eq!(entity.profiler_registry.state(&connection_id), None);
            assert!(!entity.gpu_sampling_is_running(&connection_id));
            assert!(
                entity
                    .test_snapshot_dispatches
                    .as_ref()
                    .expect("snapshot test hook")
                    .is_empty()
            );
        });

        reliable_sender
            .send(super::delivery::HostToolsReliableDelivery::ServiceAction(
                HostServiceActionDelivery {
                    request: action_request,
                    result: Ok(true),
                },
            ))
            .expect("deliver hidden-page action result");
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            assert!(entity.host_services.action_running.is_none());
        });
        assert_eq!(
            events.try_recv().expect("hidden action completion notice"),
            HostToolsEvent::ShowNotice(HostToolsNotice::ServiceActionFinished {
                description: "SSH".to_string(),
                succeeded: true,
            })
        );
        assert!(events.try_recv().is_err());
        assert_eq!(handle.info().ref_count, 1);
        assert_eq!(handle.info().consumers, vec![node_consumer.clone()]);

        entity.update(cx, |entity, cx| {
            entity.update_lifecycle(
                HostToolsVisibility::VisibleSidebar,
                monitoring,
                sampling_config,
                runtime.handle().clone(),
                false,
                cx,
            );
            entity.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
        });

        // Re-showing Services restarts only the shared profiler plus Services.
        wait_for_counter(&shell_open_count, hidden_shell_open_count + 1);
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(
            shell_open_count.load(Ordering::SeqCst),
            hidden_shell_open_count + 1
        );
        entity.read_with(cx, |entity, _cx| {
            assert_eq!(
                entity.profiler_registry.state(&connection_id),
                Some(ProfilerState::Running)
            );
            assert!(!entity.gpu_sampling_is_running(&connection_id));
            assert_eq!(
                entity
                    .test_snapshot_dispatches
                    .as_ref()
                    .expect("snapshot test hook"),
                &[ContextSidebarTool::Services]
            );
        });
        assert_eq!(handle.info().ref_count, 1);
        assert_eq!(handle.info().consumers, vec![node_consumer]);
    }

    #[gpui::test]
    fn process_action_state_and_delivery_are_entity_owned_and_redacted(cx: &mut TestAppContext) {
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = cx.new(|cx| {
            HostToolsEntity::new(
                profiler_update_tx,
                profiler_update_rx,
                SshConnectionRegistry::default(),
                cx,
            )
        });
        let mut events = cx.events(&entity);
        let display_secret = Arc::new(zeroize::Zeroizing::new(
            "worker --token should-not-reach-delivery".to_string(),
        ));
        let request = HostProcessActionRequest {
            connection_id: "connection-1".to_string(),
            pid: "42".to_string(),
            display_command: display_secret.clone(),
            action: ProcessActionKind::Term,
        };

        let (delivery_tx, delivery) = entity.update(cx, |entity, cx| {
            let invalid_request = HostProcessActionRequest {
                connection_id: "connection-1".to_string(),
                pid: "42".to_string(),
                display_command: display_secret.clone(),
                action: ProcessActionKind::Renice { nice: 20 },
            };
            assert_eq!(
                entity.open_process_action_confirm(invalid_request, cx),
                Some(HostToolsNotice::ProcessInvalidNice)
            );
            assert!(
                entity
                    .open_process_action_confirm(request.clone(), cx)
                    .is_none()
            );
            let (visible_request, _) = entity.process_confirm_view().unwrap();
            assert!(Arc::ptr_eq(
                &visible_request.display_command,
                &display_secret
            ));
            entity.dismiss_process_confirm(cx);
            assert!(entity.process_confirm_view().is_none());

            let running = HostProcessActionRun {
                connection_id: request.connection_id.clone(),
                pid: request.pid.clone(),
                action: request.action.clone(),
            };
            entity.host_process_actions.running = Some(running.clone());
            (
                entity.reliable_delivery_tx.clone(),
                HostProcessActionDelivery {
                    request: running,
                    // The delivery type cannot carry captured command output.
                    result: Ok(false),
                },
            )
        });

        delivery_tx
            .send(super::delivery::HostToolsReliableDelivery::ProcessAction(
                delivery,
            ))
            .unwrap();
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            assert!(entity.host_process_actions.running.is_none());
        });
        let notice = events.try_recv().unwrap();
        assert_eq!(
            notice,
            HostToolsEvent::ShowNotice(HostToolsNotice::ProcessActionFinished {
                pid: "42".to_string(),
                succeeded: false,
            })
        );
        assert!(!format!("{notice:?}").contains(display_secret.as_str()));
        assert!(events.try_recv().is_err());
    }

    #[gpui::test]
    fn docker_actions_and_logs_are_entity_owned_and_redacted(cx: &mut TestAppContext) {
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = cx.new(|cx| {
            HostToolsEntity::new(
                profiler_update_tx,
                profiler_update_rx,
                SshConnectionRegistry::default(),
                cx,
            )
        });
        let mut events = cx.events(&entity);
        let logs_request = HostDockerLogsRequest {
            connection_id: "connection-1".to_string(),
            container_id: "abc123".to_string(),
            container_name: "web".to_string(),
            failure_fallback: "Docker logs failed".to_string(),
            empty_fallback: "No Docker logs".to_string(),
        };
        let logs_delivery_tx = entity.update(cx, |entity, _cx| {
            entity.host_docker_operations.logs_dialog = Some(HostDockerLogsDialog {
                request: logs_request.clone(),
                output: None,
                error: None,
                loading: true,
            });
            entity.reliable_delivery_tx.clone()
        });
        let secret_marker = "Bearer should-not-reach-docker-dialog";

        logs_delivery_tx
            .send(super::delivery::HostToolsReliableDelivery::DockerLogs(
                HostDockerLogsDelivery {
                    request: logs_request,
                    result: Ok(SshCommandOutput {
                        stdout: secret_marker.to_string(),
                        stderr: secret_marker.to_string(),
                        exit_code: Some(1),
                        truncated: false,
                    }),
                },
            ))
            .unwrap();
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            let dialog = entity.host_docker_operations.logs_dialog.as_ref().unwrap();
            assert!(!dialog.loading);
            assert!(dialog.output.is_none());
            assert_eq!(dialog.error.as_deref(), Some("Docker logs failed"));
            assert!(!dialog.error.as_deref().unwrap().contains(secret_marker));
        });
        assert!(events.try_recv().is_err());

        let action_request = HostDockerActionRequest {
            connection_id: "connection-1".to_string(),
            container_id: "abc123".to_string(),
            container_name: "web".to_string(),
            action: DockerActionKind::Restart,
        };
        let (action_delivery_tx, action_delivery) = entity.update(cx, |entity, cx| {
            assert!(
                entity
                    .open_docker_action_confirm(action_request.clone(), cx)
                    .is_none()
            );
            assert!(entity.docker_confirm_view().is_some());
            entity.dismiss_docker_confirm(cx);
            assert!(entity.docker_confirm_view().is_none());
            entity.host_docker_operations.action_running = Some(action_request.clone());
            (
                entity.reliable_delivery_tx.clone(),
                HostDockerActionDelivery {
                    request: action_request,
                    // Captured Docker output cannot enter this delivery type.
                    result: Ok(false),
                },
            )
        });

        action_delivery_tx
            .send(super::delivery::HostToolsReliableDelivery::DockerAction(
                action_delivery,
            ))
            .unwrap();
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            assert!(entity.host_docker_operations.action_running.is_none());
        });
        assert_eq!(
            events.try_recv().unwrap(),
            HostToolsEvent::ShowNotice(HostToolsNotice::DockerActionFinished {
                container_name: "web".to_string(),
                succeeded: false,
            })
        );
        assert!(events.try_recv().is_err());
    }

    #[gpui::test]
    fn service_snapshot_actions_and_logs_are_entity_owned_and_redacted(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = cx.new(|cx| {
            HostToolsEntity::new(
                profiler_update_tx,
                profiler_update_rx,
                SshConnectionRegistry::default(),
                cx,
            )
        });
        let mut events = cx.events(&entity);
        let snapshot_request = HostServiceSnapshotRequest {
            connection_id: "connection-1".to_string(),
            connection_fallback: "Service connection missing".to_string(),
            failure_fallback: "Service capture failed".to_string(),
        };
        let logs_request = HostServiceLogsRequest {
            connection_id: "connection-1".to_string(),
            service_id: "ssh.service".to_string(),
            description: "SSH".to_string(),
            failure_fallback: "Service logs failed".to_string(),
            empty_fallback: "No service logs".to_string(),
        };
        let sender = entity.update(cx, |entity, _cx| {
            entity.host_services.snapshot_running = Some(snapshot_request.clone());
            entity.host_services.snapshot_in_flight = true;
            entity.host_services.logs_dialog = Some(HostServiceLogsDialog {
                request: logs_request.clone(),
                output: None,
                error: None,
                loading: true,
            });
            entity.reliable_delivery_tx.clone()
        });
        let secret_marker = "Bearer should-not-reach-service-state";

        sender
            .send(super::delivery::HostToolsReliableDelivery::ServiceSnapshot(
                HostServiceSnapshotDelivery {
                    request: snapshot_request,
                    result: Ok(SshCommandOutput {
                        stdout: secret_marker.to_string(),
                        stderr: secret_marker.to_string(),
                        exit_code: Some(1),
                        truncated: false,
                    }),
                },
            ))
            .unwrap();
        sender
            .send(super::delivery::HostToolsReliableDelivery::ServiceLogs(
                HostServiceLogsDelivery {
                    request: logs_request,
                    result: Ok(SshCommandOutput {
                        stdout: secret_marker.to_string(),
                        stderr: secret_marker.to_string(),
                        exit_code: Some(1),
                        truncated: false,
                    }),
                },
            ))
            .unwrap();
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            assert!(!entity.host_services.snapshot_in_flight);
            let snapshot = entity.host_services.snapshot.as_ref().unwrap();
            assert_eq!(
                snapshot.status,
                ResourceServiceStatus::Error {
                    message: "Service capture failed".to_string(),
                }
            );
            assert!(!format!("{:?}", snapshot.status).contains(secret_marker));
            let dialog = entity.host_services.logs_dialog.as_ref().unwrap();
            assert!(!dialog.loading);
            assert!(dialog.output.is_none());
            assert_eq!(dialog.error.as_deref(), Some("Service logs failed"));
            assert!(!dialog.error.as_deref().unwrap().contains(secret_marker));
        });
        assert!(events.try_recv().is_err());

        let action_request = HostServiceActionRequest {
            connection_id: "connection-1".to_string(),
            service_id: "ssh.service".to_string(),
            description: "SSH".to_string(),
            action: ServiceActionKind::Restart,
        };
        let sender = entity.update(cx, |entity, cx| {
            entity.visibility = HostToolsVisibility::VisibleSidebar;
            entity.lifecycle_runtime = Some(runtime.handle().clone());
            entity.messages = Some(HostToolsMessages {
                service_connection_missing: "Service connection missing".to_string(),
                service_action_failed: "Service capture failed".to_string(),
                log_unknown_error: "Log capture failed".to_string(),
                port_unknown_error: "Port capture failed".to_string(),
                filesystem_unknown_error: "Filesystem capture failed".to_string(),
                package_unknown_error: "Package capture failed".to_string(),
                schedule_unknown_error: "Schedule capture failed".to_string(),
                tmux_unknown_error: "tmux capture failed".to_string(),
                tmux_unavailable: "tmux unavailable".to_string(),
            });
            entity.select_tool(ContextSidebarTool::Services, cx);
            assert!(
                entity
                    .open_service_action_confirm(action_request.clone(), cx)
                    .is_none()
            );
            assert!(entity.service_confirm_view().is_some());
            entity.dismiss_service_confirm(cx);
            assert!(entity.service_confirm_view().is_none());
            entity.host_services.action_running = Some(action_request.clone());
            entity.reliable_delivery_tx.clone()
        });

        sender
            .send(super::delivery::HostToolsReliableDelivery::ServiceAction(
                HostServiceActionDelivery {
                    request: action_request,
                    // Worker output is intentionally reduced to this boolean.
                    result: Ok(false),
                },
            ))
            .unwrap();
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            assert!(entity.host_services.action_running.is_none());
            assert_eq!(
                entity.host_services.snapshot.as_ref().unwrap().status,
                ResourceServiceStatus::Error {
                    message: "Service connection missing".to_string(),
                }
            );
        });
        assert_eq!(
            events.try_recv().unwrap(),
            HostToolsEvent::ShowNotice(HostToolsNotice::ServiceActionFinished {
                description: "SSH".to_string(),
                succeeded: false,
            })
        );
        assert!(events.try_recv().is_err());
    }

    #[gpui::test]
    fn tmux_snapshot_action_and_confirm_are_entity_owned_and_redacted(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
        let runtime_handle = runtime.handle().clone();
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = cx.new(|cx| {
            HostToolsEntity::new(
                profiler_update_tx,
                profiler_update_rx,
                SshConnectionRegistry::default(),
                cx,
            )
        });
        let mut events = cx.events(&entity);
        let snapshot_request = HostTmuxSnapshotRequest {
            connection_id: "connection-1".to_string(),
            feedback: HostSnapshotFeedback::Silent,
            search_query: String::new(),
            failure_fallback: "tmux capture failed".to_string(),
            unavailable_fallback: "tmux unavailable".to_string(),
        };
        let sender = entity.update(cx, |entity, _cx| {
            entity.host_tmux.snapshot_running = Some(snapshot_request.clone());
            entity.host_tmux.snapshot_in_flight = true;
            entity.reliable_delivery_tx.clone()
        });
        let secret_marker = "Authorization: should-not-reach-tmux-state";

        sender
            .send(super::delivery::HostToolsReliableDelivery::TmuxSnapshot(
                HostTmuxSnapshotDelivery {
                    request: snapshot_request,
                    result: Ok(SshCommandOutput {
                        stdout: secret_marker.to_string(),
                        stderr: secret_marker.to_string(),
                        exit_code: Some(1),
                        truncated: false,
                    }),
                },
            ))
            .unwrap();
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            assert!(!entity.host_tmux.snapshot_in_flight);
            assert_eq!(
                entity.host_tmux.snapshot.as_ref().unwrap().status,
                ResourceTmuxStatus::Error {
                    message: "tmux capture failed".to_string(),
                }
            );
            assert!(
                !format!("{:?}", entity.host_tmux.snapshot.as_ref().unwrap().status)
                    .contains(secret_marker)
            );
        });
        assert!(events.try_recv().is_err());

        let confirm_request = HostTmuxActionRequest {
            connection_id: "connection-1".to_string(),
            session_id: "$1".to_string(),
            session_name: "work".to_string(),
            target_label: "$1".to_string(),
            action: HostTmuxDestructiveAction::KillSession {
                target: "$1".to_string(),
            },
        };
        let action_request = HostTmuxActionRun {
            connection_id: "connection-1".to_string(),
            session_id: "$1".to_string(),
            session_name: "work".to_string(),
            target_label: "$1".to_string(),
        };
        let sender = entity.update(cx, |entity, cx| {
            assert!(
                entity
                    .open_tmux_action_confirm(confirm_request, cx)
                    .is_none()
            );
            assert!(entity.tmux_confirm_view().is_some());
            entity.dismiss_tmux_confirm(cx);
            assert!(entity.tmux_confirm_view().is_none());
            let empty_dialog = HostTmuxInputDialog {
                connection_id: "connection-1".to_string(),
                session_id: "$1".to_string(),
                session_name: "work".to_string(),
                target_label: "%1".to_string(),
                value: zeroize::Zeroizing::new("   ".to_string()),
                kind: HostTmuxInputDialogKind::SendPaneCommand {
                    target: "%1".to_string(),
                },
            };
            entity.open_tmux_input_dialog(empty_dialog, cx);
            assert_eq!(
                entity.submit_tmux_input(runtime_handle, cx),
                vec![HostToolsNotice::TmuxInputRequired]
            );
            entity.host_tmux.action_running = Some(action_request.clone());
            entity.reliable_delivery_tx.clone()
        });

        sender
            .send(super::delivery::HostToolsReliableDelivery::TmuxAction(
                HostTmuxActionDelivery {
                    request: action_request,
                    // Worker output is intentionally reduced to this boolean.
                    result: Ok(false),
                },
            ))
            .unwrap();
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            assert!(entity.host_tmux.action_running.is_none());
        });
        assert_eq!(
            events.try_recv().unwrap(),
            HostToolsEvent::ShowNotice(HostToolsNotice::TmuxActionFinished {
                target_label: "$1".to_string(),
                succeeded: false,
            })
        );
        assert!(events.try_recv().is_err());
    }

    #[gpui::test]
    fn log_snapshot_delivery_is_entity_owned_and_redacted(cx: &mut TestAppContext) {
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = cx.new(|cx| {
            HostToolsEntity::new(
                profiler_update_tx,
                profiler_update_rx,
                SshConnectionRegistry::default(),
                cx,
            )
        });
        let mut events = cx.events(&entity);
        let request = HostLogSnapshotRequest {
            connection_id: "connection-1".to_string(),
            preset: LogPreset::All,
            limit: HOST_LOG_SNAPSHOT_LIMIT,
            feedback: HostSnapshotFeedback::Toast,
            failure_fallback: "Log capture failed".to_string(),
        };
        let sender = entity.update(cx, |entity, _cx| {
            entity.host_logs.running = Some(request.clone());
            entity.host_logs.snapshot_in_flight = true;
            entity.reliable_delivery_tx.clone()
        });
        let secret_marker = "Bearer should-not-reach-ui";

        sender
            .send(super::delivery::HostToolsReliableDelivery::LogSnapshot(
                HostLogSnapshotDelivery {
                    request,
                    result: Ok(SshCommandOutput {
                        stdout: format!(
                            "===HOST_LOGS===\n__OXIDE_LOG_ERROR__{secret_marker}\n===HOST_LOGS_END===\n"
                        ),
                        stderr: secret_marker.to_string(),
                        exit_code: Some(0),
                        truncated: false,
                    }),
                },
            ))
            .unwrap();
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            assert!(!entity.host_logs.snapshot_in_flight);
            let snapshot = entity.host_logs.snapshot.as_ref().unwrap();
            assert_eq!(
                snapshot.status,
                ResourceLogStatus::Error {
                    message: "Log capture failed".to_string(),
                }
            );
            assert!(!format!("{:?}", snapshot.status).contains(secret_marker));
        });
        let notice = events.try_recv().unwrap();
        assert_eq!(
            notice,
            HostToolsEvent::ShowNotice(HostToolsNotice::LogSnapshotFailed)
        );
        assert!(!format!("{notice:?}").contains(secret_marker));
    }

    #[gpui::test]
    fn schedule_logs_and_actions_are_entity_owned_and_redacted(cx: &mut TestAppContext) {
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = cx.new(|cx| {
            HostToolsEntity::new(
                profiler_update_tx,
                profiler_update_rx,
                SshConnectionRegistry::default(),
                cx,
            )
        });
        let mut events = cx.events(&entity);
        let logs_request = HostScheduleLogsRequest {
            connection_id: "connection-1".to_string(),
            task_id: "backup.timer".to_string(),
            task_name: "Backup".to_string(),
            task_source: "systemd".to_string(),
            task_unit: "backup.service".to_string(),
            failure_fallback: "Schedule logs failed".to_string(),
            empty_fallback: "No schedule logs".to_string(),
        };
        let sender = entity.update(cx, |entity, _cx| {
            entity.host_schedules.logs_dialog = Some(HostScheduleLogsDialog {
                request: logs_request.clone(),
                output: None,
                error: None,
                loading: true,
            });
            entity.reliable_delivery_tx.clone()
        });
        let secret_marker = "Authorization: should-not-reach-schedule-dialog";

        sender
            .send(super::delivery::HostToolsReliableDelivery::ScheduleLogs(
                HostScheduleLogsDelivery {
                    request: logs_request,
                    result: Ok(SshCommandOutput {
                        stdout: secret_marker.to_string(),
                        stderr: secret_marker.to_string(),
                        exit_code: Some(1),
                        truncated: false,
                    }),
                },
            ))
            .unwrap();
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            let dialog = entity.host_schedules.logs_dialog.as_ref().unwrap();
            assert!(!dialog.loading);
            assert!(dialog.output.is_none());
            assert_eq!(dialog.error.as_deref(), Some("Schedule logs failed"));
            // Failed command output must be reduced before it reaches renderable state.
            assert!(!dialog.error.as_deref().unwrap().contains(secret_marker));
        });
        assert!(events.try_recv().is_err());

        let action_request = HostScheduleActionRequest {
            connection_id: "connection-1".to_string(),
            task_id: "backup.timer".to_string(),
            task_name: "Backup".to_string(),
            unit: "backup.service".to_string(),
            action: ScheduledTaskActionKind::RunNow {
                id: "backup.timer".to_string(),
                unit: "backup.service".to_string(),
            },
        };
        let sender = entity.update(cx, |entity, cx| {
            assert!(
                entity
                    .open_schedule_action_confirm(action_request.clone(), cx)
                    .is_none()
            );
            assert!(entity.schedule_confirm_view().is_some());
            entity.dismiss_schedule_confirm(cx);
            assert!(entity.schedule_confirm_view().is_none());
            entity.host_schedules.action_running = Some(action_request.clone());
            entity.reliable_delivery_tx.clone()
        });

        sender
            .send(super::delivery::HostToolsReliableDelivery::ScheduleAction(
                HostScheduleActionDelivery {
                    request: action_request,
                    // Worker output is intentionally reduced to this boolean.
                    result: Ok(false),
                },
            ))
            .unwrap();
        cx.run_until_parked();

        entity.read_with(cx, |entity, _cx| {
            assert!(entity.host_schedules.action_running.is_none());
        });
        assert_eq!(
            events.try_recv().unwrap(),
            HostToolsEvent::ShowNotice(HostToolsNotice::ScheduleActionFinished {
                kind: ScheduleActionNoticeKind::RunNow,
                task_name: "Backup".to_string(),
                succeeded: false,
            })
        );
        assert!(events.try_recv().is_err());
    }
}
