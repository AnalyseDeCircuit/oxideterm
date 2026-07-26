use super::*;

use oxideterm_connection_monitor::ResourceSampler;

/// Owns Host Tools sampling state independently from WorkspaceApp and SSH nodes.
pub(in crate::workspace) struct HostToolsEntity {
    pub(super) profiler_registry: ProfilerRegistry,
    pub(super) profiler_update_tx: tokio::sync::mpsc::UnboundedSender<ProfilerUpdate>,
    pub(super) sampler_delivery_wake: crate::workspace::delivery::ActiveDeliveryWake,
    pub(super) sampler_delivery_rx:
        std::sync::mpsc::Receiver<super::delivery::HostToolsSamplerDelivery>,
    pub(super) host_gpu: HostGpuViewState,
    pub(in crate::workspace) active_runtime_section: ConnectionRuntimeSection,
    pub(in crate::workspace) previous_runtime_section: ConnectionRuntimeSection,
    pub(in crate::workspace) section_list_state: ListState,
    pub(in crate::workspace) section_list_cache: RefCell<VirtualListSignatureCache>,
}

impl HostToolsEntity {
    pub(in crate::workspace) fn new(
        profiler_update_tx: tokio::sync::mpsc::UnboundedSender<ProfilerUpdate>,
        profiler_update_rx: tokio::sync::mpsc::UnboundedReceiver<ProfilerUpdate>,
        cx: &mut Context<Self>,
    ) -> Self {
        let sampler_delivery_wake = crate::workspace::delivery::ActiveDeliveryWake::default();
        let (sampler_delivery_tx, sampler_delivery_rx) =
            crate::workspace::delivery::ActiveDeliverySender::channel_with_wake(
                sampler_delivery_wake.clone(),
            );
        let (gpu_update_tx, gpu_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = Self {
            profiler_registry: ProfilerRegistry::new(),
            profiler_update_tx,
            sampler_delivery_wake,
            sampler_delivery_rx,
            host_gpu: HostGpuViewState::new(gpu_update_tx),
            active_runtime_section: ConnectionRuntimeSection::Overview,
            previous_runtime_section: ConnectionRuntimeSection::Overview,
            // Monitor pages have variable-height browser sections and retain one
            // ListState owner across the main tab and detached-window surfaces.
            section_list_state: ListState::new(
                CONNECTION_MONITOR_SECTION_LIST_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(CONNECTION_MONITOR_SECTION_LIST_ESTIMATED_HEIGHT),
                    CONNECTION_MONITOR_SECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            section_list_cache: RefCell::new(VirtualListSignatureCache::default()),
        };
        entity.schedule_sampler_delivery(
            super::delivery::HostToolsDeliveryBridges {
                profiler_update_rx,
                gpu_update_rx,
                sampler_delivery_tx,
            },
            cx,
        );
        entity
    }

    pub(in crate::workspace) fn profiler_registry(&self) -> &ProfilerRegistry {
        &self.profiler_registry
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
        ssh_registry: &SshConnectionRegistry,
        sampling_config: oxideterm_connection_monitor::ResourceSamplingConfig,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = ssh_registry.get(&connection_id) else {
            return;
        };
        let Some(os_type) = handle.remote_env().map(|environment| environment.os_type) else {
            // Environment detection owns OS selection; never guess a probe dialect.
            return;
        };
        let sampler: Arc<dyn ResourceSampler> = Arc::new(handle);
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
        ssh_registry: &SshConnectionRegistry,
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
        let Some(handle) = ssh_registry.get(&connection_id) else {
            return;
        };
        let Some(os_type) = handle.remote_env().map(|environment| environment.os_type) else {
            return;
        };
        let sampler: Arc<dyn ResourceSampler> = Arc::new(handle);
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

    pub(super) fn restart_gpu_sampling(
        &mut self,
        connection_id: String,
        enabled_and_visible: bool,
        selected_connection_id: Option<String>,
        ssh_registry: &SshConnectionRegistry,
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
        self.sync_gpu_sampling(
            enabled_and_visible,
            selected_connection_id,
            ssh_registry,
            runtime,
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn runtime_navigation_transition_is_entity_owned(cx: &mut TestAppContext) {
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = cx.new(|cx| HostToolsEntity::new(profiler_update_tx, profiler_update_rx, cx));

        entity.update(cx, |entity, _cx| {
            assert_eq!(
                entity.active_runtime_section,
                ConnectionRuntimeSection::Overview
            );
            assert!(entity.set_runtime_section(ConnectionRuntimeSection::Topology));
            assert_eq!(
                entity.previous_runtime_section,
                ConnectionRuntimeSection::Overview
            );
            assert_eq!(
                entity.active_runtime_section,
                ConnectionRuntimeSection::Topology
            );
            assert!(!entity.set_runtime_section(ConnectionRuntimeSection::Topology));
        });
    }
}
