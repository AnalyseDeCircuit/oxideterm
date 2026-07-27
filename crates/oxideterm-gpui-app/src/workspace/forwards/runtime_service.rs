// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_ssh::SshConnectionRegistry;
use std::sync::{Mutex, MutexGuard};

#[derive(Default)]
struct ForwardingBindingState {
    consumers: HashMap<String, (String, ConnectionConsumer)>,
}

impl ForwardingBindingState {
    fn connection_id(&self, session_id: &str) -> Option<String> {
        self.consumers
            .get(session_id)
            .map(|(connection_id, _)| connection_id.clone())
    }

    fn node_for_connection_id(&self, connection_id: &str) -> Option<NodeId> {
        self.consumers
            .iter()
            .find_map(|(session_id, (candidate_connection_id, _))| {
                (candidate_connection_id == connection_id)
                    .then(|| node_id_from_forwarding_session(session_id))
                    .flatten()
            })
    }

    fn replace(
        &mut self,
        session_id: String,
        connection_id: String,
        consumer: ConnectionConsumer,
    ) -> Option<(String, ConnectionConsumer)> {
        self.consumers.insert(session_id, (connection_id, consumer))
    }

    fn remove(&mut self, session_id: &str) -> Option<(String, ConnectionConsumer)> {
        self.consumers.remove(session_id)
    }

    fn remove_exact(
        &mut self,
        session_id: &str,
        connection_id: &str,
        consumer: &ConnectionConsumer,
    ) {
        if self
            .consumers
            .get(session_id)
            .is_some_and(|(stored_connection_id, stored_consumer)| {
                stored_connection_id == connection_id && stored_consumer == consumer
            })
        {
            self.consumers.remove(session_id);
        }
    }
}

/// Owns forwarding managers and SSH consumer bindings independently of UI mounts.
#[derive(Clone)]
pub(in crate::workspace) struct ForwardingRuntimeService {
    registry: ForwardingRegistry,
    ssh_registry: SshConnectionRegistry,
    node_router: NodeRouter,
    bindings: Arc<Mutex<ForwardingBindingState>>,
}

impl ForwardingRuntimeService {
    pub(in crate::workspace) fn new(
        registry: ForwardingRegistry,
        ssh_registry: SshConnectionRegistry,
        node_router: NodeRouter,
    ) -> Self {
        Self {
            registry,
            ssh_registry,
            node_router,
            bindings: Arc::new(Mutex::new(ForwardingBindingState::default())),
        }
    }

    pub(in crate::workspace) fn registry(&self) -> &ForwardingRegistry {
        &self.registry
    }

    pub(in crate::workspace) fn session_id_for_node(node_id: &NodeId) -> String {
        format!("{FORWARDS_NODE_SESSION_PREFIX}{}", node_id.0)
    }

    pub(in crate::workspace) fn node_id_for_session(session_id: &str) -> Option<NodeId> {
        node_id_from_forwarding_session(session_id)
    }

    pub(in crate::workspace) fn connection_id_for_node(&self, node_id: &NodeId) -> Option<String> {
        self.binding_state()
            .connection_id(&Self::session_id_for_node(node_id))
    }

    pub(in crate::workspace) fn node_for_connection_id(
        &self,
        connection_id: &str,
    ) -> Option<NodeId> {
        self.binding_state().node_for_connection_id(connection_id)
    }

    pub(in crate::workspace) fn manager_for_node(
        &self,
        node_id: &NodeId,
    ) -> Option<Arc<ForwardingManager>> {
        self.registry.get(&Self::session_id_for_node(node_id))
    }

    pub(in crate::workspace) fn release_binding_for_node(
        &self,
        node_id: &NodeId,
    ) -> Option<String> {
        let session_id = Self::session_id_for_node(node_id);
        self.release_binding_for_session_inner(&session_id, Some(node_id))
    }

    pub(in crate::workspace) fn release_binding_for_session(
        &self,
        session_id: &str,
    ) -> Option<String> {
        self.release_binding_for_session_inner(session_id, None)
    }

    pub(in crate::workspace) fn discard_binding(
        &self,
        session_id: &str,
        connection_id: &str,
        consumer: &ConnectionConsumer,
    ) {
        self.registry.stop_port_profiler(connection_id);
        self.ssh_registry.release(connection_id, consumer);
        self.binding_state()
            .remove_exact(session_id, connection_id, consumer);
    }

    pub(in crate::workspace) fn remember_binding(
        &self,
        binding: Option<(String, String, ConnectionConsumer)>,
        node_is_disconnected: bool,
    ) {
        let Some((session_id, connection_id, consumer)) = binding else {
            return;
        };
        if node_is_disconnected || !self.binding_is_current(&session_id, &connection_id) {
            // A late worker result cannot revive a consumer after explicit
            // node teardown or after NodeRouter moved to another connection.
            self.discard_binding(&session_id, &connection_id, &consumer);
            return;
        }

        let previous =
            self.binding_state()
                .replace(session_id, connection_id.clone(), consumer.clone());
        if let Some((previous_connection_id, previous_consumer)) = previous
            && (previous_connection_id != connection_id || previous_consumer != consumer)
        {
            // Reconnect swaps the logical consumer to the fresh node-owned
            // transport and releases the old connection reference.
            self.registry.stop_port_profiler(&previous_connection_id);
            self.ssh_registry
                .release(&previous_connection_id, &previous_consumer);
        }
    }

    fn release_binding_for_session_inner(
        &self,
        session_id: &str,
        node_id: Option<&NodeId>,
    ) -> Option<String> {
        let consumer = ConnectionConsumer::PortForward(session_id.to_string());
        let connection_id = if let Some((connection_id, stored_consumer)) =
            self.binding_state().remove(session_id)
        {
            self.ssh_registry.release(&connection_id, &stored_consumer);
            Some(connection_id)
        } else if let Some(manager) = self.registry.get(session_id) {
            // The manager may be registered before its worker delivery is
            // applied, so explicit disconnect also releases this fallback.
            let connection_id = manager.ssh_connection_handle().connection_id().to_string();
            self.ssh_registry.release(&connection_id, &consumer);
            Some(connection_id)
        } else if let Some(connection_id) =
            node_id.and_then(|node_id| self.node_router.connection_id_for_node(node_id))
        {
            self.ssh_registry.release(&connection_id, &consumer);
            Some(connection_id)
        } else {
            None
        };

        if let Some(connection_id) = connection_id.as_ref() {
            self.registry.stop_port_profiler(connection_id);
        }
        connection_id
    }

    fn binding_is_current(&self, session_id: &str, connection_id: &str) -> bool {
        if !self
            .registry
            .get(session_id)
            .is_some_and(|manager| manager.ssh_connection_handle().connection_id() == connection_id)
        {
            return false;
        }
        let Some(node_id) = node_id_from_forwarding_session(session_id) else {
            return true;
        };
        self.node_router
            .connection_id_for_node(&node_id)
            .is_some_and(|current_connection_id| current_connection_id == connection_id)
    }

    fn binding_state(&self) -> MutexGuard<'_, ForwardingBindingState> {
        // Preserve cleanup access after an unrelated panic; consumer release
        // must remain available during explicit node teardown.
        self.bindings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn node_id_from_forwarding_session(session_id: &str) -> Option<NodeId> {
    session_id
        .strip_prefix(FORWARDS_NODE_SESSION_PREFIX)
        .map(|raw_node_id| NodeId(raw_node_id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_state_replaces_and_removes_one_logical_consumer() {
        let mut state = ForwardingBindingState::default();
        let session_id = ForwardingRuntimeService::session_id_for_node(&NodeId::new("node-a"));
        let first_consumer = ConnectionConsumer::PortForward(session_id.clone());
        let second_consumer = first_consumer.clone();

        assert!(
            state
                .replace(
                    session_id.clone(),
                    "connection-a".to_string(),
                    first_consumer,
                )
                .is_none()
        );
        assert_eq!(
            state.replace(
                session_id.clone(),
                "connection-b".to_string(),
                second_consumer.clone(),
            ),
            Some((
                "connection-a".to_string(),
                ConnectionConsumer::PortForward(session_id.clone()),
            ))
        );
        assert_eq!(
            state.node_for_connection_id("connection-b"),
            Some(NodeId::new("node-a"))
        );
        assert_eq!(
            state.remove(&session_id),
            Some(("connection-b".to_string(), second_consumer))
        );
    }

    #[test]
    fn stale_exact_removal_does_not_delete_reconnected_binding() {
        let mut state = ForwardingBindingState::default();
        let session_id = ForwardingRuntimeService::session_id_for_node(&NodeId::new("node-b"));
        let consumer = ConnectionConsumer::PortForward(session_id.clone());
        state.replace(
            session_id.clone(),
            "connection-new".to_string(),
            consumer.clone(),
        );

        state.remove_exact(&session_id, "connection-old", &consumer);

        assert_eq!(
            state.connection_id(&session_id).as_deref(),
            Some("connection-new")
        );
    }
}
