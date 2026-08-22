use super::super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct WorkspaceSshNodeEndpoint {
    pub(in crate::workspace) host: String,
    pub(in crate::workspace) port: u16,
    pub(in crate::workspace) username: String,
}

impl WorkspaceSshNodeEndpoint {
    pub(in crate::workspace) fn from_config(config: &SshConfig) -> Self {
        // The UI mirror retains only display-safe endpoint metadata. Authentication
        // material remains in the node runtime and live connection owners.
        Self {
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct WorkspaceSshNode {
    pub(in crate::workspace) saved_connection_id: Option<String>,
    pub(in crate::workspace) endpoint: WorkspaceSshNodeEndpoint,
    pub(in crate::workspace) title: String,
    /// Retains terminal-only overrides for manual connections that have no saved owner.
    pub(in crate::workspace) terminal_options: ConnectionTerminalOptions,
    /// Manual nodes retain the SSH-only physical connection policy outside terminal overrides.
    pub(in crate::workspace) dedicated_new_terminal_connection: bool,
    pub(in crate::workspace) terminal_ids: Vec<TerminalSessionId>,
    pub(in crate::workspace) readiness: NodeReadiness,
}

impl WorkspaceSshNode {
    pub(in crate::workspace) fn new(
        saved_connection_id: Option<String>,
        config: &SshConfig,
        title: String,
        terminal_ids: Vec<TerminalSessionId>,
        readiness: NodeReadiness,
    ) -> Self {
        Self {
            saved_connection_id,
            endpoint: WorkspaceSshNodeEndpoint::from_config(config),
            title,
            terminal_options: ConnectionTerminalOptions::default(),
            dedicated_new_terminal_connection: false,
            terminal_ids,
            readiness,
        }
    }
}

#[derive(Debug)]
pub(in crate::workspace) enum ReconnectWorkerResult {
    NodeConnectionProgress {
        node_id: NodeId,
        stage: ConnectionTraceStage,
        attempt_id: runtime_entity::NodeTransportAttemptId,
    },
    NodeConnected {
        node_id: NodeId,
        connection_id: String,
        attempt_id: runtime_entity::NodeTransportAttemptId,
        job_id: Option<String>,
    },
    NodeConnectFailed {
        node_id: NodeId,
        connection_id: String,
        error: String,
        attempt_id: runtime_entity::NodeTransportAttemptId,
        job_id: Option<String>,
    },
    GraceRecovered {
        node_id: NodeId,
        connection_id: String,
        recovered_connections: Vec<(NodeId, String)>,
        job_id: String,
    },
    GraceExpired {
        node_id: NodeId,
        connection_id: String,
        detail: String,
        job_id: String,
    },
    SftpTransfersSnapshotted {
        node_id: NodeId,
        transfers_by_node: Vec<ReconnectNodeTransferSnapshot>,
        detail: String,
        job_id: String,
    },
    RemoteShellIntegrationGateFinished {
        node_id: NodeId,
        generation: u64,
        result: std::result::Result<(RemoteShellIntegrationStatus, bool), ()>,
    },
    RemoteShellIntegrationMaintenanceFinished {
        action: settings::RemoteShellIntegrationAction,
        node_id: NodeId,
        generation: u64,
        result: std::result::Result<RemoteShellIntegrationStatus, ()>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_ssh_node_keeps_only_non_secret_endpoint_metadata() {
        let config = SshConfig::password(
            "runtime.example.test",
            2202,
            "operator",
            "node-secret-sentinel",
        );
        let node = WorkspaceSshNode::new(
            None,
            &config,
            "Runtime".to_string(),
            Vec::new(),
            NodeReadiness::Disconnected,
        );

        assert_eq!(node.endpoint.host, "runtime.example.test");
        assert_eq!(node.endpoint.port, 2202);
        assert_eq!(node.endpoint.username, "operator");
        // Debug output represents the UI projection and must never retain auth material.
        assert!(!format!("{node:?}").contains("node-secret-sentinel"));
    }
}
