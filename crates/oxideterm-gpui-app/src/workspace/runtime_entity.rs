// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::new_connection::SshConnectionWorkerResult;
use super::*;
use oxideterm_settings::RemoteShellIntegrationMode;
use oxideterm_ssh::{
    ManagedKeyResolver, ReconnectForwardRestorePlan, ReconnectIdeSnapshot, ReconnectJob,
    ReconnectTiming,
};

const ACTIVE_PROBE_START_DELAY: Duration = Duration::from_millis(530);
const RECONNECT_DEBOUNCE_DELAY: Duration = Duration::from_millis(500);
const RECONNECT_MAX_REQUEUE: u32 = 120;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum WorkspaceRuntimeEvent {
    EffectsReady,
}

#[derive(Debug)]
pub(in crate::workspace) enum WorkspaceRuntimeEffect {
    Reconnect(ReconnectRuntimeEffect),
    Node(NodeRuntimeEffect),
    OpenReadySshTerminals {
        requests: Vec<PendingSshTerminalOpen>,
    },
    StartReconnectRoot {
        node_id: NodeId,
    },
    ContinueConnectionChain {
        node_id: NodeId,
    },
    ContinueReconnectCascade,
    StartReconnectPipeline {
        node_id: NodeId,
    },
    RetryNodeConnect {
        node_id: NodeId,
        attempt: u32,
        max_attempts: u32,
    },
    ReconnectRecoveredBeforeRetry {
        node_id: NodeId,
    },
    ReconnectJobCleaned,
    ConnectionTrace(ConnectionTraceEvent),
    ActiveConnectionsChanged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkspaceRuntimeLifecycle {
    Running,
    ShuttingDown,
    Stopped,
}

#[derive(Debug)]
pub(in crate::workspace) struct PendingSshTerminalOpen {
    pub(in crate::workspace) node_id: NodeId,
    pub(in crate::workspace) post_connect_command: Option<String>,
    pub(in crate::workspace) mark_used_connection_id: Option<String>,
    pub(in crate::workspace) save_after_open: Option<SaveConnectionRequest>,
    pub(in crate::workspace) cleanup_node_id: Option<NodeId>,
    pub(in crate::workspace) title: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum QueueSshTerminalOpenOutcome {
    Queued,
    Coalesced,
    Ready,
    WorkspaceShuttingDown,
}

#[derive(Debug)]
pub(in crate::workspace) enum ReconnectRuntimeEffect {
    NodeConnected {
        node_id: NodeId,
        connection_id: String,
        reconnecting: bool,
    },
    NodeConnectFailed {
        node_id: NodeId,
        error: String,
        action: ReconnectFailureAction,
    },
    GraceRecovered {
        node_id: NodeId,
        connection_id: String,
        recovered_connections: Vec<(NodeId, String)>,
    },
    GraceExpired {
        node_id: NodeId,
        detail: String,
    },
    SftpTransfersSnapshotted {
        node_id: NodeId,
        entered_grace_period: bool,
    },
    RemoteShellIntegrationGateFinished {
        notice: Option<settings::RemoteShellIntegrationNotice>,
    },
    RemoteShellIntegrationMaintenanceFinished {
        notice: settings::RemoteShellIntegrationNotice,
    },
}

#[derive(Debug)]
pub(in crate::workspace) enum ReconnectFailureAction {
    InitialConnect,
    Retry {
        attempt: u32,
        max_attempts: u32,
        delay: Duration,
        job_id: String,
    },
    FinishReconnect,
}

#[derive(Debug)]
pub(in crate::workspace) enum NodeRuntimeEffect {
    ConnectionStatusChanged {
        node_id: NodeId,
        connection_id: String,
        status: String,
        state: NodeReadiness,
        reason: String,
        affected_children: Vec<String>,
    },
    ConnectionStateChanged {
        node_id: String,
        state: NodeReadiness,
        reason: String,
    },
    SftpReady {
        node_id: String,
        ready: bool,
        cwd: Option<String>,
    },
    SharedSftpSessionChanged {
        node_id: String,
        connection_id: String,
        session_generation: Option<u64>,
        ready: bool,
    },
    TerminalEndpointChanged,
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

#[derive(Debug)]
pub(in crate::workspace) enum NodeTransportStartError {
    MissingRuntime,
    Route(String),
}

pub(in crate::workspace) enum ReconnectPostTerminalAction {
    RestoreForwards,
    ResumeTransfers,
}

pub(in crate::workspace) enum ReconnectPhaseOutcome {
    Failed,
    Continue,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::workspace) struct NodeTransportAttemptId(u64);

#[derive(Debug)]
struct NodeTransportAttempt {
    id: NodeTransportAttemptId,
    connection_id: String,
    abort_handle: tokio::task::AbortHandle,
}

pub(in crate::workspace) struct ReconnectGraceProbeRequest {
    pub(in crate::workspace) node_id: NodeId,
    pub(in crate::workspace) connection_id: String,
    pub(in crate::workspace) affected_transfer_nodes: Vec<(NodeId, String)>,
    pub(in crate::workspace) old_connections_by_node: Vec<ReconnectNodeConnectionSnapshot>,
    pub(in crate::workspace) old_connection_count: usize,
    pub(in crate::workspace) progress_store: Arc<dyn ProgressStore>,
    pub(in crate::workspace) job_id: String,
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

struct ScheduledReconnectTask {
    node_id: Option<NodeId>,
    task: Task<()>,
}

#[derive(Debug)]
pub(in crate::workspace) struct ConnectionChainStep {
    pub(in crate::workspace) node_id: NodeId,
    pub(in crate::workspace) trace_plan: Arc<ConnectionTracePlan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum ConnectionChainAdvance {
    Ignored,
    Continue,
    Complete,
}

#[derive(Debug)]
struct ConnectionChainRun {
    next_index: usize,
    trace_plan: Arc<ConnectionTracePlan>,
}

/// Owns runtime worker endpoints and reliable delivery independently from tabs.
pub(in crate::workspace) struct WorkspaceRuntimeEntity {
    ssh_worker_tx: delivery::ActiveDeliverySender<SshConnectionWorkerResult>,
    reconnect_worker_tx: delivery::ActiveDeliverySender<ReconnectWorkerResult>,
    reconnect_worker_rx: std::sync::mpsc::Receiver<ReconnectWorkerResult>,
    active_probe_tx: delivery::ActiveDeliverySender<usize>,
    active_probe_rx: std::sync::mpsc::Receiver<usize>,
    _node_event_subscription: NodeEventSubscription,
    node_event_rx: NodeEventReceiver,
    runtime_effects: VecDeque<WorkspaceRuntimeEffect>,
    runtime_effect_delivery_pending: bool,
    lifecycle: WorkspaceRuntimeLifecycle,
    terminal_ssh_nodes: HashMap<TerminalSessionId, NodeId>,
    terminal_endpoint_sessions: HashMap<TerminalSessionId, SharedTerminalSession>,
    pending_ssh_terminal_opens: VecDeque<PendingSshTerminalOpen>,
    node_transport_attempts: HashMap<NodeId, NodeTransportAttempt>,
    next_node_transport_attempt_id: u64,
    node_event_generations: HashMap<NodeId, u64>,
    node_router: NodeRouter,
    reconnect_enabled: bool,
    pending_reconnect_node_ids: HashSet<NodeId>,
    reconnect_debounce_generation: u64,
    reconnect_debounce_task: Option<Task<()>>,
    reconnect_pipeline_active_node: Option<NodeId>,
    reconnect_requeue_states: HashMap<NodeId, ReconnectRequeueState>,
    reconnect_requeue_tasks: HashMap<NodeId, Task<()>>,
    pending_reconnect_cascade_nodes: VecDeque<NodeId>,
    reconnect_cascade_generation: u64,
    reconnect_cascade_task: Option<Task<()>>,
    reconnect_schedule_tasks: Vec<ScheduledReconnectTask>,
    // Restore bookkeeping survives page changes and is cancelled only by node lifecycle actions.
    pending_reconnect_transfer_resumes: HashMap<NodeId, HashSet<String>>,
    reconnect_transfer_resume_successes: HashMap<NodeId, usize>,
    pending_ide_restore_transfer_counts: HashMap<NodeId, u32>,
    reconnect_forward_restore_totals: HashMap<NodeId, u32>,
    reconnect_forward_restore_tokens: HashMap<NodeId, Arc<AtomicBool>>,
    reconnect_orchestrator: ReconnectOrchestratorStore,
    active_connection_chain: Option<ConnectionChainRun>,
    connecting_node_locks: HashSet<NodeId>,
    connection_trace_state: ConnectionTraceState,
    ssh_registry: SshConnectionRegistry,
    task_runtime: Arc<tokio::runtime::Runtime>,
    reconnect_timing: ReconnectTiming,
    ssh_active_probe_in_flight: bool,
    active_probe_task: Option<tokio::task::AbortHandle>,
    active_probe_timer_generation: u64,
    active_probe_timer_task: Option<Task<()>>,
    reconnect_grace_probe_tasks: HashMap<NodeId, (String, tokio::task::AbortHandle)>,
    remote_shell_integration: settings::RemoteShellIntegrationRuntimeState,
    remote_shell_gate_tasks: HashMap<NodeId, (u64, tokio::task::AbortHandle)>,
    remote_shell_maintenance_task: Option<(NodeId, u64, tokio::task::AbortHandle)>,
}

impl WorkspaceRuntimeEntity {
    pub(in crate::workspace) fn task_runtime(&self) -> Arc<tokio::runtime::Runtime> {
        self.task_runtime.clone()
    }

    pub(in crate::workspace) fn native_ssh_prompt_handler(&self) -> Arc<NativeSshPromptHandler> {
        // Additional terminal logins use the same UI-owned prompt delivery
        // channel as the source node's initial authentication.
        Arc::new(NativeSshPromptHandler::new(self.ssh_worker_tx.clone()))
    }

    #[cfg(test)]
    pub(in crate::workspace) fn new(
        ssh_registry: SshConnectionRegistry,
        node_router: NodeRouter,
        task_runtime: Arc<tokio::runtime::Runtime>,
        reconnect_enabled: bool,
        reconnect_timing: ReconnectTiming,
        reconnect_max_attempts: u32,
        cx: &mut Context<Self>,
    ) -> Self {
        let (ssh_worker_tx, _ssh_worker_rx) = delivery::ActiveDeliverySender::channel_with_wake(
            delivery::ActiveDeliveryWake::default(),
        );
        Self::new_with_ssh_worker_sender(
            ssh_worker_tx,
            ssh_registry,
            node_router,
            task_runtime,
            reconnect_enabled,
            reconnect_timing,
            reconnect_max_attempts,
            cx,
        )
    }

    pub(in crate::workspace) fn new_with_ssh_worker_sender(
        ssh_worker_tx: delivery::ActiveDeliverySender<SshConnectionWorkerResult>,
        ssh_registry: SshConnectionRegistry,
        node_router: NodeRouter,
        task_runtime: Arc<tokio::runtime::Runtime>,
        reconnect_enabled: bool,
        reconnect_timing: ReconnectTiming,
        reconnect_max_attempts: u32,
        cx: &mut Context<Self>,
    ) -> Self {
        let runtime_wake = delivery::ActiveDeliveryWake::default();
        let (reconnect_worker_tx, reconnect_worker_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_wake.clone());
        let (active_probe_tx, active_probe_rx) =
            delivery::ActiveDeliverySender::channel_with_wake(runtime_wake.clone());
        // Node state is a latest-value stream. Its bounded mailbox may retain
        // reliable lifecycle events beyond capacity, while the shared wake
        // lets this Entity drain every runtime source without a root waiter.
        let emitter_wake = runtime_wake.clone();
        let (node_event_subscription, node_event_rx) = node_router
            .emitter()
            .subscribe_bounded_with_wake(256, Some(Arc::new(move || emitter_wake.mark())));
        let mut entity = Self {
            ssh_worker_tx,
            reconnect_worker_tx,
            reconnect_worker_rx,
            active_probe_tx,
            active_probe_rx,
            _node_event_subscription: node_event_subscription,
            node_event_rx,
            runtime_effects: VecDeque::new(),
            runtime_effect_delivery_pending: false,
            lifecycle: WorkspaceRuntimeLifecycle::Running,
            terminal_ssh_nodes: HashMap::new(),
            terminal_endpoint_sessions: HashMap::new(),
            pending_ssh_terminal_opens: VecDeque::new(),
            node_transport_attempts: HashMap::new(),
            next_node_transport_attempt_id: 0,
            node_event_generations: HashMap::new(),
            node_router,
            reconnect_enabled,
            pending_reconnect_node_ids: HashSet::new(),
            reconnect_debounce_generation: 0,
            reconnect_debounce_task: None,
            reconnect_pipeline_active_node: None,
            reconnect_requeue_states: HashMap::new(),
            reconnect_requeue_tasks: HashMap::new(),
            pending_reconnect_cascade_nodes: VecDeque::new(),
            reconnect_cascade_generation: 0,
            reconnect_cascade_task: None,
            reconnect_schedule_tasks: Vec::new(),
            pending_reconnect_transfer_resumes: HashMap::new(),
            reconnect_transfer_resume_successes: HashMap::new(),
            pending_ide_restore_transfer_counts: HashMap::new(),
            reconnect_forward_restore_totals: HashMap::new(),
            reconnect_forward_restore_tokens: HashMap::new(),
            reconnect_orchestrator: ReconnectOrchestratorStore::new(
                reconnect_timing,
                reconnect_max_attempts,
            ),
            active_connection_chain: None,
            connecting_node_locks: HashSet::new(),
            connection_trace_state: ConnectionTraceState::default(),
            ssh_registry,
            task_runtime,
            reconnect_timing,
            ssh_active_probe_in_flight: false,
            active_probe_task: None,
            active_probe_timer_generation: 0,
            active_probe_timer_task: None,
            reconnect_grace_probe_tasks: HashMap::new(),
            remote_shell_integration: settings::RemoteShellIntegrationRuntimeState::default(),
            remote_shell_gate_tasks: HashMap::new(),
            remote_shell_maintenance_task: None,
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
            self.reconnect_debounce_task = None;
        }
        self.reconnect_orchestrator
            .configure(reconnect_timing, reconnect_max_attempts);
        self.reconnect_timing = reconnect_timing;
        self.schedule_active_probe_after(ACTIVE_PROBE_START_DELAY, cx);
    }

    pub(in crate::workspace) fn register_ssh_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        node_id: NodeId,
    ) -> bool {
        if self.lifecycle != WorkspaceRuntimeLifecycle::Running {
            return false;
        }
        self.terminal_ssh_nodes.insert(session_id, node_id);
        true
    }

    pub(in crate::workspace) fn unregister_ssh_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
    ) -> Option<NodeId> {
        // Removing a terminal consumer never changes node or transport ownership.
        self.terminal_endpoint_sessions.remove(&session_id);
        let node_id = self.terminal_ssh_nodes.remove(&session_id)?;
        let endpoint_session_id = session_id.0.to_string();
        let _ = self
            .node_router
            .unbind_terminal_session(&node_id, &endpoint_session_id);
        Some(node_id)
    }

    pub(in crate::workspace) fn retain_terminal_endpoint_session(
        &mut self,
        session_id: TerminalSessionId,
        session: SharedTerminalSession,
    ) -> bool {
        if self.lifecycle != WorkspaceRuntimeLifecycle::Running
            || !self.terminal_ssh_nodes.contains_key(&session_id)
        {
            return false;
        }
        // Runtime retains the shared terminal independently of any tab or
        // native window mount until the terminal registration is removed.
        self.terminal_endpoint_sessions.insert(session_id, session);
        true
    }

    pub(in crate::workspace) fn terminal_session_lifecycles(&self) -> Vec<TerminalLifecycle> {
        self.terminal_endpoint_sessions
            .values()
            .map(|session| session.lock().lifecycle())
            .collect()
    }

    pub(in crate::workspace) fn bind_ssh_terminal_endpoint(
        &self,
        node_id: &NodeId,
        endpoint: TerminalEndpoint,
    ) {
        // NodeRouter dispatches the availability event as part of the bind.
        let _ = self.node_router.bind_terminal_endpoint(node_id, endpoint);
    }

    pub(in crate::workspace) fn ssh_terminal_node_id(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<NodeId> {
        self.terminal_ssh_nodes.get(&session_id).cloned()
    }

    pub(in crate::workspace) fn ssh_terminal_nodes(&self) -> Vec<(TerminalSessionId, NodeId)> {
        self.terminal_ssh_nodes
            .iter()
            .map(|(session_id, node_id)| (*session_id, node_id.clone()))
            .collect()
    }

    pub(in crate::workspace) fn ssh_terminal_session_ids_for_node(
        &self,
        node_id: &NodeId,
    ) -> Vec<TerminalSessionId> {
        let mut session_ids = self
            .terminal_ssh_nodes
            .iter()
            .filter_map(|(session_id, session_node_id)| {
                (session_node_id == node_id).then_some(*session_id)
            })
            .collect::<Vec<_>>();
        // Session ids are monotonic, preserving the previous projection's
        // stable oldest-terminal focus behavior despite HashMap storage.
        session_ids.sort_unstable_by_key(|session_id| session_id.0);
        session_ids
    }

    pub(in crate::workspace) fn ssh_terminal_session_belongs_to_node(
        &self,
        session_id: TerminalSessionId,
        node_id: &NodeId,
    ) -> bool {
        self.terminal_ssh_nodes.get(&session_id) == Some(node_id)
    }

    pub(in crate::workspace) fn queue_ssh_terminal_open(
        &mut self,
        mut request: PendingSshTerminalOpen,
        cx: &mut Context<Self>,
    ) -> QueueSshTerminalOpenOutcome {
        if self.lifecycle != WorkspaceRuntimeLifecycle::Running {
            return QueueSshTerminalOpenOutcome::WorkspaceShuttingDown;
        }
        let outcome = if let Some(existing) = self
            .pending_ssh_terminal_opens
            .iter_mut()
            .find(|pending| pending.node_id == request.node_id)
        {
            // Preserve the first visible terminal request while adopting later
            // one-shot side effects without cloning secret-bearing save state.
            if existing.mark_used_connection_id.is_none() {
                existing.mark_used_connection_id = request.mark_used_connection_id.take();
            }
            if existing.save_after_open.is_none() {
                existing.save_after_open = request.save_after_open.take();
            }
            if existing.post_connect_command.is_none() {
                existing.post_connect_command = request.post_connect_command.take();
            }
            QueueSshTerminalOpenOutcome::Coalesced
        } else {
            self.pending_ssh_terminal_opens.push_back(request);
            QueueSshTerminalOpenOutcome::Queued
        };
        let requests = self.take_ready_ssh_terminal_opens();
        if requests.is_empty() {
            outcome
        } else {
            self.push_runtime_effect(
                WorkspaceRuntimeEffect::OpenReadySshTerminals { requests },
                cx,
            );
            QueueSshTerminalOpenOutcome::Ready
        }
    }

    pub(in crate::workspace) fn mark_pending_ssh_terminal_open_cleanup(
        &mut self,
        node_id: &NodeId,
        cleanup_node_id: NodeId,
    ) {
        if let Some(pending) = self
            .pending_ssh_terminal_opens
            .iter_mut()
            .find(|pending| pending.node_id == *node_id)
        {
            // Only newly materialized direct roots are removed after failure.
            pending.cleanup_node_id = Some(cleanup_node_id);
        }
    }

    pub(in crate::workspace) fn pending_ssh_terminal_open_cleanup_for_node(
        &self,
        node_id: &NodeId,
    ) -> Option<NodeId> {
        self.pending_ssh_terminal_opens
            .iter()
            .find(|pending| pending.node_id == *node_id)
            .and_then(|pending| pending.cleanup_node_id.clone())
    }

    pub(in crate::workspace) fn remove_pending_ssh_terminal_opens_for_node(
        &mut self,
        node_id: &NodeId,
    ) -> bool {
        let before = self.pending_ssh_terminal_opens.len();
        self.pending_ssh_terminal_opens
            .retain(|pending| pending.node_id != *node_id);
        self.pending_ssh_terminal_opens.len() != before
    }

    fn take_ready_ssh_terminal_opens(&mut self) -> Vec<PendingSshTerminalOpen> {
        let mut remaining = VecDeque::new();
        let mut ready = Vec::new();
        while let Some(request) = self.pending_ssh_terminal_opens.pop_front() {
            let node_is_ready = self
                .node_router
                .node_state(&request.node_id)
                .is_ok_and(|snapshot| snapshot.state.readiness == NodeReadiness::Ready);
            if node_is_ready {
                ready.push(request);
            } else {
                remaining.push_back(request);
            }
        }
        self.pending_ssh_terminal_opens = remaining;
        ready
    }

    fn publish_ssh_terminal_opens_for_connected_node(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
        let mut remaining = VecDeque::new();
        let mut ready = Vec::new();
        while let Some(request) = self.pending_ssh_terminal_opens.pop_front() {
            if request.node_id == *node_id {
                ready.push(request);
            } else {
                remaining.push_back(request);
            }
        }
        self.pending_ssh_terminal_opens = remaining;
        if !ready.is_empty() {
            self.push_runtime_effect(
                WorkspaceRuntimeEffect::OpenReadySshTerminals { requests: ready },
                cx,
            );
        }
    }

    pub(in crate::workspace) fn has_active_reconnect_job(&self, node_id: &NodeId) -> bool {
        self.reconnect_orchestrator.is_active(&node_id.0)
    }

    pub(in crate::workspace) fn complete_reconnect_terminal_remount(
        &self,
        node_id: &NodeId,
        detail: String,
    ) -> Option<ReconnectPostTerminalAction> {
        if !self.reconnect_orchestrator.is_active(&node_id.0) {
            return None;
        }
        let _ =
            self.reconnect_orchestrator
                .complete_phase(&node_id.0, PhaseResult::Ok, Some(detail));
        let _ = self
            .reconnect_orchestrator
            .advance(&node_id.0, ReconnectPhase::RestoreForwards);
        if self.reconnect_orchestrator.has_forward_snapshot(&node_id.0) {
            Some(ReconnectPostTerminalAction::RestoreForwards)
        } else {
            let detail = "no forward rules in snapshot".to_string();
            let _ = self.reconnect_orchestrator.complete_phase(
                &node_id.0,
                PhaseResult::Skipped,
                Some(detail),
            );
            let _ = self
                .reconnect_orchestrator
                .advance(&node_id.0, ReconnectPhase::ResumeTransfers);
            Some(ReconnectPostTerminalAction::ResumeTransfers)
        }
    }

    pub(in crate::workspace) fn reconnect_terminal_session_ids(
        &self,
        node_id: &NodeId,
    ) -> Vec<String> {
        self.reconnect_orchestrator
            .terminal_session_ids_for_node(&node_id.0)
    }

    pub(in crate::workspace) fn reconnect_incomplete_sftp_transfers(
        &self,
        node_id: &NodeId,
    ) -> Vec<ReconnectNodeTransferSnapshot> {
        self.reconnect_orchestrator
            .incomplete_sftp_transfers(&node_id.0)
    }

    pub(in crate::workspace) fn start_reconnect_job(
        &self,
        node_id: &NodeId,
        node_name: String,
        snapshot: ReconnectSnapshot,
    ) -> ReconnectJob {
        let job = self
            .reconnect_orchestrator
            .schedule(node_id.0.clone(), node_name, snapshot);
        let _ = self
            .reconnect_orchestrator
            .advance(&node_id.0, ReconnectPhase::Snapshot);
        job
    }

    pub(in crate::workspace) fn finish_reconnect_job_state(
        &self,
        node_id: &NodeId,
        result: Result<u32, String>,
        verification_detail: Option<String>,
    ) -> Option<ReconnectJob> {
        if result.is_ok()
            && let Some(detail) = verification_detail
        {
            let _ = self.reconnect_orchestrator.complete_phase(
                &node_id.0,
                PhaseResult::Ok,
                Some(detail),
            );
        }
        let job = self.reconnect_orchestrator.finish(&node_id.0, result);
        self.reconnect_orchestrator
            .enforce_terminal_job_cap(MAX_RETAINED_RECONNECT_JOBS);
        job
    }

    pub(in crate::workspace) fn reconnect_forward_restore_plan(
        &self,
        node_id: &NodeId,
    ) -> Option<ReconnectForwardRestorePlan> {
        self.reconnect_orchestrator.forward_restore_plan(&node_id.0)
    }

    pub(in crate::workspace) fn reconnect_forward_rule_snapshots(
        &self,
        node_id: &NodeId,
    ) -> Vec<ReconnectForwardRuleSnapshot> {
        self.reconnect_orchestrator
            .forward_rule_snapshots(&node_id.0)
    }

    pub(in crate::workspace) fn reconnect_active_progress(
        &self,
        node_id: &NodeId,
    ) -> Option<ReconnectProgress> {
        self.reconnect_orchestrator.active_progress(&node_id.0)
    }

    pub(in crate::workspace) fn reconnect_ide_snapshot(
        &self,
        node_id: &NodeId,
    ) -> Option<(ReconnectIdeSnapshot, Option<SystemTime>)> {
        self.reconnect_orchestrator.ide_snapshot(&node_id.0)
    }

    pub(in crate::workspace) fn complete_reconnect_transfer_resume(
        &self,
        node_id: &NodeId,
        result: PhaseResult,
        detail: String,
    ) -> bool {
        if !self.reconnect_orchestrator.is_active(&node_id.0) {
            return false;
        }
        let _ = self
            .reconnect_orchestrator
            .complete_phase(&node_id.0, result, Some(detail));
        let _ = self
            .reconnect_orchestrator
            .advance(&node_id.0, ReconnectPhase::RestoreIde);
        true
    }

    pub(in crate::workspace) fn complete_reconnect_ide_restore(
        &self,
        node_id: &NodeId,
        result: PhaseResult,
        detail: String,
    ) -> Option<ReconnectPhaseOutcome> {
        if !self.reconnect_orchestrator.is_active(&node_id.0) {
            return None;
        }
        let _ = self
            .reconnect_orchestrator
            .complete_phase(&node_id.0, result, Some(detail));
        if result == PhaseResult::Failed {
            Some(ReconnectPhaseOutcome::Failed)
        } else {
            let _ = self
                .reconnect_orchestrator
                .advance(&node_id.0, ReconnectPhase::Verify);
            Some(ReconnectPhaseOutcome::Continue)
        }
    }

    pub(in crate::workspace) fn complete_reconnect_forward_restore(
        &self,
        node_id: &NodeId,
        result: PhaseResult,
        detail: String,
    ) -> Option<ReconnectPhaseOutcome> {
        if !self.reconnect_orchestrator.is_active(&node_id.0) {
            return None;
        }
        let _ = self
            .reconnect_orchestrator
            .complete_phase(&node_id.0, result, Some(detail));
        if result == PhaseResult::Failed {
            Some(ReconnectPhaseOutcome::Failed)
        } else {
            let _ = self
                .reconnect_orchestrator
                .advance(&node_id.0, ReconnectPhase::ResumeTransfers);
            Some(ReconnectPhaseOutcome::Continue)
        }
    }

    pub(in crate::workspace) fn start_node_transport(
        &mut self,
        node_id: &NodeId,
        managed_key_resolver: ManagedKeyResolver,
    ) -> Result<(), NodeTransportStartError> {
        // A replacement must retire the old registry entry before reacquiring the
        // same logical consumer, so an old task cannot release the new attempt.
        self.cancel_node_transport_attempt(node_id);
        let runtime_snapshot = self
            .node_router
            .node_runtime_snapshot(node_id)
            .ok_or(NodeTransportStartError::MissingRuntime)?;
        let stale_connection_id =
            self.node_router
                .connection_id_for_node(node_id)
                .filter(|connection_id| {
                    self.ssh_registry.get(connection_id).is_some_and(|handle| {
                        matches!(
                            handle.state(),
                            ConnectionState::LinkDown
                                | ConnectionState::Disconnected
                                | ConnectionState::Disconnecting
                                | ConnectionState::Error(_)
                        )
                    })
                });
        let force_reconnect = stale_connection_id.is_some();
        if let Some(connection_id) = stale_connection_id.as_deref() {
            self.retire_node_connection(node_id, connection_id);
        }

        let parent_id = runtime_snapshot.parent_id;
        let config = runtime_snapshot.config;
        let consumer = ConnectionConsumer::NodeRouter(node_id.0.clone());
        // The registry and the in-flight authentication attempt are distinct
        // secret owners, so both receive zeroizing SshConfig instances.
        let node_handle = self.ssh_registry.acquire(config.clone(), consumer.clone());
        let connection_id = node_handle.connection_id().to_string();
        let _ = self
            .ssh_registry
            .mark_state(&connection_id, ConnectionState::Connecting);
        self.node_router
            .bind_connection(node_id, connection_id.clone())
            .map_err(|error| {
                self.ssh_registry.release(&connection_id, &consumer);
                NodeTransportStartError::Route(error.to_string())
            })?;

        let registry = self.ssh_registry.clone();
        let router = self.node_router.clone();
        let reconnect_tx = self.reconnect_worker_tx.clone();
        let worker_job_id = self.reconnect_orchestrator.active_job_id(&node_id.0);
        let attempt_id = self.next_node_transport_attempt_id();
        let worker_node_id = node_id.clone();
        let worker_connection_id = connection_id.clone();
        let prompt_handler = Arc::new(NativeSshPromptHandler::new(self.ssh_worker_tx.clone()));
        let progress_tx = reconnect_tx.clone();
        let progress_node_id = worker_node_id.clone();
        // The node attempt owns progress delivery; terminal panes only observe the resulting trace.
        let connection_progress = ConnectionProgressReporter::new(move |stage| {
            let _ = progress_tx.send(ReconnectWorkerResult::NodeConnectionProgress {
                node_id: progress_node_id.clone(),
                stage,
                attempt_id,
            });
        });
        let task = self.task_runtime.spawn(async move {
            // This task owns the node transport independently from terminal panes and pages.
            if force_reconnect {
                node_handle.clear_physical().await;
            }
            let client = SshTransportClient::new(config)
                .with_prompt_handler(prompt_handler)
                .with_managed_key_resolver(managed_key_resolver)
                .with_connection_progress(connection_progress);
            let parent = if let Some(parent_id) = parent_id {
                let parent_consumer =
                    ConnectionConsumer::NodeRouter(format!("{}:ancestor", worker_node_id.0));
                match router
                    .acquire_connection_wait(
                        &parent_id,
                        parent_consumer.clone(),
                        Duration::from_secs(30),
                    )
                    .await
                {
                    Ok(parent) => Some((parent.handle, parent_consumer)),
                    Err(error) => {
                        registry.release(node_handle.connection_id(), &consumer);
                        let _ = registry.mark_state(
                            node_handle.connection_id(),
                            ConnectionState::Error(error.to_string()),
                        );
                        let _ = reconnect_tx.send(ReconnectWorkerResult::NodeConnectFailed {
                            node_id: worker_node_id,
                            connection_id: worker_connection_id,
                            error: error.to_string(),
                            attempt_id,
                            job_id: worker_job_id,
                        });
                        return;
                    }
                }
            } else {
                None
            };
            let result = if let Some((parent_handle, parent_consumer)) = parent {
                client
                    .connect_child_node_via_parent_with_registry(
                        registry,
                        consumer,
                        node_handle,
                        parent_handle,
                        parent_consumer,
                    )
                    .await
            } else {
                client
                    .connect_existing_node_with_registry(registry, consumer, node_handle)
                    .await
            }
            .map(|handle| handle.connection_id().to_string())
            .map_err(|error| error.to_string());
            let _ = match result {
                Ok(connection_id) => reconnect_tx.send(ReconnectWorkerResult::NodeConnected {
                    node_id: worker_node_id,
                    connection_id,
                    attempt_id,
                    job_id: worker_job_id,
                }),
                Err(error) => reconnect_tx.send(ReconnectWorkerResult::NodeConnectFailed {
                    node_id: worker_node_id,
                    connection_id: worker_connection_id,
                    error,
                    attempt_id,
                    job_id: worker_job_id,
                }),
            };
        });
        self.node_transport_attempts.insert(
            node_id.clone(),
            NodeTransportAttempt {
                id: attempt_id,
                connection_id,
                abort_handle: task.abort_handle(),
            },
        );
        Ok(())
    }

    pub(in crate::workspace) fn record_node_transport_start_failure(
        &mut self,
        node_id: &NodeId,
        detail: String,
        cx: &mut Context<Self>,
    ) {
        let detail = oxideterm_ai::sanitize_for_ai(&detail);
        if let Ok(event) = self.node_router.sync_node_readiness_event(
            node_id,
            NodeReadiness::Error,
            detail.clone(),
        ) {
            self.node_router.emitter().emit(event);
        }
        self.finish_connection_trace_failed(node_id, Some(detail), cx);
    }

    fn next_node_transport_attempt_id(&mut self) -> NodeTransportAttemptId {
        self.next_node_transport_attempt_id = self.next_node_transport_attempt_id.wrapping_add(1);
        NodeTransportAttemptId(self.next_node_transport_attempt_id)
    }

    fn invalidate_node_transport_attempt(
        &mut self,
        node_id: &NodeId,
    ) -> Option<NodeTransportAttempt> {
        let attempt = self.node_transport_attempts.remove(node_id)?;
        // Authentication and jump-host setup must not outlive their owning Entity attempt.
        attempt.abort_handle.abort();
        Some(attempt)
    }

    fn cancel_node_transport_attempt(&mut self, node_id: &NodeId) {
        let Some(attempt) = self.invalidate_node_transport_attempt(node_id) else {
            return;
        };
        self.retire_node_connection(node_id, &attempt.connection_id);
        if self.node_router.connection_id_for_node(node_id).as_deref()
            == Some(attempt.connection_id.as_str())
        {
            let _ = self.node_router.prepare_node_connection_attempt(node_id);
        }
        self.runtime_effects
            .retain(|effect| !runtime_effect_targets_node_transport(effect, node_id));
    }

    pub(in crate::workspace) fn node_transport_result_is_current(
        &self,
        node_id: &NodeId,
        attempt_id: NodeTransportAttemptId,
    ) -> bool {
        self.node_transport_attempts
            .get(node_id)
            .is_some_and(|attempt| attempt.id == attempt_id)
    }

    pub(in crate::workspace) fn complete_node_transport_attempt(
        &mut self,
        node_id: &NodeId,
        attempt_id: NodeTransportAttemptId,
    ) {
        if self.node_transport_result_is_current(node_id, attempt_id) {
            self.node_transport_attempts.remove(node_id);
        }
    }

    fn shutdown_node_transport_attempts(&mut self) {
        for (_, attempt) in self.node_transport_attempts.drain() {
            attempt.abort_handle.abort();
        }
        self.runtime_effects.retain(|effect| {
            !matches!(
                effect,
                WorkspaceRuntimeEffect::Reconnect(
                    ReconnectRuntimeEffect::NodeConnected { .. }
                        | ReconnectRuntimeEffect::NodeConnectFailed { .. }
                )
            )
        });
    }

    fn shutdown_workspace_runtime(&mut self) {
        if self.lifecycle != WorkspaceRuntimeLifecycle::Running {
            return;
        }
        self.lifecycle = WorkspaceRuntimeLifecycle::ShuttingDown;

        // Stop producers and invalidate deferred transitions before touching
        // any node or registry owner they could otherwise reacquire.
        self.shutdown_node_transport_attempts();
        for (_, (_, abort_handle)) in self.remote_shell_gate_tasks.drain() {
            abort_handle.abort();
        }
        if let Some((_, _, abort_handle)) = self.remote_shell_maintenance_task.take() {
            abort_handle.abort();
        }
        self.remote_shell_integration.cancel_terminal_gates();
        self.reconnect_debounce_generation = self.reconnect_debounce_generation.wrapping_add(1);
        // Runtime shutdown owns cancellation of the pending foreground debounce timer.
        self.reconnect_debounce_task = None;
        self.reconnect_cascade_generation = self.reconnect_cascade_generation.wrapping_add(1);
        self.active_probe_timer_generation = self.active_probe_timer_generation.wrapping_add(1);
        self.active_probe_timer_task = None;
        self.pending_reconnect_node_ids.clear();
        self.pending_reconnect_cascade_nodes.clear();
        self.reconnect_requeue_states.clear();
        self.reconnect_requeue_tasks.clear();
        self.reconnect_cascade_task = None;
        self.reconnect_schedule_tasks.clear();
        self.reconnect_pipeline_active_node = None;
        self.active_connection_chain = None;
        self.connecting_node_locks.clear();
        self.connection_trace_state = ConnectionTraceState::default();
        for cancellation in self
            .reconnect_forward_restore_tokens
            .drain()
            .map(|(_, value)| value)
        {
            cancellation.store(false, Ordering::Release);
        }
        for node_id in self.reconnect_orchestrator.active_node_ids() {
            let _ = self.reconnect_orchestrator.cancel(&node_id);
        }
        for (_, (_, abort_handle)) in self.reconnect_grace_probe_tasks.drain() {
            abort_handle.abort();
        }
        if let Some(abort_handle) = self.active_probe_task.take() {
            abort_handle.abort();
        }
        self.ssh_active_probe_in_flight = false;
        self.runtime_effects.clear();
        self.runtime_effect_delivery_pending = false;
        self.pending_ssh_terminal_opens.clear();
        self.terminal_endpoint_sessions.clear();
        self.terminal_ssh_nodes.clear();

        // Nodes are disconnected child-first so jump-host ancestors outlive
        // every dependent route. Registry entries are retired only afterward.
        let mut disconnected_node_ids = HashSet::new();
        for root_node_id in self.node_router.root_node_ids() {
            for node_id in self.node_router.subtree_postorder(&root_node_id) {
                if !disconnected_node_ids.insert(node_id.clone()) {
                    continue;
                }
                if let Some(connection_id) = self.node_router.connection_id_for_node(&node_id) {
                    self.retire_node_connection(&node_id, &connection_id);
                }
                let _ = self
                    .node_router
                    .disconnect_node_runtime(&node_id, "workspace shutdown");
            }
        }
        for connection in self.ssh_registry.list() {
            let connection_id = connection.connection_id;
            if let Some(handle) = self.ssh_registry.get(&connection_id) {
                self.task_runtime.spawn(async move {
                    handle.clear_physical().await;
                });
            }
            self.node_router.emitter().unregister(&connection_id);
            let _ = self
                .ssh_registry
                .mark_state_without_event(&connection_id, ConnectionState::Disconnected);
            let _ = self.ssh_registry.retire_connection(&connection_id);
        }

        self.lifecycle = WorkspaceRuntimeLifecycle::Stopped;
    }

    pub(in crate::workspace) fn configure_remote_shell_integration(
        &mut self,
        mode: RemoteShellIntegrationMode,
        awareness_enabled: bool,
    ) {
        self.remote_shell_integration
            .configure(mode, awareness_enabled);
        if mode == RemoteShellIntegrationMode::Disabled || !awareness_enabled {
            for (_, (_, abort_handle)) in self.remote_shell_gate_tasks.drain() {
                abort_handle.abort();
            }
            self.remote_shell_integration.cancel_terminal_gates();
        }
    }

    pub(in crate::workspace) fn remote_shell_integration_pending(&self) -> bool {
        self.remote_shell_integration.pending()
    }

    pub(in crate::workspace) fn remote_shell_integration_confirm_snapshot(
        &self,
    ) -> Option<settings::RemoteShellIntegrationConfirmSnapshot> {
        self.remote_shell_integration.confirm_snapshot()
    }

    pub(in crate::workspace) fn remote_shell_integration_confirm_open(&self) -> bool {
        self.remote_shell_integration.confirm_open()
    }

    pub(in crate::workspace) fn remote_shell_integration_card_snapshot(
        &self,
        node_id: Option<&NodeId>,
    ) -> settings::RemoteShellIntegrationCardSnapshot {
        self.remote_shell_integration.card_snapshot(node_id)
    }

    pub(in crate::workspace) fn open_remote_shell_integration_toolbar_confirm(
        &mut self,
        node_id: Option<NodeId>,
    ) {
        self.remote_shell_integration.open_toolbar_confirm(node_id);
    }

    pub(in crate::workspace) fn toggle_remote_shell_integration_prompt_suppression(&mut self) {
        self.remote_shell_integration.toggle_prompt_suppression();
    }

    pub(in crate::workspace) fn cancel_remote_shell_integration_confirm(&mut self) -> bool {
        self.remote_shell_integration.cancel_confirm()
    }

    pub(in crate::workspace) fn accept_remote_shell_integration_confirm(
        &mut self,
    ) -> Option<(NodeId, settings::RemoteShellIntegrationConfirmSource)> {
        self.remote_shell_integration.accept_confirm()
    }

    pub(in crate::workspace) fn start_remote_shell_integration_gate(
        &mut self,
        node_id: NodeId,
        force_install: bool,
    ) -> bool {
        let Some(generation) = self.remote_shell_integration.begin_terminal_gate(&node_id) else {
            return false;
        };
        let mode = self.remote_shell_integration.deployment_mode();
        let router = self.node_router.clone();
        let result_tx = self.reconnect_worker_tx.clone();
        let task_node_id = node_id.clone();
        let task = self.task_runtime.spawn(async move {
            // The node owns this capability check independently from the
            // terminal pane, matching the IDE Agent deployment lifecycle.
            let result = async {
                let resolved = router
                    .resolve_connection(&task_node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                // Detection starts only after the first visible Shell request,
                // preserving PAM, MOTD, and Last login output ordering.
                let mut remote_env = resolved.handle.remote_env();
                for _ in 0..80 {
                    if remote_env.is_some() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    remote_env = resolved.handle.remote_env();
                }
                let remote_env = remote_env.ok_or_else(|| {
                    "remote Shell detection did not finish after the visible terminal opened"
                        .to_string()
                })?;
                let sftp = router
                    .acquire_sftp(&task_node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                let sftp = sftp.lock().await;
                let status =
                    oxideterm_terminal::inspect_remote_shell_integration(&sftp, Some(&remote_env))
                        .await?;
                if should_install_remote_shell_integration(force_install, mode, status.state) {
                    oxideterm_terminal::install_remote_shell_integration(&sftp, Some(&remote_env))
                        .await
                        .map(|status| (status, true))
                } else {
                    Ok((status, false))
                }
            }
            .await
            // Delivery exposes only a typed failure category; backend details
            // never cross into UI state, notifications, or diagnostics.
            .map_err(|_| ());
            let _ = result_tx.send(ReconnectWorkerResult::RemoteShellIntegrationGateFinished {
                node_id: task_node_id,
                generation,
                result,
            });
        });
        self.remote_shell_gate_tasks
            .insert(node_id, (generation, task.abort_handle()));
        true
    }

    pub(in crate::workspace) fn start_remote_shell_integration_maintenance(
        &mut self,
        action: settings::RemoteShellIntegrationAction,
        node_id: NodeId,
    ) -> bool {
        let Some(generation) = self
            .remote_shell_integration
            .begin_maintenance(action, node_id.clone())
        else {
            return false;
        };
        if let Some((_, _, abort_handle)) = self.remote_shell_maintenance_task.take() {
            abort_handle.abort();
        }
        let router = self.node_router.clone();
        let result_tx = self.reconnect_worker_tx.clone();
        let task_node_id = node_id.clone();
        let task = self.task_runtime.spawn(async move {
            let result = async {
                let resolved = router
                    .resolve_connection(&task_node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                let remote_env = resolved.handle.remote_env();
                let sftp = router
                    .acquire_sftp(&task_node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                let sftp = sftp.lock().await;
                match action {
                    settings::RemoteShellIntegrationAction::Inspect => {
                        oxideterm_terminal::inspect_remote_shell_integration(
                            &sftp,
                            remote_env.as_ref(),
                        )
                        .await
                    }
                    settings::RemoteShellIntegrationAction::Install => {
                        oxideterm_terminal::install_remote_shell_integration(
                            &sftp,
                            remote_env.as_ref(),
                        )
                        .await
                    }
                    settings::RemoteShellIntegrationAction::RemoveReference => {
                        oxideterm_terminal::remove_remote_shell_integration(
                            &sftp,
                            remote_env.as_ref(),
                            false,
                        )
                        .await
                    }
                    settings::RemoteShellIntegrationAction::RemoveAll => {
                        oxideterm_terminal::remove_remote_shell_integration(
                            &sftp,
                            remote_env.as_ref(),
                            true,
                        )
                        .await
                    }
                }
            }
            .await
            // Maintenance failures follow the same content-free UI boundary.
            .map_err(|_| ());
            let _ = result_tx.send(
                ReconnectWorkerResult::RemoteShellIntegrationMaintenanceFinished {
                    action,
                    node_id: task_node_id,
                    generation,
                    result,
                },
            );
        });
        self.remote_shell_maintenance_task = Some((node_id, generation, task.abort_handle()));
        true
    }

    pub(in crate::workspace) fn start_reconnect_grace_probe(
        &mut self,
        request: ReconnectGraceProbeRequest,
    ) {
        self.cancel_reconnect_grace_probe(&request.node_id);
        let registry = self.ssh_registry.clone();
        let reconnect_tx = self.reconnect_worker_tx.clone();
        let timing = self.reconnect_orchestrator.timing();
        let task_node_id = request.node_id.clone();
        let task_job_id = request.job_id.clone();
        let task = self.task_runtime.spawn(async move {
            let ReconnectGraceProbeRequest {
                node_id,
                connection_id,
                affected_transfer_nodes,
                old_connections_by_node,
                old_connection_count,
                progress_store,
                job_id,
            } = request;
            let mut transfers_by_node = Vec::new();
            for (affected_node_id, old_connection_id) in affected_transfer_nodes {
                if let Ok(transfers) = progress_store.list_incomplete(&old_connection_id).await {
                    let transfer_ids = transfers
                        .into_iter()
                        .filter(StoredTransferProgress::is_incomplete)
                        .map(|transfer| transfer.transfer_id)
                        .collect::<Vec<_>>();
                    if !transfer_ids.is_empty() {
                        transfers_by_node.push(ReconnectNodeTransferSnapshot {
                            node_id: affected_node_id.0,
                            transfer_ids,
                        });
                    }
                }
            }
            let transfer_count = transfers_by_node
                .iter()
                .map(|entry| entry.transfer_ids.len())
                .sum::<usize>();
            let detail =
                format!("{transfer_count} transfer(s), {old_connection_count} connection(s)");
            let _ = reconnect_tx.send(ReconnectWorkerResult::SftpTransfersSnapshotted {
                node_id: node_id.clone(),
                transfers_by_node,
                detail,
                job_id: job_id.clone(),
            });

            let started_at = tokio::time::Instant::now();
            loop {
                match registry
                    .probe_single_connection(&connection_id, timing.proactive_keepalive_timeout)
                    .await
                {
                    ProbeConnectionStatus::Alive => {
                        let mut recovered_connections = Vec::new();
                        for old_connection in &old_connections_by_node {
                            if old_connection.node_id == node_id.0 {
                                continue;
                            }
                            if matches!(
                                registry
                                    .probe_single_connection(
                                        &old_connection.old_connection_id,
                                        timing.proactive_keepalive_timeout,
                                    )
                                    .await,
                                ProbeConnectionStatus::Alive
                            ) {
                                recovered_connections.push((
                                    NodeId::new(old_connection.node_id.clone()),
                                    old_connection.old_connection_id.clone(),
                                ));
                            }
                        }
                        let _ = reconnect_tx.send(ReconnectWorkerResult::GraceRecovered {
                            node_id,
                            connection_id,
                            recovered_connections,
                            job_id,
                        });
                        return;
                    }
                    ProbeConnectionStatus::NotFound => {
                        let detail =
                            format!("connection {connection_id} is unavailable for grace probe");
                        let _ = reconnect_tx.send(ReconnectWorkerResult::GraceExpired {
                            node_id,
                            connection_id,
                            detail,
                            job_id,
                        });
                        return;
                    }
                    ProbeConnectionStatus::Dead | ProbeConnectionStatus::NotApplicable => {
                        if started_at.elapsed() >= timing.grace_period {
                            let detail = format!(
                                "connection {connection_id} did not recover within {:?}",
                                timing.grace_period
                            );
                            let _ = reconnect_tx.send(ReconnectWorkerResult::GraceExpired {
                                node_id,
                                connection_id,
                                detail,
                                job_id,
                            });
                            return;
                        }
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                }
            }
        });
        // A reconnect probe can outlive its initiating UI action, so the
        // runtime owner retains cancellation until the terminal result arrives.
        self.reconnect_grace_probe_tasks
            .insert(task_node_id, (task_job_id, task.abort_handle()));
    }

    fn cancel_reconnect_grace_probe(&mut self, node_id: &NodeId) {
        if let Some((_, abort_handle)) = self.reconnect_grace_probe_tasks.remove(node_id) {
            abort_handle.abort();
        }
    }

    fn complete_reconnect_grace_probe(&mut self, node_id: &NodeId, job_id: &str) {
        if self
            .reconnect_grace_probe_tasks
            .get(node_id)
            .is_some_and(|(current_job_id, _)| current_job_id == job_id)
        {
            self.reconnect_grace_probe_tasks.remove(node_id);
        }
    }

    pub(in crate::workspace) fn apply_grace_recovery(
        &self,
        root_node_id: &NodeId,
        root_connection_id: &str,
        recovered_connections: Vec<(NodeId, String)>,
    ) -> Vec<NodeId> {
        let mut recovered_node_ids = Vec::new();
        for (node_id, connection_id) in
            std::iter::once((root_node_id.clone(), root_connection_id.to_string()))
                .chain(recovered_connections)
        {
            let connection_matches_node =
                self.node_router.connection_id_for_node(&node_id).as_deref()
                    == Some(connection_id.as_str());
            let has_physical_transport = self
                .ssh_registry
                .get(&connection_id)
                .is_some_and(|handle| handle.has_physical());
            if !connection_matches_node || !has_physical_transport {
                continue;
            }
            let Some(connection) = self
                .ssh_registry
                .mark_state_without_event(&connection_id, ConnectionState::Active)
            else {
                continue;
            };
            let _ =
                self.node_router
                    .sync_connection_state(&node_id, &connection, "grace recovered");
            if self
                .node_router
                .node_state(&node_id)
                .is_ok_and(|snapshot| snapshot.state.readiness == NodeReadiness::Ready)
            {
                recovered_node_ids.push(node_id);
            }
        }
        recovered_node_ids
    }

    pub(in crate::workspace) fn apply_grace_expiration(&self, connection_id: &str) {
        if let Some(connection) = self
            .ssh_registry
            .mark_state_without_event(connection_id, ConnectionState::LinkDown)
            && let Some(event) = self
                .node_router
                .sync_connection_state_by_connection_id(&connection, "grace expired")
        {
            self.node_router.emitter().emit(event);
        }
    }

    pub(in crate::workspace) fn cascade_connection_status_to_children(
        &self,
        root_node_id: &NodeId,
        affected_connection_ids: Option<&[String]>,
        state: NodeReadiness,
        reason: String,
    ) -> Vec<NodeId> {
        let connection_state = match state {
            NodeReadiness::Error => ConnectionState::LinkDown,
            NodeReadiness::Disconnected => ConnectionState::Disconnected,
            NodeReadiness::Ready | NodeReadiness::Connecting => return Vec::new(),
        };
        let affected_node_ids = self
            .node_router
            .connection_id_for_node(root_node_id)
            .map(|root_connection_id| {
                affected_connection_ids
                    .map(|connection_ids| connection_ids.to_vec())
                    .unwrap_or_else(|| {
                        self.ssh_registry
                            .descendant_connection_infos(&root_connection_id)
                            .into_iter()
                            .map(|info| info.connection_id)
                            .collect()
                    })
                    .into_iter()
                    .filter_map(|connection_id| {
                        self.node_router.node_id_for_connection(&connection_id)
                    })
                    .filter(|node_id| node_id != root_node_id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for affected_node_id in &affected_node_ids {
            let _ = self.node_router.sync_node_readiness_event(
                affected_node_id,
                state.clone(),
                reason.clone(),
            );
            if let Some(connection_id) = self.node_router.connection_id_for_node(affected_node_id) {
                let _ = self
                    .ssh_registry
                    .mark_state_without_event(&connection_id, connection_state.clone());
            }
        }
        affected_node_ids
    }

    pub(in crate::workspace) fn retire_stale_node_connection(
        &self,
        node_id: &NodeId,
        connection_id: &str,
    ) {
        // Stale completion cleanup must not disconnect a newer binding for the same node.
        if self
            .node_router
            .connection_id_for_node(node_id)
            .is_some_and(|current_id| current_id == connection_id)
        {
            return;
        }
        self.retire_node_connection(node_id, connection_id);
    }

    pub(in crate::workspace) fn reset_node_connection(&mut self, node_id: &NodeId) {
        self.cancel_node_transport_attempt(node_id);
        if let Some(connection_id) = self.node_router.connection_id_for_node(node_id) {
            self.retire_node_connection(node_id, &connection_id);
        }
        let _ = self.node_router.prepare_node_connection_attempt(node_id);
    }

    pub(in crate::workspace) fn remove_node_runtime_subtree(
        &mut self,
        cleanup_root: &NodeId,
        cx: &mut Context<Self>,
    ) -> Vec<NodeId> {
        let nodes_to_remove = self.runtime_subtree_postorder(cleanup_root);
        self.cancel_node_runtime_work(&nodes_to_remove, cx);
        for node_id in &nodes_to_remove {
            if let Some(connection_id) = self.node_router.connection_id_for_node(node_id) {
                self.retire_node_connection(node_id, &connection_id);
            }
        }
        self.node_router.remove_runtime_subtree(cleanup_root)
    }

    pub(in crate::workspace) fn disconnect_node_runtime_subtree(
        &mut self,
        root_node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> Vec<NodeId> {
        let nodes_to_disconnect = self.runtime_subtree_postorder(root_node_id);
        self.cancel_node_runtime_work(&nodes_to_disconnect, cx);
        for node_id in &nodes_to_disconnect {
            if let Some(connection_id) = self.node_router.connection_id_for_node(node_id) {
                self.retire_node_connection(node_id, &connection_id);
            }
            let _ = self
                .node_router
                .disconnect_node_runtime(node_id, "explicit disconnect");
        }
        nodes_to_disconnect
    }

    fn runtime_subtree_postorder(&self, root_node_id: &NodeId) -> Vec<NodeId> {
        let mut node_ids = self.node_router.subtree_postorder(root_node_id);
        if node_ids.is_empty() {
            node_ids.push(root_node_id.clone());
        }
        node_ids
    }

    fn cancel_node_runtime_work(&mut self, node_ids: &[NodeId], cx: &mut Context<Self>) {
        self.cancel_queued_reconnects(node_ids);
        for node_id in node_ids {
            if let Some((_, abort_handle)) = self.remote_shell_gate_tasks.remove(node_id) {
                abort_handle.abort();
            }
            if self
                .remote_shell_maintenance_task
                .as_ref()
                .is_some_and(|(current, _, _)| current == node_id)
                && let Some((_, _, abort_handle)) = self.remote_shell_maintenance_task.take()
            {
                abort_handle.abort();
            }
            self.remote_shell_integration.cancel_node(node_id);
            self.cancel_connection_trace(node_id, cx);
            self.abort_connection_chain_for_node(node_id);
            self.unlock_connecting_node(node_id);
            self.reconnect_orchestrator.cancel(&node_id.0);
        }
    }

    fn retire_node_connection(&self, node_id: &NodeId, connection_id: &str) {
        let consumer = ConnectionConsumer::NodeRouter(node_id.0.clone());
        self.ssh_registry.release(connection_id, &consumer);
        if let Some(handle) = self.ssh_registry.get(connection_id) {
            self.task_runtime.spawn(async move {
                handle.clear_physical().await;
            });
        }
        let _ = self
            .ssh_registry
            .mark_state_without_event(connection_id, ConnectionState::Disconnected);
        self.node_router.emitter().unregister(connection_id);
        let _ = self.ssh_registry.retire_connection(connection_id);
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

    pub(in crate::workspace) fn has_active_connection_chain(&self) -> bool {
        self.active_connection_chain.is_some()
    }

    pub(in crate::workspace) fn connection_chain_contains(&self, node_id: &NodeId) -> bool {
        self.active_connection_chain
            .as_ref()
            .is_some_and(|run| run.trace_plan.node_ids.contains(node_id))
    }

    pub(in crate::workspace) fn connection_chain_position(
        &self,
        node_id: &NodeId,
    ) -> Option<(usize, usize)> {
        let run = self.active_connection_chain.as_ref()?;
        let position = run
            .trace_plan
            .node_ids
            .iter()
            .position(|candidate| candidate == node_id)?;
        Some((position, run.trace_plan.node_ids.len()))
    }

    pub(in crate::workspace) fn any_connecting_node_is_locked(&self, node_ids: &[NodeId]) -> bool {
        node_ids
            .iter()
            .any(|node_id| self.connecting_node_locks.contains(node_id))
    }

    pub(in crate::workspace) fn try_lock_connecting_node(&mut self, node_id: &NodeId) -> bool {
        self.connecting_node_locks.insert(node_id.clone())
    }

    pub(in crate::workspace) fn unlock_connecting_node(&mut self, node_id: &NodeId) {
        self.connecting_node_locks.remove(node_id);
    }

    pub(in crate::workspace) fn try_begin_connection_chain(
        &mut self,
        trace_plan: ConnectionTracePlan,
    ) -> bool {
        if trace_plan.node_ids.is_empty()
            || self.active_connection_chain.is_some()
            || self.any_connecting_node_is_locked(&trace_plan.node_ids)
        {
            return false;
        }
        // The trace plan is shared shallowly across steps instead of cloning its node path.
        self.connecting_node_locks
            .extend(trace_plan.node_ids.iter().cloned());
        self.active_connection_chain = Some(ConnectionChainRun {
            next_index: 0,
            trace_plan: Arc::new(trace_plan),
        });
        true
    }

    pub(in crate::workspace) fn connection_chain_next_step(&self) -> Option<ConnectionChainStep> {
        let run = self.active_connection_chain.as_ref()?;
        let node_id = run.trace_plan.node_ids.get(run.next_index)?.clone();
        Some(ConnectionChainStep {
            node_id,
            trace_plan: Arc::clone(&run.trace_plan),
        })
    }

    pub(in crate::workspace) fn advance_connection_chain(
        &mut self,
        node_id: &NodeId,
    ) -> ConnectionChainAdvance {
        let Some(run) = self.active_connection_chain.as_mut() else {
            return ConnectionChainAdvance::Ignored;
        };
        if run.trace_plan.node_ids.get(run.next_index) != Some(node_id) {
            return ConnectionChainAdvance::Ignored;
        }
        run.next_index += 1;
        if run.next_index < run.trace_plan.node_ids.len() {
            return ConnectionChainAdvance::Continue;
        }
        self.release_connection_chain();
        ConnectionChainAdvance::Complete
    }

    pub(in crate::workspace) fn connection_chain_waits_after_node(&self, node_id: &NodeId) -> bool {
        self.active_connection_chain.as_ref().is_some_and(|run| {
            run.next_index > 0
                && run
                    .trace_plan
                    .node_ids
                    .get(run.next_index - 1)
                    .is_some_and(|current_id| current_id == node_id)
        })
    }

    pub(in crate::workspace) fn abort_connection_chain_for_node(
        &mut self,
        node_id: &NodeId,
    ) -> bool {
        if !self.connection_chain_contains(node_id) {
            return false;
        }
        self.release_connection_chain();
        true
    }

    fn release_connection_chain(&mut self) {
        if let Some(run) = self.active_connection_chain.take() {
            for node_id in &run.trace_plan.node_ids {
                self.connecting_node_locks.remove(node_id);
            }
        }
    }

    pub(in crate::workspace) fn new_connection_trace_plan(
        &mut self,
        mode: ConnectionTraceMode,
        node_ids: Vec<NodeId>,
    ) -> ConnectionTracePlan {
        ConnectionTracePlan {
            attempt_id: self.connection_trace_state.next_attempt_id(),
            mode,
            node_ids,
        }
    }

    pub(in crate::workspace) fn begin_connection_trace(
        &mut self,
        node_id: &NodeId,
        label: Option<String>,
        endpoint: Option<String>,
        plan: Option<&ConnectionTracePlan>,
        _parent_id: Option<&NodeId>,
        cx: &mut Context<Self>,
    ) {
        self.connection_trace_state
            .begin(node_id.clone(), label, endpoint, plan);
        self.push_connection_trace_event(
            node_id,
            ConnectionTraceStage::Queued,
            ConnectionTraceStatus::Running,
            5.0,
            None,
            cx,
        );
        self.push_connection_trace_event(
            node_id,
            ConnectionTraceStage::Preparing,
            ConnectionTraceStatus::Running,
            15.0,
            None,
            cx,
        );
    }

    pub(in crate::workspace) fn finish_connection_trace_success(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
        if !self.connection_trace_state.contains(node_id) {
            return;
        }
        self.push_connection_trace_event(
            node_id,
            ConnectionTraceStage::Ready,
            ConnectionTraceStatus::Ready,
            100.0,
            None,
            cx,
        );
        self.connection_trace_state.finish(node_id);
    }

    pub(in crate::workspace) fn finish_connection_trace_failed(
        &mut self,
        node_id: &NodeId,
        detail: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if !self.connection_trace_state.contains(node_id) {
            return;
        }
        let classified_stage = oxideterm_ssh::connection_trace_failure_stage(detail.as_deref());
        let stage = self
            .connection_trace_state
            .current_stage(node_id)
            .map(|current_stage| {
                if connection_trace_progress(current_stage)
                    >= connection_trace_progress(classified_stage)
                {
                    current_stage
                } else {
                    classified_stage
                }
            })
            .unwrap_or(classified_stage);
        self.push_connection_trace_event(
            node_id,
            stage,
            ConnectionTraceStatus::Failed,
            100.0,
            detail,
            cx,
        );
        self.connection_trace_state.finish(node_id);
    }

    pub(in crate::workspace) fn cancel_connection_trace(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) {
        if !self.connection_trace_state.contains(node_id) {
            return;
        }
        self.push_connection_trace_event(
            node_id,
            ConnectionTraceStage::Authentication,
            ConnectionTraceStatus::Cancelled,
            100.0,
            None,
            cx,
        );
        self.connection_trace_state.finish(node_id);
    }

    pub(in crate::workspace) fn take_runtime_effects(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<WorkspaceRuntimeEffect> {
        let budget = delivery::NOTIFICATION_DELIVERY_BUDGET;
        let started_at = Instant::now();
        let mut effects = Vec::new();
        while budget.allows_next(effects.len(), started_at.elapsed()) {
            let Some(effect) = self.runtime_effects.pop_front() else {
                break;
            };
            effects.push(effect);
        }
        if self.runtime_effects.is_empty() {
            self.runtime_effect_delivery_pending = false;
        } else {
            // Continue a bounded reliable drain without involving a root waiter or heartbeat.
            cx.emit(WorkspaceRuntimeEvent::EffectsReady);
        }
        effects
    }

    fn push_connection_trace_event(
        &mut self,
        node_id: &NodeId,
        stage: ConnectionTraceStage,
        status: ConnectionTraceStatus,
        progress: f32,
        detail: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(mut event) = self
            .connection_trace_state
            .event(node_id, stage, status, progress, detail)
        else {
            return;
        };
        // Trace details cross the runtime-to-UI boundary and must never preserve credentials.
        event.detail = event.detail.as_deref().map(oxideterm_ai::sanitize_for_ai);
        self.push_runtime_effect(WorkspaceRuntimeEffect::ConnectionTrace(event), cx);
    }

    #[cfg(test)]
    fn reconnect_worker_sender(&self) -> delivery::ActiveDeliverySender<ReconnectWorkerResult> {
        // Tests inject results through the same reliable wake path used by workers.
        self.reconnect_worker_tx.clone()
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
        // Retaining the latest debounce task cancels superseded timers and
        // prevents their async wake from escaping the runtime entity lifetime.
        self.reconnect_debounce_task = Some(cx.spawn(async move |entity, cx| {
            Timer::after(RECONNECT_DEBOUNCE_DELAY).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.flush_reconnect_roots(generation, cx);
            });
        }));
    }

    pub(in crate::workspace) fn cancel_queued_reconnects(&mut self, node_ids: &[NodeId]) {
        self.pending_reconnect_node_ids
            .retain(|node_id| !node_ids.contains(node_id));
        self.runtime_effects
            .retain(|effect| !runtime_effect_is_reconnect_schedule_for_nodes(effect, node_ids));
        self.cancel_reconnect_scheduler_nodes(node_ids);
        for node_id in node_ids {
            self.cancel_reconnect_grace_probe(node_id);
            self.cancel_node_transport_attempt(node_id);
            self.clear_reconnect_restore_state(node_id);
        }
    }

    pub(in crate::workspace) fn claim_reconnect_pipeline(
        &mut self,
        node_id: &NodeId,
        expected_connection_id: Option<String>,
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
                self.reconnect_requeue_tasks.remove(node_id);
                return ReconnectPipelineClaim::Exhausted;
            }
            let generation = requeue_state.generation;
            let retry_node_id = node_id.clone();
            let retry_delay = self.reconnect_orchestrator.retry_delay_for_attempt(1);
            let task = cx.spawn(async move |entity, cx| {
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
            });
            // Each node owns at most one delayed requeue; replacement cancels
            // the previous timer instead of leaving a stale async wake behind.
            self.reconnect_requeue_tasks.insert(node_id.clone(), task);
            return ReconnectPipelineClaim::Requeued;
        }

        self.reconnect_pipeline_active_node = Some(node_id.clone());
        self.reconnect_requeue_states.remove(node_id);
        self.reconnect_requeue_tasks.remove(node_id);
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
        self.reconnect_requeue_tasks.remove(node_id);
    }

    pub(in crate::workspace) fn cancel_reconnect_retry(&mut self, node_id: &NodeId) {
        self.reconnect_requeue_states.remove(node_id);
        self.reconnect_requeue_tasks.remove(node_id);
        self.cancel_node_transport_attempt(node_id);
    }

    pub(in crate::workspace) fn replace_reconnect_cascade(
        &mut self,
        node_ids: impl IntoIterator<Item = NodeId>,
    ) {
        self.pending_reconnect_cascade_nodes = node_ids.into_iter().collect();
        self.reconnect_cascade_generation = self.reconnect_cascade_generation.wrapping_add(1);
        self.reconnect_cascade_task = None;
    }

    pub(in crate::workspace) fn clear_reconnect_cascade(&mut self) {
        self.pending_reconnect_cascade_nodes.clear();
        // Invalidate a delayed continuation when its owning cascade is cleared.
        self.reconnect_cascade_generation = self.reconnect_cascade_generation.wrapping_add(1);
        self.reconnect_cascade_task = None;
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
        self.reconnect_cascade_task = Some(cx.spawn(async move |entity, cx| {
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
        }));
    }

    pub(in crate::workspace) fn take_next_reconnect_cascade_node(&mut self) -> Option<NodeId> {
        self.pending_reconnect_cascade_nodes.pop_front()
    }

    pub(in crate::workspace) fn schedule_reconnect_action(
        &mut self,
        action: ReconnectScheduleAction,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        self.reconnect_schedule_tasks
            .retain(|scheduled| !scheduled.task.is_ready());
        let node_id = reconnect_schedule_action_node_id(&action);
        let task = cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.push_reconnect_schedule_action(action, cx);
            });
        });
        // Delayed actions remain cancellable by node and runtime shutdown.
        self.reconnect_schedule_tasks
            .push(ScheduledReconnectTask { node_id, task });
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
        self.reconnect_requeue_tasks
            .retain(|node_id, _| !node_ids.contains(node_id));
        self.pending_reconnect_cascade_nodes
            .retain(|node_id| !node_ids.contains(node_id));
        self.reconnect_schedule_tasks.retain(|scheduled| {
            scheduled
                .node_id
                .as_ref()
                .is_none_or(|node_id| !node_ids.contains(node_id))
        });
        self.runtime_effects
            .retain(|effect| !runtime_effect_is_reconnect_schedule_for_nodes(effect, node_ids));
        if self
            .reconnect_pipeline_active_node
            .as_ref()
            .is_some_and(|active_node_id| node_ids.contains(active_node_id))
        {
            self.reconnect_pipeline_active_node = None;
        }
        self.reconnect_cascade_generation = self.reconnect_cascade_generation.wrapping_add(1);
        self.reconnect_cascade_task = None;
    }

    fn push_reconnect_schedule_action(
        &mut self,
        action: ReconnectScheduleAction,
        cx: &mut Context<Self>,
    ) {
        if let Some(effect) = self.reduce_reconnect_schedule_action(action) {
            self.push_runtime_effect(effect, cx);
        }
    }

    fn reduce_reconnect_schedule_action(
        &mut self,
        action: ReconnectScheduleAction,
    ) -> Option<WorkspaceRuntimeEffect> {
        match action {
            ReconnectScheduleAction::ContinueConnectionChain { node_id } => {
                Some(WorkspaceRuntimeEffect::ContinueConnectionChain { node_id })
            }
            ReconnectScheduleAction::ContinueReconnectCascade => {
                Some(WorkspaceRuntimeEffect::ContinueReconnectCascade)
            }
            ReconnectScheduleAction::StartReconnectPipeline {
                node_id,
                expected_connection_id,
            } => {
                if expected_connection_id.as_ref().is_some_and(|expected| {
                    self.node_router.connection_id_for_node(&node_id).as_ref() != Some(expected)
                }) {
                    self.cancel_reconnect_retry(&node_id);
                    None
                } else {
                    Some(WorkspaceRuntimeEffect::StartReconnectPipeline { node_id })
                }
            }
            ReconnectScheduleAction::RetryNodeConnect { node_id, job_id } => {
                if !self.reconnect_job_is_current(&node_id, &job_id) {
                    return None;
                }
                let (attempt, max_attempts) =
                    self.reconnect_orchestrator.active_attempt(&node_id.0)?;
                let still_needs_reconnect = self
                    .node_router
                    .node_state(&node_id)
                    .is_ok_and(|snapshot| snapshot.state.readiness != NodeReadiness::Ready);
                if !still_needs_reconnect {
                    let _ = self.reconnect_orchestrator.complete_phase(
                        &node_id.0,
                        PhaseResult::Ok,
                        Some("node recovered before retry".to_string()),
                    );
                    Some(WorkspaceRuntimeEffect::ReconnectRecoveredBeforeRetry { node_id })
                } else {
                    let detail = format!("starting retry {attempt}/{max_attempts}");
                    let _ = self.reconnect_orchestrator.complete_phase(
                        &node_id.0,
                        PhaseResult::Ok,
                        Some(detail),
                    );
                    let _ = self
                        .reconnect_orchestrator
                        .advance(&node_id.0, ReconnectPhase::SshConnect);
                    let _ = self.reconnect_orchestrator.begin_ssh_attempt(&node_id.0);
                    Some(WorkspaceRuntimeEffect::RetryNodeConnect {
                        node_id,
                        attempt,
                        max_attempts,
                    })
                }
            }
            ReconnectScheduleAction::CleanupReconnectJob {
                node_id,
                started_at,
            } => self
                .reconnect_orchestrator
                .cleanup_terminal_job(&node_id.0, started_at)
                .then_some(WorkspaceRuntimeEffect::ReconnectJobCleaned),
        }
    }

    fn push_runtime_effect(&mut self, effect: WorkspaceRuntimeEffect, cx: &mut Context<Self>) {
        self.runtime_effects.push_back(effect);
        if !self.runtime_effect_delivery_pending {
            self.runtime_effect_delivery_pending = true;
            cx.emit(WorkspaceRuntimeEvent::EffectsReady);
        }
    }

    fn schedule_worker_delivery(&self, cx: &mut Context<Self>) {
        let runtime_wake = self.reconnect_worker_tx.wake();
        let release_wake = runtime_wake.clone();
        cx.on_release(move |entity, _| {
            // Entity release is the explicit Workspace owner boundary. Tabs,
            // pages, and detached windows never execute this shutdown path.
            entity.shutdown_workspace_runtime();
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
        // Only the runtime entity owns the keepalive timer. Replacing or
        // releasing the entity must cancel the previous async wake.
        self.active_probe_timer_task = Some(cx.spawn(async move |entity, cx| {
            Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                if entity.active_probe_timer_generation == generation {
                    entity.active_probe_timer_task = None;
                    entity.start_active_ssh_probe(cx);
                }
            });
        }));
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
        let task = self.task_runtime.spawn(async move {
            let changed = ssh_registry.probe_active_connections(timeout).await.len();
            let _ = result_sender.send(changed);
        });
        // Workspace shutdown aborts the in-flight probe before retiring its registry.
        self.active_probe_task = Some(task.abort_handle());
    }

    fn drain_worker_results(&mut self, cx: &mut Context<Self>) -> bool {
        let reconnect_batch = delivery::drain_channel(
            &self.reconnect_worker_rx,
            delivery::LIFECYCLE_DELIVERY_BUDGET,
        );
        let active_probe_batch =
            delivery::drain_channel(&self.active_probe_rx, delivery::LIFECYCLE_DELIVERY_BUDGET);
        let (node_event_items, node_event_backlog_remaining) =
            drain_node_event_mailbox(&self.node_event_rx);
        for result in reconnect_batch.items {
            if let Some(effect) = self.reduce_reconnect_worker_result(result, cx) {
                let connected_node_id = match &effect {
                    ReconnectRuntimeEffect::NodeConnected { node_id, .. } => Some(node_id.clone()),
                    _ => None,
                };
                self.push_runtime_effect(WorkspaceRuntimeEffect::Reconnect(effect), cx);
                if let Some(node_id) = connected_node_id {
                    self.publish_ssh_terminal_opens_for_connected_node(&node_id, cx);
                }
            }
        }
        for event in node_event_items {
            if let Some(effect) = self.reduce_node_event(event) {
                self.push_runtime_effect(WorkspaceRuntimeEffect::Node(effect), cx);
            }
        }
        let active_probe_completed = !active_probe_batch.items.is_empty();
        let active_connections_changed = active_probe_batch
            .items
            .into_iter()
            .any(|changed| changed > 0);
        if active_probe_completed {
            self.ssh_active_probe_in_flight = false;
            self.active_probe_task = None;
            self.schedule_active_probe_after(self.reconnect_timing.ssh_keepalive_interval, cx);
        }
        if active_connections_changed {
            self.push_runtime_effect(WorkspaceRuntimeEffect::ActiveConnectionsChanged, cx);
        }
        reconnect_batch.outcome.backlog_remaining
            || active_probe_batch.outcome.backlog_remaining
            || node_event_backlog_remaining
    }

    fn reduce_reconnect_worker_result(
        &mut self,
        result: ReconnectWorkerResult,
        cx: &mut Context<Self>,
    ) -> Option<ReconnectRuntimeEffect> {
        match result {
            ReconnectWorkerResult::NodeConnectionProgress {
                node_id,
                stage,
                attempt_id,
            } => {
                if self.node_transport_result_is_current(&node_id, attempt_id) {
                    self.push_connection_trace_event(
                        &node_id,
                        stage,
                        ConnectionTraceStatus::Running,
                        connection_trace_progress(stage),
                        None,
                        cx,
                    );
                }
                None
            }
            ReconnectWorkerResult::NodeConnected {
                node_id,
                connection_id,
                attempt_id,
                job_id,
            } => {
                if !self.accept_node_transport_result(
                    &node_id,
                    &connection_id,
                    attempt_id,
                    job_id.as_deref(),
                ) {
                    return None;
                }
                self.complete_node_transport_attempt(&node_id, attempt_id);
                self.finish_connection_trace_success(&node_id, cx);
                let reconnecting = self.reconnect_orchestrator.is_active(&node_id.0);
                if reconnecting {
                    let _ = self.reconnect_orchestrator.complete_phase(
                        &node_id.0,
                        PhaseResult::Ok,
                        Some(format!("reconnected as {connection_id}")),
                    );
                    let _ = self
                        .reconnect_orchestrator
                        .advance(&node_id.0, ReconnectPhase::AwaitTerminal);
                }
                Some(ReconnectRuntimeEffect::NodeConnected {
                    node_id,
                    connection_id,
                    reconnecting,
                })
            }
            ReconnectWorkerResult::NodeConnectFailed {
                node_id,
                connection_id,
                error,
                attempt_id,
                job_id,
            } => {
                if !self.accept_node_transport_result(
                    &node_id,
                    &connection_id,
                    attempt_id,
                    job_id.as_deref(),
                ) {
                    return None;
                }
                self.complete_node_transport_attempt(&node_id, attempt_id);
                // Worker errors cross a UI and notification boundary, so redact before queuing.
                let error = oxideterm_ai::sanitize_for_ai(&error);
                self.record_node_transport_start_failure(&node_id, error.clone(), cx);
                let action = if self.reconnect_orchestrator.is_active(&node_id.0) {
                    let _ = self.reconnect_orchestrator.complete_phase(
                        &node_id.0,
                        PhaseResult::Failed,
                        Some(error.clone()),
                    );
                    if !reconnect_error_is_non_retryable(&error) {
                        self.reconnect_orchestrator
                            .schedule_retry(&node_id.0)
                            .and_then(|retry| {
                                job_id
                                    .or_else(|| {
                                        self.reconnect_orchestrator.active_job_id(&node_id.0)
                                    })
                                    .map(|job_id| ReconnectFailureAction::Retry {
                                        attempt: retry.attempt,
                                        max_attempts: retry.max_attempts,
                                        delay: retry.delay,
                                        job_id,
                                    })
                            })
                            .unwrap_or(ReconnectFailureAction::FinishReconnect)
                    } else {
                        ReconnectFailureAction::FinishReconnect
                    }
                } else {
                    ReconnectFailureAction::InitialConnect
                };
                Some(ReconnectRuntimeEffect::NodeConnectFailed {
                    node_id,
                    error,
                    action,
                })
            }
            ReconnectWorkerResult::GraceRecovered {
                node_id,
                connection_id,
                recovered_connections,
                job_id,
            } => {
                self.complete_reconnect_grace_probe(&node_id, &job_id);
                self.reconnect_job_is_current(&node_id, &job_id).then(|| {
                    let _ = self.reconnect_orchestrator.complete_phase(
                        &node_id.0,
                        PhaseResult::Ok,
                        Some(format!(
                            "connection {connection_id} recovered during grace period"
                        )),
                    );
                    ReconnectRuntimeEffect::GraceRecovered {
                        node_id,
                        connection_id,
                        recovered_connections,
                    }
                })
            }
            ReconnectWorkerResult::GraceExpired {
                node_id,
                connection_id,
                detail,
                job_id,
            } => {
                self.complete_reconnect_grace_probe(&node_id, &job_id);
                self.reconnect_job_is_current(&node_id, &job_id).then(|| {
                    let detail = oxideterm_ai::sanitize_for_ai(&detail);
                    let _ = self.reconnect_orchestrator.complete_phase(
                        &node_id.0,
                        PhaseResult::Failed,
                        Some(detail.clone()),
                    );
                    self.apply_grace_expiration(&connection_id);
                    let _ = self
                        .reconnect_orchestrator
                        .advance(&node_id.0, ReconnectPhase::SshConnect);
                    let _ = self.reconnect_orchestrator.begin_ssh_attempt(&node_id.0);
                    ReconnectRuntimeEffect::GraceExpired { node_id, detail }
                })
            }
            ReconnectWorkerResult::SftpTransfersSnapshotted {
                node_id,
                transfers_by_node,
                detail,
                job_id,
            } => self.reconnect_job_is_current(&node_id, &job_id).then(|| {
                let entered_grace_period = self.reconnect_orchestrator.is_active(&node_id.0);
                let _ = self
                    .reconnect_orchestrator
                    .update_snapshot(&node_id.0, |snapshot| {
                        snapshot.inflight_sftp_transfer_ids = transfers_by_node
                            .iter()
                            .flat_map(|entry| entry.transfer_ids.iter().cloned())
                            .collect();
                        snapshot.incomplete_sftp_transfers_by_node = transfers_by_node;
                    });
                if entered_grace_period {
                    let _ = self.reconnect_orchestrator.complete_phase(
                        &node_id.0,
                        PhaseResult::Ok,
                        Some(oxideterm_ai::sanitize_for_ai(&detail)),
                    );
                    let _ = self
                        .reconnect_orchestrator
                        .advance(&node_id.0, ReconnectPhase::GracePeriod);
                }
                ReconnectRuntimeEffect::SftpTransfersSnapshotted {
                    node_id,
                    entered_grace_period,
                }
            }),
            ReconnectWorkerResult::RemoteShellIntegrationGateFinished {
                node_id,
                generation,
                result,
            } => {
                if self
                    .remote_shell_gate_tasks
                    .get(&node_id)
                    .is_some_and(|(current, _)| *current == generation)
                {
                    self.remote_shell_gate_tasks.remove(&node_id);
                }
                let outcome = self
                    .remote_shell_integration
                    .finish_terminal_gate(node_id, generation, result);
                match outcome {
                    settings::RemoteShellIntegrationGateOutcome::Applied => {
                        Some(ReconnectRuntimeEffect::RemoteShellIntegrationGateFinished {
                            notice: None,
                        })
                    }
                    settings::RemoteShellIntegrationGateOutcome::RetryInstall(node_id) => {
                        self.start_remote_shell_integration_gate(node_id, true);
                        Some(ReconnectRuntimeEffect::RemoteShellIntegrationGateFinished {
                            notice: None,
                        })
                    }
                    settings::RemoteShellIntegrationGateOutcome::Failed => {
                        Some(ReconnectRuntimeEffect::RemoteShellIntegrationGateFinished {
                            notice: Some(settings::RemoteShellIntegrationNotice::Failed),
                        })
                    }
                    settings::RemoteShellIntegrationGateOutcome::Stale => None,
                }
            }
            ReconnectWorkerResult::RemoteShellIntegrationMaintenanceFinished {
                action,
                node_id,
                generation,
                result,
            } => {
                if self
                    .remote_shell_maintenance_task
                    .as_ref()
                    .is_some_and(|(_, current, _)| *current == generation)
                {
                    self.remote_shell_maintenance_task = None;
                }
                self.remote_shell_integration
                    .finish_maintenance(action, node_id, generation, result)
                    .map(|notice| {
                        ReconnectRuntimeEffect::RemoteShellIntegrationMaintenanceFinished { notice }
                    })
            }
        }
    }

    fn accept_node_transport_result(
        &mut self,
        node_id: &NodeId,
        connection_id: &str,
        attempt_id: NodeTransportAttemptId,
        job_id: Option<&str>,
    ) -> bool {
        let attempt_is_current = self.node_transport_result_is_current(node_id, attempt_id);
        let job_is_current =
            job_id.is_none_or(|job_id| self.reconnect_job_is_current(node_id, job_id));
        if attempt_is_current && job_is_current {
            return true;
        }

        if attempt_is_current {
            // A cancelled reconnect job invalidates the attempt even if its result won the race.
            self.cancel_node_transport_attempt(node_id);
        } else {
            self.retire_stale_node_connection(node_id, connection_id);
        }
        false
    }

    fn reduce_node_event(&mut self, event: NodeStateEvent) -> Option<NodeRuntimeEffect> {
        let Some((node_id, generation)) = node_event_generation(&event) else {
            return match event {
                NodeStateEvent::ConnectionStatusChanged {
                    connection_id,
                    status,
                    affected_children,
                    ..
                } => {
                    let node_id = self.node_router.node_id_for_connection(&connection_id)?;
                    let event_state = readiness_for_runtime_connection_status(&status)?;
                    let state = self
                        .node_router
                        .node_state(&node_id)
                        .map(|snapshot| snapshot.state.readiness)
                        .unwrap_or(event_state);
                    let reason = reason_for_runtime_connection_status(&status);
                    let _ = self.node_router.sync_node_readiness_event(
                        &node_id,
                        state.clone(),
                        reason.clone(),
                    );
                    Some(NodeRuntimeEffect::ConnectionStatusChanged {
                        node_id,
                        connection_id,
                        status,
                        state,
                        reason,
                        affected_children,
                    })
                }
                _ => None,
            };
        };
        if self
            .node_event_generations
            .get(&node_id)
            .is_some_and(|seen| generation <= *seen)
        {
            return None;
        }
        self.node_event_generations.insert(node_id, generation);
        match event {
            NodeStateEvent::ConnectionStatusChanged { .. } => None,
            NodeStateEvent::ConnectionStateChanged {
                node_id,
                state,
                reason,
                ..
            } => {
                let reason = oxideterm_ai::sanitize_for_ai(&reason);
                let node_id = NodeId::new(node_id);
                let _ = self.node_router.sync_node_readiness_event(
                    &node_id,
                    state.clone(),
                    reason.clone(),
                );
                Some(NodeRuntimeEffect::ConnectionStateChanged {
                    node_id: node_id.0,
                    state,
                    reason,
                })
            }
            NodeStateEvent::SftpReady {
                node_id,
                ready,
                cwd,
                ..
            } => Some(NodeRuntimeEffect::SftpReady {
                node_id,
                ready,
                cwd,
            }),
            NodeStateEvent::SharedSftpSessionChanged {
                node_id,
                connection_id,
                session_generation,
                ready,
                ..
            } => Some(NodeRuntimeEffect::SharedSftpSessionChanged {
                node_id,
                connection_id,
                session_generation,
                ready,
            }),
            NodeStateEvent::TerminalEndpointChanged { .. } => {
                Some(NodeRuntimeEffect::TerminalEndpointChanged)
            }
        }
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
        let roots = self.node_router.minimal_subtree_roots(pending);
        if roots.is_empty() {
            return;
        }
        for node_id in roots {
            self.push_runtime_effect(WorkspaceRuntimeEffect::StartReconnectRoot { node_id }, cx);
        }
    }
}

fn readiness_for_runtime_connection_status(status: &str) -> Option<NodeReadiness> {
    match status {
        "connected" => Some(NodeReadiness::Ready),
        "link_down" => Some(NodeReadiness::Error),
        "reconnecting" => Some(NodeReadiness::Connecting),
        "disconnected" => Some(NodeReadiness::Disconnected),
        _ => None,
    }
}

fn reason_for_runtime_connection_status(status: &str) -> String {
    match status {
        "connected" => "connection restored",
        "link_down" => "link down",
        "reconnecting" => "reconnecting",
        "disconnected" => "connection disconnected",
        _ => "connection status changed",
    }
    .to_string()
}

fn should_install_remote_shell_integration(
    force_install: bool,
    mode: RemoteShellIntegrationMode,
    state: oxideterm_terminal::RemoteShellIntegrationState,
) -> bool {
    force_install
        || (mode == RemoteShellIntegrationMode::Enabled
            && state != oxideterm_terminal::RemoteShellIntegrationState::Installed)
}

fn reconnect_error_is_non_retryable(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    [
        "authentication failed",
        "hostkeymismatch",
        "host key",
        "permission denied",
        "user_cancelled",
        "cancelled",
    ]
    .iter()
    .any(|needle| error.contains(needle))
}

fn connection_trace_progress(stage: ConnectionTraceStage) -> f32 {
    match stage {
        ConnectionTraceStage::Queued => 5.0,
        ConnectionTraceStage::Preparing => 15.0,
        ConnectionTraceStage::OpeningTransport => 30.0,
        ConnectionTraceStage::SshHandshake => 50.0,
        ConnectionTraceStage::HostKey => 65.0,
        ConnectionTraceStage::Authentication => 72.0,
        ConnectionTraceStage::KerberosCredentials => 76.0,
        ConnectionTraceStage::GssapiExchange => 80.0,
        ConnectionTraceStage::FallbackAuthentication => 84.0,
        ConnectionTraceStage::Pty => 90.0,
        ConnectionTraceStage::ShellReady => 96.0,
        ConnectionTraceStage::Ready => 100.0,
    }
}

fn runtime_effect_targets_node_transport(
    effect: &WorkspaceRuntimeEffect,
    target_node_id: &NodeId,
) -> bool {
    matches!(
        effect,
        WorkspaceRuntimeEffect::Reconnect(
            ReconnectRuntimeEffect::NodeConnected { node_id, .. }
                | ReconnectRuntimeEffect::NodeConnectFailed { node_id, .. }
        ) if node_id == target_node_id
    )
}

fn runtime_effect_is_reconnect_schedule_for_nodes(
    effect: &WorkspaceRuntimeEffect,
    node_ids: &[NodeId],
) -> bool {
    match effect {
        WorkspaceRuntimeEffect::StartReconnectRoot { node_id }
        | WorkspaceRuntimeEffect::ContinueConnectionChain { node_id }
        | WorkspaceRuntimeEffect::StartReconnectPipeline { node_id }
        | WorkspaceRuntimeEffect::RetryNodeConnect { node_id, .. }
        | WorkspaceRuntimeEffect::ReconnectRecoveredBeforeRetry { node_id } => {
            node_ids.contains(node_id)
        }
        _ => false,
    }
}

fn reconnect_schedule_action_node_id(action: &ReconnectScheduleAction) -> Option<NodeId> {
    match action {
        ReconnectScheduleAction::ContinueConnectionChain { node_id }
        | ReconnectScheduleAction::StartReconnectPipeline { node_id, .. }
        | ReconnectScheduleAction::RetryNodeConnect { node_id, .. }
        | ReconnectScheduleAction::CleanupReconnectJob { node_id, .. } => Some(node_id.clone()),
        ReconnectScheduleAction::ContinueReconnectCascade => None,
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
        | NodeStateEvent::SharedSftpSessionChanged {
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use oxideterm_ssh::NodeEventEmitter;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn remote_shell_gate_installs_only_when_requested_or_enabled_and_missing() {
        use oxideterm_terminal::RemoteShellIntegrationState;

        assert!(should_install_remote_shell_integration(
            true,
            RemoteShellIntegrationMode::Ask,
            RemoteShellIntegrationState::Installed,
        ));
        assert!(should_install_remote_shell_integration(
            false,
            RemoteShellIntegrationMode::Enabled,
            RemoteShellIntegrationState::NotInstalled,
        ));
        assert!(!should_install_remote_shell_integration(
            false,
            RemoteShellIntegrationMode::Enabled,
            RemoteShellIntegrationState::Installed,
        ));
        assert!(!should_install_remote_shell_integration(
            false,
            RemoteShellIntegrationMode::Ask,
            RemoteShellIntegrationState::NotInstalled,
        ));
        assert!(!should_install_remote_shell_integration(
            false,
            RemoteShellIntegrationMode::Disabled,
            RemoteShellIntegrationState::NotInstalled,
        ));
    }

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
        let node_router = NodeRouter::new(ssh_registry.clone());
        let task_runtime = test_task_runtime();
        cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry,
                node_router,
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        })
    }

    fn unavailable_managed_key_resolver() -> ManagedKeyResolver {
        Arc::new(|_| {
            Err(oxideterm_ssh::SshTransportError::AuthenticationFailed(
                "managed key unavailable in test".to_string(),
            ))
        })
    }

    fn take_trace_effects(
        entity: &mut WorkspaceRuntimeEntity,
        cx: &mut Context<WorkspaceRuntimeEntity>,
    ) -> Vec<ConnectionTraceEvent> {
        entity
            .take_runtime_effects(cx)
            .into_iter()
            .filter_map(|effect| match effect {
                WorkspaceRuntimeEffect::ConnectionTrace(event) => Some(event),
                _ => None,
            })
            .collect()
    }

    fn take_reconnect_effects(
        entity: &mut WorkspaceRuntimeEntity,
        cx: &mut Context<WorkspaceRuntimeEntity>,
    ) -> Vec<ReconnectRuntimeEffect> {
        entity
            .take_runtime_effects(cx)
            .into_iter()
            .filter_map(|effect| match effect {
                WorkspaceRuntimeEffect::Reconnect(effect) => Some(effect),
                _ => None,
            })
            .collect()
    }

    fn register_test_node_transport_attempt(
        entity: &mut WorkspaceRuntimeEntity,
        node_id: &NodeId,
        connection_id: &str,
    ) -> NodeTransportAttemptId {
        let attempt_id = entity.next_node_transport_attempt_id();
        // A pending task models authentication work that remains cancellable.
        let task = entity.task_runtime.spawn(std::future::pending::<()>());
        entity.node_transport_attempts.insert(
            node_id.clone(),
            NodeTransportAttempt {
                id: attempt_id,
                connection_id: connection_id.to_string(),
                abort_handle: task.abort_handle(),
            },
        );
        attempt_id
    }

    #[gpui::test]
    fn reconnect_worker_results_and_release_lifecycle_are_entity_owned(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let job_id = entity.update(cx, |entity, _cx| {
            entity
                .start_reconnect_job(
                    &NodeId::new("node-a"),
                    "Node A".to_string(),
                    ReconnectSnapshot::default(),
                )
                .job_id
        });
        let (reconnect_tx, wake) = cx.read(|cx| {
            let entity = entity.read(cx);
            (
                entity.reconnect_worker_sender(),
                entity.reconnect_worker_tx.wake(),
            )
        });
        reconnect_tx
            .send(ReconnectWorkerResult::GraceExpired {
                node_id: NodeId::new("node-a"),
                connection_id: "connection-a".to_string(),
                detail: "test timeout".to_string(),
                job_id,
            })
            .expect("reconnect worker delivery");

        cx.run_until_parked();

        entity.update(cx, |entity, cx| {
            let reconnect_results = take_reconnect_effects(entity, cx);
            assert!(matches!(
                reconnect_results.first(),
                Some(ReconnectRuntimeEffect::GraceExpired { .. })
            ));
        });

        drop(entity);
        cx.update(|_cx| {});
        assert!(wake.is_stopped());
    }

    #[gpui::test]
    fn runtime_effect_delivery_is_exact_once_across_a_bounded_backlog(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let ready_events = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let ready_event_count = Arc::clone(&ready_events);
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &WorkspaceRuntimeEvent, _cx| {
                if *event == WorkspaceRuntimeEvent::EffectsReady {
                    ready_event_count.fetch_add(1, Ordering::AcqRel);
                }
            })
        });

        entity.update(cx, |entity, cx| {
            for sequence in 0..65 {
                entity.push_runtime_effect(
                    WorkspaceRuntimeEffect::ContinueConnectionChain {
                        node_id: NodeId::new(format!("node-{sequence}")),
                    },
                    cx,
                );
            }
        });
        assert_eq!(ready_events.load(Ordering::Acquire), 1);

        let first_batch = entity.update(cx, |entity, cx| entity.take_runtime_effects(cx));
        assert_eq!(first_batch.len(), 64);
        assert_eq!(ready_events.load(Ordering::Acquire), 2);
        let second_batch = entity.update(cx, |entity, cx| entity.take_runtime_effects(cx));
        assert_eq!(first_batch.len() + second_batch.len(), 65);
        assert!(!entity.read_with(cx, |entity, _cx| { entity.runtime_effect_delivery_pending }));
    }

    #[gpui::test]
    fn reconnect_failure_is_redacted_before_it_becomes_a_typed_effect(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        entity.update(cx, |entity, cx| {
            let node_id = NodeId::new("node-a");
            let connection_id = "connection-a";
            let attempt_id = register_test_node_transport_attempt(entity, &node_id, connection_id);
            let effect = entity
                .reduce_reconnect_worker_result(
                    ReconnectWorkerResult::NodeConnectFailed {
                        node_id,
                        connection_id: connection_id.to_string(),
                        error: "password=super-secret-value".to_string(),
                        attempt_id,
                        job_id: None,
                    },
                    cx,
                )
                .expect("current worker result");

            assert!(matches!(
                effect,
                ReconnectRuntimeEffect::NodeConnectFailed { error, .. }
                    if !error.contains("super-secret-value") && error.contains("[REDACTED]")
            ));
        });
    }

    #[gpui::test]
    fn active_probe_completion_stays_inside_runtime_entity(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let changed_event_seen = Arc::new(AtomicBool::new(false));
        let changed_event_flag = changed_event_seen.clone();
        let _event_subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &WorkspaceRuntimeEvent, _cx| {
                if *event == WorkspaceRuntimeEvent::EffectsReady {
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

        entity.update(cx, |entity, cx| {
            let reconnect_results = entity.take_runtime_effects(cx);
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
            let job = entity.start_reconnect_job(
                &node_id,
                "Node A".to_string(),
                ReconnectSnapshot::default(),
            );

            assert_eq!(job.max_attempts, 4);
            assert!(entity.has_active_reconnect_job(&node_id));
            assert!(entity.reconnect_job_is_current(&node_id, &job.job_id));
            assert_eq!(entity.active_reconnect_node_ids(), vec![node_id.clone()]);

            entity.finish_reconnect_job_state(&node_id, Ok(0), None);
            assert!(!entity.has_active_reconnect_job(&node_id));
        });
    }

    #[gpui::test]
    fn connection_chain_state_and_locks_are_entity_owned(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let parent_node_id = NodeId::new("node-parent");
        let child_node_id = NodeId::new("node-child");

        entity.update(cx, |entity, _cx| {
            assert!(entity.try_lock_connecting_node(&parent_node_id));
            assert!(!entity.try_lock_connecting_node(&parent_node_id));
            entity.unlock_connecting_node(&parent_node_id);

            let trace_plan = ConnectionTracePlan {
                attempt_id: "attempt-a".to_string(),
                mode: ConnectionTraceMode::Connect,
                node_ids: vec![parent_node_id.clone(), child_node_id.clone()],
            };
            assert!(entity.try_begin_connection_chain(trace_plan));
            assert!(entity.has_active_connection_chain());
            assert!(entity.connection_chain_contains(&parent_node_id));
            assert_eq!(
                entity.connection_chain_position(&child_node_id),
                Some((1, 2))
            );
            assert!(
                entity.any_connecting_node_is_locked(&[
                    parent_node_id.clone(),
                    child_node_id.clone()
                ])
            );

            let first_step = entity.connection_chain_next_step().unwrap();
            assert_eq!(first_step.node_id, parent_node_id);
            assert_eq!(first_step.trace_plan.attempt_id, "attempt-a");
            assert_eq!(
                entity.advance_connection_chain(&child_node_id),
                ConnectionChainAdvance::Ignored
            );
            assert_eq!(
                entity.advance_connection_chain(&parent_node_id),
                ConnectionChainAdvance::Continue
            );
            assert!(entity.connection_chain_waits_after_node(&parent_node_id));
            assert_eq!(
                entity.connection_chain_next_step().unwrap().node_id,
                child_node_id
            );
            assert_eq!(
                entity.advance_connection_chain(&child_node_id),
                ConnectionChainAdvance::Complete
            );
            assert!(!entity.has_active_connection_chain());

            // Completing the chain releases every lock for future user or reconnect actions.
            assert!(entity.try_lock_connecting_node(&parent_node_id));
            entity.unlock_connecting_node(&parent_node_id);
            assert!(entity.try_lock_connecting_node(&child_node_id));
            entity.unlock_connecting_node(&child_node_id);
        });
    }

    #[gpui::test]
    fn aborting_connection_chain_releases_only_the_owned_chain(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let chain_node_id = NodeId::new("node-chain");
        let unrelated_node_id = NodeId::new("node-unrelated");

        entity.update(cx, |entity, _cx| {
            assert!(entity.try_lock_connecting_node(&unrelated_node_id));
            assert!(entity.try_begin_connection_chain(ConnectionTracePlan {
                attempt_id: "attempt-a".to_string(),
                mode: ConnectionTraceMode::Reconnect,
                node_ids: vec![chain_node_id.clone()],
            }));
            assert!(!entity.abort_connection_chain_for_node(&unrelated_node_id));
            assert!(entity.connection_chain_contains(&chain_node_id));
            assert!(entity.abort_connection_chain_for_node(&chain_node_id));
            assert!(!entity.has_active_connection_chain());
            assert!(entity.try_lock_connecting_node(&chain_node_id));
            assert!(!entity.try_lock_connecting_node(&unrelated_node_id));
        });
    }

    #[gpui::test]
    fn connection_trace_transitions_and_delivery_are_entity_owned(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let node_id = NodeId::new("node-a");
        let ready_events = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let ready_event_count = Arc::clone(&ready_events);
        let _subscription = entity.update(cx, |_, cx| {
            cx.subscribe(&entity, move |_, _, event: &WorkspaceRuntimeEvent, _cx| {
                if *event == WorkspaceRuntimeEvent::EffectsReady {
                    ready_event_count.fetch_add(1, Ordering::AcqRel);
                }
            })
        });

        entity.update(cx, |entity, cx| {
            let plan = entity
                .new_connection_trace_plan(ConnectionTraceMode::Connect, vec![node_id.clone()]);
            entity.begin_connection_trace(
                &node_id,
                Some("Node A".to_string()),
                Some("user@node-a:22".to_string()),
                Some(&plan),
                None,
                cx,
            );
        });
        assert_eq!(ready_events.load(Ordering::Acquire), 1);

        let started_events = entity.update(cx, |entity, cx| take_trace_effects(entity, cx));
        assert_eq!(started_events.len(), 2);
        assert_eq!(
            started_events.first().map(|event| event.stage),
            Some(ConnectionTraceStage::Queued)
        );
        assert_eq!(
            started_events.last().map(|event| event.stage),
            Some(ConnectionTraceStage::Preparing)
        );

        entity.update(cx, |entity, cx| {
            entity.finish_connection_trace_success(&node_id, cx);
        });
        let finished_events = entity.update(cx, |entity, cx| take_trace_effects(entity, cx));
        assert_eq!(finished_events.len(), 1);
        assert_eq!(
            finished_events.last().map(|event| event.status),
            Some(ConnectionTraceStatus::Ready)
        );

        entity.update(cx, |entity, cx| {
            entity.finish_connection_trace_success(&node_id, cx);
        });
        assert!(
            entity
                .update(cx, |entity, cx| take_trace_effects(entity, cx))
                .is_empty()
        );
    }

    #[gpui::test]
    fn connection_trace_failure_and_cancel_are_terminal_once(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let failed_node_id = NodeId::new("node-failed");
        let cancelled_node_id = NodeId::new("node-cancelled");

        entity.update(cx, |entity, cx| {
            entity.begin_connection_trace(&failed_node_id, None, None, None, None, cx);
            entity.begin_connection_trace(&cancelled_node_id, None, None, None, None, cx);
            let _ = take_trace_effects(entity, cx);
            entity.push_connection_trace_event(
                &failed_node_id,
                ConnectionTraceStage::Authentication,
                ConnectionTraceStatus::Running,
                connection_trace_progress(ConnectionTraceStage::Authentication),
                None,
                cx,
            );
            let _ = take_trace_effects(entity, cx);
            entity.finish_connection_trace_failed(
                &failed_node_id,
                Some("connection timed out".to_string()),
                cx,
            );
            entity.cancel_connection_trace(&cancelled_node_id, cx);
        });

        let terminal_events = entity.update(cx, |entity, cx| take_trace_effects(entity, cx));
        assert_eq!(terminal_events.len(), 2);
        assert!(terminal_events.iter().any(|event| {
            event.node_id == failed_node_id
                && event.status == ConnectionTraceStatus::Failed
                && event.stage == ConnectionTraceStage::Authentication
        }));
        assert!(terminal_events.iter().any(|event| {
            event.node_id == cancelled_node_id && event.status == ConnectionTraceStatus::Cancelled
        }));
    }

    #[gpui::test]
    fn closing_last_terminal_consumer_preserves_node_and_registry(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_id = NodeId::new("test-node");
        let node_consumer = ConnectionConsumer::NodeRouter(node_id.0.clone());
        let config = SshConfig::default();
        let connection_handle = ssh_registry.acquire(config.clone(), node_consumer.clone());
        let connection_id = connection_handle.connection_id().to_string();
        let _ = ssh_registry.mark_state(&connection_id, ConnectionState::Active);
        let node_router = NodeRouter::new(ssh_registry.clone());
        node_router.upsert_node(node_id.clone(), config);
        node_router
            .bind_connection(&node_id, connection_id.clone())
            .expect("bind node connection");
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry.clone(),
                node_router.clone(),
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });
        let (endpoint_event_tx, endpoint_event_rx) = std::sync::mpsc::channel();
        node_router.emitter().subscribe(endpoint_event_tx);

        let session_id = TerminalSessionId(7);
        let terminal_session: SharedTerminalSession = Arc::new(parking_lot::Mutex::new(
            oxideterm_terminal::TerminalSession::recording_playback(
                80,
                24,
                oxideterm_terminal::GraphicsOptions::default(),
                1_000,
            ),
        ));
        let terminal_session_weak = Arc::downgrade(&terminal_session);
        entity.update(cx, |entity, _cx| {
            assert!(entity.register_ssh_terminal_session(session_id, node_id.clone()));
            assert!(entity.retain_terminal_endpoint_session(session_id, terminal_session.clone()));
            entity.bind_ssh_terminal_endpoint(
                &node_id,
                TerminalEndpoint {
                    ws_port: 0,
                    ws_token: zeroize::Zeroizing::new("terminal-token".to_string()),
                    session_id: session_id.0.to_string(),
                },
            );
            assert_eq!(entity.terminal_session_lifecycles().len(), 1);
        });
        drop(terminal_session);
        assert!(
            terminal_session_weak.upgrade().is_some(),
            "runtime must retain the terminal after its tab-side handoff"
        );
        entity.update(cx, |entity, _cx| {
            assert_eq!(
                entity.unregister_ssh_terminal_session(session_id),
                Some(node_id.clone())
            );
        });
        assert!(
            terminal_session_weak.upgrade().is_none(),
            "terminal unregister must release the retained endpoint session"
        );
        assert!(matches!(
            endpoint_event_rx.try_recv(),
            Ok(NodeStateEvent::TerminalEndpointChanged {
                node_id: event_node_id,
                available: true,
                ..
            }) if event_node_id == node_id.0
        ));
        assert!(matches!(
            endpoint_event_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));

        let connection_info = ssh_registry
            .get(&connection_id)
            .expect("closing the terminal must retain the node connection")
            .info();
        assert_eq!(connection_info.ref_count, 1);
        assert_eq!(connection_info.consumers, vec![node_consumer]);
        assert_eq!(
            node_router.connection_id_for_node(&node_id),
            Some(connection_id)
        );
        assert!(
            node_router.terminal_url(&node_id).is_err(),
            "closing a terminal removes only its endpoint"
        );
    }

    #[gpui::test]
    fn first_terminal_requests_are_runtime_owned_and_coalesced(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let node_id = NodeId::new("node-a");
        entity.update(cx, |entity, cx| {
            let first = entity.queue_ssh_terminal_open(
                PendingSshTerminalOpen {
                    node_id: node_id.clone(),
                    post_connect_command: None,
                    mark_used_connection_id: None,
                    save_after_open: None,
                    cleanup_node_id: None,
                    title: "First terminal".to_string(),
                },
                cx,
            );
            let second = entity.queue_ssh_terminal_open(
                PendingSshTerminalOpen {
                    node_id: node_id.clone(),
                    post_connect_command: Some("pwd".to_string()),
                    mark_used_connection_id: Some("saved-a".to_string()),
                    save_after_open: None,
                    cleanup_node_id: None,
                    title: "Ignored duplicate".to_string(),
                },
                cx,
            );

            assert_eq!(first, QueueSshTerminalOpenOutcome::Queued);
            assert_eq!(second, QueueSshTerminalOpenOutcome::Coalesced);
            assert_eq!(entity.pending_ssh_terminal_opens.len(), 1);
            let pending = entity
                .pending_ssh_terminal_opens
                .front()
                .expect("coalesced terminal request");
            assert_eq!(pending.title, "First terminal");
            assert_eq!(pending.post_connect_command.as_deref(), Some("pwd"));
            assert_eq!(pending.mark_used_connection_id.as_deref(), Some("saved-a"));
        });
    }

    #[gpui::test]
    fn node_connected_transition_emits_first_terminal_effect_once(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let node_id = NodeId::new("node-ready");
        entity.update(cx, |entity, cx| {
            let outcome = entity.queue_ssh_terminal_open(
                PendingSshTerminalOpen {
                    node_id: node_id.clone(),
                    post_connect_command: None,
                    mark_used_connection_id: None,
                    save_after_open: None,
                    cleanup_node_id: None,
                    title: "Ready terminal".to_string(),
                },
                cx,
            );
            assert_eq!(outcome, QueueSshTerminalOpenOutcome::Queued);
            entity.publish_ssh_terminal_opens_for_connected_node(&node_id, cx);
            assert!(entity.pending_ssh_terminal_opens.is_empty());

            let effects = entity.take_runtime_effects(cx);
            assert_eq!(effects.len(), 1);
            let WorkspaceRuntimeEffect::OpenReadySshTerminals { requests } =
                effects.into_iter().next().expect("typed open effect")
            else {
                panic!("expected typed SSH terminal open effect");
            };
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].node_id, node_id);
            assert!(entity.take_runtime_effects(cx).is_empty());
        });
    }

    #[gpui::test]
    fn workspace_shutdown_stops_nodes_and_registry_once(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(ssh_registry.clone());
        let node_id = NodeId::new("node-a");
        let config = SshConfig::default();
        node_router.upsert_node(node_id.clone(), config.clone());
        let connection =
            ssh_registry.acquire(config, ConnectionConsumer::NodeRouter(node_id.0.clone()));
        let connection_id = connection.connection_id().to_string();
        let _ = ssh_registry.mark_state(&connection_id, ConnectionState::Active);
        node_router
            .bind_connection(&node_id, connection_id.clone())
            .expect("bind node connection");
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry.clone(),
                node_router.clone(),
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });

        entity.update(cx, |entity, _cx| {
            assert!(entity.register_ssh_terminal_session(TerminalSessionId(9), node_id.clone()));
            let active_probe_task = entity.task_runtime.spawn(std::future::pending::<()>());
            entity.active_probe_task = Some(active_probe_task.abort_handle());
            entity.ssh_active_probe_in_flight = true;
            let grace_probe_task = entity.task_runtime.spawn(std::future::pending::<()>());
            entity.reconnect_grace_probe_tasks.insert(
                node_id.clone(),
                ("grace-job".to_string(), grace_probe_task.abort_handle()),
            );
            assert!(entity.active_probe_timer_task.is_some());
            entity.shutdown_workspace_runtime();
            entity.shutdown_workspace_runtime();
            assert_eq!(entity.lifecycle, WorkspaceRuntimeLifecycle::Stopped);
            assert!(entity.terminal_ssh_nodes.is_empty());
            assert!(entity.pending_ssh_terminal_opens.is_empty());
            assert!(entity.active_probe_task.is_none());
            assert!(entity.active_probe_timer_task.is_none());
            assert!(!entity.ssh_active_probe_in_flight);
            assert!(entity.reconnect_grace_probe_tasks.is_empty());
        });

        assert!(ssh_registry.get(&connection_id).is_none());
        assert!(node_router.connection_id_for_node(&node_id).is_none());
    }

    #[gpui::test]
    fn entity_shutdown_invalidates_attempt_and_queued_completion(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        entity.update(cx, |entity, _cx| {
            let node_id = NodeId::new("node-a");
            let connection_id = "connection-a";
            let attempt_id = register_test_node_transport_attempt(entity, &node_id, connection_id);
            entity
                .runtime_effects
                .push_back(WorkspaceRuntimeEffect::Reconnect(
                    ReconnectRuntimeEffect::NodeConnected {
                        node_id: node_id.clone(),
                        connection_id: connection_id.to_string(),
                        reconnecting: false,
                    },
                ));

            entity.shutdown_node_transport_attempts();

            assert!(!entity.node_transport_result_is_current(&node_id, attempt_id));
            assert!(entity.runtime_effects.is_empty());
        });
    }

    #[gpui::test]
    fn cancelling_reconnect_clears_the_owned_grace_probe(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        let node_id = NodeId::new("node-a");
        entity.update(cx, |entity, _cx| {
            let grace_probe_task = entity.task_runtime.spawn(std::future::pending::<()>());
            entity.reconnect_grace_probe_tasks.insert(
                node_id.clone(),
                ("grace-job".to_string(), grace_probe_task.abort_handle()),
            );

            entity.cancel_queued_reconnects(std::slice::from_ref(&node_id));

            assert!(!entity.reconnect_grace_probe_tasks.contains_key(&node_id));
        });
    }

    #[gpui::test]
    fn disconnecting_a_reconnecting_node_cancels_trace_and_transport(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(ssh_registry.clone());
        let node_id = NodeId::new("node-a");
        let config = SshConfig {
            host: "node-a.example.test".to_string(),
            ..SshConfig::default()
        };
        node_router.upsert_node(node_id.clone(), config.clone());
        let connection =
            ssh_registry.acquire(config, ConnectionConsumer::NodeRouter(node_id.0.clone()));
        let connection_id = connection.connection_id().to_string();
        node_router
            .bind_connection(&node_id, connection_id.clone())
            .expect("node connection binding");
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry.clone(),
                node_router,
                test_task_runtime(),
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });

        entity.update(cx, |entity, cx| {
            entity.start_reconnect_job(
                &node_id,
                "Node A".to_string(),
                ReconnectSnapshot::default(),
            );
            let trace_plan = entity
                .new_connection_trace_plan(ConnectionTraceMode::Reconnect, vec![node_id.clone()]);
            entity.begin_connection_trace(
                &node_id,
                Some("Node A".to_string()),
                None,
                Some(&trace_plan),
                None,
                cx,
            );
            take_trace_effects(entity, cx);
            let attempt_id = register_test_node_transport_attempt(entity, &node_id, &connection_id);

            assert_eq!(
                entity.disconnect_node_runtime_subtree(&node_id, cx),
                vec![node_id.clone()]
            );
            assert!(!entity.has_active_reconnect_job(&node_id));
            assert!(!entity.node_transport_result_is_current(&node_id, attempt_id));
            assert_eq!(
                entity
                    .node_router
                    .node_runtime_snapshot(&node_id)
                    .expect("node runtime")
                    .state
                    .readiness,
                NodeReadiness::Disconnected
            );
            assert!(take_trace_effects(entity, cx).iter().any(|event| {
                event.node_id == node_id && event.status == ConnectionTraceStatus::Cancelled
            }));
        });

        assert!(ssh_registry.get(&connection_id).is_none());
    }

    #[gpui::test]
    fn node_transport_start_requires_entity_owned_runtime_config(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        entity.update(cx, |entity, _cx| {
            assert!(matches!(
                entity.start_node_transport(
                    &NodeId::new("missing-node"),
                    unavailable_managed_key_resolver(),
                ),
                Err(NodeTransportStartError::MissingRuntime)
            ));
        });
    }

    #[gpui::test]
    fn node_transport_start_binds_registry_before_worker_delivery(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(ssh_registry.clone());
        let node_id = NodeId::new("node-a");
        node_router.upsert_node(node_id.clone(), SshConfig::default());
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry.clone(),
                node_router,
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });

        entity.update(cx, |entity, _cx| {
            entity
                .start_node_transport(&node_id, unavailable_managed_key_resolver())
                .expect("node transport start");
            let connection_id = entity
                .node_router
                .connection_id_for_node(&node_id)
                .expect("node connection binding");
            let connection = ssh_registry
                .get(&connection_id)
                .expect("registered connection");
            assert_eq!(connection.state(), ConnectionState::Connecting);
            assert!(
                connection
                    .info()
                    .consumers
                    .contains(&ConnectionConsumer::NodeRouter(node_id.0.clone()))
            );
        });
    }

    #[gpui::test]
    fn stale_worker_completion_preserves_current_pooled_binding(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(ssh_registry.clone());
        let node_id = NodeId::new("node-a");
        let config = SshConfig {
            host: "node-a.example".to_string(),
            ..SshConfig::default()
        };
        node_router.upsert_node(node_id.clone(), config.clone());
        let node_consumer = ConnectionConsumer::NodeRouter(node_id.0.clone());
        let connection = ssh_registry.acquire(config, node_consumer.clone());
        let connection_id = connection.connection_id().to_string();
        node_router
            .bind_connection(&node_id, connection_id.clone())
            .expect("node connection binding");
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry.clone(),
                node_router,
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });

        entity.update(cx, |entity, _cx| {
            entity.retire_stale_node_connection(&node_id, &connection_id);
        });

        let connection_info = ssh_registry
            .get(&connection_id)
            .expect("current pooled connection remains registered")
            .info();
        assert!(connection_info.consumers.contains(&node_consumer));
    }

    #[gpui::test]
    fn explicit_disconnect_rejects_late_node_transport_success(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(ssh_registry.clone());
        let node_id = NodeId::new("node-a");
        let config = SshConfig {
            host: "node-a.example.test".to_string(),
            ..SshConfig::default()
        };
        node_router.upsert_node(node_id.clone(), config.clone());
        let connection =
            ssh_registry.acquire(config, ConnectionConsumer::NodeRouter(node_id.0.clone()));
        let connection_id = connection.connection_id().to_string();
        node_router
            .bind_connection(&node_id, connection_id.clone())
            .expect("node connection binding");
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry.clone(),
                node_router,
                test_task_runtime(),
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });
        let (sender, attempt_id) = entity.update(cx, |entity, cx| {
            let attempt_id = register_test_node_transport_attempt(entity, &node_id, &connection_id);
            entity.disconnect_node_runtime_subtree(&node_id, cx);
            (entity.reconnect_worker_sender(), attempt_id)
        });

        sender
            .send(ReconnectWorkerResult::NodeConnected {
                node_id: node_id.clone(),
                connection_id: connection_id.clone(),
                attempt_id,
                job_id: None,
            })
            .expect("late worker result");
        entity.update(cx, |entity, cx| {
            entity.drain_worker_results(cx);
            assert!(take_reconnect_effects(entity, cx).is_empty());
            assert!(!entity.node_transport_result_is_current(&node_id, attempt_id));
            assert_eq!(
                entity
                    .node_router
                    .node_runtime_snapshot(&node_id)
                    .expect("node runtime")
                    .state
                    .readiness,
                NodeReadiness::Disconnected
            );
        });
        assert!(ssh_registry.get(&connection_id).is_none());
    }

    #[gpui::test]
    fn explicit_disconnect_rejects_late_node_transport_failure(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(ssh_registry.clone());
        let node_id = NodeId::new("node-a");
        let config = SshConfig {
            host: "node-a.example.test".to_string(),
            ..SshConfig::default()
        };
        node_router.upsert_node(node_id.clone(), config.clone());
        let connection =
            ssh_registry.acquire(config, ConnectionConsumer::NodeRouter(node_id.0.clone()));
        let connection_id = connection.connection_id().to_string();
        node_router
            .bind_connection(&node_id, connection_id.clone())
            .expect("node connection binding");
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry,
                node_router,
                test_task_runtime(),
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });
        let (sender, attempt_id) = entity.update(cx, |entity, cx| {
            let attempt_id = register_test_node_transport_attempt(entity, &node_id, &connection_id);
            entity.disconnect_node_runtime_subtree(&node_id, cx);
            (entity.reconnect_worker_sender(), attempt_id)
        });

        sender
            .send(ReconnectWorkerResult::NodeConnectFailed {
                node_id: node_id.clone(),
                connection_id,
                error: "late authentication failure".to_string(),
                attempt_id,
                job_id: None,
            })
            .expect("late worker result");
        entity.update(cx, |entity, cx| {
            entity.drain_worker_results(cx);
            assert!(take_reconnect_effects(entity, cx).is_empty());
            assert_eq!(
                entity
                    .node_router
                    .node_runtime_snapshot(&node_id)
                    .expect("node runtime")
                    .state
                    .readiness,
                NodeReadiness::Disconnected
            );
        });
    }

    #[gpui::test]
    fn replacement_attempt_rejects_old_completion_and_accepts_new_completion(
        cx: &mut TestAppContext,
    ) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(ssh_registry.clone());
        let node_id = NodeId::new("node-a");
        let config = SshConfig {
            host: "node-a.example.test".to_string(),
            ..SshConfig::default()
        };
        node_router.upsert_node(node_id.clone(), config.clone());
        let old_connection = ssh_registry.acquire(
            config.clone(),
            ConnectionConsumer::NodeRouter(node_id.0.clone()),
        );
        let old_connection_id = old_connection.connection_id().to_string();
        node_router
            .bind_connection(&node_id, old_connection_id.clone())
            .expect("old node connection binding");
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry.clone(),
                node_router,
                test_task_runtime(),
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });
        let (sender, old_attempt_id, new_attempt_id, new_connection_id) =
            entity.update(cx, |entity, _cx| {
                let old_attempt_id =
                    register_test_node_transport_attempt(entity, &node_id, &old_connection_id);
                entity.cancel_node_transport_attempt(&node_id);
                let new_connection =
                    ssh_registry.acquire(config, ConnectionConsumer::NodeRouter(node_id.0.clone()));
                let new_connection_id = new_connection.connection_id().to_string();
                entity
                    .node_router
                    .bind_connection(&node_id, new_connection_id.clone())
                    .expect("new node connection binding");
                let new_attempt_id =
                    register_test_node_transport_attempt(entity, &node_id, &new_connection_id);
                (
                    entity.reconnect_worker_sender(),
                    old_attempt_id,
                    new_attempt_id,
                    new_connection_id,
                )
            });

        sender
            .send(ReconnectWorkerResult::NodeConnected {
                node_id: node_id.clone(),
                connection_id: old_connection_id,
                attempt_id: old_attempt_id,
                job_id: None,
            })
            .expect("old worker result");
        sender
            .send(ReconnectWorkerResult::NodeConnected {
                node_id: node_id.clone(),
                connection_id: new_connection_id.clone(),
                attempt_id: new_attempt_id,
                job_id: None,
            })
            .expect("new worker result");
        entity.update(cx, |entity, cx| {
            entity.drain_worker_results(cx);
            let results = take_reconnect_effects(entity, cx);
            assert_eq!(results.len(), 1);
            assert!(matches!(
                results.first(),
                Some(ReconnectRuntimeEffect::NodeConnected {
                    connection_id,
                    ..
                }) if connection_id.as_str() == new_connection_id.as_str()
            ));
            assert!(!entity.node_transport_result_is_current(&node_id, new_attempt_id));
        });
        assert!(ssh_registry.get(&new_connection_id).is_some());
    }

    #[gpui::test]
    fn child_reset_releases_only_ancestor_consumer(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_runtime_store = NodeRuntimeStore::default();
        let parent_id = NodeId::new("parent");
        let child_id = NodeId::new("child");
        let parent_config = SshConfig {
            host: "parent.example".to_string(),
            ..SshConfig::default()
        };
        let child_config = SshConfig {
            host: "child.example".to_string(),
            ..SshConfig::default()
        };
        node_runtime_store.upsert_node(parent_id.clone(), parent_config.clone());
        node_runtime_store
            .upsert_child_node(parent_id.clone(), child_id.clone(), child_config.clone())
            .expect("child runtime node");
        let node_router = NodeRouter::with_runtime_store(ssh_registry.clone(), node_runtime_store);
        let parent_consumer = ConnectionConsumer::NodeRouter(parent_id.0.clone());
        let parent_connection = ssh_registry.acquire(parent_config, parent_consumer.clone());
        let parent_connection_id = parent_connection.connection_id().to_string();
        node_router
            .bind_connection(&parent_id, parent_connection_id.clone())
            .expect("parent connection binding");
        let child_consumer = ConnectionConsumer::NodeRouter(child_id.0.clone());
        let child_connection = ssh_registry.acquire(child_config, child_consumer);
        let child_connection_id = child_connection.connection_id().to_string();
        node_router
            .bind_connection(&child_id, child_connection_id.clone())
            .expect("child connection binding");
        let ancestor_consumer = ConnectionConsumer::NodeRouter(format!("{}:ancestor", child_id.0));
        ssh_registry
            .acquire_consumer_for_connection(&parent_connection_id, ancestor_consumer.clone())
            .expect("ancestor consumer");
        ssh_registry
            .set_parent_connection_ownership(
                &child_connection_id,
                parent_connection_id.clone(),
                ancestor_consumer.clone(),
            )
            .expect("parent connection ownership");
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry.clone(),
                node_router,
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });

        entity.update(cx, |entity, _cx| {
            entity.reset_node_connection(&child_id);
        });

        assert!(ssh_registry.get(&child_connection_id).is_none());
        let parent_info = ssh_registry
            .get(&parent_connection_id)
            .expect("shared parent connection remains registered")
            .info();
        assert!(parent_info.consumers.contains(&parent_consumer));
        assert!(!parent_info.consumers.contains(&ancestor_consumer));
    }

    #[gpui::test]
    fn connection_chain_reset_does_not_publish_stale_disconnect(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(ssh_registry.clone());
        let node_id = NodeId::new("node-a");
        node_router.upsert_node(node_id.clone(), SshConfig::default());
        let (_, node_event_rx) = node_router.emitter().subscribe_bounded(8);
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry,
                node_router,
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });

        entity.update(cx, |entity, _cx| {
            entity.reset_node_connection(&node_id);
        });

        assert!(matches!(
            node_event_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
    }

    #[gpui::test]
    fn grace_recovery_marks_only_connections_with_physical_transport_ready(
        cx: &mut TestAppContext,
    ) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_runtime_store = NodeRuntimeStore::default();
        let root_id = NodeId::new("root");
        let child_id = NodeId::new("child");
        let root_config = SshConfig {
            host: "root.example".to_string(),
            ..SshConfig::default()
        };
        let child_config = SshConfig {
            host: "child.example".to_string(),
            ..SshConfig::default()
        };
        node_runtime_store.upsert_node(root_id.clone(), root_config.clone());
        node_runtime_store
            .upsert_child_node(root_id.clone(), child_id.clone(), child_config.clone())
            .expect("child runtime node");
        let node_router = NodeRouter::with_runtime_store(ssh_registry.clone(), node_runtime_store);
        let root_connection = ssh_registry.acquire(
            root_config,
            ConnectionConsumer::NodeRouter(root_id.0.clone()),
        );
        let root_connection_id = root_connection.connection_id().to_string();
        root_connection.set_physical(Arc::new(()));
        node_router
            .bind_connection(&root_id, root_connection_id.clone())
            .expect("root connection binding");
        let child_connection = ssh_registry.acquire(
            child_config,
            ConnectionConsumer::NodeRouter(child_id.0.clone()),
        );
        let child_connection_id = child_connection.connection_id().to_string();
        node_router
            .bind_connection(&child_id, child_connection_id.clone())
            .expect("child connection binding");
        let _ =
            ssh_registry.mark_state_without_event(&root_connection_id, ConnectionState::LinkDown);
        let _ =
            ssh_registry.mark_state_without_event(&child_connection_id, ConnectionState::LinkDown);
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry.clone(),
                node_router,
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });

        entity.update(cx, |entity, _cx| {
            let recovered_node_ids = entity.apply_grace_recovery(
                &root_id,
                &root_connection_id,
                vec![(child_id.clone(), child_connection_id.clone())],
            );
            assert_eq!(recovered_node_ids, vec![root_id.clone()]);
            assert_eq!(
                entity
                    .node_router
                    .node_state(&root_id)
                    .expect("root runtime state")
                    .state
                    .readiness,
                NodeReadiness::Ready
            );
            assert_ne!(
                entity
                    .node_router
                    .node_state(&child_id)
                    .expect("child runtime state")
                    .state
                    .readiness,
                NodeReadiness::Ready
            );
            entity.apply_grace_expiration(&root_connection_id);
        });

        assert_eq!(
            ssh_registry
                .get(&root_connection_id)
                .expect("root connection")
                .state(),
            ConnectionState::LinkDown
        );
        assert_eq!(
            ssh_registry
                .get(&child_connection_id)
                .expect("child connection")
                .state(),
            ConnectionState::LinkDown
        );
    }

    #[gpui::test]
    fn node_event_delivery_filters_stale_generations_inside_entity(cx: &mut TestAppContext) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_event_emitter = NodeEventEmitter::default();
        let entity_emitter = node_event_emitter.clone();
        let node_router = NodeRouter::with_runtime_store_and_emitter(
            ssh_registry.clone(),
            NodeRuntimeStore::default(),
            entity_emitter,
        );
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry,
                node_router,
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
                if *event == WorkspaceRuntimeEvent::EffectsReady {
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

        entity.update(cx, |entity, cx| {
            let events = entity
                .take_runtime_effects(cx)
                .into_iter()
                .filter_map(|effect| match effect {
                    WorkspaceRuntimeEffect::Node(effect) => Some(effect),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(events.len(), 2);
            assert!(matches!(
                events.first(),
                Some(NodeRuntimeEffect::ConnectionStateChanged { .. })
            ));
            assert!(matches!(
                events.last(),
                Some(NodeRuntimeEffect::TerminalEndpointChanged)
            ));
            assert_eq!(
                entity.node_event_generations.get(&NodeId::new("node-a")),
                Some(&3)
            );
        });
    }

    #[gpui::test]
    fn connection_status_reducer_updates_runtime_before_emitting_ui_effect(
        cx: &mut TestAppContext,
    ) {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(ssh_registry.clone());
        let node_id = NodeId::new("node-a");
        let config = SshConfig::default();
        node_router.upsert_node(node_id.clone(), config.clone());
        let connection =
            ssh_registry.acquire(config, ConnectionConsumer::NodeRouter(node_id.0.clone()));
        let connection_id = connection.connection_id().to_string();
        node_router
            .bind_connection(&node_id, connection_id.clone())
            .expect("bind node connection");
        let _ = ssh_registry.mark_state_without_event(&connection_id, ConnectionState::LinkDown);
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry,
                node_router,
                task_runtime,
                true,
                ReconnectTiming::default(),
                3,
                cx,
            )
        });

        entity.update(cx, |entity, _cx| {
            let effect = entity
                .reduce_node_event(NodeStateEvent::ConnectionStatusChanged {
                    connection_id: connection_id.clone(),
                    status: "link_down".to_string(),
                    affected_children: Vec::new(),
                    timestamp: 0,
                })
                .expect("typed connection status effect");
            assert!(matches!(
                effect,
                NodeRuntimeEffect::ConnectionStatusChanged {
                    node_id: effect_node_id,
                    state: NodeReadiness::Error,
                    ..
                } if effect_node_id == node_id
            ));
            assert_eq!(
                entity
                    .node_router
                    .node_runtime_snapshot(&node_id)
                    .expect("runtime snapshot")
                    .state
                    .readiness,
                NodeReadiness::Error
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
        let node_router = NodeRouter::with_runtime_store(ssh_registry.clone(), node_runtime_store);
        let task_runtime = test_task_runtime();
        let entity = cx.new(|cx| {
            WorkspaceRuntimeEntity::new(
                ssh_registry,
                node_router,
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
            assert!(entity.runtime_effects.is_empty());
            entity.flush_reconnect_roots(7, cx);
            assert!(matches!(
                entity.take_runtime_effects(cx).as_slice(),
                [WorkspaceRuntimeEffect::StartReconnectRoot { node_id }] if node_id == &root
            ));
        });
    }

    #[gpui::test]
    fn disabling_reconnect_clears_pending_debounce_state(cx: &mut TestAppContext) {
        let entity = test_runtime_entity(cx);
        entity.update(cx, |entity, cx| {
            entity.queue_reconnect_root(NodeId::new("node-a"), cx);
            let scheduled_generation = entity.reconnect_debounce_generation;
            assert!(!entity.pending_reconnect_node_ids.is_empty());
            assert!(entity.reconnect_debounce_task.is_some());

            entity.configure_reconnect(false, ReconnectTiming::default(), 3, cx);

            assert!(entity.pending_reconnect_node_ids.is_empty());
            assert!(entity.reconnect_debounce_task.is_none());
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
                    cx,
                ),
                ReconnectPipelineClaim::Acquired
            );
            assert_eq!(
                entity.claim_reconnect_pipeline(
                    &waiting_node_id,
                    Some("connection-b".to_string()),
                    cx,
                ),
                ReconnectPipelineClaim::Requeued
            );
            assert!(
                entity
                    .reconnect_requeue_tasks
                    .contains_key(&waiting_node_id)
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
                    cx,
                ),
                ReconnectPipelineClaim::Exhausted
            );
            assert!(
                !entity
                    .reconnect_requeue_states
                    .contains_key(&waiting_node_id)
            );
            assert!(
                !entity
                    .reconnect_requeue_tasks
                    .contains_key(&waiting_node_id)
            );

            entity.release_reconnect_pipeline(&active_node_id);
            assert_eq!(
                entity.claim_reconnect_pipeline(&waiting_node_id, None, cx),
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
                entity.claim_reconnect_pipeline(&active_node_id, None, cx,),
                ReconnectPipelineClaim::Acquired
            );
            entity.replace_reconnect_cascade([active_node_id.clone(), child_node_id.clone()]);
            entity
                .runtime_effects
                .push_back(WorkspaceRuntimeEffect::ContinueConnectionChain {
                    node_id: child_node_id.clone(),
                });

            entity.cancel_queued_reconnects(&[active_node_id.clone(), child_node_id]);

            assert!(entity.reconnect_pipeline_active_node.is_none());
            assert!(entity.pending_reconnect_cascade_nodes.is_empty());
            assert!(entity.runtime_effects.is_empty());
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
                if *event == WorkspaceRuntimeEvent::EffectsReady {
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

        entity.update(cx, |entity, cx| {
            assert!(matches!(
                entity.take_runtime_effects(cx).first(),
                Some(WorkspaceRuntimeEffect::ContinueConnectionChain { .. })
            ));
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
