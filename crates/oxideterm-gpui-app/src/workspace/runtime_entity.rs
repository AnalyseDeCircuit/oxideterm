// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_ssh::ReconnectTiming;

const ACTIVE_PROBE_START_DELAY: Duration = Duration::from_millis(530);
const RECONNECT_DEBOUNCE_DELAY: Duration = Duration::from_millis(500);
const RECONNECT_MAX_REQUEUE: u32 = 120;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum WorkspaceRuntimeEvent {
    WorkerResultsReady,
    NodeEventsReady,
    ReconnectRootsReady,
    ReconnectScheduleReady,
    ActiveConnectionsChanged,
}

#[derive(Debug)]
pub(in crate::workspace) enum ReconnectScheduleAction {
    ContinueConnectionChain {
        node_id: NodeId,
    },
    ContinueReconnectCascade,
    StartReconnectPipeline {
        node_id: NodeId,
        expected_connection_id: Option<String>,
    },
    RetryNodeConnect {
        node_id: NodeId,
        job_id: String,
    },
    CleanupReconnectJob {
        node_id: NodeId,
        started_at: SystemTime,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum ReconnectPipelineClaim {
    Acquired,
    Requeued,
    Exhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct ReconnectTransferResumeCompletion {
    pub(in crate::workspace) node_id: NodeId,
    pub(in crate::workspace) resumed: u32,
}

#[derive(Clone, Copy, Debug)]
struct ReconnectRequeueState {
    attempt: u32,
    generation: u64,
}

/// Owns runtime worker endpoints and reliable delivery independently from tabs.
pub(in crate::workspace) struct WorkspaceRuntimeEntity {
    ssh_worker_tx: delivery::ActiveDeliverySender<SshConnectionWorkerResult>,
    ssh_worker_rx: std::sync::mpsc::Receiver<SshConnectionWorkerResult>,
    reconnect_worker_tx: delivery::ActiveDeliverySender<ReconnectWorkerResult>,
    reconnect_worker_rx: std::sync::mpsc::Receiver<ReconnectWorkerResult>,
    active_probe_tx: delivery::ActiveDeliverySender<usize>,
    active_probe_rx: std::sync::mpsc::Receiver<usize>,
    _node_event_subscription: NodeEventSubscription,
    node_event_rx: NodeEventReceiver,
    ssh_results: VecDeque<SshConnectionWorkerResult>,
    reconnect_results: VecDeque<ReconnectWorkerResult>,
    node_events: VecDeque<NodeStateEvent>,
    node_event_generations: HashMap<NodeId, u64>,
    node_runtime_store: NodeRuntimeStore,
    reconnect_enabled: bool,
    pending_reconnect_node_ids: HashSet<NodeId>,
    reconnect_debounce_generation: u64,
    ready_reconnect_roots: VecDeque<NodeId>,
    reconnect_pipeline_active_node: Option<NodeId>,
    reconnect_requeue_states: HashMap<NodeId, ReconnectRequeueState>,
    pending_reconnect_cascade_nodes: VecDeque<NodeId>,
    reconnect_cascade_generation: u64,
    reconnect_schedule_actions: VecDeque<ReconnectScheduleAction>,
    // Restore bookkeeping survives page changes and is cancelled only by node lifecycle actions.
    pending_reconnect_transfer_resumes: HashMap<NodeId, HashSet<String>>,
    reconnect_transfer_resume_successes: HashMap<NodeId, usize>,
    pending_ide_restore_transfer_counts: HashMap<NodeId, u32>,
    reconnect_forward_restore_totals: HashMap<NodeId, u32>,
    reconnect_forward_restore_tokens: HashMap<NodeId, Arc<AtomicBool>>,
    reconnect_orchestrator: ReconnectOrchestratorStore,
    ssh_registry: SshConnectionRegistry,
    task_runtime: Arc<tokio::runtime::Runtime>,
    reconnect_timing: ReconnectTiming,
    ssh_active_probe_in_flight: bool,
    active_probe_timer_generation: u64,
}

impl WorkspaceRuntimeEntity {
    pub(in crate::workspace) fn new(
        ssh_registry: SshConnectionRegistry,
        node_runtime_store: NodeRuntimeStore,
        node_event_emitter: NodeEventEmitter,
        task_runtime: Arc<tokio::runtime::Runtime>,
        reconnect_enabled: bool,
        reconnect_timing: ReconnectTiming,
        reconnect_max_attempts: u32,
        cx: &mut Context<Self>,
    ) -> Self {
        let runtime_wake = delivery::ActiveDeliveryWake::default();
        let (ssh_worker_tx, ssh_worker_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_wake.clone());
        let (reconnect_worker_tx, reconnect_worker_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_wake.clone());
        let (active_probe_tx, active_probe_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_wake.clone());
        // Node state is a latest-value stream. Its bounded mailbox may retain
        // reliable lifecycle events beyond capacity, while the shared wake
        // lets this Entity drain every runtime source without a root waiter.
        let emitter_wake = runtime_wake.clone();
        let (node_event_subscription, node_event_rx) = node_event_emitter
            .subscribe_bounded_with_wake(256, Some(Arc::new(move || emitter_wake.mark())));
        let mut entity = Self {
            ssh_worker_tx,
            ssh_worker_rx,
            reconnect_worker_tx,
            reconnect_worker_rx,
            active_probe_tx,
            active_probe_rx,
            _node_event_subscription: node_event_subscription,
            node_event_rx,
            ssh_results: VecDeque::new(),
            reconnect_results: VecDeque::new(),
            node_events: VecDeque::new(),
            node_event_generations: HashMap::new(),
            node_runtime_store,
            reconnect_enabled,
            pending_reconnect_node_ids: HashSet::new(),
            reconnect_debounce_generation: 0,
            ready_reconnect_roots: VecDeque::new(),
            reconnect_pipeline_active_node: None,
            reconnect_requeue_states: HashMap::new(),
            pending_reconnect_cascade_nodes: VecDeque::new(),
            reconnect_cascade_generation: 0,
            reconnect_schedule_actions: VecDeque::new(),
            pending_reconnect_transfer_resumes: HashMap::new(),
            reconnect_transfer_resume_successes: HashMap::new(),
            pending_ide_restore_transfer_counts: HashMap::new(),
            reconnect_forward_restore_totals: HashMap::new(),
            reconnect_forward_restore_tokens: HashMap::new(),
            reconnect_orchestrator: ReconnectOrchestratorStore::new(
                reconnect_timing,
                reconnect_max_attempts,
            ),
            ssh_registry,
            task_runtime,
            reconnect_timing,
            ssh_active_probe_in_flight: false,
            active_probe_timer_generation: 0,
        };
        entity.schedule_worker_delivery(cx);
        entity.schedule_active_probe_after(ACTIVE_PROBE_START_DELAY, cx);
        entity
    }

    pub(in crate::workspace) fn configure_reconnect(
        &mut self,
        reconnect_enabled: bool,
        reconnect_timing: ReconnectTiming,
        reconnect_max_attempts: u32,
        cx: &mut Context<Self>,
    ) {
        self.reconnect_enabled = reconnect_enabled;
        if !reconnect_enabled {
            self.pending_reconnect_node_ids.clear();
            // Invalidate timers scheduled under the previous settings.
            self.reconnect_debounce_generation = self.reconnect_debounce_generation.wrapping_add(1);
        }
        self.reconnect_orchestrator
            .configure(reconnect_timing, reconnect_max_attempts);
        self.reconnect_timing = reconnect_timing;
        self.schedule_active_probe_after(ACTIVE_PROBE_START_DELAY, cx);
    }

    pub(in crate::workspace) fn reconnect_orchestrator(&self) -> &ReconnectOrchestratorStore {
        // The domain store owns job transitions; this Entity owns its workspace lifetime.
        &self.reconnect_orchestrator
    }

    pub(in crate::workspace) fn has_active_reconnect_job(&self, node_id: &NodeId) -> bool {
        self.reconnect_orchestrator.is_active(&node_id.0)
    }

    pub(in crate::workspace) fn reconnect_job_is_current(
        &self,
        node_id: &NodeId,
        job_id: &str,
    ) -> bool {
        self.reconnect_orchestrator.is_current(&node_id.0, job_id)
    }

    pub(in crate::workspace) fn active_reconnect_node_ids(&self) -> Vec<NodeId> {
        self.reconnect_orchestrator
            .active_node_ids()
            .into_iter()
            .map(NodeId::new)
            .collect()
    }

    pub(in crate::workspace) fn ssh_worker_sender(
        &self,
    ) -> delivery::ActiveDeliverySender<SshConnectionWorkerResult> {
        // Worker tasks need only a shallow channel endpoint, never runtime state.
        self.ssh_worker_tx.clone()
    }

    pub(in crate::workspace) fn reconnect_worker_sender(
        &self,
    ) -> delivery::ActiveDeliverySender<ReconnectWorkerResult> {
        // Delayed reconnect phases keep the Entity wake path without a root sender field.
        self.reconnect_worker_tx.clone()
    }

    pub(in crate::workspace) fn take_worker_results(
        &mut self,
    ) -> (
        VecDeque<SshConnectionWorkerResult>,
        VecDeque<ReconnectWorkerResult>,
    ) {
        (
            std::mem::take(&mut self.ssh_results),
            std::mem::take(&mut self.reconnect_results),
        )
    }

    pub(in crate::workspace) fn take_node_events(&mut self) -> VecDeque<NodeStateEvent> {
        std::mem::take(&mut self.node_events)
    }

    pub(in crate::workspace) fn queue_reconnect_root(
        &mut self,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        if !self.reconnect_enabled {
            return;
        }
        self.pending_reconnect_node_ids.insert(node_id);
        self.reconnect_debounce_generation = self.reconnect_debounce_generation.wrapping_add(1);
        let generation = self.reconnect_debounce_generation;
        cx.spawn(async move |entity, cx| {
            Timer::after(RECONNECT_DEBOUNCE_DELAY).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.flush_reconnect_roots(generation, cx);
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn take_reconnect_roots(&mut self) -> VecDeque<NodeId> {
        std::mem::take(&mut self.ready_reconnect_roots)
    }

    pub(in crate::workspace) fn cancel_queued_reconnects(&mut self, node_ids: &[NodeId]) {
        self.pending_reconnect_node_ids
            .retain(|node_id| !node_ids.contains(node_id));
        self.ready_reconnect_roots
            .retain(|node_id| !node_ids.contains(node_id));
        self.cancel_reconnect_scheduler_nodes(node_ids);
        for node_id in node_ids {
            self.clear_reconnect_restore_state(node_id);
        }
    }

    pub(in crate::workspace) fn claim_reconnect_pipeline(
        &mut self,
        node_id: &NodeId,
        expected_connection_id: Option<String>,
        retry_delay: Duration,
        cx: &mut Context<Self>,
    ) -> ReconnectPipelineClaim {
        if self
            .reconnect_pipeline_active_node
            .as_ref()
            .is_some_and(|active_node_id| active_node_id != node_id)
        {
            let requeue_state = self
                .reconnect_requeue_states
                .entry(node_id.clone())
                .and_modify(|state| {
                    state.attempt = state.attempt.saturating_add(1);
                    state.generation = state.generation.wrapping_add(1);
                })
                .or_insert(ReconnectRequeueState {
                    attempt: 1,
                    generation: 1,
                });
            if requeue_state.attempt > RECONNECT_MAX_REQUEUE {
                self.reconnect_requeue_states.remove(node_id);
                return ReconnectPipelineClaim::Exhausted;
            }
            let generation = requeue_state.generation;
            let retry_node_id = node_id.clone();
            cx.spawn(async move |entity, cx| {
                Timer::after(retry_delay).await;
                let _ = entity.update(cx, |entity, cx| {
                    let retry_is_current = entity
                        .reconnect_requeue_states
                        .get(&retry_node_id)
                        .is_some_and(|state| state.generation == generation);
                    if retry_is_current {
                        entity.push_reconnect_schedule_action(
                            ReconnectScheduleAction::StartReconnectPipeline {
                                node_id: retry_node_id,
                                expected_connection_id,
                            },
                            cx,
                        );
                    }
                });
            })
            .detach();
            return ReconnectPipelineClaim::Requeued;
        }

        self.reconnect_pipeline_active_node = Some(node_id.clone());
        self.reconnect_requeue_states.remove(node_id);
        ReconnectPipelineClaim::Acquired
    }

    pub(in crate::workspace) fn release_reconnect_pipeline(&mut self, node_id: &NodeId) {
        if self
            .reconnect_pipeline_active_node
            .as_ref()
            .is_some_and(|active_node_id| active_node_id == node_id)
        {
            self.reconnect_pipeline_active_node = None;
        }
        self.reconnect_requeue_states.remove(node_id);
    }

    pub(in crate::workspace) fn cancel_reconnect_retry(&mut self, node_id: &NodeId) {
        self.reconnect_requeue_states.remove(node_id);
    }

    pub(in crate::workspace) fn replace_reconnect_cascade(
        &mut self,
        node_ids: impl IntoIterator<Item = NodeId>,
    ) {
        self.pending_reconnect_cascade_nodes = node_ids.into_iter().collect();
        self.reconnect_cascade_generation = self.reconnect_cascade_generation.wrapping_add(1);
    }

    pub(in crate::workspace) fn clear_reconnect_cascade(&mut self) {
        self.pending_reconnect_cascade_nodes.clear();
        // Invalidate a delayed continuation when its owning cascade is cleared.
        self.reconnect_cascade_generation = self.reconnect_cascade_generation.wrapping_add(1);
    }

    pub(in crate::workspace) fn schedule_next_reconnect_cascade(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        if self.pending_reconnect_cascade_nodes.is_empty() {
            return;
        }
        self.reconnect_cascade_generation = self.reconnect_cascade_generation.wrapping_add(1);
        let generation = self.reconnect_cascade_generation;
        cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                if entity.reconnect_cascade_generation == generation
                    && !entity.pending_reconnect_cascade_nodes.is_empty()
                {
                    entity.push_reconnect_schedule_action(
                        ReconnectScheduleAction::ContinueReconnectCascade,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn take_next_reconnect_cascade_node(&mut self) -> Option<NodeId> {
        self.pending_reconnect_cascade_nodes.pop_front()
    }

    pub(in crate::workspace) fn schedule_reconnect_action(
        &self,
        action: ReconnectScheduleAction,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.push_reconnect_schedule_action(action, cx);
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn take_reconnect_schedule_actions(
        &mut self,
    ) -> VecDeque<ReconnectScheduleAction> {
        std::mem::take(&mut self.reconnect_schedule_actions)
    }

    pub(in crate::workspace) fn begin_reconnect_transfer_resumes(
        &mut self,
        reconnect_node_id: &NodeId,
        candidates: impl IntoIterator<Item = (NodeId, String)>,
    ) -> Vec<(NodeId, String)> {
        let mut candidates = candidates.into_iter().peekable();
        if candidates.peek().is_none() {
            return Vec::new();
        }
        // Register each transfer before dispatch so synchronous completions cannot outrun state.
        let pending_transfer_ids = self
            .pending_reconnect_transfer_resumes
            .entry(reconnect_node_id.clone())
            .or_default();
        let requests = candidates
            .filter(|(_, transfer_id)| pending_transfer_ids.insert(transfer_id.clone()))
            .collect::<Vec<_>>();
        if requests.is_empty() {
            return requests;
        }
        self.reconnect_transfer_resume_successes
            .insert(reconnect_node_id.clone(), 0);
        requests
    }

    pub(in crate::workspace) fn finish_reconnect_transfer_resume(
        &mut self,
        transfer_id: &str,
        success: bool,
    ) -> Vec<ReconnectTransferResumeCompletion> {
        let reconnect_node_ids = self
            .pending_reconnect_transfer_resumes
            .iter()
            .filter_map(|(node_id, pending)| {
                pending.contains(transfer_id).then_some(node_id.clone())
            })
            .collect::<Vec<_>>();
        let mut completions = Vec::new();
        for reconnect_node_id in reconnect_node_ids {
            let Some(pending_transfer_ids) = self
                .pending_reconnect_transfer_resumes
                .get_mut(&reconnect_node_id)
            else {
                continue;
            };
            if success {
                *self
                    .reconnect_transfer_resume_successes
                    .entry(reconnect_node_id.clone())
                    .or_default() += 1;
            }
            pending_transfer_ids.remove(transfer_id);
            if !pending_transfer_ids.is_empty() {
                continue;
            }
            self.pending_reconnect_transfer_resumes
                .remove(&reconnect_node_id);
            let resumed = self
                .reconnect_transfer_resume_successes
                .remove(&reconnect_node_id)
                .unwrap_or_default() as u32;
            completions.push(ReconnectTransferResumeCompletion {
                node_id: reconnect_node_id,
                resumed,
            });
        }
        completions
    }

    pub(in crate::workspace) fn remember_ide_restore_transfer_count(
        &mut self,
        node_id: NodeId,
        restored_transfers: u32,
    ) {
        self.pending_ide_restore_transfer_counts
            .insert(node_id, restored_transfers);
    }

    pub(in crate::workspace) fn clear_ide_restore_transfer_count(&mut self, node_id: &NodeId) {
        self.pending_ide_restore_transfer_counts.remove(node_id);
    }

    pub(in crate::workspace) fn complete_reconnect_restore_counts(
        &mut self,
        node_id: &NodeId,
    ) -> (u32, u32) {
        let restored_forwards = self
            .reconnect_forward_restore_totals
            .remove(node_id)
            .unwrap_or_default();
        let restored_transfers = self
            .pending_ide_restore_transfer_counts
            .remove(node_id)
            .unwrap_or_default();
        (restored_forwards, restored_transfers)
    }

    pub(in crate::workspace) fn begin_forward_restore(
        &mut self,
        node_id: &NodeId,
    ) -> Arc<AtomicBool> {
        self.cancel_forward_restore(node_id);
        // The worker receives the only shallow clone; replacement or node cancellation flips it.
        let cancellation = Arc::new(AtomicBool::new(true));
        self.reconnect_forward_restore_tokens
            .insert(node_id.clone(), cancellation.clone());
        cancellation
    }

    pub(in crate::workspace) fn complete_forward_restore(
        &mut self,
        node_id: &NodeId,
        restored_forwards: u32,
    ) {
        self.reconnect_forward_restore_tokens.remove(node_id);
        self.reconnect_forward_restore_totals
            .insert(node_id.clone(), restored_forwards);
    }

    pub(in crate::workspace) fn cancel_forward_restore(&mut self, node_id: &NodeId) {
        if let Some(cancellation) = self.reconnect_forward_restore_tokens.remove(node_id) {
            cancellation.store(false, Ordering::Release);
        }
    }

    pub(in crate::workspace) fn clear_reconnect_restore_state(&mut self, node_id: &NodeId) {
        self.pending_reconnect_transfer_resumes.remove(node_id);
        self.reconnect_transfer_resume_successes.remove(node_id);
        self.pending_ide_restore_transfer_counts.remove(node_id);
        self.reconnect_forward_restore_totals.remove(node_id);
        self.cancel_forward_restore(node_id);
    }

    fn cancel_reconnect_scheduler_nodes(&mut self, node_ids: &[NodeId]) {
        self.reconnect_requeue_states
            .retain(|node_id, _| !node_ids.contains(node_id));
        self.pending_reconnect_cascade_nodes
            .retain(|node_id| !node_ids.contains(node_id));
        self.reconnect_schedule_actions
            .retain(|action| !reconnect_schedule_action_targets_any(action, node_ids));
        if self
            .reconnect_pipeline_active_node
            .as_ref()
            .is_some_and(|active_node_id| node_ids.contains(active_node_id))
        {
            self.reconnect_pipeline_active_node = None;
        }
        self.reconnect_cascade_generation = self.reconnect_cascade_generation.wrapping_add(1);
    }

    fn push_reconnect_schedule_action(
        &mut self,
        action: ReconnectScheduleAction,
        cx: &mut Context<Self>,
    ) {
        self.reconnect_schedule_actions.push_back(action);
        cx.emit(WorkspaceRuntimeEvent::ReconnectScheduleReady);
    }

    fn schedule_worker_delivery(&self, cx: &mut Context<Self>) {
        let runtime_wake = self.ssh_worker_tx.wake();
        let release_wake = runtime_wake.clone();
        cx.on_release(move |_, _| {
            // Workspace release stops UI delivery, not in-flight backend work.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                runtime_wake.wait().await;
                let should_drain = runtime_wake.take();
                let stopped = runtime_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |entity, cx| entity.drain_worker_results(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        runtime_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn schedule_active_probe_after(&mut self, delay: Duration, cx: &mut Context<Self>) {
        self.active_probe_timer_generation = self.active_probe_timer_generation.wrapping_add(1);
        let generation = self.active_probe_timer_generation;
        cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                if entity.active_probe_timer_generation == generation {
                    entity.start_active_ssh_probe(cx);
                }
            });
        })
        .detach();
    }

    fn start_active_ssh_probe(&mut self, cx: &mut Context<Self>) {
        if self.ssh_active_probe_in_flight {
            // A settings change can reschedule while a previous probe is still running.
            self.schedule_active_probe_after(ACTIVE_PROBE_START_DELAY, cx);
            return;
        }
        let registry_stats = self.ssh_registry.stats();
        if registry_stats.active == 0 && registry_stats.idle == 0 {
            self.schedule_active_probe_after(self.reconnect_timing.ssh_keepalive_interval, cx);
            return;
        }

        self.ssh_active_probe_in_flight = true;
        let ssh_registry = self.ssh_registry.clone();
        let timeout = self.reconnect_timing.proactive_keepalive_timeout;
        let result_sender = self.active_probe_tx.clone();
        self.task_runtime.spawn(async move {
            let changed = ssh_registry.probe_active_connections(timeout).await.len();
            let _ = result_sender.send(changed);
        });
    }

    fn drain_worker_results(&mut self, cx: &mut Context<Self>) -> bool {
        let ssh_batch =
            delivery::drain_channel(&self.ssh_worker_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        let reconnect_batch = delivery::drain_channel(
            &self.reconnect_worker_rx,
            delivery::LIFECYCLE_DELIVERY_BUDGET,
        );
        let active_probe_batch =
            delivery::drain_channel(&self.active_probe_rx, delivery::LIFECYCLE_DELIVERY_BUDGET);
        let (node_event_items, node_event_backlog_remaining) =
            drain_node_event_mailbox(&self.node_event_rx);
        let received_ssh_results = !ssh_batch.items.is_empty();
        self.ssh_results.extend(ssh_batch.items);
        let received_reconnect_results = !reconnect_batch.items.is_empty();
        self.reconnect_results.extend(reconnect_batch.items);
        let mut received_node_events = false;
        for event in node_event_items {
            if self.accept_node_event(&event) {
                self.node_events.push_back(event);
                received_node_events = true;
            }
        }
        let active_probe_completed = !active_probe_batch.items.is_empty();
        let active_connections_changed = active_probe_batch
            .items
            .into_iter()
            .any(|changed| changed > 0);
        if active_probe_completed {
            self.ssh_active_probe_in_flight = false;
            self.schedule_active_probe_after(self.reconnect_timing.ssh_keepalive_interval, cx);
        }
        if received_ssh_results || received_reconnect_results {
            cx.emit(WorkspaceRuntimeEvent::WorkerResultsReady);
        }
        if received_node_events {
            cx.emit(WorkspaceRuntimeEvent::NodeEventsReady);
        }
        if active_connections_changed {
            cx.emit(WorkspaceRuntimeEvent::ActiveConnectionsChanged);
        }
        ssh_batch.outcome.backlog_remaining
            || reconnect_batch.outcome.backlog_remaining
            || active_probe_batch.outcome.backlog_remaining
            || node_event_backlog_remaining
    }

    fn accept_node_event(&mut self, event: &NodeStateEvent) -> bool {
        let Some((node_id, generation)) = node_event_generation(event) else {
            return true;
        };
        if self
            .node_event_generations
            .get(&node_id)
            .is_some_and(|seen| generation <= *seen)
        {
            return false;
        }
        self.node_event_generations.insert(node_id, generation);
        true
    }

    fn flush_reconnect_roots(&mut self, generation: u64, cx: &mut Context<Self>) {
        if generation != self.reconnect_debounce_generation {
            return;
        }
        if !self.reconnect_enabled {
            self.pending_reconnect_node_ids.clear();
            return;
        }
        let pending = self.pending_reconnect_node_ids.drain().collect::<Vec<_>>();
        let roots = self.node_runtime_store.minimal_subtree_roots(pending);
        if roots.is_empty() {
            return;
        }
        self.ready_reconnect_roots.extend(roots);
        cx.emit(WorkspaceRuntimeEvent::ReconnectRootsReady);
    }
}

fn reconnect_schedule_action_targets_any(
    action: &ReconnectScheduleAction,
    node_ids: &[NodeId],
) -> bool {
    match action {
        ReconnectScheduleAction::ContinueReconnectCascade => false,
        ReconnectScheduleAction::ContinueConnectionChain { node_id }
        | ReconnectScheduleAction::StartReconnectPipeline { node_id, .. }
        | ReconnectScheduleAction::RetryNodeConnect { node_id, .. }
        | ReconnectScheduleAction::CleanupReconnectJob { node_id, .. } => {
            node_ids.contains(node_id)
        }
    }
}

fn drain_node_event_mailbox(receiver: &NodeEventReceiver) -> (Vec<NodeStateEvent>, bool) {
    let started_at = Instant::now();
    let mut events = Vec::new();
    while delivery::LIFECYCLE_DELIVERY_BUDGET.allows_next(events.len(), started_at.elapsed()) {
        match receiver.try_recv() {
            Ok(event) => events.push(event),
            Err(
                std::sync::mpsc::TryRecvError::Empty | std::sync::mpsc::TryRecvError::Disconnected,
            ) => return (events, false),
        }
    }
    // Re-marking once when the budget ends on an empty mailbox is harmless and
    // avoids consuming an extra lifecycle event merely to detect backlog.
    (events, true)
}

fn node_event_generation(event: &NodeStateEvent) -> Option<(NodeId, u64)> {
    match event {
        NodeStateEvent::ConnectionStatusChanged { .. } => None,
        NodeStateEvent::ConnectionStateChanged {
            node_id,
            generation,
            ..
        }
        | NodeStateEvent::SftpReady {
            node_id,
            generation,
            ..
        }
        | NodeStateEvent::TerminalEndpointChanged {
            node_id,
            generation,
            ..
        } => Some((NodeId::new(node_id.clone()), *generation)),
    }
}

impl gpui::EventEmitter<WorkspaceRuntimeEvent> for WorkspaceRuntimeEntity {}

impl WorkspaceApp {
    pub(in crate::workspace) fn ssh_worker_sender(
        &self,
        cx: &App,
    ) -> delivery::ActiveDeliverySender<SshConnectionWorkerResult> {
        self.workspace_runtime.read(cx).ssh_worker_sender()
    }

    pub(in crate::workspace) fn reconnect_worker_sender(
        &self,
        cx: &App,
    ) -> delivery::ActiveDeliverySender<ReconnectWorkerResult> {
        self.workspace_runtime.read(cx).reconnect_worker_sender()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn test_task_runtime() -> Arc<tokio::runtime::Runtime> {
        Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime"),
        )
    }

    fn test_runtime_entity(cx: &mut TestAppContext) -> Entity<WorkspaceRuntimeEntity> {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let task_runtime = test_task_runtime();
        cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry,
                NodeRuntimeStore::default(),
                NodeEventEmitter::default(),
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        })
    }

    #[gpui::test]
    fn worker_results_and_release_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let (ssh_tx, reconnect_tx, wake) = cx.read(|cx| {
            let entity = entity.read(cx);
            (
                entity.ssh_worker_sender(),
                entity.reconnect_worker_sender(),
                entity.ssh_worker_tx.wake(),
            )
        });
        ssh_tx
            .send(SshConnectionWorkerResult::Test { result: Ok(()) })
            .expect("SSH worker delivery");
        reconnect_tx
            .send(ReconnectWorkerResult::GraceExpired {
                node_id: NodeId::new("node-a"),
                connection_id: "connection-a".to_string(),
                detail: "test timeout".to_string(),
                job_id: "job-a".to_string(),
            })
            .expect("reconnect worker delivery");

        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            let (ssh_results, reconnect_results) = entity.take_worker_results();
            assert!(matches!(
                ssh_results.front(),
                Some(SshConnectionWorkerResult::Test { result: Ok(()) })
            ));
            assert!(matches!(
                reconnect_results.front(),
                Some(ReconnectWorkerResult::GraceExpired { .. })
            ));
        });

        drop(entity);
        cx.update(|_cx| {});
        assert!(wake.is_stopped());
    }

    #[gpui::test]
    fn active_probe_completion_stays_inside_runtime_entity(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let changed_event_seen = Arc::new(AtomicBool::new(false));
        let changed_event_flag = changed_event_seen.clone();
        let _event_subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &WorkspaceRuntimeEvent, _cx| {
                if *event == WorkspaceRuntimeEvent::ActiveConnectionsChanged {
                    changed_event_flag.store(true, Ordering::Release);
                }
            })
        });
        let active_probe_sender = entity.update(cx, |entity, _cx| {
            entity.ssh_active_probe_in_flight = true;
            entity.active_probe_tx.clone()
        });
        active_probe_sender
            .send(0)
            .expect("active probe completion");

        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            let (_ssh_results, reconnect_results) = entity.take_worker_results();
            assert!(reconnect_results.is_empty());
            assert!(!entity.ssh_active_probe_in_flight);
        });
        assert!(!changed_event_seen.load(Ordering::Acquire));

        entity.update(cx, |entity, _cx| {
            entity.ssh_active_probe_in_flight = true;
        });
        active_probe_sender.send(1).expect("changed active probe");
        cx.run_until_parked();
        assert!(changed_event_seen.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn empty_registry_and_timing_changes_reschedule_without_probe(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let initial_generation = cx.read(|cx| entity.read(cx).active_probe_timer_generation);

        entity.update(cx, |entity, cx| {
            entity.start_active_ssh_probe(cx);
            assert!(!entity.ssh_active_probe_in_flight);
            assert!(entity.active_probe_timer_generation > initial_generation);

            let mut reconnect_timing = ReconnectTiming::default();
            reconnect_timing.ssh_keepalive_interval = Duration::from_secs(37);
            entity.configure_reconnect(true, reconnect_timing, 4, cx);
            assert_eq!(
                entity.reconnect_timing.ssh_keepalive_interval,
                Duration::from_secs(37)
            );
        });
    }

    #[gpui::test]
    fn reconnect_jobs_and_configuration_are_entity_owned(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let node_id = NodeId::new("node-a");

        entity.update(cx, |entity, cx| {
            entity.configure_reconnect(true, ReconnectTiming::default(), 4, cx);
            let job = entity.reconnect_orchestrator().schedule(
                node_id.0.clone(),
                "Node A",
                ReconnectSnapshot::default(),
            );

            assert_eq!(job.max_attempts, 4);
            assert!(entity.has_active_reconnect_job(&node_id));
            assert!(entity.reconnect_job_is_current(&node_id, &job.job_id));
            assert_eq!(entity.active_reconnect_node_ids(), vec![node_id.clone()]);

            entity.reconnect_orchestrator().finish(&node_id.0, Ok(0));
            assert!(!entity.has_active_reconnect_job(&node_id));
        });
    }

    #[gpui::test]
    fn entity_release_preserves_registry_consumers(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_consumer = ConnectionConsumer::NodeRouter("test-node".to_string());
        let connection_handle = ssh_registry.acquire(SshConfig::default(), node_consumer.clone());
        let connection_id = connection_handle.connection_id().to_string();
        let entity_registry = ssh_registry.clone();
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                entity_registry,
                NodeRuntimeStore::default(),
                NodeEventEmitter::default(),
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });

        drop(entity);
        cx.update(|_cx| {});

        let connection_info = ssh_registry
            .get(&connection_id)
            .expect("runtime connection remains registered")
            .info();
        assert_eq!(connection_info.ref_count, 1);
        assert_eq!(connection_info.consumers, vec![node_consumer]);
    }

    #[gpui::test]
    fn node_event_delivery_filters_stale_generations_inside_entity(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_event_emitter = NodeEventEmitter::default();
        let entity_emitter = node_event_emitter.clone();
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry,
                NodeRuntimeStore::default(),
                entity_emitter,
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });
        let node_events_ready = Arc::new(AtomicBool::new(false));
        let node_events_ready_flag = node_events_ready.clone();
        let _event_subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &WorkspaceRuntimeEvent, _cx| {
                if *event == WorkspaceRuntimeEvent::NodeEventsReady {
                    node_events_ready_flag.store(true, Ordering::Release);
                }
            })
        });

        node_event_emitter.emit(NodeStateEvent::ConnectionStateChanged {
            node_id: "node-a".to_string(),
            generation: 2,
            state: NodeReadiness::Ready,
            reason: String::new(),
        });
        node_event_emitter.emit(NodeStateEvent::SftpReady {
            node_id: "node-a".to_string(),
            generation: 1,
            ready: true,
            cwd: Some("/tmp".to_string()),
        });
        node_event_emitter.emit(NodeStateEvent::TerminalEndpointChanged {
            node_id: "node-a".to_string(),
            generation: 3,
            available: true,
        });

        cx.run_until_parked();
        assert!(node_events_ready.load(Ordering::Acquire));

        entity.update(cx, |entity, _cx| {
            let events = entity.take_node_events();
            assert_eq!(events.len(), 2);
            assert!(matches!(
                events.front(),
                Some(NodeStateEvent::ConnectionStateChanged { generation: 2, .. })
            ));
            assert!(matches!(
                events.back(),
                Some(NodeStateEvent::TerminalEndpointChanged { generation: 3, .. })
            ));
            assert_eq!(
                entity.node_event_generations.get(&NodeId::new("node-a")),
                Some(&3)
            );
        });
    }

    #[gpui::test]
    fn reconnect_debounce_selects_minimal_runtime_subtrees(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_runtime_store = NodeRuntimeStore::default();
        let root = NodeId::new("root");
        let child = NodeId::new("child");
        node_runtime_store.upsert_node(root.clone(), SshConfig::default());
        node_runtime_store
            .upsert_child_node(root.clone(), child.clone(), SshConfig::default())
            .unwrap();
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry,
                node_runtime_store,
                NodeEventEmitter::default(),
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });

        entity.update(cx, |entity, cx| {
            entity.pending_reconnect_node_ids.insert(child);
            entity.pending_reconnect_node_ids.insert(root.clone());
            entity.reconnect_debounce_generation = 7;
            entity.flush_reconnect_roots(6, cx);
            assert!(entity.ready_reconnect_roots.is_empty());
            entity.flush_reconnect_roots(7, cx);
            assert_eq!(entity.take_reconnect_roots(), VecDeque::from([root]));
        });
    }

    #[gpui::test]
    fn disabling_reconnect_clears_pending_debounce_state(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        entity.update(cx, |entity, cx| {
            entity.queue_reconnect_root(NodeId::new("node-a"), cx);
            let scheduled_generation = entity.reconnect_debounce_generation;
            assert!(!entity.pending_reconnect_node_ids.is_empty());

            entity.configure_reconnect(false, ReconnectTiming::default(), 3, cx);

            assert!(entity.pending_reconnect_node_ids.is_empty());
            assert!(entity.reconnect_debounce_generation > scheduled_generation);
        });
    }

    #[gpui::test]
    fn reconnect_pipeline_is_single_owner_and_requeue_is_bounded(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        entity.update(cx, |entity, cx| {
            let active_node_id = NodeId::new("node-a");
            let waiting_node_id = NodeId::new("node-b");
            assert_eq!(
                entity.claim_reconnect_pipeline(
                    &active_node_id,
                    Some("connection-a".to_string()),
                    Duration::from_secs(60),
                    cx,
                ),
                ReconnectPipelineClaim::Acquired
            );
            assert_eq!(
                entity.claim_reconnect_pipeline(
                    &waiting_node_id,
                    Some("connection-b".to_string()),
                    Duration::from_secs(60),
                    cx,
                ),
                ReconnectPipelineClaim::Requeued
            );
            entity
                .reconnect_requeue_states
                .get_mut(&waiting_node_id)
                .expect("waiting reconnect state")
                .attempt = RECONNECT_MAX_REQUEUE;
            assert_eq!(
                entity.claim_reconnect_pipeline(
                    &waiting_node_id,
                    Some("connection-b".to_string()),
                    Duration::from_secs(60),
                    cx,
                ),
                ReconnectPipelineClaim::Exhausted
            );
            assert!(
                !entity
                    .reconnect_requeue_states
                    .contains_key(&waiting_node_id)
            );

            entity.release_reconnect_pipeline(&active_node_id);
            assert_eq!(
                entity.claim_reconnect_pipeline(
                    &waiting_node_id,
                    None,
                    Duration::from_secs(60),
                    cx,
                ),
                ReconnectPipelineClaim::Acquired
            );
        });
    }

    #[gpui::test]
    fn reconnect_scheduler_cancel_clears_owned_state_and_actions(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        entity.update(cx, |entity, cx| {
            let active_node_id = NodeId::new("node-a");
            let child_node_id = NodeId::new("node-b");
            assert_eq!(
                entity
                    .claim_reconnect_pipeline(&active_node_id, None, Duration::from_secs(60), cx,),
                ReconnectPipelineClaim::Acquired
            );
            entity.replace_reconnect_cascade([active_node_id.clone(), child_node_id.clone()]);
            entity.reconnect_schedule_actions.push_back(
                ReconnectScheduleAction::ContinueConnectionChain {
                    node_id: child_node_id.clone(),
                },
            );

            entity.cancel_queued_reconnects(&[active_node_id.clone(), child_node_id]);

            assert!(entity.reconnect_pipeline_active_node.is_none());
            assert!(entity.pending_reconnect_cascade_nodes.is_empty());
            assert!(entity.reconnect_schedule_actions.is_empty());
        });
    }

    #[gpui::test]
    fn reconnect_cascade_preserves_fifo_order(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        entity.update(cx, |entity, _cx| {
            let first_node_id = NodeId::new("node-a");
            let second_node_id = NodeId::new("node-b");
            entity.replace_reconnect_cascade([first_node_id.clone(), second_node_id.clone()]);

            assert_eq!(
                entity.take_next_reconnect_cascade_node(),
                Some(first_node_id)
            );
            assert_eq!(
                entity.take_next_reconnect_cascade_node(),
                Some(second_node_id)
            );
            assert_eq!(entity.take_next_reconnect_cascade_node(), None);
        });
    }

    #[gpui::test]
    fn delayed_reconnect_actions_emit_without_worker_delivery(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let schedule_event_seen = Arc::new(AtomicBool::new(false));
        let schedule_event_flag = schedule_event_seen.clone();
        let _event_subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &WorkspaceRuntimeEvent, _cx| {
                if *event == WorkspaceRuntimeEvent::ReconnectScheduleReady {
                    schedule_event_flag.store(true, Ordering::Release);
                }
            })
        });
        entity.update(cx, |entity, cx| {
            entity.schedule_reconnect_action(
                ReconnectScheduleAction::ContinueConnectionChain {
                    node_id: NodeId::new("node-a"),
                },
                Duration::ZERO,
                cx,
            );
        });

        cx.run_until_parked();

        entity.update(cx, |entity, _cx| {
            assert!(matches!(
                entity.take_reconnect_schedule_actions().front(),
                Some(ReconnectScheduleAction::ContinueConnectionChain { .. })
            ));
            let (_ssh_results, reconnect_results) = entity.take_worker_results();
            assert!(reconnect_results.is_empty());
        });
        assert!(schedule_event_seen.load(Ordering::Acquire));
    }

    #[gpui::test]
    fn reconnect_transfer_resume_state_deduplicates_and_completes(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        entity.update(cx, |entity, _cx| {
            let reconnect_node_id = NodeId::new("root");
            let transfer_node_id = NodeId::new("child");
            let requests = entity.begin_reconnect_transfer_resumes(
                &reconnect_node_id,
                [
                    (transfer_node_id.clone(), "transfer-a".to_string()),
                    (transfer_node_id.clone(), "transfer-a".to_string()),
                    (transfer_node_id, "transfer-b".to_string()),
                ],
            );
            assert_eq!(requests.len(), 2);
            assert!(
                entity
                    .finish_reconnect_transfer_resume("transfer-a", true)
                    .is_empty()
            );
            assert_eq!(
                entity.finish_reconnect_transfer_resume("transfer-b", false),
                vec![ReconnectTransferResumeCompletion {
                    node_id: reconnect_node_id,
                    resumed: 1,
                }]
            );
        });
    }

    #[gpui::test]
    fn reconnect_restore_counts_are_consumed_once(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        entity.update(cx, |entity, _cx| {
            let node_id = NodeId::new("node-a");
            entity.remember_ide_restore_transfer_count(node_id.clone(), 3);
            entity.complete_forward_restore(&node_id, 2);

            assert_eq!(entity.complete_reconnect_restore_counts(&node_id), (2, 3));
            assert_eq!(entity.complete_reconnect_restore_counts(&node_id), (0, 0));
        });
    }

    #[gpui::test]
    fn node_cancellation_stops_forward_restore_and_clears_counts(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        entity.update(cx, |entity, _cx| {
            let node_id = NodeId::new("node-a");
            let cancellation = entity.begin_forward_restore(&node_id);
            entity.remember_ide_restore_transfer_count(node_id.clone(), 4);
            assert!(cancellation.load(Ordering::Acquire));

            entity.cancel_queued_reconnects(std::slice::from_ref(&node_id));

            assert!(!cancellation.load(Ordering::Acquire));
            assert_eq!(entity.complete_reconnect_restore_counts(&node_id), (0, 0));
        });
    }
}
