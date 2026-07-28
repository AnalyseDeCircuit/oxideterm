fn readiness_for_connection(connection: &ConnectionInfo) -> NodeReadiness {
    readiness_for_connection_state(&connection.state)
}

fn connection_info_for_runtime(handle: &SshConnectionHandle) -> ConnectionInfo {
    let mut connection = handle.info();
    if matches!(
        connection.state,
        ConnectionState::Active | ConnectionState::Idle
    ) && !handle.has_physical()
    {
        // Registry state alone cannot make a node usable without a physical transport.
        connection.state =
            ConnectionState::Error("registry connection has no physical SSH transport".to_string());
    }
    connection
}

fn readiness_for_connection_state(state: &ConnectionState) -> NodeReadiness {
    match state {
        ConnectionState::Active | ConnectionState::Idle => NodeReadiness::Ready,
        ConnectionState::Connecting | ConnectionState::Reconnecting => NodeReadiness::Connecting,
        ConnectionState::Error(_) | ConnectionState::LinkDown => NodeReadiness::Error,
        ConnectionState::Disconnecting | ConnectionState::Disconnected => {
            NodeReadiness::Disconnected
        }
    }
}

fn generated_tree_node_id(prefix: &str) -> NodeId {
    NodeId::new(format!("{prefix}-{}", Uuid::new_v4()))
}

fn sftp_route_error(prefix: &str, error: SftpError) -> RouteError {
    RouteError::CapabilityUnavailable(format!("{prefix}: {error}"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
