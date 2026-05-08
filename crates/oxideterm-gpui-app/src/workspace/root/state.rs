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
    GraceRecovered {
        node_id: NodeId,
        connection_id: String,
    },
    GraceExpired {
        node_id: NodeId,
        connection_id: String,
        detail: String,
    },
}
