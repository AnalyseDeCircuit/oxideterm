use super::*;
use oxideterm_gpui_ui::select::{BrowserSelectKeyDirection, browser_select_next_index};

mod ai;
mod capture;

use gpui::Task;
use oxideterm_connection_monitor::ResourceSampler;
use oxideterm_connection_monitor::multiplexer::Multiplexer;
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_topology::ConnectionTopologySnapshot;

pub(crate) fn finish_host_query_audit(
    audit: oxideterm_audit::AuditOperation,
    result: &Result<oxideterm_ssh::SshCommandOutput, ()>,
) {
    let exit_code = result.as_ref().ok().and_then(|output| output.exit_code);
    let output_bytes = result
        .as_ref()
        .ok()
        .map(|output| output.stdout.len().saturating_add(output.stderr.len()) as u64);
    let outcome = match (result, exit_code) {
        (Err(_), _) => oxideterm_audit::AuditOutcome::Failed,
        (_, Some(0)) => oxideterm_audit::AuditOutcome::Succeeded,
        (_, None) => oxideterm_audit::AuditOutcome::Unknown,
        (_, Some(_)) => oxideterm_audit::AuditOutcome::Failed,
    };
    audit.finish(
        outcome,
        if exit_code.is_some() {
            oxideterm_audit::AuditEvidence::ExitCode
        } else {
            oxideterm_audit::AuditEvidence::Protocol
        },
        exit_code,
        output_bytes,
    );
}

#[derive(Clone)]
struct HostToolsSshConsumerContext {
    node_router: NodeRouter,
    prompt_handler: Arc<dyn SshPromptHandler>,
    managed_key_resolver: ManagedKeyResolver,
}

/// Owns Host Tools sampling state independently from WorkspaceApp and SSH nodes.
pub struct HostToolsEntity {
    // This handle is private to the Entity. Host Tools exposes snapshots and
    // sampler acquisition only; page code cannot disconnect shared nodes.
    ssh_registry: SshConnectionRegistry,
    ssh_consumer_context: Option<HostToolsSshConsumerContext>,
    pub profiler_registry: ProfilerRegistry,
    pub(crate) profiler_update_tx: tokio::sync::mpsc::Sender<()>,
    pub(crate) sampler_delivery_wake: oxideterm_workspace::delivery::ActiveDeliveryWake,
    pub(crate) sampler_delivery_rx:
        std::sync::mpsc::Receiver<super::delivery::HostToolsSamplerDelivery>,
    // Reliable user-action results have their own wake and budget so sampler
    // traffic cannot delay a completed Host Tools operation.
    pub(crate) reliable_delivery_wake: oxideterm_workspace::delivery::ActiveDeliveryWake,
    pub(crate) reliable_delivery_tx: oxideterm_workspace::delivery::ActiveDeliverySender<
        super::delivery::HostToolsReliableDelivery,
    >,
    pub(crate) reliable_delivery_rx:
        std::sync::mpsc::Receiver<super::delivery::HostToolsReliableDelivery>,
    // Search, list, expansion, and dialog state belongs to the shared Host
    // Tools surface rather than whichever workspace mount renders it.
    pub ui: HostToolsUiState,
    pub(crate) host_process_actions: HostProcessActionsState,
    pub(crate) host_docker_operations: HostDockerOperationsState,
    pub host_container: HostContainerState,
    pub(crate) host_services: HostServicesState,
    pub host_redis: HostRedisState,
    pub(crate) host_tmux: HostTmuxState,
    pub(crate) host_gpu: HostGpuViewState,
    pub(crate) host_logs: HostLogsState,
    pub(crate) host_ports: HostPortsState,
    pub(crate) host_filesystems: FilesystemPanel,
    pub(crate) host_packages: HostPackagesState,
    pub(crate) host_schedules: HostSchedulesState,
    pub active_runtime_section: ConnectionRuntimeSection,
    pub previous_runtime_section: ConnectionRuntimeSection,
    selected_connection_id: Option<String>,
    selector_open: bool,
    selector_highlighted_index: Option<usize>,
    selector_focus_origin: Option<BrowserFocusOrigin>,
    // Tool state survives sidebar navigation and visibility changes.
    active_tool: ContextSidebarTool,
    // Visibility is an Entity lifecycle input. It controls only page-scoped
    // sampling and never owns or releases shared SSH transports.
    pub visibility: HostToolsVisibility,
    // These inputs let the Entity refresh its own samplers after an action.
    // The runtime handle has no registry release or disconnect capability.
    pub(crate) lifecycle_runtime: Option<tokio::runtime::Handle>,
    pub(crate) sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
    // Host Tools receives one read-only settings snapshot. Persistent writes
    // remain owned by SettingsStore and are applied back through lifecycle.
    pub monitoring: oxideterm_settings::HostToolsSettings,
    pub(crate) messages: Option<HostToolsMessages>,
    pub(crate) lifecycle_refresh_task: Option<Task<()>>,
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
    pub topology_transform: TopologyTransform,
    pub(crate) topology_drag: Option<TopologyDragState>,
    pub topology_menu: Option<TopologyNodeMenuState>,
    compact_monitor_list_state: ListState,
    compact_monitor_list_cache: RefCell<VirtualListSignatureCache>,
    monitor_expanded_cpu_connections: RefCell<HashSet<String>>,
}

impl HostToolsEntity {
    pub(crate) fn host_tool_audit(
        &self,
        connection_id: &str,
        action: &str,
        detail: Option<&str>,
    ) -> oxideterm_audit::AuditOperation {
        let context = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.audit_context());
        if let Some(context) = context {
            oxideterm_audit::AuditOperation::in_request(
                Some(&context),
                oxideterm_audit::AuditCategory::Host,
                action,
                detail,
            )
        } else {
            oxideterm_audit::AuditOperation::begin(
                oxideterm_audit::AuditCategory::Host,
                action,
                Some(connection_id),
                detail,
            )
        }
    }

    pub fn window_modal_snapshot(&self) -> Option<HostToolsWindowModalSnapshot> {
        if self.host_schedules.logs_dialog.is_some() {
            Some(HostToolsWindowModalSnapshot::ScheduleLogs)
        } else if let Some(confirm) = self.host_schedules.pending_confirm.as_ref() {
            Some(HostToolsWindowModalSnapshot::ScheduleConfirm(
                confirm.presence.phase(),
            ))
        } else if self.host_tmux.mux.read.is_some() {
            Some(HostToolsWindowModalSnapshot::MuxRead)
        } else if self.ui.host_mux_input_dialog.is_some() {
            Some(HostToolsWindowModalSnapshot::MuxInput)
        } else if let Some(confirm) = self.host_tmux.mux.confirm.as_ref() {
            Some(HostToolsWindowModalSnapshot::MuxConfirm(
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
        } else if self.ui.host_container_input_dialog.is_some() {
            Some(HostToolsWindowModalSnapshot::ContainerInput)
        } else if self.host_container.text_dialog.is_some() {
            Some(HostToolsWindowModalSnapshot::ContainerText)
        } else if let Some(confirm) = self.host_container.confirm.as_ref() {
            Some(HostToolsWindowModalSnapshot::ContainerConfirm(
                confirm.presence.phase(),
            ))
        } else if let Some(confirm) = self.host_packages.pending_confirm.as_ref() {
            Some(HostToolsWindowModalSnapshot::PackageConfirm(
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

    pub fn new(
        profiler_update_tx: tokio::sync::mpsc::Sender<()>,
        profiler_update_rx: tokio::sync::mpsc::Receiver<()>,
        ssh_registry: SshConnectionRegistry,
        cx: &mut Context<Self>,
    ) -> Self {
        let sampler_delivery_wake = oxideterm_workspace::delivery::ActiveDeliveryWake::default();
        let (sampler_delivery_tx, sampler_delivery_rx) =
            oxideterm_workspace::delivery::ActiveDeliverySender::channel_with_wake(
                sampler_delivery_wake.clone(),
            );
        let (gpu_update_tx, gpu_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let reliable_delivery_wake = oxideterm_workspace::delivery::ActiveDeliveryWake::default();
        let (reliable_delivery_tx, reliable_delivery_rx) =
            oxideterm_workspace::delivery::ActiveDeliverySender::channel_with_wake(
                reliable_delivery_wake.clone(),
            );
        let mut entity = Self {
            ssh_registry,
            ssh_consumer_context: None,
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
            host_container: HostContainerState::new(),
            host_services: HostServicesState::new(),
            host_redis: HostRedisState::new(),
            host_tmux: HostTmuxState::new(),
            host_gpu: HostGpuViewState::new(gpu_update_tx),
            host_logs: HostLogsState::new(),
            host_ports: HostPortsState::new(),
            host_filesystems: FilesystemPanel::default(),
            host_packages: HostPackagesState::new(),
            host_schedules: HostSchedulesState::new(),
            active_runtime_section: ConnectionRuntimeSection::Overview,
            previous_runtime_section: ConnectionRuntimeSection::Overview,
            selected_connection_id: None,
            selector_open: false,
            selector_highlighted_index: None,
            selector_focus_origin: None,
            active_tool: ContextSidebarTool::Monitor,
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
            monitor_expanded_cpu_connections: RefCell::new(HashSet::new()),
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

    pub fn set_ssh_consumer_context(
        &mut self,
        node_router: NodeRouter,
        prompt_handler: Arc<dyn SshPromptHandler>,
        managed_key_resolver: ManagedKeyResolver,
    ) {
        self.ssh_consumer_context = Some(HostToolsSshConsumerContext {
            node_router,
            prompt_handler,
            managed_key_resolver,
        });
    }

    pub fn filesystem_search_value(&self) -> Option<&str> {
        self.ui
            .input_is_focused(HostToolsTextInput::FilesystemSearch)
            .then(|| self.host_filesystems.query())
    }

    pub fn blur_input(&mut self, cx: &mut Context<Self>) -> bool {
        if self.ui.focused_input.is_none() {
            return false;
        }
        self.ui.clear_input_focus();
        cx.notify();
        true
    }

    pub fn replace_text_input(
        &mut self,
        input: HostToolsTextInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
    ) -> bool {
        if input == HostToolsTextInput::FilesystemSearch {
            if !self.ui.input_is_focused(input) {
                return false;
            }
            self.host_filesystems.replace_query(replacement_range, text);
            return true;
        }
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
            // The multiplexer tree reveals search matches without mutating the
            // click-managed expansion set, so an edit needs no reset here.
            HostToolsTextInput::PortSearch => self.host_ports.expanded_index = None,
            HostToolsTextInput::ScheduleSearch => self.host_schedules.expanded_index = None,
            HostToolsTextInput::PackageSearch => self.host_packages.expanded_index = None,
            // The container search and dialog inputs own no expansion state.
            HostToolsTextInput::ContainerSearch
            | HostToolsTextInput::ContainerDialog
            | HostToolsTextInput::ContainerDialogRegistry
            | HostToolsTextInput::ContainerRunName
            | HostToolsTextInput::ContainerRunPorts
            | HostToolsTextInput::ContainerRunVolumes
            | HostToolsTextInput::ContainerRunCommand
            | HostToolsTextInput::MuxDialog => {}
            HostToolsTextInput::ProcessRenice
            | HostToolsTextInput::FilesystemSearch
            | HostToolsTextInput::TmuxDialog
            | HostToolsTextInput::RedisPassword
            | HostToolsTextInput::RedisRename
            | HostToolsTextInput::RedisSetValue
            | HostToolsTextInput::RedisSetValueField
            | HostToolsTextInput::RedisCreateKeyName
            | HostToolsTextInput::RedisCreateKeyValue
            | HostToolsTextInput::RedisCreateKeyField
            | HostToolsTextInput::RedisCreateKeyTtl
            | HostToolsTextInput::RedisTtl
            | HostToolsTextInput::RedisSearch => {}
        }
        true
    }

    pub fn profiler_registry(&self) -> &ProfilerRegistry {
        &self.profiler_registry
    }

    pub fn refresh_pool_snapshot(&mut self, cx: &mut Context<Self>) {
        self.pool_stats = Some(self.ssh_registry.monitor_stats());
        self.pool_summaries = self.ssh_registry.list_connection_summaries();
        self.topology_snapshot = Some(self.ssh_registry.connection_topology_snapshot());
        self.last_pool_refresh = Some(Instant::now());
        cx.notify();
    }

    pub(crate) fn pool_refresh_is_stale(&self, interval: Duration) -> bool {
        self.last_pool_refresh
            .is_none_or(|last_refresh| last_refresh.elapsed() >= interval)
    }

    pub fn pool_stats_snapshot(&self) -> Option<ConnectionPoolMonitorStats> {
        self.pool_stats.clone()
    }

    pub fn pool_summaries_snapshot(&self) -> Vec<ConnectionPoolEntrySummary> {
        // Runtime views receive an immutable projection instead of borrowing
        // the registry-owned cache across GPUI rendering callbacks.
        self.pool_summaries.clone()
    }

    pub fn topology_snapshot(&self) -> Option<ConnectionTopologySnapshot> {
        self.topology_snapshot.clone()
    }

    pub fn monitor_connections(&self) -> Vec<MonitorConnectionOption> {
        let mut connections = if !self.pool_summaries.is_empty() {
            self.pool_summaries
                .iter()
                .filter(|summary| summary.is_displayed_in_pool())
                .map(MonitorConnectionOption::from_pool_summary)
                .collect::<Vec<_>>()
        } else {
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
        };
        // The local target stays last so an existing SSH selection keeps its
        // position. It appears once the localized label has been delivered.
        if let Some(label) = self
            .messages
            .as_ref()
            .map(|messages| messages.local_connection_label.clone())
        {
            connections.push(MonitorConnectionOption::local(label));
        }
        connections
    }

    pub(crate) fn connection_os_type(&self, connection_id: &str) -> Option<String> {
        if connection_id == LOCAL_HOST_CONNECTION_ID {
            return Some(local_host_os_type().to_string());
        }
        self.ssh_registry.get(connection_id).map(|handle| {
            handle
                .remote_env()
                .map(|environment| environment.os_type)
                .unwrap_or_else(|| "Unknown".to_string())
        })
    }

    /// Resolves the execution target for one Host Tools connection. The local
    /// target runs the same generated command lines without an SSH node.
    pub(crate) fn host_command_target(&self, connection_id: &str) -> Option<HostCommandTarget> {
        if connection_id == LOCAL_HOST_CONNECTION_ID {
            return Some(HostCommandTarget::Local);
        }
        self.ssh_registry
            .get(connection_id)
            .map(HostCommandTarget::Ssh)
    }

    pub(crate) fn remote_env_detection_running(&self, connection_id: &str) -> bool {
        self.ssh_registry
            .get(connection_id)
            .is_some_and(|handle| handle.remote_env_detection_running())
    }

    pub(crate) fn compact_monitor_list_state(&self) -> ListState {
        self.compact_monitor_list_state.clone()
    }

    pub(crate) fn sync_compact_monitor_list_signatures(&self, identity: &str, signatures: &[u64]) {
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

    pub fn request_profiler_refresh(&mut self, connection_id: String, cx: &mut Context<Self>) {
        if !self.visibility.sidebar_is_visible() || self.sampling_config.is_empty() {
            return;
        }
        if self
            .ssh_registry
            .get(&connection_id)
            .is_some_and(|handle| handle.remote_env().is_none())
        {
            self.ssh_registry.retry_remote_env_detection(&connection_id);
            cx.notify();
            return;
        }
        if self.profiler_registry.request_refresh(&connection_id) {
            return;
        }
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        self.start_profiler(connection_id, self.sampling_config, runtime, cx);
    }

    pub fn set_runtime_section(&mut self, section: ConnectionRuntimeSection) -> bool {
        if self.active_runtime_section == section {
            return false;
        }
        self.previous_runtime_section = self.active_runtime_section;
        self.active_runtime_section = section;
        true
    }

    pub fn reset_runtime_section(&mut self) {
        self.active_runtime_section = ConnectionRuntimeSection::Overview;
        self.previous_runtime_section = ConnectionRuntimeSection::Overview;
    }

    pub fn selected_connection_id(&self) -> Option<&str> {
        self.selected_connection_id.as_deref()
    }

    pub fn active_tool(&self) -> ContextSidebarTool {
        self.active_tool
    }

    pub fn select_tool(&mut self, tool: ContextSidebarTool, cx: &mut Context<Self>) -> bool {
        if self.active_tool == tool {
            return false;
        }
        self.active_tool = tool;
        cx.notify();
        true
    }

    pub(crate) fn spawn_redis_snapshot_capture(
        &self,
        command: String,
        request: HostRedisSnapshotRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(target) = self.host_command_target(&request.connection_id) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Probe output stays inside the Entity and is parsed before render.
        runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::RedisSnapshot(
                HostRedisSnapshotDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_redis_keys_capture(
        &self,
        command: String,
        request: HostRedisKeysRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(target) = self.host_command_target(&request.connection_id) else {
            return false;
        };
        // The password travels on the exec channel's stdin: never in argv, the
        // command text, an audit record, or a Debug formatter.
        let pattern = request.pattern.clone().unwrap_or_default();
        let mut payload = zeroize::Zeroizing::new(Vec::with_capacity(
            request.password.len() + pattern.len() + 2,
        ));
        payload.extend_from_slice(request.password.as_bytes());
        payload.push(b'\n');
        payload.extend_from_slice(pattern.as_bytes());
        payload.push(b'\n');
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Browse output stays inside the Entity and is parsed before render.
        runtime.spawn(async move {
            let result = target
                .run_secret_command_capture_with_stdin(&command, payload, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::RedisKeys(
                HostRedisKeysDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_redis_value_capture(
        &self,
        command: String,
        request: HostRedisValueRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(target) = self.host_command_target(&request.connection_id) else {
            return false;
        };
        // Password, database index and key ride the exec channel's stdin,
        // never the command line.
        let database = request.database.to_string();
        let Some(payload) = redis_stdin_payload(&request.password, &[&database, &request.key])
        else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        runtime.spawn(async move {
            let result = target
                .run_secret_command_capture_with_stdin(&command, payload, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::RedisValue(
                HostRedisValueDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_redis_set_value_capture(
        &self,
        command: String,
        request: HostRedisSetValueRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(target) = self.host_command_target(&request.connection_id) else {
            return false;
        };
        // Password, database and key are lines; the replacement value follows
        // verbatim and is piped to `redis-cli -x`, never put in argv.
        let database = request.database.to_string();
        let Some(payload) = redis_stdin_payload_with_body(
            &request.password,
            &[&database, &request.key],
            request.value.as_str(),
        ) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        runtime.spawn(async move {
            let result = target
                .run_secret_command_capture_with_stdin(&command, payload, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::RedisSetValue(
                HostRedisSetValueDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_redis_create_key_capture(
        &self,
        command: String,
        request: HostRedisCreateKeyRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(target) = self.host_command_target(&request.connection_id) else {
            return false;
        };
        // Password, database and key are lines; the value follows verbatim and
        // is piped to `redis-cli -x`, never put in argv.
        let database = request.database.to_string();
        let Some(payload) = redis_stdin_payload_with_body(
            &request.password,
            &[&database, &request.key],
            request.value.as_str(),
        ) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        runtime.spawn(async move {
            let result = target
                .run_secret_command_capture_with_stdin(&command, payload, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::RedisCreateKey(
                HostRedisCreateKeyDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_redis_mutation_capture(
        &self,
        command: String,
        request: HostRedisMutationRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let Some(target) = self.host_command_target(&request.connection_id) else {
            return false;
        };
        let database = request.database.to_string();
        let ttl = request.ttl_seconds.map(|seconds| seconds.to_string());
        let values: Vec<&str> = match request.kind {
            HostRedisMutationKind::DeleteMany => {
                let mut values = Vec::with_capacity(1 + request.keys.len());
                values.push(database.as_str());
                values.extend(request.keys.iter().map(String::as_str));
                values
            }
            HostRedisMutationKind::Expire => vec![
                database.as_str(),
                request.key.as_str(),
                ttl.as_deref().unwrap_or("0"),
            ],
            HostRedisMutationKind::Persist => vec![database.as_str(), request.key.as_str()],
            _ => match request.new_key.as_deref() {
                Some(new_key) => vec![database.as_str(), request.key.as_str(), new_key],
                None => vec![database.as_str(), request.key.as_str()],
            },
        };
        let Some(payload) = redis_stdin_payload(&request.password, &values) else {
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        runtime.spawn(async move {
            let result = target
                .run_secret_command_capture_with_stdin(&command, payload, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::RedisMutation(
                HostRedisMutationDelivery { request, result },
            ));
        });
        true
    }

    pub(crate) fn spawn_container_capture(
        &mut self,
        command: String,
        request: HostContainerCaptureRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        self.spawn_container_capture_handle(command, request, timeout, max_output_size, runtime)
            .is_some()
    }

    /// Same capture, but hands back the task handle so a superseded probe
    /// (a 40s discovery run) can be aborted instead of delaying the next host.
    pub(crate) fn spawn_container_capture_handle(
        &mut self,
        command: String,
        request: HostContainerCaptureRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let target = self.host_command_target(&request.connection_id)?;
        let delivery_tx = self.reliable_delivery_tx.clone();
        // Capture output stays inside the Entity and is parsed before render.
        Some(runtime.spawn(async move {
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map_err(|_| ());
            let _ = delivery_tx.send(
                super::delivery::HostToolsReliableDelivery::ContainerCapture(
                    HostContainerCaptureDelivery { request, result },
                ),
            );
        }))
    }

    /// Stops the running container discovery probe. Its delivery is dropped
    /// with the aborted task, so the panel never sees the old host's engines.
    pub(crate) fn cancel_container_discovery(&mut self) {
        if let Some(task) = self.host_container.discovery_task.take() {
            task.abort();
        }
        self.host_container.discovery_running = false;
    }

    /// Audits the action class only; the composed command never reaches the record.
    pub(crate) fn spawn_container_action(
        &self,
        command: String,
        request: HostContainerConfirmRequest,
        timeout: Duration,
        max_output_size: usize,
        runtime: tokio::runtime::Handle,
    ) -> bool {
        let audit = self.host_tool_audit(
            &request.connection_id,
            "container_action",
            Some(&format!("resource={:?}", request.kind)),
        );
        let Some(target) = self.host_command_target(&request.connection_id) else {
            audit.finish(
                oxideterm_audit::AuditOutcome::Failed,
                oxideterm_audit::AuditEvidence::Request,
                None,
                None,
            );
            return false;
        };
        let delivery_tx = self.reliable_delivery_tx.clone();
        runtime.spawn(async move {
            let mut audit_exit_code = None;
            let result = target
                .run_command_capture(&command, timeout, max_output_size)
                .await
                .map(|mut output| {
                    audit_exit_code = output.exit_code;
                    zeroize::Zeroize::zeroize(&mut output.stdout);
                    zeroize::Zeroize::zeroize(&mut output.stderr);
                    output
                })
                .map_err(|_| ());
            let audit_outcome = match (&result, audit_exit_code) {
                (Err(_), _) => oxideterm_audit::AuditOutcome::Failed,
                (_, Some(0)) => oxideterm_audit::AuditOutcome::Succeeded,
                (_, Some(_)) => oxideterm_audit::AuditOutcome::Failed,
                (_, None) => oxideterm_audit::AuditOutcome::Unknown,
            };
            audit.finish(
                audit_outcome,
                if audit_exit_code.is_some() {
                    oxideterm_audit::AuditEvidence::ExitCode
                } else {
                    oxideterm_audit::AuditEvidence::Protocol
                },
                audit_exit_code,
                None,
            );
            let _ = delivery_tx.send(super::delivery::HostToolsReliableDelivery::ContainerAction(
                HostContainerConfirmDelivery { request, result },
            ));
        });
        true
    }

    /// Starts one bounded session-list capture and returns its handle, which
    /// the panel aborts when the request is superseded or its host changes.
    /// Output stays inside this delivery boundary and is matched against the
    /// request identity before anything is rendered.
    /// Audits the provider and the target label only; user-authored labels and
    /// run commands never enter the audit record.
    pub fn select_sidebar_tool(
        &mut self,
        tool: ContextSidebarTool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.select_tool(tool, cx) {
            // Re-clicking the active tool retries a page snapshot that never
            // started, for example while the connection was still coming up.
            self.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
            return false;
        }
        self.ui.retain_input_focus_for_tool(tool);
        if tool != ContextSidebarTool::Processes {
            self.dismiss_process_confirm(cx);
        }
        if tool != ContextSidebarTool::Docker {
            self.dismiss_docker_confirm(cx);
        }
        if tool != ContextSidebarTool::Container {
            self.dismiss_container_confirm(cx);
            // Leaving the panel cancels the capture it opened.
            self.invalidate_container_text_dialog(cx);
        }
        if tool != ContextSidebarTool::Services {
            self.dismiss_service_confirm(cx);
            self.pause_service_refreshes();
        }
        if tool != ContextSidebarTool::Tmux {
            self.dismiss_tmux_confirm(cx);
            self.dismiss_tmux_input_dialog(cx);
            self.dismiss_mux_confirm(cx);
            self.dismiss_mux_input_dialog(cx);
            self.dismiss_mux_read_dialog(cx);
        }
        if tool != ContextSidebarTool::Schedules {
            self.dismiss_schedule_confirm(cx);
        }
        if tool != ContextSidebarTool::Redis {
            // The Redis password is a credential: it lives only while that tool
            // is on screen, and the buffer zeroizes on drop.
            self.ui.host_redis_password = zeroize::Zeroizing::new(String::new());
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
        cx.emit(HostToolsEvent::ToolSelected);
        true
    }

    pub fn selected_connection_id_owned(&self) -> Option<String> {
        self.selected_connection_id.clone()
    }

    pub fn selector_open(&self) -> bool {
        self.selector_open
    }

    pub(crate) fn selector_highlighted_index(&self) -> Option<usize> {
        self.selector_highlighted_index
    }

    pub(crate) fn selector_focus_origin(&self) -> Option<BrowserFocusOrigin> {
        self.selector_focus_origin
    }

    pub fn close_selector(&mut self, clear_focus: bool, cx: &mut Context<Self>) -> bool {
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

    pub(crate) fn toggle_selector_from_pointer(
        &mut self,
        selected_index: usize,
        cx: &mut Context<Self>,
    ) {
        self.selector_focus_origin = Some(BrowserFocusOrigin::Pointer);
        if self.selector_open {
            self.selector_open = false;
            self.selector_highlighted_index = None;
        } else {
            self.selector_open = true;
            self.selector_highlighted_index = Some(selected_index);
        }
        cx.notify();
    }

    pub(crate) fn highlight_selector_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.selector_highlighted_index != Some(index) {
            self.selector_highlighted_index = Some(index);
            cx.notify();
        }
    }

    fn focus_selector_trigger(&mut self, cx: &mut Context<Self>) {
        self.selector_focus_origin = Some(BrowserFocusOrigin::Keyboard);
        cx.notify();
    }

    fn open_selector_from_keyboard(&mut self, selected_index: usize, cx: &mut Context<Self>) {
        self.selector_open = true;
        self.selector_highlighted_index = Some(selected_index);
        self.selector_focus_origin = Some(BrowserFocusOrigin::Keyboard);
        cx.notify();
    }

    fn close_selector_to_keyboard_trigger(&mut self, cx: &mut Context<Self>) {
        self.selector_open = false;
        self.selector_highlighted_index = None;
        self.selector_focus_origin = Some(BrowserFocusOrigin::Keyboard);
        cx.notify();
    }

    fn highlight_selector_from_keyboard(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selector_highlighted_index = Some(index);
        self.selector_focus_origin = Some(BrowserFocusOrigin::Keyboard);
        cx.notify();
    }

    pub fn handle_connection_selector_key(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        if keystroke.modifiers.platform || keystroke.modifiers.control {
            return false;
        }
        let connections = self.monitor_connections();
        if !monitor_connection_can_switch(&connections) {
            self.close_selector(true, cx);
            return false;
        }
        let selected_id = self
            .selected_connection_id
            .as_deref()
            .unwrap_or(&connections[0].connection_id);
        let selected_index = monitor_connection_selected_index(&connections, selected_id);
        let key = keystroke.key.as_str();
        let has_focus = self.selector_focus_origin.is_some();

        if !self.selector_open {
            match key {
                "tab" => self.focus_selector_trigger(cx),
                "enter" | "space" | " " | "arrowdown" | "down" if has_focus => {
                    self.open_selector_from_keyboard(selected_index, cx);
                }
                "escape" if has_focus => {
                    self.close_selector(true, cx);
                }
                _ => return false,
            }
            return true;
        }

        let current = self.selector_highlighted_index.unwrap_or(selected_index);
        let count = connections.len();
        match key {
            "escape" => self.close_selector_to_keyboard_trigger(cx),
            "tab" => {
                self.close_selector(true, cx);
            }
            "arrowdown" | "down" => self.highlight_selector_from_keyboard(
                browser_select_next_index(current, count, BrowserSelectKeyDirection::Next),
                cx,
            ),
            "arrowup" | "up" => self.highlight_selector_from_keyboard(
                browser_select_next_index(current, count, BrowserSelectKeyDirection::Previous),
                cx,
            ),
            "home" => self.highlight_selector_from_keyboard(0, cx),
            "end" => self.highlight_selector_from_keyboard(count - 1, cx),
            "enter" | "space" | " " => self.select_connection_for_active_tool(
                connections[current.min(count - 1)].connection_id.clone(),
                Some(BrowserFocusOrigin::Keyboard),
                cx,
            ),
            _ => return false,
        }
        true
    }

    pub(crate) fn select_connection(
        &mut self,
        connection_id: String,
        focus_origin: Option<BrowserFocusOrigin>,
        cx: &mut Context<Self>,
    ) {
        if self.selected_connection_id.as_deref() != Some(connection_id.as_str()) {
            self.host_tmux.mux.clear_host();
            // The Redis password belongs to the host it was typed for: never
            // send it to the next host, and drop the old host's key tree so a
            // refresh cannot act on it either.
            self.ui.host_redis_password = zeroize::Zeroizing::new(String::new());
            self.host_redis.keys = None;
            self.host_redis.keys_connection_id = None;
            self.host_redis.keys_endpoint = None;
            self.host_redis.keys_cache_endpoint = None;
            // Queued reads were made against the old host: starting one now
            // would read the old node and install its tree under the new host.
            self.host_redis.keys_pending = None;
            // The in-flight read still runs on the old node. Forgetting its
            // marker discards the late result and keeps the new host from
            // waiting behind a request it did not make.
            self.host_redis.keys_running = None;
            self.host_redis.keys_in_flight = false;
            self.host_redis.collapsed_databases.clear();
            self.host_redis.expanded_segments.clear();
        }
        self.selected_connection_id = Some(connection_id);
        self.selector_open = false;
        self.selector_highlighted_index = None;
        self.selector_focus_origin = focus_origin;
        cx.notify();
    }

    pub fn select_connection_for_active_tool(
        &mut self,
        connection_id: String,
        focus_origin: Option<BrowserFocusOrigin>,
        cx: &mut Context<Self>,
    ) {
        self.select_connection(connection_id, focus_origin, cx);
        self.dismiss_process_confirm(cx);
        self.dismiss_docker_confirm(cx);
        self.dismiss_container_confirm(cx);
        // Switching hosts also cancels a running capture: its output belongs
        // to the host that was selected when it was requested.
        self.invalidate_container_text_dialog(cx);
        self.dismiss_service_confirm(cx);
        self.dismiss_tmux_input_dialog(cx);
        self.dismiss_tmux_confirm(cx);
        self.dismiss_mux_confirm(cx);
        self.dismiss_mux_input_dialog(cx);
        self.dismiss_mux_read_dialog(cx);
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

    pub fn request_active_tool_snapshot(
        &mut self,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        if !self.visibility.sidebar_is_visible()
            || !self.active_tool.monitoring_enabled(&self.monitoring)
        {
            return;
        }
        let Some(runtime) = self.lifecycle_runtime.clone() else {
            return;
        };
        let Some(messages) = self.messages.clone() else {
            return;
        };
        let connection_id = match self.selected_connection_id_owned() {
            Some(connection_id) => connection_id,
            None => {
                // The panel falls back to the first monitor connection; the
                // request must target that same connection or its delivery is
                // filtered out as belonging to another node.
                let Some(connection) = self.monitor_connections().into_iter().next() else {
                    return;
                };
                self.selected_connection_id = Some(connection.connection_id.clone());
                cx.notify();
                connection.connection_id
            }
        };
        #[cfg(test)]
        if let Some(dispatches) = self.test_snapshot_dispatches.as_mut()
            && !matches!(
                self.active_tool,
                ContextSidebarTool::Monitor
                    | ContextSidebarTool::Gpu
                    | ContextSidebarTool::Processes
                    | ContextSidebarTool::Docker
                    | ContextSidebarTool::Container
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
            ContextSidebarTool::Tmux if self.host_tmux.mux.provider.is_some() => {
                self.refresh_mux_sessions(connection_id, feedback, cx);
                Vec::new()
            }
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
            ContextSidebarTool::Redis => {
                self.request_redis_snapshot(
                    connection_id,
                    runtime,
                    messages.redis_connection_missing,
                    messages.redis_probe_failed,
                    cx,
                );
                Vec::new()
            }
            ContextSidebarTool::Container => {
                self.request_container_discovery(connection_id, feedback, cx);
                Vec::new()
            }
            ContextSidebarTool::Monitor
            | ContextSidebarTool::Gpu
            | ContextSidebarTool::Processes
            | ContextSidebarTool::Docker => Vec::new(),
        };
        for notice in notices {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    pub fn take_selected_connection(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let selected = self.selected_connection_id.take();
        self.close_selector(true, cx);
        selected
    }

    pub fn ensure_selected_connection(
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
            self.host_tmux.mux.clear_host();
            // The dialog carries typed text bound to the host that just went
            // away; keeping it open would let that value be submitted against
            // the fallback connection.
            self.dismiss_mux_input_dialog(cx);
            self.selected_connection_id = live_connection_ids.first().cloned();
            cx.notify();
        }
        self.selected_connection_id.clone()
    }

    pub(crate) fn stop_profiler_sampling(&self) {
        self.profiler_registry.stop_all();
    }

    pub(crate) fn profiler_connection_ids(&self) -> Vec<String> {
        self.profiler_registry.connection_ids()
    }

    pub(crate) fn remove_profiler_connection(&self, connection_id: &str) {
        self.profiler_registry.remove(connection_id);
        self.monitor_expanded_cpu_connections
            .borrow_mut()
            .remove(connection_id);
    }

    pub(crate) fn monitor_cpu_cores_expanded(&self, connection_id: &str) -> bool {
        self.monitor_expanded_cpu_connections
            .borrow()
            .contains(connection_id)
    }

    pub(crate) fn toggle_monitor_cpu_cores(
        &mut self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) {
        let expanded = self.monitor_expanded_cpu_connections.get_mut();
        if !expanded.remove(&connection_id) {
            expanded.insert(connection_id);
        }
        cx.notify();
    }

    pub(crate) fn profiler_connection_missing(&self, connection_id: &str) -> bool {
        self.profiler_registry.state(connection_id).is_none()
    }

    pub(crate) fn start_profiler(
        &self,
        connection_id: String,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        if connection_id == LOCAL_HOST_CONNECTION_ID {
            // The local machine has no SSH handle; its samples run through the
            // same command builders via one-shot local captures.
            self.profiler_registry.start_with_sampler_on_config(
                connection_id,
                Arc::new(LocalResourceSampler),
                local_host_os_type(),
                sampling_config,
                Some(self.profiler_update_tx.clone()),
                runtime,
            );
            cx.notify();
            return;
        }
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

    pub(crate) fn sync_gpu_sampling(
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
        if connection_id == LOCAL_HOST_CONNECTION_ID {
            self.host_gpu.snapshot_connection_id = Some(connection_id.clone());
            self.host_gpu.snapshot = None;
            self.host_gpu.expanded_uuid = None;
            self.host_gpu.sampling_task = Some(start_gpu_sampling_on(
                connection_id,
                Arc::new(LocalResourceSampler),
                local_host_os_type().to_string(),
                self.host_gpu.update_tx.clone(),
                runtime,
            ));
            cx.notify();
            return;
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
        if let Some(context) = self.ssh_consumer_context.as_ref()
            && let Some(node_id) = context
                .node_router
                .node_id_for_connection(handle.connection_id())
            && context
                .node_router
                .node_runtime_snapshot(&node_id)
                .is_some_and(|snapshot| {
                    snapshot
                        .config
                        .ssh_channel_strategy
                        .requires_dedicated_consumers()
                })
        {
            return Arc::new(oxideterm_ssh::DedicatedNodeResourceSampler::new(
                context.node_router.clone(),
                node_id,
                context.prompt_handler.clone(),
                context.managed_key_resolver.clone(),
            ));
        }
        Arc::new(handle)
    }

    pub(crate) fn gpu_snapshot_for(&self, connection_id: &str) -> Option<GpuSnapshot> {
        self.host_gpu
            .snapshot
            .as_ref()
            .filter(|_| self.host_gpu.snapshot_connection_id.as_deref() == Some(connection_id))
            .cloned()
    }

    pub(crate) fn gpu_sampling_is_running(&self, connection_id: &str) -> bool {
        self.host_gpu
            .sampling_task
            .as_ref()
            .is_some_and(|task| task.connection_id() == connection_id && !task.is_finished())
    }

    pub(crate) fn gpu_list_state(&self) -> ListState {
        self.host_gpu.list_state.clone()
    }

    pub(crate) fn toggle_gpu_device(&mut self, device_uuid: String, cx: &mut Context<Self>) {
        if self.host_gpu.expanded_uuid.as_deref() == Some(device_uuid.as_str()) {
            self.host_gpu.expanded_uuid = None;
        } else {
            self.host_gpu.expanded_uuid = Some(device_uuid);
        }
        cx.notify();
    }

    pub(crate) fn gpu_device_is_expanded(&self, device_uuid: &str) -> bool {
        self.host_gpu.expanded_uuid.as_deref() == Some(device_uuid)
    }

    pub(crate) fn sync_gpu_list_state(
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

    pub(crate) fn request_gpu_refresh(&mut self, connection_id: String, cx: &mut Context<Self>) {
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

    pub(crate) fn restart_gpu_sampling(
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

impl HostToolsEntity {
    /// Escaping the capture dialog cancels its request too. The spawn cannot be
    /// aborted, so bumping the dialog epoch is what keeps the late delivery
    /// from landing in a later dialog that asks for the same capture.
    pub(crate) fn invalidate_container_text_dialog(&mut self, cx: &mut Context<Self>) {
        if self.host_container.invalidate_container_text_dialog() {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests;
