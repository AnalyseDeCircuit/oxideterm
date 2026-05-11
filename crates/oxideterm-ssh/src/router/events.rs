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
pub struct NodeEventEmitter {
    sequencer: NodeEventSequencer,
    connection_nodes: Arc<DashMap<String, NodeId>>,
    listeners: Arc<parking_lot::RwLock<Vec<mpsc::Sender<NodeStateEvent>>>>,
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

    pub fn register(&self, connection_id: impl Into<String>, node_id: NodeId) {
        self.connection_nodes.insert(connection_id.into(), node_id);
    }

    pub fn unregister(&self, connection_id: &str) -> Option<NodeId> {
        self.connection_nodes
            .remove(connection_id)
            .map(|(_, node_id)| node_id)
    }

    pub fn node_id_for_connection(&self, connection_id: &str) -> Option<NodeId> {
        self.connection_nodes
            .get(connection_id)
            .map(|entry| entry.value().clone())
    }

    pub fn emit_connection_state_changed(
        &self,
        connection_id: &str,
        state: NodeReadiness,
        reason: impl Into<String>,
    ) -> Option<NodeStateEvent> {
        let node_id = self.node_id_for_connection(connection_id)?;
        let generation = self.sequencer.next(&node_id);
        let event = NodeStateEvent::ConnectionStateChanged {
            node_id: node_id.0,
            generation,
            state,
            reason: reason.into(),
        };
        self.dispatch(&event);
        Some(event)
    }

    pub fn emit_state_from_connection(
        &self,
        connection_id: &str,
        connection_state: &ConnectionState,
        reason: impl Into<String>,
    ) -> Option<NodeStateEvent> {
        let reason = reason.into();
        let reason = match connection_state {
            ConnectionState::Error(error) if reason.is_empty() => error.clone(),
            ConnectionState::Error(error) => format!("{reason}: {error}"),
            ConnectionState::LinkDown if reason.is_empty() => "link down".to_string(),
            _ => reason,
        };
        self.emit_connection_state_changed(
            connection_id,
            readiness_for_connection_state(connection_state),
            reason,
        )
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
        ws_port: u16,
        ws_token: impl Into<String>,
    ) -> Option<NodeStateEvent> {
        let node_id = self.node_id_for_connection(connection_id)?;
        let generation = self.sequencer.next(&node_id);
        let event = NodeStateEvent::TerminalEndpointChanged {
            node_id: node_id.0,
            generation,
            ws_port,
            ws_token: ws_token.into(),
        };
        self.dispatch(&event);
        Some(event)
    }

    fn dispatch(&self, event: &NodeStateEvent) {
        let listeners = self.listeners.read().clone();
        for listener in listeners {
            let _ = listener.send(event.clone());
        }
    }
}
