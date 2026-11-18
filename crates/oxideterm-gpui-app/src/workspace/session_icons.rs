use super::WorkspaceApp;
pub(super) use oxideterm_gpui_connections::icons::{SessionIcon, resolved_session_icon, session_icon_from_id};

impl WorkspaceApp {
    pub(in crate::workspace) fn node_session_icon(
        &self,
        node_id: &oxideterm_ssh::NodeId,
    ) -> SessionIcon {
        let manual = self
            .ssh_nodes
            .get(node_id)
            .and_then(|node| node.saved_connection_id.as_deref())
            .and_then(|id| self.connection_store.get(id))
            .and_then(|connection| connection.icon.as_deref());
        let environment = self
            .node_router
            .connection_id_for_node(node_id)
            .and_then(|id| self.ssh_registry.get(&id))
            .and_then(|handle| handle.remote_env());
        resolved_session_icon(
            manual,
            environment
                .as_ref()
                .and_then(|env| env.os_version.as_deref()),
        )
    }
}
