use super::super::*;

#[derive(Clone, Debug)]
pub(in crate::workspace) struct WorkspaceSshNode {
    pub(in crate::workspace) saved_connection_id: Option<String>,
    pub(in crate::workspace) config: SshConfig,
    pub(in crate::workspace) title: String,
    pub(in crate::workspace) terminal_ids: Vec<TerminalSessionId>,
    pub(in crate::workspace) readiness: NodeReadiness,
}

#[derive(Clone, Debug)]
pub(in crate::workspace) struct PendingSshTerminalOpen {
    pub(in crate::workspace) node_id: NodeId,
    pub(in crate::workspace) post_connect_command: Option<String>,
    pub(in crate::workspace) mark_used_connection_id: Option<String>,
    pub(in crate::workspace) save_after_open: Option<SaveConnectionRequest>,
    pub(in crate::workspace) cleanup_node_id: Option<NodeId>,
    pub(in crate::workspace) title: String,
}

#[derive(Debug)]
pub(in crate::workspace) enum ReconnectWorkerResult {
    NodeConnected {
        node_id: NodeId,
        connection_id: String,
        job_id: Option<String>,
    },
    NodeConnectFailed {
        node_id: NodeId,
        error: String,
        job_id: Option<String>,
    },
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
    ForwardRulesRestored {
        node_id: NodeId,
        result: PhaseResult,
        restored: u32,
        detail: String,
        job_id: String,
        created_forwards: Vec<(String, String)>,
        bindings: Vec<(String, String, ConnectionConsumer)>,
    },
    RemoteShellIntegrationGateFinished {
        node_id: NodeId,
        result: std::result::Result<(RemoteShellIntegrationStatus, bool), String>,
    },
}
