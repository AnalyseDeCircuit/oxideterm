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
    Runtime(ForwardEvent),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum ForwardingWorkspaceEvent {
    DeliveryReady,
}

/// Owns forwarding UI delivery and sampling state without owning tunnel lifetime.
pub(in crate::workspace) struct ForwardingWorkspaceEntity {
    pub(super) view: ForwardsViewState,
    pub(super) section_list_state: ListState,
    pub(super) section_list_cache: RefCell<VirtualListSignatureCache>,
    pub(super) table_row_list_state: ListState,
    pub(super) table_row_list_cache: RefCell<VirtualListSignatureCache>,
    worker_tx: delivery::ActiveDeliverySender<ForwardingWorkerResult>,
    worker_rx: std::sync::mpsc::Receiver<ForwardingWorkerResult>,
    runtime_event_rx: std::sync::mpsc::Receiver<ForwardEvent>,
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
            section_list_state: ListState::new(0, ListAlignment::Top, px(0.0)),
            section_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            table_row_list_state: ListState::new(0, ListAlignment::Top, px(0.0)),
            table_row_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            worker_tx,
            worker_rx,
            runtime_event_rx,
            delivery_intents: VecDeque::new(),
            port_detection_by_node: HashMap::new(),
            port_profiler_nodes: std::collections::HashSet::new(),
        }
    }

    pub(in crate::workspace) fn new(
        worker_tx: delivery::ActiveDeliverySender<ForwardingWorkerResult>,
        worker_rx: std::sync::mpsc::Receiver<ForwardingWorkerResult>,
        runtime_event_rx: std::sync::mpsc::Receiver<ForwardEvent>,
        cx: &mut Context<Self>,
    ) -> Self {
        let entity = Self {
            view: ForwardsViewState::default(),
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
            delivery_intents: VecDeque::new(),
            port_detection_by_node: HashMap::new(),
            port_profiler_nodes: std::collections::HashSet::new(),
        };
        entity.schedule_delivery(cx);
        entity
    }

    pub(in crate::workspace) fn worker_sender(
        &self,
    ) -> delivery::ActiveDeliverySender<ForwardingWorkerResult> {
        self.worker_tx.clone()
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
                } => self
                    .delivery_intents
                    .push_back(ForwardingDeliveryIntent::Operation {
                        tab_id,
                        message_key,
                        sync_saved_forwards_on_success,
                        binding,
                        result,
                    }),
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
                    self.delivery_intents
                        .push_back(ForwardingDeliveryIntent::PortScan { node_id, binding });
                }
            }
        }

        let event_drain =
            delivery::drain_channel(&self.runtime_event_rx, delivery::LIFECYCLE_DELIVERY_BUDGET);
        self.delivery_intents.extend(
            event_drain
                .items
                .into_iter()
                .map(ForwardingDeliveryIntent::Runtime),
        );
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
    fn hidden_port_scan_delivery_updates_entity_state(cx: &mut TestAppContext) {
        let (worker_tx, worker_rx) = delivery::ActiveDeliverySender::channel();
        let (_runtime_tx, runtime_rx) = std::sync::mpsc::channel();
        let entity = cx
            .new(|cx| ForwardingWorkspaceEntity::new(worker_tx.clone(), worker_rx, runtime_rx, cx));
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
    fn entity_release_stops_only_the_delivery_waiter(cx: &mut TestAppContext) {
        let (worker_tx, worker_rx) = delivery::ActiveDeliverySender::channel();
        let delivery_wake = worker_tx.wake();
        let (_runtime_tx, runtime_rx) = std::sync::mpsc::channel();
        let entity =
            cx.new(|cx| ForwardingWorkspaceEntity::new(worker_tx, worker_rx, runtime_rx, cx));

        drop(entity);
        cx.update(|_cx| {});
        cx.run_until_parked();

        // The Entity has no registry or manager handle, so release can only
        // stop its own waiter and cannot stop a tunnel.
        assert!(delivery_wake.is_stopped());
    }
}
