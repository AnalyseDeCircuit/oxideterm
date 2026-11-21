#[derive(Clone, Debug, Default)]
pub struct NodeEventSequencer {
    generations: Arc<DashMap<NodeId, u64>>,
}

impl NodeEventSequencer {
    pub fn next(&self, node_id: &NodeId) -> u64 {
        let mut generation = self.generations.entry(node_id.clone()).or_insert(0);
        *generation += 1;
        *generation
    }

    pub fn current(&self, node_id: &NodeId) -> u64 {
        self.generations
            .get(node_id)
            .map(|generation| *generation)
            .unwrap_or_default()
    }

    pub fn reset(&self, node_id: &NodeId) {
        self.generations.remove(node_id);
    }
}

#[derive(Clone, Debug, Default)]
struct ConnectionNodeBindings {
    nodes: Arc<parking_lot::RwLock<HashMap<String, Vec<NodeId>>>>,
}

impl ConnectionNodeBindings {
    fn register(&self, connection_id: String, node_id: NodeId) {
        let mut nodes = self.nodes.write();
        let bound = nodes.entry(connection_id).or_default();
        if !bound.contains(&node_id) {
            bound.push(node_id);
        }
    }

    fn unbind(&self, connection_id: &str, node_id: &NodeId) -> bool {
        let mut nodes = self.nodes.write();
        let Some(bound) = nodes.get_mut(connection_id) else {
            return false;
        };
        bound.retain(|id| id != node_id);
        if bound.is_empty() {
            nodes.remove(connection_id);
            return false;
        }
        true
    }

    fn primary(&self, connection_id: &str) -> Option<NodeId> {
        self.nodes.read().get(connection_id)?.last().cloned()
    }
}

#[derive(Clone, Debug, Default)]
pub struct NodeEventEmitter {
    sequencer: NodeEventSequencer,
    connection_nodes: ConnectionNodeBindings,
    listeners: Arc<parking_lot::RwLock<Vec<mpsc::Sender<NodeStateEvent>>>>,
    mailbox_listeners: Arc<parking_lot::RwLock<HashMap<u64, Weak<NodeEventMailbox>>>>,
    next_listener_id: Arc<AtomicU64>,
}

struct NodeEventMailbox {
    queue: parking_lot::Mutex<VecDeque<NodeStateEvent>>,
    capacity: usize,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

pub struct NodeEventReceiver {
    mailbox: Arc<NodeEventMailbox>,
}

pub struct NodeEventSubscription {
    listener_id: u64,
    listeners: Weak<parking_lot::RwLock<HashMap<u64, Weak<NodeEventMailbox>>>>,
}

impl Drop for NodeEventSubscription {
    fn drop(&mut self) {
        if let Some(listeners) = self.listeners.upgrade() {
            listeners.write().remove(&self.listener_id);
        }
    }
}

impl NodeEventReceiver {
    pub fn try_recv(&self) -> Result<NodeStateEvent, mpsc::TryRecvError> {
        self.mailbox
            .queue
            .lock()
            .pop_front()
            .ok_or(mpsc::TryRecvError::Empty)
    }
}

impl NodeEventEmitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sequencer(&self) -> &NodeEventSequencer {
        &self.sequencer
    }

    pub fn subscribe(&self, sender: mpsc::Sender<NodeStateEvent>) {
        self.listeners.write().push(sender);
    }

    pub fn subscribe_bounded(&self, capacity: usize) -> (NodeEventSubscription, NodeEventReceiver) {
        self.subscribe_bounded_with_wake(capacity, None)
    }

    /// Creates a bounded mailbox and invokes the optional wake after a new event is queued.
    pub fn subscribe_bounded_with_wake(
        &self,
        capacity: usize,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> (NodeEventSubscription, NodeEventReceiver) {
        assert!(capacity > 0, "node event mailbox capacity must be positive");
        let listener_id = self.next_listener_id.fetch_add(1, Ordering::Relaxed);
        let mailbox = Arc::new(NodeEventMailbox {
            queue: parking_lot::Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            wake,
        });
        self.mailbox_listeners
            .write()
            .insert(listener_id, Arc::downgrade(&mailbox));
        (
            NodeEventSubscription {
                listener_id,
                listeners: Arc::downgrade(&self.mailbox_listeners),
            },
            NodeEventReceiver { mailbox },
        )
    }

    pub fn emit(&self, event: NodeStateEvent) {
        self.dispatch(&event);
    }

    pub fn register(&self, connection_id: impl Into<String>, node_id: NodeId) {
        self.connection_nodes
            .register(connection_id.into(), node_id);
    }

    pub fn unregister(&self, connection_id: &str) -> Option<NodeId> {
        self.connection_nodes
            .nodes
            .write()
            .remove(connection_id)
            .and_then(|mut nodes| nodes.pop())
    }

    /// Removes one route and reports whether the transport still serves other routes.
    pub fn unbind_node(&self, connection_id: &str, node_id: &NodeId) -> bool {
        self.connection_nodes.unbind(connection_id, node_id)
    }

    pub fn node_id_for_connection(&self, connection_id: &str) -> Option<NodeId> {
        self.connection_nodes.primary(connection_id)
    }

    pub(crate) fn only_bound_node_consumers(
        &self,
        connection_id: &str,
        consumers: &[ConnectionConsumer],
    ) -> bool {
        let nodes = self.connection_nodes.nodes.read();
        nodes.get(connection_id).is_some_and(|bound| {
            consumers.iter().all(|consumer| {
                matches!(consumer, ConnectionConsumer::NodeRouter(id) if bound.iter().any(|node| &node.0 == id))
            })
        })
    }

    pub fn emit_connection_state_changed(
        &self,
        connection_id: &str,
        state: NodeReadiness,
        reason: impl Into<String>,
    ) -> Option<NodeStateEvent> {
        self.emit_bound_state(connection_id, state, &reason.into(), None)
    }

    pub(crate) fn emit_idle_retirement(&self, connection_id: &str, reason: &str) {
        self.emit_bound_state(
            connection_id,
            NodeReadiness::Disconnected,
            reason,
            Some(connection_id),
        );
    }

    fn emit_bound_state(
        &self,
        connection_id: &str,
        state: NodeReadiness,
        reason: &str,
        retired_connection_id: Option<&str>,
    ) -> Option<NodeStateEvent> {
        let nodes = self
            .connection_nodes
            .nodes
            .read()
            .get(connection_id)?
            .clone();
        let mut last = None;
        for node_id in nodes {
            last =
                Some(self.emit_node_state(&node_id, state.clone(), reason, retired_connection_id));
        }
        last
    }

    fn emit_node_state(
        &self,
        node_id: &NodeId,
        state: NodeReadiness,
        reason: &str,
        retired_connection_id: Option<&str>,
    ) -> NodeStateEvent {
        let event = NodeStateEvent::ConnectionStateChanged {
            retired_connection_id: retired_connection_id.map(str::to_owned),
            node_id: node_id.0.clone(),
            generation: self.sequencer.next(node_id),
            state,
            reason: reason.to_owned(),
        };
        self.dispatch(&event);
        event
    }

    pub fn emit_state_from_connection(
        &self,
        connection_id: &str,
        connection_state: &ConnectionState,
        reason: impl Into<String>,
    ) -> Option<NodeStateEvent> {
        self.emit_connection_state_changed(
            connection_id,
            readiness_for_connection_state(connection_state),
            Self::connection_reason(connection_state, reason.into()),
        )
    }

    fn connection_reason(state: &ConnectionState, reason: String) -> String {
        match state {
            ConnectionState::Error(error) if reason.is_empty() => error.clone(),
            ConnectionState::Error(error) => format!("{reason}: {error}"),
            ConnectionState::LinkDown if reason.is_empty() => "link down".to_string(),
            _ => reason,
        }
    }

    pub fn emit_connection_status_changed(
        &self,
        connection_id: impl Into<String>,
        status: impl Into<String>,
        affected_children: Vec<String>,
    ) -> NodeStateEvent {
        let event = NodeStateEvent::ConnectionStatusChanged {
            connection_id: connection_id.into(),
            status: status.into(),
            affected_children,
            timestamp: now_ms(),
        };
        self.dispatch(&event);
        event
    }

    pub fn emit_sftp_ready(
        &self,
        connection_id: &str,
        ready: bool,
        cwd: Option<String>,
    ) -> Option<NodeStateEvent> {
        let node_id = self.node_id_for_connection(connection_id)?;
        let generation = self.sequencer.next(&node_id);
        let event = NodeStateEvent::SftpReady {
            node_id: node_id.0,
            generation,
            ready,
            cwd,
        };
        self.dispatch(&event);
        Some(event)
    }

    pub fn emit_terminal_endpoint_changed(
        &self,
        connection_id: &str,
        available: bool,
    ) -> Option<NodeStateEvent> {
        let node_id = self.node_id_for_connection(connection_id)?;
        let generation = self.sequencer.next(&node_id);
        let event = NodeStateEvent::TerminalEndpointChanged {
            node_id: node_id.0,
            generation,
            available,
        };
        self.dispatch(&event);
        Some(event)
    }

    fn dispatch(&self, event: &NodeStateEvent) {
        self.listeners
            .write()
            .retain(|listener| listener.send(event.clone()).is_ok());

        let mut listeners = self.mailbox_listeners.write();
        listeners.retain(|_, listener| {
            let Some(mailbox) = listener.upgrade() else {
                return false;
            };
            let mut queue = mailbox.queue.lock();
            if queue.iter().any(|queued| queued == event) {
                // NodeRouter callers may also forward the returned event. Drop
                // only exact duplicate deliveries, not distinct transitions.
                return true;
            }
            if !node_event_requires_reliable_delivery(event)
                && let Some(index) = queue.iter().position(|queued| {
                    !node_event_requires_reliable_delivery(queued)
                        && node_event_coalesce_key(queued) == node_event_coalesce_key(event)
                })
            {
                // Move replacements to the back so per-node generations remain
                // ordered across different event kinds in the shared mailbox.
                queue.remove(index);
            }
            if queue.len() >= mailbox.capacity
                && let Some(index) = queue
                    .iter()
                    .position(|queued| !node_event_requires_reliable_delivery(queued))
            {
                queue.remove(index);
            }
            // Error/disconnect transitions carry one-shot cleanup side effects.
            // They may temporarily exceed the level-event capacity rather than
            // being silently discarded while the UI is suspended.
            queue.push_back(event.clone());
            let wake = mailbox.wake.clone();
            drop(queue);
            if let Some(wake) = wake {
                wake();
            }
            true
        });
    }
}

fn node_event_coalesce_key(event: &NodeStateEvent) -> (&str, u8) {
    match event {
        NodeStateEvent::ConnectionStatusChanged { connection_id, .. } => (connection_id, 0),
        NodeStateEvent::ConnectionStateChanged { node_id, .. } => (node_id, 1),
        NodeStateEvent::SftpReady { node_id, .. } => (node_id, 2),
        NodeStateEvent::TerminalEndpointChanged { node_id, .. } => (node_id, 3),
        NodeStateEvent::SharedSftpSessionChanged { node_id, .. } => (node_id, 4),
    }
}

fn node_event_requires_reliable_delivery(event: &NodeStateEvent) -> bool {
    match event {
        NodeStateEvent::ConnectionStatusChanged { status, .. } => {
            matches!(status.as_str(), "link_down" | "disconnected")
        }
        NodeStateEvent::ConnectionStateChanged { state, .. } => {
            matches!(state, NodeReadiness::Error | NodeReadiness::Disconnected)
        }
        // Shared SFTP changes grant or revoke capability authority, so they
        // must not be coalesced away while the UI event consumer is suspended.
        NodeStateEvent::SharedSftpSessionChanged { .. } => true,
        NodeStateEvent::SftpReady { .. } | NodeStateEvent::TerminalEndpointChanged { .. } => false,
    }
}

#[cfg(test)]
mod mailbox_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn bounded_subscription_wakes_after_event_is_queued() {
        let emitter = NodeEventEmitter::new();
        let woke = Arc::new(AtomicBool::new(false));
        let callback_flag = woke.clone();
        let (_subscription, receiver) = emitter.subscribe_bounded_with_wake(
            1,
            Some(Arc::new(move || {
                callback_flag.store(true, Ordering::Release);
            })),
        );

        emitter.emit_connection_state_changed_for_test("node-a", NodeReadiness::Ready);

        assert!(woke.load(Ordering::Acquire));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            NodeStateEvent::ConnectionStateChanged {
                state: NodeReadiness::Ready,
                ..
            }
        ));
    }

    #[test]
    fn bounded_subscription_coalesces_latest_event_and_unsubscribes_on_drop() {
        let emitter = NodeEventEmitter::new();
        let (subscription, receiver) = emitter.subscribe_bounded(1);
        emitter.emit_connection_state_changed_for_test("node-a", NodeReadiness::Connecting);
        emitter.emit_connection_state_changed_for_test("node-a", NodeReadiness::Ready);

        assert!(matches!(
            receiver.try_recv().unwrap(),
            NodeStateEvent::ConnectionStateChanged {
                state: NodeReadiness::Ready,
                ..
            }
        ));
        drop(subscription);
        emitter.emit_connection_state_changed_for_test("node-a", NodeReadiness::Error);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn coalesced_events_keep_cross_kind_generation_order() {
        let emitter = NodeEventEmitter::new();
        let (_subscription, receiver) = emitter.subscribe_bounded(2);
        emitter.dispatch(&NodeStateEvent::ConnectionStateChanged {
            retired_connection_id: None,
            node_id: "node-a".to_string(),
            generation: 1,
            state: NodeReadiness::Connecting,
            reason: String::new(),
        });
        emitter.dispatch(&NodeStateEvent::SftpReady {
            node_id: "node-a".to_string(),
            generation: 2,
            ready: true,
            cwd: None,
        });
        emitter.dispatch(&NodeStateEvent::ConnectionStateChanged {
            retired_connection_id: None,
            node_id: "node-a".to_string(),
            generation: 3,
            state: NodeReadiness::Ready,
            reason: String::new(),
        });

        assert!(matches!(
            receiver.try_recv().unwrap(),
            NodeStateEvent::SftpReady { generation: 2, .. }
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            NodeStateEvent::ConnectionStateChanged { generation: 3, .. }
        ));
    }

    #[test]
    fn error_transition_is_not_coalesced_by_reconnecting_state() {
        let emitter = NodeEventEmitter::new();
        let (_subscription, receiver) = emitter.subscribe_bounded(1);
        emitter.dispatch(&NodeStateEvent::ConnectionStateChanged {
            retired_connection_id: None,
            node_id: "node-a".to_string(),
            generation: 1,
            state: NodeReadiness::Error,
            reason: "link down".to_string(),
        });
        emitter.dispatch(&NodeStateEvent::ConnectionStateChanged {
            retired_connection_id: None,
            node_id: "node-a".to_string(),
            generation: 2,
            state: NodeReadiness::Connecting,
            reason: String::new(),
        });

        assert!(matches!(
            receiver.try_recv().unwrap(),
            NodeStateEvent::ConnectionStateChanged {
                generation: 1,
                state: NodeReadiness::Error,
                ..
            }
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            NodeStateEvent::ConnectionStateChanged {
                generation: 2,
                state: NodeReadiness::Connecting,
                ..
            }
        ));
    }

    #[test]
    fn shared_sftp_revocation_is_not_coalesced_by_replacement() {
        let emitter = NodeEventEmitter::new();
        let (_subscription, receiver) = emitter.subscribe_bounded(1);
        emitter.dispatch(&NodeStateEvent::SharedSftpSessionChanged {
            node_id: "node-a".to_string(),
            generation: 1,
            connection_id: "connection-a".to_string(),
            session_generation: None,
            ready: false,
        });
        emitter.dispatch(&NodeStateEvent::SharedSftpSessionChanged {
            node_id: "node-a".to_string(),
            generation: 2,
            connection_id: "connection-a".to_string(),
            session_generation: Some(2),
            ready: true,
        });

        assert!(matches!(
            receiver.try_recv().unwrap(),
            NodeStateEvent::SharedSftpSessionChanged {
                generation: 1,
                ready: false,
                ..
            }
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            NodeStateEvent::SharedSftpSessionChanged {
                generation: 2,
                ready: true,
                ..
            }
        ));
    }

    impl NodeEventEmitter {
        fn emit_connection_state_changed_for_test(&self, node_id: &str, state: NodeReadiness) {
            self.dispatch(&NodeStateEvent::ConnectionStateChanged {
                retired_connection_id: None,
                node_id: node_id.to_string(),
                generation: 0,
                state,
                reason: String::new(),
            });
        }
    }
}
