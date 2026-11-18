// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::workspace::delivery;
use gpui::{AppContext, Entity, Subscription};
use std::{cell::Cell, rc::Rc};

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
        binding: Option<(String, String, ConnectionConsumer)>,
    },
    ReconnectRestore {
        node_id: NodeId,
        result: PhaseResult,
        restored: u32,
        detail: String,
        job_id: String,
        created_forwards: Vec<(Arc<ForwardingManager>, String)>,
        bindings: Vec<(String, String, ConnectionConsumer)>,
    },
    Runtime(ForwardEvent),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum ForwardingWorkspaceEvent {
    DeliveryReady,
    SamplingDue,
}

/// Delivers runtime results and associates each presentation with its native tab.
pub(in crate::workspace) struct ForwardingWorkspaceEntity {
    pages: HashMap<TabId, ForwardingPage>,
    dispatch_page: Rc<Cell<Option<TabId>>>,
    selected_page: Option<TabId>,
    tab_nodes: HashMap<TabId, NodeId>,
    sampler: Entity<ForwardsSampler>,
    _sampler_subscription: Subscription,
    worker_tx: delivery::ActiveDeliverySender<ForwardingWorkerResult>,
    worker_rx: std::sync::mpsc::Receiver<ForwardingWorkerResult>,
    runtime_event_rx: std::sync::mpsc::Receiver<ForwardEvent>,
    runtime_service: ForwardingRuntimeService,
    runtime_snapshots: HashMap<NodeId, ForwardingRuntimeSnapshot>,
    delivery_intents: VecDeque<ForwardingDeliveryIntent>,
    pub(super) port_detection_by_node: HashMap<NodeId, PortDetectionViewState>,
    port_profiler_nodes: std::collections::HashSet<NodeId>,
}

type DeferredStart = (&'static str, bool, ForwardingRuntimeOperation);

struct ForwardingPage {
    state: Entity<ForwardsPageState>,
    views: Option<ForwardsViews>,
    pending_start: Option<DeferredStart>,
    start_after_save: Option<ForwardingRuntimeOperation>,
}

/// Synchronous keyboard routing selects a tab; worker results carry their own IDs.
pub(in crate::workspace) struct ForwardingPageScope {
    slot: Rc<Cell<Option<TabId>>>,
    previous: Option<TabId>,
}
impl Drop for ForwardingPageScope {
    fn drop(&mut self) {
        self.slot.set(self.previous);
    }
}
impl ForwardingWorkspaceEntity {
    pub(in crate::workspace) fn page_id(&self) -> Option<TabId> {
        self.dispatch_page.get().or(self.selected_page)
    }
    pub(in crate::workspace) fn has_page(&self, id: TabId) -> bool {
        self.pages.contains_key(&id)
    }
    pub(in crate::workspace) fn enter_page(&self, id: Option<TabId>) -> ForwardingPageScope {
        ForwardingPageScope {
            slot: self.dispatch_page.clone(),
            previous: self.dispatch_page.replace(id),
        }
    }
    pub(in crate::workspace) fn select_page(&mut self, id: TabId, cx: &mut App) {
        if self.selected_page != Some(id) {
            if let Some(previous) = self.selected_page.and_then(|id| self.page_state(id)) {
                previous.update(cx, |state, cx| {
                    if state.clear_input_focus() {
                        cx.notify();
                    }
                });
            }
            self.selected_page = Some(id);
        }
    }

    pub(in crate::workspace) fn page_state(&self, id: TabId) -> Option<Entity<ForwardsPageState>> {
        self.pages.get(&id).map(|page| page.state.clone())
    }

    pub(in crate::workspace) fn page_for<'a>(
        &self,
        id: TabId,
        cx: &'a App,
    ) -> Option<&'a ForwardsPageState> {
        self.pages.get(&id).map(|page| page.state.read(cx))
    }

    pub(in crate::workspace) fn current_page<'a>(
        &self,
        cx: &'a App,
    ) -> Option<&'a ForwardsPageState> {
        self.page_for(self.page_id()?, cx)
    }

    pub(super) fn mount_page(&mut self, id: TabId, bindings: ForwardsViewBindings, cx: &mut App) {
        if let Some(page) = self.pages.get_mut(&id)
            && page.views.is_none()
        {
            page.views = Some(ForwardsViews::new(page.state.clone(), bindings, cx));
        }
    }

    pub(super) fn views(&self, id: TabId) -> Option<&ForwardsViews> {
        self.pages.get(&id)?.views.as_ref()
    }

    pub(super) fn set_deferred_start(&mut self, id: TabId, pending: DeferredStart) {
        if let Some(page) = self.pages.get_mut(&id) {
            page.pending_start = Some(pending);
        }
    }

    pub(super) fn take_deferred_start(&mut self, id: TabId) -> Option<DeferredStart> {
        self.pages.get_mut(&id)?.pending_start.take()
    }

    pub(super) fn set_start_after_save(
        &mut self,
        id: TabId,
        operation: Option<ForwardingRuntimeOperation>,
    ) {
        if let Some(page) = self.pages.get_mut(&id) {
            page.start_after_save = operation;
        }
    }

    pub(super) fn take_start_after_save(
        &mut self,
        id: TabId,
    ) -> Option<ForwardingRuntimeOperation> {
        self.pages.get_mut(&id)?.start_after_save.take()
    }

    #[cfg(test)]
    fn test_fixture(cx: &mut Context<Self>) -> Self {
        let (worker_tx, worker_rx) = delivery::ActiveDeliverySender::channel();
        let (_runtime_tx, runtime_event_rx) = std::sync::mpsc::channel();
        Self::new(
            worker_tx,
            worker_rx,
            runtime_event_rx,
            ForwardingRuntimeService::test_fixture(),
            cx,
        )
    }

    pub(in crate::workspace) fn new(
        worker_tx: delivery::ActiveDeliverySender<ForwardingWorkerResult>,
        worker_rx: std::sync::mpsc::Receiver<ForwardingWorkerResult>,
        runtime_event_rx: std::sync::mpsc::Receiver<ForwardEvent>,
        runtime_service: ForwardingRuntimeService,
        cx: &mut Context<Self>,
    ) -> Self {
        let sampler = cx.new(|_| ForwardsSampler::default());
        let sampler_subscription = cx.subscribe(&sampler, |_, _, _: &ForwardsSamplingDue, cx| {
            cx.emit(ForwardingWorkspaceEvent::SamplingDue);
        });
        let entity = Self {
            pages: HashMap::new(),
            dispatch_page: Rc::new(Cell::new(None)),
            selected_page: None,
            tab_nodes: HashMap::new(),
            sampler,
            _sampler_subscription: sampler_subscription,
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

    pub(in crate::workspace) fn operation_pending(&self, id: TabId, cx: &App) -> bool {
        self.page_for(id, cx)
            .is_some_and(ForwardsPageState::pending)
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

    pub(super) fn refresh_runtime_snapshot(&mut self, node_id: &NodeId, cx: &mut App) -> bool {
        let snapshot = self.runtime_service.snapshot_for_node(node_id);
        if self.runtime_snapshots.get(node_id) == Some(&snapshot) {
            return false;
        }
        self.runtime_snapshots.insert(node_id.clone(), snapshot);
        self.notify_node(node_id, cx);
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

    pub(in crate::workspace) fn map_tab_to_node(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        self.pages.entry(tab_id).or_insert_with(|| ForwardingPage {
            state: cx.new(|_| ForwardsPageState::new()),
            views: None,
            pending_start: None,
            start_after_save: None,
        });
        self.selected_page = Some(tab_id);
        self.tab_nodes.insert(tab_id, node_id.clone());
        self.refresh_runtime_snapshot(&node_id, cx);
    }

    pub(in crate::workspace) fn unmap_tab(
        &mut self,
        tab_id: TabId,
        cx: &mut App,
    ) -> Option<NodeId> {
        // Removing a view mapping must not release the registry-owned tunnel.
        let removed = self.tab_nodes.remove(&tab_id);
        self.pages.remove(&tab_id);
        if self.dispatch_page.get() == Some(tab_id) {
            self.dispatch_page.set(None);
        }
        if self.selected_page == Some(tab_id) {
            self.selected_page = None;
        }
        if let Some(node_id) = removed.as_ref()
            && !self.tab_nodes.values().any(|mapped| mapped == node_id)
        {
            self.runtime_snapshots.remove(node_id);
            // A closed page releases its sampler, not the forwarding consumer.
            if let Some(connection_id) = self.runtime_service.connection_id_for_node(node_id) {
                self.runtime_service
                    .registry()
                    .stop_port_profiler(&connection_id);
            }
        }
        if removed.is_some() && self.tab_nodes.is_empty() {
            // Cancel the page sampler without touching long-lived runtime state.
            self.set_sampling_visible(false, cx);
        }
        removed
    }

    pub(in crate::workspace) fn tab_node_mappings(&self) -> &HashMap<TabId, NodeId> {
        &self.tab_nodes
    }

    pub(in crate::workspace) fn set_sampling_visible(
        &mut self,
        visible: bool,
        cx: &mut App,
    ) -> bool {
        let visible = visible && !self.tab_nodes.is_empty();
        let changed = self
            .sampler
            .update(cx, |sampler, cx| sampler.set_visible(visible, cx));
        if changed {
            for state in self.port_detection_by_node.values_mut() {
                if !state.port_scan_pending {
                    state.last_port_scan_started = None;
                }
            }
        }
        changed
    }

    pub(in crate::workspace) fn sampling_visible(&self, cx: &App) -> bool {
        self.sampler.read(cx).visible()
    }

    pub(super) fn stats_due(&self, cx: &mut App) -> bool {
        self.sampler.update(cx, |sampler, _| sampler.stats_due())
    }

    pub(super) fn notify_node(&self, node_id: &NodeId, cx: &mut App) {
        for (tab, node) in &self.tab_nodes {
            if node == node_id
                && let Some(page) = self.pages.get(tab)
            {
                cx.notify(page.state.entity_id());
            }
        }
    }

    pub(in crate::workspace) fn take_delivery_intents(
        &mut self,
    ) -> VecDeque<ForwardingDeliveryIntent> {
        std::mem::take(&mut self.delivery_intents)
    }

    pub(super) fn port_detection_state(&self, node_id: &NodeId) -> Option<PortDetectionViewState> {
        self.port_detection_by_node.get(node_id).cloned()
    }

    pub(in crate::workspace) fn track_port_profiler(&mut self, node_id: NodeId) {
        self.port_profiler_nodes.insert(node_id);
    }

    pub(in crate::workspace) fn untrack_port_profiler(&mut self, node_id: &NodeId, cx: &mut App) {
        self.port_profiler_nodes.remove(node_id);
        self.port_detection_by_node.remove(node_id);
        self.notify_node(node_id, cx);
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

    pub(in crate::workspace) fn dismiss_detected_port(
        &mut self,
        node_id: &NodeId,
        port: u16,
        cx: &mut App,
    ) {
        self.runtime_service
            .ignore_detected_port_for_node(node_id, port);
        if let Some(state) = self.port_detection_by_node.get_mut(node_id) {
            state.new_ports.retain(|detected| detected.port != port);
        }
        self.notify_node(node_id, cx);
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
                        self.refresh_runtime_snapshot(&node_id, cx);
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
                    self.apply_port_detection_result(&node_id, connection_id, result, cx);
                    self.refresh_runtime_snapshot(&node_id, cx);
                    self.delivery_intents
                        .push_back(ForwardingDeliveryIntent::PortScan { binding });
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
                        self.notify_node(&node_id, cx);
                    }
                }
                ForwardEvent::StatusChanged { session_id, .. }
                | ForwardEvent::SessionSuspended { session_id, .. } => {
                    if let Some(node_id) = ForwardingRuntimeService::node_id_for_session(session_id)
                    {
                        self.refresh_runtime_snapshot(&node_id, cx);
                    }
                }
                ForwardEvent::PortDetected { .. } => {}
            }
            self.delivery_intents
                .push_back(ForwardingDeliveryIntent::Runtime(event));
        }
        if !self.delivery_intents.is_empty() {
            cx.emit(ForwardingWorkspaceEvent::DeliveryReady);
        }
        worker_drain.outcome.backlog_remaining || event_drain.outcome.backlog_remaining
    }

    pub(in crate::workspace) fn apply_port_detection_result(
        &mut self,
        node_id: &NodeId,
        connection_id: Option<String>,
        result: Result<PortDetectionSnapshot, String>,
        cx: &mut App,
    ) {
        self.port_detection_by_node
            .entry(node_id.clone())
            .or_default()
            .apply(connection_id, result);
        self.notify_node(node_id, cx);
    }
}

impl gpui::EventEmitter<ForwardingWorkspaceEvent> for ForwardingWorkspaceEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};

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
            assert!(entity.stats_due(cx));
            assert!(!entity.stats_due(cx));
        });

        entity.update(cx, |entity, cx| {
            assert!(entity.set_sampling_visible(false, cx));
            assert!(!entity.sampling_visible(cx));
            assert!(
                entity
                    .port_detection_by_node
                    .get(&node_id)
                    .is_some_and(|state| state.last_port_scan_started.is_none())
            );
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
        let removed = entity.update(cx, |entity, cx| entity.unmap_tab(tab_id, cx));
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
            assert!(entity.sampling_visible(cx));
            assert!(entity.stats_due(cx));
        });
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
                    ..Default::default()
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
