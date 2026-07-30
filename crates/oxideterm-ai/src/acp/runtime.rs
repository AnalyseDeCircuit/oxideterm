use std::{
    collections::HashMap,
    fmt,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use agent_client_protocol::schema::v1::{
    AuthMethod, DeleteSessionRequest, ErrorCode, ListSessionsRequest, ListSessionsResponse,
    McpServer, PromptRequest,
};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{
    sync::{mpsc, oneshot},
    task::AbortHandle,
};
use zeroize::Zeroizing;

use super::{
    AcpActiveSession, AcpAgentRuntime, AcpClientEvent, AcpHostCapabilityPolicy, AcpLaunchConfig,
    AcpLaunchConfigError, AcpPromptSessionOutcome, AcpSessionConfigSelection,
    acp_session_config_options, acp_session_mode_state, build_acp_stdio_launcher,
    with_acp_agent_runtime_events,
};

static ACP_CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Identifies the lifecycle of one external ACP agent connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcpConnectionState {
    Connecting,
    Ready,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcpControlOperation {
    Config,
    Mode,
}

/// Events produced by the persistent ACP connection owner.
pub enum AcpManagedEvent {
    ConnectionState {
        agent_id: String,
        connection_id: u64,
        state: AcpConnectionState,
        message: Option<String>,
    },
    Diagnostic {
        agent_id: String,
        connection_id: u64,
        message: String,
    },
    AuthenticationMethods {
        agent_id: String,
        connection_id: u64,
        methods: Vec<super::AcpAuthMethod>,
    },
    AuthenticationFinished {
        agent_id: String,
        connection_id: u64,
        result: Result<(), AcpConnectionError>,
    },
    SessionReady {
        agent_id: String,
        thread_id: String,
        turn_id: String,
        outcome: AcpPromptSessionOutcome,
    },
    ConfigUpdated {
        agent_id: String,
        connection_id: u64,
        thread_id: String,
        config_options: Vec<super::AcpSessionConfigOption>,
    },
    ModeUpdated {
        agent_id: String,
        connection_id: u64,
        thread_id: String,
        mode_id: String,
    },
    ControlFailed {
        agent_id: String,
        connection_id: u64,
        thread_id: String,
        operation: AcpControlOperation,
        error: AcpConnectionError,
    },
    Client {
        agent_id: String,
        connection_id: u64,
        event: AcpClientEvent,
    },
    TurnFinished {
        agent_id: String,
        thread_id: String,
        turn_id: String,
        result: Result<AcpPromptSessionOutcome, AcpConnectionError>,
    },
}

impl fmt::Debug for AcpManagedEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConnectionState {
                agent_id,
                connection_id,
                state,
                message,
            } => formatter
                .debug_struct("ConnectionState")
                .field("agent_id", agent_id)
                .field("connection_id", connection_id)
                .field("state", state)
                .field("message", &message.as_ref().map(|_| "<redacted>"))
                .finish(),
            Self::Diagnostic {
                agent_id,
                connection_id,
                ..
            } => formatter
                .debug_struct("Diagnostic")
                .field("agent_id", agent_id)
                .field("connection_id", connection_id)
                .field("message", &"<redacted>")
                .finish(),
            Self::AuthenticationMethods {
                agent_id,
                connection_id,
                methods,
            } => formatter
                .debug_struct("AuthenticationMethods")
                .field("agent_id", agent_id)
                .field("connection_id", connection_id)
                .field("method_count", &methods.len())
                .finish(),
            Self::AuthenticationFinished {
                agent_id,
                connection_id,
                result,
            } => formatter
                .debug_struct("AuthenticationFinished")
                .field("agent_id", agent_id)
                .field("connection_id", connection_id)
                .field("result", &result.as_ref().map(|_| ()))
                .finish(),
            Self::SessionReady {
                agent_id,
                thread_id,
                turn_id,
                outcome,
            } => formatter
                .debug_struct("SessionReady")
                .field("agent_id", agent_id)
                .field("thread_id", thread_id)
                .field("turn_id", turn_id)
                .field("session_id", &outcome.session_id)
                .finish(),
            Self::ConfigUpdated {
                agent_id,
                connection_id,
                thread_id,
                config_options,
            } => formatter
                .debug_struct("ConfigUpdated")
                .field("agent_id", agent_id)
                .field("connection_id", connection_id)
                .field("thread_id", thread_id)
                .field("config_options", &config_options.len())
                .finish(),
            Self::ModeUpdated {
                agent_id,
                connection_id,
                thread_id,
                mode_id,
            } => formatter
                .debug_struct("ModeUpdated")
                .field("agent_id", agent_id)
                .field("connection_id", connection_id)
                .field("thread_id", thread_id)
                .field("mode_id", mode_id)
                .finish(),
            Self::ControlFailed {
                agent_id,
                connection_id,
                thread_id,
                operation,
                error,
            } => formatter
                .debug_struct("ControlFailed")
                .field("agent_id", agent_id)
                .field("connection_id", connection_id)
                .field("thread_id", thread_id)
                .field("operation", operation)
                .field("error", error)
                .finish(),
            Self::Client {
                agent_id,
                connection_id,
                ..
            } => formatter
                .debug_struct("Client")
                .field("agent_id", agent_id)
                .field("connection_id", connection_id)
                .field("event", &"<redacted>")
                .finish(),
            Self::TurnFinished {
                agent_id,
                thread_id,
                turn_id,
                result,
            } => formatter
                .debug_struct("TurnFinished")
                .field("agent_id", agent_id)
                .field("thread_id", thread_id)
                .field("turn_id", turn_id)
                .field(
                    "result",
                    &result.as_ref().map(|outcome| &outcome.session_id),
                )
                .finish(),
        }
    }
}

/// One prompt submitted to a connection-owned ACP thread.
pub struct AcpManagedPromptRequest {
    pub thread_id: String,
    pub turn_id: String,
    pub existing_session_id: Option<String>,
    pub cwd: PathBuf,
    pub config_selections: Vec<AcpSessionConfigSelection>,
    pub mode_id: Option<String>,
    pub mcp_servers: Vec<McpServer>,
    pub prompt: Zeroizing<String>,
}

impl fmt::Debug for AcpManagedPromptRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcpManagedPromptRequest")
            .field("thread_id", &self.thread_id)
            .field("turn_id", &self.turn_id)
            .field("existing_session_id", &self.existing_session_id)
            .field("cwd", &self.cwd)
            .field("config_selection_count", &self.config_selections.len())
            .field("has_mode_selection", &self.mode_id.is_some())
            .field("mcp_server_count", &self.mcp_servers.len())
            .field("prompt", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Error, Eq, PartialEq)]
pub enum AcpConnectionError {
    #[error("{0}")]
    Launch(String),
    #[error("{0}")]
    Protocol(String),
    #[error("ACP agent connection is unavailable")]
    Unavailable,
    #[error("ACP thread already has a running turn")]
    TurnAlreadyRunning,
    #[error("ACP agent authentication is required")]
    AuthRequired,
    #[error("ACP authentication method requires client-side setup")]
    AuthSetupRequired,
}

impl fmt::Debug for AcpConnectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::Launch(_) => "Launch",
            Self::Protocol(_) => "Protocol",
            Self::Unavailable => "Unavailable",
            Self::TurnAlreadyRunning => "TurnAlreadyRunning",
            Self::AuthRequired => "AuthRequired",
            Self::AuthSetupRequired => "AuthSetupRequired",
        };
        formatter
            .debug_struct("AcpConnectionError")
            .field("kind", &kind)
            .finish()
    }
}

impl From<AcpLaunchConfigError> for AcpConnectionError {
    fn from(error: AcpLaunchConfigError) -> Self {
        Self::Launch(error.to_string())
    }
}

struct AcpConnectionEntry {
    connection_id: u64,
    fingerprint: [u8; 32],
    command_tx: mpsc::UnboundedSender<AcpConnectionCommand>,
    abort_handle: AbortHandle,
}

struct AcpConnectionManagerInner {
    connections: Mutex<HashMap<String, AcpConnectionEntry>>,
    event_tx: mpsc::UnboundedSender<AcpManagedEvent>,
    client_version: String,
}

impl Drop for AcpConnectionManagerInner {
    fn drop(&mut self) {
        // The manager is the lifecycle owner. Dropping it must not detach
        // external agents or leave their stdio processes running.
        for (_, entry) in self.connections.get_mut().drain() {
            let _ = entry.command_tx.send(AcpConnectionCommand::Shutdown);
            entry.abort_handle.abort();
        }
    }
}

/// Owns one long-lived process per configured ACP agent and several sessions per process.
#[derive(Clone)]
pub struct AcpConnectionManager {
    inner: Arc<AcpConnectionManagerInner>,
}

impl fmt::Debug for AcpConnectionManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcpConnectionManager")
            .field("connection_count", &self.inner.connections.lock().len())
            .finish()
    }
}

impl AcpConnectionManager {
    pub fn new(
        client_version: impl Into<String>,
        event_tx: mpsc::UnboundedSender<AcpManagedEvent>,
    ) -> Self {
        Self {
            inner: Arc::new(AcpConnectionManagerInner {
                connections: Mutex::new(HashMap::new()),
                event_tx,
                client_version: client_version.into(),
            }),
        }
    }

    pub async fn prompt(
        &self,
        launch_config: AcpLaunchConfig,
        policy: AcpHostCapabilityPolicy,
        request: AcpManagedPromptRequest,
    ) -> Result<AcpPromptSessionOutcome, AcpConnectionError> {
        let agent_id = launch_config.id.clone();
        let thread_id = request.thread_id.clone();
        let turn_id = request.turn_id.clone();
        let command_tx = match self.connection_sender(launch_config, policy) {
            Ok(command_tx) => command_tx,
            Err(error) => {
                self.emit_turn_setup_failure(agent_id, thread_id, turn_id, error.clone());
                return Err(error);
            }
        };
        let (response_tx, response_rx) = oneshot::channel();
        if command_tx
            .send(AcpConnectionCommand::Prompt {
                request,
                response_tx,
            })
            .is_err()
        {
            let error = AcpConnectionError::Unavailable;
            self.emit_turn_setup_failure(agent_id, thread_id, turn_id, error.clone());
            return Err(error);
        }
        match response_rx.await {
            Ok(result) => result,
            Err(_) => {
                let error = AcpConnectionError::Unavailable;
                self.emit_turn_setup_failure(agent_id, thread_id, turn_id, error.clone());
                Err(error)
            }
        }
    }

    pub fn cancel(&self, agent_id: &str, thread_id: &str) -> Result<(), AcpConnectionError> {
        self.send_existing(
            agent_id,
            AcpConnectionCommand::Cancel {
                thread_id: thread_id.to_string(),
            },
        )
        .map(|_| ())
    }

    pub async fn set_config_selection(
        &self,
        agent_id: &str,
        thread_id: &str,
        selection: AcpSessionConfigSelection,
    ) -> Result<Vec<super::AcpSessionConfigOption>, AcpConnectionError> {
        let (response_tx, response_rx) = oneshot::channel();
        let connection_id = self.send_existing(
            agent_id,
            AcpConnectionCommand::SetConfig {
                thread_id: thread_id.to_string(),
                selection,
                response_tx,
            },
        )?;
        let config_options = response_rx
            .await
            .map_err(|_| AcpConnectionError::Unavailable)?;
        match config_options {
            Ok(config_options) => {
                let _ = self.inner.event_tx.send(AcpManagedEvent::ConfigUpdated {
                    agent_id: agent_id.to_string(),
                    connection_id,
                    thread_id: thread_id.to_string(),
                    config_options: config_options.clone(),
                });
                Ok(config_options)
            }
            Err(error) => {
                let _ = self.inner.event_tx.send(AcpManagedEvent::ControlFailed {
                    agent_id: agent_id.to_string(),
                    connection_id,
                    thread_id: thread_id.to_string(),
                    operation: AcpControlOperation::Config,
                    error: error.clone(),
                });
                Err(error)
            }
        }
    }

    pub async fn set_mode(
        &self,
        agent_id: &str,
        thread_id: &str,
        mode_id: String,
    ) -> Result<(), AcpConnectionError> {
        let (response_tx, response_rx) = oneshot::channel();
        let connection_id = self.send_existing(
            agent_id,
            AcpConnectionCommand::SetMode {
                thread_id: thread_id.to_string(),
                mode_id: mode_id.clone(),
                response_tx,
            },
        )?;
        let result = response_rx
            .await
            .map_err(|_| AcpConnectionError::Unavailable)?;
        match result {
            Ok(()) => {
                let _ = self.inner.event_tx.send(AcpManagedEvent::ModeUpdated {
                    agent_id: agent_id.to_string(),
                    connection_id,
                    thread_id: thread_id.to_string(),
                    mode_id,
                });
                Ok(())
            }
            Err(error) => {
                let _ = self.inner.event_tx.send(AcpManagedEvent::ControlFailed {
                    agent_id: agent_id.to_string(),
                    connection_id,
                    thread_id: thread_id.to_string(),
                    operation: AcpControlOperation::Mode,
                    error: error.clone(),
                });
                Err(error)
            }
        }
    }

    pub async fn list_sessions(
        &self,
        agent_id: &str,
        request: ListSessionsRequest,
    ) -> Result<ListSessionsResponse, AcpConnectionError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.send_existing(
            agent_id,
            AcpConnectionCommand::ListSessions {
                request,
                response_tx,
            },
        )?;
        response_rx
            .await
            .map_err(|_| AcpConnectionError::Unavailable)?
    }

    pub async fn authenticate(
        &self,
        agent_id: &str,
        method_id: String,
    ) -> Result<(), AcpConnectionError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.send_existing(
            agent_id,
            AcpConnectionCommand::Authenticate {
                method_id,
                response_tx,
            },
        )?;
        response_rx
            .await
            .map_err(|_| AcpConnectionError::Unavailable)?
    }

    pub async fn close_thread(
        &self,
        agent_id: &str,
        thread_id: &str,
        delete_remote: bool,
    ) -> Result<(), AcpConnectionError> {
        let (response_tx, response_rx) = oneshot::channel();
        self.send_existing(
            agent_id,
            AcpConnectionCommand::CloseThread {
                thread_id: thread_id.to_string(),
                delete_remote,
                response_tx,
            },
        )?;
        response_rx
            .await
            .map_err(|_| AcpConnectionError::Unavailable)?
    }

    pub fn shutdown_agent(&self, agent_id: &str) {
        let entry = self.inner.connections.lock().remove(agent_id);
        if let Some(entry) = entry {
            let _ = entry.command_tx.send(AcpConnectionCommand::Shutdown);
            entry.abort_handle.abort();
        }
    }

    fn emit_turn_setup_failure(
        &self,
        agent_id: String,
        thread_id: String,
        turn_id: String,
        error: AcpConnectionError,
    ) {
        let _ = self.inner.event_tx.send(AcpManagedEvent::TurnFinished {
            agent_id,
            thread_id,
            turn_id,
            result: Err(error),
        });
    }

    fn send_existing(
        &self,
        agent_id: &str,
        command: AcpConnectionCommand,
    ) -> Result<u64, AcpConnectionError> {
        let (connection_id, command_tx) = self
            .inner
            .connections
            .lock()
            .get(agent_id)
            .filter(|entry| !entry.command_tx.is_closed())
            .map(|entry| (entry.connection_id, entry.command_tx.clone()))
            .ok_or(AcpConnectionError::Unavailable)?;
        command_tx
            .send(command)
            .map_err(|_| AcpConnectionError::Unavailable)?;
        Ok(connection_id)
    }

    fn connection_sender(
        &self,
        launch_config: AcpLaunchConfig,
        policy: AcpHostCapabilityPolicy,
    ) -> Result<mpsc::UnboundedSender<AcpConnectionCommand>, AcpConnectionError> {
        let agent_id = launch_config.id.clone();
        let fingerprint = launch_fingerprint(&launch_config, &policy);
        let mut connections = self.inner.connections.lock();
        if let Some(entry) = connections.get(&agent_id)
            && entry.fingerprint == fingerprint
            && !entry.command_tx.is_closed()
        {
            // The unused launch owner is dropped here and zeroizes its secret-bearing fields.
            return Ok(entry.command_tx.clone());
        }
        if let Some(stale) = connections.remove(&agent_id) {
            let _ = stale.command_tx.send(AcpConnectionCommand::Shutdown);
            stale.abort_handle.abort();
        }

        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let event_tx = self.inner.event_tx.clone();
        let client_version = self.inner.client_version.clone();
        let runtime_agent_id = agent_id.clone();
        let connection_id = ACP_CONNECTION_COUNTER.fetch_add(1, Ordering::Relaxed);
        let task = tokio::spawn(async move {
            run_connection(
                runtime_agent_id,
                connection_id,
                launch_config,
                client_version,
                policy,
                command_rx,
                event_tx,
            )
            .await;
        });
        connections.insert(
            agent_id,
            AcpConnectionEntry {
                connection_id,
                fingerprint,
                command_tx: command_tx.clone(),
                abort_handle: task.abort_handle(),
            },
        );
        drop(task);
        Ok(command_tx)
    }
}

enum AcpConnectionCommand {
    Prompt {
        request: AcpManagedPromptRequest,
        response_tx: oneshot::Sender<Result<AcpPromptSessionOutcome, AcpConnectionError>>,
    },
    Cancel {
        thread_id: String,
    },
    SetConfig {
        thread_id: String,
        selection: AcpSessionConfigSelection,
        response_tx:
            oneshot::Sender<Result<Vec<super::AcpSessionConfigOption>, AcpConnectionError>>,
    },
    SetMode {
        thread_id: String,
        mode_id: String,
        response_tx: oneshot::Sender<Result<(), AcpConnectionError>>,
    },
    ListSessions {
        request: ListSessionsRequest,
        response_tx: oneshot::Sender<Result<ListSessionsResponse, AcpConnectionError>>,
    },
    Authenticate {
        method_id: String,
        response_tx: oneshot::Sender<Result<(), AcpConnectionError>>,
    },
    CloseThread {
        thread_id: String,
        delete_remote: bool,
        response_tx: oneshot::Sender<Result<(), AcpConnectionError>>,
    },
    Shutdown,
}

struct AcpManagedSession {
    session: AcpActiveSession,
    metadata: Option<serde_json::Value>,
}

struct AcpCompletedTurn {
    thread_id: String,
    turn_id: String,
    result: Result<AcpPromptSessionOutcome, AcpConnectionError>,
    response_tx: oneshot::Sender<Result<AcpPromptSessionOutcome, AcpConnectionError>>,
}

struct AcpPendingClose {
    delete_remote: bool,
    response_tx: oneshot::Sender<Result<(), AcpConnectionError>>,
}

async fn run_connection(
    agent_id: String,
    connection_id: u64,
    launch_config: AcpLaunchConfig,
    client_version: String,
    policy: AcpHostCapabilityPolicy,
    command_rx: mpsc::UnboundedReceiver<AcpConnectionCommand>,
    event_tx: mpsc::UnboundedSender<AcpManagedEvent>,
) {
    let _ = event_tx.send(AcpManagedEvent::ConnectionState {
        agent_id: agent_id.clone(),
        connection_id,
        state: AcpConnectionState::Connecting,
        message: None,
    });
    let launcher = match build_acp_stdio_launcher(launch_config) {
        Ok(launcher) => launcher,
        Err(error) => {
            let _ = event_tx.send(AcpManagedEvent::ConnectionState {
                agent_id,
                connection_id,
                state: AcpConnectionState::Failed,
                message: Some(error.to_string()),
            });
            return;
        }
    };
    const DIAGNOSTIC_CHANNEL_CAPACITY: usize = 100;

    let (diagnostic_tx, mut diagnostic_rx) = mpsc::channel(DIAGNOSTIC_CHANNEL_CAPACITY);
    let diagnostic_agent_id = agent_id.clone();
    let diagnostic_event_tx = event_tx.clone();
    let diagnostic_relay = tokio::spawn(async move {
        while let Some(message) = diagnostic_rx.recv().await {
            if diagnostic_event_tx
                .send(AcpManagedEvent::Diagnostic {
                    agent_id: diagnostic_agent_id.clone(),
                    connection_id,
                    message,
                })
                .is_err()
            {
                break;
            }
        }
    });
    let launcher = launcher.with_diagnostic_sender(diagnostic_tx);
    let (client_event_tx, mut client_event_rx) = mpsc::unbounded_channel();
    let relay_agent_id = agent_id.clone();
    let relay_event_tx = event_tx.clone();
    let relay = tokio::spawn(async move {
        while let Some(event) = client_event_rx.recv().await {
            if relay_event_tx
                .send(AcpManagedEvent::Client {
                    agent_id: relay_agent_id.clone(),
                    connection_id,
                    event,
                })
                .is_err()
            {
                break;
            }
        }
    });
    let runtime_agent_id = agent_id.clone();
    let runtime_event_tx = event_tx.clone();
    let result = with_acp_agent_runtime_events(
        launcher,
        client_version,
        policy,
        client_event_tx,
        async move |runtime| {
            let _ = runtime_event_tx.send(AcpManagedEvent::AuthenticationMethods {
                agent_id: runtime_agent_id.clone(),
                connection_id,
                methods: super::acp_auth_methods(runtime.auth_methods()),
            });
            let _ = runtime_event_tx.send(AcpManagedEvent::ConnectionState {
                agent_id: runtime_agent_id.clone(),
                connection_id,
                state: AcpConnectionState::Ready,
                message: None,
            });
            run_connection_commands(
                runtime_agent_id,
                connection_id,
                runtime,
                command_rx,
                runtime_event_tx,
            )
            .await
        },
    )
    .await;
    relay.abort();
    let _ = relay.await;
    diagnostic_relay.abort();
    let _ = diagnostic_relay.await;
    let (state, message) = match result {
        Ok(()) => (AcpConnectionState::Stopped, None),
        Err(error) => (
            AcpConnectionState::Failed,
            Some(super::super::sanitize_for_persistence(&error.to_string())),
        ),
    };
    let _ = event_tx.send(AcpManagedEvent::ConnectionState {
        agent_id,
        connection_id,
        state,
        message,
    });
}

async fn run_connection_commands(
    agent_id: String,
    connection_id: u64,
    runtime: AcpAgentRuntime,
    mut command_rx: mpsc::UnboundedReceiver<AcpConnectionCommand>,
    event_tx: mpsc::UnboundedSender<AcpManagedEvent>,
) -> Result<(), agent_client_protocol::Error> {
    let mut sessions = HashMap::<String, AcpManagedSession>::new();
    let mut running_turns = HashMap::<String, String>::new();
    let mut pending_closes = HashMap::<String, AcpPendingClose>::new();
    let (completion_tx, mut completion_rx) = mpsc::unbounded_channel::<AcpCompletedTurn>();

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else {
                    return Ok(());
                };
                match command {
                    AcpConnectionCommand::Prompt { request, response_tx } => {
                        let AcpManagedPromptRequest {
                            thread_id,
                            turn_id,
                            existing_session_id,
                            cwd,
                            config_selections,
                            mode_id,
                            mcp_servers,
                            prompt,
                        } = request;
                        if running_turns.contains_key(&thread_id) {
                            finish_prompt_with_error(
                                &event_tx,
                                &agent_id,
                                thread_id,
                                turn_id,
                                response_tx,
                                AcpConnectionError::TurnAlreadyRunning,
                            );
                            continue;
                        }
                        if !sessions.contains_key(&thread_id) {
                            let session = match runtime
                                .start_or_resume_session(existing_session_id, cwd, mcp_servers)
                                .await
                            {
                                Ok(session) => session,
                                Err(error) => {
                                    finish_prompt_with_error(
                                        &event_tx,
                                        &agent_id,
                                        thread_id,
                                        turn_id,
                                        response_tx,
                                        protocol_error(error),
                                    );
                                    continue;
                                }
                            };
                            let metadata = session.meta().clone().map(serde_json::Value::Object);
                            sessions.insert(
                                thread_id.clone(),
                                AcpManagedSession { session, metadata },
                            );
                        }
                        let session = sessions
                            .get_mut(&thread_id)
                            .expect("ACP session was inserted before prompt configuration");
                        if let Err(error) =
                            apply_config_selections(&runtime, &mut session.session, &config_selections)
                                .await
                        {
                            finish_prompt_with_error(
                                &event_tx,
                                &agent_id,
                                thread_id,
                                turn_id,
                                response_tx,
                                protocol_error(error),
                            );
                            continue;
                        }
                        if let Err(error) =
                            apply_mode_selection(&runtime, &mut session.session, mode_id.as_deref())
                                .await
                        {
                            finish_prompt_with_error(
                                &event_tx,
                                &agent_id,
                                thread_id,
                                turn_id,
                                response_tx,
                                protocol_error(error),
                            );
                            continue;
                        }
                        let outcome = session_outcome(session);
                        let _ = event_tx.send(AcpManagedEvent::SessionReady {
                            agent_id: agent_id.clone(),
                            thread_id: thread_id.clone(),
                            turn_id: turn_id.clone(),
                            outcome: outcome.clone(),
                        });
                        running_turns.insert(thread_id.clone(), turn_id.clone());
                        let prompt_connection = runtime.connection.clone();
                        let prompt_session_id = session.session.session_id().clone();
                        let completed_outcome = outcome.clone();
                        let completed_tx = completion_tx.clone();
                        let _ = runtime.connection.clone().spawn(async move {
                            // The ACP wire DTO requires an owned String. Keep the
                            // zeroizing source alive only until the request has
                            // been queued and never copy it into diagnostics.
                            let result = prompt_connection
                                .send_request(PromptRequest::new(
                                    prompt_session_id,
                                    vec![prompt.as_str().to_string().into()],
                                ))
                                .block_task()
                                .await
                                .map(|_| completed_outcome)
                                .map_err(protocol_error);
                            let _ = completed_tx.send(AcpCompletedTurn {
                                thread_id,
                                turn_id,
                                result,
                                response_tx,
                            });
                            Ok(())
                        });
                    }
                    AcpConnectionCommand::Cancel { thread_id } => {
                        if let Some(session) = sessions.get(&thread_id) {
                            // Cancellation failure is reported by the active
                            // prompt request; it must not tear down unrelated
                            // sessions sharing this agent connection.
                            let _ = runtime.cancel_session(session.session.session_id().clone());
                        }
                    }
                    AcpConnectionCommand::SetConfig {
                        thread_id,
                        selection,
                        response_tx,
                    } => {
                        let result = match sessions.get_mut(&thread_id) {
                            Some(session) => {
                                let Some(value) =
                                    session.session.config_value_for_selection(&selection)
                                else {
                                    let _ = response_tx.send(Err(
                                        AcpConnectionError::Protocol(
                                            "ACP session config selection is unavailable"
                                                .to_string(),
                                        ),
                                    ));
                                    continue;
                                };
                                runtime
                                    .set_session_config_value(
                                        session.session.session_id().clone(),
                                        selection.config_id,
                                        value,
                                    )
                                    .await
                                    .map(|response| {
                                        session.session.replace_config_options(
                                            response.config_options.clone(),
                                        );
                                        acp_session_config_options(&response.config_options)
                                    })
                                    .map_err(protocol_error)
                            }
                            None => Err(AcpConnectionError::Unavailable),
                        };
                        let _ = response_tx.send(result);
                    }
                    AcpConnectionCommand::SetMode {
                        thread_id,
                        mode_id,
                        response_tx,
                    } => {
                        let result = match sessions.get_mut(&thread_id) {
                            Some(session) => {
                                let mode_exists = session
                                    .session
                                    .modes()
                                    .is_some_and(|modes| {
                                        modes
                                            .available_modes
                                            .iter()
                                            .any(|mode| mode.id.to_string() == mode_id)
                                    });
                                if !mode_exists {
                                    Err(AcpConnectionError::Protocol(
                                        "ACP session mode is unavailable".to_string(),
                                    ))
                                } else {
                                    runtime
                                        .set_session_mode(
                                            session.session.session_id().clone(),
                                            mode_id.clone(),
                                        )
                                        .await
                                        .map(|_| {
                                            if let Some(modes) = session.session.modes.as_mut() {
                                                modes.current_mode_id = mode_id.into();
                                            }
                                        })
                                        .map_err(protocol_error)
                                }
                            }
                            None => Err(AcpConnectionError::Unavailable),
                        };
                        let _ = response_tx.send(result);
                    }
                    AcpConnectionCommand::ListSessions { request, response_tx } => {
                        let result = runtime.list_sessions(request).await.map_err(protocol_error);
                        let _ = response_tx.send(result);
                    }
                    AcpConnectionCommand::Authenticate {
                        method_id,
                        response_tx,
                    } => {
                        let can_authenticate_directly = runtime
                            .auth_methods()
                            .iter()
                            .find(|method| method.id().to_string() == method_id)
                            .is_some_and(|method| matches!(method, AuthMethod::Agent(_)));
                        let result = if can_authenticate_directly {
                            runtime
                                .authenticate(method_id)
                                .await
                                .map(|_| ())
                                .map_err(protocol_error)
                        } else {
                            Err(AcpConnectionError::AuthSetupRequired)
                        };
                        let _ = event_tx.send(AcpManagedEvent::AuthenticationFinished {
                            agent_id: agent_id.clone(),
                            connection_id,
                            result: result.clone(),
                        });
                        let _ = response_tx.send(result);
                    }
                    AcpConnectionCommand::CloseThread {
                        thread_id,
                        delete_remote,
                        response_tx,
                    } => {
                        if running_turns.contains_key(&thread_id) {
                            // ACP cancellation is asynchronous. Closing the
                            // session before the prompt response completes can
                            // be rejected by the agent and leak remote state.
                            if let Some(session) = sessions.get(&thread_id) {
                                let _ =
                                    runtime.cancel_session(session.session.session_id().clone());
                            }
                            pending_closes.insert(
                                thread_id,
                                AcpPendingClose {
                                    delete_remote,
                                    response_tx,
                                },
                            );
                        } else {
                            let result = close_managed_thread(
                                &runtime,
                                &mut sessions,
                                &thread_id,
                                delete_remote,
                            )
                            .await;
                            let _ = response_tx.send(result);
                        }
                    }
                    AcpConnectionCommand::Shutdown => return Ok(()),
                }
            }
            completion = completion_rx.recv() => {
                let Some(completion) = completion else {
                    return Ok(());
                };
                let is_current = running_turns
                    .get(&completion.thread_id)
                    .is_some_and(|turn_id| turn_id == &completion.turn_id);
                if is_current {
                    running_turns.remove(&completion.thread_id);
                }
                if let Some(pending_close) = pending_closes.remove(&completion.thread_id) {
                    let close_result = close_managed_thread(
                        &runtime,
                        &mut sessions,
                        &completion.thread_id,
                        pending_close.delete_remote,
                    )
                    .await;
                    let _ = pending_close.response_tx.send(close_result);
                }
                let event_result = completion.result.clone();
                let _ = completion.response_tx.send(completion.result);
                let _ = event_tx.send(AcpManagedEvent::TurnFinished {
                    agent_id: agent_id.clone(),
                    thread_id: completion.thread_id,
                    turn_id: completion.turn_id,
                    result: event_result,
                });
            }
        }
    }
}

async fn close_managed_thread(
    runtime: &AcpAgentRuntime,
    sessions: &mut HashMap<String, AcpManagedSession>,
    thread_id: &str,
    delete_remote: bool,
) -> Result<(), AcpConnectionError> {
    let Some(session) = sessions.remove(thread_id) else {
        return Ok(());
    };
    let session_id = session.session.session_id().clone();
    if delete_remote {
        runtime
            .delete_session(DeleteSessionRequest::new(session_id))
            .await
            .map(|_| ())
    } else if runtime
        .agent_capabilities()
        .session_capabilities
        .close
        .is_some()
    {
        runtime.close_session(session_id).await.map(|_| ())
    } else {
        Ok(())
    }
    .map_err(protocol_error)
}

async fn apply_config_selections(
    runtime: &AcpAgentRuntime,
    session: &mut AcpActiveSession,
    selections: &[AcpSessionConfigSelection],
) -> Result<(), agent_client_protocol::Error> {
    for selection in selections {
        if !session.supports_config_selection(selection) {
            continue;
        }
        let value = session
            .config_value_for_selection(selection)
            .ok_or_else(|| {
                agent_client_protocol::util::internal_error(
                    "ACP session config selection has an invalid value",
                )
            })?;
        let response = runtime
            .set_session_config_value(
                session.session_id().clone(),
                selection.config_id.clone(),
                value,
            )
            .await?;
        // The response is an authoritative complete snapshot.
        session.replace_config_options(response.config_options);
    }
    Ok(())
}

async fn apply_mode_selection(
    runtime: &AcpAgentRuntime,
    session: &mut AcpActiveSession,
    mode_id: Option<&str>,
) -> Result<(), agent_client_protocol::Error> {
    let Some(mode_id) = mode_id else {
        return Ok(());
    };
    let session_id = session.session_id().clone();
    let Some(modes) = session.modes.as_mut() else {
        return Ok(());
    };
    if modes.current_mode_id.to_string() == mode_id
        || !modes
            .available_modes
            .iter()
            .any(|mode| mode.id.to_string() == mode_id)
    {
        return Ok(());
    }
    runtime
        .set_session_mode(session_id, mode_id.to_string())
        .await?;
    modes.current_mode_id = mode_id.to_string().into();
    Ok(())
}

fn session_outcome(session: &AcpManagedSession) -> AcpPromptSessionOutcome {
    AcpPromptSessionOutcome {
        session_id: session.session.session_id().to_string(),
        session_metadata: session.metadata.clone(),
        session_config_options: acp_session_config_options(session.session.config_options()),
        session_modes: acp_session_mode_state(session.session.modes()),
    }
}

fn finish_prompt_with_error(
    event_tx: &mpsc::UnboundedSender<AcpManagedEvent>,
    agent_id: &str,
    thread_id: String,
    turn_id: String,
    response_tx: oneshot::Sender<Result<AcpPromptSessionOutcome, AcpConnectionError>>,
    error: AcpConnectionError,
) {
    let _ = response_tx.send(Err(error.clone()));
    let _ = event_tx.send(AcpManagedEvent::TurnFinished {
        agent_id: agent_id.to_string(),
        thread_id,
        turn_id,
        result: Err(error),
    });
}

fn protocol_error(error: agent_client_protocol::Error) -> AcpConnectionError {
    if error.code == ErrorCode::AuthRequired {
        AcpConnectionError::AuthRequired
    } else {
        AcpConnectionError::Protocol(super::super::sanitize_for_persistence(&error.to_string()))
    }
}

fn launch_fingerprint(config: &AcpLaunchConfig, policy: &AcpHostCapabilityPolicy) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hash_component(&mut hasher, config.command.as_bytes());
    for argument in &config.args {
        hash_component(&mut hasher, argument.as_bytes());
    }
    for (name, value) in &config.env {
        hash_component(&mut hasher, name.as_bytes());
        hash_component(&mut hasher, value.as_bytes());
    }
    if let Some(cwd) = &config.cwd {
        hash_component(&mut hasher, cwd.as_os_str().as_encoded_bytes());
    }
    // Capability negotiation is fixed at initialize time. Include every
    // permission bit so a stricter policy can never reuse a permissive process.
    hash_component(&mut hasher, &[u8::from(policy.fs_read_text_file)]);
    hash_component(&mut hasher, &[u8::from(policy.fs_write_text_file)]);
    hash_component(&mut hasher, &[u8::from(policy.terminal)]);
    hasher.finalize().into()
}

fn hash_component(hasher: &mut Sha256, component: &[u8]) {
    // Length framing prevents different argument/env boundaries from producing
    // the same digest while retaining no plaintext launch material.
    hasher.update((component.len() as u64).to_le_bytes());
    hasher.update(component);
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::{
        Agent,
        schema::v1::{
            AgentCapabilities, AuthMethodAgent, AuthenticateRequest, AuthenticateResponse,
            CancelNotification, CloseSessionRequest, CloseSessionResponse, InitializeRequest,
            InitializeResponse, NewSessionRequest, NewSessionResponse, PromptResponse,
            ResumeSessionRequest, ResumeSessionResponse, SessionCapabilities,
            SessionCloseCapabilities, SessionConfigOption, SessionMode, SessionModeState,
            SessionResumeCapabilities, SetSessionConfigOptionRequest,
            SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse,
            StopReason,
        },
    };
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    const TEST_CONNECTION_ID: u64 = 1;

    fn launch_config(secret: &str) -> AcpLaunchConfig {
        AcpLaunchConfig {
            id: "agent".to_string(),
            display_name: "Agent".to_string(),
            command: "agent-command".to_string(),
            args: vec!["--token".to_string(), secret.to_string()],
            env: BTreeMap::from([("AGENT_TOKEN".to_string(), secret.to_string())]),
            cwd: Some(PathBuf::from("/workspace")),
        }
    }

    #[test]
    fn launch_fingerprint_changes_when_secret_bearing_configuration_changes() {
        let policy = AcpHostCapabilityPolicy::default();
        let first = launch_fingerprint(&launch_config("first"), &policy);
        let second = launch_fingerprint(&launch_config("second"), &policy);
        assert_ne!(first, second);
    }

    #[test]
    fn launch_fingerprint_separates_negotiated_host_capabilities() {
        let config = launch_config("same-secret");
        let restricted = AcpHostCapabilityPolicy::default();
        let terminal_enabled = AcpHostCapabilityPolicy {
            terminal: true,
            ..AcpHostCapabilityPolicy::default()
        };

        assert_ne!(
            launch_fingerprint(&config, &restricted),
            launch_fingerprint(&config, &terminal_enabled)
        );
    }

    #[test]
    fn managed_errors_do_not_debug_print_agent_text() {
        let error = AcpConnectionError::Protocol("private-agent-output".to_string());
        assert!(!format!("{error:?}").contains("private-agent-output"));
    }

    async fn submit_prompt(
        command_tx: &mpsc::UnboundedSender<AcpConnectionCommand>,
        thread_id: &str,
        turn_id: &str,
        existing_session_id: Option<&str>,
    ) -> Result<AcpPromptSessionOutcome, AcpConnectionError> {
        let (response_tx, response_rx) = oneshot::channel();
        command_tx
            .send(AcpConnectionCommand::Prompt {
                request: AcpManagedPromptRequest {
                    thread_id: thread_id.to_string(),
                    turn_id: turn_id.to_string(),
                    existing_session_id: existing_session_id.map(str::to_string),
                    cwd: PathBuf::from("/workspace"),
                    config_selections: Vec::new(),
                    mode_id: None,
                    mcp_servers: Vec::new(),
                    prompt: Zeroizing::new("hello".to_string()),
                },
                response_tx,
            })
            .expect("connection command receiver");
        response_rx.await.expect("prompt response")
    }

    #[tokio::test]
    async fn connection_owner_reuses_one_session_per_thread() {
        let session_count = Arc::new(AtomicUsize::new(0));
        let session_count_for_new = Arc::clone(&session_count);
        let fake_agent = Agent
            .builder()
            .on_receive_request(
                async move |request: InitializeRequest, responder, _connection| {
                    responder.respond(
                        InitializeResponse::new(request.protocol_version)
                            .agent_capabilities(AgentCapabilities::new()),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: NewSessionRequest, responder, _connection| {
                    let number = session_count_for_new.fetch_add(1, Ordering::SeqCst) + 1;
                    responder.respond(NewSessionResponse::new(format!("session-{number}")))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: PromptRequest, responder, _connection| {
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            );
        let (client_event_tx, _client_event_rx) = mpsc::unbounded_channel();

        with_acp_agent_runtime_events(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            client_event_tx,
            async move |runtime| {
                let (command_tx, command_rx) = mpsc::unbounded_channel();
                let (event_tx, _event_rx) = mpsc::unbounded_channel();
                let runner = tokio::spawn(run_connection_commands(
                    "agent".to_string(),
                    TEST_CONNECTION_ID,
                    runtime,
                    command_rx,
                    event_tx,
                ));

                let first = submit_prompt(&command_tx, "thread-a", "turn-1", None)
                    .await
                    .expect("first prompt");
                let second = submit_prompt(&command_tx, "thread-a", "turn-2", None)
                    .await
                    .expect("second prompt");
                let third = submit_prompt(&command_tx, "thread-b", "turn-3", None)
                    .await
                    .expect("third prompt");

                assert_eq!(first.session_id, second.session_id);
                assert_ne!(first.session_id, third.session_id);
                assert_eq!(session_count.load(Ordering::SeqCst), 2);
                command_tx
                    .send(AcpConnectionCommand::Shutdown)
                    .expect("shutdown command");
                runner
                    .await
                    .expect("connection command task")
                    .expect("connection command result");
                Ok(())
            },
        )
        .await
        .expect("persistent connection");
    }

    #[tokio::test]
    async fn connection_owner_closes_a_running_thread_after_cancellation_finishes() {
        let prompt_started = Arc::new(tokio::sync::Notify::new());
        let allow_prompt_finish = Arc::new(tokio::sync::Notify::new());
        let prompt_finished = Arc::new(AtomicBool::new(false));
        let cancel_seen = Arc::new(AtomicBool::new(false));
        let prompt_started_for_agent = Arc::clone(&prompt_started);
        let allow_prompt_finish_for_agent = Arc::clone(&allow_prompt_finish);
        let prompt_finished_for_agent = Arc::clone(&prompt_finished);
        let cancel_seen_for_agent = Arc::clone(&cancel_seen);
        let prompt_finished_for_close = Arc::clone(&prompt_finished);
        let cancel_seen_for_close = Arc::clone(&cancel_seen);
        let fake_agent = Agent
            .builder()
            .on_receive_request(
                async move |request: InitializeRequest, responder, _connection| {
                    let capabilities = AgentCapabilities::new().session_capabilities(
                        SessionCapabilities::new().close(SessionCloseCapabilities::new()),
                    );
                    responder.respond(
                        InitializeResponse::new(request.protocol_version)
                            .agent_capabilities(capabilities),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: NewSessionRequest, responder, _connection| {
                    responder.respond(NewSessionResponse::new("session-1"))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: PromptRequest, responder, _connection| {
                    prompt_started_for_agent.notify_one();
                    allow_prompt_finish_for_agent.notified().await;
                    prompt_finished_for_agent.store(true, Ordering::SeqCst);
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_notification(
                async move |_notification: CancelNotification, _connection| {
                    cancel_seen_for_agent.store(true, Ordering::SeqCst);
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |_request: CloseSessionRequest, responder, _connection| {
                    assert!(cancel_seen_for_close.load(Ordering::SeqCst));
                    assert!(prompt_finished_for_close.load(Ordering::SeqCst));
                    responder.respond(CloseSessionResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            );
        let (client_event_tx, _client_event_rx) = mpsc::unbounded_channel();

        with_acp_agent_runtime_events(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            client_event_tx,
            async move |runtime| {
                let (command_tx, command_rx) = mpsc::unbounded_channel();
                let (event_tx, _event_rx) = mpsc::unbounded_channel();
                let runner = tokio::spawn(run_connection_commands(
                    "agent".to_string(),
                    TEST_CONNECTION_ID,
                    runtime,
                    command_rx,
                    event_tx,
                ));
                let (prompt_response_tx, prompt_response_rx) = oneshot::channel();
                command_tx
                    .send(AcpConnectionCommand::Prompt {
                        request: AcpManagedPromptRequest {
                            thread_id: "thread-a".to_string(),
                            turn_id: "turn-1".to_string(),
                            existing_session_id: None,
                            cwd: PathBuf::from("/workspace"),
                            config_selections: Vec::new(),
                            mode_id: None,
                            mcp_servers: Vec::new(),
                            prompt: Zeroizing::new("hello".to_string()),
                        },
                        response_tx: prompt_response_tx,
                    })
                    .expect("prompt command");
                prompt_started.notified().await;

                let (close_response_tx, close_response_rx) = oneshot::channel();
                command_tx
                    .send(AcpConnectionCommand::CloseThread {
                        thread_id: "thread-a".to_string(),
                        delete_remote: false,
                        response_tx: close_response_tx,
                    })
                    .expect("close command");

                // Give the connection owner a scheduling turn to record the
                // pending close before the fake prompt is allowed to finish.
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                allow_prompt_finish.notify_one();
                tokio::time::timeout(std::time::Duration::from_secs(1), close_response_rx)
                    .await
                    .expect("close response timeout")
                    .expect("close response")
                    .expect("close after cancellation");
                prompt_response_rx
                    .await
                    .expect("prompt response")
                    .expect("cancelled prompt completion");
                command_tx
                    .send(AcpConnectionCommand::Shutdown)
                    .expect("shutdown command");
                runner
                    .await
                    .expect("connection command task")
                    .expect("connection command result");
                Ok(())
            },
        )
        .await
        .expect("close-after-cancel connection");
    }

    #[tokio::test]
    async fn connection_owner_restores_exact_saved_session() {
        let fake_agent = Agent
            .builder()
            .on_receive_request(
                async move |request: InitializeRequest, responder, _connection| {
                    let capabilities = AgentCapabilities::new().session_capabilities(
                        SessionCapabilities::new().resume(SessionResumeCapabilities::new()),
                    );
                    responder.respond(
                        InitializeResponse::new(request.protocol_version)
                            .agent_capabilities(capabilities),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: ResumeSessionRequest, responder, _connection| {
                    assert_eq!(request.session_id.to_string(), "saved-session");
                    responder.respond(ResumeSessionResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: PromptRequest, responder, _connection| {
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            );
        let (client_event_tx, _client_event_rx) = mpsc::unbounded_channel();

        with_acp_agent_runtime_events(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            client_event_tx,
            async move |runtime| {
                let (command_tx, command_rx) = mpsc::unbounded_channel();
                let (event_tx, _event_rx) = mpsc::unbounded_channel();
                let runner = tokio::spawn(run_connection_commands(
                    "agent".to_string(),
                    TEST_CONNECTION_ID,
                    runtime,
                    command_rx,
                    event_tx,
                ));
                let outcome =
                    submit_prompt(&command_tx, "thread-a", "turn-1", Some("saved-session"))
                        .await
                        .expect("restored prompt");
                assert_eq!(outcome.session_id, "saved-session");
                command_tx
                    .send(AcpConnectionCommand::Shutdown)
                    .expect("shutdown command");
                runner
                    .await
                    .expect("connection command task")
                    .expect("connection command result");
                Ok(())
            },
        )
        .await
        .expect("restored persistent connection");
    }

    #[tokio::test]
    async fn connection_owner_sends_boolean_config_as_boolean_value() {
        let fake_agent = Agent
            .builder()
            .on_receive_request(
                async move |request: InitializeRequest, responder, _connection| {
                    responder.respond(
                        InitializeResponse::new(request.protocol_version)
                            .agent_capabilities(AgentCapabilities::new()),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: NewSessionRequest, responder, _connection| {
                    responder.respond(NewSessionResponse::new("session-1").config_options(vec![
                        SessionConfigOption::boolean("auto-approve", "Auto approve", false),
                    ]))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: SetSessionConfigOptionRequest, responder, _connection| {
                    assert_eq!(request.config_id.to_string(), "auto-approve");
                    assert_eq!(request.value.as_bool(), Some(true));
                    responder.respond(SetSessionConfigOptionResponse::new(vec![
                        SessionConfigOption::boolean("auto-approve", "Auto approve", true),
                    ]))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: PromptRequest, responder, _connection| {
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            );
        let (client_event_tx, _client_event_rx) = mpsc::unbounded_channel();

        with_acp_agent_runtime_events(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            client_event_tx,
            async move |runtime| {
                let (command_tx, command_rx) = mpsc::unbounded_channel();
                let (event_tx, _event_rx) = mpsc::unbounded_channel();
                let runner = tokio::spawn(run_connection_commands(
                    "agent".to_string(),
                    TEST_CONNECTION_ID,
                    runtime,
                    command_rx,
                    event_tx,
                ));
                submit_prompt(&command_tx, "thread-a", "turn-1", None)
                    .await
                    .expect("session prompt");
                let (response_tx, response_rx) = oneshot::channel();
                command_tx
                    .send(AcpConnectionCommand::SetConfig {
                        thread_id: "thread-a".to_string(),
                        selection: AcpSessionConfigSelection {
                            config_id: "auto-approve".to_string(),
                            value_id: "true".to_string(),
                        },
                        response_tx,
                    })
                    .expect("config command");
                let options = response_rx
                    .await
                    .expect("config response")
                    .expect("boolean config update");
                assert_eq!(options[0].current_value_id, "true");
                command_tx
                    .send(AcpConnectionCommand::Shutdown)
                    .expect("shutdown command");
                runner
                    .await
                    .expect("connection command task")
                    .expect("connection command result");
                Ok(())
            },
        )
        .await
        .expect("boolean config connection");
    }

    #[tokio::test]
    async fn connection_owner_applies_saved_mode_before_prompt() {
        let mode_applied = Arc::new(AtomicBool::new(false));
        let mode_applied_for_set = Arc::clone(&mode_applied);
        let mode_applied_for_prompt = Arc::clone(&mode_applied);
        let fake_agent = Agent
            .builder()
            .on_receive_request(
                async move |request: InitializeRequest, responder, _connection| {
                    responder.respond(
                        InitializeResponse::new(request.protocol_version)
                            .agent_capabilities(AgentCapabilities::new()),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: NewSessionRequest, responder, _connection| {
                    responder.respond(NewSessionResponse::new("session-1").modes(
                        SessionModeState::new(
                            "plan",
                            vec![
                                SessionMode::new("plan", "Plan"),
                                SessionMode::new("agent", "Agent"),
                            ],
                        ),
                    ))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: SetSessionModeRequest, responder, _connection| {
                    assert_eq!(request.mode_id.to_string(), "agent");
                    mode_applied_for_set.store(true, Ordering::SeqCst);
                    responder.respond(SetSessionModeResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: PromptRequest, responder, _connection| {
                    assert!(mode_applied_for_prompt.load(Ordering::SeqCst));
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            );
        let (client_event_tx, _client_event_rx) = mpsc::unbounded_channel();

        with_acp_agent_runtime_events(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            client_event_tx,
            async move |runtime| {
                let (command_tx, command_rx) = mpsc::unbounded_channel();
                let (event_tx, _event_rx) = mpsc::unbounded_channel();
                let runner = tokio::spawn(run_connection_commands(
                    "agent".to_string(),
                    TEST_CONNECTION_ID,
                    runtime,
                    command_rx,
                    event_tx,
                ));
                let (response_tx, response_rx) = oneshot::channel();
                command_tx
                    .send(AcpConnectionCommand::Prompt {
                        request: AcpManagedPromptRequest {
                            thread_id: "thread-a".to_string(),
                            turn_id: "turn-1".to_string(),
                            existing_session_id: None,
                            cwd: PathBuf::from("/workspace"),
                            config_selections: Vec::new(),
                            mode_id: Some("agent".to_string()),
                            mcp_servers: Vec::new(),
                            prompt: Zeroizing::new("hello".to_string()),
                        },
                        response_tx,
                    })
                    .expect("prompt command");
                response_rx
                    .await
                    .expect("prompt response")
                    .expect("prompt after mode selection");
                command_tx
                    .send(AcpConnectionCommand::Shutdown)
                    .expect("shutdown command");
                runner
                    .await
                    .expect("connection command task")
                    .expect("connection command result");
                Ok(())
            },
        )
        .await
        .expect("mode-selection connection");
    }

    #[tokio::test]
    async fn direct_agent_authentication_unblocks_a_thread() {
        let authenticated = Arc::new(AtomicBool::new(false));
        let authenticated_for_new = Arc::clone(&authenticated);
        let authenticated_for_auth = Arc::clone(&authenticated);
        let fake_agent = Agent
            .builder()
            .on_receive_request(
                async move |request: InitializeRequest, responder, _connection| {
                    responder.respond(
                        InitializeResponse::new(request.protocol_version)
                            .agent_capabilities(AgentCapabilities::new())
                            .auth_methods(vec![AuthMethod::Agent(AuthMethodAgent::new(
                                "login", "Login",
                            ))]),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: NewSessionRequest, responder, _connection| {
                    if authenticated_for_new.load(Ordering::SeqCst) {
                        responder.respond(NewSessionResponse::new("session-1"))
                    } else {
                        responder
                            .respond_with_result(Err(agent_client_protocol::Error::auth_required()))
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |request: AuthenticateRequest, responder, _connection| {
                    assert_eq!(request.method_id.to_string(), "login");
                    authenticated_for_auth.store(true, Ordering::SeqCst);
                    responder.respond(AuthenticateResponse::new())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_request: PromptRequest, responder, _connection| {
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            );
        let (client_event_tx, _client_event_rx) = mpsc::unbounded_channel();

        with_acp_agent_runtime_events(
            fake_agent,
            "2.0.0-test".to_string(),
            AcpHostCapabilityPolicy::default(),
            client_event_tx,
            async move |runtime| {
                let (command_tx, command_rx) = mpsc::unbounded_channel();
                let (event_tx, _event_rx) = mpsc::unbounded_channel();
                let runner = tokio::spawn(run_connection_commands(
                    "agent".to_string(),
                    TEST_CONNECTION_ID,
                    runtime,
                    command_rx,
                    event_tx,
                ));
                assert_eq!(
                    submit_prompt(&command_tx, "thread-a", "turn-1", None).await,
                    Err(AcpConnectionError::AuthRequired)
                );

                let (response_tx, response_rx) = oneshot::channel();
                command_tx
                    .send(AcpConnectionCommand::Authenticate {
                        method_id: "login".to_string(),
                        response_tx,
                    })
                    .expect("authenticate command");
                response_rx
                    .await
                    .expect("authenticate response")
                    .expect("agent authentication");

                let outcome = submit_prompt(&command_tx, "thread-a", "turn-2", None)
                    .await
                    .expect("authenticated prompt");
                assert_eq!(outcome.session_id, "session-1");
                command_tx
                    .send(AcpConnectionCommand::Shutdown)
                    .expect("shutdown command");
                runner
                    .await
                    .expect("connection command task")
                    .expect("connection command result");
                Ok(())
            },
        )
        .await
        .expect("authenticated persistent connection");
    }
}
