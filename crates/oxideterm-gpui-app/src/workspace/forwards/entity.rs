// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::workspace::delivery;
use crate::workspace::{
    FORWARDS_SECTION_LIST_INITIAL_ITEM_COUNT, FORWARDS_TABLE_ROW_LIST_ESTIMATED_HEIGHT,
    FORWARDS_TABLE_ROW_LIST_INITIAL_ITEM_COUNT, VirtualListSignatureCache,
};
use gpui::{ListAlignment, ListState};
use std::cell::RefCell;

pub(in crate::workspace) enum ForwardingDeliveryIntent {
    Operation {
        tab_id: TabId,
        message_key: &'static str,
        sync_saved_forwards_on_success: bool,
        binding: Option<(String, String, ConnectionConsumer)>,
        result: Result<(), String>,
    },
    Binding {
        binding: Option<(String, String, ConnectionConsumer)>,
    },
    PortScan {
        node_id: NodeId,
        binding: Option<(String, String, ConnectionConsumer)>,
    },
    ReconnectRestore {
        node_id: NodeId,
        result: PhaseResult,
        restored: u32,
        detail: String,
        job_id: String,
        created_forwards: Vec<(String, String)>,
        bindings: Vec<(String, String, ConnectionConsumer)>,
    },
    Runtime(ForwardEvent),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum ForwardingWorkspaceEvent {
    DeliveryReady,
    SamplingDue,
}

/// Owns forwarding UI delivery and sampling state without owning tunnel lifetime.
pub(in crate::workspace) struct ForwardingWorkspaceEntity {
    pub(super) view: ForwardsViewState,
    tab_nodes: HashMap<TabId, NodeId>,
    sampling_visible: bool,
    sampling_generation: u64,
    sampling_task: Option<gpui::Task<()>>,
    #[cfg(test)]
    sampling_tick_count: usize,
    pub(super) section_list_state: ListState,
    pub(super) section_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) table_row_list_state: ListState,
    pub(super) table_row_list_cache: RefCell<VirtualListSignatureCache>,
    worker_tx: delivery::ActiveDeliverySender<ForwardingWorkerResult>,
    worker_rx: std::sync::mpsc::Receiver<ForwardingWorkerResult>,
    runtime_event_rx: std::sync::mpsc::Receiver<ForwardEvent>,
    runtime_service: ForwardingRuntimeService,
    runtime_snapshots: HashMap<NodeId, ForwardingRuntimeSnapshot>,
    delivery_intents: VecDeque<ForwardingDeliveryIntent>,
    pub(super) port_detection_by_node: HashMap<NodeId, PortDetectionViewState>,
    port_profiler_nodes: std::collections::HashSet<NodeId>,
}

impl ForwardingWorkspaceEntity {
    #[cfg(test)]
    pub(super) fn test_fixture() -> Self {
        let (worker_tx, worker_rx) = delivery::ActiveDeliverySender::channel();
        let (_runtime_tx, runtime_event_rx) = std::sync::mpsc::channel();
        Self {
            view: ForwardsViewState::default(),
            tab_nodes: HashMap::new(),
            sampling_visible: false,
            sampling_generation: 0,
            sampling_task: None,
            sampling_tick_count: 0,
            section_list_state: ListState::new(0, ListAlignment::Top, px(0.0)),
            section_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            table_row_list_state: ListState::new(0, ListAlignment::Top, px(0.0)),
            table_row_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            worker_tx,
            worker_rx,
            runtime_event_rx,
            runtime_service: ForwardingRuntimeService::test_fixture(),
            runtime_snapshots: HashMap::new(),
            delivery_intents: VecDeque::new(),
            port_detection_by_node: HashMap::new(),
            port_profiler_nodes: std::collections::HashSet::new(),
        }
    }

    pub(in crate::workspace) fn new(
        worker_tx: delivery::ActiveDeliverySender<ForwardingWorkerResult>,
        worker_rx: std::sync::mpsc::Receiver<ForwardingWorkerResult>,
        runtime_event_rx: std::sync::mpsc::Receiver<ForwardEvent>,
        runtime_service: ForwardingRuntimeService,
        cx: &mut Context<Self>,
    ) -> Self {
        let entity = Self {
            view: ForwardsViewState::default(),
            tab_nodes: HashMap::new(),
            sampling_visible: false,
            sampling_generation: 0,
            sampling_task: None,
            #[cfg(test)]
            sampling_tick_count: 0,
            section_list_state: ListState::new(
                FORWARDS_SECTION_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(FORWARDS_SECTION_LIST_ESTIMATED_HEIGHT),
                    FORWARDS_SECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            section_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            table_row_list_state: ListState::new(
                FORWARDS_TABLE_ROW_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(FORWARDS_TABLE_ROW_LIST_ESTIMATED_HEIGHT),
                    FORWARDS_TABLE_ROW_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            table_row_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            worker_tx,
            worker_rx,
            runtime_event_rx,
            runtime_service,
            runtime_snapshots: HashMap::new(),
            delivery_intents: VecDeque::new(),
            port_detection_by_node: HashMap::new(),
            port_profiler_nodes: std::collections::HashSet::new(),
        };
        entity.schedule_delivery(cx);
        entity
    }

    pub(super) fn request_operation(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        owner_connection_id: Option<String>,
        message_key: &'static str,
        sync_saved_forwards_on_success: bool,
        operation: ForwardingRuntimeOperation,
    ) {
        self.begin_operation();
        self.runtime_service.submit_operation(
            tab_id,
            node_id,
            owner_connection_id,
            message_key,
            sync_saved_forwards_on_success,
            operation,
            self.worker_tx.clone(),
        );
    }

    pub(super) fn request_port_scan(
        &mut self,
        node_id: NodeId,
        owner_connection_id: Option<String>,
        restart_degraded_profiler: bool,
    ) {
        self.mark_port_scan_started(node_id.clone());
        self.runtime_service.submit_port_scan(
            node_id,
            owner_connection_id,
            restart_degraded_profiler,
            self.worker_tx.clone(),
        );
    }

    pub(in crate::workspace) fn request_session_restore(&self, node_id: NodeId) {
        // Reconnect restores the node-owned runtime without exposing the
        // Entity's delivery sender to workspace coordination code.
        self.runtime_service
            .submit_session_restore(node_id, self.worker_tx.clone());
    }

    pub(in crate::workspace) fn request_reconnect_restore(
        &self,
        request: ReconnectForwardRestoreRequest,
    ) {
        self.runtime_service
            .submit_reconnect_restore(request, self.worker_tx.clone());
    }

    pub(super) fn refresh_runtime_snapshot(&mut self, node_id: &NodeId) -> bool {
        let snapshot = self.runtime_service.snapshot_for_node(node_id);
        if self.runtime_snapshots.get(node_id) == Some(&snapshot) {
            return false;
        }
        self.runtime_snapshots.insert(node_id.clone(), snapshot);
        true
    }

    pub(super) fn runtime_snapshot(&self, node_id: &NodeId) -> ForwardingRuntimeSnapshot {
        self.runtime_snapshots
            .get(node_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::workspace) fn node_for_tab(&self, tab_id: TabId) -> Option<NodeId> {
        self.tab_nodes.get(&tab_id).cloned()
    }

    pub(in crate::workspace) fn tab_for_node(&self, node_id: &NodeId) -> Option<TabId> {
        self.tab_nodes
            .iter()
            .find_map(|(tab_id, mapped_node_id)| (mapped_node_id == node_id).then_some(*tab_id))
    }

    pub(in crate::workspace) fn tab_matches_node(&self, tab_id: TabId, node_id: &NodeId) -> bool {
        self.tab_nodes.get(&tab_id) == Some(node_id)
    }

    pub(in crate::workspace) fn map_tab_to_node(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        _cx: &mut Context<Self>,
    ) {
        self.tab_nodes.insert(tab_id, node_id.clone());
        self.refresh_runtime_snapshot(&node_id);
    }

    pub(in crate::workspace) fn unmap_tab(&mut self, tab_id: TabId) -> Option<NodeId> {
        // Removing a view mapping must not release the registry-owned tunnel.
        let removed = self.tab_nodes.remove(&tab_id);
        if let Some(node_id) = removed.as_ref()
            && !self.tab_nodes.values().any(|mapped| mapped == node_id)
        {
            self.runtime_snapshots.remove(node_id);
        }
        if removed.is_some() && self.tab_nodes.is_empty() {
            // Cancel the page sampler without touching long-lived runtime state.
            self.stop_sampling();
        }
        removed
    }

    pub(in crate::workspace) fn tab_node_mappings(&self) -> &HashMap<TabId, NodeId> {
        &self.tab_nodes
    }

    pub(in crate::workspace) fn set_sampling_visible(
        &mut self,
        visible: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let should_sample = visible && !self.tab_nodes.is_empty();
        if self.sampling_visible == should_sample {
            return false;
        }
        if !should_sample {
            self.stop_sampling();
            return true;
        }

        self.sampling_visible = true;
        self.sampling_generation = self.sampling_generation.wrapping_add(1);
        self.view.last_stats_refresh = None;
        for state in self.port_detection_by_node.values_mut() {
            if !state.port_scan_pending {
                state.last_port_scan_started = None;
            }
        }
        self.schedule_sampling(self.sampling_generation, cx);
        true
    }

    pub(in crate::workspace) fn sampling_visible(&self) -> bool {
        self.sampling_visible
    }

    fn stop_sampling(&mut self) {
        if !self.sampling_visible && self.sampling_task.is_none() {
            return;
        }
        self.sampling_visible = false;
        self.sampling_generation = self.sampling_generation.wrapping_add(1);
        // Dropping the Entity-owned GPUI task cancels its pending Timer.
        self.sampling_task.take();
        for state in self.port_detection_by_node.values_mut() {
            if !state.port_scan_pending {
                state.last_port_scan_started = None;
            }
        }
    }

    fn schedule_sampling(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.sampling_task = Some(cx.spawn(async move |entity, cx| {
            loop {
                Timer::after(FORWARDS_SAMPLING_TICK_INTERVAL).await;
                let keep_running = entity
                    .update(cx, |entity, cx| entity.apply_sampling_tick(generation, cx))
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        }));
    }

    fn apply_sampling_tick(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if self.sampling_generation != generation
            || !self.sampling_visible
            || self.tab_nodes.is_empty()
        {
            return false;
        }
        #[cfg(test)]
        {
            self.sampling_tick_count = self.sampling_tick_count.saturating_add(1);
        }
        cx.emit(ForwardingWorkspaceEvent::SamplingDue);
        true
    }

    pub(in crate::workspace) fn take_delivery_intents(
        &mut self,
    ) -> VecDeque<ForwardingDeliveryIntent> {
        std::mem::take(&mut self.delivery_intents)
    }

    #[cfg(test)]
    pub(in crate::workspace) fn port_detection_state(
        &self,
        node_id: &NodeId,
    ) -> Option<PortDetectionViewState> {
        self.port_detection_by_node.get(node_id).cloned()
    }

    pub(in crate::workspace) fn track_port_profiler(&mut self, node_id: NodeId) {
        self.port_profiler_nodes.insert(node_id);
    }

    pub(in crate::workspace) fn untrack_port_profiler(&mut self, node_id: &NodeId) {
        self.port_profiler_nodes.remove(node_id);
        self.port_detection_by_node.remove(node_id);
    }

    pub(in crate::workspace) fn tracked_port_profiler_nodes(&self) -> Vec<NodeId> {
        self.port_profiler_nodes.iter().cloned().collect()
    }

    pub(in crate::workspace) fn port_scan_pending(&self, node_id: &NodeId) -> bool {
        self.port_detection_by_node
            .get(node_id)
            .is_some_and(|state| state.port_scan_pending)
    }

    pub(in crate::workspace) fn mark_port_scan_not_ready(&mut self, node_id: NodeId) {
        self.port_detection_by_node
            .entry(node_id)
            .or_default()
            .port_scan_pending = false;
    }

    pub(in crate::workspace) fn mark_port_scan_started(&mut self, node_id: NodeId) {
        let state = self.port_detection_by_node.entry(node_id).or_default();
        state.port_scan_pending = true;
        state.port_scan_error = None;
        state.last_port_scan_started = Some(Instant::now());
    }

    pub(in crate::workspace) fn port_scan_due(&self, node_id: &NodeId, interval: Duration) -> bool {
        self.port_detection_by_node
            .get(node_id)
            .is_none_or(|state| {
                !state.port_scan_pending
                    && state
                        .last_port_scan_started
                        .is_none_or(|last| last.elapsed() >= interval)
            })
    }

    pub(in crate::workspace) fn reset_hidden_port_scan_schedule(&mut self, node_id: &NodeId) {
        if let Some(state) = self.port_detection_by_node.get_mut(node_id)
            && !state.port_scan_pending
        {
            // A newly visible mount should restart sampling immediately.
            state.last_port_scan_started = None;
        }
    }

    pub(in crate::workspace) fn dismiss_detected_port(&mut self, node_id: &NodeId, port: u16) {
        self.runtime_service
            .ignore_detected_port_for_node(node_id, port);
        self.view.new_ports.retain(|detected| detected.port != port);
        if let Some(state) = self.port_detection_by_node.get_mut(node_id) {
            state.new_ports.retain(|detected| detected.port != port);
        }
    }

    fn schedule_delivery(&self, cx: &mut Context<Self>) {
        let delivery_wake = self.worker_tx.wake();
        let release_wake = delivery_wake.clone();
        cx.on_release(move |_, _| {
            // Entity release stops only the UI waiter. Registry-owned tunnels
            // and node consumers keep their independent runtime lifetime.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                delivery_wake.wait().await;
                let should_drain = delivery_wake.take();
                let stopped = delivery_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_deliveries(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        delivery_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_deliveries(&mut self, cx: &mut Context<Self>) -> bool {
        let worker_drain =
            delivery::drain_channel(&self.worker_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        for result in worker_drain.items {
            match result {
                ForwardingWorkerResult::Operation {
                    tab_id,
                    message_key,
                    sync_saved_forwards_on_success,
                    binding,
                    result,
                } => {
                    if let Some(node_id) = self.node_for_tab(tab_id) {
                        self.refresh_runtime_snapshot(&node_id);
                    }
                    self.delivery_intents
                        .push_back(ForwardingDeliveryIntent::Operation {
                            tab_id,
                            message_key,
                            sync_saved_forwards_on_success,
                            binding,
                            result,
                        });
                }
                ForwardingWorkerResult::Binding { binding } => self
                    .delivery_intents
                    .push_back(ForwardingDeliveryIntent::Binding { binding }),
                ForwardingWorkerResult::PortScan {
                    node_id,
                    connection_id,
                    binding,
                    result,
                } => {
                    self.apply_port_detection_result(&node_id, connection_id, result);
                    self.refresh_runtime_snapshot(&node_id);
                    self.delivery_intents
                        .push_back(ForwardingDeliveryIntent::PortScan { node_id, binding });
                }
                ForwardingWorkerResult::ReconnectRestore {
                    node_id,
                    result,
                    restored,
                    detail,
                    job_id,
                    created_forwards,
                    bindings,
                } => self
                    .delivery_intents
                    .push_back(ForwardingDeliveryIntent::ReconnectRestore {
                        node_id,
                        result,
                        restored,
                        detail,
                        job_id,
                        created_forwards,
                        bindings,
                    }),
            }
        }

        let event_drain =
            delivery::drain_channel(&self.runtime_event_rx, delivery::LIFECYCLE_DELIVERY_BUDGET);
        for event in event_drain.items {
            match &event {
                ForwardEvent::StatsUpdated {
                    session_id,
                    forward_id,
                    stats,
                } => {
                    if let Some(node_id) = ForwardingRuntimeService::node_id_for_session(session_id)
                        && let Some(snapshot) = self.runtime_snapshots.get_mut(&node_id)
                        && snapshot.rules.iter().any(|rule| {
                            rule.id == *forward_id && rule.status == ForwardStatus::Active
                        })
                    {
                        // A stats read emits this event. Apply its payload directly so
                        // delivery cannot refresh stats and recursively emit another event.
                        snapshot
                            .stats_by_forward_id
                            .insert(forward_id.clone(), stats.clone());
                    }
                }
                ForwardEvent::StatusChanged { session_id, .. }
                | ForwardEvent::SessionSuspended { session_id, .. } => {
                    if let Some(node_id) = ForwardingRuntimeService::node_id_for_session(session_id)
                    {
                        self.refresh_runtime_snapshot(&node_id);
                    }
                }
                ForwardEvent::PortDetected { .. } => {}
            }
            self.delivery_intents
                .push_back(ForwardingDeliveryIntent::Runtime(event));
        }
        if !self.delivery_intents.is_empty() {
            cx.emit(ForwardingWorkspaceEvent::DeliveryReady);
            cx.notify();
        }
        worker_drain.outcome.backlog_remaining || event_drain.outcome.backlog_remaining
    }

    pub(in crate::workspace) fn apply_port_detection_result(
        &mut self,
        node_id: &NodeId,
        connection_id: Option<String>,
        result: Result<PortDetectionSnapshot, String>,
    ) {
        let state = self
            .port_detection_by_node
            .entry(node_id.clone())
            .or_default();
        if connection_id.is_some() && state.connection_id != connection_id {
            // Detection is connection-scoped. Reconnect must discard samples
            // and dismissals associated with the previous transport.
            state.connection_id = connection_id;
            state.detected_ports.clear();
            state.new_ports.clear();
            state.has_scanned_ports = false;
            state.port_scan_error = None;
        }
        state.port_scan_pending = false;
        match result {
            Ok(snapshot) => {
                state.has_scanned_ports = snapshot.has_scanned;
                state.detected_ports = snapshot.all_ports;
                if !snapshot.new_ports.is_empty() {
                    let existing = state
                        .new_ports
                        .iter()
                        .map(|port| port.port)
                        .collect::<std::collections::HashSet<_>>();
                    state.new_ports.extend(
                        snapshot
                            .new_ports
                            .into_iter()
                            .filter(|port| !existing.contains(&port.port)),
                    );
                }
                if !snapshot.closed_ports.is_empty() {
                    let closed = snapshot
                        .closed_ports
                        .iter()
                        .map(|port| port.port)
                        .collect::<std::collections::HashSet<_>>();
                    state.new_ports.retain(|port| !closed.contains(&port.port));
                }
                state.port_scan_error = None;
            }
            Err(_error) => {
                // Sampling failures are retried while the surface is visible;
                // they do not replace user-action errors in the form.
                state.port_scan_error = None;
            }
        }
    }
}

impl gpui::EventEmitter<ForwardingWorkspaceEvent> for ForwardingWorkspaceEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};

    #[test]
    fn connection_handoff_discards_previous_detection_state() {
        let mut entity = ForwardingWorkspaceEntity::test_fixture();
        let node_id = NodeId::new("forward-test");
        entity.apply_port_detection_result(
            &node_id,
            Some("connection-a".to_string()),
            Ok(PortDetectionSnapshot {
                new_ports: vec![DetectedPort {
                    port: 3000,
                    bind_addr: "127.0.0.1".to_string(),
                    process_name: None,
                    pid: None,
                }],
                closed_ports: Vec::new(),
                all_ports: Vec::new(),
                has_scanned: true,
            }),
        );

        entity.apply_port_detection_result(
            &node_id,
            Some("connection-b".to_string()),
            Ok(PortDetectionSnapshot::default()),
        );

        let state = entity.port_detection_state(&node_id).unwrap();
        assert_eq!(state.connection_id.as_deref(), Some("connection-b"));
        assert!(state.new_ports.is_empty());
    }

    #[gpui::test]
    fn closing_tab_removes_only_the_view_mapping(cx: &mut TestAppContext) {
        let entity = cx.new(|_cx| ForwardingWorkspaceEntity::test_fixture());
        let tab_id = TabId(7);
        let node_id = NodeId::new("shared-forward");
        entity.update(cx, |entity, cx| {
            entity.map_tab_to_node(tab_id, node_id.clone(), cx);
            entity.set_sampling_visible(true, cx);
        });

        cx.read(|cx| {
            assert_eq!(entity.read(cx).node_for_tab(tab_id), Some(node_id.clone()));
            assert!(entity.read(cx).runtime_snapshots.contains_key(&node_id));
            assert!(entity.read(cx).sampling_visible);
            assert_eq!(entity.read(cx).sampling_generation, 1);
            assert!(entity.read(cx).sampling_task.is_some());
        });
        let removed = entity.update(cx, |entity, _cx| entity.unmap_tab(tab_id));
        assert_eq!(removed, Some(node_id.clone()));
        cx.read(|cx| {
            assert!(entity.read(cx).node_for_tab(tab_id).is_none());
            assert!(!entity.read(cx).runtime_snapshots.contains_key(&node_id));
            assert!(!entity.read(cx).sampling_visible);
            assert_eq!(entity.read(cx).sampling_generation, 2);
            assert!(entity.read(cx).sampling_task.is_none());
        });

        // The Entity deliberately has no stop/remove call here; a tab close
        // cannot change registry-owned forwarding lifetime.
        cx.read(|cx| assert!(entity.read(cx).delivery_intents.is_empty()));
    }

    #[gpui::test]
    fn hidden_mount_stops_sampling_without_blocking_reliable_delivery(cx: &mut TestAppContext) {
        let (worker_tx, worker_rx) = delivery::ActiveDeliverySender::channel();
        let (_runtime_tx, runtime_rx) = std::sync::mpsc::channel();
        let runtime_service = ForwardingRuntimeService::test_fixture();
        let entity = cx.new(|cx| {
            ForwardingWorkspaceEntity::new(
                worker_tx.clone(),
                worker_rx,
                runtime_rx,
                runtime_service,
                cx,
            )
        });
        let tab_id = TabId(8);
        let node_id = NodeId::new("visibility-forward");
        entity.update(cx, |entity, cx| {
            entity.map_tab_to_node(tab_id, node_id.clone(), cx);
            assert!(entity.set_sampling_visible(true, cx));
            entity
                .port_detection_by_node
                .entry(node_id.clone())
                .or_default()
                .last_port_scan_started = Some(Instant::now());
            entity.view.last_stats_refresh = Some(Instant::now());
        });

        let visible_generation = cx.read(|cx| entity.read(cx).sampling_generation);
        entity.update(cx, |entity, cx| {
            assert!(entity.apply_sampling_tick(visible_generation, cx));
            assert_eq!(entity.sampling_tick_count, 1);
            assert!(entity.set_sampling_visible(false, cx));
            assert!(!entity.sampling_visible);
            assert!(entity.sampling_task.is_none());
            assert!(
                entity
                    .port_detection_by_node
                    .get(&node_id)
                    .is_some_and(|state| state.last_port_scan_started.is_none())
            );
            // A queued tick from the canceled generation cannot wake sampling.
            assert!(!entity.apply_sampling_tick(visible_generation, cx));
            assert_eq!(entity.sampling_tick_count, 1);
        });

        worker_tx
            .send(ForwardingWorkerResult::ReconnectRestore {
                node_id: node_id.clone(),
                result: PhaseResult::Ok,
                restored: 1,
                detail: "restored 1 forward".to_string(),
                job_id: "hidden-job".to_string(),
                created_forwards: Vec::new(),
                bindings: Vec::new(),
            })
            .expect("hidden reconnect restore delivery");
        cx.run_until_parked();
        let intents = entity.update(cx, |entity, _cx| entity.take_delivery_intents());
        assert!(matches!(
            intents.back(),
            Some(ForwardingDeliveryIntent::ReconnectRestore {
                node_id: delivered_node_id,
                job_id,
                ..
            }) if delivered_node_id == &node_id && job_id == "hidden-job"
        ));

        // Closing the view removes only its mapping; reliable runtime delivery
        // remains owned by the Entity and is independent from tunnel lifetime.
        let removed = entity.update(cx, |entity, _cx| entity.unmap_tab(tab_id));
        assert_eq!(removed, Some(node_id.clone()));
        worker_tx
            .send(ForwardingWorkerResult::ReconnectRestore {
                node_id: node_id.clone(),
                result: PhaseResult::Ok,
                restored: 1,
                detail: "restored after close".to_string(),
                job_id: "closed-view-job".to_string(),
                created_forwards: Vec::new(),
                bindings: Vec::new(),
            })
            .expect("closed-view reconnect restore delivery");
        cx.run_until_parked();
        let intents = entity.update(cx, |entity, _cx| entity.take_delivery_intents());
        assert!(matches!(
            intents.back(),
            Some(ForwardingDeliveryIntent::ReconnectRestore {
                node_id: delivered_node_id,
                job_id,
                ..
            }) if delivered_node_id == &node_id && job_id == "closed-view-job"
        ));

        entity.update(cx, |entity, cx| {
            entity.map_tab_to_node(tab_id, node_id, cx);
            assert!(entity.set_sampling_visible(true, cx));
            assert!(entity.sampling_task.is_some());
            assert!(entity.view.last_stats_refresh.is_none());
            let resumed_generation = entity.sampling_generation;
            assert!(entity.apply_sampling_tick(resumed_generation, cx));
            assert_eq!(entity.sampling_tick_count, 2);
        });
    }

    #[gpui::test]
    fn hidden_port_scan_delivery_updates_entity_state(cx: &mut TestAppContext) {
        let (worker_tx, worker_rx) = delivery::ActiveDeliverySender::channel();
        let (_runtime_tx, runtime_rx) = std::sync::mpsc::channel();
        let runtime_service = ForwardingRuntimeService::test_fixture();
        let entity = cx.new(|cx| {
            ForwardingWorkspaceEntity::new(
                worker_tx.clone(),
                worker_rx,
                runtime_rx,
                runtime_service,
                cx,
            )
        });
        let node_id = NodeId::new("hidden-forward");
        worker_tx
            .send(ForwardingWorkerResult::PortScan {
                node_id: node_id.clone(),
                connection_id: Some("hidden-connection".to_string()),
                binding: None,
                result: Ok(PortDetectionSnapshot {
                    new_ports: Vec::new(),
                    closed_ports: Vec::new(),
                    all_ports: vec![DetectedPort {
                        port: 8080,
                        bind_addr: "127.0.0.1".to_string(),
                        process_name: None,
                        pid: None,
                    }],
                    has_scanned: true,
                }),
            })
            .unwrap();

        cx.run_until_parked();

        let state = cx
            .read(|cx| entity.read(cx).port_detection_state(&node_id))
            .unwrap();
        assert!(state.has_scanned_ports);
        assert_eq!(state.detected_ports.len(), 1);
    }

    #[gpui::test]
    fn stats_delivery_updates_cached_snapshot_without_refreshing_manager(cx: &mut TestAppContext) {
        let (worker_tx, worker_rx) = delivery::ActiveDeliverySender::channel();
        let delivery_wake = worker_tx.wake();
        let (runtime_tx, runtime_rx) = std::sync::mpsc::channel();
        let runtime_service = ForwardingRuntimeService::test_fixture();
        let entity = cx.new(|cx| {
            ForwardingWorkspaceEntity::new(worker_tx, worker_rx, runtime_rx, runtime_service, cx)
        });
        let node_id = NodeId::new("stats-forward");
        let forwarded_port = 3000;
        let mut rule = ForwardRule::local("127.0.0.1", forwarded_port, "localhost", forwarded_port);
        rule.status = ForwardStatus::Active;
        let forward_id = rule.id.clone();
        entity.update(cx, |entity, _cx| {
            entity.runtime_snapshots.insert(
                node_id.clone(),
                ForwardingRuntimeSnapshot {
                    rules: vec![rule],
                    stats_by_forward_id: HashMap::new(),
                },
            );
        });
        let stats = ForwardStats {
            connection_count: 2,
            active_connections: 1,
            bytes_sent: 128,
            bytes_received: 256,
        };

        runtime_tx
            .send(ForwardEvent::StatsUpdated {
                forward_id: forward_id.clone(),
                session_id: ForwardingRuntimeService::session_id_for_node(&node_id),
                stats: stats.clone(),
            })
            .expect("stats delivery");
        delivery_wake.mark();
        cx.run_until_parked();

        cx.read(|cx| {
            let snapshot = entity
                .read(cx)
                .runtime_snapshots
                .get(&node_id)
                .expect("cached forwarding snapshot");
            assert_eq!(snapshot.rules.len(), 1);
            assert_eq!(snapshot.stats_by_forward_id.get(&forward_id), Some(&stats));
        });
        let intents = entity.update(cx, |entity, _cx| entity.take_delivery_intents());
        assert_eq!(intents.len(), 1);
        assert!(matches!(
            intents.front(),
            Some(ForwardingDeliveryIntent::Runtime(
                ForwardEvent::StatsUpdated {
                    forward_id: delivered_forward_id,
                    ..
                }
            )) if delivered_forward_id == &forward_id
        ));
    }

    #[gpui::test]
    fn hidden_reconnect_restore_completion_uses_forwarding_delivery(cx: &mut TestAppContext) {
        let (worker_tx, worker_rx) = delivery::ActiveDeliverySender::channel();
        let (_runtime_tx, runtime_rx) = std::sync::mpsc::channel();
        let runtime_service = ForwardingRuntimeService::test_fixture();
        let entity = cx.new(|cx| {
            ForwardingWorkspaceEntity::new(
                worker_tx.clone(),
                worker_rx,
                runtime_rx,
                runtime_service,
                cx,
            )
        });
        worker_tx
            .send(ForwardingWorkerResult::ReconnectRestore {
                node_id: NodeId::new("hidden-forward"),
                result: PhaseResult::Ok,
                restored: 2,
                detail: "restored 2 forward(s)".to_string(),
                job_id: "job-a".to_string(),
                created_forwards: Vec::new(),
                bindings: Vec::new(),
            })
            .expect("reconnect restore delivery");

        cx.run_until_parked();

        let intents = entity.update(cx, |entity, _cx| entity.take_delivery_intents());
        assert!(matches!(
            intents.front(),
            Some(ForwardingDeliveryIntent::ReconnectRestore {
                node_id,
                result: PhaseResult::Ok,
                restored: 2,
                job_id,
                ..
            }) if node_id == &NodeId::new("hidden-forward") && job_id == "job-a"
        ));
    }

    #[gpui::test]
    fn entity_release_stops_only_the_delivery_waiter(cx: &mut TestAppContext) {
        let (worker_tx, worker_rx) = delivery::ActiveDeliverySender::channel();
        let delivery_wake = worker_tx.wake();
        let (_runtime_tx, runtime_rx) = std::sync::mpsc::channel();
        let runtime_service = ForwardingRuntimeService::test_fixture();
        let entity = cx.new(|cx| {
            ForwardingWorkspaceEntity::new(worker_tx, worker_rx, runtime_rx, runtime_service, cx)
        });

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        // The Entity has no registry or manager handle, so release can only
        // stop its own waiter and cannot stop a tunnel.
        assert!(delivery_wake.is_stopped());
    }
}
