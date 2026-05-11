#[derive(Default)]
struct AgentRegistry {
    // Tauri keys remote agents by SSH connection id, not by node id or IDE tab.
    // Reconnect creates a new connection id, so stale agent channels cannot
    // make a node look alive or serve requests after the node has moved on.
    sessions: DashMap<String, Arc<AgentSession>>,
}

impl AgentRegistry {
    fn register(&self, connection_id: String, session: AgentSession) {
        self.sessions.insert(connection_id, Arc::new(session));
    }

    fn get(&self, connection_id: &str) -> Option<Arc<AgentSession>> {
        self.sessions
            .get(connection_id)
            .map(|session| session.value().clone())
    }

    fn remove_without_shutdown(&self, connection_id: &str) {
        self.sessions.remove(connection_id);
    }

    async fn remove(&self, connection_id: &str) {
        if let Some((_, session)) = self.sessions.remove(connection_id) {
            session.shutdown().await;
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum AgentError {
    #[error("Agent channel closed")]
    ChannelClosed,
    #[error("Agent RPC timeout after {0}s")]
    Timeout(u64),
    #[error("Failed to serialize agent request: {0}")]
    Serialize(String),
    #[error("Failed to deserialize agent response: {0}")]
    Deserialize(String),
    #[error("Agent RPC error {code}: {message}")]
    Rpc { code: i32, message: String },
    #[error("SSH error: {0}")]
    Ssh(String),
    #[error("SFTP error: {0}")]
    Sftp(String),
    #[error("Unsupported architecture: {0}")]
    UnsupportedArch(String),
    #[error("Agent binary not found: {0}")]
    BinaryNotFound(String),
    #[error("Local I/O error: {0}")]
    LocalIo(String),
    #[error("Route error: {0}")]
    Route(String),
    #[error("Handshake failed: {0}")]
    Handshake(String),
}

impl From<AgentRpcError> for AgentError {
    fn from(error: AgentRpcError) -> Self {
        Self::Rpc {
            code: error.code,
            message: error.message,
        }
    }
}

impl From<SftpError> for AgentError {
    fn from(error: SftpError) -> Self {
        Self::Sftp(error.to_string())
    }
}

impl From<oxideterm_ssh::RouteError> for AgentError {
    fn from(error: oxideterm_ssh::RouteError) -> Self {
        Self::Route(error.to_string())
    }
}

async fn detect_arch(handle: &SshConnectionHandle) -> Result<String, AgentError> {
    let arch = handle
        .run_command("uname -m", Duration::from_secs(10), 512)
        .await
        .map_err(|error| AgentError::Ssh(error.to_string()))?
        .trim()
        .to_string();
    if arch.is_empty() {
        Err(AgentError::UnsupportedArch("unknown".to_string()))
    } else {
        Ok(arch)
    }
}

async fn remote_agent_path(handle: &SshConnectionHandle) -> Result<String, AgentError> {
    let home = handle
        .run_command("echo \"$HOME\"", Duration::from_secs(10), 1024)
        .await
        .map_err(|error| AgentError::Ssh(error.to_string()))?
        .trim()
        .to_string();
    if home.is_empty() || !home.starts_with('/') {
        return Err(AgentError::Ssh(format!(
            "Cannot resolve HOME directory on remote host (got {home:?})"
        )));
    }
    Ok(format!("{home}/{AGENT_REMOTE_DIR}/{AGENT_BINARY_NAME}"))
}
