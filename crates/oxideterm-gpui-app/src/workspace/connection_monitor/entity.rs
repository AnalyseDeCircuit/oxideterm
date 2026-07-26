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
    selected_connection_id: Option<String>,
    selector_open: bool,
    selector_highlighted_index: Option<usize>,
    selector_focus_origin: Option<browser_behavior::BrowserFocusOrigin>,
    tab_scroll_handle: ScrollHandle,
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
            selected_connection_id: None,
            selector_open: false,
            selector_highlighted_index: None,
            selector_focus_origin: None,
            tab_scroll_handle: ScrollHandle::new(),
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

    pub(in crate::workspace) fn selected_connection_id(&self) -> Option<&str> {
        self.selected_connection_id.as_deref()
    }

    pub(super) fn tab_scroll_handle(&self) -> ScrollHandle {
        self.tab_scroll_handle.clone()
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
        // Events carry only the stable connection id; registry state and SSH
        // credentials remain behind the workspace runtime boundary.
        cx.emit(HostToolsEvent::RefreshGpu { connection_id });
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

    #[gpui::test]
    fn connection_selector_state_is_entity_owned(cx: &mut TestAppContext) {
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = cx.new(|cx| HostToolsEntity::new(profiler_update_tx, profiler_update_rx, cx));

        entity.update(cx, |entity, cx| {
            let live_connections = vec!["connection-1".to_string(), "connection-2".to_string()];
            assert_eq!(
                entity.ensure_selected_connection(&live_connections, cx),
                Some("connection-1".to_string())
            );
            entity.toggle_selector_from_pointer(0, cx);
            assert!(entity.selector_open());
            assert_eq!(entity.selector_highlighted_index(), Some(0));

            entity.select_connection("connection-2".to_string(), None, cx);
            assert_eq!(entity.selected_connection_id(), Some("connection-2"));
            assert!(!entity.selector_open());

            assert_eq!(
                entity.ensure_selected_connection(&["connection-1".to_string()], cx),
                Some("connection-1".to_string())
            );
            assert_eq!(entity.ensure_selected_connection(&[], cx), None);
            assert_eq!(entity.selected_connection_id(), None);
        });
    }

    #[gpui::test]
    fn gpu_actions_and_expansion_are_entity_owned(cx: &mut TestAppContext) {
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = cx.new(|cx| HostToolsEntity::new(profiler_update_tx, profiler_update_rx, cx));
        let mut events = cx.events(&entity);

        entity.update(cx, |entity, cx| {
            entity.toggle_gpu_device("gpu-1".to_string(), cx);
            assert!(entity.gpu_device_is_expanded("gpu-1"));
            entity.request_gpu_refresh("connection-1".to_string(), cx);
        });

        assert_eq!(
            events.try_recv().unwrap(),
            HostToolsEvent::RefreshGpu {
                connection_id: "connection-1".to_string(),
            }
        );
        entity.update(cx, |entity, cx| {
            entity.toggle_gpu_device("gpu-1".to_string(), cx);
            assert!(!entity.gpu_device_is_expanded("gpu-1"));
        });
    }
}
