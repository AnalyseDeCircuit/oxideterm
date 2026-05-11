#[derive(Clone, Debug)]
struct WorkspaceSshNode {
    saved_connection_id: Option<String>,
    config: SshConfig,
    title: String,
    terminal_ids: Vec<TerminalSessionId>,
    readiness: NodeReadiness,
}

#[derive(Clone, Debug)]
struct PendingSshTerminalOpen {
    node_id: NodeId,
    saved_connection_id: Option<String>,
    title: String,
}

#[derive(Debug)]
pub(super) enum ReconnectWorkerResult {
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
    FlushPendingReconnect {
        generation: u64,
    },
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
    },
    ActiveConnectionsProbed {
        changed: usize,
    },
}
