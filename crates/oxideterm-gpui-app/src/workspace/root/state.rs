#[derive(Clone, Debug)]
struct WorkspaceSshNode {
    saved_connection_id: Option<String>,
    config: SshConfig,
    title: String,
    terminal_ids: Vec<TerminalSessionId>,
    readiness: NodeReadiness,
}

#[derive(Debug)]
pub(super) enum ReconnectWorkerResult {
    NodeConnected {
        node_id: NodeId,
        connection_id: String,
    },
    NodeConnectFailed {
        node_id: NodeId,
        error: String,
    },
    GraceRecovered {
        node_id: NodeId,
        connection_id: String,
    },
    GraceExpired {
        node_id: NodeId,
        connection_id: String,
        detail: String,
    },
    SftpTransfersSnapshotted {
        node_id: NodeId,
        transfers_by_node: Vec<ReconnectNodeTransferSnapshot>,
        detail: String,
    },
    ForwardRulesRestored {
        node_id: NodeId,
        result: PhaseResult,
        restored: u32,
        detail: String,
    },
}
